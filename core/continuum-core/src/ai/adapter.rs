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
use crate::model_registry::Capability;
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

/// How the adapter ACCEPTS tool-call requests. This is arc 1's pivot
/// insurance: cognition asks "can you do tools?" and the substrate
/// routes accordingly — no special-casing per adapter, no "if openai
/// then ..." branches. Per `[[adapter-pattern-is-the-pivot-insurance]]`.
///
/// The substrate's tool-execution loop reads this and either:
/// 1. Calls the adapter natively (NativeFunctionCalling, JsonMode) and
///    parses the structured response, OR
/// 2. Wraps tool descriptors into the prompt itself (JsonInPrompt,
///    XmlTags) and parses tool calls out of the text output stream
///
/// Adapters declare ONE protocol — the best they natively support.
/// Bridged protocols (e.g., wrapping JsonInPrompt over a base model that
/// could do better) belong in cognition's compose phase, not here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToolCallProtocol {
    /// No tool calling at all — caller must implement tool execution
    /// out-of-band or skip tool use. Pure-text completion adapters
    /// (HeuristicAdapter, embedding-only models).
    #[default]
    None,
    /// Tools described in the system/user prompt as JSON schema; the
    /// model emits JSON in its text output, substrate parses. Works on
    /// any text model with sufficient instruction-following. The
    /// fallback any prompt-driven model can fulfill.
    JsonInPrompt,
    /// Provider's native JSON mode (`response_format = json_object`) —
    /// the model is constrained at sampling time to emit valid JSON.
    /// Stronger guarantee than JsonInPrompt; weaker than function calling.
    JsonMode,
    /// Native function calling primitives — provider returns structured
    /// tool_calls in its API response shape (OpenAI tools, Anthropic
    /// tool_use). The substrate consumes them directly without parsing.
    NativeFunctionCalling,
    /// XML-style tool tags inside text output — Anthropic's pre-tool-use
    /// pattern. Substrate parses `<tool>...</tool>` blocks.
    XmlTags,
}

/// How the adapter ACCEPTS structured-output schemas. Same shape as
/// `ToolCallProtocol` — cognition asks "can you constrain output to
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
    /// Context window (input + output limit).
    pub max_context_window: u32,
    /// Maximum tokens the adapter will emit in a single response. Distinct
    /// from `max_context_window`; used by cognition to bound the compose phase.
    pub max_output_tokens: u32,

    /// Tool-calling protocol the adapter NATIVELY speaks. Cognition's tool
    /// loop routes through this. Default means prompt-text emulation in compose.
    pub tool_call_protocol: ToolCallProtocol,
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
    /// Richer adapters extend the set with struct-update syntax:
    /// ```ignore
    /// AdapterCapabilities {
    ///     capabilities: BTreeSet::from([Capability::TextGeneration, Capability::Chat, Capability::ToolUse]),
    ///     tool_call_protocol: ToolCallProtocol::NativeFunctionCalling,
    ///     ..AdapterCapabilities::text_only()
    /// }
    /// ```
    pub fn text_only() -> Self {
        Self {
            capabilities: BTreeSet::from([Capability::TextGeneration, Capability::Chat]),
            max_context_window: 4096,
            max_output_tokens: 2048,
            ..Default::default()
        }
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

    /// Generate text (main entry point)
    /// Handles both plain text generation AND tool calling
    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String>;

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
                write!(
                    f,
                    "no production-capable adapter found for "
                )?;
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
        self.adapters
            .get(provider_id)
            .cloned()
            .or_else(|| {
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

        // 2. Cloud-provider prefix detection (always eligible regardless of device).
        // These are the well-known cloud API providers whose model names
        // unambiguously identify the provider.
        if let Some(model_name) = model {
            let model_lower = model_name.to_lowercase();
            let cloud_match: Option<&str> = if model_lower.starts_with("claude") {
                Some("anthropic")
            } else if model_lower.starts_with("gpt")
                || model_lower.starts_with("o1")
                || model_lower.starts_with("o3")
            {
                Some("openai")
            } else if model_lower.starts_with("deepseek") {
                Some("deepseek")
            } else if model_lower.starts_with("grok") {
                Some("xai")
            } else if model_lower.starts_with("gemini") {
                Some("google")
            } else {
                None
            };
            if let Some(provider_id) = cloud_match {
                if let Some(adapter) = self.get(provider_id) {
                    return Some((provider_id, adapter));
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
}
