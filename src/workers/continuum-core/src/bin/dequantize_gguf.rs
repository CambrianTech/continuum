//! GGUF → F16 Safetensors Dequantizer
//!
//! Converts a quantized GGUF model (Q5_K_S, Q4_K_M, etc.) to F16 safetensors
//! format for higher numerical precision. GGUF already supports batch prefill
//! via Metal SDPA — this is an optional upgrade for quality, not speed.
//!
//! Usage:
//!   dequantize_gguf --input <path.gguf> --output <dir/>
//!
//! Output:
//!   bf16/model.safetensors   — dequantized F16 weights (~19.6GB for 14B)
//!   bf16/config.json         — copied from GGUF dir
//!   bf16/tokenizer.json      — copied from GGUF dir
//!   bf16/tokenizer_config.json — copied if present
//!   bf16/head_topology.json  — copied if present (for compacted models)
//!
//! Auto-detected by load_model_by_id/load_model_from_dir when ≥24GB RAM available.
//!
//! Memory requirement: ~20GB for a 14B model (all tensors in RAM simultaneously).
//! One-time operation; subsequent runs are skipped if bf16/ exists.

use std::collections::HashMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Instant;

use candle_core::quantized::gguf_file;
use candle_core::{DType, Device};
use safetensors::tensor::TensorView;
use safetensors::Dtype as StDtype;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let input_path = get_arg(&args, "--input").unwrap_or_else(|| {
        eprintln!("Usage: dequantize_gguf --input <path.gguf> --output <dir/>");
        eprintln!("Example:");
        eprintln!(
            "  dequantize_gguf --input ~/.continuum/genome/models/qwen14b-compacted-v1/qwen14b-compacted-q5ks.gguf \\"
        );
        eprintln!(
            "                  --output ~/.continuum/genome/models/qwen14b-compacted-v1/bf16/"
        );
        std::process::exit(1);
    });

    let output_dir = get_arg(&args, "--output").unwrap_or_else(|| {
        eprintln!("--output <dir/> required");
        std::process::exit(1);
    });

    let input = PathBuf::from(&input_path);
    let output = PathBuf::from(&output_dir);

    if !input.exists() {
        eprintln!("Error: GGUF file not found: {:?}", input);
        std::process::exit(1);
    }

    // Skip if output already exists (idempotent)
    let output_model = output.join("model.safetensors");
    if output_model.exists() {
        eprintln!("BF16 safetensors already exists at {:?} — skipping.", output_model);
        return;
    }

    println!("Dequantizing: {:?}", input);
    println!("Output dir:   {:?}", output);
    println!("This converts GGUF quantized weights to F16 safetensors.");
    println!("Expected RAM usage: ~20GB for a 14B model. One-time operation.");
    println!();

    if let Err(e) = dequantize(&input, &output) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    println!();
    println!("Done. The bf16/ directory will be auto-detected on next inference start.");
}

fn dequantize(gguf_path: &Path, output_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output dir {:?}: {e}", output_dir))?;

    // Phase 1: Read GGUF metadata
    let mut meta_file =
        std::fs::File::open(gguf_path).map_err(|e| format!("Cannot open GGUF: {e}"))?;
    let content = gguf_file::Content::read(&mut meta_file)
        .map_err(|e| format!("Cannot read GGUF metadata: {e}"))?;
    drop(meta_file);

    let tensor_names: Vec<String> = content.tensor_infos.keys().cloned().collect();
    let total = tensor_names.len();
    println!("Found {total} tensors to dequantize");

    // Phase 2: Dequantize all tensors — open a single reader and seek per tensor
    // content.tensor() seeks the reader to the tensor's offset before reading.
    let mut reader = BufReader::new(
        std::fs::File::open(gguf_path).map_err(|e| format!("Cannot reopen GGUF: {e}"))?,
    );

    // Store (name, shape, f16_bytes) for each tensor
    let mut tensor_data: Vec<(String, Vec<usize>, Vec<u8>)> = Vec::with_capacity(total);
    let start = Instant::now();

    for (i, name) in tensor_names.iter().enumerate() {
        if (i + 1) % 50 == 0 || i + 1 == total {
            let elapsed = start.elapsed().as_secs_f32();
            let remaining_est = if i > 0 {
                elapsed / (i as f32) * ((total - i - 1) as f32)
            } else {
                0.0
            };
            let idx = i + 1;
            println!(
                "  [{idx:>3}/{total}] {name:50} — {elapsed:.0}s elapsed, ~{remaining_est:.0}s remaining"
            );
        }

        // Read quantized tensor
        let qtensor = content
            .tensor(&mut reader, name, &Device::Cpu)
            .map_err(|e| format!("Read tensor '{name}': {e}"))?;

        // Dequantize to F32 on CPU (the dequant math lives in candle's kernel)
        let f32_tensor = qtensor
            .dequantize(&Device::Cpu)
            .map_err(|e| format!("Dequantize '{name}': {e}"))?;

        // Downcast to F16 (Metal prefers F16 for BF16-equivalent inference; half the size of F32)
        let f16_tensor = f32_tensor
            .to_dtype(DType::F16)
            .map_err(|e| format!("F32→F16 '{name}': {e}"))?;

        let shape: Vec<usize> = f16_tensor.dims().to_vec();

        // Extract raw bytes
        let f16_values = f16_tensor
            .flatten_all()
            .map_err(|e| format!("Flatten '{name}': {e}"))?
            .to_vec1::<half::f16>()
            .map_err(|e| format!("to_vec1 '{name}': {e}"))?;

        let bytes: Vec<u8> = f16_values.iter().flat_map(|x| x.to_le_bytes()).collect();

        tensor_data.push((name.clone(), shape, bytes));
    }

    let elapsed = start.elapsed();
    println!();
    println!(
        "Dequantized {total} tensors in {:.1}s ({:.1} tensors/s)",
        elapsed.as_secs_f32(),
        total as f32 / elapsed.as_secs_f32()
    );

    // Phase 3: Build TensorViews (borrow from stored bytes) and write safetensors
    println!("Writing safetensors (this may take a moment for large files)...");
    let write_start = Instant::now();

    // Build a HashMap<&str, TensorView> — views borrow from tensor_data
    let views: HashMap<&str, TensorView> = tensor_data
        .iter()
        .map(|(name, shape, bytes)| {
            let view = TensorView::new(StDtype::F16, shape.clone(), bytes)
                .map_err(|e| format!("TensorView for '{name}': {e}"))?;
            Ok((name.as_str(), view))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;

    let output_path = output_dir.join("model.safetensors");
    safetensors::tensor::serialize_to_file(views, None, &output_path)
        .map_err(|e| format!("Write safetensors to {:?}: {e}", output_path))?;

    println!(
        "Wrote {:?} in {:.1}s",
        output_path,
        write_start.elapsed().as_secs_f32()
    );

    // Phase 4: Copy supporting files so the safetensors backend can load the model.
    // head_topology.json (or compacted_model.topology.json) is required for CompactLlama
    // to load the pruned per-layer head counts correctly.
    let gguf_dir = gguf_path.parent().unwrap_or(Path::new("."));
    for filename in &[
        "config.json",
        "tokenizer.json",
        "tokenizer_config.json",
        "head_topology.json",
        "compacted_model.topology.json",
    ] {
        let src = gguf_dir.join(filename);
        if src.exists() {
            let dst = output_dir.join(filename);
            std::fs::copy(&src, &dst)
                .map_err(|e| format!("Copy {filename} from {:?}: {e}", gguf_dir))?;
            println!("Copied {filename}");
        } else if *filename == "config.json" || *filename == "tokenizer.json" {
            println!("WARNING: {filename} not found in GGUF dir — model may fail to load");
        }
    }

    let total_elapsed = start.elapsed();
    println!();
    println!("Total time: {:.1}s", total_elapsed.as_secs_f32());
    println!("Output: {:?}", output_dir);

    Ok(())
}

fn get_arg(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}
