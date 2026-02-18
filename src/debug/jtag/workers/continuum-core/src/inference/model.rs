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
use candle_transformers::models::llama::{
    Cache, Llama, LlamaConfig,
};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::Tokenizer;

use super::backends::{GenomeAdapter, ModelBackend};
use super::backends::llama_safetensors::LlamaSafetensorsBackend;
use super::lora::{map_lora_name_to_model_name, merge_lora_weight, LoRAWeights};
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

        runtime::logger("candle").info(&format!("  Downloading {} weight shards...", shard_files.len()));

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
    let llama_config: LlamaConfig = serde_json::from_str(&config_str)?;
    log.info(&format!(
        "  Config: vocab_size={}, hidden_size={}, layers={}",
        llama_config.vocab_size, llama_config.hidden_size, llama_config.num_hidden_layers
    ));

    let use_flash_attn = false;
    let config = llama_config.into_config(use_flash_attn);

    // Context length from config — the model's true limit
    log.info(&format!(
        "  Context length: {} (from config.max_position_embeddings)",
        config.max_position_embeddings
    ));

    let eos_token_ids = LlamaSafetensorsBackend::parse_eos_tokens(&config.eos_token_id);
    log.info(&format!("  EOS token IDs: {:?}", eos_token_ids));

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
    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&weight_paths, dtype, &device)? };

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

/// Load default model from environment variable.
pub fn load_default_model() -> Result<Box<dyn ModelBackend>, Box<dyn std::error::Error + Send + Sync>> {
    let model_id = std::env::var("INFERENCE_MODEL_ID")
        .unwrap_or_else(|_| "unsloth/Llama-3.2-3B-Instruct".to_string());
    load_model_by_id(&model_id)
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
            adapter.adapter_id, adapter.scale, adapter.weights.len()
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
                        runtime::logger("candle").debug(&format!("  Failed to merge {}: {}", lora_name, e));
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
