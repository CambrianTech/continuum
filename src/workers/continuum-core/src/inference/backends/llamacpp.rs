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
    /// Optional path to the multimodal projector GGUF (mmproj). When
    /// present, the backend lazily loads an `MtmdContext` and exposes
    /// `generate_with_image()` so vision-capable models can receive raw
    /// image bytes natively. None = text-only model (the common case);
    /// `generate_with_image()` returns an error.
    pub mmproj_path: Option<PathBuf>,
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
            mmproj_path: None,
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
    /// Lazy-loaded multimodal projector. Built on first `generate_with_image`
    /// call from `config.mmproj_path` (so text-only backends pay zero cost).
    /// Sits behind a Mutex<Option<...>> so concurrent first-call requests
    /// don't double-load. None until first use OR if `mmproj_path` is unset.
    mtmd: Mutex<Option<Arc<llama::MtmdContext>>>,
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
            mtmd: Mutex::new(None),
            loras: Mutex::new(HashMap::new()),
        })
    }

    /// Lazily load the multimodal projector. Returns Err when
    /// `config.mmproj_path` is None (text-only backend) or when the
    /// mmproj file fails to load. Idempotent — caches the loaded
    /// MtmdContext under the mutex.
    fn ensure_mtmd(&self) -> Result<Arc<llama::MtmdContext>, String> {
        let mut guard = self
            .mtmd
            .lock()
            .map_err(|e| format!("mtmd lock poisoned: {e}"))?;
        if let Some(existing) = guard.as_ref() {
            return Ok(existing.clone());
        }
        let mmproj = self.config.mmproj_path.as_ref().ok_or_else(|| {
            format!(
                "model {} has no mmproj configured — text-only backend can't process images. \
                 Set `mmproj_local_path` in models.toml AND declare Capability::Vision.",
                self.model_id
            )
        })?;
        if !mmproj.exists() {
            return Err(format!(
                "mmproj file declared but missing on disk: {} (model: {})",
                mmproj.display(),
                self.model_id
            ));
        }
        let ctx = llama::MtmdContext::from_file(mmproj, &self.model)
            .map_err(|e| format!("MtmdContext::from_file failed for {}: {e}", mmproj.display()))?;
        let arc = Arc::new(ctx);
        *guard = Some(arc.clone());
        Ok(arc)
    }

    /// Single-shot multimodal generation: text prompt + one image →
    /// generated text. Bypasses the continuous-batching scheduler
    /// because image encoding produces tokens that aren't trivially
    /// batchable with concurrent text seqs (image tokens have a
    /// fixed positional layout dictated by the projector). Opens a
    /// fresh per-call llama_context, evaluates the image+text via
    /// `MtmdContext::eval_image`, then samples until EOG / max_tokens
    /// / stop sequence. Concurrent multimodal calls each get their
    /// own context — slower than batched but isolated and correct.
    ///
    /// `prompt_with_marker` MUST contain the model's media marker
    /// (see `llama::MtmdContext::default_marker()`, typically
    /// `<__media__>`) — that's where the image tokens splice in. If
    /// the caller's text doesn't include it, `mtmd_tokenize` returns
    /// an error and we surface it.
    pub fn generate_with_image(
        &self,
        prompt_with_marker: &str,
        image_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
    ) -> Result<(String, usize), String> {
        self.generate_with_media(
            prompt_with_marker,
            image_bytes,
            max_tokens,
            sampling,
            stop_sequences,
            llama::MediaKind::Image,
        )
    }

    /// Audio analogue of `generate_with_image`. Same single-shot
    /// per-call-context pattern; the mtmd projector path inside auto-
    /// detects audio vs image from the bytes' magic numbers but the
    /// caller's `MediaKind::Audio` selects the capability check
    /// (`supports_audio` instead of `supports_vision`) and shapes error
    /// messages so a mistakenly-routed audio call doesn't surface as a
    /// confusing "vision unsupported" error.
    ///
    /// Supported audio container formats are whatever miniaudio
    /// understands inside the vendored llama.cpp build (wav, mp3, flac
    /// per upstream `tools/mtmd/mtmd-helper.h`). The caller is expected
    /// to deliver one of those — re-encoding from other formats is a
    /// sensory-bridge concern, not the backend's.
    pub fn generate_with_audio(
        &self,
        prompt_with_marker: &str,
        audio_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
    ) -> Result<(String, usize), String> {
        self.generate_with_media(
            prompt_with_marker,
            audio_bytes,
            max_tokens,
            sampling,
            stop_sequences,
            llama::MediaKind::Audio,
        )
    }

    /// Internal workhorse for single-shot multimodal generation. Mirrors
    /// the eval+sample loop the public methods need; the only thing that
    /// differs per modality is the capability check (vision vs audio
    /// projector support) and which `MtmdContext::eval_*` method runs.
    /// Centralizing here avoids the 150-LOC duplication that would land
    /// if image and audio paths were copy-pasted.
    fn generate_with_media(
        &self,
        prompt_with_marker: &str,
        media_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        kind: llama::MediaKind,
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let start = Instant::now();
        let mtmd = self.ensure_mtmd()?;
        match kind {
            llama::MediaKind::Image => {
                if !mtmd.supports_vision() {
                    return Err(format!(
                        "model {}'s mmproj does not declare vision support — \
                         caller passed an image but the projector is text-only or audio-only",
                        self.model_id
                    ));
                }
            }
            llama::MediaKind::Audio => {
                if !mtmd.supports_audio() {
                    return Err(format!(
                        "model {}'s mmproj does not declare audio support — \
                         caller passed audio but the projector is text-only or vision-only",
                        self.model_id
                    ));
                }
            }
        }

        // Per-call context — see method-level docstring on why we don't
        // share the scheduler's context.
        let per_seq = self
            .config
            .context_length
            .unwrap_or_else(|| self.model.n_ctx_train());
        let mut ctx = self
            .model
            .new_context(llama::ContextParams {
                n_ctx: per_seq,
                n_batch: self.config.n_batch,
                n_seq_max: 1,
                flash_attn: self.config.flash_attn,
                type_k: self.config.type_k,
                type_v: self.config.type_v,
            })
            .map_err(|e| format!("new_context failed: {e}"))?;

        // Eval text + media into the context, advancing n_past.
        let eval_result = match kind {
            llama::MediaKind::Image => mtmd.eval_image(
                &mut ctx,
                prompt_with_marker,
                media_bytes,
                0,
                self.config.n_batch as i32,
                0,
                true,
            ),
            llama::MediaKind::Audio => mtmd.eval_audio(
                &mut ctx,
                prompt_with_marker,
                media_bytes,
                0,
                self.config.n_batch as i32,
                0,
                true,
            ),
        };
        let n_past = eval_result.map_err(|e| format!("mtmd eval ({:?}) failed: {e}", kind))?;
        log.info(&format!(
            "mtmd eval done ({:?}): prompt+media consumed {} positions in {}ms",
            kind,
            n_past,
            start.elapsed().as_millis()
        ));

        // Sample-until-done loop. Mirrors LlamaCppBackend::generate but
        // single-seq, no scheduler. EOG / max_tokens / stop-sequence are
        // the three exit conditions, same shape.
        let mut sampler = if sampling.temperature <= 0.0 && sampling.grammar.is_none() {
            llama::Sampler::greedy()
        } else {
            let mut chain = llama::Sampler::chain();
            if let Some(g) = sampling.grammar.as_ref() {
                chain = chain.grammar(&self.model, g, "root");
            }
            if sampling.top_k > 0 {
                chain = chain.top_k(sampling.top_k as i32);
            }
            if sampling.top_p > 0.0 && sampling.top_p < 1.0 {
                chain = chain.top_p(sampling.top_p as f32, 1);
            }
            chain = chain.penalties(64, sampling.repeat_penalty, 0.0, 0.0);
            let temp = if sampling.temperature > 0.0 {
                sampling.temperature as f32
            } else {
                0.01
            };
            chain.temp(temp).dist(42).build()
        };

        // Diagnostic: dump top-10 logits at the post-image position when
        // MTMD_DEBUG_LOGITS is set. Used during the 2026-04-21 hunt for
        // why our logits diverged from brew's mtmd-cli on the same
        // model+image+prompt; kept env-gated so future bug hunts have a
        // ready-to-fire probe instead of needing to re-derive it.
        if std::env::var_os("MTMD_DEBUG_LOGITS").is_some() {
            let logits = ctx.logits_ith(-1);
            if logits.is_empty() {
                eprintln!("[gen-with-img] WARN: logits_ith(-1) returned empty");
            } else {
                let mut indexed: Vec<(usize, f32)> =
                    logits.iter().copied().enumerate().collect();
                indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                eprintln!("[gen-with-img] top-10 logits at post-image position:");
                for (id, score) in indexed.iter().take(10) {
                    let piece = self.model.token_to_piece(*id as i32);
                    eprintln!("  id={:>6} score={:.4} piece={:?}", id, score, piece);
                }
            }
        }

        let mut output = String::new();
        let mut pos = n_past;
        let mut tokens_generated = 0usize;
        // Sample at -1 = "last logits in last batch" — same convention
        // brew's mtmd-cli uses (mtmd-cli.cpp:186 calls
        // common_sampler_sample(smpl, lctx, -1) right after eval). After
        // mtmd_helper_eval_chunks with logits_last=true, the final
        // text-batch's last token has logits set and llama_get_logits_ith
        // honors -1 as that position.
        loop {
            let token = sampler.sample(&ctx, -1);
            sampler.accept(token);
            if self.model.is_eog_token(token) {
                break;
            }
            let piece = self.model.token_to_piece(token);
            output.push_str(&piece);
            tokens_generated += 1;
            // Stop sequence early-exit — same end-of-output trim shape
            // as the scheduler path.
            if stop_sequences.iter().any(|s| output.ends_with(s)) {
                break;
            }
            if tokens_generated >= max_tokens {
                break;
            }
            // Push the sampled token back so the next decode can advance.
            let mut batch = llama::Batch::allocated(1, 1);
            batch.push(token, pos, &[0], true);
            if let Err(e) = ctx.decode(&batch) {
                log.warn(&format!("decode failed mid-generation: {e}"));
                break;
            }
            pos += 1;
        }

        log.info(&format!(
            "generate_with_image done: {} tokens in {}ms ({:.1} tok/s)",
            tokens_generated,
            start.elapsed().as_millis(),
            tokens_generated as f64 / start.elapsed().as_secs_f64().max(0.001)
        ));
        Ok((output, tokens_generated))
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
