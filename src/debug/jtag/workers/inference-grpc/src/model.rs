/**
 * Model Loading and Text Generation
 *
 * Handles downloading models from HuggingFace Hub, loading them into
 * Candle, and generating text with the loaded model.
 */

use std::collections::HashMap;
use std::time::Instant;
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::llama::{LlamaConfig, Config as LlamaModelConfig, Llama, Cache, LlamaEosToks};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::Tokenizer;
use log::{info, debug};
use rand::Rng;

use crate::lora::{LoRAWeights, merge_lora_weight, map_lora_name_to_model_name};

/// Model state containing loaded model, tokenizer, and cache
pub struct ModelState {
    pub model: Llama,
    pub cache: Cache,
    pub tokenizer: Tokenizer,
    pub device: Device,
    pub eos_token_ids: Vec<u32>,
    pub dtype: DType,
    pub config: LlamaModelConfig,
    pub model_id: String,
    /// Original weight file paths for LoRA merging
    pub weight_paths: Vec<std::path::PathBuf>,
}

impl ModelState {
    pub fn new(
        model: Llama,
        cache: Cache,
        tokenizer: Tokenizer,
        device: Device,
        eos_token_ids: Vec<u32>,
        dtype: DType,
        config: LlamaModelConfig,
        model_id: String,
        weight_paths: Vec<std::path::PathBuf>,
    ) -> Self {
        Self { model, cache, tokenizer, device, eos_token_ids, dtype, config, model_id, weight_paths }
    }

    pub fn clear_cache(&mut self) {
        self.cache = Cache::new(true, self.dtype, &self.config, &self.device)
            .expect("Failed to recreate cache");
    }
}

/// Generate text from a prompt using the loaded model
pub fn generate_text(
    state: &mut ModelState,
    prompt: &str,
    max_tokens: usize,
    temperature: f64,
) -> Result<(String, usize), String> {
    let start = Instant::now();

    let encoding = state.tokenizer.encode(prompt, true)
        .map_err(|e| format!("Tokenization failed: {}", e))?;
    let prompt_tokens: Vec<u32> = encoding.get_ids().to_vec();
    let prompt_len = prompt_tokens.len();

    if prompt_len == 0 {
        return Err("Empty prompt".to_string());
    }

    state.clear_cache();

    let seed = rand::thread_rng().gen::<u64>();
    let mut logits_processor = LogitsProcessor::new(seed, Some(temperature), None);

    let mut all_tokens = prompt_tokens.clone();

    for i in 0..max_tokens {
        let input_tokens = if i == 0 {
            all_tokens.clone()
        } else {
            vec![*all_tokens.last().unwrap()]
        };

        let input = Tensor::new(&input_tokens[..], &state.device)
            .map_err(|e| format!("Tensor creation failed: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze failed: {}", e))?;

        let pos = if i == 0 { 0 } else { all_tokens.len() - 1 };
        let logits = state.model.forward(&input, pos, &mut state.cache)
            .map_err(|e| format!("Forward pass failed: {}", e))?;

        if i == 0 {
            debug!("Raw logits shape: {:?}", logits.dims());
        }

        let last_logits = if logits.dims().len() == 2 {
            logits.squeeze(0)
                .map_err(|e| format!("Squeeze batch failed: {}", e))?
        } else if logits.dims().len() == 3 {
            let logits_2d = logits.squeeze(0)
                .map_err(|e| format!("Squeeze batch failed: {}", e))?;
            if logits_2d.dims()[0] > 1 {
                logits_2d.get(logits_2d.dims()[0] - 1)
                    .map_err(|e| format!("Get last logits failed: {}", e))?
            } else {
                logits_2d.squeeze(0)
                    .map_err(|e| format!("Squeeze seq failed: {}", e))?
            }
        } else {
            return Err(format!("Unexpected logits shape: {:?}", logits.dims()));
        };

        if i == 0 {
            debug!("Logits shape: {:?}, dtype: {:?}", last_logits.dims(), last_logits.dtype());
        }

        let next_token = logits_processor.sample(&last_logits)
            .map_err(|e| format!("Sampling failed: {}", e))?;

        if state.eos_token_ids.contains(&next_token) {
            break;
        }

        all_tokens.push(next_token);
    }

    let generated_tokens = &all_tokens[prompt_len..];
    let output_text = state.tokenizer.decode(generated_tokens, true)
        .map_err(|e| format!("Decode failed: {}", e))?;

    let duration = start.elapsed();
    info!("📝 Generated {} tokens in {:?}", generated_tokens.len(), duration);

    Ok((output_text, generated_tokens.len()))
}

/// Download model weights, handling both single file and sharded models
fn download_weights(repo: &hf_hub::api::sync::ApiRepo) -> Result<Vec<std::path::PathBuf>, String> {
    if let Ok(path) = repo.get("model.safetensors") {
        info!("  Weights (single file): {:?}", path);
        return Ok(vec![path]);
    }

    if let Ok(index_path) = repo.get("model.safetensors.index.json") {
        info!("  Found sharded weights index");
        let index_str = std::fs::read_to_string(&index_path)
            .map_err(|e| format!("Failed to read index: {}", e))?;
        let index: serde_json::Value = serde_json::from_str(&index_str)
            .map_err(|e| format!("Failed to parse index: {}", e))?;

        let weight_map = index.get("weight_map")
            .and_then(|v| v.as_object())
            .ok_or("Invalid index format: no weight_map")?;

        let mut shard_files: Vec<String> = weight_map.values()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
        shard_files.sort();
        shard_files.dedup();

        info!("  Downloading {} weight shards...", shard_files.len());

        let mut paths = Vec::new();
        for shard in &shard_files {
            let path = repo.get(shard)
                .map_err(|e| format!("Failed to get shard {}: {}", shard, e))?;
            paths.push(path);
        }

        return Ok(paths);
    }

    Err("No weights found (tried model.safetensors and sharded index)".to_string())
}

/// Parse EOS token IDs from Llama config
fn parse_eos_tokens(eos: &Option<LlamaEosToks>) -> Vec<u32> {
    match eos {
        Some(LlamaEosToks::Single(id)) => vec![*id],
        Some(LlamaEosToks::Multiple(ids)) => ids.clone(),
        None => vec![128001, 128009],
    }
}

/// Load a model by HuggingFace model ID
pub fn load_model_by_id(model_id: &str) -> Result<ModelState, Box<dyn std::error::Error + Send + Sync>> {
    info!("📥 Loading {}...", model_id);
    let start = Instant::now();

    #[cfg(feature = "metal")]
    let device = Device::new_metal(0).unwrap_or_else(|_| {
        info!("  Metal not available, falling back to CPU");
        Device::Cpu
    });
    #[cfg(not(feature = "metal"))]
    let device = Device::Cpu;

    info!("  Device: {:?}", device);

    let api = Api::new()?;
    let repo = api.repo(Repo::with_revision(
        model_id.to_string(),
        RepoType::Model,
        "main".to_string(),
    ));

    info!("  Downloading model files...");
    let config_path = repo.get("config.json")?;
    let tokenizer_path = repo.get("tokenizer.json")?;

    let weight_paths = download_weights(&repo)
        .map_err(|e| format!("Failed to download weights: {}", e))?;

    let config_str = std::fs::read_to_string(&config_path)?;
    let llama_config: LlamaConfig = serde_json::from_str(&config_str)?;
    info!("  Config: vocab_size={}, hidden_size={}, layers={}",
        llama_config.vocab_size, llama_config.hidden_size, llama_config.num_hidden_layers);

    let use_flash_attn = false;
    let config = llama_config.into_config(use_flash_attn);

    let eos_token_ids = parse_eos_tokens(&config.eos_token_id);
    info!("  EOS token IDs: {:?}", eos_token_ids);

    let tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

    let dtype = match &device {
        Device::Metal(_) => DType::BF16,
        _ => DType::F32,
    };
    info!("  Dtype: {:?}", dtype);

    info!("  Loading model weights from {} file(s)...", weight_paths.len());
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&weight_paths, dtype, &device)?
    };

    let model = Llama::load(vb, &config)?;
    let cache = Cache::new(true, dtype, &config, &device)?;

    let duration = start.elapsed();
    info!("✅ Model loaded in {:?}", duration);

    Ok(ModelState::new(model, cache, tokenizer, device, eos_token_ids, dtype, config, model_id.to_string(), weight_paths))
}

/// Load default model from environment variable
pub fn load_default_model() -> Result<ModelState, Box<dyn std::error::Error + Send + Sync>> {
    let model_id = std::env::var("INFERENCE_MODEL_ID")
        .unwrap_or_else(|_| "unsloth/Llama-3.2-3B-Instruct".to_string());
    load_model_by_id(&model_id)
}

/// Rebuild model with LoRA weights merged
///
/// Loads base model weights, applies LoRA deltas (W' = W + scale × B @ A),
/// and rebuilds the Llama model with merged weights.
///
/// Takes paths and config directly to avoid lifetime issues with async tasks.
pub fn rebuild_with_lora_from_paths(
    weight_paths: &[std::path::PathBuf],
    device: &Device,
    dtype: DType,
    config: &LlamaModelConfig,
    lora_weights: &HashMap<String, LoRAWeights>,
) -> Result<Llama, Box<dyn std::error::Error + Send + Sync>> {
    use safetensors::SafeTensors;

    info!("🔄 Rebuilding model with {} LoRA layers merged", lora_weights.len());
    let start = Instant::now();

    // Load all base weights into memory
    let mut all_tensors: HashMap<String, Tensor> = HashMap::new();

    for path in weight_paths {
        let data = std::fs::read(path)?;
        let tensors = SafeTensors::deserialize(&data)?;

        for (name, tensor_view) in tensors.tensors() {
            let shape: Vec<usize> = tensor_view.shape().to_vec();
            let st_dtype = tensor_view.dtype();

            // Convert to Candle tensor
            let tensor = match st_dtype {
                safetensors::Dtype::F32 => {
                    let data: Vec<f32> = tensor_view.data().chunks(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    Tensor::from_vec(data, shape.as_slice(), device)?
                }
                safetensors::Dtype::F16 => {
                    let data: Vec<half::f16> = tensor_view.data().chunks(2)
                        .map(|b| half::f16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    let f32_data: Vec<f32> = data.iter().map(|x| x.to_f32()).collect();
                    Tensor::from_vec(f32_data, shape.as_slice(), device)?
                }
                safetensors::Dtype::BF16 => {
                    let data: Vec<half::bf16> = tensor_view.data().chunks(2)
                        .map(|b| half::bf16::from_le_bytes([b[0], b[1]]))
                        .collect();
                    let f32_data: Vec<f32> = data.iter().map(|x| x.to_f32()).collect();
                    Tensor::from_vec(f32_data, shape.as_slice(), device)?
                }
                _ => {
                    info!("  ⚠ Skipping unsupported dtype: {:?} for {}", st_dtype, name);
                    continue;
                }
            };

            // Convert to target dtype
            let tensor = if tensor.dtype() != dtype {
                tensor.to_dtype(dtype)?
            } else {
                tensor
            };

            all_tensors.insert(name.to_string(), tensor);
        }
    }

    info!("  Loaded {} base tensors", all_tensors.len());

    // Apply LoRA deltas
    let mut merged_count = 0;
    let mut failed_count = 0;
    for (lora_name, lora) in lora_weights {
        let model_name = map_lora_name_to_model_name(lora_name);

        if let Some(base_weight) = all_tensors.get(&model_name) {
            match merge_lora_weight(base_weight, lora) {
                Ok(merged) => {
                    all_tensors.insert(model_name.clone(), merged);
                    merged_count += 1;
                    debug!("  ✓ Merged: {} → {}", lora_name, model_name);
                }
                Err(e) => {
                    info!("  ⚠ Failed to merge {}: {}", lora_name, e);
                    failed_count += 1;
                }
            }
        } else {
            debug!("  ⚠ No base weight for: {} (mapped to {})", lora_name, model_name);
            failed_count += 1;
        }
    }

    if failed_count > 0 {
        info!("  ⚠ {} LoRA layers failed to merge", failed_count);
    }

    info!("  Merged {} LoRA layers into base weights", merged_count);

    // Build VarBuilder from merged tensors
    let vb = VarBuilder::from_tensors(all_tensors, dtype, device);

    // Rebuild model
    let model = Llama::load(vb, config)?;

    let duration = start.elapsed();
    info!("✅ Model rebuilt with LoRA in {:?}", duration);

    Ok(model)
}
