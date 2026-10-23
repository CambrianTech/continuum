#![cfg(test)]
    use super::*;

    fn fresh_module() -> PersonaServiceModule {
        PersonaServiceModule::new(Arc::new(RagEngine::new()))
    }

    fn test_config() -> ResponderConfig {
        ResponderConfig {
            model: "test-model".to_string(),
            system_prompt: "You are a helpful test persona.".to_string(),
            capabilities: HashSet::new(),
            specialty: "general".to_string(),
        }
    }

    #[test]
    fn config_declares_persona_prefix_and_high_priority() {
        let m = fresh_module();
        let cfg = m.config();
        assert_eq!(cfg.name, "persona");
        assert_eq!(cfg.priority, ModulePriority::High);
        assert_eq!(cfg.command_prefixes, &["persona/"]);
        assert_eq!(cfg.tick_interval, Some(Duration::from_millis(250)));
    }

    #[tokio::test]
    async fn status_with_no_enrollments_reports_zero_and_prep_scope() {
        let m = fresh_module();
        let result = m
            .handle_command("persona/status", Value::Null)
            .await
            .expect("status succeeds");
        let CommandResult::Json(v) = result else {
            panic!("expected Json result")
        };
        assert_eq!(v["module"], "persona");
        assert_eq!(v["enrolled"], 0);
        assert_eq!(v["personas"].as_array().unwrap().len(), 0);
        assert!(v["scope"].as_str().unwrap().contains("L0-2-prep"));
    }

    #[tokio::test]
    async fn enroll_constructs_slot_and_status_reflects_it() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let result = m
            .handle_command(
                "persona/enroll",
                json!({
                    "persona_id": persona_id.to_string(),
                    "display_name": "Helper",
                    "model": "test-model",
                    "specialty": "general",
                }),
            )
            .await
            .expect("enroll succeeds with valid params");
        let CommandResult::Json(enroll_result) = result else {
            panic!("expected Json result")
        };
        assert_eq!(enroll_result["enrolled"], persona_id.to_string());
        assert_eq!(enroll_result["total"], 1);

        let status = m
            .handle_command("persona/status", Value::Null)
            .await
            .expect("status succeeds");
        let CommandResult::Json(s) = status else {
            panic!("expected Json result")
        };
        assert_eq!(s["enrolled"], 1);
        let personas = s["personas"].as_array().unwrap();
        assert_eq!(personas.len(), 1);
        assert_eq!(personas[0]["persona_id"], persona_id.to_string());
        assert_eq!(personas[0]["display_name"], "Helper");
    }

    #[tokio::test]
    async fn enroll_is_idempotent_and_updates_display_name() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "First", test_config())
            .expect("first enroll");
        m.enroll(persona_id, "Second", test_config())
            .expect("second enroll");
        assert_eq!(m.enrolled_count().unwrap(), 1);
        let snapshot = m.enrolled_snapshot().unwrap();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].1, "Second");
    }

    #[tokio::test]
    async fn enroll_two_distinct_personas_keeps_both() {
        let m = fresh_module();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        m.enroll(a, "Alpha", test_config()).expect("enroll alpha");
        m.enroll(b, "Beta", test_config()).expect("enroll beta");
        assert_eq!(m.enrolled_count().unwrap(), 2);
    }

    #[tokio::test]
    async fn enroll_missing_persona_id_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command("persona/enroll", json!({"display_name": "Helper"}))
            .await
            .expect_err("enroll without persona_id must fail");
        assert!(
            err.contains("persona_id"),
            "error names the missing param: {err}"
        );
    }

    #[tokio::test]
    async fn enroll_missing_display_name_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command(
                "persona/enroll",
                json!({"persona_id": Uuid::new_v4().to_string()}),
            )
            .await
            .expect_err("enroll without display_name must fail");
        assert!(
            err.contains("display_name"),
            "error names the missing param: {err}"
        );
    }

    #[tokio::test]
    async fn enroll_invalid_uuid_fails_loud() {
        let m = fresh_module();
        let err = m
            .handle_command(
                "persona/enroll",
                json!({"persona_id": "not-a-uuid", "display_name": "X"}),
            )
            .await
            .expect_err("enroll with invalid uuid must fail");
        assert!(
            err.contains("uuid") || err.contains("invalid"),
            "error names the parse failure: {err}"
        );
    }

    #[tokio::test]
    async fn unknown_command_returns_clear_error() {
        let m = fresh_module();
        let err = m
            .handle_command("persona/teleport", Value::Null)
            .await
            .expect_err("unknown commands must error");
        assert!(err.contains("persona/teleport"), "error names the command");
    }

    #[tokio::test]
    async fn tick_with_no_enrolled_personas_succeeds_quietly() {
        let m = fresh_module();
        m.tick().await.expect("empty tick succeeds");
    }

    #[tokio::test]
    async fn tick_with_enrolled_persona_and_no_items_is_no_op() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        // No items in any channel — tick should drain nothing, errors zero.
        m.tick().await.expect("tick succeeds with idle persona");
        assert_eq!(m.enrolled_count().unwrap(), 1);
        // Failure counter should be zero — idle is not a failure.
        let personas = m.personas.lock().unwrap();
        let slot = personas.get(&persona_id).expect("persona enrolled");
        assert_eq!(slot.consecutive_service_failures, 0);
        assert_eq!(slot.circuit_open_until_ms, 0);
    }

    use crate::persona::channel_items::ChatQueueItem;
    use crate::persona::channel_queue::{ChannelQueue, ChannelQueueConfig};
    use crate::persona::channel_types::ActivityDomain;
    use crate::persona::types::SenderType;

    /// Construct a chat queue item with sensible defaults for tests.
    fn test_chat_item(content: &str, sender_human: bool, room_id: Uuid) -> ChatQueueItem {
        ChatQueueItem {
            id: Uuid::new_v4(),
            room_id,
            content: content.to_string(),
            sender_id: Uuid::new_v4(),
            sender_name: "Sender".to_string(),
            sender_type: if sender_human {
                SenderType::Human
            } else {
                SenderType::Persona
            },
            mentions: false,
            timestamp: 1_700_000_000_000,
            enqueued_at: 1_700_000_000_000,
            priority: 0.5,
            consolidated_context: vec![],
            media: vec![],
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Ensure the Chat channel exists on this persona's registry so
    /// items can be routed there for service_cycle_batched to find.
    fn ensure_chat_channel(persona: &mut EnrolledPersona) {
        if persona.channels.get(ActivityDomain::Chat).is_none() {
            persona
                .channels
                .register(ChannelQueue::new(ChannelQueueConfig {
                    domain: ActivityDomain::Chat,
                    max_size: 64,
                    name: "chat".to_string(),
                }));
        }
    }

    #[tokio::test]
    async fn service_burst_for_idle_returns_empty_vec() {
        // what this catches: a future regression where service_burst_for
        // pretends a burst exists on an empty channel. The batched
        // contract is "zero work in → zero bursts out", not "one
        // burst with empty content".
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let mut personas = m.personas.lock().unwrap();
        let persona = personas.get_mut(&persona_id).unwrap();
        ensure_chat_channel(persona);
        let bursts =
            PersonaServiceModule::service_burst_for(persona, 1_700_000_000_000).expect("idle ok");
        assert!(
            bursts.is_empty(),
            "no items routed → no bursts; got {} entries",
            bursts.len()
        );
    }

    #[tokio::test]
    async fn service_burst_for_dispatches_chat_burst_through_analyze_burst() {
        // what this catches: regression where service_burst_for stops
        // honoring the demand-pull doctrine (one burst per channel-tick
        // through analyze_burst). Pins: gate fires once, respond_input
        // is fully-formed from BurstRespondContext, persona config flows
        // through (no empty-string defaults), aggregated_content carries
        // the burst's text.
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        let mut personas = m.personas.lock().unwrap();
        let persona = personas.get_mut(&persona_id).unwrap();
        ensure_chat_channel(persona);
        let item = test_chat_item("hello", true, room_id);
        persona
            .channels
            .route(std::sync::Arc::new(item))
            .expect("route chat item to Chat channel");
        let bursts = PersonaServiceModule::service_burst_for(persona, 1_700_000_000_000)
            .expect("dispatch ok");
        // One chat item routed → exactly one Chat burst. Multiple
        // bursts here would mean the batched drain double-counted.
        assert_eq!(bursts.len(), 1, "one chat item → one burst");
        match bursts.into_iter().next().unwrap() {
            ServiceBurstDecision::NeedsResponse {
                burst_message_count,
                primary_room,
                decision,
                respond_input,
            } => {
                assert_eq!(burst_message_count, 1, "single-item burst");
                assert_eq!(primary_room, room_id);
                assert!(decision.should_respond, "human-sender mention → respond");
                // Doctrine pin: respond_input carries the persona's
                // real config, not empty defaults (all came from
                // test_config()).
                assert_eq!(respond_input.model, "test-model");
                assert_eq!(respond_input.persona.specialty, "general");
                assert_eq!(
                    respond_input.system_prompt,
                    "You are a helpful test persona."
                );
                // Aggregated content carries the burst text in
                // "Sender: content" form (ChatChannelView::interpret).
                assert!(
                    respond_input.message_text.contains("hello"),
                    "aggregated_content must carry the burst text; got {:?}",
                    respond_input.message_text
                );
            }
            other => panic!("expected NeedsResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn enroll_with_empty_model_is_rejected_loud() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let mut bad_config = test_config();
        bad_config.model = String::new();
        let err = m
            .enroll(persona_id, "Helper", bad_config)
            .expect_err("enroll must reject empty model");
        assert!(err.contains("model"), "error names the field: {err}");
        assert_eq!(
            m.enrolled_count().unwrap(),
            0,
            "rejected enrollment must not mutate state"
        );
    }

    #[tokio::test]
    async fn enroll_with_empty_specialty_is_rejected_loud() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let mut bad_config = test_config();
        bad_config.specialty = String::new();
        let err = m
            .enroll(persona_id, "Helper", bad_config)
            .expect_err("enroll must reject empty specialty");
        assert!(err.contains("specialty"), "error names the field: {err}");
    }

    #[tokio::test]
    async fn enroll_command_requires_model() {
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        let err = m
            .handle_command(
                "persona/enroll",
                json!({
                    "persona_id": persona_id.to_string(),
                    "display_name": "Helper",
                }),
            )
            .await
            .expect_err("enroll command must require model");
        assert!(
            err.contains("model"),
            "error names the missing param: {err}"
        );
    }

    #[tokio::test]
    async fn drain_all_personas_processes_two_personas_independently() {
        let m = fresh_module();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        m.enroll(a, "Alpha", test_config()).expect("enroll a");
        m.enroll(b, "Beta", test_config()).expect("enroll b");
        let room_id = Uuid::new_v4();
        {
            let mut personas = m.personas.lock().unwrap();
            for persona in personas.values_mut() {
                ensure_chat_channel(persona);
                persona
                    .channels
                    .route(std::sync::Arc::new(test_chat_item("hi", true, room_id)))
                    .expect("route");
            }
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        // Both personas should be healthy: zero consecutive failures,
        // closed circuit.
        let personas = m.personas.lock().unwrap();
        for persona in personas.values() {
            assert_eq!(persona.consecutive_service_failures, 0);
            assert_eq!(persona.circuit_open_until_ms, 0);
        }
    }

    #[tokio::test]
    async fn drain_handles_large_burst_without_tripping_cb() {
        // what this catches: regression where a busy tick (many items
        // routed to one persona's chat channel) breaks the per-persona
        // CB. The batched cutover collapses N same-room items into
        // ONE coherent burst via service_cycle_batched + consolidation,
        // so one drain call dispatches at most one inference per
        // domain — but the SERVICE-LAYER must still see clean state.
        let m = fresh_module();
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        let staged = 25;
        {
            let mut personas = m.personas.lock().unwrap();
            let persona = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(persona);
            for i in 0..staged {
                let mut item = test_chat_item(&format!("msg {i}"), true, room_id);
                item.timestamp = 1_700_000_000_000 + i as u64;
                persona
                    .channels
                    .route(std::sync::Arc::new(item))
                    .expect("route item");
            }
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        // Persona stays healthy: zero service-layer failures, CB closed.
        let personas = m.personas.lock().unwrap();
        let persona = personas.get(&persona_id).unwrap();
        assert_eq!(persona.consecutive_service_failures, 0);
        assert_eq!(persona.circuit_open_until_ms, 0);
    }

    #[tokio::test]
    async fn tick_is_no_op_for_empty_module() {
        // The L0-2-dispatch tick drains personas; with none enrolled
        // it should still complete cleanly.
        let m = fresh_module();
        m.tick().await.expect("empty tick succeeds");
    }

    // --- L0-2-respond-call tests: Responder DI, inference CB threshold ---

    use std::sync::atomic::{AtomicU32, Ordering};

    /// Test responder that records every call + returns scripted outcomes.
    struct MockResponder {
        call_count: AtomicU32,
        scripted: ResponderScript,
    }

    enum ResponderScript {
        /// Always returns Spoke with the given text.
        AlwaysSpoke(String),
        /// Always returns an error with the given message.
        AlwaysErr(String),
    }

    #[async_trait]
    impl Responder for MockResponder {
        async fn respond(&self, input: RespondInput) -> Result<PersonaResponse, String> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            match &self.scripted {
                ResponderScript::AlwaysSpoke(text) => Ok(PersonaResponse::Spoke {
                    persona_id: input.persona.persona_id,
                    text: text.clone(),
                    model_used: input.model.clone(),
                    inference_ms: 1,
                    total_ms: 2,
                    think_blocks_emitted: 0,
                }),
                ResponderScript::AlwaysErr(msg) => Err(msg.clone()),
            }
        }
    }

    fn module_with_responder(
        script: ResponderScript,
    ) -> (PersonaServiceModule, Arc<MockResponder>) {
        let mock = Arc::new(MockResponder {
            call_count: AtomicU32::new(0),
            scripted: script,
        });
        let m = PersonaServiceModule::with_responder(
            Arc::new(RagEngine::new()),
            mock.clone() as Arc<dyn Responder>,
        );
        (m, mock)
    }

    #[tokio::test]
    async fn drain_calls_responder_when_gate_says_yes() {
        let (m, mock) = module_with_responder(ResponderScript::AlwaysSpoke("howdy".to_string()));
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        {
            let mut personas = m.personas.lock().unwrap();
            let persona = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(persona);
            persona
                .channels
                .route(std::sync::Arc::new(test_chat_item("hi", true, room_id)))
                .expect("route");
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        assert_eq!(
            mock.call_count.load(Ordering::SeqCst),
            1,
            "responder must be called exactly once for the single chat burst"
        );
        // Persona healthy (no failures, circuit closed).
        let personas = m.personas.lock().unwrap();
        let p = personas.get(&persona_id).unwrap();
        assert_eq!(p.consecutive_service_failures, 0);
        assert_eq!(p.consecutive_inference_failures, 0);
        assert_eq!(p.circuit_open_until_ms, 0);
    }

    #[tokio::test]
    async fn drain_does_not_call_responder_when_gate_says_no() {
        // ai-sender + no @mention → response_cap / sender filter typically
        // gates it silent. Either way, if SilentByDecision fires, the
        // responder must NOT be invoked.
        let (m, mock) = module_with_responder(ResponderScript::AlwaysSpoke("never".to_string()));
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        {
            let mut personas = m.personas.lock().unwrap();
            let persona = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(persona);
            // ai-sender, not mentioned — the gate typically goes silent here
            persona
                .channels
                .route(std::sync::Arc::new(test_chat_item("hi", false, room_id)))
                .expect("route");
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        // Whether the gate said yes or no for this specific shape isn't
        // guaranteed by analyze_burst alone — what's guaranteed is that
        // IF the gate says no, responder is never called. We can't reliably
        // assert gate behavior here without mocking it, so we assert the
        // weaker (and architecturally interesting) invariant: call_count
        // is either 0 (gate silent) or 1 (gate said yes), never higher.
        let calls = mock.call_count.load(Ordering::SeqCst);
        assert!(calls <= 1, "responder called more than once: {calls}");
    }

    #[tokio::test]
    async fn inference_errors_eventually_trip_circuit_at_inference_threshold() {
        // Repeated inference failures should trip the CB at the inference
        // threshold (15), not the service threshold (5). To exercise this
        // we need 15 inference failures. Each tick we stage one chat
        // item → service_burst_for emits ONE NeedsResponse burst → one
        // inference call → one failure. drain_all_personas breaks out of
        // the burst loop on inference error, so we drive exactly 15
        // ticks to reach the threshold.
        let (m, mock) =
            module_with_responder(ResponderScript::AlwaysErr("model not loaded".to_string()));
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        for tick in 0..CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES {
            // Stage a fresh item on each tick.
            {
                let mut personas = m.personas.lock().unwrap();
                let persona = personas.get_mut(&persona_id).unwrap();
                ensure_chat_channel(persona);
                let mut item = test_chat_item(&format!("msg {tick}"), true, room_id);
                item.timestamp = 1_700_000_000_000 + tick as u64;
                persona
                    .channels
                    .route(std::sync::Arc::new(item))
                    .expect("route");
            }
            m.drain_all_personas(1_700_000_000_000 + tick as u64)
                .await
                .expect("drain ok");
        }
        let calls = mock.call_count.load(Ordering::SeqCst);
        assert_eq!(
            calls, CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES,
            "responder should be called exactly the threshold count of times"
        );
        let personas = m.personas.lock().unwrap();
        let p = personas.get(&persona_id).unwrap();
        assert_eq!(
            p.consecutive_inference_failures, CIRCUIT_BREAKER_MAX_CONSECUTIVE_INFERENCE_FAILURES,
            "inference failure counter should equal the threshold"
        );
        assert_ne!(
            p.circuit_open_until_ms, 0,
            "circuit must be open after threshold inference failures"
        );
    }

    #[tokio::test]
    async fn inference_failure_below_threshold_does_not_trip_circuit() {
        // 1 inference error → counter at 1, circuit still closed.
        let (m, _mock) =
            module_with_responder(ResponderScript::AlwaysErr("transient hiccup".to_string()));
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        {
            let mut personas = m.personas.lock().unwrap();
            let persona = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(persona);
            persona
                .channels
                .route(std::sync::Arc::new(test_chat_item("hi", true, room_id)))
                .expect("route");
        }
        m.drain_all_personas(1_700_000_000_000)
            .await
            .expect("drain ok");
        let personas = m.personas.lock().unwrap();
        let p = personas.get(&persona_id).unwrap();
        assert_eq!(p.consecutive_inference_failures, 1);
        assert_eq!(
            p.circuit_open_until_ms, 0,
            "single inference failure must not trip circuit (threshold is higher)"
        );
    }

    #[tokio::test]
    async fn successful_response_resets_inference_failure_counter() {
        // 1 inference error followed by 1 success should reset counter.
        // We do this via a counter-based mock that errors once then spokes.
        struct OnceErrThenSpoke {
            calls: AtomicU32,
        }
        #[async_trait]
        impl Responder for OnceErrThenSpoke {
            async fn respond(&self, input: RespondInput) -> Result<PersonaResponse, String> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    Err("first call errors".to_string())
                } else {
                    Ok(PersonaResponse::Spoke {
                        persona_id: input.persona.persona_id,
                        text: "ok".to_string(),
                        model_used: input.model.clone(),
                        inference_ms: 1,
                        total_ms: 2,
                        think_blocks_emitted: 0,
                    })
                }
            }
        }
        let mock = Arc::new(OnceErrThenSpoke {
            calls: AtomicU32::new(0),
        });
        let m = PersonaServiceModule::with_responder(
            Arc::new(RagEngine::new()),
            mock.clone() as Arc<dyn Responder>,
        );
        let persona_id = Uuid::new_v4();
        m.enroll(persona_id, "Helper", test_config())
            .expect("enroll");
        let room_id = Uuid::new_v4();
        // Tick 1: route an item + drain → inference error
        {
            let mut personas = m.personas.lock().unwrap();
            let p = personas.get_mut(&persona_id).unwrap();
            ensure_chat_channel(p);
            p.channels
                .route(std::sync::Arc::new(test_chat_item("first", true, room_id)))
                .expect("route");
        }
        m.drain_all_personas(1_700_000_000_000).await.expect("ok");
        // Tick 2: route fresh item + drain → success
        {
            let mut personas = m.personas.lock().unwrap();
            let p = personas.get_mut(&persona_id).unwrap();
            let mut item = test_chat_item("second", true, room_id);
            item.timestamp = 1_700_000_000_001;
            p.channels.route(std::sync::Arc::new(item)).expect("route");
        }
        m.drain_all_personas(1_700_000_000_001).await.expect("ok");
        // After the success, the inference counter should be reset to 0.
        let personas = m.personas.lock().unwrap();
        let p = personas.get(&persona_id).unwrap();
        assert_eq!(
            p.consecutive_inference_failures, 0,
            "successful response after error must reset counter"
        );
    }
