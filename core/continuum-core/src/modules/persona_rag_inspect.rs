//! ServiceModule wrapper for `persona::rag_inspect` per task #100.
//!
//! Joel (2026-05-31): "AIs are gonna need to analyze what's getting
//! fed into a persona." The library function in
//! `persona::rag_inspect` has shipped since slice A of #100; this
//! module exposes it as `persona/rag-inspect` so other AIs
//! (Claude, sentinel personas, peers via airc) can introspect a
//! persona's RAG state with a single `Commands.execute` call.
//!
//! ### Architecture
//!
//! - `PersonaResolver` trait — abstracts "given a persona name,
//!   give me the persona_id + the AircTranscriptReader to inspect
//!   their transcript." Production wiring plugs in a resolver that
//!   reads `~/.continuum/personas/<name>/seed.json` + attaches via
//!   `airc_lib::Airc::attach_as`. Tests use a stub.
//! - `PersonaRagInspectModule` — ServiceModule. Holds an
//!   `Arc<dyn PersonaResolver>` and contributes the typed
//!   `persona/rag-inspect` [`ActionCommand`](crate::sdk_codegen::ActionCommand)
//!   via [`commands`](PersonaRagInspectModule::commands) (dep-holding — it
//!   captures the resolver). The command body is the free fn
//!   [`inspect_persona`]: translate wire-shape params into a
//!   `RagInspectionRequest`, call `inspect_persona_rag_with_inference`,
//!   materialize the response. `handle_command` survives only as a fail-loud
//!   safety net until Registry A is retired wholesale (Wave Z, #63).
//!
//! ### Doctrine alignment
//!
//! - [[commands-are-kernel-level-and-compose]] — pure command
//!   routing; no introspection logic in the module beyond
//!   delegation.
//! - [[observability-is-half-the-architecture]] — `trace_path` flows
//!   through into the library so capture sinks fire when callers
//!   ask for replay-ready introspection.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::persona::airc_source::AircTranscriptReader;
use crate::persona::rag_inspect::{
    inspect_persona_rag_with_inference, RagInspection, RagInspectionRequest,
};
use crate::runtime::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;

// ── Command name ──────────────────────────────────────────────────

pub const COMMAND_RAG_INSPECT: &str = "persona/rag-inspect";

// ── Persona resolution (wiring seam) ──────────────────────────────

/// Result of resolving a persona name to inspection inputs.
pub struct PersonaResolution {
    pub persona_id: Uuid,
    pub airc_reader: Arc<dyn AircTranscriptReader>,
    /// Optional inference adapter for the chained probe. When the
    /// caller sets `chain_inference: true` AND the resolver
    /// returns Some, the inspection runs RAG → prompt → adapter →
    /// captured response. Resolver-supplied (not caller-supplied)
    /// so the substrate decides which adapter — typically the
    /// persona's preferred one (heuristic for tests; llama.cpp /
    /// cloud / remote-grid for production).
    pub inference_adapter: Option<Arc<dyn AIProviderAdapter>>,
}

/// Maps a persona name to its persona_id + airc reader. Production
/// wiring implements this against the real airc daemon + persona
/// seed file. Tests stub it.
///
/// `resolve` is async because the production impl (a) reads
/// `~/.continuum/personas/<name>/seed.json` via `tokio::fs` and
/// (b) attaches to the airc daemon via `airc_lib::Airc::attach_as`
/// which is async. Stubs can return immediately via `async {}.await`.
#[async_trait]
pub trait PersonaResolver: Send + Sync {
    async fn resolve(&self, name: &str) -> Result<PersonaResolution, String>;
}

// ── Wire types ────────────────────────────────────────────────────

/// Params for `persona/rag-inspect`. The persona name is the only
/// required input; everything else has defaults from the canonical
/// library `defaults_for`. Optional knobs let callers vary the
/// inspection profile (tighter window, deeper fetch, capture
/// trace).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RagInspectParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RagInspectParams {
    /// Which persona's RAG pipeline to introspect — her name or id (full or the
    /// short form shown in rosters).
    pub persona: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub context_window: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub airc_floor: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub airc_max: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub airc_fetch_limit: Option<u32>,
    /// Optional absolute path for the JSONL capture trace. When set,
    /// the inspection records the full turn there so other AIs /
    /// mechanic shop can replay it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trace_path: Option<String>,
    /// Optional override for the wall-clock timestamp the inspection
    /// reasons against. Default: substrate's current wall-clock.
    /// Set this for deterministic replay tests.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub now_ms: Option<u64>,
    /// When true, chain through inference: assemble delivered items
    /// into a prompt, call the persona's adapter, capture the
    /// response into `modelResponse`. Default false (RAG-only).
    /// Per [[inference-is-an-adapter-always-in-the-loop]] — closes
    /// the introspection loop so AIs can answer "would I respond
    /// as it requests?" in one command call.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub chain_inference: Option<bool>,
}

/// One source's allocation outcome — flattened from the library's
/// BudgetAllocation for the wire.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RagInspectAllocation.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RagInspectAllocation {
    pub source_id: String,
    #[ts(type = "number")]
    pub allocated_tokens: u32,
    #[ts(type = "number")]
    pub requested_floor: u32,
    #[ts(type = "number")]
    pub requested_min: u32,
    #[ts(type = "number")]
    pub requested_max: u32,
    /// "satisfied" / "floor_only" / "dropped" / "under_provisioned"
    pub state: String,
}

/// One item the source delivered, with the mechanic-grade rationale
/// flattened for the wire (content_preview, score, age_s, etc).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RagInspectItem.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RagInspectItem {
    #[ts(type = "number")]
    pub index: u32,
    #[ts(type = "number")]
    pub tokens: u32,
    #[ts(type = "number")]
    pub score: f64,
    pub content_preview: String,
    pub peer_id_prefix: String,
    #[ts(type = "number")]
    pub lamport: u64,
    #[ts(type = "number")]
    pub age_s: u64,
}

/// One source's delivery — its budget, what it served, and the per-
/// item rationale.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RagInspectDelivery.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RagInspectDelivery {
    pub source_id: String,
    #[ts(type = "number")]
    pub budget_requested: u32,
    #[ts(type = "number")]
    pub tokens_used: u32,
    pub has_continuation: bool,
    pub items: Vec<RagInspectItem>,
}

/// Result of `persona/rag-inspect`. Carries the full allocation
/// outcome + per-source deliveries so any AI inspecting the persona
/// can answer the three canonical questions:
/// - "Would I respond as it requests at this step?" — full prompt
///   reconstructable from `deliveries`; when `chainInference=true`,
///   the actual model response is captured in `modelResponse`.
/// - "Which layer is broken?" — per-source `allocations` show state
///   (satisfied / floor_only / dropped / under_provisioned).
/// - "Is this contextually relevant?" — per-item score + age +
///   peer in the deliveries.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RagInspectResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RagInspectResult {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub persona_name: String,
    #[ts(type = "number")]
    pub context_window: u32,
    /// Sum of all source allocations. Useful for "did we leave
    /// tokens on the table?" telemetry.
    #[ts(type = "number")]
    pub total_allocated: u32,
    /// True if the allocator reported `escalation_needed` — a
    /// required source landed under-provisioned. Callers (AIs)
    /// SHOULD flag this in their reasoning.
    pub escalation_needed: bool,
    pub allocations: Vec<RagInspectAllocation>,
    pub deliveries: Vec<RagInspectDelivery>,
    /// JSONL trace path (relative or absolute) when `trace_path`
    /// was set on the request. Other AIs / mechanic-shop tools
    /// resume replay against this.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trace_path: Option<String>,
    /// Captured model response when `chainInference=true` was set
    /// AND the resolver supplied an inference adapter. None on the
    /// RAG-only path.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_response: Option<RagInspectModelResponse>,
}

/// What the model actually said when the inspection chained through
/// inference — the answer to the canonical question "would I respond
/// as it requests at this step?"
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/RagInspectModelResponse.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RagInspectModelResponse {
    pub adapter_id: String,
    pub model: String,
    /// The assembled prompt — system + messages joined for human +
    /// AI replay. Other AIs can paste this into a different model
    /// to compare responses ("would Claude respond differently?").
    pub prompt_text: String,
    pub response_text: String,
    pub finish_reason: String,
    #[ts(type = "number")]
    pub input_tokens: u32,
    #[ts(type = "number")]
    pub output_tokens: u32,
    #[ts(type = "number")]
    pub response_time_ms: u64,
}

// ── Conversion from library types ─────────────────────────────────

impl RagInspectResult {
    fn from_library(value: RagInspection) -> Self {
        let total_allocated = value.allocation.total_allocated;
        let escalation_needed = value.allocation.escalation_needed;
        let allocations = value
            .allocation
            .allocations
            .into_iter()
            .map(|a| RagInspectAllocation {
                source_id: a.source_id,
                allocated_tokens: a.allocated_tokens,
                requested_floor: a.requested_floor,
                requested_min: a.requested_min,
                requested_max: a.requested_max,
                state: allocation_state_to_str(a.state).to_string(),
            })
            .collect();
        let deliveries = value
            .deliveries
            .into_iter()
            .map(|d| RagInspectDelivery {
                source_id: d.source_id,
                budget_requested: d.budget_requested,
                tokens_used: d.tokens_used,
                has_continuation: d.has_continuation,
                items: d
                    .items
                    .into_iter()
                    .map(|i| RagInspectItem {
                        index: i.index as u32,
                        tokens: i.tokens,
                        score: i.score,
                        content_preview: i.content_preview,
                        peer_id_prefix: i.peer_id_prefix,
                        lamport: i.lamport,
                        age_s: i.age_s,
                    })
                    .collect(),
            })
            .collect();
        let model_response = value.model_response.map(|m| RagInspectModelResponse {
            adapter_id: m.adapter_id,
            model: m.model,
            prompt_text: m.prompt_text,
            response_text: m.response_text,
            finish_reason: m.finish_reason,
            input_tokens: m.input_tokens,
            output_tokens: m.output_tokens,
            response_time_ms: m.response_time_ms,
        });
        Self {
            persona_id: value.persona_id,
            persona_name: value.persona_name,
            context_window: value.context_window,
            total_allocated,
            escalation_needed,
            allocations,
            deliveries,
            trace_path: value
                .trace_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            model_response,
        }
    }
}

fn allocation_state_to_str(state: crate::persona::rag_budget::AllocationState) -> &'static str {
    use crate::persona::rag_budget::AllocationState as S;
    match state {
        S::Satisfied => "satisfied",
        S::FloorOnly => "floor_only",
        S::Dropped => "dropped",
        S::UnderProvisioned => "under_provisioned",
    }
}

// ── Module ────────────────────────────────────────────────────────

pub struct PersonaRagInspectModule {
    resolver: Arc<dyn PersonaResolver>,
}

impl PersonaRagInspectModule {
    pub fn new(resolver: Arc<dyn PersonaResolver>) -> Self {
        Self { resolver }
    }
}

#[async_trait]
impl ServiceModule for PersonaRagInspectModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona-rag-inspect",
            priority: ModulePriority::Normal,
            command_prefixes: &["persona/rag-inspect"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // `persona/rag-inspect` is a migrated, typed `ActionCommand` that routes via
        // `route_object` (dep-holding — it captures this module's resolver; see
        // `crate::commands::persona::rag_inspect`). Reaching this legacy path means a
        // descriptor failed to register — fail loud naming the command rather than
        // silently re-handling. (Retired wholesale when Registry A's trait default
        // becomes fail-loud — #63.)
        Err(format!(
            "'{command}' is a migrated, typed persona command ({COMMAND_RAG_INSPECT}) \
             — it must route via the object registry (route_object), not the legacy \
             handle_command path. Reaching here means its descriptor failed to register."
        ))
    }

    /// The migrated `persona/rag-inspect` command as a typed self-routing object on
    /// the ONE registry. Dep-holding: it captures this module's resolver so its
    /// `CommandSpec` descriptor flows into `command_registry()` → the persona tool
    /// surface + grid ACL.
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::persona::rag_inspect::command_objects(self.resolver.clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Introspect a persona's RAG pipeline: resolve the name → build the inspection
/// request → run the (optionally inference-chained) library probe → flatten to the
/// wire shape. The `persona/rag-inspect` command body; a free fn so the typed
/// command and the module's tests share ONE implementation.
pub(crate) async fn inspect_persona(
    resolver: &Arc<dyn PersonaResolver>,
    params: RagInspectParams,
) -> Result<RagInspectResult, String> {
    if params.persona.trim().is_empty() {
        return Err(format!(
            "{COMMAND_RAG_INSPECT}: persona name is required (got empty string)"
        ));
    }
    let resolution = resolver.resolve(&params.persona).await.map_err(|e| {
        format!(
            "{COMMAND_RAG_INSPECT}: resolve persona '{}': {e}",
            params.persona
        )
    })?;

    let now_ms = params.now_ms.unwrap_or_else(now_ms_default);
    let mut request =
        RagInspectionRequest::defaults_for(resolution.persona_id, params.persona.clone(), now_ms);
    if let Some(cw) = params.context_window {
        request.context_window = cw;
    }
    if let Some(floor) = params.airc_floor {
        request.airc_floor = floor;
    }
    if let Some(max) = params.airc_max {
        request.airc_max = max;
    }
    if let Some(fetch) = params.airc_fetch_limit {
        request.airc_fetch_limit = fetch as usize;
    }
    if let Some(p) = params.trace_path {
        request.trace_path = Some(PathBuf::from(p));
    }

    // Chain through inference when the caller asks AND the resolver supplied an
    // adapter. Either being false → RAG-only.
    let inference_probe = if params.chain_inference.unwrap_or(false) {
        resolution.inference_adapter.clone()
    } else {
        None
    };
    let inspection =
        inspect_persona_rag_with_inference(&request, resolution.airc_reader, inference_probe)
            .await
            .map_err(|e| format!("{COMMAND_RAG_INSPECT}: {e}"))?;
    Ok(RagInspectResult::from_library(inspection))
}

fn now_ms_default() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::persona::rag_inspect::PersonaRagInspect;
    use crate::persona::airc_source::AircTranscriptReader;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptEvent,
        TranscriptKind,
    };
    use airc_lib::AircError;
    use std::sync::Mutex;

    fn persona_uuid() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    struct StubReader {
        events: Mutex<Vec<TranscriptEvent>>,
        fail: Mutex<bool>,
    }
    impl StubReader {
        fn with_events(events: Vec<TranscriptEvent>) -> Arc<Self> {
            Arc::new(Self {
                events: Mutex::new(events),
                fail: Mutex::new(false),
            })
        }
    }
    #[async_trait]
    impl AircTranscriptReader for StubReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self
                .events
                .lock()
                .unwrap()
                .iter()
                .take(limit)
                .cloned()
                .collect())
        }
    }

    /// All events in these tests share ONE room. A real airc channel has a single
    /// room_id, and the digest is room-scoped (filters events to the room derived
    /// from the transcript, slice-2 #43). A per-event random room would model
    /// nothing real and silently drop all-but-the-last event from the window.
    static TEST_ROOM: std::sync::LazyLock<RoomId> = std::sync::LazyLock::new(RoomId::new);

    fn make_event(text: Option<&str>, lamport: u64, occurred_at_ms: u64) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: *TEST_ROOM,
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms,
            lamport,
            target: MentionTarget::Room(*TEST_ROOM),
            headers: Headers::default(),
            body: text.map(Body::text),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    struct StubResolver {
        reader: Arc<dyn AircTranscriptReader>,
        valid_names: Vec<String>,
        inference_adapter: Option<Arc<dyn AIProviderAdapter>>,
    }

    #[async_trait]
    impl PersonaResolver for StubResolver {
        async fn resolve(&self, name: &str) -> Result<PersonaResolution, String> {
            if !self.valid_names.iter().any(|n| n == name) {
                return Err(format!("persona '{name}' not found in stub resolver"));
            }
            Ok(PersonaResolution {
                persona_id: persona_uuid(),
                airc_reader: self.reader.clone(),
                inference_adapter: self.inference_adapter.clone(),
            })
        }
    }

    fn module_with(events: Vec<TranscriptEvent>) -> PersonaRagInspectModule {
        let reader = StubReader::with_events(events);
        let resolver = Arc::new(StubResolver {
            reader,
            valid_names: vec!["Paige".to_string(), "Pax".to_string()],
            inference_adapter: None,
        });
        PersonaRagInspectModule::new(resolver)
    }

    fn module_with_inference(events: Vec<TranscriptEvent>) -> PersonaRagInspectModule {
        use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
        let reader = StubReader::with_events(events);
        let resolver = Arc::new(StubResolver {
            reader,
            valid_names: vec!["Paige".to_string(), "Pax".to_string()],
            inference_adapter: Some(
                Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>
            ),
        });
        PersonaRagInspectModule::new(resolver)
    }

    // ── command surface ───────────────────────────────────────────

    #[test]
    fn config_reports_canonical_name_and_prefix() {
        let m = module_with(vec![]);
        let cfg = m.config();
        assert_eq!(cfg.name, "persona-rag-inspect");
        assert_eq!(cfg.command_prefixes, &["persona/rag-inspect"]);
    }

    #[tokio::test]
    async fn empty_persona_name_returns_typed_error() {
        let m = module_with(vec![]);
        let result = inspect_persona(
            &m.resolver,
            RagInspectParams {
                persona: "".to_string(),
                ..Default::default()
            },
        )
        .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("persona name is required"));
    }

    #[tokio::test]
    async fn unknown_persona_surfaces_resolver_error() {
        let m = module_with(vec![]);
        let result = inspect_persona(
            &m.resolver,
            RagInspectParams {
                persona: "Unknown".to_string(),
                ..Default::default()
            },
        )
        .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found in stub resolver"));
    }

    #[tokio::test]
    async fn known_persona_with_empty_room_returns_zero_items_but_satisfied_allocation() {
        let m = module_with(vec![]);
        let result = inspect_persona(
            &m.resolver,
            RagInspectParams {
                persona: "Paige".to_string(),
                now_ms: Some(1_000_000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(result.persona_name, "Paige");
        assert_eq!(result.persona_id, persona_uuid());
        // Allocator gives the airc source its full max (default 20k);
        // delivery is empty since there are no events.
        assert!(!result.escalation_needed);
        assert_eq!(result.allocations.len(), 1);
        assert_eq!(result.allocations[0].source_id, "airc");
        assert_eq!(result.allocations[0].state, "satisfied");
        assert_eq!(result.deliveries.len(), 1);
        assert!(result.deliveries[0].items.is_empty());
    }

    #[tokio::test]
    async fn known_persona_with_events_returns_items_with_full_rationale() {
        let m = module_with(vec![
            make_event(Some("hello world"), 1, 900_000),
            make_event(Some("second message"), 2, 950_000),
        ]);
        let result = inspect_persona(
            &m.resolver,
            RagInspectParams {
                persona: "Paige".to_string(),
                now_ms: Some(1_000_000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(result.deliveries[0].items.len(), 2);
        let first = &result.deliveries[0].items[0];
        assert_eq!(first.content_preview, "hello world");
        assert!((first.score - 1.0).abs() < 1e-9);
        assert_eq!(first.age_s, 100);
        // peer_id_prefix is 8 hex chars from the UUID.
        assert_eq!(first.peer_id_prefix.len(), 8);
    }

    #[tokio::test]
    async fn context_window_override_threads_through() {
        let m = module_with(vec![make_event(Some("hi"), 1, 990_000)]);
        let result = inspect_persona(
            &m.resolver,
            RagInspectParams {
                persona: "Pax".to_string(),
                context_window: Some(8_192),
                now_ms: Some(1_000_000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(result.context_window, 8_192);
    }

    // what this catches: the migrated typed command routes plain params (no envelope)
    // through the shared inspect logic and returns the typed output — the object on the
    // ONE registry produces the same delivery a `Commands.execute('persona/rag-inspect')`
    // caller sees.
    #[tokio::test]
    async fn typed_command_routes_params_to_inspect() {
        let m = module_with(vec![make_event(Some("hi"), 1, 990_000)]);
        let cmd = PersonaRagInspect {
            resolver: m.resolver.clone(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                RagInspectParams {
                    persona: "Paige".to_string(),
                    now_ms: Some(1_000_000),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(out.persona_name, "Paige");
        assert_eq!(out.deliveries.len(), 1);
    }

    // what this catches: the legacy handle_command path is retired — the migrated
    // command fails loud naming itself, never silently re-handles on the old path. A
    // regression that re-adds an inline arm (forking a command off the typed object) is
    // caught here.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let m = module_with(vec![]);
        let err = m
            .handle_command(COMMAND_RAG_INSPECT, Value::Null)
            .await
            .expect_err("migrated command must fail loud on the legacy path");
        assert!(err.contains("migrated"), "got {err}");
        assert!(err.contains(COMMAND_RAG_INSPECT), "got {err}");
    }

    // ── chained inference probe (task #104) ────────────────────

    #[tokio::test]
    async fn rag_only_default_leaves_model_response_none() {
        // chain_inference omitted/false → no model_response in result
        // (even when the resolver could supply an adapter).
        let m = module_with_inference(vec![make_event(Some("hi"), 1, 999_000)]);
        let result = inspect_persona(
            &m.resolver,
            RagInspectParams {
                persona: "Paige".to_string(),
                now_ms: Some(1_000_000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(result.model_response.is_none());
    }

    #[tokio::test]
    async fn chain_inference_with_adapter_captures_model_response() {
        let m = module_with_inference(vec![
            make_event(Some("first message"), 1, 999_000),
            make_event(Some("second message"), 2, 999_500),
        ]);
        let result = inspect_persona(
            &m.resolver,
            RagInspectParams {
                persona: "Paige".to_string(),
                now_ms: Some(1_000_000),
                chain_inference: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let mr = result.model_response.expect("expected model_response");
        assert_eq!(mr.adapter_id, "heuristic");
        assert!(mr.response_text.starts_with("[heuristic:"));
        // Heuristic echoes the LAST user message.
        assert!(mr.response_text.contains("second message"));
        assert!(mr.prompt_text.contains("You are Paige"));
        assert_eq!(mr.finish_reason, "stop");
    }

    #[tokio::test]
    async fn chain_inference_without_adapter_stays_rag_only() {
        // chain_inference=true but resolver returns no adapter — the
        // inspection silently degrades to RAG-only (no model_response).
        let m = module_with(vec![make_event(Some("hi"), 1, 999_000)]);
        let result = inspect_persona(
            &m.resolver,
            RagInspectParams {
                persona: "Paige".to_string(),
                now_ms: Some(1_000_000),
                chain_inference: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        // Resolver returned None for inference_adapter; chain skipped.
        assert!(result.model_response.is_none());
    }

    // what this catches: chaining inference through the migrated typed command captures
    // the model response on the typed output — the object surface answers "would I
    // respond as it requests at this step?" in one call, same as the chained inspect.
    #[tokio::test]
    async fn typed_command_chains_inference_into_model_response() {
        let m = module_with_inference(vec![make_event(Some("ping"), 1, 999_000)]);
        let cmd = PersonaRagInspect {
            resolver: m.resolver.clone(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                RagInspectParams {
                    persona: "Paige".to_string(),
                    now_ms: Some(1_000_000),
                    chain_inference: Some(true),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let mr = out.model_response.expect("model_response present");
        assert_eq!(mr.adapter_id, "heuristic");
        assert!(mr.response_text.starts_with("[heuristic:"));
    }
}
