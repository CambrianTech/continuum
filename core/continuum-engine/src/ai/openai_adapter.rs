//! OpenAI-Compatible Adapter - Handles providers using OpenAI's API format
//!
//! Many providers use OpenAI's API format, so we can share 95% of the code:
//! ✅ OpenAI (official)
//! ✅ DeepSeek
//! ✅ Together AI
//! ✅ Groq
//! ✅ Fireworks AI
//! ✅ XAI (Grok)
//! ✅ Google (Gemini via OpenAI-compatible endpoint)
//!
//! Only differences:
//! - API base URL
//! - API key
//! - Available models
//! - Pricing

use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::time::Instant;

use crate::model_registry::{AuthKind, Capability};
use continuum_base::secrets::get_secret;
use crate::{clog_info, clog_warn};

use super::adapter::{
    AIProviderAdapter, AdapterCapabilities, ApiStyle, GenerationChunk, LoRACapabilities,
};
use super::openai_endpoints::OpenAiBase;
use super::registry_bridge::models_for_provider_via_registry;
use super::types::{
    ActiveAdapterRequest, ContentPart, EmbeddingInput, EmbeddingRequest, EmbeddingResponse,
    FinishReason, GenerationTiming, HealthState, HealthStatus, ModelInfo, TextGenerationRequest,
    TextGenerationResponse, ToolCall, UsageMetrics,
};

/// Runtime-resolved config carried by each `OpenAICompatibleAdapter`
/// instance. Populated exclusively by `OpenAICompatibleAdapter::from_registry`
/// — no hand-written literals. Fields that the registry doesn't know
/// about (HTTP concerns — auth shape, Authorization header requirement)
/// are derived from `Provider.auth`, not separately configured.
/// Per-gateway thinking policy for reasoning models. The gateway can't always
/// honor `chat_template_kwargs.enable_thinking` (verified: llama.cpp
/// ignores it for this forged model), but Qwen3's `/no_think` SOFT-SWITCH in the
/// message works — the model emits an empty `<think></think>` then answers
/// directly. This is the model-specific knob the adapter owns (same boundary as
/// reasoning separation); higher layers express a model-agnostic intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingMode {
    /// Leave the model at its default — reasoning models think every turn. Output
    /// is still reasoning-stripped by [`extract_reasoning`] (the safety net).
    #[default]
    Default,
    /// Suppress chain-of-thought via the model's soft-switch. Faster turns, and the
    /// runaway-loop failure mode can't happen (no reasoning is generated). For a
    /// small reasoning model whose "thinking" tends to ramble, this is usually the
    /// better default; a task that genuinely needs deliberation re-enables it.
    Suppress,
}

#[derive(Debug, Clone)]
pub struct OpenAICompatibleConfig {
    pub provider_id: String,
    pub name: String,
    pub base_url: String,
    pub api_key_env: Option<String>,
    pub default_model: String,
    /// What this provider's models can do — projected into the ONE capability
    /// vocabulary (#65). Tool-use + vision come from scanning the provider's
    /// models; embeddings + image-gen come from the provider's declared
    /// `ProviderCapabilities`. The adapter CONSUMES this set via
    /// `capabilities.contains(Capability::X)` — it never branches on
    /// `provider.id`, and there is no bool mirror.
    pub capabilities: BTreeSet<Capability>,
    pub models: Vec<ModelInfo>,
    pub model_prefixes: Vec<String>,
    /// Whether this provider requires an Authorization header. Derived
    /// from `Provider.auth`: Bearer → true, ApiKey → true, None → false.
    pub requires_auth: bool,
    /// How this provider exchanges tool calls. NativeFunctionCalling = real
    /// OpenAI function-calling; JsonInPrompt = describe tools in the prompt +
    /// parse the JSON call from the response (for gateways/models that ignore
    /// the `tools` param, e.g. a local GGUF — proven 2026-06-21). Stored
    /// verbatim from the registry `Provider.capabilities.tool_protocol` (#69,
    /// the ONE `model_registry::ToolProtocol`) — never an `id == "..."` branch,
    /// never a per-adapter mirror. Gateway-level for now; per-model refinement
    /// is the follow-up.
    pub tool_protocol: crate::model_registry::ToolProtocol,
    /// Whether to suppress the model's chain-of-thought for this gateway (Qwen3
    /// `/no_think` soft-switch). Sourced from `Provider.capabilities
    /// .suppress_thinking` (#55). Gateway-level for now; per-task/per-request
    /// refinement is the follow-up.
    pub thinking: ThinkingMode,
    /// This endpoint serves ONE resident model and ignores the request's
    /// `model` field, so the adapter must pre-flight model activation before
    /// each generation. From `Provider.capabilities.single_resident_model`
    /// (#55) — replaces a per-id branch in `generate_text`.
    pub single_resident_model: bool,
    /// This endpoint has a DYNAMIC `/v1/models` catalog with ids that differ
    /// from the registry's logical ids (DMR's `hf.co/…:latest` mangling), so
    /// the adapter fetches the live catalog at init, resolves logical→live ids
    /// per POST, and answers `supports_model` from the live set. From
    /// `Provider.capabilities.dynamic_model_catalog` (#55) — replaces the
    /// `id == "docker-model-runner"` branches in `initialize`, `generate_text`,
    /// and `supports_model`.
    pub dynamic_model_catalog: bool,
    /// This llama.cpp-family endpoint accepts native sampling extension fields
    /// (`repeat_penalty`) beyond the OpenAI body, so the adapter forwards them
    /// to stop the forged 4B looping. From
    /// `Provider.capabilities.llamacpp_sampling_extensions` (#55) — replaces
    /// the `id == "docker-model-runner"` `repeat_penalty` branch.
    pub llamacpp_sampling_extensions: bool,
    /// The server accepts `tail_tools`. From `Provider.capabilities.tail_tools`.
    pub tail_tools: bool,
}

/// What the serving backend told us when we asked it `GET /lora-adapters`.
///
/// The organism DISCOVERS whether it can page a LoRA in by *asking the
/// endpoint* — we never hardcode "provider X supports LoRA, provider Y does
/// not." A llama.cpp `llama-server` answers 200 with the array of adapters it
/// loaded at launch (each carrying the integer load-index the per-request
/// `"lora":[{"id":N,"scale":S}]` field references); a cloud API or
/// `mlx_lm.server` (whose `--adapter-path` no-ops) answers 404. We cache the
/// 404 as `Unsupported` so we don't re-probe a backend that can't do it every
/// turn, and cache the 200 as `Supported` so name→id resolution is a local
/// lookup. A transient connection error is NOT cached — a dead server is not
/// the same as a server that has no LoRA support.
#[derive(Debug, Clone)]
enum LoraSupport {
    /// Never probed.
    Unknown,
    /// Endpoint answered 404/501 — this serving backend can't page LoRA per-request.
    Unsupported,
    /// Endpoint answered 200 — `(server load-index, adapter path)` for each loaded adapter.
    Supported(Vec<(i64, String)>),
}

/// OpenAI-compatible adapter implementation
pub struct OpenAICompatibleAdapter {
    config: OpenAICompatibleConfig,
    api_key: Option<String>,
    /// Runtime base URL set via `with_runtime_base_url` — overrides
    /// `config.base_url` without mutating the registry-sourced config.
    /// Used when DMR reaches us at `model-runner.docker.internal` instead
    /// of `localhost:12434` (detected by `probe_dmr`).
    runtime_base_url: Option<String>,
    /// This adapter OWNS a dedicated, single-purpose serving lane (an
    /// `EphemeralServingLane` the eval spawned) rather than sharing the global
    /// gateway. Set via [`with_dedicated_lane`]. When true, the single-resident
    /// pre-flight guard trusts THIS lane (readiness was guaranteed at spawn — the
    /// lane was launched with exactly this model and `EphemeralServingLane::spawn`
    /// blocks until HTTP-ready) instead of consulting the GLOBAL serving snapshot,
    /// which only ever knows the living persona lane's model. Without this, a
    /// humane eval (#59) on a forged-4b copy is refused because the global snapshot
    /// reports the live 14B — the wrong authority for a lane the eval owns.
    /// Slot behavior is unchanged: a dedicated lane is still single-resident.
    dedicated_lane: bool,
    client: reqwest::Client,
    initialized: bool,
    /// Live model catalog, populated from the server's /v1/models endpoint
    /// at init and on-demand refresh. Lets `supports_model()` be HONEST —
    /// for DMR this reflects whatever the user has `docker model pull`ed,
    /// so the registry can route to DMR only when the model is actually
    /// available. Without this, supports_model falls back to static
    /// `supported_model_prefixes()` which for docker-model-runner returned
    /// `[]` → DMR never won routing → every user silently landed on Candle.
    runtime_models: std::sync::Arc<std::sync::RwLock<Option<std::collections::HashSet<String>>>>,
    /// Discovered LoRA page-in capability + catalog, populated by probing the
    /// serving backend's `GET /lora-adapters` (see [`LoraSupport`]). Starts
    /// `Unknown`; the first generation that carries `active_adapters` triggers
    /// the probe. This is the self-organizing alternative to a hardcoded
    /// "which provider supports LoRA" table — the endpoint describes itself.
    lora_support: std::sync::Arc<std::sync::RwLock<LoraSupport>>,
    /// Throttle for concurrent POSTs to this provider's endpoint.
    /// llama.cpp-backed providers (DMR) are single-slot in practice:
    /// one prompt at a time gets the full GPU. Letting N personas
    /// fan-out into N simultaneous POSTs causes each to serialize on
    /// DMR's side while reqwest's 120s client timeout burns. This
    /// semaphore does the same serialization CLIENT-side so requests
    /// wait in an observable queue instead of inside reqwest's
    /// opaque "no response yet" state, and so the adapter's 120s
    /// timeout is measured from "actually reached the server," not
    /// "joined the queue."
    ///
    /// DMR → 1 slot (single-slot llama.cpp backend).
    /// Cloud providers (OpenAI / Groq / etc.) → high slot count (no throttle).
    concurrency: std::sync::Arc<tokio::sync::Semaphore>,
    /// How many permits `concurrency` currently represents — the reconciler's
    /// bookkeeping for tracking the served slot count both directions.
    permit_target: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// One reconciler per adapter instance (spawned on first `initialize`).
    permit_reconciler_started: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Served PER-SLOT context window by server root, captured from the same
/// `/props` probe that discovers slots (`default_generation_settings.n_ctx` —
/// llama-server reports the per-slot share there, already divided by
/// `--parallel`). This is the ground truth the registry row can only claim:
/// a consumer budgeting against the model's trained window while the server
/// slices `-c` across N slots silently overshoots (#139). Keyed per SERVER
/// like the slot directory (`inference::slots`), shared across adapter instances.
fn served_ctx_by_root() -> &'static dashmap::DashMap<String, u32> {
    static CTX: std::sync::OnceLock<dashmap::DashMap<String, u32>> = std::sync::OnceLock::new();
    CTX.get_or_init(dashmap::DashMap::new)
}

// Serving-guard predicates live in `crate::inference::serving_guard` (S3b decompose).

/// Apply the llama.cpp-native sampling knobs to a request body. Pure (no `self`,
/// no I/O) so the wire contract is unit-testable and lives in ONE place —
/// gated by `llamacpp_sampling_extensions` at the call site.
///
/// `repeat_penalty` is always set (defaulting to llama.cpp's 1.1 when the request
/// omits it) because the local gateway otherwise runs with penalty=1.0/disabled and
/// produces runaway repetition. `repeat_last_n` and `frequency_penalty` are the #181
/// anti-loop pair, forwarded ONLY when the request carries them: the sampling layer
/// (SamplingParams/SamplingProfile → TextGenerationRequest) owns the values, this
/// adapter never invents model characteristics — they stay per-model tunable (#76).
///
/// #181 root cause (glass-boxed 2026-07-16, Devstral-24B): `repeat_penalty` alone did
/// NOT stop a reasoning-channel repetition loop — the model re-emitted the same wrong
/// code block ~5×, burning 14k reasoning tokens to the `length` cap and committing an
/// empty answer. llama.cpp's `repeat_penalty` scans only the last `repeat_last_n`
/// tokens (gateway default 64), but the loop's repeat span (code + paragraph + code ≈
/// 150 tok) is WIDER than 64, so the window never sees the recurrence. The pair closes
/// it: `repeat_last_n` widens that window; `frequency_penalty` is the UNWINDOWED guard
/// (scaled by whole-sequence token frequency) that catches gap-separated loops the
/// window still misses. Cloud OpenAI-compat providers reject these fields — hence the
/// capability gate at the call site.
fn apply_llamacpp_sampling_knobs(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    request: &TextGenerationRequest,
) {
    obj.insert(
        "repeat_penalty".to_string(),
        json!(request.repeat_penalty.unwrap_or(1.1)),
    );
    if let Some(rln) = request.repeat_last_n {
        obj.insert("repeat_last_n".to_string(), json!(rln));
    }
    if let Some(fp) = request.frequency_penalty {
        obj.insert("frequency_penalty".to_string(), json!(fp));
    }
}

/// Does this `/props` status PROVE the endpoint does not exist — the only verdict
/// allowed to latch the slot directory Unsupported for the process's life? 404/501
/// are the server saying "no such surface"; everything else (above all the 503 of a
/// model still loading) is a statement about NOW, and a permanent conclusion drawn
/// from a transient state is the [[unknown-is-not-a-quantity]] error with a cache
/// bolted on.
fn props_status_proves_endpoint_absent(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::NOT_IMPLEMENTED
}

mod embedding;
mod lora;
mod provider;
mod reasoning;
mod registry;
mod runtime;
mod stream;

pub(crate) use reasoning::extract_reasoning;

#[cfg(test)]
mod tests;
