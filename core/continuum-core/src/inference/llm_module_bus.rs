//! `inference-llm` PR-3a: canonical ArtifactKey constants +
//! publishing helpers for the four inference events.
//!
//! Background: PR-1 (#1387) shipped the typed events. PR-2 (#1391)
//! shipped the ServiceModule that emits InferenceComplete +
//! FirstTokenEmitted as command responses. What's been missing
//! is the artifact-dispatch path — the canonical ArtifactKeys
//! downstream subscribers (sentinel-observer, VDD harness,
//! audit-recorder) bind to.
//!
//! This module fills that gap with three building blocks (same
//! pattern as my genome::bus PR-4 / #1358):
//!
//! 1. **Canonical `ArtifactKey` constants** — every inference event
//!    has one stable key. Subscribers refer to the constant, not a
//!    string literal, so the wire stays consistent across renames.
//!
//! 2. **Publishing helpers** — `publish_inference_complete`,
//!    `publish_first_token_emitted`, `publish_residency_fault` —
//!    serialize the typed event + publish through the artifact
//!    dispatch path (#1339 + #1343).
//!
//! 3. **Subscriber convenience** — `subscribe_to_inference_events`
//!    wires a module to all three response keys at once. Producers
//!    subscribe separately if they need to observe their own
//!    requests; that's not the common case (most observers want
//!    completes + first-tokens + faults, not requests).
//!
//! ## What PR-3a does NOT ship
//!
//! - Wiring the helpers INTO `InferenceLlmModule::handle_command`
//!   so it auto-publishes after each call — that's PR-3b. PR-3a
//!   ships the wire so downstream subscribers can bind first.
//! - Real LLM engine — that's PR-4 (LlamaCppAdapter integration)
//! - InferenceRequest artifact subscription (the module subscribes
//!   to requests via this path instead of going through the command
//!   bus) — separate PR; needs persona-cognition to publish via
//!   bus first.

use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector};
use crate::runtime::message_bus::MessageBus;
use crate::runtime::registry::ModuleRegistry;

use super::llm_module::{FirstTokenEmitted, InferenceComplete, InferenceRequest, ResidencyFault};

// ─── Canonical ArtifactKey constants ─────────────────────────────

/// ArtifactKey for `InferenceRequest` events. Producers
/// (persona-cognition) publish a request on this key when they
/// want the inference engine to generate a turn. Subscribers:
/// `InferenceLlmModule` (consumes), VDD harness (logs the
/// request for prod-replay).
pub const INFERENCE_REQUEST_KEY: &str = "inference/llm.request";

/// ArtifactKey for `InferenceComplete` events. Published when
/// generation completes. Subscribers: persona-cognition
/// (consumes for downstream turn flow), sentinel-observer
/// (learns outcome → updates engram weights), audit-recorder
/// (logs every completion as a TurnReplayRecord input), VDD
/// harness (logs latency).
pub const INFERENCE_COMPLETE_KEY: &str = "inference/llm.complete";

/// ArtifactKey for `FirstTokenEmitted` events. Published when
/// the model produces its first token. Subscribers: VDD harness
/// (TTFT latency observability), persona-cognition (can start
/// downstream streaming-token-aware paths).
pub const FIRST_TOKEN_EMITTED_KEY: &str = "inference/llm.first_token";

/// ArtifactKey for `ResidencyFault` events. Published when
/// inference would have needed a not-resident page (per the
/// no-CPU-fallback contract from #1341). Subscribers:
/// sentinel-observer (learns to upgrade the missing page's
/// tier policy), audit-recorder (logs as GovernorOverride
/// audit entry — the fault represents the substrate refusing
/// to silently demote).
pub const RESIDENCY_FAULT_KEY: &str = "inference/llm.residency_fault";

// ─── Publishing helpers ─────────────────────────────────────────

/// Publish an `InferenceRequest` to the trace bus under the
/// canonical key. Async — uses `MessageBus::publish` (the path
/// that walks the artifact-subscription list I shipped in #1343).
///
/// Producers (persona-cognition) call this when they want the
/// inference engine to start generating. The InferenceLlmModule's
/// future bus subscription consumes; today (PR-2) the module is
/// command-driven and this publishing path is observer-only.
///
/// Serialization failures fall back to `Value::Null` rather than
/// panicking — the InferenceRequest shape is serde-derived and
/// known to serialize cleanly, so a failure here would indicate
/// substrate corruption. The trace bus still fires (with empty
/// payload) so subscribers see something happened.
pub async fn publish_inference_request(
    bus: &MessageBus,
    registry: &ModuleRegistry,
    request: &InferenceRequest,
) {
    let payload = serde_json::to_value(request).unwrap_or(serde_json::Value::Null);
    bus.publish(INFERENCE_REQUEST_KEY, payload, registry).await;
}

/// Publish an `InferenceComplete` to the trace bus. Same async
/// + serde semantics as `publish_inference_request`.
pub async fn publish_inference_complete(
    bus: &MessageBus,
    registry: &ModuleRegistry,
    complete: &InferenceComplete,
) {
    let payload = serde_json::to_value(complete).unwrap_or(serde_json::Value::Null);
    bus.publish(INFERENCE_COMPLETE_KEY, payload, registry).await;
}

/// Publish a `FirstTokenEmitted` event. The TTFT observability
/// signal — VDD harness binds to this for the time-to-first-token
/// latency budget.
pub async fn publish_first_token_emitted(
    bus: &MessageBus,
    registry: &ModuleRegistry,
    event: &FirstTokenEmitted,
) {
    let payload = serde_json::to_value(event).unwrap_or(serde_json::Value::Null);
    bus.publish(FIRST_TOKEN_EMITTED_KEY, payload, registry)
        .await;
}

/// Publish a `ResidencyFault` event. Sentinel-observer subscribes
/// to learn which pages to upgrade in tier policy; audit-recorder
/// subscribes for the GovernorOverride audit trail.
pub async fn publish_residency_fault(
    bus: &MessageBus,
    registry: &ModuleRegistry,
    fault: &ResidencyFault,
) {
    let payload = serde_json::to_value(fault).unwrap_or(serde_json::Value::Null);
    bus.publish(RESIDENCY_FAULT_KEY, payload, registry).await;
}

// ─── Subscriber convenience ─────────────────────────────────────

/// Wire a module to the three RESPONSE event types
/// (complete + first_token + residency_fault) via the
/// artifact-subscription path (#1343). Convenience for the most
/// common subscriber shape — observers that want to see what
/// inference does, not what's being requested.
///
/// Modules that want ALL FOUR events (incl. requests) subscribe
/// to that fourth key directly via `bus.subscribe_artifact` with
/// `INFERENCE_REQUEST_KEY`. Most observers don't need the requests;
/// the InferenceLlmModule already saw them via its command path.
pub fn subscribe_to_inference_responses(bus: &MessageBus, module_name: &'static str) {
    for selector in inference_response_selectors() {
        bus.subscribe_artifact(selector, module_name);
    }
}

/// Return the three response-event `ArtifactSelector::Exact`
/// entries. Useful for `ServiceModule::artifact_subscriptions()`
/// returns and for downstream callers that enumerate the
/// canonical observer surface.
pub fn inference_response_selectors() -> Vec<ArtifactSelector> {
    vec![
        ArtifactSelector::Exact(ArtifactKey::from(INFERENCE_COMPLETE_KEY)),
        ArtifactSelector::Exact(ArtifactKey::from(FIRST_TOKEN_EMITTED_KEY)),
        ArtifactSelector::Exact(ArtifactKey::from(RESIDENCY_FAULT_KEY)),
    ]
}

/// Return ALL FOUR inference event selectors (request + responses).
/// For the rare consumer that wants the full firehose (audit-
/// recorder may want this once it covers inference events).
pub fn all_inference_selectors() -> Vec<ArtifactSelector> {
    vec![
        ArtifactSelector::Exact(ArtifactKey::from(INFERENCE_REQUEST_KEY)),
        ArtifactSelector::Exact(ArtifactKey::from(INFERENCE_COMPLETE_KEY)),
        ArtifactSelector::Exact(ArtifactKey::from(FIRST_TOKEN_EMITTED_KEY)),
        ArtifactSelector::Exact(ArtifactKey::from(RESIDENCY_FAULT_KEY)),
    ]
}

#[cfg(test)]
mod tests {
    //! End-to-end tests: recording ServiceModule subscribes via the
    //! convenience helpers, the publishing helpers fire, the
    //! subscriber sees the right key + payload. Same shape as
    //! genome::bus tests (#1358).
    use super::*;
    use crate::genome::working_set::{ArtifactId, PageKind, PageOffset, PageRef, PersonaId};
    use crate::inference::llm_module::{
        CompositionPlan, FinishReason, GenerationBudget, InferenceRequestId, SamplingParams,
    };
    use crate::runtime::runtime::Runtime;
    use crate::runtime::service_module::{
        CommandResult, ModuleConfig, ModulePriority, ServiceModule,
    };
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use std::any::Any;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Recording module: subscribes to inference response keys,
    /// captures every (key, payload) pair.
    struct RecordingModule {
        name: &'static str,
        captured: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
        full_firehose: bool,
    }

    impl RecordingModule {
        fn new(
            name: &'static str,
            full_firehose: bool,
        ) -> (Arc<Self>, Arc<Mutex<Vec<(String, serde_json::Value)>>>) {
            let captured = Arc::new(Mutex::new(Vec::new()));
            let m = Arc::new(Self {
                name,
                captured: captured.clone(),
                full_firehose,
            });
            (m, captured)
        }
    }

    #[async_trait]
    impl ServiceModule for RecordingModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: self.name,
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _: &str,
            _: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }
        fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
            if self.full_firehose {
                all_inference_selectors()
            } else {
                inference_response_selectors()
            }
        }
        async fn on_artifact_available(
            &self,
            key: &ArtifactKey,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            self.captured
                .lock()
                .push((key.as_str().to_string(), payload));
            Ok(())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    fn sample_persona() -> PersonaId {
        PersonaId::new(Uuid::from_u128(1))
    }
    fn sample_request_id() -> InferenceRequestId {
        InferenceRequestId::new(Uuid::from_u128(42))
    }
    fn sample_request() -> InferenceRequest {
        InferenceRequest {
            request_id: sample_request_id(),
            persona: sample_persona(),
            composition: CompositionPlan(ArtifactId::new(Uuid::from_u128(100))),
            prompt_tokens: vec![1, 2, 3],
            prompt_text: None,
            budget: GenerationBudget {
                max_tokens: 100,
                max_duration_ms: 5000,
            },
            sampling: SamplingParams::default(),
            stop_sequences: vec![],
        }
    }
    fn sample_complete() -> InferenceComplete {
        InferenceComplete {
            request_id: sample_request_id(),
            persona: sample_persona(),
            completion_tokens: vec![10, 11],
            completion_text: None,
            finish_reason: FinishReason::Stop,
            elapsed_ms: 100,
            tokens_generated: 2,
        }
    }
    fn sample_first_token() -> FirstTokenEmitted {
        FirstTokenEmitted {
            request_id: sample_request_id(),
            persona: sample_persona(),
            elapsed_us: 5000,
        }
    }
    fn sample_fault() -> ResidencyFault {
        ResidencyFault {
            request_id: sample_request_id(),
            persona: sample_persona(),
            missing_page: PageRef {
                kind: PageKind::LoRALayer,
                artifact: ArtifactId::new(Uuid::from_u128(200)),
                offset: PageOffset::Whole,
            },
            reason: "page evicted mid-turn".to_string(),
        }
    }

    /// What this catches: every key string is canonical. Subscribers
    /// across modules reference these constants; if a future PR
    /// renames a string, this test pins what consumers see.
    #[test]
    fn keys_have_canonical_string_values() {
        assert_eq!(INFERENCE_REQUEST_KEY, "inference/llm.request");
        assert_eq!(INFERENCE_COMPLETE_KEY, "inference/llm.complete");
        assert_eq!(FIRST_TOKEN_EMITTED_KEY, "inference/llm.first_token");
        assert_eq!(RESIDENCY_FAULT_KEY, "inference/llm.residency_fault");
    }

    /// What this catches: inference_response_selectors covers
    /// exactly the three response event types as Exact. Adding a
    /// fourth response event would fail this test — forcing the
    /// author to verify the canonical observer surface.
    #[test]
    fn response_selectors_cover_three_keys_as_exact() {
        let selectors = inference_response_selectors();
        assert_eq!(selectors.len(), 3);
        let keys: Vec<String> = selectors
            .iter()
            .filter_map(|s| match s {
                ArtifactSelector::Exact(k) => Some(k.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&INFERENCE_COMPLETE_KEY.to_string()));
        assert!(keys.contains(&FIRST_TOKEN_EMITTED_KEY.to_string()));
        assert!(keys.contains(&RESIDENCY_FAULT_KEY.to_string()));
        // Request key NOT in the response set.
        assert!(!keys.contains(&INFERENCE_REQUEST_KEY.to_string()));
    }

    /// What this catches: all_inference_selectors includes the
    /// request key alongside the three responses. Full firehose
    /// for audit-recorder-style consumers.
    #[test]
    fn all_selectors_cover_four_keys() {
        let selectors = all_inference_selectors();
        assert_eq!(selectors.len(), 4);
        let keys: Vec<String> = selectors
            .iter()
            .filter_map(|s| match s {
                ArtifactSelector::Exact(k) => Some(k.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert!(keys.contains(&INFERENCE_REQUEST_KEY.to_string()));
        assert!(keys.contains(&INFERENCE_COMPLETE_KEY.to_string()));
        assert!(keys.contains(&FIRST_TOKEN_EMITTED_KEY.to_string()));
        assert!(keys.contains(&RESIDENCY_FAULT_KEY.to_string()));
    }

    /// What this catches: publish_inference_complete lands on
    /// INFERENCE_COMPLETE_KEY with the serialized payload. End-to-
    /// end test of the publish → dispatch → subscriber chain.
    #[tokio::test]
    async fn publish_inference_complete_routes_to_subscribed_module() {
        let runtime = Runtime::new();
        let (module, captured) = RecordingModule::new("recorder-complete", false);
        runtime.register(module);

        let c = sample_complete();
        publish_inference_complete(runtime.bus(), runtime.registry(), &c).await;

        let events = captured.lock().clone();
        let matched: Vec<_> = events
            .iter()
            .filter(|(k, _)| k == INFERENCE_COMPLETE_KEY)
            .collect();
        assert_eq!(matched.len(), 1);
        let back: InferenceComplete = serde_json::from_value(matched[0].1.clone()).unwrap();
        assert_eq!(back, c);
    }

    /// What this catches: each helper routes to its own key. A
    /// subscriber to one key doesn't see the others.
    #[tokio::test]
    async fn each_publish_helper_routes_to_its_own_key() {
        let runtime = Runtime::new();
        let (module, captured) = RecordingModule::new("recorder-each", false);
        runtime.register(module);

        publish_inference_complete(runtime.bus(), runtime.registry(), &sample_complete()).await;
        publish_first_token_emitted(runtime.bus(), runtime.registry(), &sample_first_token()).await;
        publish_residency_fault(runtime.bus(), runtime.registry(), &sample_fault()).await;

        let events = captured.lock().clone();
        let keys: Vec<String> = events.iter().map(|(k, _)| k.clone()).collect();
        assert!(keys.contains(&INFERENCE_COMPLETE_KEY.to_string()));
        assert!(keys.contains(&FIRST_TOKEN_EMITTED_KEY.to_string()));
        assert!(keys.contains(&RESIDENCY_FAULT_KEY.to_string()));
        assert_eq!(events.len(), 3);
    }

    /// What this catches: a response-only subscriber does NOT see
    /// the InferenceRequest event. Default observers (response set)
    /// don't get the noise of every request, just outcomes.
    #[tokio::test]
    async fn response_only_subscriber_does_not_see_requests() {
        let runtime = Runtime::new();
        let (module, captured) = RecordingModule::new("recorder-resp-only", false);
        runtime.register(module);

        publish_inference_request(runtime.bus(), runtime.registry(), &sample_request()).await;
        publish_inference_complete(runtime.bus(), runtime.registry(), &sample_complete()).await;

        let events = captured.lock().clone();
        // Only Complete delivered.
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, INFERENCE_COMPLETE_KEY);
    }

    /// What this catches: a full-firehose subscriber DOES see the
    /// InferenceRequest event. Audit-recorder-style consumers can
    /// log every request alongside completions for the prod-replay
    /// chain.
    #[tokio::test]
    async fn full_firehose_subscriber_sees_requests_too() {
        let runtime = Runtime::new();
        let (module, captured) = RecordingModule::new("recorder-firehose", true);
        runtime.register(module);

        publish_inference_request(runtime.bus(), runtime.registry(), &sample_request()).await;
        publish_inference_complete(runtime.bus(), runtime.registry(), &sample_complete()).await;

        let events = captured.lock().clone();
        let keys: Vec<String> = events.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(events.len(), 2);
        assert!(keys.contains(&INFERENCE_REQUEST_KEY.to_string()));
        assert!(keys.contains(&INFERENCE_COMPLETE_KEY.to_string()));
    }
}
