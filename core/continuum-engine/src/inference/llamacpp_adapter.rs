//! `LlamaCppAdapter` — implements `AIProviderAdapter` by wrapping our
//! in-process `LlamaCppBackend` (the bundled `llama` crate, statically
//! linked against the vendored llama.cpp Metal/CUDA build).
//!
//! Why this exists:
//!
//! Docker Model Runner (DMR) ships a containerized llama-server. On Mac
//! the container's Metal toolchain has been failing to compile the
//! tensor-API source on M5/Apple10 hardware (verified 2026-04-19, log:
//! `ggml_metal_library_init_from_source: error compiling source` →
//! `has tensor = false`). Result: M5 inference at 22 tok/s — slower
//! than M1 at 27 tok/s on the same model. The cripple is in DMR's
//! container build, not in llama.cpp itself.
//!
//! This adapter bypasses DMR entirely — loads the GGUF in-process via
//! our newer vendored llama.cpp build, which compiles Metal correctly
//! against the host toolchain. Empirical win: 33 tok/s vs DMR's 22 tok/s
//! on the same hardware (50% improvement, smoke test in
//! `tests/llamacpp_metal_throughput.rs`).
//!
//! Other wins from owning the inference call directly:
//! - No HTTP hop (in-process call vs localhost roundtrip)
//! - Full control of `n_gpu_layers`, batch sizes, sampling
//! - Direct access to LoRA hot-swap via `LlamaCppBackend::ensure_adapter`
//! - Metal command-buffer timing available for real GPU-utilization
//!   metrics (planned follow-up — addresses "we can't even see what
//!   percent GPU was used" observability gap)
//!
//! Coexistence with DMR adapter:
//! - Both registered. This adapter gets HIGHER priority (lower number)
//!   so local Mac inference flows here first.
//! - DMR remains the fallback for: cases where in-process load fails,
//!   non-Mac platforms, or operators who prefer the container path.

use crate::ai::adapter::{AIProviderAdapter, AdapterCapabilities, ApiStyle, InferenceDevice};
use crate::ai::registry_bridge::models_for_provider_via_registry;
use crate::ai::types::{
    EmbeddingInput, EmbeddingRequest, EmbeddingResponse, FinishReason, HealthState, HealthStatus,
    MessageContent, ModelInfo, ResponseFormat, TextGenerationRequest, TextGenerationResponse,
    UsageMetrics,
};
use crate::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use crate::inference::backends::{SamplingConfig, JSON_GRAMMAR};
use crate::inference_capability::enforce_residency;
use crate::model_registry::Capability;
use crate::runtime;
use async_trait::async_trait;
use llama::FlashAttn;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Provider ID for this adapter. Routing checks for this when the caller
/// asks for `provider="local"` (per `AdapterRegistry::select`'s
/// "local" → device-filtered local-GPU selection logic).
pub const LLAMACPP_PROVIDER_ID: &str = "llamacpp-local";

/// The embed lane's identity on the resource board — the `consumer_id` on both
/// its per-call VRAM leases and its standing reservation floor.
pub const EMBED_LANE_CONSUMER_ID: &str = "embed";

/// The embed lane's working VRAM: the bounded Metal embedding context (~1.2 GiB
/// compute buffer + ~224 MiB KV) plus headroom. ONE constant, two uses: the
/// per-call lease in [`create_embedding`], and the standing FLOOR the embedder
/// reserves on the board at resolve so serving's plan can never grow into it
/// (#225 — measured 2026-08-08: the grown window left 604 MiB governed-available
/// against this 1792 MiB need, and embedding went fully dead on the box).
pub const EMBED_LANE_VRAM_BYTES: u64 = 1792 * 1024 * 1024;

/// Overlay live runtime metadata (throughput) on top of the registry's
/// declared ModelInfo. Context-window still flows from `backend.n_ctx_train()`
/// because that's the GGUF's ground truth — the catalog value is the intent,
/// the GGUF metadata is what the runtime actually loaded. If they drift,
/// we trust the model, not the config.
fn model_info_with_runtime(
    mut info: ModelInfo,
    backend: &LlamaCppBackend,
    last_tok_per_s: f64,
) -> ModelInfo {
    let n_ctx = backend.n_ctx_train();
    info.context_window = n_ctx;
    // Same reasoning as elsewhere: the model can decode up to its full
    // context. Callers that want a smaller window declare it per-request;
    // the adapter never invents its own MAX.
    info.max_output_tokens = n_ctx;
    info.tokens_per_second = last_tok_per_s as f32;
    info
}

mod construction;
pub use construction::LlamaCppLaunch;
mod payload;
#[cfg(test)]
mod tests;

use payload::{decode_audio_bytes, decode_image_bytes, sampling_config_from_request};

/// Typed failure for [`LlamaCppAdapter::try_new`] when the model
/// registry has no `llamacpp-local` row with a resolved
/// `gguf_local_path`. Surfaces install-time-no-Qwen state as observable
/// runtime health rather than a process panic. Operators see this in
/// install/health output and know exactly what's missing.
///
/// 2026-05-11: continuum-8e97 RTX 5090 finding showed cuda stack ready,
/// VRAM available, zero personas replying — root cause was no Qwen
/// GGUF seeded by carl install. Without this typed error the silent
/// state was indistinguishable from "personas just slow."
#[derive(Debug, thiserror::Error)]
#[error(
    "no `{provider_id}` model with `gguf_local_path` resolved on disk \
     ({rows_in_registry} provider rows, {rows_with_gguf_local_path} with \
     a path on disk). Install seeded no local Qwen GGUF — run model-init \
     downloader or seed manually."
)]
pub struct NoLocalModelLoadable {
    pub provider_id: String,
    pub rows_in_registry: usize,
    pub rows_with_gguf_local_path: usize,
}

/// In-process llama.cpp adapter. Lazy-loads the model on first
/// `generate_text` call (so adapter registration doesn't pay the
/// 5-10s model-load cost up front). After load, the backend lives for
/// the process lifetime in an `Arc` for concurrent generations across
/// personas.
pub struct LlamaCppAdapter {
    backend: Arc<RwLock<Option<Arc<LlamaCppBackend>>>>,
    model_path: PathBuf,
    last_throughput_tok_s: Arc<RwLock<f64>>,
    /// The model id this adapter serves. Resolved from the registry at
    /// construction — whichever llamacpp-local model row has a
    /// `gguf_local_path` pointing at an on-disk file, we claim that id.
    /// Held as `String` so `default_model()` can return `&str`.
    default_model: String,
    /// Per-sequence context budget override. None = honor the model's
    /// declared `n_ctx_train` (e.g. qwen3.5-4b's 262144). Set this
    /// explicitly when memory pressure / hardware tier forces a smaller
    /// window — the KV cache scales linearly with context_length, and a
    /// 262K alloc on qwen3.5-4b is ~24GB even at Q4. Tests use 16K;
    /// production tier-aware sizing is a follow-up (M5 Pro = 64K? or
    /// per-persona declaration).
    context_length_override: Option<u32>,
    /// Per-residency KV quant policy. Controls type_k / type_v at each
    /// lifecycle stage (Active hot in GPU, CpuResident warm in unified
    /// memory, Idle spilled to NVMe). Default = `KvQuantPolicy::default()`
    /// (F16/F16 active, Q8_0/F16 resident, Q8_0/Q8_0 spilled). Caller
    /// overrides via `with_kv_quant_policy()` per recipe / hardware tier.
    /// Currently only `active` is consumed at backend load time;
    /// CpuResident and Idle land with the paging substrate (Phase 3.x).
    /// See docs/architecture/PERSONA-CONTEXT-PAGING.md §16.
    kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy,
    /// Continuous-batching slot count. `None` = single-seq mode
    /// (the conservative qwen3.5 default — its recurrent /
    /// Gated-Delta-Net Metal graph aborts on multi-seq). When set,
    /// the in-backend scheduler multiplexes N concurrent
    /// generations through one shared model load via
    /// `llamacpp_scheduler.rs`'s driver loop.
    ///
    /// **Coordinator wiring:** `InferenceCoordinator::open_lane`
    /// admits up to `lane_budgets.max_concurrency` lanes against
    /// this adapter; the scheduler's `n_seq_max` MUST match or
    /// exceed that number, otherwise admission lets in lanes the
    /// scheduler can't actually serve in parallel. The realistic-
    /// floor coordinator config (4 concurrent lanes) pairs with
    /// `with_n_seq_max(4)` on the adapter.
    ///
    /// **Per-model safety:** qwen3.5 (and any model with a
    /// recurrent KV layer that the Metal graph can't multiplex)
    /// must keep this at None / 1. Standard Llama / Qwen-2.5 /
    /// Gemma-2 architectures multiplex cleanly.
    ///
    /// See [`docs/architecture/INFERENCE-LANES-REALISTIC.md`]
    /// (Step 4) for the rollout plan.
    n_seq_max_override: Option<u32>,
    /// Max ubatch size override — when set, the LlamaCppConfig built at
    /// `load()` time uses this instead of the hardcoded default. The
    /// compute graph is reserved for ubatches up to this size, so
    /// setting it correctly is what avoids the
    /// `decode: failed to find a memory slot for batch of size N`
    /// panic when N exceeds the reserved graph (observed #130
    /// 2026-06-01 with RAG-built persona prompts at 337 tokens).
    /// Profile-driven via `PersonaInferenceProfile.n_ubatch`.
    n_ubatch_override: Option<u32>,
    /// GPU offload depth override. `None` = honor whatever the load
    /// path decides from tier policy. Set explicitly when the persona
    /// profile already resolved the right value (e.g., 0 on Compat
    /// tier while #131's Metal hang fix is pending).
    n_gpu_layers_override: Option<i32>,
}

#[async_trait]
impl AIProviderAdapter for LlamaCppAdapter {
    fn provider_id(&self) -> &str {
        LLAMACPP_PROVIDER_ID
    }

    fn name(&self) -> &str {
        "Llama.cpp (in-process Metal/CUDA)"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // max_context_window: if the backend has been loaded, use the
        // model's actual training ceiling; otherwise leave 0 to signal
        // "ask the model" via model_metadata. Never invent a number.
        let max_ctx = self
            .backend
            .read()
            .as_ref()
            .map(|b| b.n_ctx_train())
            .unwrap_or(0);
        // llama.cpp does text + chat + streaming, native (prompt-driven) tool
        // calls, and embeddings (--embedding mode). Vision is handled by the
        // mmproj adapter when loaded, not declared at this text-LLM layer;
        // audio is bridged via STT (whisper) / TTS in the substrate.
        // Tools are prompt-driven (no native protocol); structured output via
        // GBNF grammar-constrained sampling, which IS native to llama.cpp.
        AdapterCapabilities::builder()
            .capabilities([
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
                Capability::Embedding,
            ])
            .local()
            .context_window(max_ctx)
            .max_output_tokens(4096)
            .protocols(crate::ai::adapter::NativeProtocols::GrammarConstrained)
            .build()
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Eagerly load the model at initialize time. The previous lazy-load
        // scheme meant `model_metadata()` returned None until the first
        // `generate_text` call, which in turn made TS-side callers of
        // `ai/model-info` get back nothing → they fell through to a
        // hardcoded 8192 context-window fallback, ignoring the model's
        // actual 262144. Eager-load pays the 5-10s cost once at boot and
        // guarantees every downstream consumer sees the model's real
        // capabilities from the first query on.
        //
        // If the GGUF isn't on disk we return Ok without loading —
        // `register_adapters` has already gated registration on
        // `health_check().api_available`, so we only get called when the
        // file exists. If something changed between those two checks
        // (e.g. the file was deleted), the first `generate_text` still
        // falls back to the ensure_loaded path and surfaces a clean
        // model-not-found error then.
        if self.model_path.exists() {
            let _ = self.ensure_loaded()?;
        }
        Ok(())
    }

    async fn warmup(&self) -> Result<(), String> {
        // Tiny throwaway decode against a minimal prompt. The model
        // file is already loaded by `initialize`; this call exercises
        // the KV-cache allocation path, the attention kernels, and
        // the sampler state — so when the first real `generate_text`
        // lands, it pays only the marginal per-token cost, not the
        // cold-cache JIT bill.
        //
        // Per [[init-once-handle-then-lease-zero-copy-refs]]: the
        // adapter's hot path is leased per turn; the substrate pays
        // the init cost ONCE here, at boot, never on a user's first
        // message. On Intel Mac with Qwen 0.5B this saves ~200-500ms
        // off the first turn; on M5 Metal with a larger model the
        // save is multiples of that.
        let warmup_request = TextGenerationRequest {
            messages: vec![crate::ai::types::ChatMessage::text("user", "Hi")],
            system_prompt: None,
            model: None,
            provider: None,
            temperature: Some(0.0),
            max_tokens: Some(1),
            reasoning_budget: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tail_tools: None,
            view_messages: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: Some("warmup".to_string()),
            user_id: None,
            room_id: None,
            purpose: Some("warmup".to_string()),
            persona_id: None,
            turn_bound: None,
        };
        match self.generate_text(warmup_request).await {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("LlamaCppAdapter warmup decode failed: {e}")),
        }
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        // Drop the backend — releases GPU memory.
        *self.backend.write() = None;
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        let backend = self.ensure_loaded()?;

        // Use the model's OWN chat template (from GGUF metadata) via
        // llama.cpp's template engine. The previous hand-rolled
        // `<|im_start|>role\n ...<|im_end|>\n` prefix was wrong for
        // qwen3.5 — it caused `<|im_end<|>` special-token leakage in
        // Teacher AI output (2026-04-20). Different models use different
        // boundary tokens; the model is the source of truth.
        // Use the model's own template if embedded in GGUF metadata;
        // Resolution order, no fallback:
        //   1. GGUF metadata `tokenizer.chat_template` (forge bake should
        //      put it here).
        //   2. the Rust catalog (catalog.rs) `chat_template` field (memento's registry —
        //      authoritative when GGUF is silent).
        // No in-code constant. Adding a new model = catalog row, never an
        // adapter edit. If both sources are absent, render_chat passes
        // None to llama.cpp which is its own loud failure (chatml default
        // doesn't match qwen3.5's special tokens — output corruption).
        let registry_template: Option<String> = crate::model_registry::try_global()
            .and_then(|reg| reg.model(backend.model_id()))
            .and_then(|m| m.chat_template.clone());
        let template_string = backend.model_chat_template().or(registry_template);
        let template = template_string.as_deref();
        // Walk the request to find any image / audio content. If present,
        // the model MUST natively accept that modality (else the bridge
        // is wrong upstream — sensory-bridge converts to text BEFORE
        // reaching here for non-multimodal models). For vision-capable /
        // audio-capable local models with a loaded mmproj, media items
        // splice in as `<__media__>` markers inside the rendered text
        // and the call routes to `backend.generate_with_image()` /
        // `generate_with_audio()` instead of the scheduler.
        //
        // Single-media-per-call scope for v1: libmtmd's C API supports
        // multiple bitmaps per tokenize call (one marker each, in
        // order), but our backend signatures take one bytes blob. The
        // collected_media vector preserves order; if there's >1 item
        // OR a mix of image+audio, we hard-error rather than silently
        // dropping the rest. Multi-media is a follow-up once a real
        // caller needs it (mtmd_tokenize already does the work).
        // Diagnostic: prove what the adapter receives from the caller —
        // counts user message shapes (Text vs Parts) and ContentPart
        // variants. When vision routing breaks, this tells us whether
        // the image got dropped upstream (count=0, request had no
        // ContentPart::Image) vs in our walk (count>0 but
        // generate_with_image still doesn't fire). 2026-04-21: Vision AI
        // was producing wrong answers; this is the probe to localize.
        {
            let mut text_msgs = 0;
            let mut parts_msgs = 0;
            let mut parts_text = 0;
            let mut parts_image = 0;
            let mut parts_audio = 0;
            let mut parts_other = 0;
            for msg in &request.messages {
                match &msg.content {
                    MessageContent::Text(_) => text_msgs += 1,
                    MessageContent::Parts(parts) => {
                        parts_msgs += 1;
                        for p in parts {
                            match p {
                                crate::ai::types::ContentPart::Text { .. } => parts_text += 1,
                                crate::ai::types::ContentPart::Image { .. } => parts_image += 1,
                                crate::ai::types::ContentPart::Audio { .. } => parts_audio += 1,
                                _ => parts_other += 1,
                            }
                        }
                    }
                }
            }
            let log = runtime::logger("llamacpp");
            log.info(&format!(
                "generate_text request: model={} messages={} (text={} parts={}; parts contain text={} image={} audio={} other={})",
                request.model.as_deref().unwrap_or("?"),
                request.messages.len(),
                text_msgs,
                parts_msgs,
                parts_text,
                parts_image,
                parts_audio,
                parts_other,
            ));

            // RTOS probe: inference entry seam. The persona service loop
            // talks through `adapter.generate_text`; this is where the
            // wall-clock cost actually accrues. Class taxonomy lives in
            // docs/architecture/RTOS-DEBUGGER-PROBES.md. Per
            // [[jtag-probes-are-rtos-debugger]]: name the surrounding
            // vars so the operator filtering on
            // `class=="inference.generate.enter"` has the request
            // fingerprint without grepping logs.
            continuum_base::probe!(
                class = "inference.generate.enter",
                model = request.model.as_deref().unwrap_or("?"),
                persona_id = request.persona_id.as_deref().unwrap_or(""),
                msg_count = request.messages.len(),
                max_tokens = request.max_tokens.unwrap_or(0),
                has_system_prompt = request.system_prompt.is_some(),
                parts_image = parts_image,
                parts_audio = parts_audio,
                "generate_text entry"
            );
        }

        let mut collected_media: Vec<(llama::MediaKind, Vec<u8>)> = Vec::new();
        let mut messages: Vec<llama::ChatMsg> = Vec::new();
        if let Some(sys) = request.system_prompt.as_ref() {
            if !sys.is_empty() {
                messages.push(llama::ChatMsg {
                    role: "system".to_string(),
                    content: sys.clone(),
                });
            }
        }
        for msg in &request.messages {
            let content = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Parts(parts) => {
                    let mut out = String::new();
                    for p in parts {
                        match p {
                            crate::ai::types::ContentPart::Text { text } => {
                                out.push_str(text);
                            }
                            crate::ai::types::ContentPart::Image { image } => {
                                // Splice the marker at this exact spot —
                                // mtmd_tokenize replaces it with the
                                // image-token chunk. Position matters
                                // (text-before-image vs after changes
                                // what the model sees).
                                out.push_str(llama::MtmdContext::default_marker());
                                let bytes = decode_image_bytes(image)?;
                                collected_media.push((llama::MediaKind::Image, bytes));
                            }
                            crate::ai::types::ContentPart::Audio { audio } => {
                                // Same shape as image — splice marker,
                                // collect bytes. mtmd's bitmap helper
                                // auto-detects audio from magic bytes;
                                // the modality tag here drives backend
                                // capability checks (supports_audio
                                // instead of supports_vision) and
                                // routing to generate_with_audio.
                                out.push_str(llama::MtmdContext::default_marker());
                                let bytes = decode_audio_bytes(audio)?;
                                collected_media.push((llama::MediaKind::Audio, bytes));
                            }
                            crate::ai::types::ContentPart::Video { .. } => {
                                return Err("llamacpp_adapter: encoded video input is unsupported; decode it into timestamped frames and audio before model projection. No video was consumed.".into());
                            }
                            crate::ai::types::ContentPart::ToolUse { .. }
                            | crate::ai::types::ContentPart::ToolResult { .. } => {} // handled by tool path
                        }
                    }
                    out
                }
            };
            messages.push(llama::ChatMsg {
                role: msg.role.clone(),
                content,
            });
        }
        // RTOS probe: chat-template rendering is synchronous + small,
        // but cumulative across thousands of turns it can shadow real
        // bottlenecks. Bracketing it lets the operator subtract it
        // from `inference.forward.*` cleanly.
        let prompt = continuum_base::time_sync!("inference.render_chat", {
            llama::render_chat(template.as_deref(), &messages, true)
        })?;

        // No hardcoded cap. If the caller didn't specify, the model can
        // decode up to its trained context. Capping silently at 2048 was
        // the source of clipped JSON/XML output — the model would stop
        // mid-structure and downstream JSON.parse / XML parsers blew up.
        let max_tokens = request
            .max_tokens
            .map(|n| n as usize)
            .unwrap_or_else(|| backend.n_ctx_train() as usize);
        let sampling = sampling_config_from_request(&request);
        // Stop sequences = caller-supplied + model's registry-declared
        // text-form stops. Some GGUFs (the forged qwen3.5 included) carry
        // the wrong tokenizer.ggml.eos_token_id, so is_eog_token never
        // fires for the chat-template terminator and the model loops the
        // same answer until max_tokens. The registry's stop_sequences
        // field carries the correct strings (e.g. `<|im_end|>`) that the
        // scheduler matches against streamed output.
        let mut stop_owned: Vec<String> = request.stop_sequences.clone().unwrap_or_default();
        if let Some(model_meta) =
            crate::model_registry::try_global().and_then(|reg| reg.model(backend.model_id()))
        {
            for s in &model_meta.stop_sequences {
                if !stop_owned.contains(s) {
                    stop_owned.push(s.clone());
                }
            }
        }

        let gen_start = Instant::now();
        let backend_for_blocking = backend.clone();
        let prompt_for_blocking = prompt.clone();
        let stop_for_closure = stop_owned.clone();
        let sampling_for_closure = sampling.clone();
        // Parse the wire-format persona_id (Option<String> on the public
        // request type) to Option<Uuid> for the typed scheduler API. A
        // malformed UUID drops to None rather than failing the request —
        // the request itself is still valid, we just can't attribute its
        // KV bytes per-persona. The registry's drift-detection sanity
        // check will surface this if it becomes systemic.
        let persona_id: Option<uuid::Uuid> = request
            .persona_id
            .as_deref()
            .and_then(|s| uuid::Uuid::parse_str(s).ok());

        // Genome paging: the adapter contract carries the genes to apply as
        // `(name, path, scale)`. We page each in (idempotent) and hand the
        // resolved `(id, scale)` list to the scheduler, which applies it
        // context-level before decode. Empty in the common base case.
        let requested_genes: Vec<(String, std::path::PathBuf, f32)> = request
            .active_adapters
            .as_ref()
            .map(|v| {
                v.iter()
                    .map(|a| {
                        (
                            a.name.clone(),
                            std::path::PathBuf::from(&a.path),
                            a.scale as f32,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let result: Result<(String, usize), String> = if collected_media.is_empty() {
            // Pure-text path: scheduler-managed continuous batching.
            // RTOS timing probe: this is the actual LLM forward pass —
            // by far the dominant cost (95%+ on LCD tier per
            // 2026-06-06 baseline). `time_probe!` wraps the JoinHandle
            // future cleanly across the `.await`.
            let genes_for_closure = requested_genes;
            continuum_base::time_probe!(
                "inference.forward.text",
                tokio::task::spawn_blocking(move || {
                    let stop_refs: Vec<&str> =
                        stop_for_closure.iter().map(|s| s.as_str()).collect();
                    // Page in each requested gene (idempotent) before generation.
                    // A missing/unreadable adapter file is a hard error — never
                    // silently run the base model in its place (Rule 2).
                    for (id, path, _) in &genes_for_closure {
                        backend_for_blocking.ensure_adapter(id, path)?;
                    }
                    let active_loras: Vec<(String, f32)> = genes_for_closure
                        .iter()
                        .map(|(id, _, scale)| (id.clone(), *scale))
                        .collect();
                    backend_for_blocking.generate_for_persona(
                        persona_id,
                        &prompt_for_blocking,
                        max_tokens,
                        sampling_for_closure,
                        &stop_refs,
                        &active_loras,
                    )
                })
            )
            .map_err(|e| format!("generate task panicked: {e}"))?
        } else {
            // Multimodal path: bypass the scheduler — media tokens have
            // a fixed positional layout the scheduler can't interleave
            // with concurrent text seqs. Single-media-per-call scope for
            // v1; mtmd's C API supports multiple media in one prompt
            // (one marker each in order) but our backend signatures take
            // one bytes blob. Hard-error rather than silently dropping
            // extras — clearer signal upstream.
            // Genes on the multimodal bypass path are not yet supported (the
            // mtmd single-flight path doesn't route through the scheduler that
            // applies set_loras). Fail loud rather than silently dropping the
            // requested adapter (Rule 2).
            if !requested_genes.is_empty() {
                let names: Vec<&str> = requested_genes
                    .iter()
                    .map(|(id, _, _)| id.as_str())
                    .collect();
                return Err(format!(
                    "llamacpp_adapter: LoRA genes {names:?} requested with media — not supported \
                     on the multimodal bypass path (v1). Genes apply only on the text scheduler \
                     path; send the gene-bearing request without media."
                ));
            }
            if collected_media.len() > 1 {
                let kinds: Vec<String> = collected_media
                    .iter()
                    .map(|(k, _)| format!("{:?}", k))
                    .collect();
                return Err(format!(
                    "llamacpp_adapter: multi-media not yet supported in this adapter \
                     ({} items: {}); send one media item per request until backend.\
                     generate_with_media accepts &[(MediaKind, Vec<u8>)]",
                    collected_media.len(),
                    kinds.join(", ")
                ));
            }
            let (kind, media_bytes) = collected_media.into_iter().next().unwrap();
            // RTOS timing probe: mtmd path runs single-flight (no
            // scheduler batching for media) so the timing here is
            // direct end-to-end. Separate seam from the text path so
            // operators can `jq` text-only vs mtmd cost distinctly.
            continuum_base::time_probe!(
                "inference.forward.multimodal",
                tokio::task::spawn_blocking(move || {
                    let stop_refs: Vec<&str> =
                        stop_for_closure.iter().map(|s| s.as_str()).collect();
                    match kind {
                        llama::MediaKind::Image => backend_for_blocking.generate_with_image(
                            &prompt_for_blocking,
                            &media_bytes,
                            max_tokens,
                            sampling_for_closure,
                            &stop_refs,
                        ),
                        llama::MediaKind::Audio => backend_for_blocking.generate_with_audio(
                            &prompt_for_blocking,
                            &media_bytes,
                            max_tokens,
                            sampling_for_closure,
                            &stop_refs,
                        ),
                    }
                })
            )
            .map_err(|e| format!("generate_with_media task panicked: {e}"))?
        };
        let (text, tokens) = result?;

        let elapsed = gen_start.elapsed();
        let tok_per_sec = if elapsed.as_secs_f64() > 0.0 {
            tokens as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        *self.last_throughput_tok_s.write() = tok_per_sec;

        // RTOS probe: inference exit seam. Pair with
        // `inference.generate.enter` via the same span ancestry. The
        // critical campaign metric is `tok_per_sec` — every probe
        // here carries it so a `jq` over `inference.generate.exit`
        // events is the latency dashboard. `text_len` lets the
        // operator catch the silent-truncation class of bug where
        // the model stops short of EOS.
        // Real tokens on the served lane = proof of life for the health heartbeat, which
        // otherwise probes for a slot it cannot get while this very work holds them all.
        if tokens > 0 {
            crate::inference::llama_server::note_real_decode();
        }
        continuum_base::probe!(
            class = "inference.generate.exit",
            model = backend.model_id(),
            tokens_out = tokens,
            text_len = text.len(),
            duration_ms = elapsed.as_millis() as u64,
            tok_per_sec = tok_per_sec,
            "generate_text exit"
        );

        // No tail-strip. Previously this hand-rolled `text.rfind(stop)` and
        // truncated — only existed to clean up the special tokens that
        // leaked from the OLD hand-rolled chat-template prefixes. Now that
        // we use the model's real chat template via `render_chat`, the
        // model's actual EOS tokens stop generation (handled inside the
        // scheduler via `is_eog_token`) and don't leak as text.

        Ok(TextGenerationResponse {
            text,
            finish_reason: FinishReason::Stop,
            model: backend.model_id().to_string(),
            provider: LLAMACPP_PROVIDER_ID.to_string(),
            usage: UsageMetrics {
                input_tokens: 0, // backend doesn't return this currently; future enhancement
                output_tokens: tokens as u32,
                total_tokens: tokens as u32,
                estimated_cost: None,
            },
            response_time_ms: elapsed.as_millis() as u64,
            request_id: format!("llamacpp-{}", continuum_base::time::EpochMs::now().millis()),
            content: None,
            tool_calls: None,
            // TODO: if this in-process backend serves a reasoning model (qwen3 etc.)
            // it would emit inline `<think>` in `text` too — apply
            // `crate::ai::openai_adapter::extract_reasoning` here to separate it.
            // Not Asha's path today (she routes through the openai adapter).
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        })
    }

    /// Embeddings via the backend's dedicated embedding-mode context. The loaded
    /// model determines the vector space — for grid-comparable vectors this
    /// adapter must have loaded the canonical Qwen3-Embedding-0.6B (the
    /// `NeuralEmbeddingProvider` is responsible for loading it). `backend.embed`
    /// is a blocking forward pass, so it runs on `spawn_blocking`, off the async
    /// executor (the same bridge as `generate_text`'s forward).
    async fn create_embedding(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        let backend = self.ensure_loaded()?;
        let model_id = backend.model_id().to_string();
        let texts: Vec<String> = match request.input {
            EmbeddingInput::Single(s) => vec![s],
            EmbeddingInput::Multiple(v) => v,
        };
        let zero_usage = UsageMetrics {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            estimated_cost: None,
        };
        if texts.is_empty() {
            return Ok(EmbeddingResponse {
                embeddings: Vec::new(),
                model: model_id,
                provider: LLAMACPP_PROVIDER_ID.to_string(),
                usage: zero_usage,
                response_time_ms: 0,
            });
        }
        let start = Instant::now();

        // ── Governed GPU admission (mirrors inference::single_purpose_lane::acquire_lane_slot) ──
        // The embedding forward pass allocates a bounded Metal context (a 2048-token
        // embedding context: ~1.2 GiB compute buffer — quadratic in n_ubatch=2048 —
        // plus ~224 MiB KV). UNGOVERNED, it grabbed that context behind the governor's
        // back and competed with the serving lane for VRAM; under pressure the Metal
        // command buffer OOM'd and the decode returned an ALL-ZERO vector — the
        // "degenerate embedding" that silently broke semantic recall. Lease the VRAM
        // from the ResourceGovernor FIRST: granted ⇒ the bytes are reserved for the
        // life of the guard and the decode has room to succeed; refused ⇒ fail LOUD
        // here instead of allocating a doomed context that emits garbage. Embeddings
        // are GPU-only ([[gpu-is-non-negotiable-every-component-no-cpu-fallback]]), so
        // unlike the eval lane there is NO CPU spill — the honest degrade is a named
        // refusal the caller ([`NeuralEmbeddingProvider::embed`]) already surfaces as
        // "no signal", never a zero vector.
        //
        // Const, not env-tunable — substrate policy lives in code (concurrency guide).
        // Only the per-call CONTEXT is leased here; the embed lane's standing FLOOR on
        // the board (so serving can never grow into this slice — Joel 2026-08-08: "the
        // budgeter just has all its parts figure it out") is claimed at embedder
        // resolve via [`crate::resources::ResourceDaemon::reserve`], from the same
        // module-level constants below.
        const EMBED_LANE_LEASE_TTL_MS: u64 = 60_000; // SIGKILL backstop; the RAII guard frees on drop
        let _vram_lease = {
            use crate::resources::{
                LeaseError, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind,
            };
            match ResourceDaemon::global() {
                Some(daemon) => {
                    let req = LeaseRequest {
                        consumer_id: EMBED_LANE_CONSUMER_ID.to_string(),
                        kind: ResourceKind::Vram,
                        bytes: EMBED_LANE_VRAM_BYTES,
                        // A bounded, sub-second forward pass is not yanked mid-embed;
                        // the guard returns the bytes the instant it finishes.
                        ttl_ms: EMBED_LANE_LEASE_TTL_MS,
                        reclaim_policy: ReclaimPolicy::Pinned,
                    };
                    // ONE bounded retry on a capacity refusal (never a loop —
                    // [[brittleness-is-the-highest-priority-work-there-is]]): with
                    // the standing floor reserved at resolve, a refusal here means
                    // a TRANSIENT over-commit (a lane mid-relaunch, a burst), and
                    // relief lands within a governor tick. The second refusal is
                    // the honest failure.
                    let mut refused: Option<u64> = None;
                    let mut granted = None;
                    for attempt in 0..2u8 {
                        match daemon.acquire_guarded(&req) {
                            Ok(guard) => {
                                continuum_base::probe!(
                                    class = "embed.vram.leased",
                                    consumer = EMBED_LANE_CONSUMER_ID,
                                    bytes = EMBED_LANE_VRAM_BYTES,
                                    texts = texts.len(),
                                    retried = (attempt > 0),
                                    "embedding lane acquired a governed VRAM lease"
                                );
                                granted = Some(guard);
                                break;
                            }
                            Err(LeaseError::InsufficientCapacity { available, .. }) => {
                                continuum_base::probe!(
                                    class = "embed.vram.refused",
                                    requested = EMBED_LANE_VRAM_BYTES,
                                    available = available,
                                    attempt = attempt,
                                    "embedding VRAM lease refused — failing loud, NOT emitting degenerate zeros"
                                );
                                refused = Some(available);
                                if attempt == 0 {
                                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                }
                            }
                            Err(e) => {
                                return Err(format!(
                                    "embed: VRAM lease error ({e:?}) — refusing to embed ungoverned"
                                ));
                            }
                        }
                    }
                    match granted {
                        Some(guard) => Some(guard),
                        None => {
                            let available = refused.unwrap_or(0);
                            return Err(format!(
                                "embed: VRAM lease refused twice — {} MiB requested, {} MiB governed-available. \
                                 Refusing to allocate an OOM-doomed embedding context (it would decode to \
                                 degenerate zeros). Free VRAM (tier down serving) and retry.",
                                EMBED_LANE_VRAM_BYTES / (1024 * 1024),
                                available / (1024 * 1024),
                            ));
                        }
                    }
                }
                // Ungoverned node (no ResourceDaemon::global()): behavior unchanged —
                // the backend's own new_context allocation is the only gate, as before.
                None => None,
            }
        };

        let embeddings = tokio::task::spawn_blocking(move || backend.embed(&texts))
            .await
            .map_err(|e| format!("embedding task join failed: {e}"))??;
        // `_vram_lease` drops here → the VRAM is returned to the governor's board.
        Ok(EmbeddingResponse {
            embeddings,
            model: model_id,
            provider: LLAMACPP_PROVIDER_ID.to_string(),
            usage: zero_usage,
            response_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        let healthy = self.backend.read().is_some() || self.model_path.exists();
        HealthStatus {
            status: if healthy {
                HealthState::Healthy
            } else {
                HealthState::Unhealthy
            },
            api_available: healthy,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: continuum_base::time::EpochMs::now().millis(),
            message: Some(if healthy {
                "in-process llama.cpp backend ready".to_string()
            } else {
                format!("model GGUF missing at {:?}", self.model_path)
            }),
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        // Identity + capabilities come from the registry (the Rust catalog (catalog.rs)).
        // Runtime overlay (context_window from GGUF metadata, tokens/sec
        // from last measurement) only applies if the backend is loaded;
        // otherwise we return the catalog-declared view and let the first
        // generate_text call refresh the numbers.
        let base = models_for_provider_via_registry(LLAMACPP_PROVIDER_ID);
        let backend_guard = self.backend.read();
        let last_tok_s = *self.last_throughput_tok_s.read();
        base.into_iter()
            .map(|info| match backend_guard.as_ref() {
                Some(b) if info.id == self.default_model => {
                    model_info_with_runtime(info, b, last_tok_s)
                }
                _ => info,
            })
            .collect()
    }

    fn model_metadata(&self, model_id: &str) -> Option<ModelInfo> {
        // Match against the registry (provider's declared models), then
        // overlay runtime fields if the backend happens to be loaded.
        // Matching is case-insensitive on the declared id; no substring
        // special-casing — the id is the contract.
        let want = model_id.to_lowercase();
        let info = models_for_provider_via_registry(LLAMACPP_PROVIDER_ID)
            .into_iter()
            .find(|m| m.id.to_lowercase() == want)?;
        let backend_guard = self.backend.read();
        match backend_guard.as_ref() {
            Some(b) if info.id == self.default_model => Some(model_info_with_runtime(
                info,
                b,
                *self.last_throughput_tok_s.read(),
            )),
            _ => Some(info),
        }
    }

    fn device_type(&self) -> InferenceDevice {
        // Bundled llama.cpp is built with Metal (Mac) / CUDA (Linux) per
        // continuum's build flags. Either way: GPU-class device.
        InferenceDevice::Gpu
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // Intentionally empty — this adapter lists its models explicitly
        // in the registry, and `supports_model` below matches against the
        // declared ids directly. The old hardcoded prefixes (qwen3.5-…)
        // would silently match a Qwen3.5 row under a *different* provider
        // (DMR) and mis-route it here. Exact-id match is the contract.
        Vec::new()
    }

    fn supports_model(&self, model_name: &str) -> bool {
        self.default_model.eq_ignore_ascii_case(model_name)
    }
}
