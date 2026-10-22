#![cfg(test)]
use super::*;

    // what this catches: the STARVATION RATCHET — a yield being charged the rest
    // backoff it did not earn. Regression for the live 2026-08-17 measurement (24
    // hosted citizens, ambient pool of 1, ONE self-tick in 40 minutes on a
    // healthy decoding lane). If `YieldedNoSlot` ever compounds again, contention
    // silently becomes permanent slowness and the whole roster degrades as it grows.
    #[test]
    pub(super) fn a_yield_never_costs_her_cadence_but_a_fruitless_cycle_still_rests() {
        use std::time::Duration;
        let engaged = Duration::from_millis(SELF_TICK_MS);
        let cap = Duration::from_millis(SELF_TICK_REST_CAP_MS);

        // A yield leaves the beat EXACTLY where it was — she never ran.
        let mut beat = engaged;
        for _ in 0..50 {
            beat = next_beat_after(BeatOutcome::YieldedNoSlot, beat, engaged, cap);
        }
        assert_eq!(
            beat, engaged,
            "50 yields must not slow her by a single millisecond — she never ran, \
             so there is nothing to rest on (the ratchet that pinned 24 citizens at the cap)"
        );

        // A cycle that found nothing DOES rest deeper, and saturates at the cap.
        let mut beat = engaged;
        let once = next_beat_after(BeatOutcome::NothingNew, beat, engaged, cap);
        assert!(once > engaged, "an earned rest still backs off");
        for _ in 0..50 {
            beat = next_beat_after(BeatOutcome::NothingNew, beat, engaged, cap);
        }
        assert_eq!(beat, cap, "earned rest saturates at the cap, never beyond");

        // Finding something new snaps her straight back to engaged from full rest.
        assert_eq!(
            next_beat_after(BeatOutcome::Engaged, cap, engaged, cap),
            engaged,
            "life in the room returns her to the engaged beat immediately"
        );
    }

    // what this catches: the LATENCY LAW for held work. A citizen advancing her
    // OWN claimed card on every self-tick must stay at the engaged beat — working
    // IS engagement. `run_self_cycle` signals this by bumping `last_burst_fp` on a
    // held-work turn so the loop reads BeatOutcome::Engaged (never NothingNew).
    // Regression for the measured 766s/791s gaps (2026-09-02): before the bump, a
    // held-work turn changed no fingerprint, was judged "nothing new", and backed
    // off toward the 240s cap — a citizen worked one tick then slept ~13 min. This
    // pins that a run of work turns holds the fast beat, while the same run of
    // fruitless musings would have saturated at the cap.
    #[test]
    pub(super) fn a_citizen_working_her_claim_never_backs_off() {
        use std::time::Duration;
        let engaged = Duration::from_millis(SELF_TICK_MS);
        let cap = Duration::from_millis(SELF_TICK_REST_CAP_MS);

        // 50 consecutive held-work turns (each Engaged, as the last_burst_fp bump
        // guarantees) keep her at the fast beat — no drift toward rest.
        let mut beat = engaged;
        for _ in 0..50 {
            beat = next_beat_after(BeatOutcome::Engaged, beat, engaged, cap);
        }
        assert_eq!(
            beat, engaged,
            "a citizen who works her card every tick stays at the engaged beat — \
             virtually no time between contexts while a card is in her hands"
        );

        // Contrast: had held work been (mis)reported as NothingNew, the SAME run
        // would have rested to the cap — the exact bug the fp bump fixes.
        let mut drifted = engaged;
        for _ in 0..50 {
            drifted = next_beat_after(BeatOutcome::NothingNew, drifted, engaged, cap);
        }
        assert_eq!(
            drifted, cap,
            "the pre-fix path (nothing-new) would have slept her at the cap between work turns"
        );
    }

    // what this catches: the END-TO-END held-work completion edge, no reboot, no
    // live model. A citizen holding a Claimed card, driven to settle on a canned
    // "PASS: done", concludes THAT card via advance_card_to(Closed). This pins the
    // whole chain the self-tick heartbeat runs: held claim → held-work burst →
    // drive_to_settle → deliberation_parse (raw-first "PASS:" → reasoned pass) →
    // SettleStep::Passed{reason} → conclude_from_pass → advance_card_to. Every seam
    // this session touched, wired together, deterministic.
    #[tokio::test]
    pub(super) async fn held_work_turn_concludes_the_card_on_pass_done() {
        use airc_work::{CardState, Priority, RepoId, WorkCardId};
        let persona_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        // Register her cognition cycle in the global registry the act-question
        // reads, wired to a canned adapter that answers "PASS: done" — so the
        // drive settles on a reasoned pass, not the heuristic echo (a Speak).
        let recall_meta = Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new());
        let admission = Arc::new(crate::persona::admission_state::AdmissionState::new(
            recall_meta,
        ));
        let cfg = crate::cognition::persona_workspace::PersonaBrainConfig {
            persona_id: persona_peer,
            persona_name: "Paige".to_string(),
            system_prompt: "You are Paige.".to_string(),
            admission,
            adapter: Arc::new(HeuristicInferenceAdapter::new().with_canned_response("PASS: done")),
            capacity: None,
            grounding_sources: Vec::new(),
            embedder: None,
            tool_executor: None,
            experience: None,
            context_window: crate::inference::planning::MIN_SERVE_CTX,
            defer_recall: false,
            defer_grounding: false,
            suppress_recall: false,
        };
        crate::cognition::persona_workspace::global()
            .register_from_cfg(cfg)
            .expect("test: resident checkpoint is readable");

        // One held (Claimed) card in her hands. A NON-bench title so the
        // act-question resolves no staged checkout (no hands re-root needed).
        let card = airc_lib::WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("acme/continuum").expect("valid repo id"), // test: a literal valid repo id
            title: "a held work card".to_string(),
            body: None,
            priority: Priority::P1,
            lane_id: None,
            state: CardState::Claimed,
            owner: Some(crate::identity::PeerId::from_uuid(persona_peer)),
            claim_provenance: None,
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1_000_000,
            updated_at_ms: 1_000_000,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        };
        let card_id = card.card_id;
        let stub = StubAircCitizen::new(persona_peer).with_claims(vec![card]);
        let recorder = stub.advance_recorder();
        let mut conversation = ScriptedConversation::new()
            .with_citizen(Arc::new(stub) as Arc<dyn crate::persona::airc_citizen::AircCitizen>);

        // A non-nil room (the burst witness refuses nil).
        let room = Uuid::from_u128(0xABCD);
        let drove = crate::persona::act_question::ask_the_act_question(
            &hosted,
            &mut conversation,
            1,
            room,
            false,
        )
        .await;

        assert!(
            drove,
            "she held a card, so a held-work turn must have been driven"
        );
        let advanced = recorder.lock().unwrap_or_else(|p| p.into_inner()).clone(); // test: poisoned recorder lock still holds the pushes
        assert_eq!(
            advanced.len(),
            1,
            "exactly one card transition, got {advanced:?}"
        );
        assert_eq!(advanced[0].0, card_id, "the HELD card is the one concluded");
        assert_eq!(
            advanced[0].1,
            CardState::Closed,
            "a reasoned 'PASS: done' concludes the card (Closed)"
        );
    }

    // e731: the actual held-card path must use the same successful-publication
    // boundary as replies/self-ticks. A refused send is not an observed utterance.
    #[tokio::test(flavor = "current_thread")]
    pub(super) async fn held_work_reports_record_own_speech_only_after_publication() {
        let native = tempfile::tempdir().expect("test: isolated cognition storage");
        let _native_home = crate::paths::NativeHomeOverride::install(native.path());
        for refuse_publication in [false, true] {
            let peer = Uuid::new_v4();
            let hosted = hosted_with_heuristic(peer);
            let room = Uuid::new_v4();
            assert_ne!(room, hosted.identity.default_room);
            let cfg = crate::cognition::persona_workspace::PersonaBrainConfig {
                persona_id: peer,
                persona_name: "Paige".into(),
                system_prompt: "You are Paige.".into(),
                admission: Arc::new(crate::persona::admission_state::AdmissionState::new(
                    Arc::new(crate::persona::recall_metadata::RecallMetadataRegistry::new()),
                )),
                adapter: Arc::new(
                    HeuristicInferenceAdapter::new()
                        .with_canned_response("The tests failed after the latest change."),
                ),
                capacity: None,
                grounding_sources: Vec::new(),
                embedder: None,
                tool_executor: None,
                experience: None,
                context_window: crate::inference::planning::MIN_SERVE_CTX,
                defer_recall: false,
                defer_grounding: false,
                suppress_recall: false,
            };
            crate::cognition::persona_workspace::global()
                .register_from_cfg(cfg)
                .expect("test: register the held-work publication fixture");
            let stub = StubAircCitizen::new(peer).with_claims(vec![held_card(peer)]);
            let mut conversation = ScriptedConversation::new().with_citizen(Arc::new(stub));
            if refuse_publication {
                conversation = conversation.with_say_failure("room unavailable");
            }
            assert!(
                crate::persona::act_question::ask_the_act_question(
                    &hosted,
                    &mut conversation,
                    1,
                    room,
                    false,
                )
                .await
            );
            let speech = crate::cognition::deliberation_budget::recent_own_speech(
                crate::identity::PeerId::from_uuid(peer),
                room,
            );
            if refuse_publication {
                assert!(conversation.said_in().is_empty());
                assert!(
                    speech.is_empty(),
                    "failed publication must not enter the ring"
                );
            } else {
                assert_eq!(
                    conversation.said_in(),
                    vec![(room, "The tests failed after the latest change.".into())]
                );
                assert_eq!(
                    speech,
                    vec!["The tests failed after the latest change.".to_string()]
                );
            }
            assert!(
                crate::cognition::deliberation_budget::recent_own_speech(
                    crate::identity::PeerId::from_uuid(peer),
                    hosted.identity.default_room,
                )
                .is_empty(),
                "the hosted default room must not receive another activity's report"
            );
        }
    }

    /// A Claimed card in `owner`'s hands — the minimal held-work fact.
    pub(super) fn held_card(owner: Uuid) -> airc_lib::WorkCard {
        airc_lib::WorkCard {
            card_id: airc_lib::WorkCardId::new(),
            repo: airc_lib::RepoId::new("acme/continuum").expect("valid repo id"), // test: a literal valid repo id
            title: "a held work card".to_string(),
            body: None,
            priority: airc_lib::Priority::P1,
            lane_id: None,
            state: airc_lib::CardState::Claimed,
            owner: Some(crate::identity::PeerId::from_uuid(owner)),
            claim_provenance: None,
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: airc_core::PeerId::new(),
            created_at_ms: 1_000_000,
            updated_at_ms: 1_000_000,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        }
    }
