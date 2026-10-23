#![cfg(test)]
use super::*;

    // what this catches: the eval fork inherits owner-id, so the BENCHMARK (which
    // runs on `fork_detached`, not the live self) reflects the #166 recall fix.
    // Without the propagation, live turns would be clean but measured numbers
    // would still be polluted by the persona's own chatter — a lying benchmark.
    #[test]
    pub(super) fn fork_detached_inherits_owner_id_so_the_benchmark_gets_the_recall_fix() {
        let owner = Uuid::new_v4();
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        state.set_owner_id(owner);
        state
            .admit_reflection(chat_engram(
                "I ran commands/list because I am acting",
                owner,
            ))
            .expect("admits");
        let fork = state.fork_detached();
        let ambient: Vec<String> = fork
            .recall_candidates(2_000, 10)
            .into_iter()
            .map(|(e, _)| e.content)
            .collect();
        assert!(
            !ambient.iter().any(|c| c.contains("because I am acting")),
            "fork inherits owner-id → own chatter is gated on the measurement copy too"
        );
    }

    // what this catches: fail-open — with no owner bound (bare/test state), the
    // gate never fires, so we never drop chat we can't prove is self-authored.
    #[test]
    pub(super) fn chat_recall_gate_is_noop_without_owner_bound() {
        let author = Uuid::new_v4();
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        state
            .admit_reflection(chat_engram("some chat from an author", author))
            .expect("admits");
        let ambient: Vec<String> = state
            .recall_candidates(2_000, 10)
            .into_iter()
            .map(|(e, _)| e.content)
            .collect();
        assert!(
            ambient
                .iter()
                .any(|c| c.contains("some chat from an author")),
            "no owner bound → nothing gated (fail-open)"
        );
    }

    /// What this catches: re-distilling the same cluster on a later dream tick
    /// is idempotent — identical fact content is Dropped as Duplicate, never
    /// accumulated. Without this guard the store would fill with duplicate facts
    /// every idle tick.
    #[test]
    pub(super) fn admit_reflection_dedups_identical_facts() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let parent = Uuid::new_v4();
        let first = semantic_reflection("Headless core, many equal clients", parent);
        let second = semantic_reflection("Headless core, many equal clients", parent);

        let d1 = state.admit_reflection(first.clone()).expect("first admits");
        assert!(matches!(d1, AdmissionDecision::Admit { .. }));

        let d2 = state.admit_reflection(second).expect("second resolves");
        match d2 {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { existing_engram_id },
            } => assert_eq!(existing_engram_id, first.id),
            other => panic!("expected Duplicate drop, got {other:?}"),
        }
        assert_eq!(state.engram_count(), 1, "duplicate fact not stored twice");
    }

    /// What this catches: recall_scored ranks engrams by salience desc.
    /// A regression to admission-order or insertion-order would silently
    /// return whichever was admitted last instead of whichever scored
    /// highest — defeating the whole point of Algorithm 4 driving recall.
    #[test]
    pub(super) fn recall_scored_ranks_by_salience_desc() {
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state = AdmissionState::new(Arc::clone(&registry));
        let ids = admit_n_distinct(
            &state,
            &[
                "alpha observation worth storing",
                "beta observation worth storing",
                "gamma observation worth storing",
            ],
        );
        // Pin gamma → salience 1.0, permanent protection. Beta gets a
        // recall hit → salience uplift. Alpha is left at default (0.5).
        // Expected order after scoring: gamma > beta > alpha.
        registry.pin_permanent(ids[2]);
        registry.record_recall_hit(ids[1], 1_000);

        let now_ms = 10_000;
        let scored = state.recall_scored(now_ms, 8);
        assert_eq!(scored.len(), 3);
        assert_eq!(scored[0].0.id, ids[2], "pinned gamma ranks first");
        assert_eq!(scored[1].0.id, ids[1], "uplifted beta ranks second");
        assert_eq!(scored[2].0.id, ids[0], "untouched alpha ranks last");
        assert!(
            scored[0].1 >= scored[1].1 && scored[1].1 >= scored[2].1,
            "score values are monotonically descending: {:?}",
            scored.iter().map(|(_, s)| *s).collect::<Vec<_>>()
        );
    }

    /// What this catches: recall_scored records a recall hit on every
    /// returned engram (Hebbian rehearsal — use-it-keeps-it). Without
    /// this, scoring is one-way: salient memories only decay, never
    /// climb back from being used. The feedback loop closing here is
    /// what makes the substrate continual-learning, not just continual-
    /// remembering.
    #[test]
    pub(super) fn recall_scored_records_recall_hit_on_returned_engrams() {
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state = AdmissionState::new(Arc::clone(&registry));
        let ids = admit_n_distinct(
            &state,
            &[
                "alpha observation worth storing",
                "beta observation worth storing",
            ],
        );
        let before_alpha = registry.get(ids[0]).expect("alpha metadata exists");
        let before_beta = registry.get(ids[1]).expect("beta metadata exists");

        // Score returns both. Both should get a recall hit.
        let scored = state.recall_scored(5_000, 8);
        assert_eq!(scored.len(), 2);

        let after_alpha = registry.get(ids[0]).expect("alpha metadata after");
        let after_beta = registry.get(ids[1]).expect("beta metadata after");
        assert!(
            after_alpha.access_count > before_alpha.access_count,
            "alpha access_count climbed: {} -> {}",
            before_alpha.access_count,
            after_alpha.access_count
        );
        assert!(
            after_beta.access_count > before_beta.access_count,
            "beta access_count climbed"
        );
        assert!(
            after_alpha.last_accessed_ms == 5_000,
            "alpha last_accessed_ms updated to now_ms (was {}, now {})",
            before_alpha.last_accessed_ms,
            after_alpha.last_accessed_ms
        );
    }

    /// What this catches: recall_scored honors the limit (caps below
    /// count, doesn't exceed available) and limit=0 returns empty
    /// without panicking + without recording spurious hits.
    #[test]
    pub(super) fn recall_scored_respects_limit_and_empty() {
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state = AdmissionState::new(Arc::clone(&registry));
        let ids = admit_n_distinct(
            &state,
            &[
                "alpha observation worth storing",
                "beta observation worth storing",
                "gamma observation worth storing",
            ],
        );
        let before = registry.get(ids[0]).expect("alpha metadata exists");

        assert_eq!(
            state.recall_scored(1_000, 0).len(),
            0,
            "limit=0 returns empty"
        );

        let after_zero = registry.get(ids[0]).expect("alpha metadata after limit=0");
        assert_eq!(
            after_zero.access_count, before.access_count,
            "limit=0 records no recall hits"
        );

        assert_eq!(
            state.recall_scored(1_000, 1).len(),
            1,
            "limit=1 returns one"
        );
        assert_eq!(
            state.recall_scored(1_000, 99).len(),
            3,
            "limit > count caps at count"
        );
    }

    /// What this catches: recall_by_id returns the exact engram for a
    /// known id, None for an unknown id. Foundation of any future recall
    /// pipeline that walks parent/reflection links.
    #[test]
    pub(super) fn recall_by_id_finds_known_returns_none_unknown() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let ids = admit_n_distinct(
            &state,
            &[
                "first observation worth storing",
                "second observation worth storing",
            ],
        );
        let found = state.recall_by_id(ids[0]).expect("known id must resolve");
        assert_eq!(found.id, ids[0]);
        assert_eq!(found.content, "first observation worth storing");
        assert!(
            state.recall_by_id(Uuid::new_v4()).is_none(),
            "unknown id is None"
        );
    }

    /// What this catches: keyword search is case-insensitive substring,
    /// returns newest-first, honors limit. Empty keyword returns empty
    /// (caller-meant-to-skip semantic, not match-everything).
    #[test]
    pub(super) fn recall_by_keyword_case_insensitive_newest_first_with_limit() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        admit_n_distinct(
            &state,
            &[
                "the recall ratchet design needs work",
                "not relevant to our search needle here",
                "another RECALL ratchet observation",
            ],
        );
        let hits = state.recall_by_keyword("recall", 10);
        assert_eq!(
            hits.len(),
            2,
            "two engrams contain 'recall' (case-insensitive)"
        );
        // Newest first: "another RECALL..." was admitted last.
        assert!(
            hits[0].content.contains("another RECALL"),
            "newest-first ordering: got {}",
            hits[0].content
        );
        // Empty needle = caller skipped search.
        assert!(state.recall_by_keyword("", 10).is_empty());
        // Zero limit short-circuits.
        assert!(state.recall_by_keyword("recall", 0).is_empty());
        // Limit caps result count.
        assert_eq!(state.recall_by_keyword("recall", 1).len(), 1);
    }

    /// What this catches: origin-kind filter returns only matching
    /// variants. Inbox-sourced messages currently always synthesize
    /// `Chat` origins (per PR-3 design); if someone admits via a
    /// different origin path (PR-5+ tool/reflection ingestion), the
    /// filter must still segregate cleanly.
    #[test]
    pub(super) fn recall_by_origin_kind_filters_to_requested_variant() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        admit_n_distinct(
            &state,
            &[
                "human observation worth storing here",
                "another human observation worth storing",
            ],
        );
        // All inbox admits are Chat-origin.
        let chat_hits = state.recall_by_origin_kind(EngramOriginKind::Chat, 10);
        assert_eq!(chat_hits.len(), 2);
        // No Airc origins admitted via the inbox path.
        let airc_hits = state.recall_by_origin_kind(EngramOriginKind::Airc, 10);
        assert!(airc_hits.is_empty());
        // Limit honored.
        assert_eq!(
            state.recall_by_origin_kind(EngramOriginKind::Chat, 1).len(),
            1
        );
        // Limit zero = empty.
        assert!(state
            .recall_by_origin_kind(EngramOriginKind::Chat, 0)
            .is_empty());
    }

    /// What this catches: EngramOriginKind::from(&EngramOrigin) covers
    /// every variant of EngramOrigin. If a future PR adds a new variant
    /// to EngramOrigin without updating the From impl, this test fails
    /// to compile (exhaustive match in From). The recall filter would
    /// otherwise silently miss the new origin variant.
    #[test]
    pub(super) fn engram_origin_kind_covers_all_origin_variants() {
        // Construct one of each variant; `From` impl is exhaustive at
        // compile time. This test confirms the runtime mapping.
        let chat = synthetic_engram_with_chat_origin("x");
        let airc = synthetic_engram_with_airc_origin("y", "evt-1");
        assert_eq!(EngramOriginKind::from(&chat.origin), EngramOriginKind::Chat);
        assert_eq!(EngramOriginKind::from(&airc.origin), EngramOriginKind::Airc);
        // Agent origin (agent-memory bridge) maps to EngramOriginKind::Agent.
        let agent_origin = EngramOrigin::Agent(crate::persona::engram::AgentRef {
            agent_peer_id: Uuid::new_v4(),
            session: None,
            origin_hint: None,
        });
        assert_eq!(
            EngramOriginKind::from(&agent_origin),
            EngramOriginKind::Agent
        );
        // Tool + SelfReflection variants exist on EngramOrigin (per PR-1)
        // and are covered by the From impl's exhaustive match — no need
        // to construct them here; the compiler enforces coverage.
    }

    /// What this catches: an Agent origin serializes with tag "Agent" and its
    /// AgentRef payload (agent_peer_id required, session/origin_hint optional),
    /// and round-trips back to EngramOriginKind::Agent. The wire shape is what
    /// the agent-memory bridge's /remember + /recall commands depend on; a
    /// serde-tag drift would silently break cross-agent recall.
    #[test]
    pub(super) fn agent_origin_serde_round_trips_with_required_peer_id() {
        use crate::persona::engram::AgentRef;
        let me = Uuid::new_v4();

        let origin = EngramOrigin::Agent(AgentRef {
            agent_peer_id: me,
            session: Some("sess-1".into()),
            origin_hint: None,
        });
        let json = serde_json::to_value(&origin).unwrap();
        assert_eq!(json["kind"], "Agent");
        assert_eq!(json["ref"]["agent_peer_id"], me.to_string());
        // origin_hint is None → omitted (ts optional / serde skips nothing by
        // default, so it's present-but-null); session present.
        assert_eq!(json["ref"]["session"], "sess-1");

        let back: EngramOrigin = serde_json::from_value(json).unwrap();
        assert_eq!(EngramOriginKind::from(&back), EngramOriginKind::Agent);
    }

    /// What this catches: Admit (NOT Quarantine) records BOTH content_hash
    /// AND event_id for AIRC origins. This is the regression-anchor for
    /// the refactor that split `record_engram_origin` → `record_admitted`
    /// + `record_replay_only`. If the refactor accidentally narrowed the
    /// Admit path's recording, dedup would silently break.
    #[test]
    pub(super) fn admit_airc_origin_still_records_both_content_hash_and_event_id() {
        let state = AdmissionState::new(Arc::new(
            crate::persona::recall_metadata::RecallMetadataRegistry::new(),
        ));
        let event_id = "airc-msg-admit-1";
        let engram =
            synthetic_engram_with_airc_origin("valuable observation worth recalling", event_id);
        let content_hash = match &engram.origin {
            EngramOrigin::Airc(r) => r.content_hash.clone(),
            _ => unreachable!(),
        };
        let decision = AdmissionDecision::Admit {
            engram,
            why: "test admit".to_string(),
        };

        state.record_side_effects(&decision);

        assert!(
            state.is_event_seen(event_id),
            "airc-origin admit MUST record event_id"
        );
        assert!(
            state.is_content_seen(&content_hash),
            "airc-origin admit MUST record content_hash"
        );
        assert_eq!(state.engram_count(), 1, "admit MUST add to engram store");
    }

    // ── Persistence wire-up tests (#168) ──────────────────────────
    /// What this catches: admit observes through the persistence
    /// sink. The RecordingSink buffers each admission so the test
    /// can assert it landed.
    #[test]
    pub(super) fn admit_observes_admission_through_persistence_sink() {
        use crate::persona::admission_persistence::RecordingSink;
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let sink = Arc::new(RecordingSink::new());
        let state = AdmissionState::new_with_persistence(
            Arc::clone(&registry),
            Arc::clone(&sink)
                as Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
        );
        let msg = synthetic_human_message("watch me persist");
        state.admit(&msg, None).expect("admit");
        let seen = sink.admissions_seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0.content, "watch me persist");
    }

    /// What this catches: recall_scored observes metadata updates
    /// through the persistence sink after each record_recall_hit.
    /// Hebbian rehearsal IS the disk-write trigger — every recall
    /// that lifts salience flushes the new value.
    #[test]
    pub(super) fn recall_scored_observes_metadata_updates_through_sink() {
        use crate::persona::admission_persistence::RecordingSink;
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let sink = Arc::new(RecordingSink::new());
        let state = AdmissionState::new_with_persistence(
            Arc::clone(&registry),
            Arc::clone(&sink)
                as Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
        );
        // Admit 2 engrams so recall has something to score.
        admit_n_distinct(
            &state,
            &[
                "first observation worth recall",
                "second observation worth recall",
            ],
        );
        // Clear the sink's admission buffer before scoring — we
        // want to assert only the metadata updates that follow.
        let _ = sink.admissions_seen(); // (just a sanity touch, no clear)
        let _scored = state.recall_scored(5_000, 8);
        let updates = sink.metadata_updates_seen();
        assert_eq!(
            updates.len(),
            2,
            "each recall hit observes a metadata update"
        );
    }

    /// What this catches: eval mutating the persona it measures. The
    /// isolation seam (checkpoint → mute persistence → admit → restore →
    /// unmute) must (1) let admit fire normally INTO the frozen frame so the
    /// measured memory motion is identical to a real turn, (2) keep every muted
    /// admit off the real sink, (3) rewind engram_count AND the dedup oracle on
    /// restore — proven by re-admitting the same content after restore and
    /// seeing it ADMIT (not dedup-drop) — and (4) restore the real sink so it
    /// observes again afterward. Regresses [[eval-mutates-persona-lift-needs-isolation]].
    #[test]
    pub(super) fn eval_isolation_checkpoint_restore_leaves_no_trace() {
        use crate::persona::admission_persistence::{NoopSink, RecordingSink};
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let real_sink = Arc::new(RecordingSink::new());
        let state = AdmissionState::new_with_persistence(
            Arc::clone(&registry),
            Arc::clone(&real_sink)
                as Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
        );

        // Non-trivial content: the admission gate drops short strings as
        // NotMemorable, so both arms use durable, distinct observations.
        let content_a = "the durable baseline observation worth keeping in memory";
        let content_b = "the eval-window observation that must never reach disk";

        // Baseline: one durable engram, observed by the real sink.
        state
            .admit(&synthetic_human_message(content_a), None)
            .expect("admit A");
        assert_eq!(state.engram_count(), 1, "baseline admit lands");
        assert_eq!(real_sink.admissions_seen().len(), 1, "real sink saw A");

        // ── Begin isolation: checkpoint the frame, mute persistence. ──
        let checkpoint = state.checkpoint();
        let saved_real = state.swap_persistence(NoopSink::arc());

        // Admit INSIDE the window: admit fires (count climbs → identical
        // motion) but the real sink, now swapped out, sees nothing more.
        state
            .admit(&synthetic_human_message(content_b), None)
            .expect("admit B");
        assert_eq!(state.engram_count(), 2, "muted admit still forms memory");
        assert_eq!(
            real_sink.admissions_seen().len(),
            1,
            "muted window writes NOTHING to the real sink"
        );

        // ── End isolation: rewind the frame, restore the real sink. ──
        state.restore(&checkpoint);
        state.swap_persistence(saved_real);
        assert_eq!(
            state.engram_count(),
            1,
            "restore rewinds memory to baseline"
        );

        // The dedup oracle rewound too: content_b is admissible again (had it
        // NOT rewound, this would dedup-drop and the count would stay 1).
        state
            .admit(&synthetic_human_message(content_b), None)
            .expect("re-admit B");
        assert_eq!(
            state.engram_count(),
            2,
            "dedup oracle rewound — B re-admits"
        );
        assert_eq!(
            real_sink.admissions_seen().len(),
            2,
            "real sink restored — observes the post-window admit"
        );
    }

    /// What this catches: new_rehydrated populates the engram Vec
    /// + the metadata DashMap from loaded snapshots. Subsequent
    /// recall_scored sees those engrams and uses the loaded
    /// salience values. The proof that boot rehydration works.
    #[test]
    pub(super) fn new_rehydrated_restores_engrams_and_metadata_for_recall() {
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        // Synthesize a couple of engrams + their metadata as if they
        // had been loaded from disk.
        let alpha_engram = synthetic_engram_with_chat_origin("alpha persisted");
        let beta_engram = synthetic_engram_with_chat_origin("beta persisted");
        let loaded_engrams = vec![alpha_engram.clone(), beta_engram.clone()];
        let high_salience = crate::persona::recall_metadata::RecallMetadata {
            salience: 0.9,
            access_count: 5,
            last_accessed_ms: 1_000,
            protected_until_ms: 0,
            last_decayed_ms: 1_000,
        };
        let low_salience = crate::persona::recall_metadata::RecallMetadata {
            salience: 0.2,
            access_count: 0,
            last_accessed_ms: 0,
            protected_until_ms: 0,
            last_decayed_ms: 1_000,
        };
        let loaded_metadata = vec![
            (alpha_engram.id, low_salience),
            (beta_engram.id, high_salience),
        ];

        let state = AdmissionState::new_rehydrated(
            Arc::clone(&registry),
            crate::persona::admission_persistence::NoopSink::arc(),
            loaded_engrams,
            loaded_metadata,
        );

        assert_eq!(state.engram_count(), 2, "engrams rehydrated into Vec");

        // recall_scored uses the loaded salience values — beta
        // (high salience) ranks before alpha (low salience).
        let scored = state.recall_scored(1_500, 8);
        assert_eq!(scored.len(), 2);
        assert_eq!(scored[0].0.id, beta_engram.id, "high-salience first");
        assert_eq!(scored[1].0.id, alpha_engram.id, "low-salience second");
    }

    /// What this catches: the phantom-engram-without-metadata case
    /// from Reviewer-2 BLOCK finding on PR #1519. If the engram-save
    /// succeeded but the metadata-save failed (the inline-comment
    /// claimed self-healing but the original code did NOT), the
    /// engram on disk would be permanently invisible to recall
    /// because `recall_scored` filter_maps engrams whose registry
    /// entry is missing. Slice B's fix: at rehydrate, seed
    /// `admit_with_defaults` for any loaded engram lacking metadata.
    ///
    /// This test simulates the crash by handing `new_rehydrated` a
    /// Vec with N engrams + a metadata vec with only K < N entries.
    /// The post-fix invariant: every loaded engram is recall-visible
    /// (with default metadata if its row was missing).
    // what this catches: set_recall_salience (#166) down-weights an admitted
    // engram's recall salience WITHOUT clobbering its other metadata — critically
    // last_decayed_ms, whose loss would trigger the epoch-delta decay collapse.
    #[test]
    pub(super) fn set_recall_salience_lowers_salience_and_preserves_decay_clock() {
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state = AdmissionState::new(registry.clone());
        let id = Uuid::new_v4();
        // Seed as an ordinary admission does (default 0.5 salience, decay clock set).
        registry.admit_with_defaults(id);
        let before = registry.get(id).expect("seeded");
        assert_eq!(before.salience, 0.5);
        assert!(before.last_decayed_ms > 0, "decay clock initialized");

        state.set_recall_salience(id, PROPRIOCEPTION_RECALL_SALIENCE_FOR_TEST);
        let after = registry.get(id).expect("still present");
        assert_eq!(after.salience, PROPRIOCEPTION_RECALL_SALIENCE_FOR_TEST);
        assert_eq!(
            after.last_decayed_ms, before.last_decayed_ms,
            "decay clock preserved — no epoch-delta collapse"
        );
    }

    pub(super) const PROPRIOCEPTION_RECALL_SALIENCE_FOR_TEST: f32 = 0.25;

    #[test]
    pub(super) fn rehydrate_backfills_metadata_for_phantom_engrams() {
        let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let phantom_engram = synthetic_engram_with_chat_origin("phantom no metadata row");
        let healthy_engram = synthetic_engram_with_chat_origin("healthy has metadata");
        let phantom_id = phantom_engram.id;
        let healthy_id = healthy_engram.id;

        // Only healthy has a metadata entry — simulates the crash
        // window between engram-save and metadata-save.
        let metadata_pair = (
            healthy_id,
            crate::persona::recall_metadata::RecallMetadata {
                salience: 0.8,
                access_count: 2,
                last_accessed_ms: 1_000,
                protected_until_ms: 0,
                last_decayed_ms: 1_000,
            },
        );

        let state = AdmissionState::new_rehydrated(
            Arc::clone(&registry),
            crate::persona::admission_persistence::NoopSink::arc(),
            vec![phantom_engram, healthy_engram],
            vec![metadata_pair],
        );

        assert_eq!(state.engram_count(), 2, "both engrams in Vec");

        // The phantom MUST appear in scored recall — with default
        // salience since its row was missing. Without the fix, this
        // assertion fails because the phantom is permanently
        // invisible to filter_map.
        let scored = state.recall_scored(2_000, 8);
        assert_eq!(
            scored.len(),
            2,
            "both engrams recall-visible after rehydration"
        );

        let scored_ids: std::collections::BTreeSet<Uuid> =
            scored.iter().map(|(e, _)| e.id).collect();
        assert!(
            scored_ids.contains(&phantom_id),
            "phantom must be recall-visible (default metadata seeded by backfill)"
        );
        assert!(
            scored_ids.contains(&healthy_id),
            "healthy must remain recall-visible"
        );

        // Healthy keeps its loaded high salience; phantom got the
        // default 0.5 from admit_with_defaults — healthy ranks higher.
        let healthy_score = scored.iter().find(|(e, _)| e.id == healthy_id).unwrap().1;
        let phantom_score = scored.iter().find(|(e, _)| e.id == phantom_id).unwrap().1;
        assert!(
            healthy_score > phantom_score,
            "loaded metadata wins over seeded default: healthy {} vs phantom {}",
            healthy_score,
            phantom_score
        );
    }

    /// What this catches: AdmissionState::for_persona opens a
    /// per-persona SQLite, admits land in it, and a fresh
    /// AdmissionState constructed from the SAME PersonaHome
    /// rehydrates those admissions. Round-trip via real disk +
    /// per-persona scoping in one test.
    #[tokio::test]
    pub(super) async fn for_persona_round_trips_admissions_via_per_persona_sqlite() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = crate::persona::home::PersonaHome::for_persona(tmp.path(), "Paige");

        // ── Lifetime 1: admit through the persona's home ────────
        let original_ids: Vec<Uuid> = {
            let registry = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
            let state = AdmissionState::for_persona(&home, Arc::clone(&registry))
                .await
                .expect("for_persona setup");

            let messages = ["paige learns alpha for real", "paige learns beta for real"];
            let mut ids = Vec::new();
            for content in &messages {
                let decision = state
                    .admit(&synthetic_human_message(content), None)
                    .expect("admit");
                match decision {
                    AdmissionDecision::Admit { engram, .. } => ids.push(engram.id),
                    other => panic!("expected Admit got {other:?}"),
                }
            }
            ids
            // state dropped here
        };

        // Wait for fire-and-forget writes to land.
        let mut tries = 0;
        let registry2 = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let state2 = loop {
            let st = AdmissionState::for_persona(&home, Arc::clone(&registry2))
                .await
                .expect("for_persona lifetime 2");
            if st.engram_count() == 2 {
                break st;
            }
            tries += 1;
            if tries > 100 {
                panic!("persistent engrams never rehydrated after 100 yields");
            }
            tokio::task::yield_now().await;
        };

        let scored = state2.recall_scored(10_000, 8);
        let scored_ids: std::collections::BTreeSet<Uuid> =
            scored.iter().map(|(e, _)| e.id).collect();
        let original_set: std::collections::BTreeSet<Uuid> = original_ids.iter().copied().collect();
        assert_eq!(scored_ids, original_set);
    }
