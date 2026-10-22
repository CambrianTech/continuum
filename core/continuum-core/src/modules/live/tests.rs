#![cfg(test)]
    use super::*;
    use uuid::Uuid;

    fn module() -> VoiceModule {
        VoiceModule::new(Arc::new(VoiceState::new(
            Arc::new(VoiceService::new()),
            Arc::new(LiveKitAgentManager::new()),
            Arc::new(crate::live::transport::call_server::CallManager::new()),
            Arc::new(AudioBufferPool::new()),
        )))
    }

    /// Register a session with one Persona participant; returns the JSON result.
    /// The bridge socket doesn't exist under test — the spawned listener/agent
    /// tasks fail loud into logs without affecting the registration state under test.
    async fn register(m: &VoiceModule, session_id: &str, room_id: &str, persona: Uuid) -> Value {
        let result = m
            .handle_command(
                "voice/register-session",
                serde_json::json!({
                    "session_id": session_id,
                    "room_id": room_id,
                    "participants": [{
                        "user_id": persona,
                        "display_name": "Test Persona",
                        "participant_type": "persona",
                        "expertise": [],
                    }],
                }),
            )
            .await
            .expect("register-session");
        match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json result, got {other:?}"),
        }
    }

    // what this catches: #193 slice A — with session_id == room_id (the slice-B client
    // contract) the call registers under the room id with NO legacy alias recorded and
    // the resolver is the identity. Regresses if the alias map ever grows on the
    // canonical path.
    #[tokio::test]
    async fn call_registers_under_room_id_with_no_alias_when_ids_agree() {
        let m = module();
        let room = Uuid::new_v4().to_string();
        let v = register(&m, &room, &room, Uuid::new_v4()).await;

        assert_eq!(v["registered"], true);
        assert_eq!(v["call_id"], room);
        assert!(m.state.active_sessions.lock().unwrap().contains(&room));
        assert!(
            m.state
                .legacy_call_aliases
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty(),
            "canonical registration must not record an alias"
        );
        assert_eq!(m.state.canonical_call_id(&room), room);
    }

    // what this catches: #193 slice A — a divergent client-minted session_id registers
    // the call UNDER the airc room id (server authoritative), records a legacy alias,
    // and a subsequent verb addressed by the LEGACY id (on-utterance, then end-session)
    // resolves to the SAME session. Regresses if any verb grows per-site id logic
    // instead of the one `canonical_call_id` resolver.
    #[tokio::test]
    async fn divergent_session_id_is_aliased_and_legacy_verbs_resolve_to_the_room() {
        let m = module();
        let legacy = Uuid::new_v4().to_string();
        let room = Uuid::new_v4().to_string();
        let persona = Uuid::new_v4();
        let v = register(&m, &legacy, &room, persona).await;
        assert_eq!(v["call_id"], room);

        // Registered under the ROOM id, never the legacy id.
        {
            let sessions = m.state.active_sessions.lock().unwrap();
            assert!(sessions.contains(&room));
            assert!(!sessions.contains(&legacy));
        }
        assert_eq!(m.state.canonical_call_id(&legacy), room);

        // on-utterance addressed by the LEGACY id reaches the room-keyed session:
        // the orchestrator returns the registered persona as responder.
        let result = m
            .handle_command(
                "voice/on-utterance",
                serde_json::json!({
                    "event": {
                        "session_id": legacy,
                        "speaker_id": Uuid::new_v4(),
                        "speaker_name": "Operator",
                        "speaker_type": "human",
                        "transcript": "hello there",
                        "confidence": 1.0,
                        "timestamp": 0,
                    }
                }),
            )
            .await
            .expect("on-utterance");
        let CommandResult::Json(v) = result else {
            panic!("expected Json result");
        };
        let responders: Vec<&str> = v["responder_ids"]
            .as_array()
            .expect("responder_ids array")
            .iter()
            .map(|x| x.as_str().expect("uuid string"))
            .collect();
        assert_eq!(responders, vec![persona.to_string().as_str()]);

        // end-session by the LEGACY id tears down the canonical call and drops the alias.
        m.handle_command(
            "voice/end-session",
            serde_json::json!({ "session_id": legacy }),
        )
        .await
        .expect("end-session");
        assert!(!m.state.active_sessions.lock().unwrap().contains(&room));
        assert!(m
            .state
            .legacy_call_aliases
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty());
    }

    // what this catches: #193 slice A — the browser-refresh re-join (fresh legacy
    // session_id, same room) and a canonical re-register both dedupe on the CANONICAL
    // id: one active call, no duplicate listener/agent spawns, and the fresh legacy id
    // still resolves. Regresses if the idempotency guard keys on the raw session_id.
    #[tokio::test]
    async fn re_register_with_either_id_dedupes_on_the_canonical_id() {
        let m = module();
        let room = Uuid::new_v4().to_string();
        let persona = Uuid::new_v4();
        register(&m, &room, &room, persona).await;

        // Refresh path: new client-minted id, same room → deduped AND aliased.
        let legacy = Uuid::new_v4().to_string();
        let v = register(&m, &legacy, &room, persona).await;
        assert_eq!(v["already_active"], true);
        assert_eq!(v["call_id"], room);
        assert_eq!(m.state.canonical_call_id(&legacy), room);

        // Canonical re-register → deduped.
        let v = register(&m, &room, &room, persona).await;
        assert_eq!(v["already_active"], true);
        assert_eq!(m.state.active_sessions.lock().unwrap().len(), 1);
    }
