//! llama.cpp backend — wraps our owned `llama` crate.
//!
//! The `llama` crate vendors llama.cpp source and builds it via cmake with
//! platform-specific features (metal/cuda). This backend is the adapter
//! between Continuum's TextGenerationRequest pipeline and the safe Rust API.
//!
//! Measured 67.8 tok/s on M5 Metal with forged Qwen3.5 Q4_K_M.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use llama::{Batch, Context, ContextParams, LoraAdapter, Model, ModelParams, Sampler};

use crate::runtime;

/// Configuration for loading a model.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file
    pub model_path: PathBuf,
    /// Context length
    pub context_length: u32,
    /// Batch size for prefill
    pub n_batch: u32,
    /// GPU layers to offload (-1 = all)
    pub n_gpu_layers: i32,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            // 2048 keeps KV cache ~half of 4096 (~1.2GB saved on Qwen3.5-4B
            // Q4_K_M's hybrid attention/recurrence layers). On memory-tight
            // machines (M1 Pro 32GB with 14 personas + Postgres + RAG cache)
            // a 4096 context can push the system into Metal alloc failures
            // that surface as `llama_decode rc=1`. RAG budgets fit fine in
            // 2048 — average chat prompt is 500-1500 tokens.
            context_length: 2048,
            n_batch: 512,
            n_gpu_layers: -1,
        }
    }
}

/// The backend: owns a `Model`, creates Contexts per-inference, manages LoRA adapters.
///
/// Models are Send+Sync (read-only after load). Contexts are created per generation
/// call — cheap, avoids state sharing. LoRAs are hot-swapped on each context at
/// generation time for genome paging.
pub struct LlamaCppBackend {
    model: Arc<Model>,
    config: LlamaCppConfig,
    model_id: String,
    /// Loaded LoRA adapters keyed by caller-chosen id.
    /// We store them in a Mutex<HashMap> so ensure_adapter/remove_adapter
    /// can add/remove at runtime for genome paging.
    /// `LoraAdapter` has no lifetime parameter by design — see the llama
    /// crate docs. The invariant "adapter must not outlive model" is held
    /// here because `model: Arc<Model>` is declared BEFORE `loras` and
    /// therefore drops AFTER (Rust drops struct fields in declaration
    /// order; `loras` drops first, the model lives to the end).
    loras: Mutex<HashMap<String, LoraAdapter>>,
}

// SAFETY: Model is Send+Sync (llama.cpp models are immutable after load).
// LoraAdapter is Send+Sync per the llama crate's impl. The Mutex handles
// concurrent modification to the map.
unsafe impl Send for LlamaCppBackend {}
unsafe impl Sync for LlamaCppBackend {}

impl LlamaCppBackend {
    /// Load a GGUF model.
    pub fn load(config: LlamaCppConfig) -> Result<Self, String> {
        let log = runtime::logger("llamacpp");
        if !config.model_path.exists() {
            return Err(format!("Model file not found: {}", config.model_path.display()));
        }
        let model_id = config.model_path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let load_start = Instant::now();
        let model = Model::load(
            &config.model_path,
            ModelParams { n_gpu_layers: config.n_gpu_layers, use_mmap: true },
        )?;
        log.info(&format!(
            "Loaded {} in {:.2}s (vocab={})",
            model_id, load_start.elapsed().as_secs_f64(), model.n_vocab()
        ));

        Ok(Self {
            model: Arc::new(model),
            config,
            model_id,
            loras: Mutex::new(HashMap::new()),
        })
    }

    pub fn model_id(&self) -> &str { &self.model_id }

    /// Ensure a LoRA adapter is loaded (idempotent). Used by genome paging.
    pub fn ensure_adapter(&self, id: &str, path: &Path) -> Result<(), String> {
        let mut guard = self.loras.lock().map_err(|e| format!("LoRA lock poisoned: {e}"))?;
        if guard.contains_key(id) { return Ok(()); }
        let adapter = self.model.load_lora(path)?;
        guard.insert(id.to_string(), adapter);
        Ok(())
    }

    /// Remove a LoRA adapter from the cache.
    pub fn remove_adapter(&self, id: &str) -> Result<(), String> {
        let mut guard = self.loras.lock().map_err(|e| format!("LoRA lock poisoned: {e}"))?;
        guard.remove(id);
        Ok(())
    }

    /// Generate text. `active_loras` selects which loaded adapters apply at what scale.
    pub fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        stop_sequences: &[&str],
        active_loras: &[(String, f32)],
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let gen_start = Instant::now();

        let mut ctx = self.model.new_context(ContextParams {
            n_ctx: self.config.context_length,
            n_batch: self.config.n_batch,
            n_seq_max: 1,  // chat personas use sequence 0 only
        })?;

        // Apply LoRAs (hot-swap per generation — genome paging primitive)
        if !active_loras.is_empty() {
            let guard = self.loras.lock().map_err(|e| format!("LoRA lock: {e}"))?;
            let mut refs: Vec<(&LoraAdapter, f32)> = Vec::new();
            for (id, scale) in active_loras {
                match guard.get(id) {
                    Some(adapter) => refs.push((adapter, *scale)),
                    None => return Err(format!("LoRA adapter not loaded: {id}")),
                }
            }
            ctx.set_loras(&refs)?;
        }

        // Tokenize + prefill
        let prompt_tokens = self.model.tokenize(prompt, true, false)?;
        let prompt_len = prompt_tokens.len();

        // Chunk the prefill into n_batch-sized decode calls. Prompts longer
        // than the batch size (common with RAG — 5k+ token contexts) must be
        // fed to llama.cpp in pieces, each within the allocated batch capacity.
        // Without chunking, push overflows the llama-allocated arrays and
        // crashes in memmove.
        let n_batch = self.config.n_batch as usize;
        let mut batch = Batch::allocated(n_batch as i32, 1);
        let total = prompt_tokens.len();
        let last_idx = total - 1;
        let mut chunk_start = 0;
        while chunk_start < total {
            let chunk_end = (chunk_start + n_batch).min(total);
            batch.clear();
            for i in chunk_start..chunk_end {
                // Request logits only for the final prompt token; all
                // intermediate tokens are just context to build up the KV.
                batch.push(prompt_tokens[i], i as i32, &[0], i == last_idx);
            }
            ctx.decode(&batch)?;
            chunk_start = chunk_end;
        }

        // Sampling: greedy if temp<=0, else temperature sampling
        let mut sampler = if temperature <= 0.0 {
            Sampler::greedy()
        } else {
            Sampler::chain()
                .temp(temperature)
                .dist(42)
                .build()
        };

        let mut output = String::new();
        let mut n_decoded = 0;
        // n_cur is the absolute KV position for the next token. After
        // prefill we have `total` tokens in context, so the next position
        // is `total`. Using batch.n_tokens() would be wrong for chunked
        // prefill — it only holds the size of the last chunk.
        let mut n_cur = total as i32;

        for _ in 0..max_tokens {
            let token = sampler.sample(&ctx, -1);
            sampler.accept(token);
            if self.model.is_eog_token(token) { break; }

            let piece = self.model.token_to_piece(token);
            output.push_str(&piece);

            // Stop sequence check
            if stop_sequences.iter().any(|s| output.ends_with(s)) {
                // Trim the stop sequence
                for s in stop_sequences {
                    if output.ends_with(s) {
                        output.truncate(output.len() - s.len());
                    }
                }
                break;
            }

            batch.clear();
            batch.push(token, n_cur, &[0], true);
            ctx.decode(&batch)?;

            n_cur += 1;
            n_decoded += 1;
        }

        let elapsed = gen_start.elapsed();
        let tok_s = n_decoded as f64 / elapsed.as_secs_f64();
        log.info(&format!(
            "Generated {} tokens in {:.3}s ({:.1} tok/s, prompt={}tok)",
            n_decoded, elapsed.as_secs_f64(), tok_s, prompt_len
        ));

        Ok((output, n_decoded))
    }
}
