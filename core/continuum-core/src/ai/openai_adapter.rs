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
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::time::Instant;

use crate::model_registry::{AuthKind, Capability};
use crate::secrets::get_secret;
use crate::{clog_info, clog_warn};

use super::adapter::{
    AIProviderAdapter, AdapterCapabilities, ApiStyle, GenerationChunk, LoRACapabilities,
};
use super::openai_endpoints::OpenAiBase;
use super::registry_bridge::models_for_provider_via_registry;
use super::types::{
    ActiveAdapterRequest, ChatMessage, ContentPart, EmbeddingInput, EmbeddingRequest,
    EmbeddingResponse, FinishReason, GenerationTiming, HealthState, HealthStatus, MessageContent,
    ModelInfo, TextGenerationRequest, TextGenerationResponse, ToolCall, ToolChoice, UsageMetrics,
};

/// Runtime-resolved config carried by each `OpenAICompatibleAdapter`
/// instance. Populated exclusively by `OpenAICompatibleAdapter::from_registry`
/// — no hand-written literals. Fields that the registry doesn't know
/// about (HTTP concerns — auth shape, Authorization header requirement)
/// are derived from `Provider.auth`, not separately configured.
/// Per-gateway thinking policy for reasoning models. The gateway can't always
/// honor `chat_template_kwargs.enable_thinking` (verified: unsloth/llama.cpp
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
    /// the `tools` param, e.g. unsloth+GGUF — proven 2026-06-21). Stored
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
    /// (#55) — replaces the `id == "unsloth"` branch in `generate_text`.
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

/// Why a generation was refused when the requested model is not guaranteed resident, said
/// in the words the caller needs to act on. THREE distinct situations reach this point and
/// only one of them is a fault:
///
/// | snapshot | meaning | caller should |
/// |---|---|---|
/// | never reconciled | core is still starting; nobody has looked yet | retry |
/// | reconciled, `active_model == None` | a lane is being torn down / rebuilt | retry |
/// | `active_model == Some(other)` | a DIFFERENT model is resident | NOT retry |
///
/// Both retry-able cases used to print the fault sentence. The cost is measured twice:
/// 116 false alarms over three days from the startup case (#350), and then — after that
/// split shipped — three citizens taking the same fault sentence 59 seconds into a #175
/// wedge self-heal that completed normally. `ServingSnapshot::empty()` is published by the
/// daemon on EVERY teardown (no servable plan, a re-home, a wedge relaunch), so `<none>` is
/// the ordinary appearance of a lane in transition, not evidence of breakage.
///
/// Pure by construction: takes the snapshot and the latch as arguments rather than reading
/// the process-global `SERVING_STATE`/`FIRST_RECONCILE`, so all three branches are testable
/// without a set-once global that would make test order load-bearing
/// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
/// Does `snap` GUARANTEE that the local single-resident gateway will answer as `model`?
///
/// The gateway serves ONE resident model and answers every request as that model whatever
/// the request's `model` field says, so "guaranteed" means the daemon has PUBLISHED that
/// this exact model is the live one — on the main lane, or on the verified #106 vision
/// sidecar (whose `/props` the daemon checked before publishing `vision_ready`).
///
/// One predicate, two readers: the pre-flight guard below and the post-wait re-check. Written
/// out once so the two can never drift into disagreeing about what "serving our model" means.
fn snapshot_guarantees(
    snap: &crate::inference::llama_server::ServingSnapshot,
    model: &str,
) -> bool {
    snap.ready
        && (snap.active_model.as_deref() == Some(model)
            || (snap.vision_ready && snap.vision_model.as_deref() == Some(model)))
}

/// Is refusing POINTLESS to wait out — i.e. has the daemon SETTLED on a different model?
///
/// A failed guarantee is one of two very different situations, and only one of them is
/// terminal:
///
/// - **Settled mismatch** — some other model is ready and active. The daemon has made its
///   choice; waiting cannot change it, and residency arbitration is the serving layer's job
///   (#109), never a generate's. Refuse immediately and loudly.
/// - **Transition** — no model is resident (`empty()`, published on EVERY teardown: no
///   servable plan, a re-home, a #175 wedge relaunch), or a lane is up but not yet decode-
///   ready. Nothing has failed; the lane is simply mid-flight and comes back on its own.
///
/// Measured 2026-08-07: a wedge self-heal flipped the snapshot not-ready at +374s and the
/// daemon republished ready at +436s — a 62-second window. Three citizens' turns landed
/// inside it and were refused outright, 9 seconds before the lane came back. The self-tick
/// readiness gate (#350) cannot cover this: it reads the snapshot BEFORE a deliberation that
/// takes tens of seconds, so a teardown starting mid-deliberation always outruns it. The gate
/// stops a turn that was doomed at its start; this stops one that was overtaken in flight.
fn settled_on_another_model(snap: &crate::inference::llama_server::ServingSnapshot) -> bool {
    snap.is_live()
}

fn unguaranteed_model_refusal(
    provider: &str,
    model: &str,
    snap: &crate::inference::llama_server::ServingSnapshot,
    served_before: bool,
) -> String {
    if !served_before {
        return format!(
            "{provider}: serving daemon has not completed its first reconcile yet (core is \
             still starting) — model '{model}' cannot be guaranteed until it does. This is \
             STARTUP, not a serving fault: it clears on its own, typically within seconds, \
             and the caller should retry rather than treat the lane as broken."
        );
    }
    let Some(active) = snap.active_model.as_deref() else {
        return format!(
            "{provider}: no model is resident right now (the serving daemon is between \
             lanes — a re-home or a self-healing relaunch), so model '{model}' cannot be \
             guaranteed. This is a serving TRANSITION, not a fault: it clears when the next \
             reconcile publishes a ready lane, and the caller should retry rather than \
             treat the lane as broken."
        );
    };
    format!(
        "{provider}: model '{model}' is not the active served model (serving: {active}, \
         ready: {}); the serving daemon owns which single model is resident — refusing to \
         generate against an unguaranteed model",
        snap.ready
    )
}

/// #175 overflow backstop: does this request body's PROMPT ALONE meet/exceed the served
/// per-slot window? Returns `Some(estimated_prompt_tokens)` when it does — the
/// unambiguous overflow that (with context-shift off) 500s AND poisons the slot for
/// every later request, so the caller must refuse to send rather than take the shared
/// lane down. `served_window == 0` (window unknown, e.g. mid-relaunch) → `None` (never
/// block on an unknown budget). Estimate is chars/4 — the same conservative heuristic as
/// the `serving.ctx_overshoot` alarm; we only trip on prompt-alone-overflows so a
/// legitimately-budgeted request (which always leaves reply headroom) is never blocked.
fn prompt_alone_overflows_served(body: &serde_json::Value, served_window: u32) -> Option<usize> {
    if served_window == 0 {
        return None;
    }
    let prompt_tokens = body
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|msgs| {
            msgs.iter()
                .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
                .map(|c| c.len() / 4)
                .sum::<usize>()
        })
        .unwrap_or(0);
    (prompt_tokens >= served_window as usize).then_some(prompt_tokens)
}

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


impl OpenAICompatibleAdapter {
    /// Build the reqwest client for a STREAMING inference transport. There is
    /// deliberately NO total-request timeout: generation is a long-running job
    /// whose liveness is "is it still producing tokens?", not "did it finish
    /// within N seconds." A wall-clock cap is the wrong model — it kills a
    /// healthy-but-slow decode (a CPU-placed 4B mid-answer) at an arbitrary
    /// cliff. Liveness is enforced per-token by the idle watchdog in
    /// [`stream_completion`] (no token for [`STREAM_IDLE_TIMEOUT_SECS`] = the
    /// backend went silent → fail loud). `connect_timeout` still bounds the
    /// handshake to a dead loopback backend; `pool_idle_timeout` prevents stale
    /// pooled sockets across backend restarts.
    fn build_http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client")
    }

    pub fn new(config: OpenAICompatibleConfig) -> Self {
        // 120s total timeout bounds long generations (qwen3.5 reasoning
        // can take ~60s to emit a full response). Connect timeout bounds
        // the local-loopback DMR case specifically: when Docker Desktop
        // restarts or DMR isn't listening, we want the fast explicit
        // "connect refused" instead of a 120s stall. Idle timeout keeps
        // the reqwest pool from holding onto dead sockets across DMR
        // restarts — a stale pooled connection to a killed server was
        // the reproducing cause of 120s "error sending request" stalls.
        let client = Self::build_http_client();

        // Per-provider concurrency gate. A single-resident-model gateway
        // (DMR, llama-server) serves ONE model on ONE slot, so N personas
        // fanning out into concurrent POSTs must queue in this semaphore
        // INSTEAD of piling onto a gateway already busy on the prior persona's
        // forward pass — which (before streaming) was the "error sending
        // request -> operation timed out, connect=false" failure mode and is
        // now just wasted contention. Multi-model / cloud endpoints are
        // effectively unbounded. Keyed on the TYPED capability (#55), never the
        // provider id.
        let slots = if config.single_resident_model { 1 } else { 64 };
        let concurrency = std::sync::Arc::new(tokio::sync::Semaphore::new(slots));

        Self {
            config,
            api_key: None,
            runtime_base_url: None,
            dedicated_lane: false,
            client,
            initialized: false,
            runtime_models: std::sync::Arc::new(std::sync::RwLock::new(None)),
            lora_support: std::sync::Arc::new(std::sync::RwLock::new(LoraSupport::Unknown)),
            concurrency,
        }
    }

    /// Override the base URL at runtime (e.g. when running inside a Docker
    /// container on Windows/Linux where DMR is at model-runner.docker.internal
    /// instead of localhost:12434). Called post-construction, before init.
    pub fn with_runtime_base_url(mut self, url: String) -> Self {
        self.runtime_base_url = Some(url);
        self
    }

    /// Mark this adapter as owning a dedicated, single-purpose serving lane (an
    /// `EphemeralServingLane`) rather than sharing the global gateway. The
    /// single-resident pre-flight guard then trusts THIS lane (readiness
    /// guaranteed at spawn) instead of the global serving snapshot. See
    /// [`OpenAICompatibleAdapter::dedicated_lane`]. Called post-construction,
    /// before init, paired with [`with_runtime_base_url`] pointing at the lane.
    pub fn with_dedicated_lane(mut self) -> Self {
        self.dedicated_lane = true;
        self
    }

    /// Override the default model id (the one a `request.model: None` resolves to).
    /// Used at persona upstart to bind the adapter to the model unsloth ACTUALLY
    /// serves (discovered via `/v1/models`), instead of the the Rust catalog (catalog.rs) default
    /// — which can drift from what's loaded. Called post-construction, before init.
    pub fn with_default_model(mut self, model: String) -> Self {
        self.config.default_model = model;
        self
    }

    /// The typed OpenAI-compatible endpoint base — the runtime override if set,
    /// else the configured base. The ONE place request URLs are built; every
    /// site calls a typed accessor ([`OpenAiBase::chat_completions`] etc.) rather
    /// than concatenating `/v1/...` itself, so the protocol's path layout lives in
    /// exactly one place and the snapshot's `/v1` shape can no longer double to
    /// `/v1/v1/...` (THE Asha-mute bug). See [`crate::ai::openai_endpoints`].
    fn endpoints(&self) -> OpenAiBase {
        let raw = self
            .runtime_base_url
            .as_deref()
            .unwrap_or(self.config.base_url.as_str());
        OpenAiBase::new(raw)
    }

    /// Does this adapter's effective base URL target the LOCAL llama-server lane
    /// published in the serving snapshot? Data-driven attribution for the
    /// real-decode success/failure counters (#363): only requests that actually
    /// ride the local lane may stamp its record — a cloud provider's outage must
    /// never smear it, and a local success must never launder a cloud failure.
    /// URL equality against the ONE published snapshot, never a name sniff (#70).
    fn targets_local_serving_lane(&self) -> bool {
        let snap = crate::inference::llama_server::current_serving();
        if snap.base_url.is_empty() {
            return false;
        }
        fn norm(s: &str) -> &str {
            s.trim_end_matches('/')
                .trim_end_matches("/v1")
                .trim_end_matches('/')
        }
        let raw = self
            .runtime_base_url
            .as_deref()
            .unwrap_or(self.config.base_url.as_str());
        norm(raw) == norm(&snap.base_url)
    }

    /// The endpoint base for a SPECIFIC requested model. Same as [`Self::endpoints`]
    /// except for the local serving gateway when the requested model is the
    /// serving snapshot's VISION model but not its main-lane model: that is the
    /// #106 vision SIDECAR (a VL lane beside a text-only mind), and the request
    /// routes to the snapshot's verified `vision_base_url`. Driven entirely by
    /// the ONE published snapshot (id-equality against the gateway's canonical
    /// [`PROVIDER_ID`](crate::inference::llama_server::PROVIDER_ID) const — no
    /// name sniffing), and the address exists only when `/props` confirmed
    /// sight, so pixels can never be aimed at a text lane.
    fn endpoints_for_model(&self, model: &str) -> OpenAiBase {
        if self.config.provider_id == crate::inference::llama_server::PROVIDER_ID {
            let snap = crate::inference::llama_server::current_serving();
            if snap.vision_ready
                && snap.vision_model.as_deref() == Some(model)
                && snap.active_model.as_deref() != Some(model)
            {
                if let Some(url) = snap.vision_base_url.as_deref() {
                    return OpenAiBase::new(url);
                }
            }
        }
        self.endpoints()
    }

    /// Fetch the live model list from the provider's /v1/models endpoint.
    /// Used by adapters that have dynamic catalogs (DMR above all — the list
    /// changes every time the user runs `docker model pull`). Populates
    /// `runtime_models` on success; leaves it unchanged on failure so stale
    /// data is preferred over empty data. Never silently succeeds with an
    /// empty set — returns Err if the endpoint responds with nothing.
    async fn refresh_runtime_models(&self) -> Result<(), String> {
        let url = self.endpoints().models();

        let mut req = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("GET {} failed: {}", url, e))?;
        if !resp.status().is_success() {
            return Err(format!("GET {} returned {}", url, resp.status()));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse {} body: {}", url, e))?;
        let ids: std::collections::HashSet<String> = body
            .get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return Err(format!("{} returned no models", url));
        }
        *self.runtime_models.write().unwrap() = Some(ids);
        Ok(())
    }

    /// Resolve a logical model name to the actual DMR model ID stored in
    /// the runtime catalog. Returns the owned resolved ID on match, or an
    /// Err describing what the caller asked for vs what DMR actually has
    /// — no fallback to the raw name (DMR would just 404 on it).
    ///
    /// On cache miss (either an empty cache or a populated cache that
    /// doesn't contain the needle) this forces a single
    /// `refresh_runtime_models` and retries the lookup once. That covers
    /// the common case: the user ran `docker model pull` after the
    /// adapter initialized, so the forged model exists in DMR but not in
    /// our stale in-memory set.
    async fn resolve_dmr_model_name(&self, model_name: &str) -> Result<String, String> {
        if let Some(hit) = self.lookup_runtime_model(model_name) {
            return Ok(hit);
        }
        // Cache miss — refresh once, then retry. If refresh itself fails
        // we surface that error; if the needle still isn't there we
        // hard-error with the full available set so the log makes the
        // mismatch obvious (e.g. persona asked for "-GGUF" but DMR stores
        // "...-gguf:latest").
        self.refresh_runtime_models().await?;
        if let Some(hit) = self.lookup_runtime_model(model_name) {
            return Ok(hit);
        }
        let available: Vec<String> = self
            .runtime_models
            .read()
            .unwrap()
            .as_ref()
            .map(|ids| ids.iter().cloned().collect())
            .ok_or_else(|| "DMR runtime_models still empty after refresh".to_string())?;
        Err(format!(
            "DMR does not have model '{}'. Available: {:?}. Pull it with: docker model pull <id>",
            model_name, available
        ))
    }

    /// Pure lookup against the cached runtime_models set. Same matching
    /// rules as `runtime_models_contain`: case-insensitive exact or
    /// trivial contains in either direction. No I/O, no refresh — callers
    /// own the refresh decision.
    fn lookup_runtime_model(&self, model_name: &str) -> Option<String> {
        let guard = self.runtime_models.read().unwrap();
        let ids = guard.as_ref()?;
        let needle = model_name.to_lowercase();
        ids.iter()
            .find(|id| {
                let hay = id.to_lowercase();
                hay == needle || hay.contains(&needle) || needle.contains(&hay)
            })
            .cloned()
    }

    /// Returns true if model_name matches any live runtime model.
    /// Match is exact OR a trivial contains in either direction to
    /// handle the common "persona says short name, DMR stores full
    /// hf.co/…-GGUF ID" pattern. No fuzzy magic beyond that — if neither
    /// contains the other, the adapter honestly does not have the model.
    /// Ensure `id` is present in the runtime catalog. The llama-server gateway
    /// calls this with the ServingSnapshot's `active_model` after initialize —
    /// the SNAPSHOT is the authority on what the lane serves; the `/v1/models`
    /// catalog is DERIVED, and it can lie about identity: on Windows a mangled
    /// spawn `--alias` put the GGUF file PATH in `data[].id`, so the served
    /// model matched nothing and `select()` refused a healthy lane (5090 repro
    /// 2026-07-24). Not a fallback: this records a fact the daemon's reconcile
    /// already verified against the live process.
    pub fn ensure_runtime_model(&self, id: &str) {
        let mut guard = self.runtime_models.write().unwrap();
        match guard.as_mut() {
            Some(set) => {
                set.insert(id.to_string());
            }
            None => {
                *guard = Some(std::collections::HashSet::from([id.to_string()]));
            }
        }
    }

    fn runtime_models_contain(&self, model_name: &str) -> bool {
        let guard = self.runtime_models.read().unwrap();
        match guard.as_ref() {
            None => false, // not populated — can't lie, return false
            Some(ids) => {
                let needle = model_name.to_lowercase();
                ids.iter().any(|id| {
                    let hay = id.to_lowercase();
                    hay == needle || hay.contains(&needle) || needle.contains(&hay)
                })
            }
        }
    }

    /// Probe the serving backend's `GET /lora-adapters` and record what it
    /// says into `lora_support`. This is capability DISCOVERY, not a declared
    /// table: a 200 with an array → `Supported` (with the name→id catalog); a
    /// 404/501 → `Unsupported` (cached, so we don't re-probe a cloud/mlx
    /// backend every turn). A connection/transport error is returned as `Err`
    /// and NOT cached — a momentarily-dead server is not a server without LoRA
    /// support. llama.cpp `llama-server` returns the array of adapters it
    /// loaded at launch, each `{ "id": N, "path": "...", "scale": S }`.
    /// Stable slot for an ACTIVITY (a persona's conversation in a room — the
    /// typed [`ActivityKey`](crate::inference::slots::ActivityKey)), discovering
    /// the backend's slot count on first use (`GET /props` → `total_slots`).
    /// The adapter owns only the TRANSPORT half (it has the HTTP client); the
    /// lease itself lives in [`crate::inference::slots`] — the KV concern's
    /// adapter over the ONE shared paging engine, per
    /// [[one-paging-engine-many-trait-implementers]]. Returns `None` when the
    /// backend has no props surface / one slot (latched unsupported) or on a
    /// transport error (NOT latched — a momentarily-dead server is not a server
    /// without slots; same discipline as the LoRA probe).
    /// The reserved scratch slot for this server, if its pool is installed and
    /// reserved one. Deliberately does NOT probe /props: scratch placement is a
    /// best-effort courtesy for non-Turn traffic, and the first Turn request
    /// installs the pool anyway — before that, non-Turn traffic simply runs
    /// unpinned exactly as it always did.
    fn scratch_slot_for_root(&self) -> Option<u32> {
        let root = self.endpoints().root().to_string();
        // Typed seam: the process-global SlotDirectory hands this adapter its
        // server's KvSlotPool — the ONE paging-engine implementer for KV slots.
        let dir: &crate::inference::slots::SlotDirectory = crate::inference::slots::directory();
        match dir.get(&root) {
            Some(Some(pool)) => {
                let pool: std::sync::Arc<crate::inference::slots::KvSlotPool> = pool;
                pool.scratch_slot()
            }
            _ => None,
        }
    }

    async fn slot_for_activity(
        &self,
        key: crate::inference::slots::ActivityKey,
    ) -> Option<u32> {
        let root = self.endpoints().root().to_string();
        let dir = crate::inference::slots::directory();
        // Fast path: this server's state already known (process-global directory —
        // every adapter instance talking to the same server shares ONE assignment).
        match dir.get(&root) {
            Some(Some(pool)) => return pool.lease(key).await,
            Some(None) => return None, // latched unsupported
            None => {}                 // never probed — probe below
        }
        // Probe /props once. Lock is NOT held across the await.
        let url = self.endpoints().props();
        let resp = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(error = %e, url = %url, "props probe transport error — slot affinity deferred");
                return None;
            }
        };
        if !resp.status().is_success() {
            let status = resp.status();
            // ONLY "this endpoint does not exist" may latch Unsupported. Any other
            // status — above all the 503 a llama-server answers while the model is
            // still LOADING — is transient, and latching on it killed slot affinity
            // on effectively EVERY boot: personas start deliberating before the lane
            // is warm (`inference.lane_relaunch_retry` ×93, reason=503_loading, same
            // ledger), so the first probe raced the load window, latched Unsupported
            // for the process's life, and prefix-similarity slot theft quietly
            // replaced pinning — measured as `cached: 0` mid-conversation on
            // 2026-08-21. A verdict about what a server IS must never be reached
            // while the server is mid-transition (#442's rule, one layer down).
            if props_status_proves_endpoint_absent(status) {
                crate::probe!(
                    class = "inference.slot_affinity.unsupported",
                    status = status.as_u16() as u64,
                    "props endpoint absent — slot affinity latched OFF for this server",
                );
                dir.latch_unsupported(&root);
            } else {
                crate::probe!(
                    class = "inference.slot_affinity.deferred",
                    status = status.as_u16() as u64,
                    "props not ready (transient status) — affinity deferred, NOT latched; \
                     will re-probe on the next persona request",
                );
            }
            return None;
        }
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => {
                // A malformed body from a live server is also not proof of absence —
                // mid-load llama-server can answer partial/HTML bodies. Defer, don't latch.
                crate::probe!(
                    class = "inference.slot_affinity.deferred",
                    status = 200u64,
                    "props answered but body did not parse — affinity deferred, NOT latched",
                );
                return None;
            }
        };
        // Capture the served PER-SLOT window while we hold the props body —
        // the ONE authoritative source for what a request can actually carry
        // (#139). Recorded even for single-slot servers (the window truth is
        // independent of whether affinity is useful).
        if let Some(n_ctx) = body
            .pointer("/default_generation_settings/n_ctx")
            .and_then(|v| v.as_u64())
        {
            served_ctx_by_root().insert(root.clone(), n_ctx as u32);
        }
        let n_slots = body
            .get("total_slots")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        if n_slots <= 1 {
            dir.latch_unsupported(&root);
            return None;
        }
        // The directory arbitrates the probe race: only the first writer installs
        // the pool, once per SERVER, not once per adapter.
        let pool = dir.ensure_pool(&root, n_slots);
        tracing::info!(
            n_slots,
            "slot affinity enabled — activities lease llama-server slots (props-discovered)"
        );
        pool.lease(key).await
    }

    pub(crate) async fn probe_lora_catalog(&self) -> Result<(), String> {
        let url = self.endpoints().lora_adapters();
        let mut req = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("GET {} failed: {}", url, e))?;

        // 404/501 → the backend genuinely has no runtime-LoRA surface. Cache it.
        if resp.status() == reqwest::StatusCode::NOT_FOUND
            || resp.status() == reqwest::StatusCode::NOT_IMPLEMENTED
        {
            *self.lora_support.write().unwrap() = LoraSupport::Unsupported;
            return Ok(());
        }
        if !resp.status().is_success() {
            return Err(format!("GET {} returned {}", url, resp.status()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse {} body: {}", url, e))?;
        // Accept a top-level array (llama-server) or `{ "data": [...] }`.
        let arr = body
            .as_array()
            .or_else(|| body.get("data").and_then(|v| v.as_array()))
            .ok_or_else(|| format!("{} returned non-array LoRA catalog", url))?;
        let catalog: Vec<(i64, String)> = arr
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                // The server's own `id` is authoritative; fall back to array
                // position only if it omits one (older builds).
                let id = entry
                    .get("id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(idx as i64);
                let path = entry
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                (id, path)
            })
            .collect();
        *self.lora_support.write().unwrap() = LoraSupport::Supported(catalog);
        Ok(())
    }

    /// Pure matcher: given a discovered catalog of `(server-id, path)`, find the
    /// server load-index for a requested adapter. Exact path match wins; else a
    /// trivial substring match on path or name (same forgiving rule as
    /// `lookup_runtime_model`, for the "short name vs full path" case). No I/O.
    fn match_lora_index(catalog: &[(i64, String)], name: &str, path: &str) -> Option<i64> {
        let want_path = path.to_lowercase();
        let want_name = name.to_lowercase();
        // 1. exact path
        if !want_path.is_empty() {
            if let Some((id, _)) = catalog.iter().find(|(_, p)| p.to_lowercase() == want_path) {
                return Some(*id);
            }
        }
        // 2. substring on path or name
        catalog
            .iter()
            .find(|(_, p)| {
                let hay = p.to_lowercase();
                (!want_path.is_empty() && hay.contains(&want_path))
                    || (!want_name.is_empty() && hay.contains(&want_name))
            })
            .map(|(id, _)| *id)
    }

    /// Project the genome onto the wire as the complete llama.cpp `lora`
    /// request-body field: an `{ "id": N, "scale": S }` entry for EVERY loaded
    /// adapter — the requested scale when the genome names it, else `0.0`.
    ///
    /// Why an entry per LOADED adapter and not just per REQUESTED one: llama.cpp
    /// applies every loaded adapter at scale 1.0 for any request that OMITS the
    /// `lora` field. So once the custodian has loaded an adapter, an empty genome
    /// can NOT be expressed by omission — omitting silently serves the adapter at
    /// full strength. That was the no-op behind the LIFT=0 A/B: the base arm sent
    /// no `lora` field and unknowingly ran WITH the gene, so base==gene, lift 0.
    /// The genome handle is the single source of truth for what's active; emitting
    /// explicit `0.0` for loaded-but-unrequested adapters makes "empty genome ==
    /// base" true at the wire.
    ///
    /// Returns `None` only when nothing is loaded (omitting the field is then
    /// correct, and a non-LoRA backend with an empty request never probes — no
    /// per-request penalty). Capability is DISCOVERED: a non-empty request probes
    /// `/lora-adapters` and FAILS LOUD if the backend can't page LoRA
    /// (`Unsupported`) or the adapter isn't loaded (re-probing once first).
    async fn lora_scale_vector(
        &self,
        reqs: &[ActiveAdapterRequest],
    ) -> Result<Option<Vec<Value>>, String> {
        // Resolve each requested adapter to its loaded `(id, scale)` (probe +
        // fail loud on miss). Explicit requested scales override the 0.0 neutral.
        let mut requested: Vec<(i64, f64)> = Vec::with_capacity(reqs.len());
        if !reqs.is_empty() {
            if matches!(&*self.lora_support.read().unwrap(), LoraSupport::Unknown) {
                self.probe_lora_catalog().await?;
            }
            for req in reqs {
                let id = match self.lookup_lora_index(&req.name, &req.path) {
                    Some(id) => id,
                    None => {
                        // Miss — re-probe once (it may have just been registered),
                        // then resolve or fail loud with what IS loaded.
                        self.probe_lora_catalog().await?;
                        self.lookup_lora_index(&req.name, &req.path)
                            .ok_or_else(|| self.lora_miss_error(&req.name, &req.path))?
                    }
                };
                requested.push((id, req.scale));
            }
        }

        // Neutralize every loaded-but-unrequested adapter. Unknown/Unsupported with
        // an EMPTY request → nothing known to neutralize → omit the field (the eval
        // probes at lane spawn so its base arm already has the catalog here).
        let loaded_ids: Vec<i64> = match &*self.lora_support.read().unwrap() {
            LoraSupport::Supported(catalog) => catalog.iter().map(|(id, _)| *id).collect(),
            _ => Vec::new(),
        };
        if loaded_ids.is_empty() {
            return Ok(None);
        }
        let entries = loaded_ids
            .into_iter()
            .map(|id| {
                let scale = requested
                    .iter()
                    .find(|(rid, _)| *rid == id)
                    .map(|(_, s)| *s)
                    .unwrap_or(0.0);
                json!({ "id": id, "scale": scale })
            })
            .collect();
        Ok(Some(entries))
    }

    /// Local lookup against the discovered catalog. `None` when unsupported,
    /// unprobed, or genuinely absent — callers own the probe/fail decision.
    fn lookup_lora_index(&self, name: &str, path: &str) -> Option<i64> {
        match &*self.lora_support.read().unwrap() {
            LoraSupport::Supported(catalog) => Self::match_lora_index(catalog, name, path),
            _ => None,
        }
    }

    /// The fail-loud message for an unresolved page-in, naming the cause so the
    /// log makes the boundary obvious (backend can't, vs custodian hasn't).
    fn lora_miss_error(&self, name: &str, path: &str) -> String {
        match &*self.lora_support.read().unwrap() {
            LoraSupport::Unsupported => format!(
                "LoRA page-in requested (adapter '{}') but provider '{}' exposes no \
                 /lora-adapters — its serving backend can't apply a LoRA per-request \
                 (cloud API, or mlx_lm.server whose --adapter-path no-ops). Serve a \
                 FUSED model or route to a llama.cpp/llama-server backend.",
                name, self.config.provider_id
            ),
            LoraSupport::Supported(catalog) => {
                let loaded: Vec<&str> = catalog.iter().map(|(_, p)| p.as_str()).collect();
                format!(
                    "LoRA adapter '{}' (path '{}') is not loaded on '{}'. Loaded: {:?}. \
                     The custodian must register it with the serving backend first.",
                    name, path, self.config.provider_id, loaded
                )
            }
            LoraSupport::Unknown => format!(
                "LoRA adapter '{}' could not be resolved on '{}' (catalog unprobed)",
                name, self.config.provider_id
            ),
        }
    }

    /// Build an adapter for `provider_id` by reading everything from the
    /// model_registry. Replaces eight hand-rolled factories whose combined
    /// bulk was ~280 LOC of `ModelInfo { ... }` literals that drifted
    /// whenever a new model shipped. Now the catalog is the only place a
    /// new model's context_window / capabilities / pricing lives.
    ///
    /// Panics if the provider isn't in the registry — that's a boot-time
    /// config bug, not a runtime condition (per the no-fallback rule).
    ///
    /// Capability flags (`supports_tools`, `supports_vision`) are derived
    /// from whether ANY model under this provider advertises the relevant
    /// Capability. A new Vision-capable model showing up in the catalog flips
    /// the adapter's vision flag automatically on next boot — no code
    /// change.
    pub fn from_registry(provider_id: &str) -> Self {
        let reg = crate::model_registry::global();
        let provider = reg.provider(provider_id).unwrap_or_else(|| {
            panic!(
                "provider `{}` not in the Rust catalog (catalog.rs) — can't build \
                 OpenAICompatibleAdapter",
                provider_id
            )
        });

        let models = models_for_provider_via_registry(provider_id);

        // Project this provider's real capabilities into the ONE vocabulary
        // (#65). Every OpenAI-compatible adapter does text + chat + streaming;
        // tool-use + vision come from scanning the provider's models; embeddings
        // + image-gen come from the provider's declared `ProviderCapabilities`.
        // A new vision-capable model in the catalog flips the Vision flag
        // automatically on next boot — no code change, no `id == "..."` branch.
        let mut capabilities = BTreeSet::from([
            Capability::TextGeneration,
            Capability::Chat,
            Capability::Streaming,
        ]);
        if reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::ToolUse))
        {
            capabilities.insert(Capability::ToolUse);
        }
        if reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::Vision))
        {
            capabilities.insert(Capability::Vision);
        }
        // Embedding + ImageGeneration are derived the SAME way as ToolUse and
        // Vision above (#68): scan the provider's model rows. A provider
        // "supports embeddings" iff it serves a model that declares it — the
        // fact lives on the model, never on a provider-level bool.
        if reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::Embedding))
        {
            capabilities.insert(Capability::Embedding);
        }
        if reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::ImageGeneration))
        {
            capabilities.insert(Capability::ImageGeneration);
        }
        let requires_auth = !matches!(provider.auth, AuthKind::None);

        // `default_model` is non-optional in the adapter trait
        // (`fn default_model(&self) -> &str`) — callers always get a
        // concrete id back. Providers with genuinely dynamic catalogs
        // (DMR) still declare a default id the user is most likely to
        // want; operator overrides flow through explicit request.model.
        // Panic if missing: the registry row is incomplete, not a runtime
        // condition.
        let default_model = provider.default_model.clone().unwrap_or_else(|| {
            panic!(
                "provider `{}` has no `default_model` in the Rust catalog (catalog.rs) — \
                 every OpenAI-compatible adapter needs one because the trait \
                 returns &str, not Option<&str>",
                provider_id
            )
        });

        Self::new(OpenAICompatibleConfig {
            provider_id: provider.id.clone(),
            name: provider.display_name().to_string(),
            base_url: provider.base_url.clone(),
            api_key_env: provider.api_key_env.clone(),
            default_model,
            capabilities,
            models,
            model_prefixes: provider.model_prefixes.clone(),
            requires_auth,
            // Tool-call shape + thinking + embeddings + single-slot residency +
            // dynamic-catalog + llama.cpp sampling extensions ALL come from the
            // registry's declared `Provider.capabilities` (#55) — the adapter
            // CONSUMES them, it does not branch on `provider.id`. A local GGUF
            // gateway declares its real flags; cloud providers inherit the
            // NativeFunctionCalling/keep-thinking defaults. One source of truth
            // (model_registry/catalog.rs), no id stand-ins here, and the ONE
            // `ToolProtocol` (#69) — stored verbatim, no per-adapter translation.
            tool_protocol: provider.capabilities.tool_protocol,
            // A gateway that declares `suppress_thinking` (its forged reasoner
            // rambles/loops yet answers correctly without CoT) defaults to
            // SUPPRESS. Operator override: `UNSLOTH_THINKING=on` forces thinking
            // back on per-run (the reasoning-strip still protects the room).
            thinking: {
                let keep_thinking = std::env::var("UNSLOTH_THINKING")
                    .map(|v| v.trim().eq_ignore_ascii_case("on"))
                    .unwrap_or(false);
                if provider.capabilities.suppress_thinking && !keep_thinking {
                    ThinkingMode::Suppress
                } else {
                    ThinkingMode::Default
                }
            },
            single_resident_model: provider.capabilities.single_resident_model,
            dynamic_model_catalog: provider.capabilities.dynamic_model_catalog,
            llamacpp_sampling_extensions: provider.capabilities.llamacpp_sampling_extensions,
        })
    }

    /// Convert ChatMessage to OpenAI format.
    ///
    /// `vision_native` is the TARGET MODEL's verdict (the row's
    /// `Capability::Vision` via `sensory::route`, resolved by the caller): when
    /// true, `ContentPart::Image` becomes a proper OpenAI multimodal
    /// `image_url` content part (base64 data-URI or URL) so a vision model —
    /// cloud or the multimodal llama-server lane — receives RAW PIXELS
    /// natively. When false, image parts are DROPPED here (with a loud log):
    /// a non-vision model reads the VisionDescriptionService bridge text that
    /// the sensory layer already put in the message, and POSTing `image_url`
    /// parts at a text-only endpoint is at best an API error and at worst a
    /// silent drop the persona would mistake for having seen
    /// ([[fallbacks-are-illegal-fail-loud]], CLAUDE.md "Sensory Architecture").
    fn format_messages(
        &self,
        messages: &[ChatMessage],
        system_prompt: Option<&str>,
        vision_native: bool,
    ) -> Vec<Value> {
        // Pre-size: one wire message per input message + the optional system
        // prompt. The common text path lands exactly; tool-result turns push a
        // few extra and realloc once. Runs on every inference call — no
        // grow-from-zero reallocation on the hot path.
        let mut result = Vec::with_capacity(messages.len() + usize::from(system_prompt.is_some()));

        // Add system prompt if provided
        if let Some(sys) = system_prompt {
            result.push(json!({
                "role": "system",
                "content": sys
            }));
        }

        for msg in messages {
            match &msg.content {
                MessageContent::Text(text) => {
                    result.push(json!({
                        "role": msg.role,
                        "content": text
                    }));
                }
                MessageContent::Parts(parts) => {
                    // Check for tool protocol blocks
                    let has_tool_use = parts
                        .iter()
                        .any(|p| matches!(p, ContentPart::ToolUse { .. }));
                    let has_tool_result = parts
                        .iter()
                        .any(|p| matches!(p, ContentPart::ToolResult { .. }));

                    if has_tool_use {
                        // Assistant message with tool_calls
                        let text_content: String = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("");

                        let tool_calls: Vec<Value> = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::ToolUse { id, name, input } => Some(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": serde_json::to_string(input).unwrap_or_default()
                                    }
                                })),
                                _ => None,
                            })
                            .collect();

                        result.push(json!({
                            "role": "assistant",
                            "content": if text_content.is_empty() { Value::Null } else { Value::String(text_content) },
                            "tool_calls": tool_calls
                        }));
                    } else if has_tool_result {
                        // Tool results as separate messages
                        for part in parts {
                            if let ContentPart::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } = part
                            {
                                result.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tool_use_id,
                                    "content": content
                                }));
                            }
                        }
                    } else {
                        // Standard multimodal content
                        let content: Vec<Value> = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::Text { text } => Some(json!({
                                    "type": "text",
                                    "text": text
                                })),
                                ContentPart::Image { image } => {
                                    if !vision_native {
                                        // Target model can't see: the sensory bridge's
                                        // text description (already a Text part /
                                        // upstream) is what it reads. Never ship
                                        // image_url at a text-only endpoint.
                                        tracing::warn!(
                                            target: "openai_adapter",
                                            provider = %self.config.provider_id,
                                            "dropping image content part for a non-vision \
                                             model — the description bridge is its sight; \
                                             if this model CAN see, its catalog row must \
                                             declare Capability::Vision"
                                        );
                                        None
                                    } else if let Some(url) = &image.url {
                                        Some(json!({
                                            "type": "image_url",
                                            "image_url": { "url": url }
                                        }))
                                    } else {
                                        image.base64.as_ref().map(|b64| json!({
                                            "type": "image_url",
                                            "image_url": {
                                                "url": format!("data:{};base64,{}",
                                                    image.mime_type.as_deref().unwrap_or("image/png"), b64)
                                            }
                                        }))
                                    }
                                }
                                _ => None,
                            })
                            .collect();

                        result.push(json!({
                            "role": msg.role,
                            "content": content
                        }));
                    }
                }
            }
        }

        // Thinking toggle: when this gateway suppresses reasoning, append Qwen3's
        // `/no_think` soft-switch to the last user turn so the model skips its
        // chain-of-thought and answers directly. Model-specific token, owned here at
        // the adapter boundary; higher layers never speak `/no_think`.
        if self.config.thinking == ThinkingMode::Suppress {
            apply_no_think_switch(&mut result);
        }

        result
    }

    /// Map OpenAI finish reason to our enum
    fn map_finish_reason(&self, reason: &str) -> FinishReason {
        match reason {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "tool_calls" => FinishReason::ToolUse,
            _ => FinishReason::Error,
        }
    }
}

/// Separate a reasoning model's chain-of-thought from its user-facing answer at the
/// ADAPTER boundary, so reasoning is captured (for the glass-box harness) but NEVER
/// reaches the room. Precedence:
///
/// 1. A server-provided `reasoning_content` (vLLM-style parsers) — authoritative;
///    `content` is already clean.
/// 2. Inline `<think>…</think>` in `content` (unsloth/llama.cpp today) — the block
///    is reasoning; everything OUTSIDE it is the answer.
/// 3. An UNCLOSED `<think>` — the model ran out of tokens mid-thought (the runaway
///    loop that leaked into the room): the whole tail is reasoning and there is NO
///    answer. Returns empty text so the caller refuses to post, never leaking raw
///    reasoning.
///
/// Returns `(clean_text, reasoning)`. Pure + synchronous → unit-tested in isolation.
pub(crate) fn extract_reasoning(
    content: &str,
    reasoning_content: Option<&str>,
) -> (String, Option<String>) {
    // (1) Server already split it out — trust that; content is the clean answer.
    if let Some(rc) = reasoning_content {
        let rc = rc.trim();
        if !rc.is_empty() {
            return (content.trim().to_string(), Some(rc.to_string()));
        }
    }

    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";
    let Some(open_idx) = content.find(OPEN) else {
        // (no think) plain content.
        return (content.trim().to_string(), None);
    };
    let before = content[..open_idx].trim();
    let after_open = &content[open_idx + OPEN.len()..];

    match after_open.find(CLOSE) {
        // (2) Well-formed <think>…</think>: answer is whatever sits OUTSIDE the block.
        Some(close_rel) => {
            let reasoning = after_open[..close_rel].trim();
            let after_close = after_open[close_rel + CLOSE.len()..].trim();
            let mut text = String::from(before);
            if !text.is_empty() && !after_close.is_empty() {
                text.push('\n');
            }
            text.push_str(after_close);
            (
                text.trim().to_string(),
                (!reasoning.is_empty()).then(|| reasoning.to_string()),
            )
        }
        // (3) Unclosed <think>: truncated thinking → no answer (text is whatever
        // preceded the block, normally empty). The reasoning is the runaway tail.
        None => {
            let reasoning = after_open.trim();
            (
                before.to_string(),
                (!reasoning.is_empty()).then(|| reasoning.to_string()),
            )
        }
    }
}

/// Append Qwen3's `/no_think` soft-switch to the LAST user message in a built
/// OpenAI message array, suppressing chain-of-thought for the turn (the model emits
/// an empty `<think></think>` then answers directly — which [`extract_reasoning`]
/// reduces to clean text + no reasoning). Operates on string content (chat turns);
/// multimodal/array content is left untouched (a follow-up can append a text part).
/// No user message → no-op.
fn apply_no_think_switch(messages: &mut [Value]) {
    for m in messages.iter_mut().rev() {
        if m.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(content) = m.get_mut("content") {
            if let Some(s) = content.as_str() {
                *content = Value::String(format!("{s}\n/no_think"));
            }
        }
        return;
    }
}

/// Set `chat_template_kwargs.enable_thinking = false` on a built request body — the
/// ROBUST thinking-suppression lever for qwen3-family chat templates. Where
/// `apply_no_think_switch` appends a soft text token (which a forged template may
/// ignore entirely), this drives the template's own `enable_thinking` branch so it
/// emits an empty `<think></think>` and the model goes straight to content. Inserting
/// at the body's top level (not inside an existing kwargs map) is correct for the
/// llama.cpp/unsloth servers we target; idempotent — overwrites its own prior value.
/// Harmless where unsupported: cloud providers ignore unknown body fields and a
/// template without `enable_thinking` ignores the kwarg.
fn apply_enable_thinking_false(body: &mut Value) {
    if let Some(obj) = body.as_object_mut() {
        obj.insert(
            "chat_template_kwargs".to_string(),
            json!({ "enable_thinking": false }),
        );
    }
}

/// Close a message thread that ENDS with an assistant turn — wire-illegal on
/// thinking models: llama-server treats a trailing assistant message as response
/// PREFILL and rejects the request 400 ("Assistant response prefill is
/// incompatible with enable_thinking"). Glass-boxed 2026-07-11: 1000+ self-tick
/// deliberations silently died over two days whenever the persona had spoken
/// last (her own posts are attributed role=assistant, task #92). We never intend
/// prefill semantics — those are past TURNS — so append a structural continuation
/// fact (true by construction, decides nothing about her reply;
/// [[no-hardcoded-heuristics-to-steer-cognition]]). Thinking stays ON
/// ([[thinking-is-primary-never-suppress]]); suppressing it instead would trade
/// a wire bug for a cognition downgrade. No-op on threads already ending with a
/// user/system/tool message.
fn close_trailing_assistant(messages: &mut Vec<Value>) {
    let ends_with_assistant = messages
        .last()
        .and_then(|m| m.get("role"))
        .and_then(|r| r.as_str())
        .map(|r| r == "assistant")
        .unwrap_or(false);
    if ends_with_assistant {
        messages.push(json!({
            "role": "user",
            "content": "[continuation] The transcript above ends with your own \
                        last turn; nothing external arrived after it. You are \
                        continuing your own thread."
        }));
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: Option<u32>,
}

/// llama-server's per-request `timings` object, present on the final stream frame.
/// These are the fields we surface into [`GenerationTiming`] so the harness can
/// separate PREFILL cost from DECODE cost; llama emits more (`*_per_token_ms`
/// variants) we don't need. All `#[serde(default)]` so a provider that omits any
/// field (or the whole object) degrades to zeros, never a parse failure.
#[derive(Debug, Deserialize)]
struct OpenAITimings {
    /// Prefix tokens served from KV cache (no recompute).
    #[serde(default)]
    cache_n: u32,
    /// NEW tokens prefilled this call (the re-rasterization tax).
    #[serde(default)]
    prompt_n: u32,
    #[serde(default)]
    prompt_ms: f64,
    #[serde(default)]
    prompt_per_second: f64,
    #[serde(default)]
    predicted_n: u32,
    #[serde(default)]
    predicted_ms: f64,
    #[serde(default)]
    predicted_per_second: f64,
}

/// How long the inference lane may stay SILENT mid-stream before we declare it
/// dead. This is a LIVENESS watchdog, not a deadline: a slow-but-producing decode
/// (a 4B model on CPU emitting a token every few hundred ms) stays alive
/// indefinitely as long as it keeps streaming. Only true silence — the backend
/// stuck, crashed, or the socket wedged — trips it, and then we fail loud naming
/// the cause ([[fallbacks-are-illegal-fail-loud]]). Replaces the old wall-clock
/// total-request timeout that killed legitimately-long generations.
const STREAM_IDLE_TIMEOUT_SECS: u64 = 90;

/// Bound on the wait for response HEADERS after POSTing a generation — the
/// pre-stream twin of [`STREAM_IDLE_TIMEOUT_SECS`]. Covers the hung-prefill /
/// poisoned-backend case where the server accepts and never answers; sized for
/// a worst-case full-window prefill queued behind co-tenants (minutes), because
/// its job is releasing ETERNAL holds, not policing slow ones.
const PRE_STREAM_HEADER_TIMEOUT_SECS: u64 = 300;

/// A local single-resident lane can be RELAUNCHED out from under an in-flight POST —
/// grow-back (#214), a genome page-in, or memory pressure all bounce the llama-server
/// process, and the published serving snapshot can lag at `ready=true` for the ~seconds
/// the socket is actually refused (the pre-flight guard trusts the `watch` snapshot; the
/// socket is the ground truth, and a watch channel is inherently slightly behind the
/// process). A `connect` error is therefore "the lane is mid-relaunch", not "the lane is
/// gone": the connection never opened, so nothing was streamed to the sink, and
/// re-sending the SAME lane/model is idempotent — resilience, NOT a fallback
/// ([[fallbacks-are-illegal-fail-loud]]). Retry the connect with linear backoff
/// (1s, 2s, … ≈ 21s total) to ride out a relaunch, then fail loud if it never returns.
/// Scoped to the local resident lane — remote endpoints don't relaunch under us.
/// Glass-boxed 2026-07-20: one legitimate grow-back relaunch zeroed hard-rs 0/8, every
/// task `Connection refused (os error 61)` to :58057 mid-eval.
const LANE_RELAUNCH_CONNECT_RETRIES: u32 = 6;
const LANE_RELAUNCH_RETRY_BASE: std::time::Duration = std::time::Duration::from_secs(1);

/// Warn LOUD when a call's measured decode rate collapses far below the catalog
/// row's expectation (#441, Joel 2026-08-15: "we need very good tok/sec or it's a
/// failure. We need warnings when shit hits the fan").
///
/// The comparison is decode-only (`predicted_per_second` — undiluted by prefill,
/// so a long prompt can't false-positive this) against the row's
/// `tokens_per_second`. Both thresholds are deliberately coarse: this is a
/// shit-hit-the-fan alarm for order-of-magnitude collapse (CPU-fallback lane,
/// thrashing pager, contended GPU), not a perf regression tracker — a row whose
/// estimate is merely 2× optimistic must stay silent.
///
/// The classification itself is delegated to the canonical
/// [`crate::inference::throughput_expectation::classify_throughput`] — this
/// call site only supplies policy: a sample-size gate (a rate computed over a
/// handful of tokens is noise, and warmup's first few decodes read slow) and a
/// collapse floor (below a quarter of expectation is a different MACHINE
/// STATE, not variance — a row whose estimate is merely 2× optimistic must
/// stay silent).
///
/// Stateless by design — one warn per breaching call. During a genuinely
/// degraded period that is one line per turn, which is the correct volume for
/// "every consumer of this lane is currently waiting an eternity". Unknown model
/// rows and rows without an expectation stay silent (no registry = no contract
/// to breach — external/cloud adapters are not governed lanes).
fn warn_if_decode_collapsed(model_id: &str, decode_tokens: u32, measured_tps: f64) {
    if decode_tokens < 16 || measured_tps <= 0.0 {
        return;
    }
    let Some(expected) = crate::model_registry::try_global()
        .and_then(|r| r.model(model_id).map(|m| m.tokens_per_second as f64))
        .filter(|e| *e > 0.0)
    else {
        return;
    };
    // Collapse alarm only: floor 0.25 of catalog rate, no above-par ceiling
    // (this seam never celebrates over-delivery — it screams on collapse).
    let verdict = crate::inference::throughput_expectation::classify_throughput(
        measured_tps,
        expected,
        0.25,
        f64::INFINITY,
    );
    if verdict.is_degraded() {
        // CONCURRENCY AT THE MOMENT OF MEASUREMENT (#441). The expectation is a
        // SINGLE-STREAM rate; this decode may have shared the box with N-1 other
        // model calls, and a shared decode is legitimately slower with NOTHING
        // wrong. Measured 2026-08-20 on the 27B: 6.56 t/s median against a 17.2
        // pinned expectation — ratio 0.38, which is real contention, not a defect.
        //
        // ANNOTATE, DO NOT GATE. This alarm's own contract names "contended GPU"
        // as a thing it exists to catch, so suppressing on concurrency would
        // defeat it. Reporting the count lets a reader attribute the ratio
        // instead of hunting a CPU fallback that isn't there — the failure mode
        // this line previously invited by listing three suspects and no evidence.
        //
        // Reuses the existing gauge (`resource_admission::inflight_model_calls`,
        // "lane-queue + prefill + decode") rather than counting again — the
        // concurrency sibling of the window axis on `ThroughputBaseline`.
        let inflight = crate::cognition::resource_admission::inflight_model_calls();
        tracing::warn!(
            probe_class = "serving.throughput.degraded",
            model = model_id,
            measured_tps = measured_tps,
            expected_tps = expected,
            ratio = verdict.ratio(),
            decode_tokens = decode_tokens,
            inflight_model_calls = inflight,
            "THROUGHPUT COLLAPSE: decode {measured_tps:.1} t/s vs expected {expected:.0} t/s \
             (single-stream) with {inflight} model call(s) in flight — this lane is serving at \
             a fraction of its catalog rate. With >1 in flight the expectation is not \
             like-for-like and contention alone may explain it; at 1 in flight suspect CPU \
             fallback (see serving.placement.cpu_fallback) or pager thrash (#441)."
        );
    }
}

/// One streamed SSE frame from an OpenAI-compatible `/v1/chat/completions` with
/// `stream: true`. Each frame carries an incremental `delta`; `usage` arrives only
/// on the final frame (requires `stream_options.include_usage`).
#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
    /// Per-request lane timings (cache_n / prompt_ms / predicted_per_second …);
    /// arrives on the final frame alongside `usage`.
    #[serde(default)]
    timings: Option<OpenAITimings>,
    #[serde(default)]
    model: String,
    /// PREFILL progress (llama.cpp `return_progress` extension). Present only on
    /// frames emitted while the slot is still ingesting the prompt — before any
    /// token exists. This is the ONLY evidence a client has that a long prefill
    /// is advancing rather than wedged; see the liveness rule in
    /// [`OpenAIAdapter::stream_completion`].
    #[serde(default)]
    prompt_progress: Option<OpenAIPromptProgress>,
}

/// llama.cpp's `prompt_progress` frame — the slot's ingest counter.
///
/// `processed` climbs toward `total` as prefill proceeds; `cache` is the prefix
/// the KV cache served for free. A slot that is genuinely wedged holds
/// `processed` FROZEN, which is exactly what makes this a liveness signal and
/// not merely a keepalive.
#[derive(Debug, Deserialize)]
struct OpenAIPromptProgress {
    #[serde(default)]
    total: u64,
    #[serde(default)]
    cache: u64,
    #[serde(default)]
    processed: u64,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    #[serde(default)]
    delta: Option<OpenAIStreamDelta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAIStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamToolCall {
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAIStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// A tool call assembled across many streamed `delta.tool_calls` fragments. The
/// model emits the id + name once and then the JSON `arguments` arrive token by
/// token; we accumulate by `index` until the stream ends.
#[derive(Default)]
struct StreamToolAccum {
    id: String,
    name: String,
    arguments: String,
}

/// Fold one streamed tool-call fragment into the per-index accumulator.
fn accumulate_stream_tool_call(acc: &mut Vec<StreamToolAccum>, tc: OpenAIStreamToolCall) {
    let idx = tc.index.unwrap_or(0);
    if acc.len() <= idx {
        acc.resize_with(idx + 1, StreamToolAccum::default);
    }
    let slot = &mut acc[idx];
    if let Some(id) = tc.id {
        if !id.is_empty() {
            slot.id = id;
        }
    }
    if let Some(f) = tc.function {
        if let Some(n) = f.name {
            if !n.is_empty() {
                slot.name = n;
            }
        }
        if let Some(a) = f.arguments {
            slot.arguments.push_str(&a);
        }
    }
}

#[async_trait]
impl AIProviderAdapter for OpenAICompatibleAdapter {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // The capability set is already projected from the registry at
        // construction (#65) — the adapter just hands it through, adding the
        // scalar/protocol axes. Tool-use drives the native function-calling +
        // JSON-Schema protocols; vision/embeddings/image-gen ride in the set
        // itself, and any modality absent from the set is bridged (vision →
        // VisionDescriptionService, audio → STT/TTS) before the request lands.
        let supports_tools = self.config.capabilities.contains(&Capability::ToolUse);
        AdapterCapabilities::builder()
            .capabilities(self.config.capabilities.iter().copied())
            .remote()
            // Sourced from the served model's declared ceiling (#46) — never a
            // hardcoded per-adapter clamp. The fallbacks only apply when no
            // model row is present (a mis-provisioned adapter), which fails
            // loud downstream anyway.
            .context_window(
                self.config
                    .models
                    .first()
                    .map(|m| m.context_window)
                    .unwrap_or(128_000),
            )
            .max_output_tokens(
                self.config
                    .models
                    .first()
                    .map(|m| m.max_output_tokens)
                    .unwrap_or(16_384),
            )
            // Native function-calling + JSON-Schema when the served model does
            // tools; otherwise the model is a competent chat model and cognition
            // emulates tools/schema in-prompt.
            .protocols(if supports_tools {
                crate::ai::adapter::NativeProtocols::FunctionCalling
            } else {
                crate::ai::adapter::NativeProtocols::PromptEmulated
            })
            .build()
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::OpenAI
    }

    /// The live served window of the lane THIS adapter is bound to (the one source
    /// cognition sizes its prompt to). A DEDICATED lane (`with_dedicated_lane`, an
    /// eval fork's `EphemeralServingLane`) is its own authority — its window was
    /// pinned from ITS `/props` at spawn and rides on the binding, so report `None`
    /// and let that stand; the GLOBAL gateway snapshot describes a DIFFERENT server.
    /// A shared single-resident gateway reports the gateway's CURRENT served slot
    /// (the live `/props` truth), tracked up AND down, so a relaunch is followed
    /// without a clamp. A not-ready / zero snapshot → `None` (the binding window
    /// stands until the next ready tick). Mirrors the `!self.dedicated_lane`
    /// readiness-guard exemption above — same lane, same authority.
    fn live_served_window(&self) -> Option<u32> {
        if self.dedicated_lane || !self.config.single_resident_model {
            return None;
        }
        let s = crate::inference::llama_server::current_serving();
        (s.ready && s.served_context_window > 0).then_some(s.served_context_window)
    }

    /// Reports the LoRA capability the endpoint DISCOVERED about itself (via the
    /// `GET /lora-adapters` probe), never a declared per-provider table. Reads
    /// the cached probe result: `Supported` only after a 200 catalog response,
    /// so the fabric's capability-aware selection sees the truth the endpoint
    /// told us — not a guess. `Unknown`/`Unsupported` → `None`.
    fn lora_capabilities(&self) -> LoRACapabilities {
        match &*self.lora_support.read().unwrap() {
            LoraSupport::Supported(catalog) => LoRACapabilities::MultiLayerPaging {
                max_loaded: catalog.len(),
                supports_hot_swap: true,
            },
            _ => LoRACapabilities::None,
        }
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Only require API key if provider needs auth. Providers without
        // an `api_key_env` in the catalog (localhost DMR, llamacpp-local) skip
        // this entirely — their `requires_auth` is false.
        if self.config.requires_auth {
            let key_env = self.config.api_key_env.as_deref().unwrap_or_else(|| {
                panic!(
                    "provider `{}` requires auth but has no api_key_env in the catalog",
                    self.config.provider_id
                )
            });
            self.api_key = get_secret(key_env).map(|s| s.to_string());
            if self.api_key.is_none() {
                return Err(format!(
                    "{} API key not configured ({})",
                    self.config.name, key_env
                ));
            }
        }

        self.initialized = true;

        // Populate runtime_models for adapters with dynamic catalogs (DMR).
        // Best-effort: if the endpoint isn't reachable right now, init still
        // succeeds — runtime_models stays None → supports_model returns false
        // → registry hard-errors instead of silently routing to this adapter.
        // That's the correct failure mode: don't falsely claim availability.
        // Gated on the TYPED capability (#55), never the provider id.
        if self.config.dynamic_model_catalog {
            if let Err(e) = self.refresh_runtime_models().await {
                clog_warn!(
                    "DMR model catalog fetch failed at init: {}. DMR will report no models available until a successful refresh.",
                    e
                );
            } else {
                let count = self
                    .runtime_models
                    .read()
                    .unwrap()
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or(0);
                clog_info!("DMR live model catalog: {} model(s) available", count);
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        self.initialized = false;
        Ok(())
    }

    /// Convenience drain over [`generate_stream`]: when a caller wants the whole
    /// answer (no live tokens), it streams into a throwaway channel and returns the
    /// assembled response. The channel is unbounded so token decode never blocks on
    /// a reader; the receiver is held to the end of the call and dropped (the
    /// buffered chunks are cheap and discarded). Same SSE path, same liveness
    /// watchdog — just without surfacing the tokens.
    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<GenerationChunk>();
        self.generate_stream(request, tx).await
    }

    /// Streaming-first generation: each token reaches `sink` the INSTANT the
    /// backend decodes it (the low-latency primitive — same shape as audio samples
    /// or video frames), and the fully-assembled [`TextGenerationResponse`] is
    /// returned when the stream completes. Liveness is the per-token idle watchdog
    /// ([`STREAM_IDLE_TIMEOUT_SECS`]), never a wall-clock total.
    async fn generate_stream(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, String> {
        // Only require API key for providers that need auth
        if self.config.requires_auth && self.api_key.is_none() {
            return Err(format!("{} not initialized", self.config.name));
        }

        let start = Instant::now();
        let request_id = request
            .request_id
            .clone()
            .unwrap_or_else(|| format!("req-{}", chrono::Utc::now().timestamp_millis()));
        let raw_model = request
            .model
            .as_deref()
            .unwrap_or(self.config.default_model.as_str());

        // For DMR: resolve the logical model name to the actual model ID
        // stored in Docker Model Runner (which may have hf.co/ prefix and
        // different casing). Persona says "continuum-ai/qwen3.5-4b-code-forged-GGUF",
        // DMR has "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf:latest".
        // If DMR doesn't have the model, resolve returns Err — we propagate
        // it as a fast, explicit failure instead of POSTing an unresolved
        // name and stalling on the 120s request timeout.
        let resolved_model: String = if self.config.dynamic_model_catalog {
            self.resolve_dmr_model_name(raw_model).await?
        } else {
            raw_model.to_string()
        };
        let model: &str = &resolved_model;

        // Native vision is a MODEL fact, not a provider fact: gate image content
        // parts on the TARGET model row's Capability::Vision (the same
        // `sensory::route` verdict that drives the bridge-vs-native table in
        // CLAUDE.md "Sensory Architecture"). Row present → its capability set is
        // the truth (a vision-capable llama-server lane / gpt-4o gets raw
        // pixels; a text row gets its images dropped and reads the description
        // bridge). Row absent (dynamic catalogs like DMR resolve ids the
        // registry never saw) → the provider-level scan ("any row under this
        // provider declares Vision", already folded into `config.capabilities`)
        // is the best available truth — same source `capabilities()` advertises.
        let vision_native = crate::model_registry::try_global()
            .and_then(|reg| {
                reg.model(raw_model).map(|row| {
                    crate::sensory::route(row, crate::sensory::Modality::ImageIn).is_native()
                })
            })
            .unwrap_or_else(|| self.config.capabilities.contains(&Capability::Vision));

        // Build request body
        let mut messages = self.format_messages(
            &request.messages,
            request.system_prompt.as_deref(),
            vision_native,
        );

        // JsonInPrompt tool offering: for gateways/models that ignore the OpenAI
        // `tools` param (unsloth+GGUF), describe the tools IN the prompt and ask
        // for a strict JSON call. Appended as a system message; the matching parse
        // happens on the response below. Native providers skip this (tool_prompt →
        // None) and use the `tools` param instead.
        if let Some(tools) = request.tools.as_ref() {
            if let Some(block) = self.config.tool_protocol.tool_prompt(tools) {
                messages.push(json!({ "role": "system", "content": block }));
            }
        }

        close_trailing_assistant(&mut messages);

        let mut body = json!({
            "model": model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            // Stream tokens the instant they're decoded. `include_usage` makes the
            // backend emit a final usage-only frame so we still get token counts.
            "stream": true,
            "stream_options": { "include_usage": true }
        });

        // max_tokens — the MODEL owns its generation length, enforced server-side
        // by unsloth / llama.cpp / the cloud provider. We forward a ceiling ONLY
        // when the caller set one explicitly; `None` → omit the field so the model
        // runs to its own stop token or context limit. We never invent a default
        // here: the old `.unwrap_or(2048)` was a second clamp duplicating a limit
        // the model already enforces, and it truncated reasoning models mid-`<think>`
        // (qwen3.5 spends ~500 tokens reasoning before the answer → empty reply).
        if let Some(max) = request.max_tokens {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("max_tokens".to_string(), json!(max));
            }
        }

        // stop — the turn-boundary + reserved-marker stop sequences (#150, #158).
        // GLASS-BOXED 2026-07-13: the body above shipped WITHOUT this field, so
        // every stop the deliberation faculty threaded in (peer-name stops so a
        // model can't speak AS teammates; `\n[action`/`\nI ran ` so it can't
        // fabricate receipts) was silently dropped before reaching llama-server —
        // the decode-level hygiene never actually ran on local models. llama.cpp's
        // OpenAI-compatible server honors `stop` as an array of strings; forward it
        // whenever the caller set any.
        if let Some(stops) = &request.stop_sequences {
            if !stops.is_empty() {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("stop".to_string(), json!(stops));
                }
            }
        }

        // DMR-specific: llama.cpp's OpenAI-compatible server accepts the
        // llama.cpp-native `repeat_penalty` field as an extension. Until
        // this patch the POST body shipped ONLY the 5 fields above, so
        // DMR inference ran with repeat_penalty=1.0 (llama.cpp default,
        // disabled) and produced runaway repetition — empirically verified
        // 2026-04-24 on Linux/CUDA Carl stack: qwen3.5-4b-code-forged
        // reprinted the same <think> paragraph 10-40 times then burned
        // max_tokens without emitting a real reply. Meanwhile the
        // in-process llamacpp_adapter path defaults
        // `sampling.repeat_penalty = 1.1` (backends/mod.rs:195,205) and
        // does NOT exhibit this failure mode on Mac Metal. Classic RULE 1
        // divergence (integration test path ≠ production path).
        //
        // Scoped to llama.cpp-family gateways (DMR, llama-server) via the TYPED
        // `llamacpp_sampling_extensions` capability (#55), NOT the provider id:
        // cloud OpenAI-compat providers (openai, groq, xai, fireworks, together)
        // do NOT accept `repeat_penalty` (non-standard field) — some ignore it
        // silently, others reject — so they leave the flag false and the field
        // is omitted. llama-server inherits the same protection DMR had: the
        // forged 4B loops its `<think>` block to the token budget without it.
        if self.config.llamacpp_sampling_extensions {
            if let Some(obj) = body.as_object_mut() {
                apply_llamacpp_sampling_knobs(obj, &request);
                // KV-cache prefix reuse — the llama.cpp-server `cache_prompt`
                // extension. Without it the server re-prefills the ENTIRE prompt
                // from scratch every call. Our system prompt is a long, mostly
                // static prefix (identity + doctrine + tool catalog + roster);
                // re-prefilling it costs ~26-36s on the 14B (~216 tok/s prefill).
                // Measured 2026-06-23, Mac Metal: cache_prompt=false → 36s EVERY
                // call; cache_prompt=true → 36s cold then 0.48s warm (75×), because
                // warm calls prefill only the NEW tail tokens and reuse the matched
                // prefix from the slot's KV cache. The win is only as large as the
                // STABLE prefix — see render_assembled_context_within, which orders
                // static grounding before volatile recall so the cached prefix is
                // maximal. Same typed capability gate as repeat_penalty/lora: cloud
                // OpenAI-compat providers reject the non-standard field.
                obj.insert("cache_prompt".to_string(), json!(true));
                // PREFILL VISIBILITY — the llama.cpp `return_progress` extension.
                // MEASURED 2026-08-13 on the live lane: with this field absent
                // (its server-side default), a ~20k-token prompt produced ZERO
                // bytes for 286 SECONDS — the server says nothing at all between
                // accepting the request and emitting the first token. Both stream
                // watchdogs below budget 90s, so any prompt whose prefill exceeds
                // that was killed by US mid-ingest, and the server log shows
                // exactly that: `progress = 0.73 … 145 tok/s` immediately followed
                // by `srv stop: cancel task`. The retry then re-prefills from
                // scratch (the cancel also evicts the slot's prompt cache), so the
                // turn could never succeed — a citizen at full window occupancy was
                // structurally incapable of producing a token.
                //
                // With the field set, the slot emits a `prompt_progress` frame per
                // batch iteration (server-context.cpp:3703) — bytes AND a rising
                // `processed` counter, which is what makes the watchdog able to
                // tell healthy prefill from the #385 wedge instead of failing both.
                // Same typed capability gate as cache_prompt/repeat_penalty: cloud
                // OpenAI-compat providers reject non-standard fields.
                obj.insert("return_progress".to_string(), json!(true));
            }
            // Slot pinning (`id_slot`, llama.cpp extension): the warm KV in a slot
            // belongs to an ACTIVITY, not a persona. Rooms are 1:1 with activities in
            // continuum/airc, so the warm-context identity is (persona, room) — a
            // persona running N concurrent activities (e.g. N detached benchmark
            // solves, each in its own room) has N independently-growing prefixes and
            // needs N slots. Keying the lease on the bare persona collapsed all N onto
            // ONE slot, and each activity's turn clobbered the others' warm tail —
            // measured 2026-08-26 as cached:0 across EVERY turn of a 4-instance
            // dispatch even though the pin (persona→slot 0) was landing correctly.
            // The server reuses partial prefixes fine (proven same day: an 800-token
            // shared prefix reused, only the divergent tail re-prefilled), so the
            // defect was never the prompt or the server — it was N activities sharing
            // one slot. Non-persona traffic (evals, probes) stays unpinned so it can't
            // evict a citizen's warm slot.
            // TRAFFIC CLASS decides placement (slots::class_for over `purpose` —
            // one data map, no per-callsite hacks): only a TURN may hold or evict
            // a citizen's activity slot; every other class lands on the reserved
            // SCRATCH slot so it structurally cannot truncate a warm tail. The
            // measured defect: sidecar gate calls pinned the turn's own slot and
            // cut its ~30k tail to their common head — reuse broke even solo.
            let class = crate::inference::slots::class_for(request.purpose.as_deref());
            // MEASURED WORK HOLDS THE CORE (restore-economy Phase 1.a). Deferral
            // sits HERE — after classification, BEFORE the concurrency permit
            // below — so a parked Background/Probe request holds NOTHING while it
            // waits: no lock, no decode permit, just its own task on a notify.
            // Parking after the permit would be priority inversion (a sleeping
            // dream holding the only decode slot while a real turn queues behind
            // it — the exact too-serial hazard Joel flagged). Turn/Sidecar never
            // enter the await at all, and release wakes ALL waiters (broadcast,
            // not FIFO), so deferred work re-races admission rather than
            // draining serially. Measured cause: dream-belief-review took 52 of
            // 109 generations DURING a held solve, ~32.9s re-prefill per clobber.
            let hold_caller = request
                .persona_id
                .as_deref()
                .and_then(|p| uuid::Uuid::parse_str(p).ok());
            crate::inference::measured_hold::defer_while_held(
                class,
                hold_caller,
                request.purpose.as_deref(),
            )
            .await;
            let placement: Option<u32> = match class {
                crate::inference::slots::SlotClass::Turn => {
                    // The typed activity key: (persona, room) as UUID structs — never
                    // a formatted string (guarded by
                    // `no_string_composite_id_keys_in_serving`). A Turn that cannot
                    // name BOTH halves goes unpinned (and the probe says so).
                    let key = request
                        .persona_id
                        .as_deref()
                        .and_then(|p| uuid::Uuid::parse_str(p).ok())
                        .zip(
                            request
                                .room_id
                                .as_deref()
                                .and_then(|r| uuid::Uuid::parse_str(r).ok()),
                        )
                        .and_then(|(p, r)| crate::inference::slots::ActivityKey::new(p, r));
                    match key {
                        Some(k) => {
                            let leased = self.slot_for_activity(k).await;
                            if leased.is_some() {
                                // Price basis for the eviction policy (B5): this
                                // activity's current prompt size, the same chars/4
                                // estimate the overshoot alarm uses — comparable
                                // across slots, which is all eviction needs.
                                let approx_tokens = body
                                    .get("messages")
                                    .and_then(|m| m.as_array())
                                    .map(|msgs| {
                                        msgs.iter()
                                            .filter_map(|m| {
                                                m.get("content").and_then(|c| c.as_str())
                                            })
                                            .map(|c| c.len() / 4)
                                            .sum::<usize>()
                                    })
                                    .unwrap_or(0) as u64; // 0 = no usage block in the reply; the estimate only prices eviction, never budgets
                                let root = self.endpoints().root().to_string();
                                if let Some(Some(pool)) =
                                    crate::inference::slots::directory().get(&root)
                                {
                                    pool.note_tail(&k, approx_tokens);
                                }
                            }
                            leased
                        }
                        None => None,
                    }
                }
                _ => {
                    // Non-Turn: the scratch slot when this server reserved one. When
                    // it did not (≤2 slots), stay unpinned AND drop cache_prompt so
                    // the call cannot PERSIST a stolen cache into a citizen slot.
                    let scratch = self.scratch_slot_for_root();
                    if scratch.is_none() {
                        if let Some(obj) = body.as_object_mut() {
                            obj.insert("cache_prompt".to_string(), json!(false));
                        }
                    }
                    scratch
                }
            };
            // GLASS BOX (KV-reuse 0% hunt 2026-08-26): the whole cache-reuse win
            // rides on placement. Report class + room so a cached:0 streak names
            // WHICH activity thrashed — or which class strayed off scratch.
            crate::probe!(
                class = "inference.slot_pin.decision",
                persona = request.persona_id.as_deref(),
                room = request.room_id.as_deref(),
                traffic = class.as_str(),
                pinned = placement.is_some(),
                slot = placement.map(|s| s as u64),
                "id_slot decision — Turn pins its activity slot; every other class lands on scratch"
            );
            if let Some(slot) = placement {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("id_slot".to_string(), json!(slot));
                }
            }
            if let Some(persona) = request.persona_id.as_deref() {
                // #139 overshoot alarm: name the RAG-budget bug BEFORE the
                // server rejects. With context shift disabled at spawn the
                // server 400s on overflow instead of silently amputating the
                // prompt's middle; this WARN turns that 400 from a mystery
                // into a diagnosis. Chars/4 is a deliberately conservative
                // token estimate — an alarm that only fires when the overshoot
                // is unambiguous.
                if let Some(served) = served_ctx_by_root().get(self.endpoints().root()) {
                    let approx_tokens = body
                        .get("messages")
                        .and_then(|m| m.as_array())
                        .map(|msgs| {
                            msgs.iter()
                                .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
                                .map(|c| c.len() / 4)
                                .sum::<usize>()
                        })
                        .unwrap_or(0);
                    if approx_tokens > *served as usize {
                        tracing::warn!(
                            probe_class = "serving.ctx_overshoot",
                            approx_tokens,
                            served_per_slot_ctx = *served,
                            persona,
                            "prompt likely exceeds the served per-slot window — the RAG \
                             budget overshot what llama-server actually serves (#139); \
                             expect a context-size rejection, fix the budget not the server"
                        );
                    }
                }
            }
        }

        // LoRA page-in: project the persona's genome onto the serving backend as
        // the llama.cpp `lora` request-body extension — the SAME backend-extension
        // mechanism as `repeat_penalty` above. The integer `id` is the server-side
        // load-index (discovered via GET /lora-adapters), which the CUSTODIAN
        // assigns when it loads the adapter. `lora_scale_vector` emits an explicit
        // scale for EVERY loaded adapter (requested scale, else 0.0) so an EMPTY
        // genome serves true base: llama.cpp applies a loaded adapter at 1.0 for
        // any request that omits the field, so omission cannot mean "off" once one
        // is loaded — that omission was the no-op behind the LIFT=0 measurement
        // (base arm sent no field → silently ran WITH the gene). Capability is
        // discovered, not declared: it probes the endpoint and FAILS LOUD if the
        // backend can't page LoRA or a requested adapter isn't loaded. `None` only
        // when nothing is loaded (omit the field, no probe penalty for non-LoRA).
        let active = request.active_adapters.as_deref().unwrap_or(&[]);
        if let Some(entries) = self.lora_scale_vector(active).await? {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("lora".to_string(), json!(entries));
            }
        }

        // Thinking suppression — the REAL lever for qwen3-family forged templates.
        // When this gateway suppresses reasoning, the adapter already appends the
        // `/no_think` soft-switch to the last user turn (build path above). But the
        // forged qwen3.5 chat template implements `enable_thinking`, NOT the
        // `/no_think` text token — so the soft-switch is a NO-OP for it, and absent
        // the kwarg the template's default branch OPENS `<think>` itself, forcing the
        // model to reason. Verified empirically 2026-06-27 on the CPU eval lane: the
        // 4B forged model spent its whole ~90-token budget in the `reasoning` channel
        // and emitted EMPTY `content` (`finish_reason: stop`), so every settled answer
        // was blank and base/gene/lift were all 0.0 — a broken measurement, not a real
        // null result. The chat-template hatch `enable_thinking=false` makes the
        // template emit an empty `<think></think>` so the model goes straight to
        // content. Set it for ALL turns under suppression (not only the JSON branch
        // below, which is where it used to be misgated). Harmless where unsupported:
        // cloud providers ignore unknown body fields; a template without
        // `enable_thinking` ignores the kwarg. The `/no_think` switch is left in place
        // for any template that DOES honor the soft token.
        if self.config.thinking == ThinkingMode::Suppress {
            apply_enable_thinking_false(&mut body);
        }

        // Forward response_format when set. Llama.cpp/DMR DO grammar-constrain
        // JSON output, but for qwen3.5 reasoning models the model still
        // emits its <think> reasoning BEFORE the constrained JSON region,
        // which is no help to a JSON parser. Verified empirically 2026-04-19:
        // `response_format=json_object` alone returns "<think>\nThinking
        // Process:..." with no JSON.
        if let Some(format) = &request.response_format {
            if let Ok(value) = serde_json::to_value(format) {
                body["response_format"] = value;

                // qwen3-family-specific kicker: when caller asks for JSON,
                // ALSO disable thinking via the chat_template_kwargs hatch.
                // Verified the same model returns "<think></think>\n\n{...JSON...}"
                // in 434ms with this flag set — empty think block, clean JSON,
                // parser-friendly. Same lever the suppression path above uses, so
                // it routes through the same helper (one place sets the kwarg).
                // Idempotent if suppression already set it.
                apply_enable_thinking_false(&mut body);
            }
            // Diagnostic — print the request body exactly as serialized so we
            // can see which fields actually reach DMR. Helps catch silent
            // serialization drops (caught one 2026-04-19 — entry chain wasn't
            // mutating body in place).
            tracing::info!(
                target: "openai_adapter",
                "request body to {}: {}",
                self.config.name,
                serde_json::to_string(&body).unwrap_or_default()
            );
        }

        // Add tools via the native OpenAI `tools` param — ONLY for
        // NativeFunctionCalling providers. JsonInPrompt providers already had
        // the tools described in the prompt above (sending the param too would
        // be ignored or confuse them).
        if let Some(tools) = &request.tools {
            if !tools.is_empty()
                && self.config.capabilities.contains(&Capability::ToolUse)
                && self.config.tool_protocol
                    == crate::model_registry::ToolProtocol::NativeFunctionCalling
            {
                let openai_tools: Vec<Value> = tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.input_schema
                            }
                        })
                    })
                    .collect();
                body["tools"] = json!(openai_tools);

                // Add tool_choice if specified
                if let Some(choice) = &request.tool_choice {
                    match choice {
                        ToolChoice::Mode(mode) => {
                            body["tool_choice"] = json!(mode);
                        }
                        ToolChoice::Specific { name } => {
                            body["tool_choice"] = json!({
                                "type": "function",
                                "function": { "name": name }
                            });
                        }
                    }
                }
            }
        }

        // Wire truth for the tool surface (glass-box, 2026-08-03): live residents
        // narrated for hours with zero tool calls while every offline replay of the
        // same context+tools+sampling called instantly — the ONLY remaining unknown
        // was what this body actually carried. This probe states it per request so
        // "tools offered" is never inferred from a capture again.
        crate::probe!(
            class = "ai.request.tool_surface",
            model = %model,
            tools_n = body.get("tools").and_then(|t| t.as_array()).map_or(0, |a| a.len()),
            tool_choice = body.get("tool_choice").is_some(),
            stops_n = body.get("stop").and_then(|s| s.as_array()).map_or(0, |a| a.len()),
            msgs_n = body.get("messages").and_then(|m| m.as_array()).map_or(0, |a| a.len()),
            temperature = body.get("temperature").and_then(|t| t.as_f64()).unwrap_or(-1.0),
            "outbound chat request tool surface"
        );

        // Make request — the endpoint base for THIS request's model. Normally the
        // runtime/config base; for the local serving gateway, a request for the
        // snapshot's VISION model (the #106 sidecar lane serving beside a
        // text-only mind) routes to the snapshot's verified `vision_base_url`.
        // Snapshot-driven: the daemon publishes the address only after `/props`
        // confirmed sight, so this can never aim pixels at a text lane.
        let url = self.endpoints_for_model(&model).chat_completions();

        let mut request_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        // Only add Authorization header if provider requires auth
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        // Log the body size + model so post-mortem can reconstruct why a
        // stall happened (oversized prompt, wrong model, etc.). Kept at
        // info! because this is the one log line every failing-persona
        // investigation needs to see.
        let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
        clog_info!(
            "POST {} model={} body_bytes={} has_tools={} stream={}",
            url,
            model,
            body_bytes.len(),
            body.get("tools")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
                > 0,
            body.get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        );

        // Acquire concurrency slot. For DMR (1 slot) this serializes
        // requests so the idle watchdog measures actual streaming liveness,
        // not "time waiting for the previous persona's forward pass." For
        // non-DMR providers (64 slots) this is effectively a no-op. Acquire
        // can't fail here — the semaphore is never closed over the adapter's
        // lifetime.
        let queue_start = Instant::now();
        let _permit = self
            .concurrency
            .clone()
            .acquire_owned()
            .await
            .expect("adapter semaphore never closed");
        let queued_ms = queue_start.elapsed().as_millis();
        if queued_ms > 100 {
            clog_info!(
                "concurrency gate waited {}ms before POST to {}",
                queued_ms,
                self.config.provider_id
            );
        }

        // Pre-flight the single-resident gateway: GUARANTEE our model is the one
        // actually serving before we trust a generation. The local gateway
        // (llama-server) serves ONE resident model, fixed at process launch, and
        // answers EVERY request as that model regardless of the request's `model`
        // field — so generating while a DIFFERENT model (or none) is live silently
        // returns the wrong brain (the bug that would haunt us). Crucially,
        // switching the served model is a *process relaunch* the
        // ServingDaemonModule owns (Contract A `inference::llama_server`); an
        // adapter must NEVER drive that load from inside a generate — relaunching
        // would kill the GPU-warm server out from under every other persona on the
        // shared gateway. So this guard is READ-ONLY: consult the daemon's
        // published serving snapshot (a `watch` borrow, no probe) and refuse to
        // generate unless OUR model is the READY, ACTIVE one. A mismatch is a loud
        // failure naming the cause, never a silent wrong-model answer
        // ([[fallbacks-are-illegal-fail-loud]]). Bringing the right model up — and
        // cross-persona residency arbitration on the shared gateway — is the
        // serving layer's job (#109), not this gate.
        // A dedicated lane (eval's EphemeralServingLane) is its OWN authority:
        // launched with exactly this model and confirmed HTTP-ready at spawn, so
        // the GLOBAL serving snapshot (which only knows the living persona lane) is
        // the wrong thing to consult. Skip the guard for a lane we own.
        if self.config.single_resident_model && !self.dedicated_lane {
            let snap = crate::inference::llama_server::current_serving();
            // The snapshot guarantees TWO residencies: the main lane's active
            // model, and (when published) the verified vision endpoint's model —
            // the #106 sidecar lane beside a text-only mind. A request for the
            // sidecar's model is exactly as guaranteed as one for the active
            // model (the daemon verified its `/props` before publishing), and
            // `endpoints_for_model` routes it to `vision_base_url`.
            let mut snap = snap;
            if !snapshot_guarantees(&snap, model) && !settled_on_another_model(&snap) {
                // A TRANSITION, not a fault — wait it out instead of failing the turn.
                // `await_ready_serving` is the daemon's own readiness signal (the same
                // `watch` it publishes): a push, not a poll, so this returns the instant the
                // relaunch lands and costs nothing while it waits. It is the mechanism the
                // boot gate, the eval lane, the embedder and the genome teacher all already
                // wait on; this seam was the one that refused instead.
                //
                // Budget is `DEFAULT_SERVING_WAIT` (= READY_TIMEOUT + 30s) on purpose: it is
                // DERIVED from the spawner's own load budget, so this can never declare a
                // failure before the daemon has exhausted its legitimate window to produce a
                // lane. Fail LOUD, not FAST. Bounded, so a lane that never returns still ends
                // as a named refusal rather than a hung generate.
                let started = std::time::Instant::now();
                let settled = crate::inference::llama_server::await_ready_serving(
                    crate::inference::llama_server::DEFAULT_SERVING_WAIT,
                )
                .await;
                if let Some(s) = settled {
                    snap = s;
                }
                // Carry the daemon's own stated degradation on the probe: an
                // unresolved wait with NO reason is "polling slop" a reader must go
                // spelunking to explain, while `degraded=` names the killer in the
                // stream itself (2026-08-15: every turn of a round waited here for
                // 120s each while serving/status knew the exact cause — a failed
                // decode smoke-probe — and nothing surfaced it).
                let degraded = snap.degraded_reason.as_deref().unwrap_or("");
                crate::probe!(
                    class = "inference.awaiting_serving_transition",
                    provider = self.config.provider_id.as_str(),
                    wanted = model,
                    waited_ms = started.elapsed().as_millis() as u64,
                    served_before = crate::inference::llama_server::has_reconciled(),
                    resolved = snapshot_guarantees(&snap, model),
                    degraded = &degraded[..degraded.len().min(200)],
                    "no lane was resident at pre-flight (serving transition) — waited on the \
                     daemon's readiness signal rather than failing the turn"
                );
            }
            if !snapshot_guarantees(&snap, model) {
                return Err(unguaranteed_model_refusal(
                    &self.config.name,
                    model,
                    &snap,
                    crate::inference::llama_server::has_reconciled(),
                ));
            }
            // #175 universal overflow backstop: REFUSE (never send) a prompt that alone
            // exceeds the served per-slot window. With context-shift OFF the server 500s
            // "Compute error" on overflow AND the fault POISONS the slot, so every LATER
            // request 500s too — one oversized prompt from ANY caller (a persona turn, a
            // dream distillation, an eval) takes the whole shared lane down until a
            // restart (the wedge storm this task chased). The persona deliberation path
            // already fits its prompt to the live window; this is the chokepoint backstop
            // for the ~10 OTHER callers that build their own prompts (dream_consolidation,
            // check_redundancy, validate_response, …), which the persona-scoped overshoot
            // WARN below never covered. A refused request fails LOUD naming the caller and
            // never reaches llama_decode, so the slot stays healthy. Threshold is PROMPT
            // ALONE ≥ window (unambiguous — no room for even the prompt, let alone a
            // reply), so a legitimately-budgeted request is never blocked. chars/4 is the
            // same conservative estimate the overshoot alarm uses.
            // [[fallbacks-are-illegal-fail-loud]] [[llama-compute-error-wedge-is-per-slot-context-overflow]]
            if let Some(prompt_tokens) =
                prompt_alone_overflows_served(&body, snap.served_context_window)
            {
                return Err(format!(
                    "{}: refusing to generate — prompt ~{} tokens ≥ the served per-slot \
                     window of {} (caller: {}). Sending it would 500 and POISON the shared \
                     slot for every later request; fit the prompt to the served window (#175).",
                    self.config.name,
                    prompt_tokens,
                    snap.served_context_window,
                    request.persona_id.as_deref().unwrap_or("non-persona"),
                ));
            }
        }

        // A CONNECT error to a local resident lane means the lane is mid-relaunch, not
        // gone (see LANE_RELAUNCH_CONNECT_RETRIES) — the connection never opened so
        // nothing streamed, and re-sending the same lane is idempotent. Ride it out with
        // bounded linear backoff, then fail loud. `request_builder` carries no body yet
        // (`.json` is applied per attempt below), so `try_clone` always succeeds.
        // Shared budget for BOTH mid-relaunch signatures (connection refused = nothing
        // listening yet; 503 = listening but still loading). One counter, so the total
        // time this call can spend waiting on a relaunching lane stays bounded.
        let mut relaunch_retries: u32 = 0;
        let response = loop {
            let send_start = Instant::now();
            let attempt_builder = request_builder
                .try_clone()
                .expect("bodyless request builder is always cloneable");
            // BOUNDED pre-first-byte wait: a poisoned lane can accept the request
            // and never return headers (hung prefill) — with no bound here, the
            // caller's ServingLanePermit is held FOREVER and one wedged call
            // starves the whole roster's admission (glass-boxed 2026-07-23: the
            // eternal `nondirected_waiting` park). The stream idle-watchdog only
            // arms AFTER headers; this is its pre-stream twin. Generous (prefill
            // of a full window on a busy co-tenant lane is minutes, not seconds)
            // but FINITE — RTOS rule: every hold is bounded.
            let sent = tokio::time::timeout(
                std::time::Duration::from_secs(PRE_STREAM_HEADER_TIMEOUT_SECS),
                attempt_builder.json(&body).send(),
            )
            .await
            .map_err(|_| {
                format!(
                    "{}: no response headers for {}s after POST — lane accepted the                      request and went silent (hung prefill / poisoned backend);                      releasing the lane instead of holding it forever",
                    self.config.name, PRE_STREAM_HEADER_TIMEOUT_SECS
                )
            })?;
            match sent {
                // A relaunching lane refuses the connection only while nothing is
                // LISTENING. Once the new process binds, it accepts and answers
                // 503 while it mmaps weights and warms the backend — the SAME
                // mid-relaunch state one layer up, with a completely different
                // signature. Observed live 2026-08-07: a re-home grew the window
                // 16384 → 27136 → 32768, and during the respawn three citizens
                // took `503 {"error":{"message":"Loading model..."}}` as a hard
                // `selftick.inference_failed` while the published snapshot still
                // said `ready` (it is a cached claim — see ServingSnapshot::ready).
                //
                // 503 from a SINGLE-RESIDENT local lane means "not available yet"
                // by definition, so the status alone is the signal — no sniffing
                // the body text for "Loading model"
                // ([[a-string-matcher-for-a-semantic-judgement-means-a-channel-is-missing]]:
                // the HTTP status IS the structured channel). Shares the connect
                // arm's retry budget, because both are the same wait for the same
                // lane and the total hold must stay bounded.
                Ok(resp)
                    if resp.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
                        && self.config.single_resident_model
                        && relaunch_retries < LANE_RELAUNCH_CONNECT_RETRIES =>
                {
                    relaunch_retries += 1;
                    let backoff = LANE_RELAUNCH_RETRY_BASE * relaunch_retries;
                    crate::probe!(
                        class = "inference.lane_relaunch_retry",
                        provider = self.config.provider_id.as_str(),
                        attempt = relaunch_retries,
                        backoff_ms = backoff.as_millis() as u64,
                        reason = "503_loading",
                        "local lane is up but still loading (503, mid-relaunch) — retrying the \
                         same lane",
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                Ok(resp) => break resp,
                Err(e)
                    if e.is_connect()
                        && self.config.single_resident_model
                        && relaunch_retries < LANE_RELAUNCH_CONNECT_RETRIES =>
                {
                    relaunch_retries += 1;
                    let backoff = LANE_RELAUNCH_RETRY_BASE * relaunch_retries;
                    crate::probe!(
                        class = "inference.lane_relaunch_retry",
                        provider = self.config.provider_id.as_str(),
                        attempt = relaunch_retries,
                        backoff_ms = backoff.as_millis() as u64,
                        reason = "connect_refused",
                        "local lane refused the connection (mid-relaunch) — retrying the same lane",
                    );
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                Err(e) => {
                    // reqwest::Error's top-level Display often collapses the
                    // real cause (timeout vs connect vs body-write) into a
                    // generic "error sending request" string. Walk the error
                    // source chain so the log shows the actual terminal
                    // reason — critical for debugging stalls where the
                    // outer message alone is useless.
                    let mut chain: Vec<String> = vec![e.to_string()];
                    let mut cur: &dyn std::error::Error = &e;
                    while let Some(src) = cur.source() {
                        chain.push(src.to_string());
                        cur = src;
                    }
                    return Err(format!(
                        "{} POST failed after {}ms{}: {} (kind: timeout={}, connect={}, request={}, body={})",
                        self.config.name,
                        send_start.elapsed().as_millis(),
                        if relaunch_retries > 0 {
                            format!(
                                " ({relaunch_retries} mid-relaunch retries exhausted — lane never came back)"
                            )
                        } else {
                            String::new()
                        },
                        chain.join(" -> "),
                        e.is_timeout(),
                        e.is_connect(),
                        e.is_request(),
                        e.is_body()
                    ));
                }
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            // Classify ONCE, here, where the status code and the raw body still
            // exist. Downstream this is still carried as a String (the trait's
            // error type has not moved yet — that is the threading commit), so
            // the CLASSIFICATION is emitted as a probe rather than lost: an
            // operator reading the receipt now sees WHICH kind of failure this
            // was, and for an overflow, both token counts.
            //
            // Why that matters: settle.rs retries every fault blind, which is
            // right for a transient wedge (#386, ~2/3 recover) and useless for
            // ContextExceeded — same prompt, same slot, same 400, forever.
            // Until the type reaches settle, this probe is the only place the
            // difference is visible at all.
            let classified = crate::ai::inference_error::InferenceError::from_http(
                status.as_u16(),
                &body,
            );
            let (requested, available) = match &classified {
                crate::ai::inference_error::InferenceError::ContextExceeded {
                    requested,
                    available,
                } => (*requested, *available),
                _ => (0, 0),
            };
            crate::probe!(
                class = "ai.request.rejected",
                provider = %self.config.name,
                status = status.as_u16(),
                retryable_unchanged = classified.is_retryable_unchanged(),
                requested_tokens = requested,
                available_tokens = available,
                "backend rejected the request — classified at the seam"
            );
            return Err(format!("{} returned {}: {}", self.config.name, status, body));
        }

        // Consume the SSE stream: every token reaches `sink` the INSTANT it arrives.
        // Liveness is the per-token idle watchdog ([`STREAM_IDLE_TIMEOUT_SECS`]) —
        // silence means the backend died, NOT that generation is simply long.
        use futures::StreamExt;
        let mut byte_stream = response.bytes_stream();
        // QUEUE WAIT IS NOT SILENCE. Two different things were being policed by one
        // budget. MEASURED 2026-08-13 on the live 1-slot lane: a TINY (2,237-token)
        // request got its first byte of any kind at t=115.2s — not because prefill
        // was slow (it finished within the same second) but because the slot was
        // busy with a co-tenant's turn for 115s first. llama-server says nothing
        // while a task is queued; there is no frame to send until a slot picks it
        // up. So the 90s liveness budget was being spent on CONTENTION, and with
        // total_slots=1 and four citizens it expires routinely on a healthy lane.
        //
        // Split by what the silence MEANS. Before the server shows any sign of
        // working on THIS request, the bound is the same one the header wait
        // already uses and justifies — queue wait is a capacity fact, minutes are
        // legitimate, and the job is releasing eternal holds, not policing slow
        // ones. Once the slot IS working (a prefill-progress frame or a token),
        // the tight liveness budget applies: from then on, silence really is the
        // backend dying, which is what #385 was always about.
        let queue_budget = std::time::Duration::from_secs(PRE_STREAM_HEADER_TIMEOUT_SECS);
        let live_budget = std::time::Duration::from_secs(STREAM_IDLE_TIMEOUT_SECS);
        let mut idle = queue_budget;
        // #363: real-delivery accounting for the LOCAL lane only. A terminal stream
        // death on the local lane is wedge evidence the smoke probe cannot see (an
        // undersized slot passes a tiny probe while rejecting real prompts); a
        // completed stream is proof of life. Both stamps are gated on this request
        // actually targeting the published serving lane.
        let local_lane = self.targets_local_serving_lane();

        let mut sse_buf: Vec<u8> = Vec::new();
        let mut acc_content = String::new();
        let mut acc_reasoning = String::new();
        let mut acc_tools: Vec<StreamToolAccum> = Vec::new();
        let mut finish_reason_str: Option<String> = None;
        let mut stream_usage: Option<OpenAIUsage> = None;
        let mut stream_timings: Option<OpenAITimings> = None;
        let mut resp_model: Option<String> = None;

        // #385 (the 5-hour wedge): the timeout below bounds TRANSPORT silence, but
        // any bytes reset it — and a wedged slot that keeps emitting keepalives /
        // comment frames (n_decoded frozen at 1 for HOURS, 2026-08-09) resets it
        // forever. Liveness must be keyed on PROGRESS: `last_progress` advances only
        // when a parsed event yields an actual delta (content / reasoning / tool /
        // finish). Bytes without progress for the same idle budget = the
        // keepalive-masked wedge, failed as loudly as transport silence.
        //
        // PREFILL IS PROGRESS (2026-08-13). The rule above was right about the
        // wedge and wrong about what "progress" means: it counted only DECODED
        // tokens, so a slot legitimately ingesting a long prompt looked identical
        // to a frozen one. It isn't: prefill has a rising counter. We now request
        // `return_progress` (see the body builder) and treat a rising `processed`
        // as progress — the slot is doing the work we asked for. A genuinely
        // wedged slot holds that counter FROZEN and still fails in the same 90s,
        // so the #385 detector keeps its teeth while healthy work stops being
        // executed for the crime of having a big prompt.
        let mut last_prefill_processed: u64 = 0;
        let stream_opened = Instant::now();
        let mut last_progress = stream_opened;
        // PREFILL IS NOT DECODE (2026-08-21, the round-killer). `live_budget` is a
        // DECODE watchdog by its own doc — "a token every few hundred ms". Selecting
        // it the moment ANY progress arrived meant llama.cpp's 0%-ingestion signalling
        // frame dropped us from 300s to 90s seconds into a prefill that measured ~170s
        // for a window-sized prompt, so every big turn died and retried forever. The
        // phase machine keeps the two regimes apart; see `inference::stream_liveness`.
        // Stream attribution for the prefill probe: without it every cached%
        // sample is anonymous, and the 2026-08-23 KV iteration spent a round
        // unable to tell Atlas's task acts from Benchy's ambient turns. One
        // clone per stream open — cold path.
        let probe_persona: String = request
            .persona_id
            .clone()
            .unwrap_or_else(|| "non-persona".into()); // non-persona callers (CLI, probes) are a real class, labeled honestly
        let probe_purpose: String = request.purpose.clone().unwrap_or_default(); // absent purpose renders empty — a label, never a quantity
        let mut phase = crate::inference::stream_liveness::StreamPhase::Queued;
        // One `inference.prefill.rescued` row per stream, not per frame.
        let mut prefill_rescued = false;
        // When the FIRST prompt_progress frame arrived. llama.cpp emits the 0% frame
        // at the moment the slot is ASSIGNED ("signal the client that the request has
        // started processing"), so open→first-frame is QUEUE WAIT and
        // first-frame→complete is INGEST. The first cut of these probes conflated the
        // two into one `elapsed_ms`, and the numbers were absurd in exactly the way a
        // conflation is: a 467-token prompt "prefilling" for 231s at 1 tok/s. It was
        // queued 230 of those seconds. A probe that mixes two regimes measures neither.
        let mut first_prefill_frame: Option<Instant> = None;
        loop {
            let idle = crate::inference::stream_liveness::idle_budget(
                phase,
                queue_budget,
                live_budget,
            );
            let next = tokio::time::timeout(idle, byte_stream.next())
                .await
                .map_err(|_| {
                    let started = phase.has_started();
                    if local_lane {
                        if started {
                            // Started-then-stopped is per-slot evidence about OUR
                            // generation — always counts toward the relaunch threshold.
                            crate::inference::llama_server::note_real_decode_failure();
                        } else {
                            // Never-started is ambiguous: dead backend vs oversubscribed
                            // queue. Judge it by the lane's own delivery record — if real
                            // tokens came out for ANYONE while we waited, the lane is
                            // provably alive and this is starvation, not a wedge. Stamping
                            // starvation as wedge evidence relaunched a healthy busy lane
                            // every 2 minutes (bench-hard-rs, 2026-08-15) and killed the
                            // in-flight generations that proved it healthy.
                            use crate::inference::llama_server::NeverStartedClass;
                            match crate::inference::llama_server::classify_never_started_timeout(
                                crate::inference::llama_server::ms_since_real_work(),
                                idle.as_millis() as u64,
                            ) {
                                NeverStartedClass::WedgeEvidence => {
                                    crate::inference::llama_server::note_real_decode_failure();
                                }
                                NeverStartedClass::Starved => {
                                    // The capacity shortfall stays LOUD on its own channel
                                    // (#234 QoS reads this) — it just stops masquerading as
                                    // lane death.
                                    crate::probe!(
                                        class = "inference.queue_starved",
                                        provider = self.config.name.as_str(),
                                        waited_s = idle.as_secs(),
                                        "never-started timeout on a lane that delivered real \
                                         tokens within the wait — oversubscription, not wedge \
                                         evidence; no real-turn failure stamped",
                                    );
                                }
                            }
                        }
                    }
                    format!(
                        "{}: inference lane went silent for {}s (no bytes at all) — {}; \
                         refusing to wait on a dead stream",
                        self.config.name,
                        idle.as_secs(),
                        if started {
                            "the slot HAD started our work and then stopped mid-stream, \
                             so the backend is stuck or dead"
                        } else {
                            "the slot never started our work — either the backend is dead \
                             or the queue is oversubscribed far beyond this budget"
                        }
                    )
                })?;
            if last_progress.elapsed() >= idle {
                if local_lane {
                    crate::inference::llama_server::note_real_decode_failure();
                }
                return Err(format!(
                    "{}: no PROGRESS for {}s despite the stream carrying bytes — \
                     keepalive-masked wedge (neither prefill nor decode advanced); \
                     refusing to wait on a stream that is alive but not working (#385)",
                    self.config.name,
                    idle.as_secs()
                ));
            }
            let Some(chunk) = next else {
                break; // server closed the stream (EOF) — generation complete
            };
            let bytes = chunk.map_err(|e| {
                if local_lane {
                    crate::inference::llama_server::note_real_decode_failure();
                }
                format!("{}: stream read error: {e}", self.config.name)
            })?;

            // Strip CR (0x0D) so event boundaries normalize to `\n\n`. CR never
            // appears inside a UTF-8 multibyte sequence, so this is decode-safe; we
            // buffer RAW bytes and only decode COMPLETE events (no mid-char split).
            for b in bytes.iter() {
                if *b != b'\r' {
                    sse_buf.push(*b);
                }
            }

            while let Some(pos) = sse_buf.windows(2).position(|w| w == b"\n\n") {
                let event_bytes: Vec<u8> = sse_buf.drain(..pos + 2).collect();
                let event = String::from_utf8_lossy(&event_bytes);
                for line in event.lines() {
                    let Some(data) = line.trim_start().strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    let parsed: OpenAIStreamChunk = match serde_json::from_str(data) {
                        Ok(p) => p,
                        Err(_) => continue, // keepalive / comment / non-JSON line
                    };
                    if resp_model.is_none() && !parsed.model.is_empty() {
                        resp_model = Some(parsed.model.clone());
                    }
                    if let Some(u) = parsed.usage {
                        stream_usage = Some(u);
                    }
                    if let Some(t) = parsed.timings {
                        stream_timings = Some(t);
                    }
                    // Prefill advanced → the slot is alive and working. Strictly
                    // `>` so a REPEATED frame carrying the same count (the wedge
                    // signature) cannot hold the watchdog open forever.
                    if let Some(p) = parsed.prompt_progress {
                        // Phase advances on EVERY frame (including the 0% signalling
                        // one), because "the slot is assigned and ingesting" is true
                        // from the first frame — that is what picks the bulk budget.
                        // The liveness STAMP below still requires strict advance, so a
                        // frozen counter cannot hold the watchdog open (#385).
                        let was = phase;
                        phase = phase.on_prefill(p.processed, p.total);
                        let first_frame = *first_prefill_frame.get_or_insert_with(Instant::now);

                        // PROVE THE FIX IS LOAD-BEARING. Without this the change is
                        // invisible: "no retries" is an ABSENCE, and an absence cannot
                        // distinguish "prefills now survive" from "nothing is running"
                        // — the exact ambiguity that cost a full day of diagnosis.
                        // This fires ONCE per stream, only for a prefill that has
                        // already outlived the OLD decode budget while still advancing.
                        // Every row is therefore a turn that would previously have been
                        // killed and retried forever.
                        let elapsed = stream_opened.elapsed();
                        if !prefill_rescued
                            && matches!(phase, crate::inference::stream_liveness::StreamPhase::Prefilling { .. })
                            && elapsed > live_budget
                        {
                            prefill_rescued = true;
                            crate::probe!(
                                class = "inference.prefill.rescued",
                                provider = self.config.name.as_str(),
                                elapsed_s = elapsed.as_secs(),
                                queued_s = first_frame.duration_since(stream_opened).as_secs(),
                                old_budget_s = live_budget.as_secs(),
                                processed = p.processed,
                                total = p.total,
                                cached = p.cache,
                                "prefill outlived the OLD decode watchdog and is STILL \
                                 advancing — under the previous flat budget this turn \
                                 would have been killed here and retried forever",
                            );
                        }
                        // Per-stream ingest receipt, emitted once at the prefill→decode
                        // edge, with QUEUE and INGEST separated: llama.cpp's 0% frame
                        // marks slot ASSIGNMENT, so open→first-frame is time spent
                        // waiting for a slot and first-frame→now is real ingest work.
                        // `ingest_tok_per_s` is the number the 90s constant was
                        // implicitly guessing at and the one a derived budget should
                        // come from per model+device (#441); `queued_ms` is the
                        // admission/oversubscription signal (#234 QoS).
                        if matches!(was, crate::inference::stream_liveness::StreamPhase::Prefilling { .. })
                            && matches!(phase, crate::inference::stream_liveness::StreamPhase::Decoding)
                        {
                            let queued_ms = first_frame.duration_since(stream_opened).as_millis() as u64;
                            let ingest_ms = (first_frame.elapsed().as_millis().max(1)) as u64;
                            let fresh = p.total.saturating_sub(p.cache);
                            crate::probe!(
                                class = "inference.prefill.complete",
                                provider = self.config.name.as_str(),
                                persona = probe_persona.as_str(),
                                purpose = probe_purpose.as_str(),
                                total = p.total,
                                cached = p.cache,
                                fresh = fresh,
                                queued_ms = queued_ms,
                                ingest_ms = ingest_ms,
                                ingest_tok_per_s = (fresh as f64 * 1000.0 / ingest_ms as f64) as u64,
                                would_have_died = u8::from(elapsed > live_budget) as u64,
                                "prefill complete — queue wait vs real ingest, and the \
                                 cache's actual contribution, per stream",
                            );
                        }
                        if p.processed > last_prefill_processed {
                            last_prefill_processed = p.processed;
                            last_progress = Instant::now();
                            // L9: prefill advance is LANE liveness, not just request
                            // liveness — the health heartbeat and the never-started
                            // classifier both read this stamp via ms_since_real_work.
                            if local_lane {
                                crate::inference::llama_server::note_real_prefill_progress();
                            }
                            let _ = sink.send(GenerationChunk::Prefill {
                                processed: p.processed,
                                total: p.total,
                                cached: p.cache,
                            });
                        }
                    }
                    // Any REAL output (token, reasoning, tool delta, finish) means
                    // prefill is definitionally over. Detected once here, by observing
                    // whether the sites below moved the liveness stamp, rather than
                    // repeating a phase assignment at each of the four — one decision,
                    // one place, and a new output kind cannot forget to declare itself.
                    let progress_before_output = last_progress;
                    if let Some(choice) = parsed.choices.into_iter().next() {
                        if let Some(fr) = choice.finish_reason {
                            finish_reason_str = Some(fr);
                            last_progress = Instant::now();
                        }
                        if let Some(delta) = choice.delta {
                            if let Some(c) = delta.content {
                                if !c.is_empty() {
                                    acc_content.push_str(&c);
                                    let _ = sink.send(GenerationChunk::Token(c));
                                    last_progress = Instant::now();
                                }
                            }
                            if let Some(r) = delta.reasoning_content {
                                if !r.is_empty() {
                                    acc_reasoning.push_str(&r);
                                    let _ = sink.send(GenerationChunk::Reasoning(r));
                                    last_progress = Instant::now();
                                }
                            }
                            if let Some(tcs) = delta.tool_calls {
                                for tc in tcs {
                                    accumulate_stream_tool_call(&mut acc_tools, tc);
                                    last_progress = Instant::now();
                                }
                            }
                        }
                    }
                    if last_progress != progress_before_output {
                        phase = phase.on_output();
                    }
                }
            }
        }

        let response_time_ms = start.elapsed().as_millis() as u64;

        // Separate reasoning from the answer AT THE BOUNDARY: a reasoning model's
        // `<think>…</think>` (or a server `reasoning_content`) is captured for the
        // #363: a stream that reached EOF with real output is proof of life for the
        // local lane — this is the streaming sibling of the blocking path's
        // `note_real_decode` (which streaming never stamped, leaving the citizens'
        // primary path invisible to the liveness record). It also ends any failure
        // streak. Gated on the local lane like the failure stamps above.
        if local_lane
            && (!acc_content.is_empty() || !acc_reasoning.is_empty() || !acc_tools.is_empty())
        {
            crate::inference::llama_server::note_real_decode();
        }

        // harness/memory and stripped from `text` so it can NEVER reach the room.
        let raw_content = acc_content;
        let (text, reasoning) = extract_reasoning(
            &raw_content,
            (!acc_reasoning.is_empty()).then_some(acc_reasoning.as_str()),
        );
        let mut finish_reason = finish_reason_str
            .as_deref()
            .map(|r| self.map_finish_reason(r))
            .unwrap_or(FinishReason::Stop);

        // Assemble native tool calls from the streamed fragments.
        let mut tool_calls: Option<Vec<ToolCall>> = if acc_tools.is_empty() {
            None
        } else {
            let calls: Vec<ToolCall> = acc_tools
                .into_iter()
                .filter(|t| !t.name.is_empty())
                .map(|t| {
                    // A model's tool arguments that do not parse as JSON are a REAL
                    // failure of the generation, and this seam is the only place that
                    // knows it. The old behavior wrapped the unparseable text as
                    // `{"_raw": …}` and handed it downstream as if it were a valid
                    // params object — a fallback, and a lossy one: NOTHING in the tree
                    // reads `_raw` (verified 2026-08-06, zero consumers), so the true
                    // cause was destroyed here and the failure resurfaced later as a
                    // misleading typed-deser error ("missing field file_path") against
                    // params the model never successfully emitted.
                    //
                    // Glass-boxed from Asha's capture: Devstral emitted `write_file`
                    // whose `file_path` ran away into a repeating token block, breaking
                    // the JSON. The turn showed a confusing downstream error instead of
                    // "your tool arguments were not valid JSON."
                    // [[fallbacks-are-illegal-fail-loud]] (#334)
                    let input: Value = match serde_json::from_str(&t.arguments) {
                        Ok(v) => v,
                        Err(e) => {
                            crate::probe!(
                                class = "ai.tool_call.unparseable_args",
                                tool = t.name.as_str(),
                                error = e.to_string().as_str(),
                                arg_len = t.arguments.len(),
                                // The head is what a human/persona needs to SEE the
                                // shape of the corruption; the whole blob can be a
                                // runaway token block and must never flood the probe.
                                head = t.arguments.chars().take(200).collect::<String>().as_str(),
                                "model emitted tool arguments that are not valid JSON — the call cannot be honored as written",
                            );
                            // Carry the parse error itself, under a SELF-DESCRIBING key,
                            // so whatever rejects this call downstream can say what
                            // actually went wrong instead of inventing a missing-field
                            // story about params that were never parsed.
                            json!({
                                "__malformed_tool_arguments": {
                                    "error": e.to_string(),
                                    "raw": t.arguments,
                                }
                            })
                        }
                    };
                    ToolCall {
                        id: t.id,
                        name: t.name,
                        input,
                    }
                })
                .collect();
            (!calls.is_empty()).then_some(calls)
        };

        // UNIVERSAL text-format tool-call fallback. When no NATIVE tool_calls came
        // back, scan the model's TEXT for `{"tool_call": {...}}` envelopes and lift
        // them into the canonical ToolUse shape — so the agent loop executes them
        // EXACTLY like native calls. Run REGARDLESS of declared protocol: the base
        // model picks the surface format (and a "native" gateway sometimes still
        // emits the call as content), so the adapter stays flexible and never lets a
        // persona's hands go dead over a formatting mismatch. Robust to malformed
        // siblings + multiple calls + ``` fences (see json_in_prompt_tools). A LoRA
        // can tighten the model to native later; this is the floor that always works.
        if tool_calls.as_ref().map_or(true, |t| t.is_empty()) {
            let parsed = super::json_in_prompt_tools::parse_tool_calls(&text);
            if !parsed.is_empty() {
                finish_reason = FinishReason::ToolUse;
                tool_calls = Some(parsed);
            }
        }

        // Build content blocks
        let mut content_blocks = Vec::new();
        if !text.is_empty() {
            content_blocks.push(ContentPart::Text { text: text.clone() });
        }
        if let Some(ref tcs) = tool_calls {
            for tc in tcs {
                content_blocks.push(ContentPart::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.input.clone(),
                });
            }
        }

        let usage = stream_usage
            .map(|u| UsageMetrics {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                total_tokens: u
                    .total_tokens
                    .unwrap_or(u.prompt_tokens + u.completion_tokens),
                estimated_cost: None, // TODO: Calculate from model pricing
            })
            .unwrap_or_default();

        // Per-call PREFILL-vs-DECODE split for the speed harness. cache_n vs
        // prompt_n is the KV-cache hit/miss that dominates Metal wall-clock.
        let timing = stream_timings.map(|t| GenerationTiming {
            cached_tokens: t.cache_n,
            prefill_tokens: t.prompt_n,
            prefill_ms: t.prompt_ms,
            prefill_tokens_per_second: t.prompt_per_second,
            decode_tokens: t.predicted_n,
            decode_ms: t.predicted_ms,
            decode_tokens_per_second: t.predicted_per_second,
        });
        // THROUGHPUT FLOOR (#441): every call already carries the lane's own decode
        // rate; the catalog row states what this model is EXPECTED to serve at. A
        // collapse far below expectation is the eternity-class failure nobody was
        // catching (CPU-fallback lane, thrashing pager, contended GPU) — warn on
        // every breaching call rather than wait for a human to notice slowness.
        if let Some(t) = &timing {
            warn_if_decode_collapsed(model, t.decode_tokens, t.decode_tokens_per_second);
        }

        Ok(TextGenerationResponse {
            text,
            finish_reason,
            model: resp_model.unwrap_or_else(|| model.to_string()),
            provider: self.config.provider_id.to_string(),
            usage,
            response_time_ms,
            request_id,
            content: if content_blocks.is_empty() {
                None
            } else {
                Some(content_blocks)
            },
            tool_calls,
            reasoning,
            routing: None,
            error: None,
            timing,
        })
    }

    /// Create embeddings over the OpenAI-compatible `/v1/embeddings` endpoint.
    /// This is the path continuum's neural recall ([`NeuralEmbeddingProvider`])
    /// takes through the local llama-server /v1 gateway — it replaces the in-process
    /// fastembed/ONNX embedder. Degrades to an `Err` (never panics) when the
    /// endpoint is unreachable or the model isn't an embedding model; the caller
    /// falls back to the lexical embedder.
    ///
    /// [`NeuralEmbeddingProvider`]: crate::cognition::embedding::NeuralEmbeddingProvider
    async fn create_embedding(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        let start = Instant::now();

        // The model is the embedding SPACE identity — no silent default among
        // chat models ([[no-fallbacks-ever]]). NeuralEmbeddingProvider always
        // pins the canonical embedding slug; a None here is a config error.
        let model = request.model.clone().ok_or_else(|| {
            format!(
                "{} embeddings require an explicit model (the embedding-space identity)",
                self.config.name
            )
        })?;

        let body = build_embedding_body(&request.input, &model);

        let url = self.endpoints().embeddings();

        let mut request_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {api_key}"));
            }
        }

        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("{} embeddings POST failed: {e}", self.config.name))?;

        let status = response.status();
        if !status.is_success() {
            let err_body = response.text().await.unwrap_or_default();
            return Err(format!(
                "{} /v1/embeddings {status}: {err_body}",
                self.config.name
            ));
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| format!("{} embeddings response parse failed: {e}", self.config.name))?;

        let embeddings = parse_embedding_response(&json)?;
        let usage = parse_embedding_usage(&json);

        Ok(EmbeddingResponse {
            embeddings,
            model,
            provider: self.config.provider_id.clone(),
            usage,
            response_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        // Only require API key if provider needs auth
        if self.config.requires_auth && self.api_key.is_none() {
            return HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms: 0,
                error_rate: 1.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("{} API key not configured", self.config.name)),
            };
        }

        let start = Instant::now();

        // Try to list models as health check
        let url = self.endpoints().models();

        let mut request_builder = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5));

        // Only add Authorization header if provider requires auth
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        let result = request_builder.send().await;

        let response_time_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(resp) if resp.status().is_success() => HealthStatus {
                status: HealthState::Healthy,
                api_available: true,
                response_time_ms,
                error_rate: 0.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("{} API is accessible", self.config.name)),
            },
            Ok(resp) => HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms,
                error_rate: 1.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("{} returned {}", self.config.name, resp.status())),
            },
            Err(e) => HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms,
                error_rate: 1.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("{} error: {}", self.config.name, e)),
            },
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        self.config.models.clone()
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // Intentionally empty: prefixes live in the registry's
        // `Provider.model_prefixes` and are consulted directly by
        // `supports_model` below. The trait's Vec<&'static str> return
        // can't carry the registry's dynamic Vec<String> without leaking,
        // so we bypass it rather than faking a static slice.
        Vec::new()
    }

    /// Dynamic catalog for DMR, registry-declared prefix match for
    /// everyone else.
    ///
    /// The default trait impl uses `starts_with` against
    /// `supported_model_prefixes`. We override because prefixes now live
    /// in the Rust catalog (catalog.rs) (Provider.model_prefixes), not as
    /// `&'static str` embedded in code. A dynamic-catalog gateway (DMR) is
    /// special-cased because its catalog depends on `docker model pull`
    /// history — so we check the live runtime_models set populated at init.
    ///
    /// Returning false when the live set is empty/missing is the right
    /// behavior: AdapterRegistry::select hard-errors when no adapter
    /// supports a model, which surfaces the real problem ("user never
    /// pulled X") instead of silently routing to some other provider.
    /// Gated on the TYPED `dynamic_model_catalog` capability (#55), not
    /// the provider id.
    fn supports_model(&self, model_name: &str) -> bool {
        if self.config.dynamic_model_catalog {
            return self.runtime_models_contain(model_name);
        }
        let lower = model_name.to_lowercase();
        // Exact id match against the registry's declared models.
        if self
            .config
            .models
            .iter()
            .any(|m| m.id.to_lowercase() == lower)
        {
            return true;
        }
        // Family prefix match for "id we haven't listed yet but this
        // provider clearly owns" (e.g. gpt-5-preview → openai).
        self.config
            .model_prefixes
            .iter()
            .any(|prefix| lower.starts_with(&prefix.to_lowercase()))
    }
}

// ─── /v1/embeddings helpers (pure — TDD'd apart from the HTTP I/O) ──────────────

/// Build the OpenAI-compatible `/v1/embeddings` request body. A single input is
/// sent as a string and a batch as an array — both shapes the spec accepts —
/// and `model` is the embedding-space identity (already resolved by the caller).
fn build_embedding_body(input: &EmbeddingInput, model: &str) -> Value {
    let input = match input {
        EmbeddingInput::Single(s) => json!(s),
        EmbeddingInput::Multiple(v) => json!(v),
    };
    json!({ "input": input, "model": model })
}

/// Parse an OpenAI-compatible `/v1/embeddings` response into vectors ordered by
/// the response's `index` field. The spec does NOT guarantee `data` comes back
/// in input order, so we sort by `index` — getting this wrong silently
/// misaligns every vector with its source text (a corruption, not a crash).
/// Errors (rather than fabricating a vector) when `data` is missing or an entry
/// has no `embedding`, so a misconfigured endpoint degrades to the lexical
/// fallback instead of poisoning recall with junk.
fn parse_embedding_response(body: &Value) -> Result<Vec<Vec<f32>>, String> {
    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| format!("embeddings response missing `data` array: {body}"))?;

    let mut indexed: Vec<(usize, Vec<f32>)> = Vec::with_capacity(data.len());
    for (i, item) in data.iter().enumerate() {
        let idx = item
            .get("index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(i);
        let emb = item
            .get("embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("embeddings response item {i} missing `embedding`"))?;
        let vec: Vec<f32> = emb
            .iter()
            .map(|n| n.as_f64().unwrap_or(0.0) as f32)
            .collect();
        indexed.push((idx, vec));
    }
    indexed.sort_by_key(|(i, _)| *i);
    Ok(indexed.into_iter().map(|(_, v)| v).collect())
}

/// Extract token usage from an embeddings response, defaulting missing fields to
/// 0 — usage is observability, never load-bearing, so a provider that omits it
/// must not fail the embed.
fn parse_embedding_usage(body: &Value) -> UsageMetrics {
    let usage = body.get("usage");
    let prompt = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let total = usage
        .and_then(|u| u.get("total_tokens"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(prompt);
    UsageMetrics {
        input_tokens: prompt,
        output_tokens: 0,
        total_tokens: total,
        estimated_cost: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ai::types::ImageInput;

    mod prefill_progress_is_liveness {
        use super::*;

        /// what this catches: the SSE frame llama-server emits during PREFILL
        /// silently failing to decode, which would restore the 2026-08-13 defect —
        /// the watchdog seeing no progress and killing a slot that was healthily
        /// ingesting a long prompt. If `prompt_progress` stops parsing, a citizen
        /// with a big prompt can never produce a token.
        #[test]
        fn prefill_frame_decodes_with_no_choices_and_no_tokens() {
            let frame = r#"{"choices":[],"created":1,"model":"m","object":"x",
                "prompt_progress":{"total":16800,"cache":0,"processed":12288,"time_ms":84380}}"#;
            let parsed: OpenAIStreamChunk =
                serde_json::from_str(frame).expect("a prefill frame must decode");
            let p = parsed
                .prompt_progress
                .expect("prompt_progress must survive deserialization");
            assert_eq!(p.processed, 12288);
            assert_eq!(p.total, 16800);
            assert!(
                parsed.choices.is_empty(),
                "a prefill frame carries NO choices — this is exactly why the \
                 token-only watchdog could not see it"
            );
        }

        /// what this catches: a REPEATED progress frame being treated as fresh
        /// progress. The wedge signature (#385) is a slot that keeps emitting while
        /// its counter is frozen; if a non-advancing frame reset the watchdog, the
        /// detector would never fire again and we would be back to the 5-hour hang.
        #[test]
        fn a_frozen_counter_is_not_progress() {
            let mut last: u64 = 12288;
            let repeated: u64 = 12288;
            assert!(
                !(repeated > last),
                "a frame carrying the SAME processed count must NOT count as progress"
            );
            let advanced: u64 = 14336;
            assert!(advanced > last, "a rising count is progress");
            last = advanced;
            assert_eq!(last, 14336);
        }

        /// what this catches: the two silences collapsing back into one budget.
        /// Queue wait (measured 115s on a 1-slot lane for a 2,237-token prompt) is
        /// contention, not death; only silence AFTER the slot starts working is a
        /// wedge. If these budgets are ever made equal, healthy turns die under
        /// normal multi-citizen load.
        #[test]
        fn queue_budget_outlives_the_liveness_budget() {
            assert!(
                PRE_STREAM_HEADER_TIMEOUT_SECS > STREAM_IDLE_TIMEOUT_SECS,
                "the pre-start (queue) budget must be strictly larger than the \
                 post-start liveness budget: {PRE_STREAM_HEADER_TIMEOUT_SECS} vs \
                 {STREAM_IDLE_TIMEOUT_SECS}"
            );
        }
    }

    mod unguaranteed_model_refusal_says_which_situation {
        use super::*;
        use crate::inference::llama_server::ServingSnapshot;

        fn serving(active: &str) -> ServingSnapshot {
            ServingSnapshot {
                active_model: Some(active.to_string()),
                ready: true,
                ..ServingSnapshot::empty()
            }
        }

        // what this catches: the WAIT-vs-REFUSE split collapsing. This is the decision that
        // separates a turn we can still save from one we cannot, and the two branches read
        // almost identically at the call site — so the predicate itself is pinned here.
        // Live 2026-08-07: the lane flipped not-ready at +374s and republished ready at
        // +436s; three citizens' turns landed inside that 62s window and were refused 9
        // seconds before the lane returned. Every case below except the last is waitable.
        #[test]
        fn only_a_resident_ready_other_model_is_worth_refusing_immediately() {
            // boot / teardown — nothing resident: wait, the daemon is mid-flight
            assert!(!settled_on_another_model(&ServingSnapshot::empty()));
            // a lane is coming up (model named, decode not yet verified): wait
            let warming = ServingSnapshot {
                active_model: Some("m".into()),
                ready: false,
                ..ServingSnapshot::empty()
            };
            assert!(!settled_on_another_model(&warming));
            // the daemon has SETTLED on something else — waiting cannot change that
            assert!(settled_on_another_model(&serving("other")));
        }

        // what this catches: `snapshot_guarantees` drifting from the guard it replaced. The
        // gateway answers every request as its ONE resident model whatever the request says,
        // so a false positive here is a silently wrong brain — the failure this whole guard
        // exists to prevent. The vision arm is a real guarantee (the daemon verifies the
        // sidecar's /props before publishing `vision_ready`), so it must keep passing.
        #[test]
        fn a_guarantee_needs_ready_plus_this_exact_model_on_either_lane() {
            assert!(snapshot_guarantees(&serving("m"), "m"));
            assert!(!snapshot_guarantees(&serving("other"), "m"));
            assert!(!snapshot_guarantees(&ServingSnapshot::empty(), "m"));
            // named but not decode-ready is NOT a guarantee
            let warming = ServingSnapshot {
                active_model: Some("m".into()),
                ready: false,
                ..ServingSnapshot::empty()
            };
            assert!(!snapshot_guarantees(&warming, "m"));
            // the #106 vision sidecar is its own verified residency
            let with_vision = ServingSnapshot {
                vision_ready: true,
                vision_model: Some("vl".into()),
                ..serving("m")
            };
            assert!(snapshot_guarantees(&with_vision, "vl"));
            // ... but only when the daemon actually verified it
            let unverified = ServingSnapshot {
                vision_ready: false,
                ..with_vision
            };
            assert!(!snapshot_guarantees(&unverified, "vl"));
        }

        // what this catches: the startup case regressing to the fault sentence. Before the
        // daemon's first reconcile every reader borrows the boot placeholder, so a refusal
        // here says nothing about the lane's health — 116 false alarms over 3 days came
        // from printing the fault wording during ordinary core startup (#350).
        #[test]
        fn before_the_first_reconcile_it_names_startup_and_invites_a_retry() {
            let msg = unguaranteed_model_refusal("gw", "m", &ServingSnapshot::empty(), false);
            assert!(msg.contains("STARTUP"), "{msg}");
            assert!(msg.contains("retry"), "{msg}");
            assert!(
                !msg.contains("is not the active served model"),
                "startup must not borrow the mismatch wording: {msg}"
            );
        }

        // what this catches: THE REGRESSION THIS TEST EXISTS FOR. An empty snapshot AFTER
        // the first reconcile is still usually a transition, not a fault: the daemon
        // republishes `empty()` on every teardown (no plan, re-home, #175 wedge self-heal).
        // Live 2026-08-07 three citizens took the fault sentence 59s into a wedge-triggered
        // relaunch that completed normally — the latch said "reconciled", so the earlier
        // two-way split routed a healthy transition into the fault branch.
        #[test]
        fn a_reconciled_but_empty_snapshot_reads_as_a_transition_not_a_fault() {
            let msg = unguaranteed_model_refusal("gw", "m", &ServingSnapshot::empty(), true);
            assert!(msg.contains("TRANSITION"), "{msg}");
            assert!(msg.contains("retry"), "{msg}");
            assert!(
                !msg.contains("is not the active served model"),
                "a lane between relaunches is not a model mismatch: {msg}"
            );
        }

        // what this catches: the genuine mismatch losing its loudness. A DIFFERENT model
        // being resident is the one case retrying cannot fix, so it must keep naming both
        // models — softening this back into "just retry" would hide a real misroute.
        #[test]
        fn a_different_resident_model_stays_a_named_mismatch() {
            let msg = unguaranteed_model_refusal("gw", "wanted", &serving("resident"), true);
            assert!(msg.contains("is not the active served model"), "{msg}");
            assert!(msg.contains("wanted") && msg.contains("resident"), "{msg}");
            assert!(
                !msg.contains("TRANSITION") && !msg.contains("STARTUP"),
                "a real mismatch must not be excused as a transition: {msg}"
            );
        }
    }

    // what this catches: the `{"_raw": …}` fallback returning. Unparseable tool
    // arguments used to be wrapped as a params object with a key NOTHING in the tree
    // reads, which destroyed the real cause here and made the failure resurface
    // downstream as a misleading "missing field X" about params that were never
    // parsed. Glass-boxed from Asha's live capture 2026-08-06: Devstral emitted
    // `write_file` whose file_path ran away into a repeating token block, breaking
    // the JSON. The marker must NAME the failure and carry the parser's own error.
    // [[fallbacks-are-illegal-fail-loud]] (#334)
    #[test]
    fn unparseable_tool_arguments_are_marked_malformed_never_silently_wrapped() {
        let runaway = format!(
            "{{\"content\":\"x\",\"file_path\":\"/a{}",
            "e072".repeat(50)
        );
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&runaway);
        let err = parsed.expect_err("fixture must actually be invalid JSON");

        let input = json!({
            "__malformed_tool_arguments": { "error": err.to_string(), "raw": runaway.clone() }
        });

        // The marker is self-describing: a reader downstream can tell that the
        // ARGUMENTS never parsed, rather than guessing at a missing field.
        let m = input
            .get("__malformed_tool_arguments")
            .expect("malformed marker must be present and named for what happened");
        assert!(
            m.get("error")
                .and_then(|e| e.as_str())
                .is_some_and(|e| !e.is_empty()),
            "the parser's own error must survive — it is the only account of WHY"
        );
        assert_eq!(
            m.get("raw").and_then(|r| r.as_str()),
            Some(runaway.as_str()),
            "the raw text is kept for diagnosis, but under a key that says it is broken"
        );
        // The dead escape hatch must not come back: `_raw` had zero consumers, so a
        // params object carrying it reads as valid to every caller and is not.
        assert!(
            input.get("_raw").is_none(),
            "the silent `_raw` wrapper must stay gone"
        );
    }

    /// Minimal adapter for pure payload-assembly tests — no network, no registry.
    fn test_adapter() -> OpenAICompatibleAdapter {
        OpenAICompatibleAdapter::new(OpenAICompatibleConfig {
            provider_id: "test-gateway".into(),
            name: "Test Gateway".into(),
            base_url: "http://127.0.0.1:0".into(),
            api_key_env: None,
            default_model: "test-model".into(),
            capabilities: std::collections::BTreeSet::new(),
            models: Vec::new(),
            model_prefixes: Vec::new(),
            requires_auth: false,
            tool_protocol: crate::model_registry::ToolProtocol::NativeFunctionCalling,
            thinking: ThinkingMode::Default,
            single_resident_model: false,
            dynamic_model_catalog: false,
            llamacpp_sampling_extensions: false,
        })
    }

    fn image_message() -> Vec<ChatMessage> {
        vec![ChatMessage {
            role: "user".into(),
            content: MessageContent::Parts(vec![
                ContentPart::Text {
                    text: "what do you see?".into(),
                },
                ContentPart::Image {
                    image: ImageInput {
                        url: None,
                        base64: Some("QUJD".into()),
                        mime_type: Some("image/png".into()),
                    },
                },
            ]),
            name: None,
        }]
    }

    // what this catches (#106 native-vision branch): a VISION-capable target model must
    // receive the RAW image as a proper OpenAI multimodal content part — `image_url`
    // with a base64 data-URI — in the /v1 chat payload (this is exactly what the
    // multimodal llama-server lane's mtmd tokenizer consumes). A regression that drops
    // or re-texts the image blinds every natively-sighted model while all the plumbing
    // upstream still reports success.
    #[test]
    fn vision_capable_model_gets_raw_image_content_parts() {
        let wire = test_adapter().format_messages(&image_message(), None, true);
        assert_eq!(wire.len(), 1);
        let content = wire[0]["content"].as_array().expect("multimodal array");
        assert_eq!(content.len(), 2, "text part + image part");
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(
            content[1]["image_url"]["url"], "data:image/png;base64,QUJD",
            "raw pixels ride as a base64 data-URI — native sight, not a description"
        );
    }

    // what this catches (#106 bridge branch): a NON-vision target model must NOT be
    // sent `image_url` parts — its sight is the VisionDescriptionService bridge text
    // (unchanged behavior); shipping pixels at a text-only endpoint is an API error or
    // a silent drop the persona would mistake for having seen. The text parts (which
    // carry the bridge's description) must survive untouched.
    #[test]
    fn non_vision_model_has_image_parts_dropped_and_keeps_bridge_text() {
        let wire = test_adapter().format_messages(&image_message(), None, false);
        assert_eq!(wire.len(), 1);
        let content = wire[0]["content"].as_array().expect("content array");
        assert_eq!(
            content.len(),
            1,
            "image part dropped for a model that cannot see; bridge text remains"
        );
        assert_eq!(content[0]["type"], "text");
        assert!(
            !serde_json::to_string(&wire).unwrap().contains("image_url"),
            "no image_url may reach a non-vision model's payload"
        );
    }

    // what this catches (#181): the anti-loop knobs reach the llama.cpp wire body.
    // The reasoning-channel repetition loop (Devstral-24B looped an identical wrong
    // code block to the length cap, empty answer) is only stopped if `repeat_last_n`
    // (widened window) AND `frequency_penalty` (unwindowed guard) actually make it
    // onto the POST body — a silent drop here was the exact shape of the earlier
    // `stop`-sequence and `repeat_penalty` regressions (RULE 1: the field the
    // faculty threaded in never reached the server). `repeat_penalty` is always
    // present (defaulting when omitted); the anti-loop pair only when the request
    // carries them, so the sampling layer stays the single owner of the values.
    #[test]
    fn sampling_knobs_carry_the_antiloop_pair_onto_the_wire_body() {
        let mut obj = serde_json::Map::new();
        let req = TextGenerationRequest {
            repeat_penalty: Some(1.1),
            repeat_last_n: Some(320),
            frequency_penalty: Some(0.3),
            ..Default::default()
        };
        apply_llamacpp_sampling_knobs(&mut obj, &req);
        // f32→JSON→f64 widening is not bit-exact (1.1f32 ≈ 1.10000002), so compare
        // the penalties with tolerance; repeat_last_n is an integer and must be exact.
        let approx = |k: &str| obj.get(k).and_then(|v| v.as_f64()).unwrap();
        assert!((approx("repeat_penalty") - 1.1).abs() < 1e-4);
        assert_eq!(
            obj.get("repeat_last_n").and_then(|v| v.as_u64()),
            Some(320),
            "the widened window must reach llama-server or the loop slips through the 64-token default"
        );
        assert!(
            (approx("frequency_penalty") - 0.3).abs() < 1e-4,
            "the unwindowed guard must reach the wire — it catches gap-separated loops repeat_last_n misses"
        );
    }

    // what this catches: the adapter NEVER invents the anti-loop values. When the
    // request omits them (an external/cloud caller that didn't set sampling), the
    // fields are absent from the body so the gateway keeps its own default — the
    // sampling layer, not this adapter, is the single source of the knob values (#76).
    // repeat_penalty still defaults, matching the pre-existing DMR runaway fix.
    #[test]
    fn sampling_knobs_omit_antiloop_when_request_does_not_carry_them() {
        let mut obj = serde_json::Map::new();
        apply_llamacpp_sampling_knobs(&mut obj, &TextGenerationRequest::default());
        assert!(
            (obj.get("repeat_penalty").and_then(|v| v.as_f64()).unwrap() - 1.1).abs() < 1e-4,
            "repeat_penalty always set — a local gateway at 1.0 runs away (pre-#181 DMR fix)"
        );
        assert!(
            !obj.contains_key("repeat_last_n"),
            "no request value → omit, do not hardcode a window in the adapter"
        );
        assert!(
            !obj.contains_key("frequency_penalty"),
            "no request value → omit, do not hardcode a penalty in the adapter"
        );
    }

    // what this catches (#175 universal backstop): the local-gateway adapter must
    // REFUSE any request whose prompt ALONE meets/exceeds the served per-slot window —
    // sending it 500s and POISONS the shared slot for every later request. Fires for
    // ANY caller (a dream distillation, an eval — none carry a persona_id), only on the
    // unambiguous prompt-alone overflow (a budgeted request that leaves reply headroom
    // is never blocked), and NEVER when the window is unknown (0) so a mid-relaunch
    // snapshot can't wrongly block cognition.
    #[test]
    fn refuses_only_when_prompt_alone_overflows_the_served_slot() {
        let body = |chars: usize| serde_json::json!({ "messages": [{ "role": "user", "content": "x".repeat(chars) }] });
        // ~12000 tokens (48000 chars / 4) vs a 8000-token slot → refuse, report the est.
        assert_eq!(
            prompt_alone_overflows_served(&body(48_000), 8_000),
            Some(12_000),
            "prompt alone over the window must be refused"
        );
        // ~4000 tokens vs an 8000 slot → fits (room for the prompt + a reply) → allow.
        assert_eq!(prompt_alone_overflows_served(&body(16_000), 8_000), None);
        // Window unknown (mid-relaunch) → never block, whatever the prompt size.
        assert_eq!(prompt_alone_overflows_served(&body(48_000), 0), None);
        // No messages array → nothing to overflow.
        assert_eq!(
            prompt_alone_overflows_served(&serde_json::json!({}), 8_000),
            None
        );
    }

    // what this catches: a well-formed <think>…</think> (unsloth/llama.cpp today) is
    // SEPARATED — reasoning captured, the answer after </think> is the clean text.
    // This is the leak Asha hit: without it the whole think block reached the room.
    #[test]
    fn extract_reasoning_splits_well_formed_think_block() {
        let raw = "<think>\nThe capital is Paris. Keep it to one sentence.\n</think>\n\nParis is the capital of France.";
        let (text, reasoning) = extract_reasoning(raw, None);
        assert_eq!(text, "Paris is the capital of France.");
        assert_eq!(
            reasoning.as_deref(),
            Some("The capital is Paris. Keep it to one sentence.")
        );
        assert!(
            !text.contains("<think>"),
            "answer must be free of reasoning tags"
        );
    }

    // what this catches: THE runaway loop — an UNCLOSED <think> (model ran out of
    // tokens mid-thought). There is NO answer, so text is empty (the caller refuses
    // to post) and the raw reasoning is captured, NOT leaked.
    #[test]
    fn extract_reasoning_unclosed_think_yields_empty_answer() {
        let raw = "<think>\nWait, the recall section... wait, no... wait, the recall section";
        let (text, reasoning) = extract_reasoning(raw, None);
        assert_eq!(
            text, "",
            "a truncated think block produces no postable answer"
        );
        assert!(reasoning.unwrap().contains("recall section"));
    }

    // what this catches: a server that already splits reasoning into
    // `reasoning_content` (vLLM-style) is trusted — content is the clean answer,
    // the field is the reasoning, no tag parsing.
    #[test]
    fn extract_reasoning_prefers_server_reasoning_content() {
        let (text, reasoning) = extract_reasoning("Paris.", Some("I recall France's capital."));
        assert_eq!(text, "Paris.");
        assert_eq!(reasoning.as_deref(), Some("I recall France's capital."));
    }

    // what this catches: a plain answer with no reasoning passes through untouched,
    // and an empty `<think></think>` (the JSON-path shape) yields no reasoning.
    #[test]
    fn extract_reasoning_plain_and_empty_think() {
        let (text, reasoning) = extract_reasoning("Just the answer.", None);
        assert_eq!(text, "Just the answer.");
        assert!(reasoning.is_none());

        let (text, reasoning) = extract_reasoning("<think></think>\n\n{\"ok\":true}", None);
        assert_eq!(text, "{\"ok\":true}");
        assert!(
            reasoning.is_none(),
            "empty think block confers no reasoning"
        );
    }

    // what this catches: a thread ending with an assistant turn must be closed with
    // the continuation user message (llama-server 400s trailing-assistant as prefill
    // under thinking — the 1000+ silently-dead self-ticks of 2026-07-10/11), while a
    // thread already ending with user/system stays untouched. Regression for
    // close_trailing_assistant.
    // what this catches: the hot-slot LEASE contract (2026-07-16 "alive" fix).
    // A persona reuses the slot it holds (WARM — the 0.48s-vs-40s win); distinct
    // personas take distinct free slots; once full, a NEW persona evicts the
    // LEAST-recently-active holder, not a fixed round-robin victim — so the active
    // set keeps its warm slots and co-active minds never share one. This is the
    // what this catches: the boot-race latch that killed slot affinity on effectively
    // EVERY boot (2026-08-21). Personas start deliberating while llama-server still
    // answers 503_loading (measured ×93 in the same ledger); the old code latched
    // Unsupported on ANY non-success status, so one probe racing the load window
    // disabled pinning for the process's life and prefix-similarity slot theft took
    // over (`cached: 0` mid-conversation). Only "no such endpoint" may be permanent.
    #[test]
    fn a_loading_lane_must_not_latch_slot_affinity_off() {
        use reqwest::StatusCode;
        for transient in [
            StatusCode::SERVICE_UNAVAILABLE, // llama-server mid-load — THE incident
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert!(
                !props_status_proves_endpoint_absent(transient),
                "{transient} is a statement about NOW, not about what the server IS — \
                 latching Unsupported on it re-opens the boot race"
            );
        }
        for absent in [StatusCode::NOT_FOUND, StatusCode::NOT_IMPLEMENTED] {
            assert!(
                props_status_proves_endpoint_absent(absent),
                "{absent} genuinely proves the surface is missing — without the latch \
                 every cloud provider would be re-probed per persona request forever"
            );
        }
    }

    #[test]
    fn trailing_assistant_thread_is_closed_with_continuation_fact() {
        let mut msgs = vec![
            json!({"role": "system", "content": "be helpful"}),
            json!({"role": "user", "content": "peer turn"}),
            json!({"role": "assistant", "content": "my own last post"}),
        ];
        close_trailing_assistant(&mut msgs);
        assert_eq!(msgs.len(), 4, "continuation appended");
        assert_eq!(msgs[3]["role"], json!("user"));
        assert!(
            msgs[3]["content"]
                .as_str()
                .unwrap()
                .contains("[continuation]"),
            "closure is the structural continuation fact"
        );

        // Already-legal threads are untouched (user-final and system-final).
        let mut user_final = vec![
            json!({"role": "assistant", "content": "earlier"}),
            json!({"role": "user", "content": "newest peer turn"}),
        ];
        close_trailing_assistant(&mut user_final);
        assert_eq!(user_final.len(), 2, "user-final thread unchanged");

        let mut system_final = vec![
            json!({"role": "assistant", "content": "earlier"}),
            json!({"role": "system", "content": "tool block"}),
        ];
        close_trailing_assistant(&mut system_final);
        assert_eq!(system_final.len(), 2, "system-final thread unchanged");
    }

    // what this catches: the thinking toggle's mechanism — `/no_think` is appended
    // to the LAST user message (Qwen3 soft-switch), not the system or an earlier
    // turn. Verified live: this makes the model emit an empty think block + a direct
    // answer (which extract_reasoning reduces to clean text).
    #[test]
    fn apply_no_think_switch_targets_last_user_message() {
        let mut msgs = vec![
            json!({"role": "system", "content": "be helpful"}),
            json!({"role": "user", "content": "earlier turn"}),
            json!({"role": "assistant", "content": "earlier reply"}),
            json!({"role": "user", "content": "what is 2+2?"}),
        ];
        apply_no_think_switch(&mut msgs);
        assert_eq!(msgs[3]["content"], json!("what is 2+2?\n/no_think"));
        // earlier user turn + system untouched
        assert_eq!(msgs[1]["content"], json!("earlier turn"));
        assert_eq!(msgs[0]["content"], json!("be helpful"));
    }

    // what this catches: no user message → no-op (never corrupts a system-only or
    // tool-only message array).
    #[test]
    fn apply_no_think_switch_noop_without_user() {
        let mut msgs = vec![json!({"role": "system", "content": "be helpful"})];
        apply_no_think_switch(&mut msgs);
        assert_eq!(
            msgs[0]["content"],
            json!("be helpful"),
            "no user turn → unchanged"
        );
    }

    // what this catches: the ROBUST thinking-suppression lever — under
    // ThinkingMode::Suppress the request body must carry
    // `chat_template_kwargs.enable_thinking=false` on EVERY turn, not only JSON
    // (response_format) turns. Regresses the 2026-06-27 misgating where the kwarg
    // lived solely in the response_format branch, so free-form act/speak turns let
    // the forged qwen3.5 template open `<think>` and emit empty content (all eval
    // answers blank, lift=0.0). Idempotent: a second apply leaves the same value.
    #[test]
    fn apply_enable_thinking_false_sets_kwarg_idempotently() {
        let mut body = json!({ "model": "m", "messages": [], "stream": true });
        apply_enable_thinking_false(&mut body);
        assert_eq!(
            body["chat_template_kwargs"],
            json!({ "enable_thinking": false }),
            "suppression must set the template hatch the forged template honors"
        );
        // second apply is a no-op on the value (overwrites identically)
        apply_enable_thinking_false(&mut body);
        assert_eq!(
            body["chat_template_kwargs"],
            json!({ "enable_thinking": false })
        );
    }

    // what this catches: a single string input serializes as a JSON string (not
    // a 1-element array) and the resolved model is carried through — the request
    // shape unsloth/OpenAI actually expects.
    #[test]
    fn build_body_single_input() {
        let body = build_embedding_body(&EmbeddingInput::Single("hello".into()), "qwen3-embed");
        assert_eq!(body["input"], json!("hello"));
        assert_eq!(body["model"], json!("qwen3-embed"));
    }

    // what this catches: a batch input serializes as a JSON array, so batched
    // embeds (the recall hot path) go out in one request.
    #[test]
    fn build_body_batch_input() {
        let body = build_embedding_body(
            &EmbeddingInput::Multiple(vec!["a".into(), "b".into()]),
            "qwen3-embed",
        );
        assert_eq!(body["input"], json!(["a", "b"]));
    }

    // what this catches: THE CORRUPTION GUARD — `data` returned out of order is
    // re-sorted by `index`, so vector[k] always corresponds to input[k]. A
    // regression here silently pairs every memory with the wrong vector.
    #[test]
    fn parse_response_orders_by_index() {
        let body = json!({
            "data": [
                { "index": 1, "embedding": [0.4, 0.5] },
                { "index": 0, "embedding": [0.1, 0.2] },
            ],
            "model": "qwen3-embed",
            "usage": { "prompt_tokens": 3, "total_tokens": 3 }
        });
        let vecs = parse_embedding_response(&body).unwrap();
        assert_eq!(vecs, vec![vec![0.1, 0.2], vec![0.4, 0.5]]);
    }

    // what this catches: a malformed response (no `data`) is an Err, NOT a
    // panic and NOT an empty success — so recall degrades to the lexical
    // fallback instead of treating "no signal" as a real (empty) embedding.
    #[test]
    fn parse_response_missing_data_errors() {
        let body = json!({ "object": "error", "message": "no model loaded" });
        assert!(parse_embedding_response(&body).is_err());
    }

    // what this catches: usage is optional — a provider that omits it yields
    // zeroed metrics, never an error (usage is observability, not load-bearing).
    #[test]
    fn parse_usage_defaults_to_zero_when_absent() {
        let usage = parse_embedding_usage(&json!({ "data": [] }));
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    // The LoRA page-in resolver (slice 1 of the Model Endpoint Fabric). These
    // exercise the PURE name→server-id matcher; the probe/HTTP + fail-loud
    // paths need a live llama-server and are covered by the organism eval, not
    // a unit test (the integration path, per RULE 1).
    mod lora_page_in {
        use super::*;

        fn catalog() -> Vec<(i64, String)> {
            vec![
                (0, "/genome/coder-4b-keystone/adapter.gguf".to_string()),
                (1, "/genome/asha-selfverify/adapter.gguf".to_string()),
            ]
        }

        // what this catches: an exact path match resolves to the server's own
        // load-index — the field the `"lora":[{id,scale}]` body needs.
        #[test]
        fn exact_path_resolves_to_server_index() {
            let id = OpenAICompatibleAdapter::match_lora_index(
                &catalog(),
                "asha-selfverify",
                "/genome/asha-selfverify/adapter.gguf",
            );
            assert_eq!(id, Some(1));
        }

        // what this catches: the common "persona names the short slug, server
        // stores the full path" case still resolves via substring on name.
        #[test]
        fn short_name_resolves_via_substring() {
            let id = OpenAICompatibleAdapter::match_lora_index(&catalog(), "coder-4b-keystone", "");
            assert_eq!(id, Some(0));
        }

        // what this catches: an adapter the custodian has NOT registered is a
        // miss (None) — the caller turns this into a fail-loud, never a silent
        // drop. (Silent drop was the original LIFT=0 no-op.)
        #[test]
        fn unregistered_adapter_is_a_miss() {
            let id = OpenAICompatibleAdapter::match_lora_index(&catalog(), "does-not-exist", "");
            assert_eq!(id, None);
        }

        // what this catches: an empty catalog (server loaded no adapters at
        // launch) never spuriously matches.
        #[test]
        fn empty_catalog_never_matches() {
            assert_eq!(
                OpenAICompatibleAdapter::match_lora_index(&[], "anything", "/some/path"),
                None
            );
        }
    }
}
