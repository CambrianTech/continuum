//! AI Provider Adapter Trait - The AI abstraction interface
//!
//! All AI providers implement this trait. The AIProviderModule works with
//! this trait, never with concrete implementations directly.
//!
//! Supported backends:
//! - OpenAI (GPT models)
//! - Anthropic (Claude models)
//! - DeepSeek
//! - Together AI
//! - Groq
//! - Fireworks
//! - XAI (Grok)
//! - Google (Gemini)
//! - Local (Candle, llama.cpp)

use crate::clog_warn;
use crate::model_registry::{Capability, ProviderKind, ToolProtocol};
use async_trait::async_trait;
use std::collections::BTreeSet;
use std::sync::Arc;

use super::types::{
    EmbeddingRequest, EmbeddingResponse, HealthStatus, ModelInfo, TextGenerationRequest,
    TextGenerationResponse,
};

/// Device preference for inference — same pattern as PyTorch device='cuda'
/// or Android's MediaCodec hardware acceleration flags. Callers declare
/// what they need; the registry picks the best match from what's available.
///
/// Default: Gpu (enforced now — no silent CPU fallback).
/// Auto (try GPU, explicit CPU fallback) is reserved for future opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceDevice {
    /// GPU-accelerated inference only. Metal (Mac) / CUDA (Nvidia) /
    /// ROCm (AMD) / Vulkan (everyone else). If no GPU adapter can serve
    /// the model → hard error, never silent CPU.
    Gpu,
    /// CPU-only inference. Candle / future CPU adapter. Currently only
    /// reachable when caller EXPLICITLY requests it (training pipelines,
    /// or env CONTINUUM_ALLOW_CPU_INFERENCE=1). Never auto-selected.
    Cpu,
    /// Try GPU first; if unavailable, fall back to CPU WITH a visible
    /// log warning. NOT IMPLEMENTED YET — reserved for when we trust
    /// the CPU path enough to ship it as a degraded-but-acceptable
    /// experience. Until then, `Auto` behaves identically to `Gpu`.
    Auto,
}

impl Default for InferenceDevice {
    fn default() -> Self {
        InferenceDevice::Gpu
    }
}

/// AI provider adapter configuration
#[derive(Debug, Clone)]
pub struct AdapterConfig {
    /// Provider identifier (e.g., "openai", "anthropic", "deepseek")
    pub provider_id: String,
    /// Human-readable name
    pub name: String,
    /// Base URL for API calls
    pub base_url: String,
    /// Environment variable name for API key
    pub api_key_env: String,
    /// Default model to use
    pub default_model: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retries on failure
    pub max_retries: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            name: String::new(),
            base_url: String::new(),
            api_key_env: String::new(),
            default_model: String::new(),
            timeout_ms: 120_000,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// How the adapter ACCEPTS structured-output schemas. Same shape as
/// `ToolProtocol` — cognition asks "can you constrain output to
/// this schema?" and routes accordingly. Independent of tool calling
/// because some adapters support schemas without tools (and vice versa).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StructuredOutputProtocol {
    /// No structured-output enforcement. Substrate must validate + retry.
    #[default]
    None,
    /// JSON Schema enforced by the provider (OpenAI structured outputs).
    /// Strongest guarantee — the API rejects invalid outputs.
    JsonSchema,
    /// Grammar-constrained sampling (llama.cpp `--grammar` / GBNF).
    /// Local-model strength: the sampler refuses tokens that violate
    /// the grammar.
    GrammarConstrained,
    /// Schema described in the prompt; substrate parses + retries on
    /// invalid. Always-available fallback for text models.
    PromptOnly,
}

/// The coherent NATIVE protocol pair an adapter speaks — `tool_call_protocol`
/// and `structured_output_protocol` always travel together, so the pairing is
/// codified here once instead of being re-derived (and risked-inconsistent) in
/// every `capabilities()`. Picking a `NativeProtocols` makes an incoherent
/// combo (native function-calling tools but prompt-only structure, say)
/// unrepresentable, and gives the next adapter author ONE thing to choose.
///
/// Each variant is a real, observed adapter shape. To add a base model with a
/// new wire format, add a variant here — never a fresh ad-hoc `(tool, struct)`
/// pair in an adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NativeProtocols {
    /// No native protocol — the model makes no tool/structure guarantee.
    /// Cognition does everything out-of-band. The text/embedding floor
    /// (heuristic, embedding-only, remote-peer-unknown).
    #[default]
    None,
    /// No native protocol, but the model is a competent chat model: tools and
    /// schema are described in-prompt and cognition validates + retries.
    /// (OpenAI-compatible endpoint serving a model without tool support.)
    PromptEmulated,
    /// Cloud function-calling: native `tool_use`/`tool_calls` blocks + native
    /// JSON-Schema enforcement. (Anthropic, OpenAI-with-tools.)
    FunctionCalling,
    /// llama.cpp family: prompt-driven tool JSON + GBNF grammar-constrained
    /// structured output (the sampler refuses grammar-violating tokens).
    GrammarConstrained,
}

impl NativeProtocols {
    /// The tool-calling protocol half of the pair.
    pub fn tool_call(self) -> ToolProtocol {
        match self {
            Self::None => ToolProtocol::None,
            Self::PromptEmulated => ToolProtocol::None,
            Self::FunctionCalling => ToolProtocol::NativeFunctionCalling,
            Self::GrammarConstrained => ToolProtocol::JsonInPrompt,
        }
    }

    /// The structured-output protocol half of the pair.
    pub fn structured_output(self) -> StructuredOutputProtocol {
        match self {
            Self::None => StructuredOutputProtocol::None,
            Self::PromptEmulated => StructuredOutputProtocol::PromptOnly,
            Self::FunctionCalling => StructuredOutputProtocol::JsonSchema,
            Self::GrammarConstrained => StructuredOutputProtocol::GrammarConstrained,
        }
    }
}

/// AI provider adapter capabilities — the typed surface the substrate's
/// coordinator consults when routing a workload to the best-fit adapter.
///
/// ONE capability vocabulary (#65 collapse): `model_registry::Capability`.
/// There is no `ModalitySet`, no `supports_*` bool mirror, no second enum —
/// `caps.has(Capability::Vision)` IS the modality check, and that same check
/// is what fires the sensory bridge: a capability NOT in this set is bridged
/// by the substrate before the adapter sees the request (vision →
/// VisionDescriptionService, AudioInput → STT, AudioOutput → TTS). Capability
/// declared honestly = bridges applied correctly = LCD personas get the same
/// sensory experience as Claude (`[[ai-namespace-multimodal-crutches]]`).
///
/// Per `[[adapter-pattern-is-the-pivot-insurance]]`: every ML-touching
/// capability sits behind this trait so the substrate can pivot (swap
/// framework, swap model, swap provider) by declaration, not rewrite. The
/// only non-capability members are the scalars cognition needs to bound a
/// turn and the protocol descriptors the tool/structured-output loops route
/// through (protocol = HOW, distinct axis from WHAT the model can do).
#[derive(Debug, Clone, Default)]
pub struct AdapterCapabilities {
    /// What the adapter's model can do — the single source of truth.
    pub capabilities: BTreeSet<Capability>,
    /// Inference runs on this host (provider kind == Local).
    pub is_local: bool,
    /// Context window (input + output limit) AS DECLARED BY THE ADAPTER.
    ///
    /// `None` means the adapter has not declared one — NOT "assume a small default". This
    /// used to be a bare `u32` seeded from a `FLOOR_CONTEXT_WINDOW = 4096` constant, so any
    /// adapter that didn't override reported 4096 as its ceiling. Nothing budgeted against it
    /// yet, which is the only reason it hadn't already broken a 1M-context model — but the
    /// obvious next use of this field (budgeting, which is exactly what #46 was about) would
    /// have silently clamped every under-declaring adapter to 4k. Making "undeclared"
    /// unrepresentable as a number closes that off by construction: a consumer must handle
    /// `None` deliberately (ask the served lane) instead of inheriting a guess.
    /// [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
    pub max_context_window: Option<u32>,
    /// Maximum tokens the adapter will emit in a single response, as declared. Distinct from
    /// `max_context_window`. `None` = undeclared, same contract as above (#45: the adapter
    /// owns generation length; nobody downstream invents a cap).
    pub max_output_tokens: Option<u32>,

    /// Tool-calling protocol the adapter NATIVELY speaks. Cognition's tool
    /// loop routes through this. Default means prompt-text emulation in compose.
    pub tool_call_protocol: ToolProtocol,
    /// Structured-output protocol the adapter NATIVELY speaks. Independent of
    /// tool calling. Default means schema validation + retry happen in cognition.
    pub structured_output_protocol: StructuredOutputProtocol,
}

impl AdapterCapabilities {
    /// Does the adapter's model declare this capability? The one accessor —
    /// modality routing, tool gating, embedding/image-gen all resolve here.
    pub fn has(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    /// A minimal text-only capability set — the common case for a basic
    /// chat/completion adapter (and the trait's default `capabilities()`).
    /// This is also the [`builder`](Self::builder) seed, so a richer adapter
    /// declares only what it adds on top of the floor.
    pub fn text_only() -> Self {
        Self {
            capabilities: BTreeSet::from([Capability::TextGeneration, Capability::Chat]),
            // Undeclared, NOT a small default — see the field docs.
            max_context_window: None,
            max_output_tokens: None,
            // The floor has no ToolUse capability, so its protocol pair must be
            // None — set it explicitly, NOT via Default. `ToolProtocol::default()`
            // is `NativeFunctionCalling` (the right default for a registry Provider
            // that DOES declare tools), which would falsely advertise native
            // function calling on a text-only adapter. The protocol must always
            // match the capability: no ToolUse ⇒ no tool protocol.
            tool_call_protocol: ToolProtocol::None,
            structured_output_protocol: StructuredOutputProtocol::None,
            ..Default::default()
        }
    }

    /// Fluent constructor seeded from the text-only floor. The codified
    /// projection every adapter's `capabilities()` builds through, so the next
    /// adapter declares (what, where, how-big, which-protocols) without
    /// hand-assembling the struct or risking an incoherent protocol pair:
    /// ```ignore
    /// AdapterCapabilities::builder()
    ///     .capabilities([Capability::TextGeneration, Capability::Chat, Capability::ToolUse, Capability::Vision])
    ///     .remote()
    ///     .context_window(200_000)
    ///     .max_output_tokens(8_192)
    ///     .protocols(NativeProtocols::FunctionCalling)
    ///     .build()
    /// ```
    /// Zero runtime cost — moves a value, compiles to the same as a literal.
    pub fn builder() -> AdapterCapabilitiesBuilder {
        AdapterCapabilitiesBuilder {
            inner: Self::text_only(),
        }
    }
}

/// Builder for [`AdapterCapabilities`] — see [`AdapterCapabilities::builder`].
#[derive(Debug, Clone)]
pub struct AdapterCapabilitiesBuilder {
    inner: AdapterCapabilities,
}

impl AdapterCapabilitiesBuilder {
    /// Declare the full capability set (replaces the floor's text+chat).
    pub fn capabilities(mut self, caps: impl IntoIterator<Item = Capability>) -> Self {
        self.inner.capabilities = caps.into_iter().collect();
        self
    }

    /// Add one capability on top of the current set.
    pub fn with(mut self, cap: Capability) -> Self {
        self.inner.capabilities.insert(cap);
        self
    }

    /// Inference runs on this host (provider kind == Local).
    pub fn local(mut self) -> Self {
        self.inner.is_local = true;
        self
    }

    /// Inference runs off-host (cloud API or remote grid peer).
    pub fn remote(mut self) -> Self {
        self.inner.is_local = false;
        self
    }

    /// Context window (input + output ceiling) — from the served model.
    pub fn context_window(mut self, n: u32) -> Self {
        self.inner.max_context_window = Some(n);
        self
    }

    /// Maximum tokens emitted in a single response — from the served model.
    pub fn max_output_tokens(mut self, n: u32) -> Self {
        self.inner.max_output_tokens = Some(n);
        self
    }

    /// Declare the coherent native (tool-call, structured-output) protocol
    /// pair in one move. See [`NativeProtocols`].
    pub fn protocols(mut self, protocols: NativeProtocols) -> Self {
        self.inner.tool_call_protocol = protocols.tool_call();
        self.inner.structured_output_protocol = protocols.structured_output();
        self
    }

    /// Finish building.
    pub fn build(self) -> AdapterCapabilities {
        self.inner
    }
}

/// LoRA capabilities reported by adapters
#[derive(Debug, Clone, Default)]
pub enum LoRACapabilities {
    /// No LoRA support (most cloud APIs)
    #[default]
    None,
    /// Single adapter at a time (cloud fine-tuning APIs like Together, Fireworks)
    SingleAdapter,
    /// Full local control with multi-adapter paging
    MultiLayerPaging {
        max_loaded: usize,
        supports_hot_swap: bool,
    },
}

/// Information about a loaded LoRA adapter
#[derive(Debug, Clone)]
pub struct LoRAAdapterInfo {
    pub adapter_id: String,
    pub path: String,
    pub scale: f64,
    pub loaded: bool,
    pub active: bool,
}

/// API style for the provider
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiStyle {
    /// OpenAI-compatible API (most providers)
    /// POST /v1/chat/completions with Bearer auth
    OpenAI,
    /// Anthropic API (different format)
    /// POST /v1/messages with x-api-key header
    Anthropic,
    /// Google Gemini API
    /// POST /v1beta/models/{model}:generateContent
    Google,
    /// Local inference (Candle, llama.cpp)
    Local,
}

/// A unit of generated output, delivered to the consumer the INSTANT the backend
/// produces it — the streaming primitive. A token that exists now reaches the
/// subscriber now: the same shape as an audio sample, a video frame, or a game
/// state delta on the wire. Generation is a low-latency *stream*, not a `Future`
/// you await for the whole result; [`AIProviderAdapter::generate_text`] is just
/// the convenience drain over [`AIProviderAdapter::generate_stream`] for callers
/// that don't need the tokens live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerationChunk {
    /// A fragment of the user-facing answer, emitted as the model decodes it.
    Token(String),
    /// A fragment of the model's private reasoning (`<think>` / `reasoning_content`).
    /// Surfaced on its OWN variant so a consumer can show "thinking…" live without
    /// ever leaking chain-of-thought into the room — the answer/reasoning split is
    /// preserved on the stream, not just on the assembled response.
    Reasoning(String),
    /// PREFILL advanced — the slot has ingested `processed` of `total` prompt
    /// tokens (`cached` of them served free by the KV prefix cache). Emitted
    /// BEFORE any token exists, so a consumer can show honest progress during
    /// the long silence a big prompt buys ([[honest-presence-lifecycle]]).
    ///
    /// This is also the liveness signal the stream watchdog keys on: a healthy
    /// prefill raises `processed`, a wedged slot freezes it. Consumers that only
    /// care about text may ignore this variant.
    Prefill {
        processed: u64,
        total: u64,
        cached: u64,
    },
}

/// The universal AI provider adapter trait
///
/// All AI providers implement this trait. The AIProviderModule calls
/// these methods; adapters translate to native provider API calls.
#[async_trait]
pub trait AIProviderAdapter: Send + Sync {
    /// Get adapter provider ID (e.g., "openai", "anthropic")
    fn provider_id(&self) -> &str;

    /// Get adapter human-readable name
    fn name(&self) -> &str;

    /// Get adapter capabilities.
    ///
    /// Default: a text-only set (`AdapterCapabilities::text_only`).
    /// Override to declare tools, vision, audio, streaming, or richer
    /// context/output limits.
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::text_only()
    }

    /// Get API style.
    ///
    /// Default: `Local` (in-process inference). Cloud adapters override
    /// with `OpenAI` / `Anthropic` / `Google`.
    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    /// The LIVE served context window (tokens) of the lane THIS adapter serves on,
    /// or `None` when the adapter's own binding window is already authoritative.
    ///
    /// This is the single source of truth cognition budgets its prompt against — the
    /// window of the persona's ACTUAL lane, read live, never a global snapshot and
    /// never a post-hoc clamp ([[budget-at-assembly-never-clamp-the-prompt]],
    /// [[no-hardcoded-context-numbers-derive-from-the-live-window]]). Each adapter
    /// knows which lane it is bound to, so each answers for itself:
    /// - shared single-resident gateway → the gateway's current served slot (the live
    ///   `/props` truth for the one resident model), so a lane that relaunched
    ///   smaller/larger is tracked in BOTH directions;
    /// - a DEDICATED lane an adapter owns (an eval fork's `EphemeralServingLane`) →
    ///   `None`: its window was pinned from ITS OWN `/props` at spawn and is carried on
    ///   the binding; the global gateway snapshot describes a DIFFERENT server and must
    ///   never be consulted for it (glass-boxed 2026-07-20: reading the global slot
    ///   starved an eval fork's prompt to the live lane's per-slot window → webdev-rs
    ///   0/6 while short coder prompts passed);
    /// - cloud / in-process → `None`: the declared binding window stands.
    ///
    /// Default `None` — the binding window is authoritative unless an adapter has a
    /// live lane to report.
    fn live_served_window(&self) -> Option<u32> {
        None
    }

    /// Get default model for this provider
    fn default_model(&self) -> &str;

    /// Initialize the adapter (verify API key, load the model file
    /// off disk). Pays the model-load wall-clock once at boot so
    /// downstream consumers see the model's real capabilities from
    /// the first query on.
    ///
    /// Default: `Ok(())` — adapters with no init contract (cloud
    /// providers that authenticate lazily, in-process/heuristic
    /// adapters, test fixtures) opt out silently. Local model adapters
    /// that load weights off disk MUST override.
    async fn initialize(&mut self) -> Result<(), String> {
        Ok(())
    }

    /// Warm the adapter's hot path BEFORE the first real `generate_text`
    /// call. For llama.cpp: run a tiny throwaway decode against a
    /// minimal prompt so the KV-cache buffers, attention kernels,
    /// and sampling state are warm-resident in the substrate's working
    /// set when the first real turn lands.
    ///
    /// Per [[init-once-handle-then-lease-zero-copy-refs]]: the
    /// substrate's latency story is "init once at boot, lease on hot
    /// path." `warmup` is the inference-layer instance of that
    /// pattern, paying the JIT / cache-cold cost in the supervisor's
    /// `materialize_adapters` step instead of on Joel's first message.
    ///
    /// Default impl is `Ok(())` — adapters without a meaningful
    /// warmup contract (cloud providers, heuristic adapter) opt out
    /// silently. Local model adapters (LlamaCpp, future Candle) MUST
    /// override.
    ///
    /// Returning Err means the adapter couldn't warm — surfaced by
    /// the supervisor as a typed slot failure per [[no-fallbacks-ever]].
    /// The persona doesn't reach "hosted" state if her adapter
    /// refuses to warm; better fail-loud at boot than degrade
    /// silently at first turn.
    async fn warmup(&self) -> Result<(), String> {
        Ok(())
    }

    /// Shutdown the adapter.
    ///
    /// Default: `Ok(())` — adapters holding no releasable resources opt
    /// out. Adapters owning a model handle, socket, or worker process
    /// override to release it.
    async fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }

    // ─── Text Generation ────────────────────────────────────────────────────

    /// Generate text (convenience drain over [`generate_stream`])
    /// Handles both plain text generation AND tool calling
    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String>;

    /// Generate, delivering each [`GenerationChunk`] to `sink` the INSTANT the
    /// backend produces it, and returning the fully-assembled response when the
    /// stream completes. This is the streaming primitive — low-latency, buffered,
    /// the shape every UI / audio / video / live-cognition path wants. The
    /// blocking [`generate_text`] is the drain over this: await the whole answer
    /// when you don't need the tokens live.
    ///
    /// `sink` is an unbounded channel so a slow consumer never stalls token
    /// decode; a consumer that only wants the final answer passes a sink it
    /// drops/ignores (the chunks are cheap to discard).
    ///
    /// Default impl: a NON-incremental adapter (a cloud one-shot endpoint, the
    /// heuristic test adapter) genuinely has nothing to stream — it produces the
    /// whole answer at once. It honestly emits that as a single trailing chunk
    /// then returns the same response. This is a capability statement, not a
    /// fallback that hides a failure: there is no partial output to deliver.
    /// Streaming backends (OpenAI-compatible / llama-server) override this with a
    /// real token-by-token stream and reimplement `generate_text` on top of it.
    async fn generate_stream(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, String> {
        let response = self.generate_text(request).await?;
        if let Some(reasoning) = response.reasoning.as_ref() {
            if !reasoning.is_empty() {
                let _ = sink.send(GenerationChunk::Reasoning(reasoning.clone()));
            }
        }
        if !response.text.is_empty() {
            let _ = sink.send(GenerationChunk::Token(response.text.clone()));
        }
        Ok(response)
    }

    // ─── Embeddings (optional) ──────────────────────────────────────────────

    /// Create embeddings (optional - not all providers support this)
    async fn create_embedding(
        &self,
        _request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        Err(format!("{} does not support embeddings", self.name()))
    }

    // ─── Health & Metadata ──────────────────────────────────────────────────

    /// Check provider health.
    ///
    /// Default: `HealthStatus::healthy()` — in-process / local / test
    /// adapters with no remote endpoint to probe are nominally healthy
    /// once constructed. Cloud adapters override to probe their endpoint
    /// and report real latency / error-rate / rate-limit state.
    async fn health_check(&self) -> HealthStatus {
        HealthStatus::healthy()
    }

    /// Get available models from this provider.
    ///
    /// Default: empty — a minimal adapter advertises no model catalog
    /// (callers use `default_model`). Adapters with a real catalog
    /// (cloud `/v1/models`, DMR) override with their live list. Note
    /// `ModelInfo` has no defaults by design, so the honest default here
    /// is "no catalog," not a synthesized entry.
    async fn get_available_models(&self) -> Vec<ModelInfo> {
        Vec::new()
    }

    /// Get metadata for a specific model by ID.
    /// Returns the ModelInfo with ALL required fields (context_window,
    /// tokens_per_second, cost, capabilities). The adapter is the authority
    /// on its own models — no lookup tables, no guessing.
    fn model_metadata(&self, model_id: &str) -> Option<ModelInfo> {
        // Default: search available_models synchronously from cached list.
        // Adapters with runtime catalogs (DMR, cloud /v1/models) should
        // override this with their live data.
        None // Adapters MUST override — None means "I don't know my own models"
    }

    /// Check if this adapter's model declares a capability. Resolves through
    /// the ONE vocabulary — `adapter.capabilities().has(Capability::X)`.
    fn supports(&self, capability: Capability) -> bool {
        self.capabilities().has(capability)
    }

    // ─── LoRA Capabilities ─────────────────────────────────────────────────────
    // These methods enable fine-tuning/adapter support across providers.
    // Cloud providers may support single adapters (Together, Fireworks).
    // Local Candle supports full multi-layer paging.

    /// Get LoRA capabilities for this adapter
    fn lora_capabilities(&self) -> LoRACapabilities {
        LoRACapabilities::None
    }

    /// Apply a LoRA adapter (for adapters that support it)
    /// Cloud providers: Sets the active fine-tuned model
    /// Local Candle: Activates the adapter (may require model rebuild)
    async fn apply_lora(&self, _adapter_id: &str) -> Result<(), String> {
        Err(format!("{} does not support LoRA", self.name()))
    }

    /// Remove/deactivate a LoRA adapter
    async fn remove_lora(&self, _adapter_id: &str) -> Result<(), String> {
        Err(format!("{} does not support LoRA", self.name()))
    }

    /// List available LoRA adapters
    fn list_lora_adapters(&self) -> Vec<LoRAAdapterInfo> {
        vec![]
    }

    // ─── Device & Capability Routing ─────────────────────────────────────────
    // Adapters declare their device class (GPU/CPU/Cloud) and what model
    // prefixes they support. AdapterRegistry::select() uses both to pick
    // the best match for the caller's request.

    /// What device class does this adapter run on?
    ///
    /// - Gpu: Metal, CUDA, ROCm, Vulkan — hardware-accelerated inference.
    ///   Docker Model Runner, llama.cpp-metal, llama-vulkan all return Gpu.
    /// - Cpu: Candle CPU inference. Only selected when explicitly requested
    ///   (training pipelines) or when CONTINUUM_ALLOW_CPU_INFERENCE is set.
    /// - Cloud: API-based providers (Anthropic, OpenAI, etc.) — not local
    ///   compute at all. Always eligible regardless of device preference
    ///   because they don't consume local resources.
    ///
    /// Default: Gpu. Override in CPU-only adapters (Candle).
    fn device_type(&self) -> InferenceDevice {
        InferenceDevice::Gpu
    }

    /// Get model name prefixes this adapter supports.
    /// Used by AdapterRegistry to auto-route requests based on model name.
    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        vec![] // Default: no auto-routing by model name
    }

    /// Check if this adapter can handle a specific model by name.
    /// Default implementation checks supported_model_prefixes().
    fn supports_model(&self, model_name: &str) -> bool {
        let model_lower = model_name.to_lowercase();
        self.supported_model_prefixes()
            .iter()
            .any(|prefix| model_lower.starts_with(prefix))
    }

    /// Whether this adapter is suitable for serving PRODUCTION inference
    /// traffic — i.e. real cognition for personas talking to users.
    ///
    /// Per [[no-fallbacks-ever]] and [[no-if-statements-use-llms-for-cognition]]:
    /// the substrate NEVER silently substitutes a non-production-capable
    /// adapter for a production-capable one. Heuristic / canned /
    /// pattern-matching adapters return `false` here; the production
    /// selector (`AdapterRegistry::select_production`) hard-errors with a
    /// diagnostic instead of degrading.
    ///
    /// Joel (2026-06-01): "We don't build fucking if statements we use
    /// LLMs!" and "No fallbacks ever it's forbidden." HeuristicInferenceAdapter
    /// exists for CI, debug, replay, and similar non-production contexts —
    /// the substrate is RUINED if those outputs ever serve real personas.
    ///
    /// Default: `true`. Override and return `false` ONLY for adapters whose
    /// outputs are not genuine model inference.
    fn is_production_capable(&self) -> bool {
        true
    }
}

/// Reason no eligible adapter was found by `AdapterRegistry::select_production`.
///
/// Per [[no-fallbacks-ever]] the substrate refuses to substitute a lesser
/// adapter; instead it returns this error with enough context for the
/// caller to surface a diagnosable failure (which model, which device, what
/// IS registered, what's the remediation). The selector NEVER falls back to
/// a non-production-capable adapter or to a different device class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterSelectionError {
    /// No production-capable adapter is registered that satisfies the
    /// device + model constraints. Carries the registered-adapter list so
    /// the error message can name what IS available and what's missing.
    NoEligibleProductionAdapter {
        requested_model: Option<String>,
        requested_device: InferenceDevice,
        preferred_provider: Option<String>,
        registered_providers: Vec<String>,
        /// `true` if a HeuristicInferenceAdapter (or similar non-production
        /// adapter) IS registered but was filtered out. Surfaces the
        /// "you're not falling back to it for a reason" diagnosis.
        non_production_adapters_present: bool,
    },
}

impl std::fmt::Display for AdapterSelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoEligibleProductionAdapter {
                requested_model,
                requested_device,
                preferred_provider,
                registered_providers,
                non_production_adapters_present,
            } => {
                write!(f, "no production-capable adapter found for ")?;
                if let Some(p) = preferred_provider {
                    write!(f, "preferred_provider='{}' ", p)?;
                }
                if let Some(m) = requested_model {
                    write!(f, "model='{}' ", m)?;
                }
                write!(f, "device={:?}. ", requested_device)?;
                if registered_providers.is_empty() {
                    write!(f, "No adapters are registered. ")?;
                } else {
                    write!(
                        f,
                        "Registered production adapters: {:?}. ",
                        registered_providers
                    )?;
                }
                if *non_production_adapters_present {
                    write!(
                        f,
                        "A non-production adapter (heuristic / canned) IS registered \
                         but the substrate refuses to substitute it for a real model \
                         (per no-fallbacks doctrine). "
                    )?;
                }
                write!(
                    f,
                    "Remediation: install/configure a real-model adapter that supports \
                     this model+device, or route this request through `select()` if \
                     it's a CI/debug context that legitimately wants a non-production \
                     adapter."
                )?;
                Ok(())
            }
        }
    }
}

impl std::error::Error for AdapterSelectionError {}

/// Registry of AI provider adapters.
///
/// Stores `Arc<dyn AIProviderAdapter>` so the substrate's shared
/// adapter ownership (supervisor + service_loop + cognition layer
/// + future shared-base + LoRA paging #122 all see the same
/// instance) maps cleanly into the registry.
///
/// **The registry is storage + lookup. It is NOT lifecycle.** The
/// caller initializes adapters BEFORE registering them
/// (init-then-register pattern). The adapter is "ready" the moment
/// it reaches the registry. Shutdown happens when the Arc's last
/// holder drops it; no registry-side `shutdown_all` is needed.
/// This is the elegant intentional architecture Joel called for
/// 2026-06-03 — no Box→Arc shim hacks, no &mut self lifecycle
/// methods accessed through shared handles.
pub struct AdapterRegistry {
    adapters: std::collections::HashMap<String, Arc<dyn AIProviderAdapter>>,
    priority_order: Vec<String>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: std::collections::HashMap::new(),
            priority_order: Vec::new(),
        }
    }

    /// Register an already-initialized adapter with a priority (lower
    /// = higher priority). The caller is responsible for calling
    /// `adapter.initialize()` BEFORE wrapping in `Arc::new` and
    /// passing here; the registry trusts that registered adapters
    /// are ready to serve.
    pub fn register(&mut self, adapter: Arc<dyn AIProviderAdapter>, priority: usize) {
        let id = self.registration_key(adapter.provider_id());

        // Insert into priority order
        if priority >= self.priority_order.len() {
            self.priority_order.push(id.clone());
        } else {
            self.priority_order.insert(priority, id.clone());
        }

        self.adapters.insert(id, adapter);
    }

    fn registration_key(&self, provider_id: &str) -> String {
        if !self.adapters.contains_key(provider_id) {
            return provider_id.to_string();
        }
        let mut i = 2;
        loop {
            let candidate = format!("{provider_id}#{i}");
            if !self.adapters.contains_key(&candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    /// Drop an adapter from the registry. Mirror of `register`. The
    /// hot-swap lever for adapters whose health is dynamic (e.g. DMR
    /// when Docker Desktop crashes — see `DmrWatchdog`). Returns true
    /// if the adapter was registered, false if it wasn't present.
    /// Removes from both the adapters map AND the priority_order vec
    /// so a subsequent `available()` / `select()` reflects reality.
    /// Caller is responsible for invoking `adapter.shutdown()` first
    /// if there's per-adapter cleanup to do; this method drops the
    /// boxed adapter (Drop impl runs).
    pub fn deregister(&mut self, provider_id: &str) -> bool {
        let keys: Vec<String> = self
            .adapters
            .iter()
            .filter_map(|(key, adapter)| {
                if key == provider_id || adapter.provider_id() == provider_id {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();
        let removed = !keys.is_empty();
        if removed {
            for key in &keys {
                self.adapters.remove(key);
            }
            self.priority_order.retain(|id| !keys.contains(id));
        }
        removed
    }

    /// True if the given provider_id is currently registered. Cheap
    /// HashMap lookup. Used by health-watchdogs to decide whether they
    /// need to register or deregister on a probe state change.
    pub fn is_registered(&self, provider_id: &str) -> bool {
        self.adapters
            .iter()
            .any(|(key, adapter)| key == provider_id || adapter.provider_id() == provider_id)
    }

    /// Get adapter by provider ID.
    pub fn get(&self, provider_id: &str) -> Option<&dyn AIProviderAdapter> {
        self.adapters
            .get(provider_id)
            .map(|a| a.as_ref())
            .or_else(|| {
                self.priority_order.iter().find_map(|key| {
                    self.adapters
                        .get(key)
                        .filter(|adapter| adapter.provider_id() == provider_id)
                        .map(|a| a.as_ref())
                })
            })
    }

    /// Get adapter by provider ID as `Arc` — for callers that need
    /// to keep a reference past the registry lock's scope (cognition
    /// layer's evaluate_response holds the Arc across the inference
    /// call so the read lock can drop). Cheap reference count bump.
    pub fn get_arc(&self, provider_id: &str) -> Option<Arc<dyn AIProviderAdapter>> {
        self.adapters.get(provider_id).cloned().or_else(|| {
            self.priority_order.iter().find_map(|key| {
                self.adapters
                    .get(key)
                    .filter(|adapter| adapter.provider_id() == provider_id)
                    .cloned()
            })
        })
    }

    /// Get available adapters (those that initialized successfully)
    pub fn available(&self) -> Vec<&str> {
        self.priority_order
            .iter()
            .filter_map(|id| self.adapters.get(id).map(|_| id.as_str()))
            .collect()
    }

    /// Select best adapter based on request.
    ///
    /// Per [[no-fallbacks-ever]] (Joel, 2026-06-01: "No fallbacks ever
    /// it's forbidden."): if the caller specifies neither `model` nor
    /// `preferred_provider`, this is auto-discovery without any specifier
    /// — the textbook leak path that lets fake adapters silently serve
    /// production traffic. We refuse it and return `None` with a warning.
    /// Callers MUST specify at least one of: which provider, or which
    /// model. The substrate's role is to honor that intent precisely,
    /// not to guess.
    ///
    /// Device-aware routing (like PyTorch device='cuda' / Android MediaCodec):
    /// - `device = Gpu`: only GPU-capable adapters (DMR, llama-metal, llama-vulkan).
    ///   Hard error if no GPU adapter supports the model. DEFAULT.
    /// - `device = Cpu`: only CPU-capable adapters (Candle). Explicit opt-in for
    ///   training/LoRA. Never auto-selected for chat.
    /// - `device = Auto`: try GPU first, CPU fallback WITH warning. RESERVED —
    ///   not implemented yet, behaves as Gpu until we trust the CPU path.
    ///
    /// Explicit `preferred_provider` always wins regardless of device.
    /// Cloud providers (Anthropic, OpenAI, etc.) are always eligible — they're
    /// not local compute, so device preference doesn't apply.
    pub fn select<'a>(
        &'a self,
        preferred_provider: Option<&str>,
        model: Option<&str>,
        device: InferenceDevice,
    ) -> Option<(&'a str, &'a dyn AIProviderAdapter)> {
        // 0. No-specifier guard. Auto-discovery without ANY specifier is
        // the silent-substitution path forbidden by [[no-fallbacks-ever]].
        // Caller must say what they want.
        if preferred_provider.is_none() && model.is_none() {
            clog_warn!(
                "AdapterRegistry::select called with no preferred_provider AND no model. \
                 Auto-discovery without a specifier is forbidden per the no-fallbacks doctrine \
                 — caller MUST specify which provider or which model they want. \
                 Registered: {:?}.",
                self.available()
            );
            return None;
        }

        // 1. Explicit provider — bypass routing for NAMED adapters.
        //    Special case: "local" means "best available local GPU adapter"
        //    — NOT a specific adapter name. Drops through to device-filtered
        //    auto-selection (tier 3) with the requested model. This is how
        //    local personas get DMR when available, Vulkan when not, and
        //    hard-error when neither can serve the model.
        if let Some(pref) = preferred_provider {
            if pref != "local" {
                for key in &self.priority_order {
                    if let Some(adapter) = self.adapters.get(key) {
                        if key == pref || adapter.provider_id() == pref {
                            if model.map_or(true, |m| adapter.supports_model(m)) {
                                return Some((adapter.provider_id(), adapter.as_ref()));
                            }
                        }
                    }
                }
                clog_warn!(
                    "Provider '{}' explicitly requested but not available. Available: {:?}",
                    pref,
                    self.available()
                );
                return None;
            }
            // "local" — fall through to device-filtered auto-selection below
        }

        // 2. Registry-driven cloud routing (#70). A model's provider is a
        // registry FACT — look it up instead of re-guessing it from the name
        // prefix. Only CLOUD providers short-circuit here: they're always
        // eligible (no local device cost), whereas local models must fall
        // through to tier-3 device-filtered selection. An UNREGISTERED model
        // name resolves to nothing and falls through too — we never route by
        // guessing a provider from an unmodeled name (that was the smell).
        // `try_global()` (not `global()`): consult the registry IF it's up.
        // This is not a fallback — tier 2 asks "is this a registered CLOUD
        // model?", and "registry not yet initialized" is materially the same
        // answer as "not a registered cloud model": proceed to tier 3. In
        // production the registry is always booted in backend_init before any
        // adapter selects, so the None branch here is exclusively the bare
        // unit-test case; panicking via global() would couple selection to
        // global boot for no gain.
        if let Some(model_name) = model {
            if let Some(reg) = crate::model_registry::try_global() {
                if let Some(spec) = reg.model(model_name) {
                    let is_cloud =
                        reg.provider(&spec.provider).map(|p| p.kind) == Some(ProviderKind::Cloud);
                    if is_cloud {
                        if let Some(adapter) = self.get(&spec.provider) {
                            return Some((spec.provider.as_str(), adapter));
                        }
                    }
                }
            }
        }

        // 3. Device-filtered local adapter selection.
        // Walk priority order; only consider adapters whose device_type
        // matches the request. GPU adapter that honestly supports the model
        // wins. No silent cross-device fallback.
        let device_matches = |adapter_device: InferenceDevice| -> bool {
            match device {
                InferenceDevice::Gpu => adapter_device == InferenceDevice::Gpu,
                InferenceDevice::Cpu => adapter_device == InferenceDevice::Cpu,
                InferenceDevice::Auto => true, // future: GPU-first then CPU
            }
        };

        for id in &self.priority_order {
            if let Some(adapter) = self.adapters.get(id) {
                if !device_matches(adapter.device_type()) {
                    continue; // wrong device class — skip, don't fallback
                }
                // If model specified, adapter must honestly support it.
                // If no model specified, any adapter on the right device works.
                if model.map_or(true, |m| adapter.supports_model(m)) {
                    return Some((adapter.provider_id(), adapter.as_ref()));
                }
            }
        }

        // No adapter matched. Fail loud.
        if let Some(model_name) = model {
            clog_warn!(
                "No {:?}-device adapter supports model '{}'. Registered: {:?}. Pull model into DMR: `docker model pull {}`, or install the right GPU backend.",
                device,
                model_name,
                self.available(),
                model_name
            );
        } else {
            clog_warn!(
                "No {:?}-device adapter available. Registered: {:?}.",
                device,
                self.available()
            );
        }
        None
    }

    // Note: `initialize_all` and `shutdown_all` were removed in task
    // #162 alongside the Box→Arc registry migration. The registry's
    // job is storage + lookup, NOT lifecycle. Callers initialize
    // adapters before registering (init-then-register pattern) and
    // adapter cleanup happens when the Arc's last holder drops the
    // adapter. Per [[init-once-handle-then-lease-zero-copy-refs]]:
    // init at boot, lease per turn, drop at end-of-life.
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ArcAdapterShim + register_arc were deleted in task #162's Box→Arc
// migration. The registry stores Arc natively; callers pass
// `Arc::new(adapter)` directly to `register`. The shim was a
// transitional wrapper; the elegant intentional architecture is the
// Arc-native registry above.

#[cfg(test)]
mod tests {
    //! Registry hot-swap tests. Verify that deregister removes from BOTH
    //! the adapters map AND the priority_order vec — drift between the
    //! two would leave a phantom in `available()` after deregister, which
    //! is exactly the bug a DMR watchdog needs to NOT have.
    use super::*;
    use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};

    /// Minimal adapter for registry-shape tests. Doubles as the live
    /// proof of the trait's default impls: it implements ONLY the four
    /// required methods (provider_id, name, default_model, generate_text)
    /// plus `supports_model` for the model-routing test — everything else
    /// (capabilities, api_style, initialize, shutdown, health_check,
    /// get_available_models, device_type) comes from the trait defaults.
    /// If those defaults regress, this stops compiling or the
    /// `minimal_adapter_inherits_sensible_defaults` test below fails.
    struct StubAdapter {
        id: String,
        model: Option<String>,
    }

    #[async_trait]
    impl AIProviderAdapter for StubAdapter {
        fn provider_id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            &self.id
        }
        fn default_model(&self) -> &str {
            "stub"
        }
        async fn generate_text(
            &self,
            _r: TextGenerationRequest,
        ) -> Result<TextGenerationResponse, String> {
            Err("stub adapter — no inference".into())
        }
        fn supports_model(&self, model: &str) -> bool {
            self.model.as_deref().map_or(true, |m| m == model)
        }
    }

    fn stub(id: &str) -> Arc<dyn AIProviderAdapter> {
        Arc::new(StubAdapter {
            id: id.to_string(),
            model: None,
        })
    }

    fn stub_model(id: &str, model: &str) -> Arc<dyn AIProviderAdapter> {
        Arc::new(StubAdapter {
            id: id.to_string(),
            model: Some(model.to_string()),
        })
    }

    #[test]
    fn deregister_removes_from_both_map_and_priority_order() {
        let mut r = AdapterRegistry::new();
        r.register(stub("dmr"), 0);
        r.register(stub("vulkan"), 1);
        r.register(stub("cloud"), 2);

        assert!(r.is_registered("dmr"));
        assert!(r.deregister("dmr"));
        assert!(!r.is_registered("dmr"));

        let available = r.available();
        assert!(
            !available.contains(&"dmr"),
            "dmr must be gone from available()"
        );
        assert!(available.contains(&"vulkan"));
        assert!(available.contains(&"cloud"));
    }

    #[test]
    fn deregister_returns_false_for_unknown_adapter() {
        let mut r = AdapterRegistry::new();
        r.register(stub("vulkan"), 0);
        assert!(!r.deregister("nonexistent"));
        assert!(r.is_registered("vulkan"));
    }

    #[test]
    fn register_after_deregister_restores_full_state() {
        // The DMR watchdog hot-swap path: deregister on Docker crash,
        // re-register when Docker comes back. Must work cleanly across
        // many cycles without leaking phantom state.
        let mut r = AdapterRegistry::new();
        for _ in 0..5 {
            r.register(stub("dmr"), 0);
            assert!(r.is_registered("dmr"));
            assert!(r.deregister("dmr"));
            assert!(!r.is_registered("dmr"));
        }
        // Final cycle leaves it unregistered.
        assert_eq!(r.available().len(), 0);
    }

    #[test]
    fn duplicate_provider_ids_remain_independently_selectable_by_model() {
        let mut r = AdapterRegistry::new();
        r.register(stub_model("llamacpp-local", "qwen3.5"), 0);
        r.register(stub_model("llamacpp-local", "qwen2-vl"), 0);

        assert_eq!(r.available().len(), 2);
        assert!(r.is_registered("llamacpp-local"));

        let (_, qwen35) = r
            .select(Some("local"), Some("qwen3.5"), InferenceDevice::Gpu)
            .expect("qwen3.5 adapter selected");
        assert_eq!(qwen35.default_model(), "stub");
        assert!(qwen35.supports_model("qwen3.5"));
        assert!(!qwen35.supports_model("qwen2-vl"));

        let (_, qwen2) = r
            .select(Some("local"), Some("qwen2-vl"), InferenceDevice::Gpu)
            .expect("qwen2-vl adapter selected");
        assert!(qwen2.supports_model("qwen2-vl"));
        assert!(!qwen2.supports_model("qwen3.5"));
    }

    // what this catches: regression in the AIProviderAdapter default impls.
    // A minimal adapter (4 required methods) must inherit sensible defaults
    // for the long tail so new providers + test fixtures don't boilerplate
    // 10 methods (cv::Algorithm anti-pattern — common case must be trivial).
    #[tokio::test]
    async fn minimal_adapter_inherits_sensible_defaults() {
        let mut a = StubAdapter {
            id: "minimal".to_string(),
            model: None,
        };

        // Lifecycle defaults are no-op-Ok.
        assert!(a.initialize().await.is_ok());
        assert!(a.shutdown().await.is_ok());

        // api_style defaults to local in-process inference.
        assert_eq!(a.api_style(), ApiStyle::Local);

        // capabilities default to a text-only set — one vocabulary.
        let caps = a.capabilities();
        assert!(caps.has(Capability::TextGeneration));
        assert!(caps.has(Capability::Chat));
        assert!(!caps.has(Capability::ToolUse));
        assert!(!caps.has(Capability::Vision));

        // health defaults to nominal-healthy (no remote probe).
        let health = a.health_check().await;
        assert_eq!(health.status, crate::ai::types::HealthState::Healthy);
        assert!(health.api_available);

        // no advertised catalog by default — callers use default_model.
        assert!(a.get_available_models().await.is_empty());

        // device defaults to GPU; production-capable unless opted out.
        assert_eq!(a.device_type(), InferenceDevice::Gpu);
        assert!(a.is_production_capable());
    }

    // what this catches: the codified `capabilities()` projection regressing —
    // builder() must seed the text-only floor (so an adapter declares only what
    // it adds), and each NativeProtocols variant must map to its coherent
    // (tool_call, structured_output) pair. If someone re-splits the protocol
    // pair or changes the floor, the next adapter silently gets a wrong/
    // incoherent surface; this pins both.
    #[test]
    fn builder_seeds_floor_and_native_protocols_pair_coherently() {
        // builder() with no overrides == the text-only floor.
        let floor = AdapterCapabilities::builder().build();
        assert_eq!(
            floor.capabilities,
            AdapterCapabilities::text_only().capabilities
        );
        assert!(floor.has(Capability::TextGeneration) && floor.has(Capability::Chat));
        assert!(!floor.has(Capability::ToolUse));
        assert!(!floor.is_local);
        assert_eq!(floor.tool_call_protocol, ToolProtocol::None);
        assert_eq!(
            floor.structured_output_protocol,
            StructuredOutputProtocol::None
        );

        // A rich declaration adds only the deltas on top of the floor.
        let rich = AdapterCapabilities::builder()
            .capabilities([
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
            ])
            .local()
            .context_window(200_000)
            .max_output_tokens(8_192)
            .protocols(NativeProtocols::FunctionCalling)
            .build();
        assert!(rich.has(Capability::ToolUse) && rich.is_local);
        assert_eq!(rich.max_context_window, Some(200_000));
        assert_eq!(rich.max_output_tokens, Some(8_192));

        // Each protocol profile maps to its coherent pair — the whole point of
        // NativeProtocols (an incoherent combo is unrepresentable).
        for (profile, tool, structured) in [
            (
                NativeProtocols::None,
                ToolProtocol::None,
                StructuredOutputProtocol::None,
            ),
            (
                NativeProtocols::PromptEmulated,
                ToolProtocol::None,
                StructuredOutputProtocol::PromptOnly,
            ),
            (
                NativeProtocols::FunctionCalling,
                ToolProtocol::NativeFunctionCalling,
                StructuredOutputProtocol::JsonSchema,
            ),
            (
                NativeProtocols::GrammarConstrained,
                ToolProtocol::JsonInPrompt,
                StructuredOutputProtocol::GrammarConstrained,
            ),
        ] {
            assert_eq!(profile.tool_call(), tool, "{profile:?} tool half");
            assert_eq!(
                profile.structured_output(),
                structured,
                "{profile:?} structured half"
            );
        }
    }
}
