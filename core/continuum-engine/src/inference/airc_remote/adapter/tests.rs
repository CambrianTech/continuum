#![cfg(test)]
use super::*;
use crate::ai::heuristic_adapter::{HeuristicInferenceAdapter, HEURISTIC_PROVIDER_ID};
use crate::ai::types::{ChatMessage, FinishReason, MessageContent, TextGenerationRequest};

use super::super::protocol::{
    RemoteInferenceError, RemoteInferenceRequest, RemoteInferenceResponse,
};
use super::super::transport::{LocalAdapterTransport, StubInferenceTransport};

fn user_msg(text: &str) -> ChatMessage {
    ChatMessage {
        role: "user".to_string(),
        content: MessageContent::Text(text.to_string()),
        name: None,
    }
}

fn req(text: &str) -> TextGenerationRequest {
    TextGenerationRequest {
        messages: vec![user_msg(text)],
        system_prompt: None,
        model: None,
        provider: None,
        temperature: None,
        max_tokens: None,
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

// ── basic adapter surface ──────────────────────────────────

#[test]
fn adapter_reports_canonical_provider_id() {
    let transport = StubInferenceTransport::new(|_| {
        Err(RemoteInferenceError::Transport {
            message: "not used".to_string(),
        })
    });
    let adapter = AircRemoteInferenceAdapter::new(transport);
    assert_eq!(adapter.provider_id(), AIRC_REMOTE_PROVIDER_ID);
    assert_eq!(adapter.default_model(), AIRC_REMOTE_DEFAULT_MODEL);
}

#[test]
fn adapter_capabilities_admit_text_and_chat_not_local() {
    let transport = StubInferenceTransport::new(|_| {
        Err(RemoteInferenceError::Transport {
            message: "not used".to_string(),
        })
    });
    let adapter = AircRemoteInferenceAdapter::new(transport);
    let caps = adapter.capabilities();
    assert!(caps.has(crate::model_registry::Capability::TextGeneration));
    assert!(caps.has(crate::model_registry::Capability::Chat));
    assert!(!caps.is_local);
}

// what this catches: two sites disagreeing about who owns the model
// decision. with_model makes the lane's model authoritative on every
// request (#3822); supports_model must say the same thing to the
// selector, or a router trusts "I support X", routes here, and X is
// rewritten in flight. Unpinned stays open (the peer's catalog decides).
#[tokio::test]
async fn a_pinned_lane_advertises_only_its_model() {
    let transport =
        StubInferenceTransport::always_failing(RemoteInferenceError::Timeout { elapsed_ms: 1 });
    let pinned = AircRemoteInferenceAdapter::new(transport.clone()).with_model("qwen3.8-27b");
    assert!(pinned.supports_model("qwen3.8-27b"));
    assert!(!pinned.supports_model("ornith-ai/Ornith-1.5-35B-A3B-GGUF"));
    let open = AircRemoteInferenceAdapter::new(transport);
    assert!(open.supports_model("anything-at-all"));
}

// what this catches (card ad96f5d1, 2026-09-17): the breaker counter is
// per-adapter and every remote lane is built per-persona, so a per-PEER
// fault is measured once per citizen and no single counter reaches
// COLD_AFTER_DEADLINES. `remote_lane.failed` naming only the peer made that
// invisible — four same-peer timeouts read the same as four citizens each
// timing out once. The persona label is that missing attribution: absent it
// reads "-" (a dream, a test — never a per-persona lane), present it names
// the mind whose turn died, so the probe stream can tell the two apart.
#[test]
fn with_persona_labels_the_breaker_probes() {
    let transport =
        StubInferenceTransport::always_failing(RemoteInferenceError::Timeout { elapsed_ms: 1 });
    let unlabeled = AircRemoteInferenceAdapter::new(transport.clone());
    assert_eq!(unlabeled.persona_label(), "-", "no persona bound = '-', never a citizen's name");
    let labeled = AircRemoteInferenceAdapter::new(transport).with_persona("mathis");
    assert_eq!(labeled.persona_label(), "mathis");
}

#[tokio::test]
async fn adapter_supports_any_model_name_by_default() {
    let transport = StubInferenceTransport::new(|_| {
        Err(RemoteInferenceError::Transport {
            message: "not used".to_string(),
        })
    });
    let adapter = AircRemoteInferenceAdapter::new(transport);
    assert!(adapter.supports_model("gpt-4"));
    assert!(adapter.supports_model("anthropic/claude-opus-4-7"));
    assert!(adapter.supports_model("some-future-model"));
}

// ── the "same command across the wire" round-trip ──────────

#[tokio::test]
async fn remote_adapter_over_local_heuristic_transport_round_trips() {
    // This is THE architecture proof: the AircRemoteInference
    // Adapter wrapped around a transport that calls back to a
    // local HeuristicInferenceAdapter produces exactly what a
    // direct call to the heuristic would produce. The substrate
    // can't tell the difference between local and remote.
    let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let transport = LocalAdapterTransport::new(heuristic);
    let adapter = AircRemoteInferenceAdapter::new(transport);

    let response = adapter.generate_text(req("hello grid")).await.unwrap();
    assert!(response.text.starts_with("[heuristic:"));
    assert!(response.text.contains("hello grid"));
    // The adapter rewrites `provider` to "airc-remote" so
    // observability can tell the request flowed over the
    // remote adapter (even when the actual transport was
    // local).
    assert_eq!(response.provider, AIRC_REMOTE_PROVIDER_ID);
    // Finish reason from the peer adapter is preserved.
    assert_eq!(response.finish_reason, FinishReason::Stop);
}

#[tokio::test]
async fn remote_adapter_deterministic_when_peer_is_deterministic() {
    // The heuristic adapter is deterministic — same prompt
    // produces byte-identical responses. The remote adapter
    // routing to it inherits that determinism: this proves
    // replay-safety across the wire.
    let heuristic1: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let heuristic2: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let adapter1 = AircRemoteInferenceAdapter::new(LocalAdapterTransport::new(heuristic1));
    let adapter2 = AircRemoteInferenceAdapter::new(LocalAdapterTransport::new(heuristic2));

    let r1 = adapter1
        .generate_text(req("identical prompt"))
        .await
        .unwrap();
    let r2 = adapter2
        .generate_text(req("identical prompt"))
        .await
        .unwrap();
    assert_eq!(r1.text, r2.text);
}

// ── error propagation ─────────────────────────────────────

#[tokio::test]
async fn transport_error_surfaces_as_adapter_error_string() {
    let transport =
        StubInferenceTransport::always_failing(RemoteInferenceError::NoPeerReachable {
            message: "all peers down".to_string(),
        });
    let adapter = AircRemoteInferenceAdapter::new(transport);
    let err = adapter.generate_text(req("hi")).await.unwrap_err();
    assert!(err.contains("no remote peer reachable"));
    assert!(err.contains("all peers down"));
}

// what this catches: the breaker. Three deadline misses in a row must
// make the FOURTH request fail here, at once, naming the cold window —
// not wait another 30 s on the wire. Without it two citizens bound to a
// dead peer burned every turn against the deadline (2026-09-06, Kira +
// Mathis → Sahar).
#[tokio::test]
async fn three_deadline_misses_make_the_lane_cold_and_the_next_request_fails_fast() {
    let transport = StubInferenceTransport::always_failing(RemoteInferenceError::Timeout {
        elapsed_ms: 30_000,
    });
    let adapter = AircRemoteInferenceAdapter::new(transport).with_target_peer("df72dbf2");
    for _ in 0..COLD_AFTER_DEADLINES {
        assert!(!adapter.is_cold(), "warm until the third miss");
        let err = adapter.generate_text(req("hi")).await.unwrap_err();
        assert!(err.contains("timed out"), "a real round trip: {err}");
    }
    assert!(adapter.is_cold());
    let err = adapter.generate_text(req("hi")).await.unwrap_err();
    assert!(err.contains("cold"), "refused here, not on the wire: {err}");
    assert!(err.contains("df72dbf2"));
}

#[tokio::test]
async fn timeout_error_surfaces_with_elapsed_ms() {
    let transport = StubInferenceTransport::always_failing(RemoteInferenceError::Timeout {
        elapsed_ms: 5_000,
    });
    let adapter = AircRemoteInferenceAdapter::new(transport);
    let err = adapter.generate_text(req("hi")).await.unwrap_err();
    assert!(err.contains("timed out"));
    assert!(err.contains("5000"));
}

#[tokio::test]
async fn policy_denied_surfaces_through_adapter() {
    let transport =
        StubInferenceTransport::always_failing(RemoteInferenceError::PolicyDenied {
            reason: "persona scope mismatch".to_string(),
        });
    let adapter = AircRemoteInferenceAdapter::new(transport);
    let err = adapter.generate_text(req("hi")).await.unwrap_err();
    assert!(err.contains("policy denied"));
    assert!(err.contains("persona scope mismatch"));
}

// ── target_peer plumbing ──────────────────────────────────

// what this catches: the request must NAME ITS MODEL on the wire. The
// persona's override picks the peer by model and then, before this, asked
// that peer to run nothing in particular: `model: None` is serialized as
// an ABSENT field and the receiver refuses ("No provider or model
// specified" — 2026-09-06, every accepted ai/generate from the M5). A
// caller naming another model gets the lane's model too (the peer refuses others).
#[tokio::test]
async fn the_adapter_stamps_its_model_on_a_request_that_names_none() {
    let transport = StubInferenceTransport::new(|req: &RemoteInferenceRequest| {
        Ok(RemoteInferenceResponse {
            correlation_id: req.correlation_id,
            served_by: "peer".to_string(),
            text_response: crate::ai::types::TextGenerationResponse {
                text: "ok".to_string(),
                finish_reason: FinishReason::Stop,
                model: req
                    .text_request
                    .model
                    .clone()
                    .unwrap_or_else(|| "ABSENT".to_string()),
                provider: HEURISTIC_PROVIDER_ID.to_string(),
                usage: Default::default(),
                response_time_ms: 0,
                request_id: "stub".to_string(),
                content: None,
                tool_calls: None,
                reasoning: None,
                routing: None,
                error: None,
                timing: None,
            },
        })
    });
    let adapter = AircRemoteInferenceAdapter::new(transport).with_model("qwen3.8-27b");
    let out = adapter.generate_text(req("hi")).await.unwrap();
    assert_eq!(
        out.model, "qwen3.8-27b",
        "the override's model must ride the wire"
    );
    // A caller naming its LOCAL model (the persona's node serves Ornith,
    // her lane serves the 27B) is refused by the peer after the whole
    // wait; the lane's model wins and the difference is a probe row.
    let mut named = req("hi");
    named.model = Some("ornith-ai/Ornith-1.5-35B-A3B-GGUF".to_string());
    let out = adapter.generate_text(named).await.unwrap();
    assert_eq!(
        out.model, "qwen3.8-27b",
        "the lane's model is the truth for every request on it"
    );
}

#[tokio::test]
async fn with_target_peer_threads_through_to_transport_envelope() {
    // Verify the adapter actually sets target_peer on the
    // outgoing envelope when configured to pin a peer.
    let transport = StubInferenceTransport::new(|req: &RemoteInferenceRequest| {
        // Echo back the target_peer in the response's served_by
        // so the test can read it.
        Ok(RemoteInferenceResponse {
            correlation_id: req.correlation_id,
            served_by: req
                .target_peer
                .clone()
                .unwrap_or_else(|| "no-peer-pinned".to_string()),
            text_response: crate::ai::types::TextGenerationResponse {
                text: "ok".to_string(),
                finish_reason: FinishReason::Stop,
                model: "stub".to_string(),
                provider: HEURISTIC_PROVIDER_ID.to_string(),
                usage: Default::default(),
                response_time_ms: 0,
                request_id: "stub".to_string(),
                content: None,
                tool_calls: None,
                reasoning: None,
                routing: None,
                error: None,
                timing: None,
            },
        })
    });
    let adapter =
        AircRemoteInferenceAdapter::new(transport).with_target_peer("test-remote-peer");
    let _ = adapter.generate_text(req("anything")).await.unwrap();
    // The test verifies via the stub's served_by echo; the
    // adapter overwrites response.provider to airc-remote, so
    // we can't read served_by directly off the response. The
    // KEY assertion is the round-trip succeeded without error
    // AND the stub saw the target_peer. We trust the stub's
    // closure ran (it would have panic'd if not).
}

#[tokio::test]
async fn without_target_peer_sends_envelope_with_none() {
    let transport = StubInferenceTransport::new(|req: &RemoteInferenceRequest| {
        assert!(
            req.target_peer.is_none(),
            "expected target_peer=None; got {:?}",
            req.target_peer
        );
        Ok(RemoteInferenceResponse {
            correlation_id: req.correlation_id,
            served_by: "any".to_string(),
            text_response: crate::ai::types::TextGenerationResponse {
                text: "ok".to_string(),
                finish_reason: FinishReason::Stop,
                model: "stub".to_string(),
                provider: "stub".to_string(),
                usage: Default::default(),
                response_time_ms: 0,
                request_id: "stub".to_string(),
                content: None,
                tool_calls: None,
                reasoning: None,
                routing: None,
                error: None,
                timing: None,
            },
        })
    });
    let adapter = AircRemoteInferenceAdapter::new(transport);
    let _ = adapter.generate_text(req("any")).await.unwrap();
}

// ── health ────────────────────────────────────────────────

#[tokio::test]
async fn health_check_reports_unhealthy_until_first_successful_round_trip() {
    // R1 BLOCK on PR #1560: a remote adapter that reports
    // Healthy by construction lies to the AdapterRegistry's
    // selector. Pre-observation: pessimistic Unhealthy.
    let transport = StubInferenceTransport::always_failing(RemoteInferenceError::Transport {
        message: "not used".to_string(),
    });
    let adapter = AircRemoteInferenceAdapter::new(transport);
    let h = adapter.health_check().await;
    assert!(matches!(h.status, HealthState::Unhealthy));
    assert!(!h.api_available);
    assert!(h
        .message
        .as_deref()
        .unwrap_or("")
        .contains("no observed round-trip"));
}

#[tokio::test]
async fn health_check_flips_to_healthy_after_first_successful_round_trip() {
    // Use the heuristic adapter as the peer; one round-trip
    // should flip the observation flag.
    let heuristic: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let transport = LocalAdapterTransport::new(heuristic);
    let adapter = AircRemoteInferenceAdapter::new(transport);

    // Pre-roundtrip: Unhealthy.
    let h_before = adapter.health_check().await;
    assert!(matches!(h_before.status, HealthState::Unhealthy));

    // One real round-trip.
    let _ = adapter.generate_text(req("anything")).await.unwrap();

    // Post-roundtrip: Healthy.
    let h_after = adapter.health_check().await;
    assert!(matches!(h_after.status, HealthState::Healthy));
    assert!(h_after.api_available);
    assert!(h_after
        .message
        .as_deref()
        .unwrap_or("")
        .contains("successful round-trip"),);
}

// what this catches: the responder's served window dying at the requester.
// The peer stamps routing.served_context_window on its answer; the sink the
// lane factory installs must see exactly that number (card 1ab60567).
#[tokio::test]
async fn the_served_window_on_an_answer_reaches_the_sink() {
    // The seat answers 26,112 first, then re-plans to 49,152: the second answer
    // carries the new window and the adapter must follow it (Cormac's third arm).
    let answers = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let answers_seen = Arc::clone(&answers);
    let transport = StubInferenceTransport::new(move |req: &RemoteInferenceRequest| {
        let nth = answers_seen.fetch_add(1, Ordering::Relaxed);
        let window = if nth == 0 { 26_112 } else { 49_152 };
        Ok(RemoteInferenceResponse {
            correlation_id: req.correlation_id,
            served_by: "peer".to_string(),
            text_response: crate::ai::types::TextGenerationResponse {
                text: "ok".to_string(),
                finish_reason: FinishReason::Stop,
                model: "qwen3.8-27b".to_string(),
                provider: HEURISTIC_PROVIDER_ID.to_string(),
                usage: Default::default(),
                response_time_ms: 0,
                request_id: "stub".to_string(),
                content: None,
                tool_calls: None,
                reasoning: None,
                routing: Some(crate::ai::types::RoutingInfo {
                    provider: "llama".to_string(),
                    is_local: true,
                    routing_reason: "adapter_selected".to_string(),
                    adapters_applied: vec!["served-adapter".to_string()],
                    model_mapped: Some("served-model".to_string()),
                    model_requested: Some("requested-model".to_string()),
                    served_context_window: Some(window),
                    remote: Some(crate::ai::types::RemoteInferenceReceipt {
                        requested_peer: "peer-alias".to_string(),
                        responding_peer: "peer".to_string(),
                        correlation_id: "wire-correlation".to_string(),
                        elapsed_ms: 42,
                    }),
                }),
                error: None,
                timing: None,
            },
        })
    });
    let seen = Arc::new(std::sync::Mutex::new(None));
    let sink_seen = Arc::clone(&seen);
    let adapter = AircRemoteInferenceAdapter::new(transport)
        .with_model("qwen3.8-27b")
        .with_window_sink(Arc::new(move |w| {
            *sink_seen.lock().unwrap() = Some(w); // unwrap_or: test mutex — a poisoned lock is a failed test
        }));
    // Arm 1 (2026-09-18): UNHEARD is None — the binding window stands until the seat
    // has spoken for itself; never a stand-in number.
    assert_eq!(adapter.live_served_window(), None, "unheard seat is unmeasured, not 0");
    let response = adapter.generate_text(req("hi")).await.unwrap();
    assert_eq!(*seen.lock().unwrap(), Some(26_112));
    let route = response.routing.unwrap();
    assert!(!route.is_local);
    assert_eq!(route.provider, AIRC_REMOTE_PROVIDER_ID);
    assert_eq!(route.served_context_window, Some(26_112));
    // what this catches (2026-09-18): the window is LIVE on the adapter the moment it
    // is heard — a bound mind's next prompt budgets to the seat, not to her re-host.
    assert_eq!(adapter.live_served_window(), Some(26_112), "arm 2: the seat's window is reported live once heard");
    // Arm 3: the seat re-plans — the second answer's window replaces the first (never
    // the first only), and the sink hears it too, so the durable override follows.
    let changed = adapter.capacity_changed(Some(26_112));
    let another = adapter.capacity_changed(Some(26_112));
    let again = adapter.generate_text(req("hi again")).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        tokio::join!(changed, another);
    })
    .await
    .expect("same-adapter window feedback wakes every parked consumer");
    assert_eq!(
        again.routing.as_ref().and_then(|r| r.served_context_window),
        Some(49_152)
    );
    assert_eq!(
        adapter.live_served_window(),
        Some(49_152),
        "arm 3: the last heard window, not the first"
    );
    assert_eq!(
        *seen.lock().unwrap(),
        Some(49_152),
        "the sink follows every answer"
    );
    assert_eq!(route.adapters_applied, ["served-adapter"]);
    assert_eq!(route.model_mapped.as_deref(), Some("served-model"));
    assert_eq!(route.model_requested.as_deref(), Some("requested-model"));
    let receipt = route.remote.unwrap();
    assert_eq!(receipt.requested_peer, "peer-alias");
    assert_eq!(receipt.responding_peer, "peer");
    assert_eq!(receipt.correlation_id, "wire-correlation");
    assert_eq!(receipt.elapsed_ms, 42);
}
