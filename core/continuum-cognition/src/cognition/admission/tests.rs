#![cfg(test)]
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    const FIXED_NOW_MS: u64 = 1_715_625_600_000;

    // ── test doubles for the lookup oracles ─────────────────────────────

    #[derive(Default)]
    struct InMemoryContent(Mutex<HashMap<String, Uuid>>);

    impl SeenContentLookup for InMemoryContent {
        fn find_by_content_hash(&self, hash: &str) -> Option<Uuid> {
            self.0.lock().unwrap().get(hash).copied()
        }
    }

    #[derive(Default)]
    struct InMemoryEvents(Mutex<HashMap<String, u64>>);

    impl SeenEventLookup for InMemoryEvents {
        fn first_seen_ms(&self, event_id: &str) -> Option<u64> {
            self.0.lock().unwrap().get(event_id).copied()
        }
    }

    fn airc_ref(message_id: &str, sig: &str, hash: &str, schema: &str) -> AircMessageRef {
        AircMessageRef {
            transport: "airc".to_string(),
            room_id: "cambriantech".to_string(),
            message_id: message_id.to_string(),
            sender_id: "airc-8a5e".to_string(),
            sent_at_ms: FIXED_NOW_MS,
            received_at_ms: FIXED_NOW_MS,
            content_hash: hash.to_string(),
            signature: sig.to_string(),
            proof_refs: vec![],
            schema_version: schema.to_string(),
            client_name: Some("airc-bash".to_string()),
        }
    }

    fn candidate(content: &str, trust: TrustState, origin: EngramOrigin) -> AdmissionCandidate {
        AdmissionCandidate {
            content: content.to_string(),
            kind: EngramKind::Episodic,
            origin,
            trust_state: trust,
            recall_keys: vec!["test".to_string()],
            content_hash: format!("sha256:fake-{}", content.len()),
        }
    }

    fn airc_candidate(content: &str, trust: TrustState, message_id: &str) -> AdmissionCandidate {
        candidate(
            content,
            trust,
            EngramOrigin::Airc(airc_ref(message_id, "sig", "hash", "v1")),
        )
    }

    fn permissive_ctx<'a>(
        config: &'a AdmissionConfig,
        content: &'a InMemoryContent,
        events: &'a InMemoryEvents,
    ) -> AdmissionContext<'a> {
        AdmissionContext {
            context_id: None,
            config,
            now_ms: FIXED_NOW_MS,
            seen_content: content,
            seen_events: events,
        }
    }

    // ── context (room) carried onto the engram ──────────────────────────

    // what this catches: an admitted engram carries its ROOM (contextId) from
    // the admission context, so a persona's memory is keyed to the conversation
    // it happened in (within the persona's identity), not contextless. Regression
    // here = engrams lose their room and per-room recall can't scope. See
    // docs/architecture/IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md Part A.
    #[test]
    fn admitted_engram_carries_its_room_context() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let room = Uuid::new_v4();
        let ctx = permissive_ctx(&cfg, &content, &events).with_context(room);
        let cand = candidate(
            "remember this",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-ctx", "sig", "hash", "v1")),
        );
        let engram = build_engram_from_candidate(&cand, &ctx);
        assert_eq!(
            engram.context_id,
            Some(room),
            "engram must carry the room it was admitted in"
        );
    }

    // ── envelope verification ───────────────────────────────────────────

    /// What this catches: empty signature on an Airc envelope is a
    /// structural failure, not a recipe-policy decision. Returns
    /// `EnvelopeVerificationFailed`, not `Drop` — the gate must fail
    /// loud rather than silently rejecting.
    #[test]
    fn empty_signature_returns_envelope_verification_failed() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "interesting",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-1", "", "hash", "v1")),
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        );
        match result {
            Err(AdmissionError::EnvelopeVerificationFailed { detail }) => {
                assert!(detail.contains("signature"), "detail: {detail}");
            }
            other => panic!("expected EnvelopeVerificationFailed, got {other:?}"),
        }
        // Seam recorded even on error — forensics need it.
        assert_eq!(trace.seam_count(), 1);
        assert_eq!(trace.last_seam_name(), Some(SEAM_ADMISSION));
    }

    /// What this catches: empty content_hash on an Airc envelope is a
    /// structural failure (the gate needs the hash for tamper detection
    /// + dedup). Symmetric with the empty-signature test; same failure
    /// class returned via `EnvelopeVerificationFailed`. Asymmetric
    /// coverage between empty-signature/empty-content-hash/empty-schema
    /// would let one of the three regress silently.
    #[test]
    fn empty_content_hash_returns_envelope_verification_failed() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "perfectly novel content of sufficient length",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-x", "sig", "", "v1")),
        );

        match AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        ) {
            Err(AdmissionError::EnvelopeVerificationFailed { detail }) => {
                assert!(detail.contains("content_hash"), "detail: {detail}");
            }
            other => panic!("expected EnvelopeVerificationFailed, got {other:?}"),
        }
        assert_eq!(trace.seam_count(), 1);
    }

    /// What this catches: empty schema_version is structurally invalid
    /// (admission can't reason about a schema with no name). Distinct
    /// from `UnsupportedSchemaVersion` which fires for unknown values
    /// — empty is its own class returned via `EnvelopeVerificationFailed`.
    /// Symmetric coverage with empty-signature/empty-content-hash.
    #[test]
    fn empty_schema_version_returns_envelope_verification_failed() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "perfectly novel content of sufficient length",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-x", "sig", "hash", "")),
        );

        match AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        ) {
            Err(AdmissionError::EnvelopeVerificationFailed { detail }) => {
                assert!(detail.contains("schema_version"), "detail: {detail}");
            }
            other => panic!("expected EnvelopeVerificationFailed, got {other:?}"),
        }
        assert_eq!(trace.seam_count(), 1);
    }

    /// What this catches: unsupported schema_version returns
    /// `UnsupportedSchemaVersion`, not silent acceptance. Forward-
    /// compatibility hinge: if a sender claims schema v2 we want to fail
    /// loudly until the v2 admission code is shipped.
    #[test]
    fn unknown_schema_version_returns_unsupported_schema_version() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "novel content of sufficient length to be memorable",
            TrustState::ApprovedPeer,
            EngramOrigin::Airc(airc_ref("msg-x", "sig", "hash", "v2")),
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        );
        match result {
            Err(AdmissionError::UnsupportedSchemaVersion { schema_version }) => {
                assert_eq!(schema_version, "v2");
            }
            other => panic!("expected UnsupportedSchemaVersion, got {other:?}"),
        }
    }

    /// What this catches: local-trust origins (chat / tool / self-reflection)
    /// don't carry signed envelopes, so the structural envelope check
    /// must pass-through rather than treating "no signature" as failure.
    /// Otherwise admission of any internal-cognition engram would be
    /// impossible.
    #[test]
    fn self_reflection_origin_passes_envelope_structure() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = AdmissionContext {
            context_id: None,
            config: &cfg,
            now_ms: FIXED_NOW_MS,
            seen_content: &content,
            seen_events: &events,
        };
        let mut trace = CognitionTrace::new();

        let parent = Uuid::new_v4();
        let cand = candidate(
            "reflection on a prior engram which is sufficiently long",
            TrustState::SelfTrust,
            EngramOrigin::SelfReflection {
                parent_engram_id: parent,
            },
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .expect("self-reflection should pass structural checks");
        match result {
            AdmissionDecision::Admit { engram, .. } => {
                assert_eq!(engram.trust_state_at_admission, TrustState::SelfTrust);
                if let EngramOrigin::SelfReflection { parent_engram_id } = engram.origin {
                    assert_eq!(parent_engram_id, parent);
                } else {
                    panic!("origin should round-trip as SelfReflection");
                }
            }
            other => panic!("expected Admit, got {other:?}"),
        }
    }

    // ── trust threshold ─────────────────────────────────────────────────

    /// What this catches: trust below the configured threshold returns
    /// `TrustBoundaryRejected` BEFORE the recipe is consulted. A strict
    /// gate must not let unauthenticated traffic reach the recipe at
    /// all, even if the recipe would have rejected anyway — defense in
    /// depth.
    #[test]
    fn untrusted_source_rejected_at_trust_boundary_before_recipe() {
        let cfg = AdmissionConfig::strict_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        // ApprovedPeer is below IntragridMember (strict_v1's threshold).
        let cand = airc_candidate(
            "totally legitimate content here",
            TrustState::ApprovedPeer,
            "msg-2",
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        );
        match result {
            Err(AdmissionError::TrustBoundaryRejected {
                source_trust,
                threshold,
            }) => {
                assert_eq!(source_trust, TrustState::ApprovedPeer);
                assert_eq!(threshold, TrustState::IntragridMember);
            }
            other => panic!("expected TrustBoundaryRejected, got {other:?}"),
        }
    }

    /// What this catches: equal-tier source passes the threshold (>=, not >).
    /// Off-by-one on the comparison would silently reject valid traffic.
    #[test]
    fn trust_threshold_uses_inclusive_comparison() {
        let cfg = AdmissionConfig::strict_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        // IntragridMember == threshold; must pass.
        let cand = airc_candidate(
            "intragrid member message of sufficient length here",
            TrustState::IntragridMember,
            "msg-3",
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .expect("equal-tier source should pass threshold");
        assert!(matches!(result, AdmissionDecision::Admit { .. }));
    }

    // ── replay protection ───────────────────────────────────────────────

    /// What this catches: an event_id present in the seen-events oracle
    /// returns `ReplayDetected`. The gate must consult the oracle and
    /// reject before the recipe runs — replay protection is structural,
    /// not policy.
    #[test]
    fn replayed_event_returns_replay_detected() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        events
            .0
            .lock()
            .unwrap()
            .insert("msg-replay".to_string(), 1_000_000);
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "perfectly novel content here",
            TrustState::ApprovedPeer,
            "msg-replay",
        );

        let result = AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        );
        match result {
            Err(AdmissionError::ReplayDetected {
                event_id,
                previously_seen_at_ms,
            }) => {
                assert_eq!(event_id, "msg-replay");
                assert_eq!(previously_seen_at_ms, 1_000_000);
            }
            other => panic!("expected ReplayDetected, got {other:?}"),
        }
    }

    /// What this catches: non-Airc origins skip replay (no wire event id
    /// to check). A SelfReflection candidate must not get
    /// `ReplayDetected` even if an unrelated event id is in the oracle.
    #[test]
    fn non_airc_origin_skips_replay_check() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        events
            .0
            .lock()
            .unwrap()
            .insert("some-airc-id".to_string(), 1_000_000);
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = candidate(
            "reflective thought of sufficient length to admit",
            TrustState::SelfTrust,
            EngramOrigin::SelfReflection {
                parent_engram_id: Uuid::new_v4(),
            },
        );

        AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .expect("non-airc origin should bypass replay check");
    }

    // (HeuristicIsMemorable policy tests moved to admission/recipes.rs
    // per continuum#1208 — keep mod.rs focused on gate-level tests.)

    // ── trace seam emission ─────────────────────────────────────────────

    /// What this catches: every admission attempt — success OR error —
    /// emits exactly one `SEAM_ADMISSION` entry. Forensics and replay
    /// tooling depend on this invariant; missing seams break the
    /// "every gate decision is auditable" promise.
    #[test]
    fn every_admission_path_emits_exactly_one_seam() {
        let cfg = AdmissionConfig::permissive_v1();
        let mut trace = CognitionTrace::new();

        // Path 1: structural failure
        {
            let content = InMemoryContent::default();
            let events = InMemoryEvents::default();
            let ctx = permissive_ctx(&cfg, &content, &events);
            let cand = candidate(
                "x",
                TrustState::ApprovedPeer,
                EngramOrigin::Airc(airc_ref("e1", "", "h", "v1")),
            );
            let _ = AdmissionGate::admit(
                &cand,
                &HeuristicIsMemorable::default_v1(),
                &ctx,
                Some(&mut trace),
            );
        }
        assert_eq!(trace.seam_count(), 1);

        // Path 2: successful admit
        {
            let content = InMemoryContent::default();
            let events = InMemoryEvents::default();
            let ctx = permissive_ctx(&cfg, &content, &events);
            let cand = airc_candidate(
                "well-formed candidate of sufficient length to admit",
                TrustState::ApprovedPeer,
                "e2",
            );
            let _ = AdmissionGate::admit(
                &cand,
                &HeuristicIsMemorable::default_v1(),
                &ctx,
                Some(&mut trace),
            );
        }
        assert_eq!(trace.seam_count(), 2);

        // Path 3: drop (length)
        {
            let content = InMemoryContent::default();
            let events = InMemoryEvents::default();
            let ctx = permissive_ctx(&cfg, &content, &events);
            let cand = airc_candidate("short", TrustState::ApprovedPeer, "e3");
            let _ = AdmissionGate::admit(
                &cand,
                &HeuristicIsMemorable::default_v1(),
                &ctx,
                Some(&mut trace),
            );
        }
        assert_eq!(trace.seam_count(), 3);

        // Each seam should be SEAM_ADMISSION.
        for seam in &trace.seams {
            assert_eq!(seam.name, SEAM_ADMISSION);
        }
    }

    /// What this catches: trace metadata on a successful admit includes
    /// the recipe id + decision label. Operators reading the seam log
    /// need to see WHICH recipe ran and WHAT it decided, without parsing
    /// neighbouring data.
    #[test]
    fn admit_seam_metadata_carries_recipe_id_and_decision() {
        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "this is a meaningful design observation worth recalling",
            TrustState::ApprovedPeer,
            "msg-trace-1",
        );

        AdmissionGate::admit(
            &cand,
            &HeuristicIsMemorable::default_v1(),
            &ctx,
            Some(&mut trace),
        )
        .unwrap();
        let seam = &trace.seams[0];
        assert_eq!(seam.metadata["recipe"], serde_json::json!("heuristic.v1"));
        assert_eq!(seam.metadata["structural"], serde_json::json!("accepted"));
        assert_eq!(seam.metadata["decision"], serde_json::json!("Admit"));
    }

    // ── recipe error path ───────────────────────────────────────────────

    /// What this catches: a recipe that returns `Err(AdmissionError::RecipeFailure)`
    /// has its error propagated unchanged. Critical that the gate doesn't
    /// silently coerce recipe errors into Drop (would hide bugs in the
    /// recipe and turn loud failures into quiet drops).
    #[test]
    fn recipe_failure_propagates_as_recipe_failure() {
        struct FailingRecipe;
        impl IsMemorable for FailingRecipe {
            fn id(&self) -> &'static str {
                "test.failing"
            }
            fn evaluate(
                &self,
                _candidate: &AdmissionCandidate,
                _ctx: &AdmissionContext<'_>,
            ) -> Result<AdmissionDecision, AdmissionError> {
                Err(AdmissionError::RecipeFailure {
                    recipe_id: "test.failing".to_string(),
                    detail: "intentional test failure".to_string(),
                })
            }
        }

        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "passes structural checks, recipe will explode",
            TrustState::ApprovedPeer,
            "msg-fail",
        );

        let result = AdmissionGate::admit(&cand, &FailingRecipe, &ctx, Some(&mut trace));
        match result {
            Err(AdmissionError::RecipeFailure { recipe_id, detail }) => {
                assert_eq!(recipe_id, "test.failing");
                assert!(detail.contains("intentional"), "detail: {detail}");
            }
            other => panic!("expected RecipeFailure, got {other:?}"),
        }
    }

    /// What this catches: a recipe that emits `Quarantine` has the
    /// decision propagated unchanged (the gate doesn't override the
    /// recipe's quarantine choice). PR-3+ recipes will use this for
    /// borderline-similarity content.
    #[test]
    fn recipe_quarantine_decision_propagates() {
        struct QuarantineRecipe;
        impl IsMemorable for QuarantineRecipe {
            fn id(&self) -> &'static str {
                "test.quarantine"
            }
            fn evaluate(
                &self,
                candidate: &AdmissionCandidate,
                ctx: &AdmissionContext<'_>,
            ) -> Result<AdmissionDecision, AdmissionError> {
                Ok(AdmissionDecision::Quarantine {
                    engram: build_engram_from_candidate(candidate, ctx),
                    reason: "borderline similarity to existing engram".to_string(),
                    expiry_ms: ctx.now_ms + ctx.config.quarantine_ttl_ms,
                })
            }
        }

        let cfg = AdmissionConfig::permissive_v1();
        let content = InMemoryContent::default();
        let events = InMemoryEvents::default();
        let ctx = permissive_ctx(&cfg, &content, &events);
        let mut trace = CognitionTrace::new();

        let cand = airc_candidate(
            "borderline content that the recipe wants to quarantine",
            TrustState::ApprovedPeer,
            "msg-quar",
        );

        match AdmissionGate::admit(&cand, &QuarantineRecipe, &ctx, Some(&mut trace)).unwrap() {
            AdmissionDecision::Quarantine {
                engram, expiry_ms, ..
            } => {
                assert_eq!(engram.trust_state_at_admission, TrustState::ApprovedPeer);
                assert_eq!(expiry_ms, FIXED_NOW_MS + cfg.quarantine_ttl_ms);
            }
            other => panic!("expected Quarantine, got {other:?}"),
        }
        // Trace metadata should carry the Quarantine decision label.
        assert_eq!(
            trace.seams[0].metadata["decision"],
            serde_json::json!("Quarantine")
        );
    }

    // ── AdmissionConfig presets ─────────────────────────────────────────

    /// What this catches: the two preset configs have the trust ordering
    /// the docs claim (permissive accepts Authenticated; strict requires
    /// IntragridMember). A regression in the preset values would silently
    /// change the security posture of every persona using the defaults.
    #[test]
    fn admission_config_presets_have_documented_thresholds() {
        let permissive = AdmissionConfig::permissive_v1();
        let strict = AdmissionConfig::strict_v1();
        assert_eq!(permissive.trust_threshold, TrustState::Authenticated);
        assert_eq!(strict.trust_threshold, TrustState::IntragridMember);
        assert!(strict.trust_threshold > permissive.trust_threshold);
        // strict is shorter quarantine (faster auto-drop in SOC ops)
        assert!(strict.quarantine_ttl_ms < permissive.quarantine_ttl_ms);
    }

    // ── ts-rs binding tests ─────────────────────────────────────────────

    #[test]
    fn export_bindings_admission_candidate() {
        let cfg = ts_rs::Config::default();
        AdmissionCandidate::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_admission_config() {
        let cfg = ts_rs::Config::default();
        AdmissionConfig::export_all(&cfg).unwrap();
    }
