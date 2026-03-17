//! GGUF writer: produces mixed-quantization GGUF files from a CompressionRecipe.
//!
//! Uses candle's built-in `gguf_file::write()` for the file format.
//! Our job: read safetensors, prune heads per topology, quantize each tensor
//! at the recipe's assigned level, assemble metadata, and hand off to candle.
//!
//! See docs/genome/COMPRESSION-PIPELINE.md

use std::collections::HashMap;
use std::path::Path;

use candle_core::quantized::gguf_file::Value;
use candle_core::quantized::{GgmlDType, QTensor};
use candle_core::{DType, Device, Tensor};
use safetensors::SafeTensors;

use super::types::*;

/// Map our GgufQuantType enum to candle's GgmlDType.
fn to_ggml_dtype(qt: GgufQuantType) -> GgmlDType {
    match qt {
        GgufQuantType::Q2K => GgmlDType::Q2K,
        GgufQuantType::Q3KS => GgmlDType::Q3K,
        GgufQuantType::Q3KM => GgmlDType::Q3K,
        GgufQuantType::Q3KL => GgmlDType::Q3K,
        GgufQuantType::Iq4Xs => GgmlDType::Q4K, // closest available
        GgufQuantType::Q4KS => GgmlDType::Q4K,
        GgufQuantType::Q4KM => GgmlDType::Q4K,
        GgufQuantType::Q5KS => GgmlDType::Q5K,
        GgufQuantType::Q5KM => GgmlDType::Q5K,
        GgufQuantType::Q6K => GgmlDType::Q6K,
        GgufQuantType::Q8_0 => GgmlDType::Q8_0,
        GgufQuantType::F16 => GgmlDType::F16,
        GgufQuantType::F32 => GgmlDType::F32,
    }
}

/// GGUF tensor name mapping: safetensors name → GGUF name.
/// Qwen2/Llama safetensors use "model.layers.N.self_attn.q_proj.weight"
/// GGUF uses "blk.N.attn_q.weight"
fn safetensor_to_gguf_name(st_name: &str) -> Option<String> {
    // Embedding
    if st_name == "model.embed_tokens.weight" {
        return Some("token_embd.weight".into());
    }
    if st_name == "model.norm.weight" {
        return Some("output_norm.weight".into());
    }
    if st_name == "lm_head.weight" {
        return Some("output.weight".into());
    }

    // Layer tensors: model.layers.N.xxx → blk.N.xxx
    if let Some(rest) = st_name.strip_prefix("model.layers.") {
        let mut parts = rest.splitn(2, '.');
        let layer_str = parts.next()?;
        let suffix = parts.next()?;

        let gguf_suffix = match suffix {
            "self_attn.q_proj.weight" => "attn_q.weight",
            "self_attn.k_proj.weight" => "attn_k.weight",
            "self_attn.v_proj.weight" => "attn_v.weight",
            "self_attn.o_proj.weight" => "attn_output.weight",
            "self_attn.q_proj.bias" => "attn_q.bias",
            "self_attn.k_proj.bias" => "attn_k.bias",
            "self_attn.v_proj.bias" => "attn_v.bias",
            "mlp.gate_proj.weight" => "ffn_gate.weight",
            "mlp.up_proj.weight" => "ffn_up.weight",
            "mlp.down_proj.weight" => "ffn_down.weight",
            "input_layernorm.weight" => "attn_norm.weight",
            "post_attention_layernorm.weight" => "ffn_norm.weight",
            _ => return None,
        };

        return Some(format!("blk.{layer_str}.{gguf_suffix}"));
    }

    None
}

/// Look up the quant type for a GGUF tensor name from the recipe.
fn lookup_quant_type(gguf_name: &str, recipe: &CompressionRecipe) -> GgufQuantType {
    for assignment in &recipe.tensor_quant_map {
        if assignment.pattern == gguf_name {
            return assignment.quant_type;
        }
    }
    // Default: Q4_K_S for weights, F32 for norms/biases
    if gguf_name.contains("norm") || gguf_name.contains("bias") {
        GgufQuantType::F32
    } else {
        GgufQuantType::Q4KS
    }
}

/// Quantize an F32 tensor to the specified GGUF quant type.
/// Returns a QTensor ready for GGUF writing.
fn quantize_tensor(
    tensor: &Tensor,
    ggml_dtype: GgmlDType,
    shape: Vec<usize>,
) -> Result<QTensor, String> {
    // Get F32 data
    let f32_data: Vec<f32> = tensor
        .to_dtype(DType::F32)
        .map_err(|e| format!("to_f32: {e}"))?
        .flatten_all()
        .map_err(|e| format!("flatten: {e}"))?
        .to_vec1()
        .map_err(|e| format!("to_vec: {e}"))?;

    // For F32/F16, create QTensor directly
    // Create a CPU tensor, then use candle's QTensor::quantize to convert.
    // This handles all GGUF quant types through candle's internal quantization.
    let cpu_tensor = Tensor::from_vec(f32_data, shape.as_slice(), &Device::Cpu)
        .map_err(|e| format!("tensor from vec: {e}"))?;

    QTensor::quantize(&cpu_tensor, ggml_dtype)
        .map_err(|e| format!("quantize to {:?}: {e}", ggml_dtype))
}

/// Build GGUF metadata for a compressed model.
fn build_metadata(recipe: &CompressionRecipe, arch: &str) -> Vec<(String, Value)> {
    let first_layer = recipe.topology.layers.first();
    let q_heads = first_layer.map(|l| l.num_heads).unwrap_or(0);
    let kv_heads = first_layer.map(|l| l.num_kv_heads).unwrap_or(0);

    let mut meta = vec![
        ("general.architecture".into(), Value::String(arch.into())),
        ("general.name".into(), Value::String(format!(
            "{} (compacted by Continuum)", recipe.base_model
        ))),
        (format!("{arch}.block_count"), Value::U32(recipe.topology.layers.len() as u32)),
        (format!("{arch}.context_length"), Value::U32(32768)),
        (format!("{arch}.embedding_length"), Value::U32(5120)), // TODO: from arch config
        (format!("{arch}.attention.head_count"), Value::U32(q_heads as u32)),
        (format!("{arch}.attention.head_count_kv"), Value::U32(kv_heads as u32)),
        (format!("{arch}.attention.key_length"), Value::U32(recipe.topology.head_dim as u32)),
        (format!("{arch}.attention.value_length"), Value::U32(recipe.topology.head_dim as u32)),
        (format!("{arch}.attention.layer_norm_rms_epsilon"), Value::F32(1e-6)),
        (format!("{arch}.rope.freq_base"), Value::F32(1_000_000.0)),
    ];

    // Custom Continuum metadata
    if let Ok(recipe_json) = serde_json::to_string(recipe) {
        meta.push(("continuum.compression_recipe".into(), Value::String(recipe_json)));
    }

    // Per-layer head counts for variable-dimension models
    let q_head_counts: Vec<u32> = recipe.topology.layers.iter()
        .map(|l| l.num_heads as u32)
        .collect();
    let kv_head_counts: Vec<u32> = recipe.topology.layers.iter()
        .map(|l| l.num_kv_heads as u32)
        .collect();

    // Store as comma-separated string (GGUF arrays are complex)
    let q_str: String = q_head_counts.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(",");
    let kv_str: String = kv_head_counts.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(",");
    meta.push(("continuum.per_layer_q_heads".into(), Value::String(q_str)));
    meta.push(("continuum.per_layer_kv_heads".into(), Value::String(kv_str)));

    meta
}

/// Write a compressed GGUF from safetensors + CompressionRecipe.
///
/// This is the main entry point for Stage 3 of the compression pipeline.
/// Reads each tensor from the base model, applies pruning + mixed quantization
/// per the recipe, and writes a single GGUF file.
pub fn write_compressed_gguf(
    safetensors_dir: &Path,
    recipe: &CompressionRecipe,
    output_path: &Path,
    arch: &str,
) -> Result<(), String> {
    let log = crate::runtime::logger("plasticity");
    log.info(&format!(
        "Writing compressed GGUF: {} → {:?}",
        recipe.base_model,
        output_path
    ));

    // Find all safetensor files
    let mut shard_paths: Vec<_> = std::fs::read_dir(safetensors_dir)
        .map_err(|e| format!("read dir: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".safetensors"))
                .unwrap_or(false)
        })
        .collect();
    shard_paths.sort();

    if shard_paths.is_empty() {
        return Err("No safetensors files found".into());
    }

    log.info(&format!("  {} safetensor shards", shard_paths.len()));

    // Process all tensors: prune + quantize
    let mut qtensors: Vec<(String, QTensor)> = Vec::new();
    let mut processed = 0usize;

    for shard_path in &shard_paths {
        let data = std::fs::read(shard_path)
            .map_err(|e| format!("read shard {:?}: {e}", shard_path))?;
        let tensors = SafeTensors::deserialize(&data)
            .map_err(|e| format!("deserialize {:?}: {e}", shard_path))?;

        for (st_name, tensor_view) in tensors.tensors() {
            let gguf_name = match safetensor_to_gguf_name(&st_name) {
                Some(name) => name,
                None => {
                    log.debug(&format!("  skip: {}", st_name));
                    continue;
                }
            };

            // Load tensor to CPU F32
            let shape: Vec<usize> = tensor_view.shape().to_vec();
            let tensor = load_safetensor_view(&tensor_view, &shape)
                .map_err(|e| format!("load {}: {e}", st_name))?;

            // TODO: Apply head pruning here based on recipe.topology
            // For now, assume safetensors are already compacted

            // Look up quant type from recipe
            let quant_type = lookup_quant_type(&gguf_name, recipe);
            let ggml_dtype = to_ggml_dtype(quant_type);

            // Check block alignment
            let block_size = ggml_dtype.block_size();
            let elem_count: usize = shape.iter().product();
            if block_size > 0 && elem_count % block_size != 0 {
                log.warn(&format!(
                    "  {} elements not divisible by block size {} for {:?}, falling back to Q8_0",
                    elem_count, block_size, ggml_dtype
                ));
                let qt = quantize_tensor(&tensor, GgmlDType::Q8_0, shape)
                    .map_err(|e| format!("quantize {}: {e}", gguf_name))?;
                qtensors.push((gguf_name, qt));
            } else {
                let qt = quantize_tensor(&tensor, ggml_dtype, shape)
                    .map_err(|e| format!("quantize {}: {e}", gguf_name))?;
                qtensors.push((gguf_name, qt));
            }

            processed += 1;
            if processed % 50 == 0 {
                log.info(&format!("  processed {} tensors", processed));
            }
        }
    }

    log.info(&format!("  {} tensors total, writing GGUF...", qtensors.len()));

    // Build metadata
    let metadata = build_metadata(recipe, arch);
    let metadata_refs: Vec<(&str, &Value)> = metadata.iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect();

    // Build tensor refs
    let tensor_refs: Vec<(&str, &QTensor)> = qtensors.iter()
        .map(|(name, qt)| (name.as_str(), qt))
        .collect();

    // Write GGUF using candle's built-in writer
    let mut file = std::fs::File::create(output_path)
        .map_err(|e| format!("create {:?}: {e}", output_path))?;
    let mut writer = std::io::BufWriter::new(&mut file);

    candle_core::quantized::gguf_file::write(&mut writer, &metadata_refs, &tensor_refs)
        .map_err(|e| format!("write GGUF: {e}"))?;

    let size = std::fs::metadata(output_path)
        .map(|m| m.len())
        .unwrap_or(0);
    log.info(&format!(
        "  GGUF written: {:?} ({:.1} GB, {} tensors)",
        output_path,
        size as f64 / 1073741824.0,
        qtensors.len()
    ));

    Ok(())
}

/// Load a safetensors tensor view into a CPU F32 Tensor.
fn load_safetensor_view(
    view: &safetensors::tensor::TensorView<'_>,
    shape: &[usize],
) -> Result<Tensor, String> {
    let dtype = view.dtype();
    let data = view.data();

    match dtype {
        safetensors::Dtype::F32 => {
            let f32_data: Vec<f32> = data
                .chunks(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            Tensor::from_vec(f32_data, shape, &Device::Cpu)
                .map_err(|e| format!("tensor from f32: {e}"))
        }
        safetensors::Dtype::BF16 => {
            let bf16_data: Vec<half::bf16> = data
                .chunks(2)
                .map(|b| half::bf16::from_le_bytes([b[0], b[1]]))
                .collect();
            let f32_data: Vec<f32> = bf16_data.iter().map(|v| v.to_f32()).collect();
            Tensor::from_vec(f32_data, shape, &Device::Cpu)
                .map_err(|e| format!("tensor from bf16: {e}"))
        }
        safetensors::Dtype::F16 => {
            let f16_data: Vec<half::f16> = data
                .chunks(2)
                .map(|b| half::f16::from_le_bytes([b[0], b[1]]))
                .collect();
            let f32_data: Vec<f32> = f16_data.iter().map(|v| v.to_f32()).collect();
            Tensor::from_vec(f32_data, shape, &Device::Cpu)
                .map_err(|e| format!("tensor from f16: {e}"))
        }
        _ => Err(format!("unsupported dtype: {:?}", dtype)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safetensor_to_gguf_name() {
        assert_eq!(
            safetensor_to_gguf_name("model.embed_tokens.weight"),
            Some("token_embd.weight".into())
        );
        assert_eq!(
            safetensor_to_gguf_name("model.layers.5.self_attn.q_proj.weight"),
            Some("blk.5.attn_q.weight".into())
        );
        assert_eq!(
            safetensor_to_gguf_name("model.layers.63.mlp.gate_proj.weight"),
            Some("blk.63.ffn_gate.weight".into())
        );
        assert_eq!(
            safetensor_to_gguf_name("model.layers.0.input_layernorm.weight"),
            Some("blk.0.attn_norm.weight".into())
        );
        assert_eq!(
            safetensor_to_gguf_name("model.norm.weight"),
            Some("output_norm.weight".into())
        );
        assert_eq!(
            safetensor_to_gguf_name("lm_head.weight"),
            Some("output.weight".into())
        );
        // Unknown tensor
        assert_eq!(
            safetensor_to_gguf_name("some.random.tensor"),
            None
        );
    }

    #[test]
    fn test_to_ggml_dtype_mapping() {
        assert_eq!(to_ggml_dtype(GgufQuantType::Q3KS), GgmlDType::Q3K);
        assert_eq!(to_ggml_dtype(GgufQuantType::Q6K), GgmlDType::Q6K);
        assert_eq!(to_ggml_dtype(GgufQuantType::F32), GgmlDType::F32);
        assert_eq!(to_ggml_dtype(GgufQuantType::Q8_0), GgmlDType::Q8_0);
    }
}
