//! InferenceHandleModule — ServiceModule wrapper exposing the
//! handle store as `ai/inference/open` / `ai/inference/generate` /
//! `ai/inference/close` / `ai/inference/inspect` commands.
//!
//! Joel (2026-05-31): "Yeah the inference command doesn't do this.
//! It's smart subsystems and daemons. Commands are dumb and short."
//!
//! ### What this module is
//!
//! The dumb-command layer. Routes open/generate/close to
//! `InferenceHandleStore` with minimal logic — parse envelope,
//! validate, call store, materialize response. NO scheduling, NO
//! batching, NO LoRA paging policy, NO base-model-sharing decisions.
//! Those live in the smart subsystems + daemons that sit BEHIND
//! this command surface ([[inference-scarcity-economics]] task #109).
//!
//! ### Adapter resolution
//!
//! The `open` handler resolves the requested provider from the
//! module's local adapter map first (adapters explicitly registered
//! on this instance), then falls back to the shared `AdapterRegistry`
//! (the same registry that `inference/llm/request` uses — the
//! system-wide source of truth where production adapters live). The
//! handle store doesn't touch the registry — the module is the bridge
//! between "I want provider X" (the caller's view) and "I have an
//! Arc<dyn AIProviderAdapter>" (the store's contract).
//!
//! ### Doctrine alignment
//!
//! - [[commands-are-kernel-level-and-compose]] — this module ONLY
//!   handles command envelopes + routing; never reaches into
//!   adapter or store internals
//! - [[inference-is-an-adapter-always-in-the-loop]] — these are the
//!   canonical handle-shape commands callers + tests will route
//!   through; one-shot `inference/llm/request` remains the legacy
//!   path for migration
//! - [[inference-scarcity-economics]] §"commands are dumb, daemons
//!   are smart" — this module is the dumb interface; smart bits
//!   land later as separate components without changing this
//!   command surface
//! - [[rust-is-the-core-node-is-the-shell]] — pure Rust ServiceModule

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ActiveAdapterRequest, TextGenerationRequest, TextGenerationResponse};
use crate::identity::PeerId;
use crate::inference::coordinator::{CoordinatorError, InferenceCoordinator, OpenLaneRequest};
use crate::inference::handle_store::{
    InferenceHandleStore, OpenSessionRequest, HANDLE_OWNER, HANDLE_TYPE_TAG,
};
use crate::inference::lane::LaneClass;
use crate::inference::recipe_budget::TaskKind;
use crate::runtime::cell_shapes::HandleRef;
use crate::runtime::{CommandRequest, CommandResult, ModuleConfig, ModulePriority, ServiceModule};

// ── Command name constants ─────────────────────────────────────────

pub const COMMAND_OPEN: &str = "ai/inference/open";
pub const COMMAND_GENERATE: &str = "ai/inference/generate";
pub const COMMAND_CLOSE: &str = "ai/inference/close";
pub const COMMAND_INSPECT: &str = "ai/inference/inspect";

// ── SDK command-surface declarations ──────────────────────────────
//
// These four commands are ENVELOPED: the handler parses
// `CommandRequest::<P>::from_value` and returns
// `CommandResponse::ok(T).into_command_result()` (see `handle_command` below).
// The CommandSpec declarations (registered at the bottom of this file, after the
// types) carry `WireShape::Enveloped`, so the generator wraps them as
// `CommandRequest<P>` → `CommandResponse<T>` — faithful to that wire, including
// the flattened `handle` (`open` mints it; `generate`/`close`/`inspect` consume
// it). This is the envelope modeling the earlier deferral was waiting on.

// ── Typed params ───────────────────────────────────────────────────

/// Params for `ai/inference/open`.
///
/// The caller specifies the provider by name; the module resolves
/// via the AdapterRegistry. Sticky session inputs (system_prompt,
/// model override, active LoRA adapters, persona scope) all flow
/// through here and live on the session for the handle's lifetime.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/OpenParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct OpenParams {
    /// Provider ID from the AdapterRegistry (e.g. "anthropic",
    /// "heuristic", "llamacpp"). Required.
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_adapters: Option<Vec<ActiveAdapterRequest>>,
    /// Persona scope. When set, every subsequent generate against
    /// this handle MUST carry a matching persona_id. Defense in
    /// depth at the inference layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "string")]
    pub persona_id: Option<Uuid>,
    /// What the persona is doing — drives the lane's KV budget +
    /// class derivation (via [[INFERENCE-LANES-REALISTIC.md]]).
    /// Defaults to `Chat` when omitted. Ignored when the module
    /// runs without a coordinator (back-compat path).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub task: Option<TaskKind>,
    /// Override the class derived from `task`. Coordinator-mode
    /// only. Use when a daemon knows persona context (e.g. voice
    /// engaged) that implies a different class than `task`
    /// defaults to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub class_override: Option<LaneClass>,
}

/// Result of `ai/inference/open`. The minted handle is carried by
/// the CommandResponse envelope's top-level `handle` field; these
/// payload fields hold only the open-call's report.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/OpenResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    /// Echo of the resolved provider, so callers can confirm the
    /// adapter the module routed to (especially useful when the
    /// caller's open params lean on defaults).
    pub provider: String,
}

/// Params for `ai/inference/generate`.
///
/// The handle is carried by the CommandRequest envelope's top-level
/// `handle` field (per substrate convention) — these params hold
/// only the per-call generation request. The session's defaults
/// (system_prompt, model, active_adapters) fill in any unset fields
/// on `request` at generate time.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/GenerateParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenerateParams {
    pub request: TextGenerationRequest,
}

/// Result of `ai/inference/generate`.
pub type GenerateResult = TextGenerationResponse;

/// Params for `ai/inference/close`. Handle is in the envelope.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/CloseParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CloseParams {}

/// Result of `ai/inference/close`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/CloseResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CloseResult {
    /// True if the handle was open at close time. False = already
    /// closed or evicted; callers can treat this as idempotent.
    pub released: bool,
}

/// Params for `ai/inference/inspect`. Handle is in the envelope.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/InspectParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InspectParams {}

/// Result of `ai/inference/inspect`. The observability snapshot
/// per [[observability-is-half-the-architecture]].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai_inference/InspectResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InspectResult {
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub persona_id: Option<Uuid>,
    #[ts(type = "number")]
    pub created_at_ms: u64,
    #[ts(type = "number")]
    pub last_used_ms: u64,
    #[ts(type = "number")]
    pub generation_count: u64,
    pub has_system_prompt: bool,
    #[ts(type = "number")]
    pub active_adapter_count: u32,
    // ── Lane fields (populated when the module is coordinator-wired) ──
    /// The persona's task class for this lane. None = non-coordinator
    /// mode (handle store only).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub task: Option<TaskKind>,
    /// Lane class (Realtime / Interactive / Background / Sentinel).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub class: Option<LaneClass>,
    /// Seed KV tokens from the recipe budget table.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub seed_kv_tokens: Option<u32>,
    /// Max KV tokens the lane is allowed to grow to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_kv_tokens: Option<u32>,
    /// Bytes accounted in FootprintRegistry for this lane.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub bytes_accounted: Option<u64>,
    /// Lease expiration wall-clock — observers track approaching
    /// expiry to renew or close.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub lease_expires_at_ms: Option<u64>,
    /// True when the lease is `Pinned` (Realtime) and the pressure
    /// broker must not evict mid-turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_pinned: Option<bool>,
}

// ── CommandSpec registrations (sdk_codegen) ────────────────────────
//
// All four are Enveloped (verified against `handle_command`). `open` mints a
// handle; `generate`/`close`/`inspect` consume one — the handle rides the
// CommandRequest/CommandResponse envelope, which is exactly what the
// `Enveloped` wire shape models. GenerateResult is `TextGenerationResponse`.

/// `ai/inference/open` — open an inference session, returns a handle. Enveloped.
pub struct OpenCommand;
impl crate::sdk_codegen::CommandSpec for OpenCommand {
    const NAME: &'static str = COMMAND_OPEN;
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Low-level substrate: open a raw inference session handle. You do NOT need this \
         for normal work — your own replies already run through inference; use code/*, \
         chat/*, data/*, and work/* for tasks. (Handle-lifecycle plumbing.)";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Enveloped;
    type Params = OpenParams;
    type Result = OpenResult;
}
crate::register_command!(OpenCommand);

/// `ai/inference/generate` — generate against an open session (consumes handle).
pub struct GenerateCommand;
impl crate::sdk_codegen::CommandSpec for GenerateCommand {
    const NAME: &'static str = COMMAND_GENERATE;
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Low-level substrate: generate text against an open inference handle. NOT a task \
         tool — to answer, just answer. (Handle-lifecycle plumbing.)";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Enveloped;
    type Params = GenerateParams;
    type Result = GenerateResult;
}
crate::register_command!(GenerateCommand);

/// `ai/inference/close` — close an open session (consumes handle). Enveloped.
pub struct CloseCommand;
impl crate::sdk_codegen::CommandSpec for CloseCommand {
    const NAME: &'static str = COMMAND_CLOSE;
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Low-level substrate: close an open inference handle. Not needed for normal work. \
         (Handle-lifecycle plumbing.)";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Enveloped;
    type Params = CloseParams;
    type Result = CloseResult;
}
crate::register_command!(CloseCommand);

/// `ai/inference/inspect` — inspect an open session (consumes handle). Enveloped.
pub struct InspectCommand;
impl crate::sdk_codegen::CommandSpec for InspectCommand {
    const NAME: &'static str = COMMAND_INSPECT;
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Low-level substrate: inspect an open inference handle's state. Diagnostic \
         plumbing, not a task tool.";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Enveloped;
    type Params = InspectParams;
    type Result = InspectResult;
}
crate::register_command!(InspectCommand);

// ── Typed handlers (the authoring trait) ───────────────────────────
//
// Each command is now ONE typed `execute`: typed params in, typed output out,
// `?` for errors, `ctx.handle()?` for the envelope handle — no from_value, no
// manual CommandRequest/CommandResponse, no match-on-name, no try/catch. The
// framework's `dispatch` (driven by each command's WireShape) owns all of that.
// The handlers borrow the module so they reach its shared state (the handle
// store + adapter map). This is the shape every one of the ~260 commands takes.

use crate::sdk_codegen::{dispatch, CommandError, CommandHandler, Ctx, Outcome};

struct OpenHandler<'a>(&'a InferenceHandleModule);
#[async_trait]
impl CommandHandler for OpenHandler<'_> {
    type Spec = OpenCommand;
    async fn execute(
        &self,
        _ctx: &Ctx,
        p: OpenParams,
    ) -> Result<Outcome<OpenResult>, CommandError> {
        let (handle, payload) = self.0.open(p).await?;
        Ok(Outcome::with_handle(payload, handle)) // mint — framework places it on the envelope
    }
}

struct GenerateHandler<'a>(&'a InferenceHandleModule);
#[async_trait]
impl CommandHandler for GenerateHandler<'_> {
    type Spec = GenerateCommand;
    async fn execute(
        &self,
        ctx: &Ctx,
        p: GenerateParams,
    ) -> Result<Outcome<GenerateResult>, CommandError> {
        let handle = ctx.handle()?; // consume — typed accessor, loud if absent
        Ok(self.0.generate(handle, p.request).await?.into())
    }
}

struct CloseHandler<'a>(&'a InferenceHandleModule);
#[async_trait]
impl CommandHandler for CloseHandler<'_> {
    type Spec = CloseCommand;
    async fn execute(
        &self,
        ctx: &Ctx,
        _p: CloseParams,
    ) -> Result<Outcome<CloseResult>, CommandError> {
        Ok(self.0.close(ctx.handle()?).await?.into())
    }
}

struct InspectHandler<'a>(&'a InferenceHandleModule);
#[async_trait]
impl CommandHandler for InspectHandler<'_> {
    type Spec = InspectCommand;
    async fn execute(
        &self,
        ctx: &Ctx,
        _p: InspectParams,
    ) -> Result<Outcome<InspectResult>, CommandError> {
        Ok(self.0.inspect(ctx.handle()?).await?.into())
    }
}

// ── Module ─────────────────────────────────────────────────────────

/// The ServiceModule. Holds Arc<InferenceHandleStore> + a local
/// adapter map (provider_id → Arc<dyn AIProviderAdapter>) populated
/// at wiring time.
///
/// **Why a local adapter map instead of reading from AdapterRegistry**:
/// historically AdapterRegistry stored `Box<dyn AIProviderAdapter>`
/// which made sharing references awkward. Task #162 fixed that —
/// the registry now stores `Arc<dyn AIProviderAdapter>` natively
/// and exposes `get_arc(provider_id)` for callers that need to
/// hold the reference past the read-lock scope. `open` now resolves
/// through `get_arc` as a fallback (local map first, then the shared
/// registry), so providers registered anywhere in the system are
/// reachable; the local map remains for adapters explicitly registered
/// on this module (tests + wiring sites that pre-dated #162).
///
/// A future refactor (after task #109 lands) can fold this into a
/// unified Arc-based registry; for now keeping the two surfaces
/// independent makes slice B small and reviewable.
pub struct InferenceHandleModule {
    store: Arc<InferenceHandleStore>,
    providers: Arc<DashMap<String, Arc<dyn AIProviderAdapter>>>,
    /// Optional coordinator. When set, `open` / `close` route
    /// through lane lifecycle (admission, lease, footprint); when
    /// None, the module is in back-compat direct-store mode (the
    /// shipped #107B behavior).
    coordinator: Option<Arc<InferenceCoordinator>>,
}

impl InferenceHandleModule {
    /// Construct without a coordinator — direct-store mode
    /// (existing #107B behavior). Useful for tests that don't
    /// need lane lifecycle / observability + for incremental
    /// rollout where wiring picks coordinator-or-not at boot.
    pub fn new(store: Arc<InferenceHandleStore>) -> Self {
        Self {
            store,
            providers: Arc::new(DashMap::new()),
            coordinator: None,
        }
    }

    /// Construct with a coordinator. The store inside the
    /// coordinator is used; the `store` field shadows it for
    /// the back-compat read path (`generate` still routes
    /// directly to the handle store; Step 4 will wire batching
    /// via the coordinator).
    pub fn with_coordinator(coordinator: Arc<InferenceCoordinator>) -> Self {
        Self {
            store: coordinator.handle_store(),
            providers: Arc::new(DashMap::new()),
            coordinator: Some(coordinator),
        }
    }

    /// Register an adapter under a provider_id. Called at wiring
    /// time before the module is exposed to commands. Returns the
    /// previous adapter if one was registered for this provider_id
    /// (so callers can decide whether to log a swap).
    pub fn register_adapter(
        &self,
        provider_id: impl Into<String>,
        adapter: Arc<dyn AIProviderAdapter>,
    ) -> Option<Arc<dyn AIProviderAdapter>> {
        self.providers.insert(provider_id.into(), adapter)
    }

    pub fn store(&self) -> Arc<InferenceHandleStore> {
        self.store.clone()
    }
}

#[async_trait]
impl ServiceModule for InferenceHandleModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ai-inference-handle",
            priority: ModulePriority::High,
            command_prefixes: &["ai/inference/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // The handle store uses DashMap + per-session atomics;
            // it's safe under arbitrary concurrency. The scheduler
            // (task #109) will introduce slot caps; until then, 0
            // = unlimited (module manages own concurrency).
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        // Each arm is one line: build the typed handler (borrowing self for shared
        // state) and hand it to the framework dispatch, which parses the envelope,
        // runs the handler's typed `execute`, shapes the reply per WireShape, and
        // maps errors to the refusal channel. All the per-command boilerplate that
        // used to live here now lives in the framework, once.
        match command {
            COMMAND_OPEN => dispatch(&OpenHandler(self), params).await,
            COMMAND_GENERATE => dispatch(&GenerateHandler(self), params).await,
            COMMAND_CLOSE => dispatch(&CloseHandler(self), params).await,
            COMMAND_INSPECT => dispatch(&InspectHandler(self), params).await,
            other => Err(format!(
                "ai-inference-handle: unknown command '{other}' \
                 (known: {COMMAND_OPEN}, {COMMAND_GENERATE}, {COMMAND_CLOSE}, {COMMAND_INSPECT})"
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl InferenceHandleModule {
    async fn open(&self, params: OpenParams) -> Result<(HandleRef, OpenResult), String> {
        // Resolve the adapter: the module's local map first (adapters
        // explicitly registered on this instance — tests + pre-#162 wiring
        // sites), then the shared `AdapterRegistry` (the system-wide source
        // of truth where production adapters live, per #162's `get_arc`).
        // The local lookup is resolved into an OWNED Option in its own
        // statement so the DashMap `Ref` is dropped before the async
        // registry read — never a lock held across an `await`
        // (CONCURRENCY-STYLE-GUIDE forbidden move #7).
        let local_adapter = self
            .providers
            .get(&params.provider)
            .map(|entry| entry.value().clone());
        let adapter = match local_adapter {
            Some(adapter) => adapter,
            None => {
                // Async read guard scoped to this single statement; the
                // returned `Arc` is owned, so the guard drops at the `;`.
                // (`open_lane` below is synchronous, so no await follows the
                // read anyway — the guard is held across nothing.)
                let from_registry = crate::modules::ai_provider::global_registry()
                    .read()
                    .await
                    .get_arc(&params.provider);
                match from_registry {
                    Some(adapter) => adapter,
                    None => {
                        let available: Vec<String> =
                            self.providers.iter().map(|e| e.key().clone()).collect();
                        return Err(format!(
                            "{COMMAND_OPEN}: provider '{}' not registered on this module \
                             (local: {available:?}) nor in the shared AdapterRegistry",
                            params.provider
                        ));
                    }
                }
            }
        };

        if let Some(coordinator) = &self.coordinator {
            let persona = PeerId::from_uuid(params.persona_id.unwrap_or_else(Uuid::new_v4));
            let task = params.task.unwrap_or(TaskKind::Chat);
            let now_ms = crate::time::EpochMs::now().millis();
            let lane_req = OpenLaneRequest {
                persona,
                task,
                adapter,
                model: params.model,
                system_prompt: params.system_prompt,
                active_adapters: params.active_adapters,
                class_override: params.class_override,
                now_ms,
            };
            let handle = coordinator.open_lane(lane_req).map_err(|e| match e {
                CoordinatorError::AdmissionDenied { reason, task, persona } => format!(
                    "{COMMAND_OPEN}: admission denied (reason: {reason:?}, task: {task:?}, persona: {})",
                    persona.as_uuid()
                ),
                CoordinatorError::LeaseAcquireFailed(msg) => {
                    format!("{COMMAND_OPEN}: lease acquire failed: {msg}")
                }
                CoordinatorError::HandleNotFound { handle_id } => {
                    format!("{COMMAND_OPEN}: handle not found: {handle_id}")
                }
            })?;
            return Ok((
                handle,
                OpenResult {
                    provider: params.provider,
                },
            ));
        }

        // Back-compat path — direct store, no lane lifecycle.
        let handle = self.store.open(
            adapter,
            OpenSessionRequest {
                model: params.model,
                system_prompt: params.system_prompt,
                active_adapters: params.active_adapters,
                persona_id: params.persona_id,
            },
        );
        Ok((
            handle,
            OpenResult {
                provider: params.provider,
            },
        ))
    }

    async fn generate(
        &self,
        handle: HandleRef,
        request: TextGenerationRequest,
    ) -> Result<GenerateResult, String> {
        self.store
            .generate(&handle, request)
            .await
            .map_err(|e| format!("{COMMAND_GENERATE}: {e}"))
    }

    async fn close(&self, handle: HandleRef) -> Result<CloseResult, String> {
        if let Some(coordinator) = &self.coordinator {
            let released = coordinator
                .close_lane(&handle)
                .map_err(|e| format!("{COMMAND_CLOSE}: {e}"))?;
            return Ok(CloseResult { released });
        }
        let released = self
            .store
            .close(&handle)
            .map_err(|e| format!("{COMMAND_CLOSE}: {e}"))?;
        Ok(CloseResult { released })
    }

    async fn inspect(&self, handle: HandleRef) -> Result<InspectResult, String> {
        let snapshot = self
            .store
            .inspect(&handle)
            .map_err(|e| format!("{COMMAND_INSPECT}: {e}"))?;
        let mut result = InspectResult {
            provider_id: snapshot.provider_id,
            model: snapshot.model,
            persona_id: snapshot.persona_id,
            created_at_ms: snapshot.created_at_ms,
            last_used_ms: snapshot.last_used_ms,
            generation_count: snapshot.generation_count,
            has_system_prompt: snapshot.has_system_prompt,
            active_adapter_count: snapshot.active_adapter_count as u32,
            task: None,
            class: None,
            seed_kv_tokens: None,
            max_kv_tokens: None,
            bytes_accounted: None,
            lease_expires_at_ms: None,
            is_pinned: None,
        };
        if let Some(coordinator) = &self.coordinator {
            if let Some(lane) = coordinator.inspect(&handle) {
                result.task = Some(lane.task);
                result.class = Some(lane.class);
                result.seed_kv_tokens = Some(lane.seed_kv_tokens);
                result.max_kv_tokens = Some(lane.max_kv_tokens);
                result.bytes_accounted = Some(lane.bytes_accounted);
                result.lease_expires_at_ms = Some(lane.lease_expires_at_ms);
                result.is_pinned = Some(lane.is_pinned);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
