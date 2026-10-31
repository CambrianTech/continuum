#![cfg(test)]
use super::*;

    // what this catches: a fruitless cycle that never rests, or a rest that never ends: a
    // musing that found nothing backs off to the cap and saturates there, and finding
    // something new snaps her back to the engaged beat.
    #[test]
    pub(super) fn a_fruitless_cycle_rests_to_the_cap_and_news_wakes_her() {
        use std::time::Duration;
        let engaged = Duration::from_millis(SELF_TICK_MS);
        let cap = Duration::from_millis(SELF_TICK_REST_CAP_MS);

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
        let recall_meta = Arc::new(crate::cognition::recall_metadata::RecallMetadataRegistry::new());
        let admission = Arc::new(crate::cognition::admission_state::AdmissionState::new(
            recall_meta,
        ));
        let cfg = crate::cognition::persona_workspace::PersonaBrainConfig {
            persona_id: persona_peer,
            persona_name: "Paige".to_string(),
            system_prompt: "You are Paige.".to_string(),
            admission,
            adapter: Arc::new(HeuristicInferenceAdapter::new().with_canned_response("PASS: done")),
            grounding_sources: Vec::new(),
            embedder: None,
            tool_executor: None,
            experience: None,
            context_window: crate::inference::planning::MIN_SERVE_CTX,
            defer_recall: false,
            defer_grounding: false,
            suppress_recall: false,
            forked: false,
            read_receipt: None,
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
        // Offered, on purpose: a card in her hands takes the held-work turn exactly as
        // before, whatever the cadence offers (her initiative is for holding nothing).
        let drove = crate::persona::act_question::ask_the_act_question(
            &hosted,
            &mut conversation,
            1,
            room,
            false,
            crate::persona::initiative_turn::Initiative::Offered,
        )
        .await;

        assert!(
        drove.drove,
            "she held a card, so a held-work turn must have been driven"
        );
        assert!(drove.engaged, "held work is always engagement: her beat stays engaged");
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
                admission: Arc::new(crate::cognition::admission_state::AdmissionState::new(
                    Arc::new(crate::cognition::recall_metadata::RecallMetadataRegistry::new()),
                )),
                adapter: Arc::new(
                    HeuristicInferenceAdapter::new()
                        .with_canned_response("The tests failed after the latest change."),
                ),
                grounding_sources: Vec::new(),
                embedder: None,
                tool_executor: None,
                experience: None,
                context_window: crate::inference::planning::MIN_SERVE_CTX,
                defer_recall: false,
                defer_grounding: false,
                suppress_recall: false,
                forked: false,
                read_receipt: None,
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
                    crate::persona::initiative_turn::Initiative::NotOffered,
                )
                .await
            .drove
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

    // what this catches (moved from cognition::handoff when the card bind moved up, mind split
    // step 7): a card whose hands cannot be bound must not clear the handoff's bound card.
    #[tokio::test]
    async fn a_failed_bind_keeps_the_bound_card_in_the_handoff() {
        use crate::cognition::handoff::{compose, note_held, note_unbound, TornBy};
        let me = Uuid::from_u128(1_073);
        let current = held_card(me);
        note_held(me, None, &current, 1_850);
        let cycle = crate::cognition::workspace::WorkspaceCycle::new(
            vec![],
            Arc::new(crate::cognition::workspace::SalienceArbiter),
        );
        let mut unavailable = current.clone();
        unavailable.card_id = airc_lib::WorkCardId::new();
        let turn = crate::persona::card_hands::bind_card(&cycle, me, &unavailable).await;
        assert!(turn.hands.is_none(), "no checkout/hands can be bound by this fixture");
        let root = std::path::Path::new("current-checkout");
        let bound = compose(me, TornBy::Periodic, Some(root), None, 1_875).expect("bound").card;
        assert_eq!(bound, Some(current));
        note_unbound(me);
    }

// Actual wake selection stays reachable while original capacity custody waits;
// neither a new directed input nor Stop is hidden behind the retry timer.
#[tokio::test(start_paused = true)]
async fn capacity_park_keeps_new_input_and_stop_reachable() {
    use crate::cognition::act_observe::SettleStep;
    use crate::cognition::workspace::{PromptCapacityError, TurnAttention};
    use crate::persona::wake_backlog::PendingInput;
    let ctx = hosted_with_heuristic(Uuid::new_v4());
    let original = IncomingMessage {
        event_id: Uuid::new_v4(),
        lamport: 1,
        peer_id: Uuid::new_v4(),
        room_id: Uuid::new_v4(),
        text: "original required payload".into(),
        work: None,
        media: Vec::new(),
    };
    let mut newer = original.clone();
    newer.event_id = Uuid::new_v4();
    newer.text = "new directed input".into();
    let engram = Some(Uuid::new_v4());
    let mut pending = PendingInput::default();
    pending.settle(
        &original,
        engram,
        TurnAttention::HumanOpportunity,
        &SettleStep::CapacityRefused {
            error: PromptCapacityError {
                required_tokens: 8192,
                budget_tokens: 4096,
            }
            .into(),
            replay_input: true,
        },
    );
    let mut conversation =
        ScriptedConversation::new().with_events(vec![Ok(Some(newer.clone())), Ok(None)]);
    conversation.prime().await.unwrap();
    let mut outcome = ServeOutcome::default();
    let mut mind = MindWakeState::default();
    let opts = ServeOptions::default();
    let (wake, _) = await_wake(
        &ctx,
        &mut conversation,
        &mut outcome,
        &opts,
        &pending,
        &mut mind,
        std::time::Duration::from_secs(1),
    )
    .await;
    assert!(matches!(wake, Wake::Msg(m) if m == newer));
    let (wake, _) = await_wake(
        &ctx,
        &mut conversation,
        &mut outcome,
        &opts,
        &pending,
        &mut mind,
        std::time::Duration::from_secs(1),
    )
    .await;
    assert!(matches!(wake, Wake::Stop));
    assert_eq!(pending.message(), Some(&original));
    assert_eq!(pending.wake_engram_for(&original), engram);
}

// Board-only attention is perceived before trigger selection, even when it is
// not chat that would earn a turn. The existing custody owner carries readiness.
#[tokio::test(start_paused = true)]
async fn capacity_park_observes_semantic_board_change_before_trigger_filter() {
    use crate::cognition::act_observe::SettleStep;
    use crate::cognition::workspace::{PromptCapacityError, TurnAttention};
    use crate::persona::wake_backlog::PendingInput;
    use crate::cognition::conversation::seen_ids::SeenIds;
    let ctx = hosted_with_heuristic(Uuid::new_v4());
    let room = Uuid::new_v4();
    let original = IncomingMessage {
        event_id: Uuid::new_v4(), lamport: 1, peer_id: Uuid::new_v4(), room_id: room,
        text: "retained required input".into(), work: None, media: Vec::new(),
    };
    let refusal = SettleStep::CapacityRefused {
        error: PromptCapacityError { required_tokens: 8192, budget_tokens: 4096 }.into(),
        replay_input: true,
    };
    let mut pending = PendingInput::default();
    let engram = Some(Uuid::new_v4());
    let mut conversation = ScriptedConversation::new();
    conversation.prime().await.unwrap();
    let mut outcome = ServeOutcome::default();
    let mut seen = SeenIds::new(256);
    let mut water = 0;
    let card = Uuid::new_v4();
    use crate::cognition::conversation::room_event::RoomWork;
    let state = |state| RoomWork::StateChanged { card_id: card, state, by: original.peer_id };
    let submission = |id| RoomWork::Submitted {
        card_id: card, publisher: original.peer_id, submission: Some(id),
    };
    let first_submission = Uuid::new_v4();
    for (work, changed) in [
        (state(airc_work::CardState::Open), true),
        (state(airc_work::CardState::Open), false),
        (state(airc_work::CardState::Claimed), true),
        (state(airc_work::CardState::Open), true),
        (submission(first_submission), true),
        (submission(first_submission), false),
        (submission(Uuid::new_v4()), true),
    ] {
        pending.settle(&original, engram, TurnAttention::HumanOpportunity, &refusal);
        pending.settle_self(room, &refusal);
        let board = IncomingMessage {
            event_id: Uuid::new_v4(), lamport: water + 1, peer_id: original.peer_id,
            room_id: room, text: String::new(), media: Vec::new(),
            work: Some(work),
        };
        // Scripted end closes this drain; observing typed state must still happen
        // before the non-trigger fact is discarded by the existing selector.
        let _ = resync_to_now(&ctx, &mut conversation, &mut outcome, &mut seen,
            &mut water, &mut pending, false, board).await;
        let ready = tokio::time::timeout(std::time::Duration::from_secs(1), pending.ready_self_room()).await;
        assert_eq!(ready.is_ok(), changed, "clock/new event ID is not a payload change");
        if changed { assert_eq!(ready.unwrap(), room); }
        assert_eq!(pending.message(), Some(&original));
        assert_eq!(pending.wake_engram_for(&original), engram);
    }
}

#[tokio::test(start_paused = true)]
async fn capacity_ready_self_waits_without_spinning_during_quiescence() {
    use crate::cognition::act_observe::SettleStep;
    use crate::cognition::workspace::PromptCapacityError;
    use crate::persona::wake_backlog::PendingInput;
    let ctx = hosted_with_heuristic(Uuid::new_v4());
    let room = Uuid::new_v4();
    let input = IncomingMessage {
        event_id: Uuid::new_v4(), lamport: 1, peer_id: Uuid::new_v4(), room_id: room,
        text: "new required payload".into(), work: None, media: Vec::new(),
    };
    let mut pending = PendingInput::default();
    pending.settle_self(room, &SettleStep::CapacityRefused {
        error: PromptCapacityError { required_tokens: 8192, budget_tokens: 4096 }.into(),
        replay_input: false,
    });
    pending.payload_changed(&input);
    let opts = ServeOptions::default();
    opts.quiesced.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut conversation = ScriptedConversation::new().with_idle_inbox();
    let mut outcome = ServeOutcome::default();
    let mut mind = MindWakeState::default();
    let beat = std::time::Duration::from_secs(1);
    let start = tokio::time::Instant::now();
    for _ in 0..3 {
        let (wake, _) = await_wake(&ctx, &mut conversation, &mut outcome, &opts,
            &pending, &mut mind, beat).await;
        assert!(matches!(wake, Wake::Idle));
    }
    assert!(tokio::time::Instant::now().duration_since(start) >= beat * 3);
    opts.quiesced.store(false, std::sync::atomic::Ordering::Relaxed);
    let (wake, _) = await_wake(&ctx, &mut conversation, &mut outcome, &opts,
        &pending, &mut mind, beat).await;
    assert!(matches!(wake, Wake::Mind { room: Some(r), .. } if r == room));
    assert!(pending.take_ready_self(Some(room)));
    assert!(!pending.take_ready_self(Some(room)), "readiness consumed once");
    opts.quiesced.store(true, std::sync::atomic::Ordering::Relaxed);
    conversation = conversation.with_events(vec![Ok(None)]);
    let (wake, _) = await_wake(&ctx, &mut conversation, &mut outcome, &opts,
        &pending, &mut mind, beat).await;
    assert!(matches!(wake, Wake::Stop));
}

// what this catches: the constraint that kept residents from working (M5, 2026-10-23:
// four residents, ~7 acts and 0 writes an hour, every gate `no_held_work`). With nothing
// held, her own heartbeat must give her a turn with her hands; a message turn must not
// (the beat paces it), and a pass must not count as engagement (her beat rests). The
// turn claims, advances and says nothing on her behalf: anything it does is her act.
#[tokio::test(flavor = "current_thread")]
async fn holding_nothing_her_heartbeat_offers_an_initiative_turn_with_her_hands() {
    use crate::persona::initiative_turn::Initiative;
    let native = tempfile::tempdir().expect("test: isolated cognition storage");
    let _native_home = crate::paths::NativeHomeOverride::install(native.path());
    let peer = Uuid::new_v4();
    let hosted = hosted_with_heuristic(peer);
    let cfg = crate::cognition::persona_workspace::PersonaBrainConfig {
        persona_id: peer,
        persona_name: "Aris".into(),
        system_prompt: "You are Aris.".into(),
        admission: Arc::new(crate::cognition::admission_state::AdmissionState::new(Arc::new(
            crate::cognition::recall_metadata::RecallMetadataRegistry::new(),
        ))),
        adapter: Arc::new(
            HeuristicInferenceAdapter::new().with_canned_response("PASS: nothing calls me right now"),
        ),
        grounding_sources: Vec::new(),
        embedder: None,
        // Her hands: an initiative turn is offered only to a mind that can act.
        tool_executor: Some(Arc::new(
            crate::cognition::act_observe::test_support::ScriptedExecutor::new(["ok"]),
        )),
        experience: None,
        // Roomy enough for framing + tools + a reply: a refused prompt is not what this tests.
        context_window: 65_536,
        defer_recall: false,
        defer_grounding: false,
        suppress_recall: false,
        forked: false,
        read_receipt: None,
    };
    crate::cognition::persona_workspace::global()
        .register_from_cfg(cfg)
        .expect("test: register the initiative fixture");
    let room = Uuid::new_v4();
    for (offer, expect_turn) in [(Initiative::NotOffered, false), (Initiative::Offered, true)] {
        let stub = StubAircCitizen::new(peer); // holds nothing
        let advanced = stub.advance_recorder();
        let mut conversation = ScriptedConversation::new().with_citizen(Arc::new(stub));
        let outcome = crate::persona::act_question::ask_the_act_question(
            &hosted,
            &mut conversation,
            1,
            room,
            false,
            offer,
        )
        .await;
        assert_eq!(outcome.drove, expect_turn, "{offer:?}: an initiative turn iff her beat offered it");
        assert!(!outcome.engaged, "{offer:?}: a pass (or no turn) never holds her beat engaged");
        assert!(
            advanced.lock().unwrap_or_else(|p| p.into_inner()).is_empty(), // test: poisoned recorder lock still holds the pushes
            "{offer:?}: no card is moved on her behalf"
        );
        assert!(conversation.said_in().is_empty(), "{offer:?}: she passed, so nothing is said for her");
    }
}

    // what this catches: THE personaRag convergence seam, end to end through real
    // code (not hand-authored roster strings). A roster member in the room's ViewState
    // flows:
    //   ViewStateRagSource::<RosterViewState>.deliver  (the SAME RosterViewState the web
    //                              roster renders; one projection for screen and mind)
    //     → project_room_roster    (the heartbeat loop's own delivery fold, extracted)
    //       → prompt_assembly       (the [Present in this room] block)
    // and lands with the member's line. Connects the separately-unit-tested halves via
    // the exact path the live turn runs, deterministically and without airc presence.
    #[tokio::test]
    async fn present_member_reaches_present_in_room_block_end_to_end() {
        use crate::cognition::prompt_assembly::{assemble, HistoryMessage, PromptAssemblyInput};
        use crate::cognition::rag_budget::{RagContext, RagSource};
        use crate::model_registry::types::MultiPartyChatStrategy;
        use crate::persona::viewstate_rag::ViewStateRagSource;
        use crate::cognition::conversation::workspace_turns::project_room_roster;
        use continuum_positron::scoping::PerRoomSubstrates;
        use continuum_positron::{RosterViewState, SenderKind, StateBuilder};
        use std::sync::Arc;

        // One present peer in the room's OWN substrate, the shape production uses.
        let persona = uuid::Uuid::new_v4();
        let room = uuid::Uuid::new_v4();
        let mut slot = continuum_positron::chat::test_roster_slot(
            uuid::Uuid::new_v4(),
            "win-claude",
            SenderKind::Agent,
        );
        slot.role_label = Some("claude".to_string());
        let rooms = Arc::new(PerRoomSubstrates::new());
        rooms.for_room(room).store(StateBuilder::standalone().session(RosterViewState {
            room_id: room,
            roster: vec![slot],
        }));
        let source = ViewStateRagSource::<RosterViewState>::per_room(rooms);

        // 1. Real delivery from the shared roster projection.
        let ctx = RagContext::for_persona_in_room(persona, 1_000_000, room);
        let delivery = source.deliver(&ctx).await;

        // 2. Real loop fold: delivery → grounding consumers (the member line +
        //    the bare name), via the exact fn the heartbeat loop calls.
        let proj = project_room_roster(std::slice::from_ref(&delivery));
        assert_eq!(proj.room_roster.len(), 1);
        assert_eq!(
            proj.room_roster[0], "win-claude (Agent) — claude",
            "the member's line (kind and role; no liveness flag in the mind's copy)"
        );
        assert_eq!(proj.other_persona_names, vec!["win-claude".to_string()]);

        // 3. Real assembly: grounding lines → the [Present in this room] block.
        let input = PromptAssemblyInput {
            persona_name: "Asha".to_string(),
            system_prompt: "You are Asha.".to_string(),
            matched_angle: String::new(),
            history: vec![],
            current_message: HistoryMessage {
                role: "user".to_string(),
                name: None,
                content: "who is here?".to_string(),
                timestamp_ms: None,
            },
            is_voice: false,
            social_signals: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            other_persona_names: proj.other_persona_names,
            recalled_engrams: vec![],
            room_roster: proj.room_roster,
            room_doctrine: None,
        };
        let assembled = assemble(&input);
        assert!(
            assembled.system_message.contains("[Present in this room]"),
            "roster block missing from live-path assembly:\n{}",
            assembled.system_message
        );
        assert!(
            assembled
                .system_message
                .contains("win-claude (Agent) — claude"),
            "roster line did not reach the prompt:\n{}",
            assembled.system_message
        );
    }
