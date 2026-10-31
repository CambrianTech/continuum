#![cfg(test)]

use super::*;
use crate::ai::heuristic_adapter::{HeuristicInferenceAdapter, HEURISTIC_PROVIDER_ID};
use crate::ai::types::{ChatMessage, MessageContent};

fn user_msg(text: &str) -> ChatMessage {
    ChatMessage {
        role: "user".to_string(),
        content: MessageContent::Text(text.to_string()),
        name: None,
    }
}

fn empty_request() -> TextGenerationRequest {
    TextGenerationRequest {
        messages: vec![user_msg("test prompt")],
        system_prompt: None,
        model: None,
        provider: None,
        temperature: None,
        max_tokens: None,
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
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: None,
        persona_id: None,
        turn_bound: None,
    }
}

fn module_with_heuristic() -> InferenceHandleModule {
    let store = Arc::new(InferenceHandleStore::new());
    let module = InferenceHandleModule::new(store);
    module.register_adapter(
        HEURISTIC_PROVIDER_ID,
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    );
    module
}

/// A coordinator-backed module with an EMPTY local adapter map — used
/// to exercise the shared-registry fallback in `open`. `module_with_
/// coordinator` layers a local heuristic registration on top.
fn bare_coordinator_module() -> InferenceHandleModule {
    use crate::resources::throughput_class::{ResourceClass, TargetSilicon};
    use crate::inference::planning::adaptive_throughput::ThroughputLaneBudget;
    use crate::inference::coordinator::{CoordinatorConfig, InferenceCoordinator};
    use crate::inference::footprint_registry::FootprintRegistry;
    let footprint = Arc::new(FootprintRegistry::new());
    let store = Arc::new(InferenceHandleStore::new());
    let config = CoordinatorConfig {
        lane_budgets: vec![ThroughputLaneBudget {
            resource_class: ResourceClass::LocalGeneration,
            target_silicon: TargetSilicon::UnifiedMemory,
            max_concurrency: 4,
            max_cost_units: 40_000,
        }],
        bytes_per_token: 64 * 1024,
        lease_duration_ms: 60_000,
        default_target_silicon: TargetSilicon::UnifiedMemory,
    };
    let coordinator = Arc::new(InferenceCoordinator::new(footprint, store, config));
    InferenceHandleModule::with_coordinator(coordinator)
}

fn module_with_coordinator() -> InferenceHandleModule {
    let module = bare_coordinator_module();
    module.register_adapter(
        HEURISTIC_PROVIDER_ID,
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    );
    module
}

// ── command surface ────────────────────────────────────────────

#[test]
fn config_reports_canonical_module_name_and_prefix() {
    let m = module_with_heuristic();
    let cfg = m.config();
    assert_eq!(cfg.name, "ai-inference-handle");
    assert_eq!(cfg.command_prefixes, &["ai/inference/"]);
    assert!(!cfg.needs_dedicated_thread);
}

#[tokio::test]
async fn open_through_command_returns_handleref_with_canonical_tags() {
    let m = module_with_heuristic();
    let envelope = serde_json::to_value(CommandRequest::new(OpenParams {
        provider: HEURISTIC_PROVIDER_ID.to_string(),
        ..Default::default()
    }))
    .unwrap();
    let result = m.handle_command(COMMAND_OPEN, envelope).await.unwrap();
    let json = match result {
        CommandResult::Json(v) => v,
        other => panic!("expected Json, got {other:?}"),
    };
    // CommandResponse flattens — `data` is NOT a nested object;
    // the OpenResult fields + envelope handle live at the top
    // level alongside `success`.
    let response = json.as_object().unwrap();
    assert_eq!(response.get("success").unwrap(), &Value::Bool(true));
    let handle = response.get("handle").unwrap().as_object().unwrap();
    assert_eq!(handle.get("owner").unwrap(), HANDLE_OWNER);
    assert_eq!(handle.get("type_tag").unwrap(), HANDLE_TYPE_TAG);
    assert_eq!(response.get("provider").unwrap(), HEURISTIC_PROVIDER_ID);
}

/// What this catches: `open` falls back to the shared `AdapterRegistry`
/// when the provider is absent from the module's local map — the
/// realization that lets production opens resolve adapters registered
/// elsewhere in the system (#162 `get_arc`). The module here registers
/// NOTHING locally; the adapter lives only in the global registry.
/// Without the fallback this open would 404.
#[tokio::test]
async fn open_resolves_via_shared_registry_when_absent_from_local_map() {
    // Register the heuristic adapter in the SHARED registry only.
    // get_arc(HEURISTIC_PROVIDER_ID) is robust to a prior test having
    // already registered it — either way it resolves to Some.
    crate::ai::adapter::global_registry()
        .write()
        .await
        .register(
            Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
            0,
        );
    let m = bare_coordinator_module();
    let (_handle, opened) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            ..Default::default()
        })
        .await
        .expect("open must resolve the provider via the shared AdapterRegistry fallback");
    assert_eq!(opened.provider, HEURISTIC_PROVIDER_ID);
}

#[tokio::test]
async fn open_with_unregistered_provider_returns_typed_error() {
    let m = module_with_heuristic();
    let envelope = serde_json::to_value(CommandRequest::new(OpenParams {
        provider: "no-such-provider".to_string(),
        ..Default::default()
    }))
    .unwrap();
    let result = m.handle_command(COMMAND_OPEN, envelope).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("not registered"),
        "expected adapter-not-registered error, got: {err}"
    );
}

#[tokio::test]
async fn generate_through_command_routes_to_adapter() {
    let m = module_with_heuristic();
    let (opened_handle, _opened) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let envelope = serde_json::to_value(
        CommandRequest::new(GenerateParams {
            request: empty_request(),
        })
        .with_handle(opened_handle.clone()),
    )
    .unwrap();
    let result = m.handle_command(COMMAND_GENERATE, envelope).await.unwrap();
    let json = match result {
        CommandResult::Json(v) => v,
        other => panic!("expected Json, got {other:?}"),
    };
    let response = json.as_object().unwrap();
    assert_eq!(response.get("success").unwrap(), &Value::Bool(true));
    // GenerateResult (TextGenerationResponse) fields flatten —
    // `text` lives at the top level, not under `data`.
    let text = response.get("text").unwrap().as_str().unwrap();
    assert!(
        text.starts_with("[heuristic:"),
        "expected heuristic adapter output, got: {text}"
    );
}

#[tokio::test]
async fn close_through_command_releases_session() {
    let m = module_with_heuristic();
    let (opened_handle, _opened) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let envelope = serde_json::to_value(
        CommandRequest::new(CloseParams::default()).with_handle(opened_handle.clone()),
    )
    .unwrap();
    let result = m.handle_command(COMMAND_CLOSE, envelope).await.unwrap();
    let json = match result {
        CommandResult::Json(v) => v,
        other => panic!("expected Json, got {other:?}"),
    };
    // CloseResult.released is flattened at the top level.
    assert_eq!(json.get("released").unwrap(), &Value::Bool(true));

    // Generate now fails — handle is closed.
    let envelope = serde_json::to_value(
        CommandRequest::new(GenerateParams {
            request: empty_request(),
        })
        .with_handle(opened_handle),
    )
    .unwrap();
    let result = m.handle_command(COMMAND_GENERATE, envelope).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[tokio::test]
async fn inspect_through_command_returns_session_snapshot() {
    let m = module_with_heuristic();
    let (opened_handle, _opened) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            system_prompt: Some("inspect me".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    let envelope = serde_json::to_value(
        CommandRequest::new(InspectParams::default()).with_handle(opened_handle),
    )
    .unwrap();
    let result = m.handle_command(COMMAND_INSPECT, envelope).await.unwrap();
    let json = match result {
        CommandResult::Json(v) => v,
        other => panic!("expected Json, got {other:?}"),
    };
    // InspectResult fields flatten at the top level.
    assert_eq!(json.get("providerId").unwrap(), HEURISTIC_PROVIDER_ID);
    assert_eq!(json.get("hasSystemPrompt").unwrap(), &Value::Bool(true));
    assert_eq!(json.get("generationCount").unwrap().as_u64().unwrap(), 0);
}

#[tokio::test]
async fn unknown_command_returns_loud_error() {
    let m = module_with_heuristic();
    let result = m
        .handle_command("ai/inference/something-bogus", Value::Null)
        .await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("unknown command"));
    assert!(err.contains(COMMAND_OPEN));
}

#[tokio::test]
async fn full_open_generate_close_round_trip_through_command_surface() {
    let m = module_with_heuristic();
    let (opened_handle, _opened) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let handle = opened_handle.clone();
    // Generate twice — same handle, two responses (increments
    // generation_count to 2).
    let r1 = m.generate(handle.clone(), empty_request()).await.unwrap();
    let r2 = m.generate(handle.clone(), empty_request()).await.unwrap();
    // Same prompt → same response (determinism contract).
    assert_eq!(r1.text, r2.text);
    // Inspect sees 2 generations.
    let snap = m.inspect(handle.clone()).await.unwrap();
    assert_eq!(snap.generation_count, 2);
    // Close releases.
    let closed = m.close(handle).await.unwrap();
    assert!(closed.released);
}

#[tokio::test]
async fn generate_without_envelope_handle_returns_loud_error() {
    let m = module_with_heuristic();
    let envelope = serde_json::to_value(CommandRequest::new(GenerateParams {
        request: empty_request(),
    }))
    .unwrap();
    let result = m.handle_command(COMMAND_GENERATE, envelope).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("missing required `handle`"));
}

// ── coordinator-wired path ─────────────────────────────────────

#[tokio::test]
async fn open_through_coordinator_creates_lane_and_returns_handle() {
    let m = module_with_coordinator();
    let (handle, _open) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            task: Some(TaskKind::VoiceChat),
            persona_id: Some(Uuid::from_u128(0xCAFE)),
            ..Default::default()
        })
        .await
        .unwrap();
    // Inspect should now carry lane fields (because coordinator-wired).
    let snapshot = m.inspect(handle).await.unwrap();
    assert_eq!(snapshot.task, Some(TaskKind::VoiceChat));
    assert_eq!(snapshot.class, Some(LaneClass::Realtime));
    assert_eq!(snapshot.seed_kv_tokens, Some(8 * 1024));
    assert_eq!(snapshot.is_pinned, Some(true));
    assert!(snapshot.bytes_accounted.unwrap() > 0);
    assert!(snapshot.lease_expires_at_ms.unwrap() > 0);
}

#[tokio::test]
async fn open_through_coordinator_defaults_task_to_chat_when_omitted() {
    let m = module_with_coordinator();
    let (handle, _open) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let snapshot = m.inspect(handle).await.unwrap();
    assert_eq!(snapshot.task, Some(TaskKind::Chat));
    assert_eq!(snapshot.class, Some(LaneClass::Interactive));
}

#[tokio::test]
async fn open_through_coordinator_admission_failure_surfaces_typed_error() {
    let m = module_with_coordinator();
    // Open 4 lanes (max_concurrency=4) → all admit. 5th denies.
    for i in 0..4 {
        m.open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            task: Some(TaskKind::Chat),
            persona_id: Some(Uuid::from_u128(i)),
            ..Default::default()
        })
        .await
        .unwrap();
    }
    let err = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            task: Some(TaskKind::Chat),
            persona_id: Some(Uuid::from_u128(99)),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(err.contains("admission denied"));
    assert!(err.contains("ResourcePressure"));
}

#[tokio::test]
async fn close_through_coordinator_releases_lane() {
    let m = module_with_coordinator();
    let (handle, _open) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            task: Some(TaskKind::Chat),
            persona_id: Some(Uuid::from_u128(1)),
            ..Default::default()
        })
        .await
        .unwrap();
    let closed = m.close(handle.clone()).await.unwrap();
    assert!(closed.released);
    // Inspect after close — base session is also gone (coordinator
    // closed the handle store entry too).
    let result = m.inspect(handle).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn class_override_promotes_chat_to_realtime_through_coordinator() {
    let m = module_with_coordinator();
    let (handle, _open) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            task: Some(TaskKind::Chat),
            persona_id: Some(Uuid::from_u128(1)),
            class_override: Some(LaneClass::Realtime),
            ..Default::default()
        })
        .await
        .unwrap();
    let snapshot = m.inspect(handle).await.unwrap();
    assert_eq!(snapshot.class, Some(LaneClass::Realtime));
    assert_eq!(snapshot.is_pinned, Some(true));
}

#[tokio::test]
async fn inspect_in_non_coordinator_mode_leaves_lane_fields_none() {
    // The original back-compat path doesn't have a coordinator, so
    // the lane fields stay None.
    let m = module_with_heuristic();
    let (handle, _open) = m
        .open(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let snapshot = m.inspect(handle).await.unwrap();
    assert!(snapshot.task.is_none());
    assert!(snapshot.class.is_none());
    assert!(snapshot.seed_kv_tokens.is_none());
    assert!(snapshot.is_pinned.is_none());
}
