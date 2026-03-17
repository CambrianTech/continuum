//! Model Loading Utilities
//!
//! Handles downloading models from HuggingFace Hub, loading them into
//! Candle, and LoRA weight merging. Model state lives in
//! `backends::LlamaSafetensorsBackend` — this module provides the loading
//! and utility functions.
//!
//! Supports:
//! - Llama architecture models (safetensors format)
//! - BF16/FP32 precision
//! - GPU acceleration (Metal/CUDA)
//! - LoRA weight merging (single and multi-adapter)

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::llama::{Cache, Llama, LlamaConfig};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::Tokenizer;

use super::backends;
use super::backends::compact_llama_safetensors::CompactLlamaSafetensorsBackend;
use super::backends::llama_safetensors::LlamaSafetensorsBackend;
use super::backends::qwen2_safetensors::Qwen2SafetensorsBackend;
use super::backends::{GenomeAdapter, ModelBackend};
use super::lora::{map_lora_name_to_model_name, merge_lora_weight, LoRAWeights};
use super::vendored::compact_llama;
use super::vendored::qwen2::{Qwen2, Qwen2Config};
use crate::modules::plasticity::topology;
use crate::runtime;

/// Select best available compute device.
pub fn select_best_device() -> Device {
    #[cfg(feature = "cuda")]
    {
        if let Ok(device) = Device::new_cuda(0) {
            runtime::logger("candle").info("  Using CUDA device");
            return device;
        }
        runtime::logger("candle").info("  CUDA not available");
    }

    #[cfg(feature = "metal")]
    {
        if let Ok(device) = Device::new_metal(0) {
            runtime::logger("candle").info("  Using Metal device");
            return device;
        }
        runtime::logger("candle").info("  Metal not available");
    }

    runtime::logger("candle").info("  Using CPU (no GPU acceleration)");
    Device::Cpu
}

/// Download model weights, handling both single file and sharded models.
fn download_weights(repo: &hf_hub::api::sync::ApiRepo) -> Result<Vec<PathBuf>, String> {
    if let Ok(path) = repo.get("model.safetensors") {
        runtime::logger("candle").info(&format!("  Weights (single file): {:?}", path));
        return Ok(vec![path]);
    }

    if let Ok(index_path) = repo.get("model.safetensors.index.json") {
        runtime::logger("candle").info("  Found sharded weights index");
        let index_str = std::fs::read_to_string(&index_path)
            .map_err(|e| format!("Failed to read index: {e}"))?;
        let index: serde_json::Value =
            serde_json::from_str(&index_str).map_err(|e| format!("Failed to parse index: {e}"))?;

        let weight_map = index
            .get("weight_map")
            .and_then(|v| v.as_object())
            .ok_or("Invalid index format: no weight_map")?;

        let mut shard_files: Vec<String> = weight_map
            .values()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
        shard_files.sort();
        shard_files.dedup();

        runtime::logger("candle").info(&format!(
            "  Downloading {} weight shards...",
            shard_files.len()
        ));

        let mut paths = Vec::new();
        for shard in &shard_files {
            let path = repo
                .get(shard)
                .map_err(|e| format!("Failed to get shard {shard}: {e}"))?;
            paths.push(path);
        }

        return Ok(paths);
    }

    Err("No weights found (tried model.safetensors and sharded index)".to_string())
}

/// Load a safetensors model by HuggingFace model ID.
///
/// Returns a `Box<dyn ModelBackend>` — context_length comes from
/// `config.json` → `max_position_embeddings`. No hardcoded values.
pub fn load_model_by_id(
    model_id: &str,
) -> Result<Box<dyn ModelBackend>, Box<dyn std::error::Error + Send + Sync>> {
    let log = runtime::logger("candle");
    log.info(&format!("Loading model: {}", model_id));
    let start = Instant::now();

    let device = select_best_device();
    log.info(&format!("  Device: {:?}", device));

    let api = Api::new()?;
    let repo = api.repo(Repo::with_revision(
        model_id.to_string(),
        RepoType::Model,
        "main".to_string(),
    ));

    log.info("  Downloading model files...");
    let config_path = repo.get("config.json")?;
    let tokenizer_path = repo.get("tokenizer.json")?;

    let weight_paths =
        download_weights(&repo).map_err(|e| format!("Failed to download weights: {e}"))?;

    let config_str = std::fs::read_to_string(&config_path)?;

    // Detect architecture from config.json to route to correct backend
    let raw_config: serde_json::Value = serde_json::from_str(&config_str)?;
    let model_type = raw_config
        .get("model_type")
        .and_then(|v| v.as_str())
        .unwrap_or("llama");

    log.info(&format!("  Model type: {model_type}"));

    let tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| format!("Failed to load tokenizer: {e}"))?;

    let dtype = match &device {
        Device::Metal(_) => DType::BF16,
        _ => DType::F32,
    };
    log.info(&format!("  Dtype: {:?}", dtype));

    log.info(&format!(
        "  Loading model weights from {} file(s)...",
        weight_paths.len()
    ));

    match model_type {
        "qwen2" => {
            let qwen2_config = Qwen2Config::from_json(&raw_config)
                .map_err(|e| format!("Invalid Qwen2 config: {e}"))?;

            log.info(&format!(
                "  Qwen2 config: {}L, {}Qh, {}KVh, hd={}, hidden={}, ctx={}",
                qwen2_config.num_hidden_layers,
                qwen2_config.num_attention_heads,
                qwen2_config.num_key_value_heads,
                qwen2_config.head_dim,
                qwen2_config.hidden_size,
                qwen2_config.max_position_embeddings,
            ));

            // Qwen2 EOS tokens from tokenizer config or defaults
            let eos_token_ids = raw_config
                .get("eos_token_id")
                .and_then(|v| v.as_u64())
                .map(|id| vec![id as u32])
                .unwrap_or_else(|| vec![151645, 151643]); // Qwen2 defaults

            log.info(&format!("  EOS token IDs: {:?}", eos_token_ids));

            let vb =
                unsafe { VarBuilder::from_mmaped_safetensors(&weight_paths, dtype, &device)? };
            let model = Qwen2::load(vb, &qwen2_config)
                .map_err(|e| format!("Qwen2 load failed: {e}"))?;

            let duration = start.elapsed();
            log.info(&format!("Qwen2 model loaded in {:?}", duration));

            Ok(Box::new(Qwen2SafetensorsBackend::new(
                model,
                tokenizer,
                device,
                dtype,
                model_id.to_string(),
                eos_token_ids,
                weight_paths,
            )))
        }
        _ => {
            // Llama-family models (llama, codellama, mistral, etc.)
            let llama_config: LlamaConfig = serde_json::from_str(&config_str)?;
            log.info(&format!(
                "  Config: vocab_size={}, hidden_size={}, layers={}",
                llama_config.vocab_size, llama_config.hidden_size, llama_config.num_hidden_layers
            ));

            let use_flash_attn = false;
            let config = llama_config.into_config(use_flash_attn);

            log.info(&format!(
                "  Context length: {} (from config.max_position_embeddings)",
                config.max_position_embeddings
            ));

            let eos_token_ids =
                LlamaSafetensorsBackend::parse_eos_tokens(&config.eos_token_id);
            log.info(&format!("  EOS token IDs: {:?}", eos_token_ids));

            // Check for compacted model topology
            let model_dir = weight_paths
                .first()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf());

            if let Some(ref dir) = model_dir {
                if let Some(topo_path) = compact_llama::detect_topology(dir) {
                    log.info(&format!(
                        "  Detected compacted topology: {:?}",
                        topo_path
                    ));
                    let topo = topology::load_topology(&topo_path)
                        .map_err(|e| format!("Failed to load topology: {e}"))?;

                    log.info(&format!(
                        "  Compact model: {:.1}% parameter reduction, {} layers",
                        topo.parameter_reduction * 100.0,
                        topo.layers.len()
                    ));

                    let vb = unsafe {
                        VarBuilder::from_mmaped_safetensors(&weight_paths, dtype, &device)?
                    };
                    let compact_model =
                        compact_llama::CompactLlama::load(vb, &config, &topo)
                            .map_err(|e| format!("CompactLlama load failed: {e}"))?;

                    let duration = start.elapsed();
                    log.info(&format!("Compact model loaded in {:?}", duration));

                    return Ok(Box::new(CompactLlamaSafetensorsBackend::new(
                        compact_model,
                        tokenizer,
                        device,
                        dtype,
                        config,
                        topo,
                        model_id.to_string(),
                        eos_token_ids,
                        weight_paths,
                    )));
                }
            }

            // Standard (non-compacted) Llama path
            let vb =
                unsafe { VarBuilder::from_mmaped_safetensors(&weight_paths, dtype, &device)? };

            let model = Llama::load(vb, &config)?;
            let cache = Cache::new(true, dtype, &config, &device)?;

            let duration = start.elapsed();
            log.info(&format!("Model loaded in {:?}", duration));

            Ok(Box::new(LlamaSafetensorsBackend::new(
                model,
                cache,
                tokenizer,
                device,
                dtype,
                config,
                model_id.to_string(),
                eos_token_ids,
                weight_paths,
            )))
        }
    }
}

/// Load default model from environment variable.
pub fn load_default_model(
) -> Result<Box<dyn ModelBackend>, Box<dyn std::error::Error + Send + Sync>> {
    let model_id = std::env::var("INFERENCE_MODEL_ID")
        .unwrap_or_else(|_| "unsloth/Llama-3.2-3B-Instruct".to_string());
    load_model_by_id(&model_id)
}

/// Load a safetensors model from a local directory.
///
/// Auto-detects architecture from config.json (supports Llama, Qwen2).
/// Used for locally-stored models (compacted, downloaded, etc.).
pub fn load_model_from_dir(
    model_dir: &std::path::Path,
    model_id: &str,
) -> Result<Box<dyn ModelBackend>, Box<dyn std::error::Error + Send + Sync>> {
    let log = runtime::logger("candle");
    log.info(&format!("Loading model from dir: {:?}", model_dir));
    let start = Instant::now();

    let device = select_best_device();

    let config_path = model_dir.join("config.json");
    let tokenizer_path = model_dir.join("tokenizer.json");

    if !config_path.exists() {
        return Err(format!("No config.json in {:?}", model_dir).into());
    }
    if !tokenizer_path.exists() {
        return Err(format!("No tokenizer.json in {:?}", model_dir).into());
    }

    // Find weight files
    let mut weight_paths: Vec<PathBuf> = Vec::new();
    let single = model_dir.join("model.safetensors");
    if single.exists() {
        weight_paths.push(single);
    } else {
        // Sharded: model-00001-of-NNNNN.safetensors
        let mut entries: Vec<_> = std::fs::read_dir(model_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("model-") && n.ends_with(".safetensors"))
                    .unwrap_or(false)
            })
            .collect();
        entries.sort();
        weight_paths = entries;
    }

    if weight_paths.is_empty() {
        // Check for GGUF files as fallback
        let mut gguf_files: Vec<PathBuf> = std::fs::read_dir(model_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e == "gguf")
                    .unwrap_or(false)
            })
            .collect();
        gguf_files.sort();

        if let Some(gguf_path) = gguf_files.first() {
            log.info(&format!("  Found GGUF: {:?}", gguf_path));
            let tokenizer = Tokenizer::from_file(&tokenizer_path)
                .map_err(|e| format!("Failed to load tokenizer: {e}"))?;
            let backend = backends::load_gguf_backend(gguf_path, tokenizer, model_id, &device)?;
            let duration = start.elapsed();
            log.info(&format!(
                "GGUF loaded from dir in {:?} (arch={}, ctx={})",
                duration,
                backend.architecture(),
                backend.context_length()
            ));
            return Ok(backend);
        }

        return Err(format!("No safetensors or GGUF files in {:?}", model_dir).into());
    }

    log.info(&format!("  {} weight file(s)", weight_paths.len()));

    let config_str = std::fs::read_to_string(&config_path)?;
    let raw_config: serde_json::Value = serde_json::from_str(&config_str)?;
    let model_type = raw_config
        .get("model_type")
        .and_then(|v| v.as_str())
        .unwrap_or("llama");

    log.info(&format!("  Model type: {model_type}"));

    let tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| format!("Failed to load tokenizer: {e}"))?;

    let dtype = match &device {
        Device::Metal(_) => DType::BF16,
        _ => DType::F32,
    };

    match model_type {
        "qwen2" => {
            let qwen2_config = Qwen2Config::from_json(&raw_config)
                .map_err(|e| format!("Invalid Qwen2 config: {e}"))?;

            log.info(&format!(
                "  Qwen2: {}L, {}Qh, {}KVh, hd={}, ctx={}",
                qwen2_config.num_hidden_layers,
                qwen2_config.num_attention_heads,
                qwen2_config.num_key_value_heads,
                qwen2_config.head_dim,
                qwen2_config.max_position_embeddings,
            ));

            let eos_token_ids = raw_config
                .get("eos_token_id")
                .and_then(|v| v.as_u64())
                .map(|id| vec![id as u32])
                .unwrap_or_else(|| vec![151645, 151643]);

            let vb =
                unsafe { VarBuilder::from_mmaped_safetensors(&weight_paths, dtype, &device)? };
            let model = Qwen2::load(vb, &qwen2_config)
                .map_err(|e| format!("Qwen2 load failed: {e}"))?;

            let duration = start.elapsed();
            log.info(&format!("Qwen2 loaded from dir in {:?}", duration));

            Ok(Box::new(Qwen2SafetensorsBackend::new(
                model,
                tokenizer,
                device,
                dtype,
                model_id.to_string(),
                eos_token_ids,
                weight_paths,
            )))
        }
        _ => {
            // Llama-family
            let llama_config: LlamaConfig = serde_json::from_str(&config_str)?;
            let config = llama_config.into_config(false);
            let eos_token_ids =
                LlamaSafetensorsBackend::parse_eos_tokens(&config.eos_token_id);

            let vb =
                unsafe { VarBuilder::from_mmaped_safetensors(&weight_paths, dtype, &device)? };
            let model = Llama::load(vb, &config)?;
            let cache = Cache::new(true, dtype, &config, &device)?;

            let duration = start.elapsed();
            log.info(&format!("Llama loaded from dir in {:?}", duration));

            Ok(Box::new(LlamaSafetensorsBackend::new(
                model,
                cache,
                tokenizer,
                device,
                dtype,
                config,
                model_id.to_string(),
                eos_token_ids,
                weight_paths,
            )))
        }
    }
}

/// Rebuild model with multiple stacked LoRA adapters (genome).
///
/// Applies formula: W' = W + sum(scale_i x B_i @ A_i)
/// Each adapter's weights are added to the base with its own scale factor.
pub fn rebuild_with_stacked_lora(
    weight_paths: &[PathBuf],
    device: &Device,
    dtype: DType,
    config: &candle_transformers::models::llama::Config,
    adapters: &[GenomeAdapter],
) -> Result<Llama, Box<dyn std::error::Error + Send + Sync>> {
    use safetensors::SafeTensors;

    let total_layers: usize = adapters.iter().map(|a| a.weights.len()).sum();
    runtime::logger("candle").info(&format!(
        "Rebuilding model with {} adapters ({} total LoRA layers)",
        adapters.len(),
        total_layers
    ));
    let start = Instant::now();

    let mut all_tensors: HashMap<String, Tensor> = HashMap::new();

    for path in weight_paths {
        let data = std::fs::read(path)?;
        let tensors = SafeTensors::deserialize(&data)?;

        for (name, tensor_view) in tensors.tensors() {
            let shape: Vec<usize> = tensor_view.shape().to_vec();
            let st_dtype = tensor_view.dtype();

            let tensor = match st_dtype {
                safetensors::Dtype::F32 => {
                    let data: Vec<f32> = tensor_view
                        .data()
                        .chunks(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    Tensor::from_vec(data, shape.as_slice(), device)?
                }
                safetensors::Dtype::F16 => {
                    let data: Vec<half::f16> = tensor_view
                        .data()
                        .chunks(2)
                        .map(|b| half::f16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    let f32_data: Vec<f32> = data.iter().map(|x| x.to_f32()).collect();
                    Tensor::from_vec(f32_data, shape.as_slice(), device)?
                }
                safetensors::Dtype::BF16 => {
                    let data: Vec<half::bf16> = tensor_view
                        .data()
                        .chunks(2)
                        .map(|b| half::bf16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    let f32_data: Vec<f32> = data.iter().map(|x| x.to_f32()).collect();
                    Tensor::from_vec(f32_data, shape.as_slice(), device)?
                }
                _ => continue,
            };

            let tensor = if tensor.dtype() != dtype {
                tensor.to_dtype(dtype)?
            } else {
                tensor
            };

            all_tensors.insert(name.to_string(), tensor);
        }
    }

    runtime::logger("candle").info(&format!("  Loaded {} base tensors", all_tensors.len()));

    // Apply LoRA deltas from ALL adapters: W' = W + sum(scale_i x B_i @ A_i)
    let mut merged_count = 0;
    let mut failed_count = 0;

    for adapter in adapters {
        runtime::logger("candle").info(&format!(
            "  Applying adapter '{}' (scale={}, {} layers)",
            adapter.adapter_id,
            adapter.scale,
            adapter.weights.len()
        ));

        for (lora_name, lora) in &adapter.weights {
            let model_name = map_lora_name_to_model_name(lora_name);

            if let Some(base_weight) = all_tensors.get(&model_name) {
                let effective_scale = lora.scale * adapter.scale;
                let scaled_lora = LoRAWeights {
                    lora_a: lora.lora_a.clone(),
                    lora_b: lora.lora_b.clone(),
                    scale: effective_scale,
                };

                match merge_lora_weight(base_weight, &scaled_lora) {
                    Ok(merged) => {
                        all_tensors.insert(model_name.clone(), merged);
                        merged_count += 1;
                    }
                    Err(e) => {
                        runtime::logger("candle")
                            .debug(&format!("  Failed to merge {}: {}", lora_name, e));
                        failed_count += 1;
                    }
                }
            } else {
                failed_count += 1;
            }
        }
    }

    if failed_count > 0 {
        runtime::logger("candle").info(&format!("  {} LoRA layers failed to merge", failed_count));
    }

    runtime::logger("candle").info(&format!(
        "  Merged {} LoRA layers from {} adapters",
        merged_count,
        adapters.len()
    ));

    let vb = VarBuilder::from_tensors(all_tensors, dtype, device);
    let model = Llama::load(vb, config)?;

    let duration = start.elapsed();
    runtime::logger("candle").info(&format!("Genome applied in {:?}", duration));

    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Smoke test: load Qwen2.5-Coder-32B compacted Q4_K_M GGUF from local disk
    /// and generate a short completion on Metal.
    ///
    /// Run with: cargo test -p continuum-core --release -- --ignored test_qwen32b_compacted_gguf_inference --nocapture
    #[test]
    #[ignore]
    fn test_qwen32b_compacted_gguf_inference() {
        let model_dir = Path::new(
            &std::env::var("HOME").unwrap_or_else(|_| "/Users/joel".to_string()),
        )
        .join(".continuum/genome/models/qwen32b-compacted-v2");

        if !model_dir.exists() {
            eprintln!("Skipping: model dir not found at {:?}", model_dir);
            return;
        }

        eprintln!("Loading model from {:?}...", model_dir);
        let start = Instant::now();

        let mut backend = load_model_from_dir(&model_dir, "qwen32b-compacted-q4km")
            .expect("Failed to load model");

        let load_time = start.elapsed();
        eprintln!("Model loaded in {:.1?}", load_time);
        eprintln!(
            "  arch={}, ctx={}, format={:?}",
            backend.architecture(),
            backend.context_length(),
            backend.format()
        );

        // Generate a short coding completion
        let prompt = "<|im_start|>user\nWrite a Python function called is_prime that checks if a number is prime.<|im_end|>\n<|im_start|>assistant\n";

        eprintln!("Generating (max 256 tokens, temp 0.1)...");
        let gen_start = Instant::now();
        let (output, token_count) = backends::generate(backend.as_mut(), prompt, 256, 0.1)
            .expect("Generation failed");
        let gen_time = gen_start.elapsed();

        eprintln!("\n--- Output ({} tokens in {:.1?}) ---", token_count, gen_time);
        eprintln!("{}", output);
        eprintln!("--- End ---\n");

        if token_count > 0 {
            let tokens_per_sec = token_count as f64 / gen_time.as_secs_f64();
            eprintln!("Speed: {:.1} tok/s", tokens_per_sec);
        }

        // Basic assertions
        assert!(token_count > 0, "Should generate at least one token");
        assert!(!output.is_empty(), "Output should not be empty");
        // Check for some sign of coherent code
        assert!(
            output.contains("def ") || output.contains("prime") || output.contains("return"),
            "Output should contain recognizable code patterns: {}",
            output
        );
    }
}
