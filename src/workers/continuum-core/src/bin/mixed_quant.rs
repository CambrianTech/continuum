//! Mixed quantization: re-quantize existing GGUF with per-tensor quant levels.
//! Dequantizes each tensor to F32, then re-quantizes at the assigned level.
//!
//! Usage: mixed_quant --input <gguf> --output <gguf>
//!
//! Critical tensors (embeddings, output head) → Q6_K
//! Attention in high-util layers → Q5_K
//! Everything else → Q3_K (same as source)

use std::time::Instant;

use candle_core::quantized::{gguf_file, GgmlDType, QTensor};
use candle_core::Device;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input_path = args
        .iter()
        .skip_while(|a| *a != "--input")
        .nth(1)
        .expect("--input <path>");
    let output_path = args
        .iter()
        .skip_while(|a| *a != "--output")
        .nth(1)
        .expect("--output <path>");

    eprintln!("=== Mixed Quantization ===");
    eprintln!("  Input:  {}", input_path);
    eprintln!("  Output: {}", output_path);

    let start = Instant::now();
    let device = Device::Cpu;

    // Read source GGUF
    let mut file = std::fs::File::open(input_path).expect("open input");
    let content = gguf_file::Content::read(&mut file).expect("read gguf");

    eprintln!(
        "  {} tensors, {} metadata keys",
        content.tensor_infos.len(),
        content.metadata.len()
    );

    // Collect all metadata
    let metadata: Vec<(String, gguf_file::Value)> = content
        .metadata
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Re-quantize each tensor
    let mut reader = std::io::BufReader::new(std::fs::File::open(input_path).expect("reopen"));

    let mut qtensors: Vec<(String, QTensor)> = Vec::new();
    let mut tensor_names: Vec<String> = content.tensor_infos.keys().cloned().collect();
    tensor_names.sort();

    for (i, name) in tensor_names.iter().enumerate() {
        let qt = content
            .tensor(&mut reader, name, &device)
            .expect("read tensor");
        let orig_dtype = qt.dtype();
        let shape = qt.shape().dims().to_vec();
        let target_dtype = assign_quant_level(name, orig_dtype);

        if target_dtype == orig_dtype {
            // Keep as-is
            qtensors.push((name.clone(), qt));
        } else {
            // Dequantize to F32, re-quantize at new level
            let f32_tensor = qt.dequantize(&device).expect("dequantize");

            // Check block alignment — fall back if dimensions don't fit
            let elem_count: usize = shape.iter().product();
            let block_size = target_dtype.block_size();
            let actual_dtype = if block_size > 0 && elem_count % block_size != 0 {
                // Try Q8_0 (block size 32) as fallback
                if elem_count % 32 == 0 {
                    GgmlDType::Q8_0
                } else {
                    // Keep original
                    orig_dtype
                }
            } else {
                target_dtype
            };

            match QTensor::quantize(&f32_tensor, actual_dtype) {
                Ok(requeued) => {
                    if actual_dtype != orig_dtype {
                        eprintln!(
                            "  {:>4}/{} {:50} {:?} → {:?}",
                            i + 1,
                            tensor_names.len(),
                            name,
                            orig_dtype,
                            actual_dtype
                        );
                    }
                    qtensors.push((name.clone(), requeued));
                }
                Err(_) => {
                    // Quantization failed — keep original
                    let orig_qt = content.tensor(&mut reader, name, &device).expect("re-read");
                    qtensors.push((name.clone(), orig_qt));
                }
            }
        }

        if (i + 1) % 100 == 0 {
            eprintln!("  processed {}/{}", i + 1, tensor_names.len());
        }
    }

    eprintln!("  Writing mixed-quant GGUF...");
    let metadata_refs: Vec<(&str, &gguf_file::Value)> =
        metadata.iter().map(|(k, v)| (k.as_str(), v)).collect();
    let tensor_refs: Vec<(&str, &QTensor)> =
        qtensors.iter().map(|(n, qt)| (n.as_str(), qt)).collect();

    let mut outfile =
        std::io::BufWriter::new(std::fs::File::create(output_path).expect("create output"));
    gguf_file::write(&mut outfile, &metadata_refs, &tensor_refs).expect("write gguf");

    let out_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
    let in_size = std::fs::metadata(input_path).map(|m| m.len()).unwrap_or(0);

    eprintln!("\n=== Done in {:.1?} ===", start.elapsed());
    eprintln!("  Input:  {:.1} GB", in_size as f64 / 1073741824.0);
    eprintln!("  Output: {:.1} GB", out_size as f64 / 1073741824.0);
}

/// Assign quantization level per tensor based on name and importance.
fn assign_quant_level(name: &str, orig: GgmlDType) -> GgmlDType {
    // Norms and biases: always F32 (already are, keep them)
    if name.contains("norm") || name.contains("bias") {
        return orig;
    }

    // CRITICAL: Output head and embeddings → Q6_K
    // This is where EOS token logits come from. At Q3_K the EOS signal
    // is too noisy and the model can't stop generating.
    if name == "output.weight" || name == "token_embd.weight" {
        return GgmlDType::Q6K;
    }

    // First and last 4 layers: higher precision (model boundaries are sensitive)
    if let Some(layer) = extract_layer_idx(name) {
        if layer < 4 || layer >= 60 {
            // Boundary layers → Q5_K
            if name.contains("ffn_") {
                return GgmlDType::Q5K;
            }
            if name.contains("attn_") {
                return GgmlDType::Q5K;
            }
        }

        // Middle layers: keep at Q3_K (same as source, saves space)
        // The bulk of the model stays cheap
    }

    // Default: keep original quantization
    orig
}

fn extract_layer_idx(name: &str) -> Option<usize> {
    // "blk.5.attn_q.weight" → 5
    if let Some(rest) = name.strip_prefix("blk.") {
        if let Some(dot_pos) = rest.find('.') {
            return rest[..dot_pos].parse().ok();
        }
    }
    None
}
