use super::*;

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
    pub(super) inner: AdapterCapabilities,
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

/// Capacity owner for one concrete generation target. This is not `is_local`:
/// an HTTP adapter can reach either our shared engine or an independent endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationAdmissionOwner {
    SharedLocal,
    Adapter,
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

    /// Resolve a mutable routing adapter to one concrete call target before admission.
    /// Static adapters return None: the caller retains its existing Arc. A switch
    /// returns the concrete handle that must also dispatch this call; a later
    /// placement change takes effect on the next call and reacquires admission.
    fn generation_target(&self) -> Option<Arc<dyn AIProviderAdapter>> {
        None
    }

    /// Refresh routing before reading the served window and assembling a prompt.
    /// A placement adapter may materialize its home lane here when its remote
    /// circuit breaker is cold. No generation or admission is held across this
    /// transition; the eventual concrete target owns admission as usual.
    async fn prepare_generation(&self) {}

    /// Admission for the concrete target, independent of API protocol or model name.
    /// Remote, dedicated and in-process adapters own their own capacity. Only the
    /// shared local serving endpoint participates in the node's lane/prefill gates.
    fn generation_admission(&self) -> GenerationAdmissionOwner {
        GenerationAdmissionOwner::Adapter
    }

    /// Get adapter capabilities.
    ///
    /// Default: a text-only set (`AdapterCapabilities::text_only`).
    /// Override to declare tools, vision, audio, streaming, or richer
    /// context/output limits.
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::text_only()
    }

    /// Whether a request for `model` reaches a server that accepts `tail_tools` (callable verbs
    /// outside the rendered head), decided for the destination that request is routed to.
    /// Default `false`: only a server that declares it gets a learned tool head.
    fn accepts_tail_tools(&self, _model: &str) -> bool {
        false
    }

    /// Get API style.
    ///
    /// Default: `Local` (in-process inference). Cloud adapters override
    /// with `OpenAI` / `Anthropic` / `Google`.
    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    /// RESTORE-AHEAD (compression-ladder rung 1, FOLLOW-THE-SIGNAL doc): the
    /// caller is ABOUT to generate for this page (a mind, or a fork) — begin paging its warm
    /// KV state into a serving slot NOW, overlapped with the caller's own
    /// prompt assembly, instead of racing the restore against the turn at pin
    /// time (measured 2026-09-01: pin-time restores queue behind busy slots to
    /// the 10s timeout; the scheduler knowing "she is next" IS the cache
    /// prediction, so use it). Non-blocking: implementations spawn and return.
    ///
    /// Default: no-op — cloud adapters and adapters without slot paging have
    /// nothing to warm.
    fn warm_ahead(&self, _page: crate::inference::slots::PageKey) {}

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

    /// Wait for a semantic capacity observation to change, without generating or
    /// polling. Live adapters subscribe before rechecking; fixed bindings have
    /// no capacity events and are woken by their binding owner instead.
    fn capacity_changed(
        &self,
        _observed: Option<u32>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(std::future::pending())
    }

    /// Get default model for this provider
    fn default_model(&self) -> &str;

    /// The model ids this adapter will ACTUALLY serve right now — what a
    /// refusal may tell a caller to pass. Default: just `default_model()`.
    /// Gateway adapters whose catalog default can drift from the live lane
    /// (llama-server / DMR after a relaunch — the 5090 2026-07-24 misname)
    /// override this with their verified runtime set. Never a guess: an
    /// adapter that cannot know returns its declared default only.
    fn served_model_ids(&self) -> Vec<String> {
        vec![self.default_model().to_string()]
    }

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

    /// Generate with typed transport/admission failures. Backends can migrate
    /// their existing streaming implementation without changing legacy callers
    /// or fabricating a classification for an untyped provider error.
    async fn generate_stream_checked(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, crate::ai::inference_error::InferenceError> {
        self.generate_stream(request, sink)
            .await
            .map_err(Into::into)
    }

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
    /// decode; a consumer that only wants the final answer uses
    /// `stream_sinks::drain_generation` to keep consuming until completion.
    /// Closing a streaming receiver can cancel the generation.
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
        if sink.is_closed() {
            return Ok(response); // No observer: keep the result without cloning discarded chunks.
        }
        if let Some(reasoning) = response.reasoning.as_ref() {
            if !reasoning.is_empty() {
                crate::ai::stream_sinks::deliver(&sink, GenerationChunk::Reasoning(reasoning.clone())).await;
            }
        }
        if !response.text.is_empty() {
            crate::ai::stream_sinks::deliver(&sink, GenerationChunk::Token(response.text.clone())).await;
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
    fn model_metadata(&self, _model_id: &str) -> Option<ModelInfo> {
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
