/**
 * Inference gRPC Server with Candle LLM Backend
 *
 * Real model inference with Llama 3.2 3B for quality matching Ollama.
 * Loads model on startup, generates text via gRPC streaming.
 */

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use log::{info, debug};

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::llama::{LlamaConfig, Config as LlamaModelConfig, Llama, Cache, LlamaEosToks};
use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::Tokenizer;
use rand::Rng;

pub mod inference {
    tonic::include_proto!("inference");
}

use inference::inference_server::{Inference, InferenceServer};
use inference::{
    generate_response, Complete, GenerateRequest, GenerateResponse, PingRequest, PingResponse,
};

// ============================================================================
// Model State
// ============================================================================

struct ModelState {
    model: Llama,
    cache: Cache,
    tokenizer: Tokenizer,
    device: Device,
    eos_token_ids: Vec<u32>,
    dtype: DType,
    config: LlamaModelConfig,
}

impl ModelState {
    fn new(
        model: Llama,
        cache: Cache,
        tokenizer: Tokenizer,
        device: Device,
        eos_token_ids: Vec<u32>,
        dtype: DType,
        config: LlamaModelConfig,
    ) -> Self {
        Self { model, cache, tokenizer, device, eos_token_ids, dtype, config }
    }

    fn clear_cache(&mut self) {
        // Recreate cache to clear KV state
        self.cache = Cache::new(true, self.dtype, &self.config, &self.device)
            .expect("Failed to recreate cache");
    }
}

// ============================================================================
// Text Generation
// ============================================================================

fn generate_text(
    state: &mut ModelState,
    prompt: &str,
    max_tokens: usize,
    temperature: f64,
) -> Result<(String, usize), String> {
    let start = Instant::now();

    // Encode prompt (formatting done by caller)
    let encoding = state.tokenizer.encode(prompt, true)
        .map_err(|e| format!("Tokenization failed: {}", e))?;
    let prompt_tokens: Vec<u32> = encoding.get_ids().to_vec();
    let prompt_len = prompt_tokens.len();

    if prompt_len == 0 {
        return Err("Empty prompt".to_string());
    }

    // Clear KV cache
    state.clear_cache();

    // Setup logits processor for sampling
    let seed = rand::thread_rng().gen::<u64>();
    let mut logits_processor = LogitsProcessor::new(seed, Some(temperature), None);

    // Track all tokens for generation
    let mut all_tokens = prompt_tokens.clone();

    for i in 0..max_tokens {
        // Get input - full sequence on first pass, just last token after
        let input_tokens = if i == 0 {
            all_tokens.clone()
        } else {
            vec![*all_tokens.last().unwrap()]
        };

        let input = Tensor::new(&input_tokens[..], &state.device)
            .map_err(|e| format!("Tensor creation failed: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze failed: {}", e))?;

        // Forward pass with cache - Llama uses external cache
        let pos = if i == 0 { 0 } else { all_tokens.len() - 1 };
        let logits = state.model.forward(&input, pos, &mut state.cache)
            .map_err(|e| format!("Forward pass failed: {}", e))?;

        // Debug: log raw logits shape
        if i == 0 {
            debug!("Raw logits shape: {:?}", logits.dims());
        }

        // Llama forward already returns [batch, vocab] for last position
        // But let's handle both cases safely
        let last_logits = if logits.dims().len() == 2 {
            // Shape is [batch, vocab] - squeeze batch
            logits.squeeze(0)
                .map_err(|e| format!("Squeeze batch failed: {}", e))?
        } else if logits.dims().len() == 3 {
            // Shape is [batch, seq, vocab] - get last token
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

        // Debug: log logits info on first token
        if i == 0 {
            debug!("Logits shape: {:?}, dtype: {:?}", last_logits.dims(), last_logits.dtype());
            if let Ok(logits_vec) = last_logits.to_vec1::<f32>() {
                let min = logits_vec.iter().cloned().fold(f32::INFINITY, f32::min);
                let max = logits_vec.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                let has_nan = logits_vec.iter().any(|x| x.is_nan());
                let has_inf = logits_vec.iter().any(|x| x.is_infinite());
                debug!("Logits stats: min={:.4}, max={:.4}, has_nan={}, has_inf={}", min, max, has_nan, has_inf);
            }
        }

        // Sample next token
        let next_token = logits_processor.sample(&last_logits)
            .map_err(|e| format!("Sampling failed: {}", e))?;

        // Check for EOS (Llama can have multiple EOS tokens)
        if state.eos_token_ids.contains(&next_token) {
            break;
        }

        all_tokens.push(next_token);
    }

    // Decode generated tokens (skip prompt tokens)
    let generated_tokens = &all_tokens[prompt_len..];
    let output_text = state.tokenizer.decode(generated_tokens, true)
        .map_err(|e| format!("Decode failed: {}", e))?;

    let duration = start.elapsed();
    info!(" Generated {} tokens in {:?}", generated_tokens.len(), duration);

    Ok((output_text, generated_tokens.len()))
}

// ============================================================================
// Model Loading
// ============================================================================

/// Download model weights, handling both single file and sharded models
fn download_weights(repo: &hf_hub::api::sync::ApiRepo) -> Result<Vec<std::path::PathBuf>, String> {
    // Try single weights file first
    if let Ok(path) = repo.get("model.safetensors") {
        info!(" Weights (single file): {:?}", path);
        return Ok(vec![path]);
    }

    // Try sharded weights (model.safetensors.index.json)
    if let Ok(index_path) = repo.get("model.safetensors.index.json") {
        info!(" Found sharded weights index: {:?}", index_path);
        let index_str = std::fs::read_to_string(&index_path)
            .map_err(|e| format!("Failed to read index: {}", e))?;
        let index: serde_json::Value = serde_json::from_str(&index_str)
            .map_err(|e| format!("Failed to parse index: {}", e))?;

        // Get unique shard files from weight_map
        let weight_map = index.get("weight_map")
            .and_then(|v| v.as_object())
            .ok_or("Invalid index format: no weight_map")?;

        let mut shard_files: Vec<String> = weight_map.values()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
        shard_files.sort();
        shard_files.dedup();

        info!(" Downloading {} weight shards...", shard_files.len());

        let mut paths = Vec::new();
        for shard in &shard_files {
            info!("   Downloading: {}", shard);
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
        None => vec![128001, 128009], // Default Llama 3 EOS tokens
    }
}

fn load_model() -> Result<ModelState, Box<dyn std::error::Error + Send + Sync>> {
    // Use Meta-Llama-3.2-3B-Instruct (gated model - requires HF_TOKEN)
    // Fall back to unsloth/Llama-3.2-3B-Instruct if auth fails
    let model_id = std::env::var("INFERENCE_MODEL_ID")
        .unwrap_or_else(|_| "unsloth/Llama-3.2-3B-Instruct".to_string());

    info!(" Loading {}...", model_id);
    let start = Instant::now();

    // Use Metal on macOS, CPU otherwise
    #[cfg(feature = "metal")]
    let device = Device::new_metal(0).unwrap_or_else(|_| {
        info!(" Metal not available, falling back to CPU");
        Device::Cpu
    });
    #[cfg(not(feature = "metal"))]
    let device = Device::Cpu;

    info!(" Using device: {:?}", device);

    // Download model from HuggingFace Hub
    let api = Api::new()?;
    let repo = api.repo(Repo::with_revision(
        model_id.clone(),
        RepoType::Model,
        "main".to_string(),
    ));

    info!(" Downloading model files...");
    let config_path = repo.get("config.json")?;
    let tokenizer_path = repo.get("tokenizer.json")?;

    // Download weights (handles both single and sharded)
    let weight_paths = download_weights(&repo)
        .map_err(|e| format!("Failed to download weights: {}", e))?;

    // Load config - Llama uses LlamaConfig which converts to Config
    let config_str = std::fs::read_to_string(&config_path)?;
    let llama_config: LlamaConfig = serde_json::from_str(&config_str)?;
    info!(" Config: vocab_size={}, hidden_size={}, layers={}",
        llama_config.vocab_size, llama_config.hidden_size, llama_config.num_hidden_layers);

    // Convert to model config (no flash attention on Metal)
    let use_flash_attn = false;
    let config = llama_config.into_config(use_flash_attn);

    // Get EOS token IDs
    let eos_token_ids = parse_eos_tokens(&config.eos_token_id);
    info!(" EOS token IDs: {:?}", eos_token_ids);

    // Load tokenizer
    let tokenizer = Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

    // Determine dtype based on device (BF16 for Metal, F32 for CPU)
    let dtype = match &device {
        Device::Metal(_) => DType::BF16,  // BF16 is faster and more stable on Metal
        _ => DType::F32,
    };
    info!(" Using dtype: {:?}", dtype);

    // Load model weights from all shards
    info!(" Loading model weights from {} file(s)...", weight_paths.len());
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&weight_paths, dtype, &device)?
    };

    // Load Llama model
    let model = Llama::load(vb, &config)?;

    // Create cache for KV storage
    let cache = Cache::new(true, dtype, &config, &device)?;

    let duration = start.elapsed();
    info!(" Model loaded in {:?}", duration);

    Ok(ModelState::new(model, cache, tokenizer, device, eos_token_ids, dtype, config))
}

// ============================================================================
// gRPC Service
// ============================================================================

pub struct InferenceService {
    state: Arc<RwLock<Option<ModelState>>>,
}

impl InferenceService {
    fn new(state: Option<ModelState>) -> Self {
        Self {
            state: Arc::new(RwLock::new(state)),
        }
    }
}

#[tonic::async_trait]
impl Inference for InferenceService {
    async fn ping(&self, _request: Request<PingRequest>) -> Result<Response<PingResponse>, Status> {
        let state = self.state.read().await;
        let model_loaded = state.is_some();

        Ok(Response::new(PingResponse {
            message: if model_loaded { "pong (model loaded)".to_string() } else { "pong (no model)".to_string() },
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        }))
    }

    type GenerateStream = ReceiverStream<Result<GenerateResponse, Status>>;

    async fn generate(
        &self,
        request: Request<GenerateRequest>,
    ) -> Result<Response<Self::GenerateStream>, Status> {
        let req = request.into_inner();
        let model_id = req.model_id;
        let prompt = req.prompt;
        let max_tokens = req.max_tokens.max(10) as usize;
        let temperature = if req.temperature > 0.0 { req.temperature } else { 0.7 };

        info!(" Generate: model={}, prompt={} chars, max_tokens={}, temp={:.2}",
            model_id, prompt.len(), max_tokens, temperature);

        let (tx, rx) = mpsc::channel(32);
        let state_arc = self.state.clone();

        tokio::spawn(async move {
            let start = Instant::now();

            // Get exclusive access to model
            let mut state_guard = state_arc.write().await;

            let result = match state_guard.as_mut() {
                Some(model_state) => {
                    // Real inference
                    generate_text(model_state, &prompt, max_tokens, temperature)
                }
                None => {
                    // No model loaded - return error
                    Err("Model not loaded".to_string())
                }
            };

            drop(state_guard); // Release lock before sending

            let duration = start.elapsed().as_millis() as i32;

            let response = match result {
                Ok((text, tokens)) => {
                    GenerateResponse {
                        response: Some(generate_response::Response::Complete(Complete {
                            text,
                            tokens: tokens as i32,
                            duration_ms: duration,
                        })),
                    }
                }
                Err(e) => {
                    // Send error as complete with error text
                    GenerateResponse {
                        response: Some(generate_response::Response::Complete(Complete {
                            text: format!("ERROR: {}", e),
                            tokens: 0,
                            duration_ms: duration,
                        })),
                    }
                }
            };

            if tx.send(Ok(response)).await.is_err() {
                info!(" Failed to send response, client gone");
            } else {
                info!(" Response sent ({}ms)", duration);
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging - RUST_LOG=debug for verbose output
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let addr = "127.0.0.1:50051".parse()?;

    let model_id = std::env::var("INFERENCE_MODEL_ID")
        .unwrap_or_else(|_| "unsloth/Llama-3.2-3B-Instruct".to_string());

    info!("===========================================");
    info!("  Inference gRPC Server (Candle + Llama)");
    info!("  Model: {}", model_id);
    info!("  Listening on: {}", addr);
    info!("===========================================");

    // Load model on startup
    let model_state = match load_model() {
        Ok(state) => {
            info!(" ✅ Model ready for inference");
            Some(state)
        }
        Err(e) => {
            info!(" ❌ Failed to load model: {}", e);
            info!(" Server will start but return errors");
            None
        }
    };

    let service = InferenceService::new(model_state);

    Server::builder()
        .add_service(InferenceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
