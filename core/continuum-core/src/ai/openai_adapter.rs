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
    EmbeddingResponse, FinishReason, HealthState, HealthStatus, MessageContent, ModelInfo,
    TextGenerationRequest, TextGenerationResponse, ToolCall, ToolChoice, UsageMetrics,
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
    async fn probe_lora_catalog(&self) -> Result<(), String> {
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

    /// Resolve each requested adapter to the `{ "id": N, "scale": S }` entries
    /// the llama.cpp `lora` request-body field wants. Discovers capability on
    /// demand (probes if `Unknown`), then:
    /// - `Unsupported` → **fail loud** (never silently drop the page-in — that
    ///   was the original no-op behind the LIFT=0 measurement); the caller asked
    ///   to page a LoRA into a backend that can't, so serve a fused model or
    ///   route to a llama.cpp backend.
    /// - `Supported` but the adapter isn't loaded → **fail loud**: the CUSTODIAN
    ///   hasn't registered it with the serving backend yet. We re-probe once in
    ///   case it was loaded after our last probe.
    async fn resolve_lora_entries(
        &self,
        reqs: &[ActiveAdapterRequest],
    ) -> Result<Vec<Value>, String> {
        if matches!(&*self.lora_support.read().unwrap(), LoraSupport::Unknown) {
            self.probe_lora_catalog().await?;
        }

        let mut entries = Vec::with_capacity(reqs.len());
        for req in reqs {
            let id = self.lookup_lora_index(&req.name, &req.path);
            let id = match id {
                Some(id) => id,
                None => {
                    // Miss — re-probe once (it may have just been registered),
                    // then resolve or fail loud with what IS loaded.
                    self.probe_lora_catalog().await?;
                    self.lookup_lora_index(&req.name, &req.path)
                        .ok_or_else(|| self.lora_miss_error(&req.name, &req.path))?
                }
            };
            entries.push(json!({ "id": id, "scale": req.scale }));
        }
        Ok(entries)
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

    /// Convert ChatMessage to OpenAI format
    fn format_messages(&self, messages: &[ChatMessage], system_prompt: Option<&str>) -> Vec<Value> {
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
                                    if let Some(url) = &image.url {
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

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: Option<u32>,
}

/// How long the inference lane may stay SILENT mid-stream before we declare it
/// dead. This is a LIVENESS watchdog, not a deadline: a slow-but-producing decode
/// (a 4B model on CPU emitting a token every few hundred ms) stays alive
/// indefinitely as long as it keeps streaming. Only true silence — the backend
/// stuck, crashed, or the socket wedged — trips it, and then we fail loud naming
/// the cause ([[fallbacks-are-illegal-fail-loud]]). Replaces the old wall-clock
/// total-request timeout that killed legitimately-long generations.
const STREAM_IDLE_TIMEOUT_SECS: u64 = 90;

/// One streamed SSE frame from an OpenAI-compatible `/v1/chat/completions` with
/// `stream: true`. Each frame carries an incremental `delta`; `usage` arrives only
/// on the final frame (requires `stream_options.include_usage`).
#[derive(Debug, Deserialize)]
struct OpenAIStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAIStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
    #[serde(default)]
    model: String,
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

        // Build request body
        let mut messages = self.format_messages(&request.messages, request.system_prompt.as_deref());

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
            let rp = request.repeat_penalty.unwrap_or(1.1);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("repeat_penalty".to_string(), json!(rp));
            }
        }

        // LoRA page-in: forward the persona's active adapters to the serving
        // backend as the llama.cpp `lora` request-body extension — the SAME
        // backend-extension mechanism as `repeat_penalty` above. The integer
        // `id` is the server-side load-index (discovered via GET /lora-adapters),
        // which the CUSTODIAN assigns when it loads the adapter; we only RESOLVE
        // name→id here and reference it. Until this block, `active_adapters`
        // reached the adapter and was silently dropped at exactly this point —
        // the LoRA page-in no-op behind the LIFT=0 measurement. Capability is
        // discovered, not declared: `resolve_lora_entries` probes the endpoint
        // and FAILS LOUD if the backend can't page LoRA or the adapter isn't
        // loaded (never a silent drop).
        if let Some(adapters) = request.active_adapters.as_ref() {
            if !adapters.is_empty() {
                let entries = self.resolve_lora_entries(adapters).await?;
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("lora".to_string(), json!(entries));
                }
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

        // Make request - use runtime base URL if set, otherwise config base URL
        let url = self.endpoints().chat_completions();

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
            if !snap.ready || snap.active_model.as_deref() != Some(model) {
                return Err(format!(
                    "{}: model '{}' is not the active served model (serving: {}, ready: {}); \
                     the serving daemon owns which single model is resident — refusing to \
                     generate against an unguaranteed model",
                    self.config.name,
                    model,
                    snap.active_model.as_deref().unwrap_or("<none>"),
                    snap.ready
                ));
            }
        }

        let send_start = Instant::now();
        let response = request_builder.json(&body).send().await.map_err(|e| {
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
            format!(
                "{} POST failed after {}ms: {} (kind: timeout={}, connect={}, request={}, body={})",
                self.config.name,
                send_start.elapsed().as_millis(),
                chain.join(" -> "),
                e.is_timeout(),
                e.is_connect(),
                e.is_request(),
                e.is_body()
            )
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "{} returned {}: {}",
                self.config.name, status, body
            ));
        }

        // Consume the SSE stream: every token reaches `sink` the INSTANT it arrives.
        // Liveness is the per-token idle watchdog ([`STREAM_IDLE_TIMEOUT_SECS`]) —
        // silence means the backend died, NOT that generation is simply long.
        use futures::StreamExt;
        let mut byte_stream = response.bytes_stream();
        let idle = std::time::Duration::from_secs(STREAM_IDLE_TIMEOUT_SECS);

        let mut sse_buf: Vec<u8> = Vec::new();
        let mut acc_content = String::new();
        let mut acc_reasoning = String::new();
        let mut acc_tools: Vec<StreamToolAccum> = Vec::new();
        let mut finish_reason_str: Option<String> = None;
        let mut stream_usage: Option<OpenAIUsage> = None;
        let mut resp_model: Option<String> = None;

        loop {
            let next = tokio::time::timeout(idle, byte_stream.next())
                .await
                .map_err(|_| {
                    format!(
                        "{}: inference lane went silent for {}s mid-stream (no token \
                         produced) — backend stuck or dead; refusing to wait on a dead \
                         stream",
                        self.config.name, STREAM_IDLE_TIMEOUT_SECS
                    )
                })?;
            let Some(chunk) = next else {
                break; // server closed the stream (EOF) — generation complete
            };
            let bytes =
                chunk.map_err(|e| format!("{}: stream read error: {e}", self.config.name))?;

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
                    if let Some(choice) = parsed.choices.into_iter().next() {
                        if let Some(fr) = choice.finish_reason {
                            finish_reason_str = Some(fr);
                        }
                        if let Some(delta) = choice.delta {
                            if let Some(c) = delta.content {
                                if !c.is_empty() {
                                    acc_content.push_str(&c);
                                    let _ = sink.send(GenerationChunk::Token(c));
                                }
                            }
                            if let Some(r) = delta.reasoning_content {
                                if !r.is_empty() {
                                    acc_reasoning.push_str(&r);
                                    let _ = sink.send(GenerationChunk::Reasoning(r));
                                }
                            }
                            if let Some(tcs) = delta.tool_calls {
                                for tc in tcs {
                                    accumulate_stream_tool_call(&mut acc_tools, tc);
                                }
                            }
                        }
                    }
                }
            }
        }

        let response_time_ms = start.elapsed().as_millis() as u64;

        // Separate reasoning from the answer AT THE BOUNDARY: a reasoning model's
        // `<think>…</think>` (or a server `reasoning_content`) is captured for the
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
                    let input: Value = serde_json::from_str(&t.arguments)
                        .unwrap_or_else(|_| json!({ "_raw": t.arguments }));
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
        })
    }

    /// Create embeddings over the OpenAI-compatible `/v1/embeddings` endpoint.
    /// This is the path continuum's neural recall ([`NeuralEmbeddingProvider`])
    /// takes through the unsloth gateway — it replaces the in-process
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
        let vec: Vec<f32> = emb.iter().map(|n| n.as_f64().unwrap_or(0.0) as f32).collect();
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
        assert!(!text.contains("<think>"), "answer must be free of reasoning tags");
    }

    // what this catches: THE runaway loop — an UNCLOSED <think> (model ran out of
    // tokens mid-thought). There is NO answer, so text is empty (the caller refuses
    // to post) and the raw reasoning is captured, NOT leaked.
    #[test]
    fn extract_reasoning_unclosed_think_yields_empty_answer() {
        let raw = "<think>\nWait, the recall section... wait, no... wait, the recall section";
        let (text, reasoning) = extract_reasoning(raw, None);
        assert_eq!(text, "", "a truncated think block produces no postable answer");
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
        assert!(reasoning.is_none(), "empty think block confers no reasoning");
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
        assert_eq!(msgs[0]["content"], json!("be helpful"), "no user turn → unchanged");
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
        assert_eq!(body["chat_template_kwargs"], json!({ "enable_thinking": false }));
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
            let id =
                OpenAICompatibleAdapter::match_lora_index(&catalog(), "coder-4b-keystone", "");
            assert_eq!(id, Some(0));
        }

        // what this catches: an adapter the custodian has NOT registered is a
        // miss (None) — the caller turns this into a fail-loud, never a silent
        // drop. (Silent drop was the original LIFT=0 no-op.)
        #[test]
        fn unregistered_adapter_is_a_miss() {
            let id =
                OpenAICompatibleAdapter::match_lora_index(&catalog(), "does-not-exist", "");
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
