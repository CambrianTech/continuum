#![cfg(test)]

    use super::*;
    use crate::persona::admission::IsMemorable as _;
    use crate::persona::engram::{
        AdmissionDropReason, AircMessageRef, ChatMessageRef, EngramKind, TrustState,
    };
    use crate::persona::inbox_admission::content_hash_sha256;
    use crate::persona::types::SenderType;

    pub(super) fn synthetic_human_message(content: &str) -> InboxMessage {
        InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-human".to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp: 1_715_625_600_000,
            priority: 0.5,
            source_modality: None,
            voice_session_id: None,
        }
    }

    /// What this catches: a clean admit records the engram in the store,
    /// records the content_hash for dedup, AND a subsequent admit of the
    /// SAME content gets dropped as Duplicate (proving the side-effect
    /// recording actually feeds back into the next call's recipe).
    #[test]
    pub(super) fn admit_records_engram_and_dedup_blocks_repeat() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();
        let content = "this is a non-trivial design observation worth storing";
        let msg = synthetic_human_message(content);

        let first = state.admit(&msg, Some(&mut trace)).unwrap();
        assert!(matches!(first, AdmissionDecision::Admit { .. }));
        assert_eq!(state.engram_count(), 1);
        assert!(state.is_content_seen(&content_hash_sha256(content)));

        // Second admit of identical content (different message id, same content)
        // should drop as Duplicate.
        let msg2 = synthetic_human_message(content);
        let second = state.admit(&msg2, Some(&mut trace)).unwrap();
        match second {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { .. },
            } => {}
            other => panic!("expected Drop Duplicate, got {other:?}"),
        }
        // No new engram was admitted.
        assert_eq!(state.engram_count(), 1);
    }

    /// What this catches: dropped messages do NOT pollute either store.
    /// A dropped message's content_hash should NOT be in seen_content
    /// (otherwise a later legit version of the same content would be
    /// blocked as duplicate against a non-existent engram).
    #[test]
    pub(super) fn dropped_message_records_no_side_effect() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();
        // Short content → drops with NotMemorable.
        let msg = synthetic_human_message("short");

        let decision = state.admit(&msg, Some(&mut trace)).unwrap();
        match decision {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::NotMemorable { .. },
            } => {}
            other => panic!("expected Drop NotMemorable, got {other:?}"),
        }
        assert_eq!(state.engram_count(), 0);
        assert!(!state.is_content_seen(&content_hash_sha256("short")));
    }

    /// What this catches: admitted engrams accumulate in admission order
    /// + each engram is retrievable by index. Future recall surface
    /// depends on this; missing items would silently break recall.
    #[test]
    pub(super) fn admitted_engrams_accumulate_in_order_and_are_retrievable() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();
        let messages = [
            "first design observation worth recording",
            "second design observation worth recording",
            "third design observation worth recording",
        ];
        for content in messages {
            let _ = state.admit(&synthetic_human_message(content), Some(&mut trace));
        }
        assert_eq!(state.engram_count(), 3);
        assert_eq!(
            state.engram_at(0).expect("first engram present").content,
            messages[0]
        );
        assert_eq!(
            state.engram_at(2).expect("third engram present").content,
            messages[2]
        );
        assert!(state.engram_at(99).is_none(), "out-of-bounds returns None");
    }

    /// What this catches: the trace seam invariant carries through the
    /// state wrapper. Every admit() call (success + drop) appends exactly
    /// one SEAM_ADMISSION to the trace. Same forensic guarantee as the
    /// underlying runner.
    #[test]
    pub(super) fn admit_emits_one_seam_per_call_through_state_wrapper() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mut trace = CognitionTrace::new();
        // Three admits with three different outcomes:
        // (1) admit, (2) drop short, (3) drop duplicate of #1.
        let msg1 = synthetic_human_message("a long enough observation worth recording");
        let msg2 = synthetic_human_message("short");
        let msg3 = synthetic_human_message("a long enough observation worth recording");
        let _ = state.admit(&msg1, Some(&mut trace));
        let _ = state.admit(&msg2, Some(&mut trace));
        let _ = state.admit(&msg3, Some(&mut trace));
        assert_eq!(trace.seam_count(), 3, "one seam per admit() call");
    }

    /// What this catches: the runner accessor returns the configured
    /// runner so callers can introspect (recipe id for trace metadata,
    /// trust thresholds for debugging). A regression in the accessor
    /// would silently hide config from observability surfaces.
    #[test]
    pub(super) fn runner_accessor_exposes_default_v1_config() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        assert_eq!(state.runner().recipe().id(), "heuristic.v1");
    }

    /// What this catches: AdmissionState is Send + Sync. Compile-time
    /// proof that it can live inside `PersonaCognition` (which is held in
    /// a `DashMap<Uuid, PersonaCognition>` + crossed across tokio tasks).
    /// If a future refactor drops Send/Sync, this test fails to compile.
    #[test]
    pub(super) fn admission_state_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AdmissionState>();
    }

    // ── Quarantine side-effect rule (claude-tab-2 review nit on #1155) ──
    //
    // v1 has no quarantine store, so a Quarantined engram is dropped on
    // the floor. Recording its content_hash → engram_id in the dedup
    // store would leave a dangling pointer (future Duplicate drops would
    // surface an existing_engram_id that can't be looked up). The right
    // behaviour: ONLY record event_id (replay protection still applies),
    // never record content_hash on Quarantine.
    //
    // These tests construct synthetic AdmissionDecision values + call
    // `record_side_effects` directly so they don't need a custom recipe
    // — the heuristic recipe shipped here doesn't naturally emit
    // Quarantine, but the rule is about the side-effect helper itself.
    pub(super) fn synthetic_engram_with_chat_origin(content: &str) -> Engram {
        Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: 1_000_000,
                content_hash: content_hash_sha256(content),
            }),
            recall_keys: vec!["test".to_string()],
            admitted_at_ms: 1_000_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    pub(super) fn synthetic_engram_with_airc_origin(content: &str, message_id: &str) -> Engram {
        Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::Airc(AircMessageRef {
                transport: "airc".to_string(),
                room_id: "cambriantech".to_string(),
                message_id: message_id.to_string(),
                sender_id: "airc-8a5e".to_string(),
                sent_at_ms: 1_000_000,
                received_at_ms: 1_000_000,
                content_hash: content_hash_sha256(content),
                signature: "sig".to_string(),
                proof_refs: vec![],
                schema_version: "v1".to_string(),
                client_name: None,
            }),
            recall_keys: vec!["test".to_string()],
            admitted_at_ms: 1_000_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    /// What this catches: Quarantine of a Chat-origin engram records
    /// NEITHER content_hash NOR event_id. Chat origins have no replay
    /// surface distinct from content dedup, so quarantine on chat is a
    /// pure no-op as far as the side-effect stores are concerned.
    /// Original PR-4 code recorded content_hash here, leaving a dangling
    /// pointer.
    #[test]
    pub(super) fn quarantine_chat_origin_records_no_side_effects() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let engram = synthetic_engram_with_chat_origin("borderline observation");
        let content_hash = match &engram.origin {
            EngramOrigin::Chat(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Quarantine {
            engram,
            reason: "test borderline".to_string(),
            expiry_ms: 2_000_000,
        };

        state.record_side_effects(&decision);

        assert!(
            !state.is_content_seen(&content_hash),
            "chat-origin quarantine MUST NOT record content_hash (would dangle)"
        );
        assert_eq!(
            state.engram_count(),
            0,
            "quarantine MUST NOT add to engram store"
        );
    }

    /// What this catches: Quarantine of an AIRC-origin engram records
    /// the event_id (replay protection — the load-bearing behaviour) but
    /// MUST NOT record the content_hash (which would dangle since v1
    /// doesn't store quarantined engrams).
    #[test]
    pub(super) fn quarantine_airc_origin_records_event_id_only_not_content_hash() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let event_id = "airc-msg-quarantine-1";
        let engram =
            synthetic_engram_with_airc_origin("borderline observation worth holding", event_id);
        let content_hash = match &engram.origin {
            EngramOrigin::Airc(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Quarantine {
            engram,
            reason: "test borderline".to_string(),
            expiry_ms: 2_000_000,
        };

        state.record_side_effects(&decision);

        assert!(
            state.is_event_seen(event_id),
            "airc-origin quarantine MUST record event_id (replay protection)"
        );
        assert!(
            !state.is_content_seen(&content_hash),
            "airc-origin quarantine MUST NOT record content_hash (would dangle)"
        );
        assert_eq!(
            state.engram_count(),
            0,
            "quarantine MUST NOT add to engram store"
        );
    }

    // ── Recall surface (#1121 PR-5) ──────────────────────────────────────
    /// Helper: admit N synthetic human messages with distinct content,
    /// returning the engram ids in admission order.
    pub(super) fn admit_n_distinct(state: &AdmissionState, contents: &[&str]) -> Vec<Uuid> {
        let mut trace = CognitionTrace::new();
        let mut ids = Vec::new();
        for c in contents {
            match state
                .admit(&synthetic_human_message(c), Some(&mut trace))
                .unwrap()
            {
                AdmissionDecision::Admit { engram, .. } => ids.push(engram.id),
                other => panic!("expected Admit for content {c:?}, got {other:?}"),
            }
        }
        ids
    }

    /// What this catches: recall_recent returns engrams in NEWEST-FIRST
    /// order (reverse insertion). A regression to insertion-order would
    /// silently invert what callers expect when they ask for "recent".
    #[test]
    pub(super) fn recall_recent_returns_newest_first() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let ids = admit_n_distinct(
            &state,
            &[
                "first observation worth storing here",
                "second observation worth storing here",
                "third observation worth storing here",
            ],
        );
        let recent = state.recall_recent(3);
        assert_eq!(recent.len(), 3);
        // Newest first → reverse of admission order.
        assert_eq!(recent[0].id, ids[2]);
        assert_eq!(recent[1].id, ids[1]);
        assert_eq!(recent[2].id, ids[0]);
    }

    /// What this catches: the amnesia flash (`forget_context`) wipes ONLY the exam episode's
    /// engrams and leaves her other memory intact — the property that makes a natural proctored
    /// exam safe (she keeps her life's memory; only the answer-key episode is neuralyzed). A
    /// regression that widened the scope would erase real memories; one that narrowed it would
    /// leak exam answers into what she carries forward.
    /// See [[benchmarks-are-proctored-exams-of-the-natural-living-persona]].
    #[test]
    pub(super) fn forget_context_wipes_only_the_tagged_episode() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        admit_n_distinct(
            &state,
            &[
                "her real life memory one worth keeping",
                "exam question engram to be neuralyzed",
                "her real life memory two worth keeping",
            ],
        );
        let exam_ctx = Uuid::new_v4();
        // Stamp the middle engram as belonging to the exam episode (the acting body tags act
        // results with a context_id in the live path; here we set it directly for the test).
        {
            let mut engrams = state.engrams.lock().unwrap();
            engrams[1].context_id = Some(exam_ctx);
        }
        let forgotten = state.forget_context(exam_ctx);
        assert_eq!(forgotten, 1, "exactly the one exam engram is neuralyzed");
        let remaining = state.recall_recent(10);
        assert_eq!(remaining.len(), 2, "her two real memories survive");
        assert!(
            remaining.iter().all(|e| e.context_id != Some(exam_ctx)),
            "no exam-tagged engram remains"
        );
        // Flashing an unrelated context wipes nothing (scope safety).
        assert_eq!(state.forget_context(Uuid::new_v4()), 0);
    }

    /// What this catches: recall_recent honors the limit, never exceeds
    /// it, never panics on limit > available.
    #[test]
    pub(super) fn recall_recent_respects_limit_above_and_below_count() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        admit_n_distinct(
            &state,
            &[
                "alpha observation worth storing",
                "beta observation worth storing",
            ],
        );
        assert_eq!(state.recall_recent(0).len(), 0, "limit=0 returns empty");
        assert_eq!(state.recall_recent(1).len(), 1, "limit=1 returns one");
        assert_eq!(
            state.recall_recent(99).len(),
            2,
            "limit > count caps at count"
        );
    }

    // What this catches: when many engrams share the same salience (the common
    // case once recall-hit uplift flattens them to a ceiling), recall_candidates
    // must keep the NEWEST under truncation, not the oldest. A stable sort on
    // tied salience used to preserve insertion order (oldest-first), so the
    // truncate evicted the just-admitted engram — a persona acting and admitting
    // "I ran X → got Y" could never recall its own fresh result and looped
    // re-issuing the identical act. Regression for the [you just acted]-fold
    // removal (2026-06-25); see [[act-results-need-a-recency-channel-not-semantic-recall]].
    #[test]
    pub(super) fn recall_candidates_keeps_newest_when_salience_ties() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        // Seven engrams, all at the default salience (a tie). The freshest is ids[6].
        let ids = admit_n_distinct(
            &state,
            &[
                "first observation worth storing here",
                "second observation worth storing here",
                "third observation worth storing here",
                "fourth observation worth storing here",
                "fifth observation worth storing here",
                "sixth observation worth storing here",
                "I ran code/read(external_fingerprint) and got the function body",
            ],
        );
        let surfaced = state.recall_candidates(10_000, 5);
        assert_eq!(surfaced.len(), 5, "honors the limit");
        let surfaced_ids: Vec<Uuid> = surfaced.iter().map(|(e, _)| e.id).collect();
        // The freshest memory survives truncation and ranks first on the tie.
        assert_eq!(
            surfaced_ids[0], ids[6],
            "newest-wins-on-ties: the just-admitted act-observation is surfaced first"
        );
        // The two oldest are the ones evicted by the limit, not the newest.
        assert!(
            !surfaced_ids.contains(&ids[0]) && !surfaced_ids.contains(&ids[1]),
            "the OLDEST tied engrams are evicted, never the newest"
        );
    }

    // what this catches: a Tool-origin engram — a persona's own "I ran X → result"
    // receipt (proprioception) — must be EXCLUDED from the semantic recall pool so
    // it can never drown durable knowledge (#166). Note it's admitted NEWER than the
    // knowledge, so if the exclusion regressed, recency would surface it first. This
    // is the coverage that was missing when act_observe mis-admitted receipts as
    // non-Tool persona messages and the gate never bit.
    #[test]
    pub(super) fn recall_candidates_excludes_tool_origin_receipts() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let knowledge = admit_n_distinct(&state, &["the ticket asks for a wordstats CLI here"]);
        let receipt = crate::persona::engram::Engram {
            id: Uuid::new_v4(),
            context_id: None,
            kind: crate::persona::engram::EngramKind::Episodic,
            content: "code/list(path=src) → ok".to_string(),
            origin: crate::persona::engram::EngramOrigin::Tool(
                crate::persona::engram::ToolInvocationRef {
                    invocation_id: Uuid::new_v4(),
                    tool_name: "code/list".to_string(),
                    invoked_at_ms: 1000,
                    input_hash: "sha256:in".to_string(),
                    output_hash: "sha256:out".to_string(),
                },
            ),
            recall_keys: Vec::new(),
            admitted_at_ms: 2000,
            trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
            admission_trace_id: None,
        };
        let receipt_id = receipt.id;
        state.admit_reflection(receipt).expect("receipt admits");

        let ids: Vec<Uuid> = state
            .recall_candidates(10_000, 10)
            .into_iter()
            .map(|(e, _)| e.id)
            .collect();
        assert!(
            ids.contains(&knowledge[0]),
            "durable knowledge stays recallable"
        );
        assert!(
            !ids.contains(&receipt_id),
            "a Tool-origin receipt is NEVER in the semantic recall pool"
        );
    }

    // what this catches: wanderer INNER-SPEECH recall is recency-bounded (#145 / the
    // 2026-07-14 "feedback vs rag" incoherence — Atlas recalled a stale historian
    // thought "you keep failing to claim" as present truth after his claim succeeded).
    // A FRESH SelfReflection-kind thought still bubbles up; a STALE one drops; a
    // dream-DISTILLED Semantic reflection is durable and ALWAYS stays.
    #[test]
    pub(super) fn recall_recency_bounds_wanderer_inner_speech_but_keeps_distilled() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let now = 100 * 60 * 1000; // 100 min in
        let inner = |content: &str, admitted_at_ms: u64| Engram {
            id: Uuid::new_v4(),
            context_id: None,
            kind: EngramKind::SelfReflection, // wanderer inner speech
            content: content.to_string(),
            origin: EngramOrigin::SelfReflection {
                parent_engram_id: Uuid::new_v4(),
            },
            recall_keys: vec!["thought:historian".to_string()],
            admitted_at_ms,
            trust_state_at_admission: TrustState::SelfTrust,
            admission_trace_id: None,
        };
        let fresh = inner(
            "[thought:historian] a passing thought, moments old",
            now - 5 * 60 * 1000,
        );
        let stale = inner(
            "[thought:historian] you keep failing to claim",
            now - 40 * 60 * 1000,
        );
        // A dream-distilled DURABLE insight (Semantic kind, SelfReflection origin).
        let distilled =
            semantic_reflection("the codebase grades via rustc exit code", Uuid::new_v4());
        let (fresh_id, stale_id, distilled_id) = (fresh.id, stale.id, distilled.id);
        for e in [fresh, stale, distilled] {
            state.admit_reflection(e).expect("admits");
        }
        let ids: Vec<Uuid> = state
            .recall_candidates(now, 10)
            .into_iter()
            .map(|(e, _)| e.id)
            .collect();
        assert!(
            ids.contains(&fresh_id),
            "fresh inner speech still bubbles up"
        );
        assert!(
            !ids.contains(&stale_id),
            "a stale wanderer thought does NOT resurface as current fact"
        );
        assert!(
            ids.contains(&distilled_id),
            "dream-distilled Semantic insight is durable — always recallable"
        );
    }

    pub(super) fn semantic_reflection(content: &str, parent: Uuid) -> Engram {
        Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Semantic,
            content: content.to_string(),
            origin: EngramOrigin::SelfReflection {
                parent_engram_id: parent,
            },
            recall_keys: vec!["distilled".to_string()],
            admitted_at_ms: 2_000_000,
            trust_state_at_admission: TrustState::SelfTrust,
            admission_trace_id: None,
        }
    }

    /// What this catches: a self-produced (dream-distilled) Semantic engram is
    /// ingested through `admit_reflection` WITHOUT an external envelope, lands
    /// in the store, and is recall-visible — proving the dream's output reaches
    /// the same persistence recall reads, not a side buffer.
    #[test]
    pub(super) fn admit_reflection_stores_self_produced_fact() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let fact = semantic_reflection("Rust is the core; Node is the shell", Uuid::new_v4());

        let decision = state
            .admit_reflection(fact.clone())
            .expect("self-admission does not error");
        assert!(matches!(decision, AdmissionDecision::Admit { .. }));
        assert_eq!(state.engram_count(), 1);
        let recent = state.recall_recent(5);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].content, fact.content);
        assert_eq!(recent[0].kind, EngramKind::Semantic);
    }

    // what this catches: a turn in her mind room becoming an engram in engrams.sqlite
    // (sink 8 of PRIVACY-OF-THOUGHT.md). Both ingestion paths are withheld for her mind room;
    // the same memory in an ordinary room still admits.
    #[test]
    pub(super) fn a_memory_from_her_mind_room_is_not_written_to_her_engrams() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let her = Uuid::new_v4();
        state.set_owner_id(her);
        let mut private = semantic_reflection("a private thought", Uuid::new_v4());
        private.context_id = Some(crate::persona::mind_room::mind_room_id(her));
        assert!(matches!(
            state.admit_reflection(private).expect("no error"),
            AdmissionDecision::Drop { reason: crate::persona::engram::AdmissionDropReason::PolicyDeniedAdmission { .. } }
        ));
        assert_eq!(state.engram_count(), 0, "her mind room: nothing in her engrams");
        let mut open = semantic_reflection("an open thought", Uuid::new_v4());
        open.context_id = Some(Uuid::new_v4());
        assert!(matches!(state.admit_reflection(open).expect("no error"), AdmissionDecision::Admit { .. }));
        assert_eq!(state.engram_count(), 1, "an ordinary room still remembers");
    }

    /// What this catches: `redact` is the surgical complement to
    /// `forget_context` — it scrubs the held-out answer key out of a memory
    /// while KEEPING the engram and the experience of having answered. The
    /// proctored-exam integrity guarantee at the store level: she can't
    /// memorize the crib sheet, but her autobiography of struggling stays.
    #[test]
    pub(super) fn redact_scrubs_exam_key_from_memory_keeping_the_experience() {
        use crate::persona::redaction::{HeldOutAnswerDetector, RedactionClass, RedactionPolicy};
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mem = semantic_reflection(
            "I was asked which file holds the loop; I answered service_loop.rs and it passed.",
            Uuid::new_v4(),
        );
        state.admit_reflection(mem).expect("admits");

        let policy = RedactionPolicy::new(vec![Box::new(HeldOutAnswerDetector::new(
            ["service_loop.rs".to_string()],
            HeldOutAnswerDetector::DEFAULT_MIN_LEN,
        ))]);
        let report = state.redact(&policy);
        assert_eq!(report.count(RedactionClass::HeldOutAnswer), 1);

        let recalled = state.recall_recent(1);
        assert_eq!(recalled.len(), 1);
        let content = &recalled[0].content;
        assert!(content.contains("I was asked which file holds the loop"));
        assert!(content.contains("it passed."));
        assert!(!content.contains("service_loop.rs"));
        assert!(content.contains("[redacted:exam-key]"));
    }

    /// What this catches: a noop policy (no detectors) short-circuits to an
    /// empty report and mutates nothing — the hot path when no exam is in play.
    #[test]
    pub(super) fn redact_with_noop_policy_changes_nothing() {
        use crate::persona::redaction::RedactionPolicy;
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let mem = semantic_reflection("plain memory, nothing sensitive", Uuid::new_v4());
        state.admit_reflection(mem).expect("admits");
        let report = state.redact(&RedactionPolicy::new(vec![]));
        assert!(report.is_empty());
        assert_eq!(
            state.recall_recent(1)[0].content,
            "plain memory, nothing sensitive"
        );
    }

    pub(super) fn chat_engram(content: &str, sender: Uuid) -> Engram {
        Engram {
            id: Uuid::new_v4(),
            context_id: None,
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: sender,
                posted_at_ms: 1_000,
                content_hash: format!("sha256:{content}"),
            }),
            recall_keys: vec![],
            admitted_at_ms: 1_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        }
    }

    // what this catches: #166 sibling leak — a persona's OWN authored chat is
    // gated out of AMBIENT recall (it's proprioception, not knowledge) once
    // owner-id is bound, while OTHERS' chat still surfaces AND the own message
    // stays explicitly keyword-queryable. This is the structural kill for the
    // 271 self-authored "I ran X because Y" engrams that were drowning Casper's
    // real memory and teaching the survey-spin.
    #[test]
    pub(super) fn own_authored_chat_is_gated_from_ambient_recall_but_stays_queryable() {
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        state.set_owner_id(owner);

        state
            .admit_reflection(chat_engram(
                "I ran commands/list because Asha is acting on the situation",
                owner,
            ))
            .expect("own chat admits");
        state
            .admit_reflection(chat_engram(
                "The service loop lives in service_loop.rs",
                other,
            ))
            .expect("other's chat admits");

        let ambient: Vec<String> = state
            .recall_candidates(2_000, 10)
            .into_iter()
            .map(|(e, _)| e.content)
            .collect();
        assert!(
            ambient.iter().any(|c| c.contains("service_loop.rs")),
            "others' knowledge still surfaces in ambient recall"
        );
        assert!(
            !ambient.iter().any(|c| c.contains("because Asha is acting")),
            "own chatter is gated out of ambient recall"
        );

        // Still there when asked for explicitly.
        let queried: Vec<String> = state
            .recall_by_keyword("commands/list", 10)
            .into_iter()
            .map(|e| e.content)
            .collect();
        assert!(
            queried.iter().any(|c| c.contains("because Asha is acting")),
            "own chat remains keyword-queryable (gated from AMBIENT recall only)"
        );
    }

mod part_2;
mod part_3;
