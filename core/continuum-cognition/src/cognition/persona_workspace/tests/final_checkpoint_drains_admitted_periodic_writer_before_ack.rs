#![cfg(test)]
use super::*;

    #[cfg(feature = "stress-tests")]
    pub(super) mod stress {
        use super::super::*;

        // What this catches (#3918): an admitted old snapshot must finish before
        // final acknowledgement, without holding the resident lookup lock.
        #[tokio::test]
        async fn final_checkpoint_drains_admitted_periodic_writer_before_ack(
        ) -> Result<(), Box<dyn std::error::Error>> {
            let home = Arc::new(tempfile::tempdir()?);
            let _native = crate::paths::NativeHomeOverride::install(home.path());
            let registry = Arc::new(PersonaWorkspaceRegistry::new());
            let persona = Uuid::new_v4();
            registry
                .register_from_cfg(cfg_for(persona))
                .expect("test: resident checkpoint is readable");
            registry.cycles.lock()[&persona]
                .working_memory
                .record_receipt("old resident action");
            let (admitted, ready) = tokio::sync::oneshot::channel();
            let (resume, paused) = std::sync::mpsc::channel();
            *registry.checkpoint_pause.lock() = Some(CheckpointPause {
                admitted,
                resume: paused,
            });
            let owner = Arc::clone(&registry);
            let root = Arc::clone(&home);
            let periodic = tokio::task::spawn_blocking(move || {
                let _native = crate::paths::NativeHomeOverride::install(root.path());
                owner.checkpoint_volatile_all()
            });
            timeout(Duration::from_secs(5), ready).await??;
            let owner = Arc::clone(&registry);
            let root = Arc::clone(&home);
            let replacement = tokio::task::spawn_blocking(move || {
                let _native = crate::paths::NativeHomeOverride::install(root.path());
                let current = owner
                    .register_from_cfg(cfg_for(persona))
                    .expect("test: resident checkpoint is readable");
                assert!(Arc::ptr_eq(&current, &owner.get(&persona).unwrap()));
                let memory = Arc::clone(&owner.cycles.lock()[&persona].working_memory);
                memory.record_receipt("replacement completed the review");
                memory
            });
            let current = timeout(Duration::from_secs(5), replacement).await??;
            let expected = current.snapshot().last_action;
            let (starting, started) = tokio::sync::oneshot::channel();
            let owner = Arc::clone(&registry);
            let root = Arc::clone(&home);
            let mut final_save = tokio::task::spawn_blocking(move || {
                let _native = crate::paths::NativeHomeOverride::install(root.path());
                starting.send(()).unwrap();
                owner.stop_volatile_checkpoints()
            });
            timeout(Duration::from_secs(5), started).await??;
            assert!(
                timeout(Duration::from_millis(100), &mut final_save)
                    .await
                    .is_err(),
                "no final ACK while the admitted writer is paused"
            );
            assert!(
                registry.checkpoint_volatile_all().is_empty(),
                "busy periodic passes coalesce"
            );
            assert!(
                registry.get(&persona).is_some(),
                "lookup remains available during IO"
            );
            resume.send(()).unwrap();
            timeout(Duration::from_secs(5), periodic)
                .await??
                .remove(0)
                .1?;
            timeout(Duration::from_secs(5), final_save)
                .await??
                .remove(0)
                .1?;
            assert_eq!(
                load_volatile(persona).unwrap().unwrap().wm.last_action,
                expected
            );
            let checkpoint = std::fs::read(volatile_path(persona).unwrap()).unwrap();
            current.record_receipt("late action after final ACK");
            assert!(registry.checkpoint_volatile_all().is_empty());
            assert_eq!(
                std::fs::read(volatile_path(persona).unwrap()).unwrap(),
                checkpoint
            );
            Ok(())
        }
    }

    pub(super) fn seed_admission(now_ms: u64) -> Arc<AdmissionState> {
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let id = Uuid::new_v4();
        let engram = Engram {
            context_id: None,
            id,
            kind: EngramKind::Episodic,
            content: "the deploy pipeline went green after the 4pm fix".to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: now_ms,
                content_hash: "h".to_string(),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now_ms,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        };
        state.push_for_test(engram);
        recall_meta.admit(
            id,
            RecallMetadata {
                salience: 0.7,
                access_count: 0,
                last_accessed_ms: 0,
                protected_until_ms: 0,
                last_decayed_ms: now_ms,
            },
        );
        state
    }

    pub(super) fn cfg_for(persona_id: Uuid) -> PersonaBrainConfig {
        PersonaBrainConfig {
            persona_id,
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar, an engineer on the grid.".to_string(),
            admission: seed_admission(1_000_000_000),
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            grounding_sources: Vec::new(),
            embedder: None,
            tool_executor: None,
            experience: None,
            context_window: crate::inference::planning::MIN_SERVE_CTX,
            // Synchronous recall in the harness: these tests assert recall bids in
            // phase 1 on the same tick, which a deferred (cold-start) worker can't
            // satisfy. Deferral is a live-path concern, tested in deferred_faculty.rs.
            defer_recall: false,
            defer_grounding: false,
            // Harness/test cycles keep recall (they assert on its bids); suppression is a
            // benchmark-reproducibility knob set only by the eval fork.
            suppress_recall: false,
            forked: false,
            read_receipt: None,
        }
    }

    // what this catches: the assembled cycle runs a FULL persona mind end-to-end —
    // recall (hippocampus) bids in phase 1, deliberation (real adapter) decides in
    // phase 2 over that context — and yields a Decision. This is the production
    // assembly path; swap the adapter for LlamaCppAdapter and it's a live persona.
    #[tokio::test]
    pub(super) async fn assembled_cycle_produces_a_decision() {
        let persona = Uuid::new_v4();
        let cycle = build_workspace_cycle(cfg_for(persona));
        let ws = cycle.run("teammate: what's the deploy status?").await;
        // The mind reached a participation verdict (heuristic adapter → Speak).
        assert!(matches!(ws.decision(), Some(Decision::Speak { .. })));
    }

    // what this catches: #396 — resolve_persona is the ONE formal identity boundary
    // for eval/benchmark. It must (a) resolve a full UUID, an 8-char short-id, and a
    // case-insensitive persona NAME to the typed id; (b) pass a well-formed full UUID
    // through WITHOUT a liveness check (race safety — a persona still assembling isn't
    // false-rejected; the fork wait absorbs it); (c) fail LOUD on garbage, naming the
    // online roster — never a silent guess. The loose-String id that skipped all of
    // this is exactly what fed a dead reference to a doomed 10s eval.
    #[test]
    pub(super) fn resolve_persona_takes_uuid_short_id_or_name_and_fails_loud() {
        let registry = PersonaWorkspaceRegistry::new();
        let asha = Uuid::new_v4();
        let atlas = Uuid::new_v4();
        let mut asha_cfg = cfg_for(asha);
        asha_cfg.persona_name = "Asha".to_string();
        let mut atlas_cfg = cfg_for(atlas);
        atlas_cfg.persona_name = "Atlas".to_string();
        registry
            .register_from_cfg(asha_cfg)
            .expect("test: resident checkpoint is readable");
        registry
            .register_from_cfg(atlas_cfg)
            .expect("test: resident checkpoint is readable");

        // full UUID
        assert_eq!(
            registry.resolve_persona(&asha.to_string().into()).unwrap(),
            PeerId::from_uuid(asha)
        );
        // 8-char short-id prefix
        assert_eq!(
            registry
                .resolve_persona(&asha.to_string()[..8].into())
                .unwrap(),
            PeerId::from_uuid(asha)
        );
        // case-insensitive name
        assert_eq!(
            registry.resolve_persona(&"atlas".into()).unwrap(),
            PeerId::from_uuid(atlas)
        );
        assert_eq!(
            registry.resolve_persona(&"ASHA".into()).unwrap(),
            PeerId::from_uuid(asha)
        );

        // (b) a well-formed but NON-live full UUID passes through — race safety. The
        // caller's fork wait, not this boundary, decides liveness.
        let ghost = Uuid::new_v4();
        assert_eq!(
            registry.resolve_persona(&ghost.to_string().into()).unwrap(),
            PeerId::from_uuid(ghost)
        );

        // (c) garbage fails loud AND names the roster so the operator can fix it.
        let err = registry.resolve_persona(&"general".into()).unwrap_err();
        assert!(
            err.contains("Asha") && err.contains("Atlas"),
            "roster hint missing: {err}"
        );
    }

    // what this catches: ONE cycle per persona — get_or_build is idempotent and
    // returns the SAME Arc, so a persona's continuous mind is reused across every
    // room it services (the "one soul, many rooms" / anti-Severance invariant).
    #[tokio::test]
    pub(super) async fn registry_keeps_one_mind_per_persona() {
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        let first = registry
            .get_or_build(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        let second = registry
            .get_or_build(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        assert!(
            Arc::ptr_eq(&first, &second),
            "same persona must resolve to the SAME mind across rooms — not severed per-room"
        );
        assert_eq!(registry.len(), 1);
        // A different persona is a different mind.
        let _ = registry
            .get_or_build(cfg_for(Uuid::new_v4()))
            .expect("test: resident checkpoint is readable");
        assert_eq!(registry.len(), 2);
    }

    // what this catches: RESPAWN must replace the mind, not keep the stale one.
    // A persona can respawn in-process (node resilience) with a fresh admission +
    // adapter; the supervisor uses register() (overwrite), so get() returns the
    // NEW cycle, not the prior lifetime's orphaned one. (get_or_build would have
    // discarded the fresh config — the bug this guards.)
    #[tokio::test]
    pub(super) async fn register_overwrites_on_respawn() {
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        // register_from_cfg IS the production overwrite path (supervisor.rs spawn);
        // it builds + caches and returns the fresh Arc.
        let first = registry
            .register_from_cfg(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        let second = registry
            .register_from_cfg(cfg_for(persona))
            .expect("test: resident checkpoint is readable");
        let got = registry.get(&persona).expect("registered");
        assert!(
            Arc::ptr_eq(&got, &second),
            "respawn must resolve to the FRESH mind"
        );
        assert!(
            !Arc::ptr_eq(&got, &first),
            "the prior lifetime's mind is replaced"
        );
        assert_eq!(registry.len(), 1);
    }

    // 9f160b78: unreadable existing memory is not permission to register a blank
    // replacement. Both the live cycle and its fork template survive refusal.
    #[tokio::test(flavor = "current_thread")]
    pub(super) async fn failed_checkpoint_load_preserves_the_registered_resident() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        assert!(load_volatile(persona).unwrap().is_none());
        let current = registry.register_from_cfg(cfg_for(persona)).unwrap();
        let path = volatile_path(persona).unwrap();
        std::fs::write(&path, br#"{"wm":{},"own_speech":[]}"#).unwrap();
        let mut replacement = cfg_for(persona);
        replacement.persona_name = "must not replace the resident".into();
        let error = match registry.register_from_cfg(replacement) {
            Err(error) => error,
            Ok(_) => panic!("test: malformed checkpoint must refuse registration"),
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(Arc::ptr_eq(&current, &registry.get(&persona).unwrap()));
        assert_eq!(registry.templates.lock()[&persona].persona_name, "Ivar");
        assert!(Arc::ptr_eq(
            &current,
            &registry.get_or_build(cfg_for(persona)).unwrap()
        ));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            br#"{"wm":{},"own_speech":[]}"#
        );
    }

    // 9f160b78: cold lazy registration must preserve lock/read/schema errors;
    // none may insert a cycle or fork template as if no file had existed.
    #[tokio::test(flavor = "current_thread")]
    pub(super) async fn resident_registration_refuses_checkpoint_and_lock_errors() {
        let home = tempfile::tempdir().unwrap();
        let _native = crate::paths::NativeHomeOverride::install(home.path());
        let registry = PersonaWorkspaceRegistry::new();
        for fault in ["schema", "read", "lock"] {
            let persona = Uuid::new_v4();
            let path = volatile_path(persona).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            match fault {
                "schema" => std::fs::write(&path, b"not a checkpoint").unwrap(),
                "read" => std::fs::create_dir(&path).unwrap(),
                "lock" => {
                    std::fs::create_dir(path.parent().unwrap().join(".volatile.lock")).unwrap()
                }
                _ => unreachable!("test: fixed fault cases"),
            }
            assert!(
                registry.get_or_build(cfg_for(persona)).is_err(),
                "{fault} must refuse registration"
            );
            assert!(registry.get(&persona).is_none());
            assert!(!registry.templates.lock().contains_key(&persona));
        }
        assert_eq!(registry.len(), 0);
    }

    // THE LIVE BRING-UP: a persona's mind thinks with the REAL local model.
    // Runs the EXACT production assembly path (build_workspace_cycle → RecallFaculty
    // + LlmDeliberationFaculty) against the real LlamaCppAdapter (qwen3.5-4b-code-
    // forged on disk), over a real consolidated burst, and prints Ivar's actual
    // words. #[ignore] — needs a local GGUF + Metal. Run:
    //   CARGO_TARGET_DIR=$HOME/.continuum/cache/cargo-target \
    //   cargo test -p continuum-core --features metal,accelerate \
    //     cognition::persona_workspace::tests::ivar_thinks_with_the_real_model \
    //     -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "needs local GGUF + Metal; run with --ignored --nocapture"]
    pub(super) async fn ivar_thinks_with_the_real_model() {
        use crate::ai::adapter::AIProviderAdapter;
        use crate::inference::llamacpp_adapter::LlamaCppAdapter;

        crate::model_registry::init_global().expect("model_registry init");
        // context_length MUST be set explicitly (the backend refuses to silently
        // fall back to n_ctx_train — the 2026-04 Metal-KV-blowup guard). new()
        // doesn't set it; production uses for_persona(profile). 8192 fits Metal.
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(
            LlamaCppAdapter::new()
                .with_context_length(8192)
                .with_n_seq_max(1),
        );
        eprintln!(
            "[live] adapter={} default_model={}",
            adapter.name(),
            adapter.default_model()
        );

        use crate::cognition::llm_deliberation_faculty::LlmDeliberationFaculty;
        use crate::cognition::recall_faculty::RecallFaculty;
        use crate::cognition::workspace::{Faculty, SalienceArbiter, Workspace, WorkspaceCaptureSink, WorkspaceCycle, WorkspaceTrace};

        let persona = Uuid::new_v4();
        let admission = seed_admission(1_718_600_000_000);
        let system_prompt = "You are Ivar, a thoughtful engineer and a citizen on the grid. \
            You speak concisely, and only when you have something worth adding.";

        // Assemble the faculties directly (mirrors build_workspace_cycle) so we
        // keep a typed handle to the deliberation faculty — to introspect the
        // EXACT prompt it feeds the model.
        let delib = Arc::new(LlmDeliberationFaculty::new(
            persona,
            "Ivar",
            system_prompt,
            adapter,
        ));
        let faculties: Vec<Arc<dyn Faculty>> = vec![
            Arc::new(RecallFaculty::new(persona, admission)),
            delib.clone(),
        ];

        // The EXISTING capture harness: a WorkspaceCaptureSink records every phase
        // of the tick (all bids incl. losers, the assembled context, the decision)
        // so we can diagnose cognition at any phase — record + recreate, not guess.
        #[derive(Default)]
        struct CapturingSink(Mutex<Vec<WorkspaceTrace>>);
        impl WorkspaceCaptureSink for CapturingSink {
            fn record(&self, t: &WorkspaceTrace) {
                self.0.lock().push(t.clone());
            }
        }
        let sink = Arc::new(CapturingSink::default());

        let cycle = WorkspaceCycle::new(
            faculties,
            Arc::new(SalienceArbiter),
        )
        .with_capture(sink.clone());

        let burst = "general room:\n\
            Joel: morning all\n\
            teammate: the deploy from yesterday — did we ever figure out what broke it?\n\
            teammate: ivar you were looking at it right?";

        let ws = cycle.run(burst).await;

        // ---- Glass box: diagnose cognition at EVERY phase ----
        let trace = sink.0.lock().pop().expect("a tick was recorded");
        eprintln!("\n================ COGNITION TRACE ================");
        eprintln!("WORLD-STATE (the burst):\n{}\n", trace.world_state);
        eprintln!("PHASE 1 — perception bids (the full competition, incl. losers):");
        for b in &trace.bids {
            eprintln!(
                "  [{:<12} s={:.2}] {}  ({})",
                b.faculty.as_str(),
                b.salience,
                b.content.replace('\n', " / "),
                b.reasoning
            );
        }
        eprintln!("\nASSEMBLED CONTEXT the decider saw (context_broadcast = the RAG):");
        for c in &trace.context_broadcast {
            eprintln!(
                "  [{}] {}",
                c.faculty.as_str(),
                c.content.replace('\n', " / ")
            );
        }

        // ---- EXACTLY what the LLM was fed (reconstruct the pre-deliberation ws) ----
        // Route through the constructor (opaque single-turn burst) so this can't
        // drift as Workspace gains fields; then graft on the recorded context
        // broadcast the decider actually saw. The trace does not yet record the
        // turn's directedness or self-initiation; reconstruct as ambient /
        // message-driven (the live defaults). TODO(#9): carry directedness on the
        // trace so a replayed directed turn shows the silence escape withheld as it was.
        let mut context_ws = Workspace::in_room(burst.to_string(), trace.room_id);
        context_ws.broadcast = trace.context_broadcast.clone();
        let view = delib.prompt_view(&context_ws);
        eprintln!("\n--------------- WHAT THE LLM WAS FED ---------------");
        eprintln!("[SYSTEM]\n{}\n", view.system);
        eprintln!("[CONVERSATION]\n{}", view.user_text());

        eprintln!("\n--------------- Ivar's DECISION ---------------");
        match ws.decision() {
            Some(Decision::Speak { text }) => eprintln!("Ivar SPEAKS:\n{text}"),
            Some(Decision::RaiseUnprompted { text }) => eprintln!("Ivar RAISES:\n{text}"),
            Some(Decision::Act { calls, intent }) => {
                eprintln!("Ivar ACTS ({} call(s)) — intent: {intent}", calls.len())
            }
            Some(Decision::Pass { .. }) | None => eprintln!("Ivar chose silence (PASS)."),
        }
        eprintln!("=================================================\n");

        assert!(
            ws.decision().is_some(),
            "the persona's mind must reach a decision through the real model"
        );
    }

    // A constructible RagSource whose delivery is never exercised — these tests
    // assert the deferrability CLASSIFICATION on GroundingSource, not delivery.
    pub(super) struct ClassifyStub;

    #[async_trait::async_trait]
    impl RagSource for ClassifyStub {
        fn source_id(&self) -> &'static str {
            "classify-stub"
        }

        fn expand_command(&self) -> Option<&'static str> {
            // Test/stub source — nothing further to fetch.
            None
        }

        /// Test/stub source — floorless, so it never encodes a production floor.
        async fn deliver(&self, _ctx: &crate::cognition::rag_budget::RagContext) -> crate::cognition::rag_budget::RagDelivery {
            crate::cognition::rag_budget::RagDelivery {
                source_id: "classify-stub".to_string(),
                items: Vec::new(),
                tokens_used: 0,
                truncated: false,
            }
        }
    }

    // what this catches: deferrability is ORTHOGONAL to salience, and the SAFE
    // DEFAULT is synchronous. framing() is ColdStartCritical — a fresh grounding
    // source runs ON the loop until deliberately opted off, so doctrine (the
    // participation gate) can never silently start serving a cold-start `None`
    // (speaking in a room it shouldn't on turn one). `.defer_tolerant()` flips
    // ONLY the schedule axis, leaving StandingFraming salience untouched. If a
    // future edit folds salience into the deferrability builder, or flips the
    // default to DeferTolerant, this fails — guarding the two-independent-axes
    // invariant the live supervisor wiring depends on.
    #[test]
    pub(super) fn deferrability_is_orthogonal_to_salience_and_defaults_to_synchronous() {
        let s: Arc<dyn RagSource> = Arc::new(ClassifyStub);

        let critical = GroundingSource::framing(Arc::clone(&s));
        assert_eq!(
            critical.deferrability,
            Deferrability::ColdStartCritical,
            "safe default: a framing source runs synchronously until opted off"
        );
        assert_eq!(critical.policy, SaliencePolicy::StandingFraming);

        let tolerant = GroundingSource::framing(Arc::clone(&s)).defer_tolerant();
        assert_eq!(
            tolerant.deferrability,
            Deferrability::DeferTolerant,
            "opt-in flips ONLY the schedule axis"
        );
        assert_eq!(
            tolerant.policy,
            SaliencePolicy::StandingFraming,
            "orthogonal: deferrability must not touch the salience policy"
        );

        // retrieved() (the other constructor) also defaults to the safe side.
        let retrieved = GroundingSource::retrieved(Arc::clone(&s));
        assert_eq!(retrieved.deferrability, Deferrability::ColdStartCritical);
    }

    // what this catches: capability consistency on the eval fork — grounding
    // marked requires_hands (workspace-map: "drill in with code/list and
    // code/tree") must be DROPPED from a tools-stripped fork and KEPT on a
    // handed fork. Glass-boxed 2026-07-10: every captured spoken-exam prompt
    // (484/484) carried the workspace-map telling her to use tools that were
    // stripped for the exam — the RAG lying to her about her own affordances.
    #[tokio::test]
    pub(super) async fn eval_fork_drops_hands_describing_grounding_when_tools_stripped() {
        const HANDS_MARKER: &str = "drill in with code/list and code/tree";
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        // SlowGrounding's fixed roster line stands in for capability-NEUTRAL
        // grounding; the hands-describing source is its own tiny stub.
        struct HandsMap;
        #[async_trait::async_trait]
        impl RagSource for HandsMap {
            fn source_id(&self) -> &'static str {
                "workspace-map"
            }

            fn expand_command(&self) -> Option<&'static str> {
                // Test/stub source — nothing further to fetch.
                None
            }

            /// Test/stub source — floorless, so it never encodes a production floor.
            async fn deliver(&self, _ctx: &crate::cognition::rag_budget::RagContext) -> crate::cognition::rag_budget::RagDelivery {
                crate::cognition::rag_budget::RagDelivery {
                    source_id: "workspace-map".to_string(),
                    items: vec![crate::cognition::rag_budget::RagItem {
                        key: String::new(),
                        summary: None,
                        pointer: None,
                        content: format!("workspace layout — {HANDS_MARKER}"),
                        tokens: 12,
                        metadata: serde_json::Value::Null,
                    }],
                    tokens_used: 12,
                    truncated: false,
                }
            }
        }
        let mut cfg = cfg_for(persona);
        cfg.grounding_sources = vec![
            GroundingSource::framing(Arc::new(HandsMap)).requires_hands(),
            // A second, hands-agnostic source so the assertion below is about the
            // hands-describing one being stripped, not about grounding being empty.
            // Its gate starts open, so it delivers straight through here.
            GroundingSource::framing(Arc::new(GatedGrounding::new())),
        ];
        registry
            .register_from_cfg(cfg)
            .expect("test: resident checkpoint is readable");

        // Spoken exam (no hands): the workspace-map must not reach her mind.
        let spoken = registry
            .fork_eval_cycle(&persona, false, None, false)
            .expect("template retained");
        let ws = spoken.run("proctor: reverse a string in Rust").await;
        assert!(
            !ws.perceived().contains(HANDS_MARKER),
            "tool-stripped fork must NOT perceive hands-describing grounding:\n{}",
            ws.perceived()
        );
        // Neutral grounding survives the filter — only hands-claims are dropped.
        assert!(
            ws.perceived().contains("roster:"),
            "capability-neutral grounding must survive the hands filter:\n{}",
            ws.perceived()
        );

        // Handed exam: the map is real guidance and must be delivered.
        let handed = registry
            .fork_eval_cycle(&persona, true, None, false)
            .expect("template retained");
        let ws = handed.run("proctor: reverse a string in Rust").await;
        assert!(
            ws.perceived().contains(HANDS_MARKER),
            "handed fork must keep the workspace-map:\n{}",
            ws.perceived()
        );
    }

    // what this catches (#207): the reproducible-absolute benchmark knob. A default fork keeps
    // the recall faculty (memories intact — the natural persona), so its ABSOLUTE score can
    // drift as her durable engram store grows between runs; a `suppress_recall` fork OMITS
    // recall entirely (system + task + grounding only), pinning the absolute baseline. The two
    // forks must differ by exactly the recall faculty — the whole point of the knob.
    #[tokio::test]
    pub(super) async fn suppress_recall_omits_the_recall_faculty_from_the_eval_fork() {
        use crate::cognition::workspace::FacultyId;
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        registry
            .register_from_cfg(cfg_for(persona))
            .expect("test: resident checkpoint is readable");

        let with_recall = registry
            .fork_eval_cycle(&persona, false, None, false)
            .expect("template retained");
        assert!(
            with_recall.faculty_ids().contains(&FacultyId::Recall),
            "default fork must keep recall (memories intact): {:?}",
            with_recall.faculty_ids()
        );

        let suppressed = registry
            .fork_eval_cycle(&persona, false, None, true)
            .expect("template retained");
        assert!(
            !suppressed.faculty_ids().contains(&FacultyId::Recall),
            "suppress_recall fork must OMIT recall for a reproducible absolute baseline: {:?}",
            suppressed.faculty_ids()
        );
    }
