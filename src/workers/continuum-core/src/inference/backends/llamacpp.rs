//! llama.cpp Backend — in-process via our vendored llama.cpp substrate.
//!
//! Loads the model directly (no HTTP, no external crate). Supports LoRA
//! hot-swap for genome paging. Metal/CUDA via feature flags.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use llama::{Batch, ContextParams, LoraAdapter, Model, ModelParams, Sampler};

use crate::runtime;

/// Configuration for loading a model via llama.cpp bindings.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file
    pub model_path: String,
    /// Context length
    pub context_length: u32,
    /// GPU layers to offload (-1 = all)
    pub n_gpu_layers: i32,
    /// Random seed for probabilistic sampling
    pub seed: u32,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            model_path: String::new(),
            context_length: 4096,
            n_gpu_layers: -1,
            seed: 42,
        }
    }
}

/// In-process llama.cpp backend.
///
/// Field declaration order matters: `loras` drops before `model` because
/// `LoraAdapter`'s FFI free expects the model's adapter memory to still
/// be live. Do not reorder.
pub struct LlamaCppBackend {
    /// Active LoRA adapters keyed by caller-chosen id. Applied to the
    /// context on every `generate()` call. Must drop before `model`.
    loras: Mutex<HashMap<String, Arc<LoraAdapter>>>,
    /// Loaded model. `Arc` so contexts can be cheaply spawned.
    model: Arc<Model>,
    context_params: ContextParams,
    config: LlamaCppConfig,
    model_id: String,
}

impl LlamaCppBackend {
    /// Load a GGUF model and initialize the backend.
    pub fn load(config: LlamaCppConfig) -> Result<Self, String> {
        let log = runtime::logger("llamacpp");
        let start = std::time::Instant::now();

        let model_path = PathBuf::from(&config.model_path);
        if !model_path.exists() {
            return Err(format!("Model file not found: {}", config.model_path));
        }

        let model_id = model_path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let model_params = ModelParams {
            n_gpu_layers: config.n_gpu_layers,
            use_mmap: true,
        };
        let model = Model::load(&model_path, model_params)?;

        let context_params = ContextParams {
            n_ctx: config.context_length,
            n_batch: 512,
        };

        log.info(&format!(
            "Loaded {} in {:?} (context={}, gpu_layers={})",
            model_id, start.elapsed(), config.context_length, config.n_gpu_layers
        ));

        Ok(Self {
            loras: Mutex::new(HashMap::new()),
            model: Arc::new(model),
            context_params,
            config,
            model_id,
        })
    }

    /// Generate text from a prompt.
    ///
    /// Creates a fresh context per call (KV cache is per-context, and we
    /// don't carry state across calls yet — that's a follow-up for
    /// chat-style use where prior turns stay in KV).
    pub fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        stop: &[&str],
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let start = std::time::Instant::now();

        let mut ctx = self.model.new_context(self.context_params.clone())?;

        // Apply active LoRA adapters (scale=1.0 for each — per-adapter scale
        // is API work for when genome paging actually ships).
        {
            let loras = self.loras.lock().unwrap();
            if !loras.is_empty() {
                let refs: Vec<(&LoraAdapter, f32)> = loras
                    .values()
                    .map(|a| (a.as_ref(), 1.0_f32))
                    .collect();
                ctx.set_loras(&refs)?;
            }
        }

        // Tokenize + prefill as a single-sequence batch.
        let tokens = self.model.tokenize(prompt, true, false)?;
        let prompt_len = tokens.len();
        ctx.decode(&Batch::for_tokens(tokens))?;

        let mut sampler = if temperature <= 0.0 {
            Sampler::greedy()
        } else {
            Sampler::chain()
                .temp(temperature)
                .dist(self.config.seed)
                .build()
        };

        let mut output = String::new();
        let mut n_decode = 0usize;

        while n_decode < max_tokens {
            let token = sampler.sample(&ctx, -1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let piece = self.model.token_to_piece(token);
            output.push_str(&piece);

            // Stop-sequence check (post-append so we don't emit partials
            // past the stop marker).
            if stop.iter().any(|s| output.contains(s)) {
                break;
            }

            // Feed the new token back as a 1-token batch. llama_batch_get_one
            // tracks position automatically from the KV cache.
            ctx.decode(&Batch::for_tokens(vec![token]))?;
            n_decode += 1;
        }

        let elapsed = start.elapsed();
        let tok_s = if elapsed.as_millis() > 0 {
            (n_decode as f64 / elapsed.as_millis() as f64) * 1000.0
        } else { 0.0 };

        log.info(&format!(
            "Generated {} tokens in {:?} ({:.1} tok/s, prompt={}tok)",
            n_decode, elapsed, tok_s, prompt_len
        ));

        Ok((output, n_decode))
    }

    /// Load a LoRA adapter from a file, keyed by id for later removal.
    /// `_scale` is stored for a future per-adapter scale API; currently
    /// all active adapters apply at scale 1.0.
    pub fn load_lora_adapter(&self, id: &str, path: &str, _scale: f32) -> Result<(), String> {
        let log = runtime::logger("llamacpp");
        let adapter = self.model.load_lora(path)?;
        self.loras.lock().unwrap().insert(id.to_string(), Arc::new(adapter));
        log.info(&format!("Loaded LoRA adapter: {}", id));
        Ok(())
    }

    /// Remove a LoRA adapter (it will not be applied to subsequent generations).
    pub fn remove_lora_adapter(&self, id: &str) -> Result<(), String> {
        let log = runtime::logger("llamacpp");
        let removed = self.loras.lock().unwrap().remove(id).is_some();
        if removed {
            log.info(&format!("Removed LoRA adapter: {}", id));
            Ok(())
        } else {
            Err(format!("LoRA adapter not found: {}", id))
        }
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }
}
