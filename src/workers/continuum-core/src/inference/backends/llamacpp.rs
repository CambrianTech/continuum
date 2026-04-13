//! llama.cpp Backend — in-process bindings via llama-cpp-2.
//!
//! Loads the model directly (no HTTP server). Supports LoRA hot-swap
//! for genome paging. Metal/CUDA via feature flags.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel, Special, LlamaLoraAdapter},
    sampling::LlamaSampler,
};

/// Wrapper to make LlamaLoraAdapter Send+Sync.
/// The underlying C pointer is thread-safe as long as we only call
/// context methods from one thread at a time (enforced by the context Mutex
/// pattern in actual use). Interior mutability via Mutex since lora_adapter_set
/// requires &mut.
struct LoraWrapper(Mutex<LlamaLoraAdapter>);
unsafe impl Send for LoraWrapper {}
unsafe impl Sync for LoraWrapper {}

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
    /// Random seed
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
/// Holds a LlamaModel and context. Thread-safe (protected by Mutex).
/// Supports LoRA hot-swap for genome paging.
pub struct LlamaCppBackend {
    backend: Arc<LlamaBackend>,
    model: Arc<LlamaModel>,
    context_params: LlamaContextParams,
    /// Loaded LoRA adapters keyed by caller-chosen id.
    /// Adapters are kept alive here; context references them via set().
    loras: Mutex<HashMap<String, Arc<LoraWrapper>>>,
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

        let backend = LlamaBackend::init()
            .map_err(|e| format!("Failed to init llama backend: {e}"))?;

        let model_params = {
            let mut params = LlamaModelParams::default();
            params = params.with_n_gpu_layers(config.n_gpu_layers as u32);
            params
        };

        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
            .map_err(|e| format!("Failed to load model: {e}"))?;

        let context_params = LlamaContextParams::default()
            .with_n_ctx(Some(NonZeroU32::new(config.context_length).expect("ctx > 0")));
        let _ = config.seed; // seed is on sampler, not context in this version

        log.info(&format!(
            "Loaded {} in {:?} (context={}, gpu_layers={})",
            model_id, start.elapsed(), config.context_length, config.n_gpu_layers
        ));

        Ok(Self {
            backend: Arc::new(backend),
            model: Arc::new(model),
            context_params,
            loras: Mutex::new(HashMap::new()),
            model_id,
        })
    }

    /// Generate text from a prompt.
    pub fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        stop: &[&str],
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let start = std::time::Instant::now();

        let mut ctx = self.model.new_context(&self.backend, self.context_params.clone())
            .map_err(|e| format!("Failed to create context: {e}"))?;

        // Apply active LoRAs
        {
            let loras = self.loras.lock().unwrap();
            for wrapper in loras.values() {
                let mut adapter = wrapper.0.lock().unwrap();
                ctx.lora_adapter_set(&mut *adapter, 1.0)
                    .map_err(|e| format!("Failed to set LoRA: {e}"))?;
            }
        }

        // Tokenize prompt
        let tokens_list = self.model.str_to_token(prompt, AddBos::Always)
            .map_err(|e| format!("Tokenize failed: {e}"))?;

        let prompt_len = tokens_list.len();
        let mut batch = LlamaBatch::new(512, 1);
        let last_index = tokens_list.len().saturating_sub(1) as i32;
        for (i, token) in tokens_list.into_iter().enumerate() {
            let is_last = i as i32 == last_index;
            batch.add(token, i as i32, &[0], is_last)
                .map_err(|e| format!("Batch add failed: {e}"))?;
        }

        ctx.decode(&mut batch).map_err(|e| format!("Prefill decode failed: {e}"))?;

        let mut output = String::new();
        let mut n_cur = batch.n_tokens();
        let mut n_decode = 0;

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(temperature.max(0.01)),
            LlamaSampler::greedy(),
        ]);

        while n_decode < max_tokens {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let token_str = self.model.token_to_str(token, Special::Tokenize)
                .unwrap_or_default();
            output.push_str(&token_str);

            // Check stop sequences
            if stop.iter().any(|s| output.contains(s)) {
                break;
            }

            batch.clear();
            batch.add(token, n_cur, &[0], true)
                .map_err(|e| format!("Batch add failed: {e}"))?;

            n_cur += 1;
            n_decode += 1;

            ctx.decode(&mut batch).map_err(|e| format!("Decode failed: {e}"))?;
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
    pub fn load_lora_adapter(&self, id: &str, path: &str, _scale: f32) -> Result<(), String> {
        let log = runtime::logger("llamacpp");
        let adapter = self.model.lora_adapter_init(PathBuf::from(path))
            .map_err(|e| format!("LoRA init failed: {e}"))?;
        self.loras.lock().unwrap().insert(id.to_string(), Arc::new(LoraWrapper(Mutex::new(adapter))));
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
