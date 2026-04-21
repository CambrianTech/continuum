//! llama.cpp backend — wraps our owned `llama` crate.
//!
//! The `llama` crate vendors llama.cpp source and builds it via cmake with
//! platform-specific features (metal/cuda). This backend is the adapter
//! between Continuum's TextGenerationRequest pipeline and the safe Rust API.
//!
//! Architecture: ONE shared `Context` driven by a continuous-batching
//! scheduler (see `llamacpp_scheduler`). Each `generate()` call enqueues a
//! request that becomes one sequence (`seq_id`) in the shared batch.
//! Multiple sequences advance per decode step — weights are read once,
//! used for N sequences at once. This replaces the prior per-call-context
//! design where N concurrent generations created N independent contexts
//! that fought for memory bandwidth (each got ~1/N throughput).
//!
//! Measured 67.8 tok/s on M5 Metal with forged Qwen3.5 Q4_K_M (single seq
//! bench). With continuous batching across 3 seqs the per-stream cost
//! drops only ~10% rather than dividing by 3.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use llama::{FlashAttn, KvCacheType, LoraAdapter, Model, ModelParams};

use super::SamplingConfig;
use super::llamacpp_scheduler::{GenerationRequest, Scheduler, SchedulerConfig, TokenEvent};
use crate::runtime;

/// Configuration for loading a model.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file
    pub model_path: PathBuf,
    /// Per-sequence context budget (tokens). `None` = use the model's
    /// trained `n_ctx_train` from GGUF metadata (the model's own ceiling).
    /// Override only when memory pressure forces a smaller window than the
    /// model natively supports — and pass it explicitly so the choice is
    /// visible. Hardcoded defaults like 8192 cap a 262144-context model
    /// at 3% of its real capability.
    ///
    /// The actual `n_ctx` passed to llama.cpp is `context_length * n_seq_max`
    /// because llama.cpp's KV cache is a single shared pool across sequences
    /// — if N seqs each hold P tokens, total KV needed is N*P.
    pub context_length: Option<u32>,
    /// Batch size for prefill / per-decode token cap. Larger = faster
    /// prefill but more Metal compute buffer.
    pub n_batch: u32,
    /// GPU layers to offload (-1 = all)
    pub n_gpu_layers: i32,
    /// Maximum concurrent sequences in the shared context. Each persona
    /// inflight occupies one seq_id (0..n_seq_max). Scaled by RAM in the
    /// caller (CandleAdapter) and matched by the TS InferenceCoordinator.
    pub n_seq_max: u32,
    /// Flash attention. `Auto` lets llama.cpp pick per-backend (Metal: ON
    /// for supported head dims). Default Auto is the right call.
    pub flash_attn: FlashAttn,
    /// KV cache K element type. F16 = lossless. Q8_0 halves K memory.
    pub type_k: KvCacheType,
    /// KV cache V element type. V is more sensitive than K — keep F16
    /// unless RAM is tight enough to need Q8_0.
    pub type_v: KvCacheType,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            // None = derive from the model's GGUF metadata at load time
            // via `Model::n_ctx_train()`. The model is the source of truth
            // for its own context. Setting Some(N) here overrides only when
            // a hardware tier can't allocate KV for the model's native
            // window (rare on M5+/RTX class).
            context_length: None,
            n_batch: 512,
            n_gpu_layers: -1,
            // 3 = M5 Pro tier (48GB+). CandleAdapter overrides per-RAM.
            n_seq_max: 3,
            flash_attn: FlashAttn::Auto,
            // F16/F16 measured fastest for single-token decode on M5 Pro.
            // K=Q8_0 was slower (44 vs 47.5 tok/s) due to per-token dequant
            // overhead. Q8_0 only pays off when KV memory pressure is the
            // bottleneck (very long contexts or many parallel sequences).
            type_k: KvCacheType::F16,
            type_v: KvCacheType::F16,
        }
    }
}

/// The backend: owns a `Model` plus a continuous-batching `Scheduler`.
///
/// Models are Send+Sync (read-only after load). The scheduler owns the
/// shared Context (n_seq_max sequences, single OS-thread driver loop)
/// and is the only place llama.cpp decode happens. Generate calls
/// enqueue requests and stream tokens back over channels — there's no
/// per-call context creation, no Rust-side semaphore.
///
/// LoRAs are loaded into `loras` for genome paging, but per-request LoRA
/// activation is a v2 follow-up — the scheduler currently ignores
/// `active_loras` (see `llamacpp_scheduler` module docs).
pub struct LlamaCppBackend {
    model: Arc<Model>,
    config: LlamaCppConfig,
    model_id: String,
    /// Lazy-spawned scheduler. Lives behind OnceLock because spawning
    /// touches the Model Arc and we want a single instance per backend.
    scheduler: OnceLock<Scheduler>,
    /// Loaded LoRA adapters. Field order matters: `model` is declared
    /// BEFORE `loras` and drops AFTER it (Rust drops fields in declaration
    /// order, top-down; therefore `loras` drops first), upholding the
    /// "adapter must not outlive model" invariant.
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
            return Err(format!(
                "Model file not found: {}",
                config.model_path.display()
            ));
        }
        let model_id = config
            .model_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let load_start = Instant::now();
        let model = Model::load(
            &config.model_path,
            ModelParams {
                n_gpu_layers: config.n_gpu_layers,
                use_mmap: true,
            },
        )?;
        log.info(&format!(
            "Loaded {} in {:.2}s (vocab={})",
            model_id,
            load_start.elapsed().as_secs_f64(),
            model.n_vocab()
        ));

        Ok(Self {
            model: Arc::new(model),
            config,
            model_id,
            scheduler: OnceLock::new(),
            loras: Mutex::new(HashMap::new()),
        })
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Model's trained context length, straight from the GGUF metadata.
    /// Single source of truth — never hardcode a context window in
    /// adapters or RAG budgeters; ask this.
    pub fn n_ctx_train(&self) -> u32 {
        self.model.n_ctx_train()
    }

    /// Model's embedded chat template (Jinja-style string). Used by
    /// adapters to render messages through `llama::render_chat`. None
    /// means the model carries no template — caller decides what to do
    /// (error, default, etc.) instead of a silent fallback.
    pub fn model_chat_template(&self) -> Option<String> {
        self.model.chat_template()
    }

    /// Ensure a LoRA adapter is loaded (idempotent). Used by genome paging.
    pub fn ensure_adapter(&self, id: &str, path: &Path) -> Result<(), String> {
        let mut guard = self
            .loras
            .lock()
            .map_err(|e| format!("LoRA lock poisoned: {e}"))?;
        if guard.contains_key(id) {
            return Ok(());
        }
        let adapter = self.model.load_lora(path)?;
        guard.insert(id.to_string(), adapter);
        Ok(())
    }

    /// Remove a LoRA adapter from the cache.
    pub fn remove_adapter(&self, id: &str) -> Result<(), String> {
        let mut guard = self
            .loras
            .lock()
            .map_err(|e| format!("LoRA lock poisoned: {e}"))?;
        guard.remove(id);
        Ok(())
    }

    /// Lazily spawn (or get) the scheduler. Single instance per backend —
    /// owns the shared Context and the OS-thread driver loop.
    fn scheduler(&self) -> &Scheduler {
        self.scheduler.get_or_init(|| {
            // Per-sequence context: the model's own training ceiling unless
            // an explicit override is set. The model is the source of truth
            // — qwen3.5-4b-code-forged carries n_ctx_train=262144 in its
            // GGUF metadata; capping that at a hardcoded 8192 wastes 32×
            // the model's real capability.
            let per_seq = self
                .config
                .context_length
                .unwrap_or_else(|| self.model.n_ctx_train());
            // n_ctx is the SHARED KV pool across all sequences. Scale by
            // n_seq_max so each seq has `per_seq` tokens of KV headroom
            // even when all slots are occupied with RAG-heavy prompts.
            // saturating_mul because 262144 × 3 overflows u32 (would be
            // 786432, fine, but n_seq_max could grow).
            let total_n_ctx = per_seq.saturating_mul(self.config.n_seq_max.max(1));
            Scheduler::spawn(
                self.model.clone(),
                SchedulerConfig {
                    n_ctx: total_n_ctx,
                    n_batch: self.config.n_batch,
                    n_seq_max: self.config.n_seq_max,
                    flash_attn: self.config.flash_attn,
                    type_k: self.config.type_k,
                    type_v: self.config.type_v,
                },
            )
        })
    }

    /// Generate text. Routes through the continuous-batching scheduler:
    /// the request becomes one sequence in the shared batch and tokens
    /// stream back as they're sampled. Multiple concurrent generate()
    /// calls share the single Context — weights are read once per decode
    /// step and used to advance every active sequence in parallel.
    ///
    /// `active_loras` is currently a no-op (scheduler v1 limitation —
    /// per-seq LoRA activation is a follow-up). Adapters are still
    /// loaded into the cache via `ensure_adapter` so the API is stable
    /// for when v2 lands.
    pub fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        active_loras: &[(String, f32)],
    ) -> Result<(String, usize), String> {
        // Forwards to the persona-aware variant with persona_id=None so
        // test rigs and ad-hoc probes don't need to change. Production
        // adapter calls go through generate_for_persona() so the registry
        // can attribute KV bytes per-persona.
        self.generate_for_persona(
            None,
            prompt,
            max_tokens,
            sampling,
            stop_sequences,
            active_loras,
        )
    }

    /// Same as `generate` but threads a `persona_id` through to the
    /// scheduler so the registry can attribute the seq slot's KV bytes
    /// to the right persona. Pass `None` for test/ad-hoc paths that
    /// shouldn't appear in per-persona accounting.
    ///
    /// `persona_id` is forwarded as-is into `ActiveSeq::persona_id`. The
    /// actual registry reporting (Piece 2 of the substrate work) hooks
    /// into seq alloc / Done events inside the scheduler — this method's
    /// only job here is to deliver the value.
    pub fn generate_for_persona(
        &self,
        persona_id: Option<uuid::Uuid>,
        prompt: &str,
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        active_loras: &[(String, f32)],
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let gen_start = Instant::now();
        let prompt_len_chars = prompt.len();

        // Channel for streaming tokens back from the scheduler.
        let (response_tx, mut response_rx) = tokio::sync::mpsc::unbounded_channel::<TokenEvent>();

        // Caller passes the full SamplingConfig (the value-object pattern
        // — adding fields like `grammar` doesn't require changing this
        // signature). Previously this path silently overwrote the caller's
        // top_k/top_p/repeat_penalty fields with no-op defaults.
        let req = GenerationRequest {
            prompt: prompt.to_string(),
            max_tokens,
            sampling,
            stop_sequences: stop_sequences.iter().map(|s| s.to_string()).collect(),
            active_loras: active_loras.to_vec(),
            response_tx,
            persona_id,
        };

        self.scheduler().enqueue(req)?;

        // Collect tokens from the channel until Done/Error. We're called
        // synchronously from spawn_blocking by CandleAdapter — block_in_place
        // a tokio runtime handle to await the channel.
        //
        // Stop-sequence trimming happens here (at the boundary): the
        // scheduler emits the stop sequence's tokens before signaling Done,
        // so we strip them from the collected output.
        let mut output = String::new();
        let mut n_decoded = 0usize;
        let runtime_handle = tokio::runtime::Handle::try_current().ok();

        loop {
            let event = if let Some(ref h) = runtime_handle {
                h.block_on(response_rx.recv())
            } else {
                // Fallback for non-tokio callers (e.g. tests). Spin briefly.
                let mut tries = 0u32;
                loop {
                    match response_rx.try_recv() {
                        Ok(e) => break Some(e),
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                            break None;
                        }
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                            tries += 1;
                            if tries > 60_000 {
                                // 2 minutes without a token — treat as fatal.
                                break None;
                            }
                        }
                    }
                }
            };
            match event {
                Some(TokenEvent::Token(piece)) => {
                    output.push_str(&piece);
                    n_decoded += 1;
                }
                Some(TokenEvent::Done {
                    tokens_generated,
                    elapsed_ms,
                }) => {
                    n_decoded = tokens_generated;
                    let elapsed = gen_start.elapsed();
                    log.info(&format!(
                        "Generated {} tokens in {:.3}s ({:.1} tok/s, scheduler={}ms, prompt={}chars)",
                        n_decoded,
                        elapsed.as_secs_f64(),
                        n_decoded as f64 / elapsed.as_secs_f64().max(0.001),
                        elapsed_ms,
                        prompt_len_chars
                    ));
                    // Trim trailing stop sequence(s) — scheduler emits them
                    // before signaling Done.
                    for s in stop_sequences {
                        if output.ends_with(s) {
                            output.truncate(output.len() - s.len());
                        }
                    }
                    return Ok((output, n_decoded));
                }
                Some(TokenEvent::Error(e)) => {
                    return Err(format!("scheduler error: {e}"));
                }
                None => {
                    return Err("scheduler closed without Done event".to_string());
                }
            }
        }
    }
}
