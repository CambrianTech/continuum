#![cfg(test)]
    use super::*;
    use continuum_base::time::EpochMs;
    use crate::persona::message_cache::{CachedMessage, SenderCategory};
    use crate::rag::RagEngine;
    use std::sync::Arc;
    use tokio::sync::watch;

    fn test_engine(name: &str) -> (PersonaCognitionEngine, Uuid) {
        let rag_engine = Arc::new(RagEngine::new());
        let (_tx, rx) = watch::channel(false);
        let id = Uuid::new_v4();
        (
            PersonaCognitionEngine::new(id, name.into(), rag_engine, rx),
            id,
        )
    }

    fn test_request(persona_id: Uuid, persona_name: &str) -> FullEvaluateRequest {
        FullEvaluateRequest {
            persona_id,
            persona_name: persona_name.into(),
            persona_unique_id: "test-bot".into(),
            message_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-user".into(),
            sender_type: SenderType::Human,
            content: "Hello everyone".into(),
            timestamp: EpochMs::now().millis(),
            is_voice: false,
            voice_session_id: None,
            sender_is_human: true,
            topic_similarity: None,
            recent_room_texts: None,
        }
    }

    #[test]
    fn test_response_count_is_signal_not_gate() {
        // Formerly test_gate_1_response_cap — asserted that hitting the cap
        // produced a hard gate=response_cap SILENT. That gate was removed:
        // local personas have no resource exhaustion, and cloud rate limits
        // belong at the provider layer. Response count is still collected as
        // a social signal so the LLM can choose to self-quiet, but it never
        // short-circuits the evaluator.
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let mut rate_limiter = RateLimiterState::new(10.0, 3);

        let room_id = request.room_id;
        let now = EpochMs::now().millis();
        rate_limiter.track_response(room_id, now - 30_000);
        rate_limiter.track_response(room_id, now - 20_000);
        rate_limiter.track_response(room_id, now - 11_000); // 11s ago — well past rate-limit window

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now,
        );
        // Gate MUST NOT be response_cap anymore.
        assert_ne!(
            result.gate, "response_cap",
            "response_cap was removed — count is a social signal now, not a hard gate"
        );
        // But the count MUST still flow into social_signals so the LLM can see it.
        let sigs = result
            .social_signals
            .expect("social_signals always populated");
        assert_eq!(sigs.response_count_this_session, Some(3));
        assert_eq!(sigs.response_cap, Some(3));
    }

    #[test]
    fn test_rate_limit_is_signal_not_gate() {
        // Rate limiting is now a SIGNAL, not a hard gate.
        // The LLM decides whether to respond — Rust just reports the info.
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let mut rate_limiter = RateLimiterState::new(10.0, 50);

        let now = EpochMs::now().millis();
        // Response 5 seconds ago — within 10s window
        rate_limiter.track_response(request.room_id, now - 5_000);

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now,
        );
        // NOT blocked — rate info passed as signal
        assert!(
            result.should_respond,
            "Rate limit should be a signal, not a veto"
        );
        // But the social signals should contain the rate info
        let signals = result.social_signals.unwrap();
        assert!(signals.seconds_since_last_response.unwrap() < 10.0);
        assert_eq!(signals.response_count_this_session, Some(1));
        // Gate details should also have rate_limit_wait_seconds
        let details = result.gate_details.unwrap();
        assert!(details.rate_limit_wait_seconds.unwrap() > 0.0);
    }

    #[test]
    fn test_gate_4_sleep_mode_sleeping() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState {
            mode: SleepMode::Sleeping,
            reason: "Taking a break".into(),
            set_at_ms: EpochMs::now().millis() - 60_000,
            wake_at_ms: None,
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        assert!(!result.should_respond);
        assert_eq!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_gate_4_sleep_mentioned_only_passes_when_mentioned() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.content = "@TestBot can you help?".into();
        let sleep = SleepState {
            mode: SleepMode::MentionedOnly,
            reason: "Focus mode".into(),
            set_at_ms: EpochMs::now().millis() - 60_000,
            wake_at_ms: None,
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        // Should pass sleep gate (mentioned) and reach fast_path
        assert!(result.should_respond);
        assert_ne!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_gate_4_sleep_auto_wake() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let now = EpochMs::now().millis();
        let sleep = SleepState {
            mode: SleepMode::Sleeping,
            reason: "Nap time".into(),
            set_at_ms: now - 3_600_000,    // 1 hour ago
            wake_at_ms: Some(now - 1_000), // Wake time already passed
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            now,
        );
        // Should NOT be blocked by sleep — auto-wake expired
        assert_ne!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_directed_mention_is_signal_not_gate() {
        // Directed mentions are now a SIGNAL, not a hard gate.
        // The LLM sees "message directed at @OtherBot" and decides for itself.
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        // Mentions someone else, NOT TestBot
        request.content = "@OtherBot please fix this bug".into();
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        // NOT hard-blocked — the LLM will see the signal and decide
        let signals = result.social_signals.unwrap();
        assert!(!signals.is_mentioned, "TestBot is NOT mentioned");
        assert!(
            signals.has_directed_mention,
            "@OtherBot IS a directed mention"
        );
        // The fast_path may still block (AI sender low priority) but NOT because of directed mention gate
        assert_ne!(
            result.gate, "directed_mention",
            "directed_mention should not be a gate anymore"
        );
    }

    #[test]
    fn test_gate_6_fast_path_self_message() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_id = persona_id; // Self-message
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        assert!(!result.should_respond);
        assert_eq!(result.gate, "fast_path");
        assert!(result.reason.contains("Own message"));
    }

    #[test]
    fn test_gate_6_fast_path_human_high_priority() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        // Human sender + recent message = high priority → should respond
        assert!(result.should_respond);
    }

    #[test]
    fn test_non_human_echo_storm_blocks_undirected_agent_chatter() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_type = SenderType::Agent;
        request.sender_is_human = false;
        request.sender_name = "airc-bridge".into();
        request.content = "[airc:mac-claude] please respond if you see this".into();

        let now = EpochMs::now().millis();
        let mut cache = RecentMessageCache::new();
        for i in 0..2 {
            cache.push(
                request.room_id,
                CachedMessage {
                    id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    sender_type: SenderCategory::AI,
                    sender_name: format!("Persona{i}"),
                    content_text: "Hello! How can I assist you today?".into(),
                    timestamp_ms: now - 1_000,
                },
            );
        }

        let result = full_evaluate(
            &request,
            &RateLimiterState::default(),
            &SleepState::default(),
            &engine,
            &cache,
            now,
        );

        assert!(!result.should_respond);
        assert_eq!(result.gate, "non_human_echo_storm");
    }

    #[test]
    fn test_undirected_persona_message_completes_turn_without_cache_warmup() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_type = SenderType::Persona;
        request.sender_is_human = false;
        request.sender_name = "Teacher AI".into();
        request.content = "Teacher AI: Yes, I can see this startup smoke test.".into();

        let result = full_evaluate(
            &request,
            &RateLimiterState::default(),
            &SleepState::default(),
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );

        assert!(!result.should_respond);
        assert_eq!(result.gate, "persona_turn_complete");
    }

    #[test]
    fn test_non_human_echo_storm_allows_direct_mentions() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.sender_type = SenderType::Agent;
        request.sender_is_human = false;
        request.sender_name = "airc-bridge".into();
        request.content = "@TestBot please respond if you see this".into();

        let now = EpochMs::now().millis();
        let mut cache = RecentMessageCache::new();
        for i in 0..5 {
            cache.push(
                request.room_id,
                CachedMessage {
                    id: Uuid::new_v4(),
                    sender_id: Uuid::new_v4(),
                    sender_type: SenderCategory::AI,
                    sender_name: format!("Persona{i}"),
                    content_text: "Hello! How can I assist you today?".into(),
                    timestamp_ms: now - 1_000,
                },
            );
        }

        let result = full_evaluate(
            &request,
            &RateLimiterState::default(),
            &SleepState::default(),
            &engine,
            &cache,
            now,
        );

        assert_ne!(result.gate, "non_human_echo_storm");
        assert!(result.social_signals.unwrap().is_mentioned);
    }

    #[test]
    fn test_gate_6_fast_path_mentioned_always_responds() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        request.content = "@TestBot what do you think?".into();
        request.sender_type = SenderType::Persona; // AI sender (normally lower priority)
        request.sender_is_human = false;
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        assert!(result.should_respond);
    }

    #[test]
    fn test_all_gates_pass_normal_message() {
        let (engine, persona_id) = test_engine("TestBot");
        let request = test_request(persona_id, "TestBot");
        let sleep = SleepState::default();
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        assert!(result.should_respond);
        // The wall-clock assertion that used to live here (`decision_time_ms < 10.0`)
        // is GONE, and deliberately not replaced with a looser bound. It failed CI at
        // 10.73ms — not because the gates regressed, but because a shared runner was
        // busy. A correctness test that a loaded machine can fail is not measuring the
        // code; it is measuring the machine, and it spends the reviewer's trust every
        // time it flakes. The performance claim it was making is real and worth
        // keeping, so it MOVED to the stress block below, where `cargo test` does not
        // adjudicate it (CLAUDE.md § test rules, item 2). Same treatment as the
        // grounding-cost flake in PR #2330. Siblings of this class still exist and are
        // NOT touched here because they are not failing and have 20-50x more headroom:
        // command_executor.rs (<500ms), rag/engine.rs (<250ms), sentinel/parallel.rs
        // (<180ms). If any of them starts flaking, this is the fix.
    }

    #[test]
    fn test_gate_4_until_topic_with_provided_similarity() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        // High similarity → continuation → should NOT respond in until_topic mode
        request.topic_similarity = Some(0.8);
        let sleep = SleepState {
            mode: SleepMode::UntilTopic,
            reason: "Waiting for new topic".into(),
            set_at_ms: EpochMs::now().millis() - 60_000,
            wake_at_ms: None,
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        assert!(!result.should_respond);
        assert_eq!(result.gate, "sleep_mode");
    }

    #[test]
    fn test_gate_4_until_topic_new_topic_passes() {
        let (engine, persona_id) = test_engine("TestBot");
        let mut request = test_request(persona_id, "TestBot");
        // Low similarity → new topic → should respond
        request.topic_similarity = Some(0.1);
        let sleep = SleepState {
            mode: SleepMode::UntilTopic,
            reason: "Waiting for new topic".into(),
            set_at_ms: EpochMs::now().millis() - 60_000,
            wake_at_ms: None,
        };
        let rate_limiter = RateLimiterState::default();

        let result = full_evaluate(
            &request,
            &rate_limiter,
            &sleep,
            &engine,
            &RecentMessageCache::new(),
            EpochMs::now().millis(),
        );
        // Should pass sleep gate (new topic) and reach fast_path
        assert_ne!(result.gate, "sleep_mode");
    }

    // RateLimiterState unit tests + the post-inference adequacy tests
    // moved to their respective submodules in continuum#1208:
    //   - rate_limiter::tests
    //   - adequacy::tests

    /// Performance claims about the gate path. Compile-time gated so a busy shared
    /// runner never adjudicates them (CLAUDE.md § test rules, item 2): default
    /// `cargo test` skips this block entirely, and it is run deliberately, on a quiet
    /// machine, when the claim is what you actually want to check.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        // what this catches: the gate path taking a slow route — an added I/O call,
        // a lock, an inference hop. Every gate is a pure function over in-memory
        // state, so the decision is sub-millisecond work; a budget of 10ms is ~10x
        // headroom over that and still catches a category change. Measured over
        // repeated runs rather than one sample, because a single timing of a live
        // system is not a fact about it — one scheduler hiccup is not a regression.
        #[test]
        fn gate_path_stays_off_the_slow_route() {
            let (engine, persona_id) = test_engine("TestBot");
            let request = test_request(persona_id, "TestBot");
            let sleep = SleepState::default();
            let rate_limiter = RateLimiterState::default();

            const RUNS: usize = 50;
            let mut times: Vec<f64> = (0..RUNS)
                .map(|_| {
                    full_evaluate(
                        &request,
                        &rate_limiter,
                        &sleep,
                        &engine,
                        &RecentMessageCache::new(),
                        EpochMs::now().millis(),
                    )
                    .decision_time_ms
                })
                .collect();
            times.sort_by(|a, b| a.partial_cmp(b).expect("decision times are finite"));
            let median = times[RUNS / 2];
            assert!(
                median < 10.0,
                "median gate decision over {RUNS} runs should be <10ms, was {median}ms \
                 (slowest {}ms) — something on the gate path is doing real work",
                times[RUNS - 1]
            );
        }
    }
