//! Tests for the kanban projection, moved out of `positron_kanban_source.rs` to keep it under
//! 1,000 lines (2026-10-24). Bodies keep their original indentation.
#![cfg(test)]

    use super::*;
    use crate::ipc::positron_source::{test_presence_payload, test_roster_slot};
    use airc_core::{PeerId, RoomId};
    use airc_work::{LaneId, RepoId, WorkCardId};
    use async_trait::async_trait;
    use continuum_positron::{Provenance, SenderKind};
    use serde_json::json;
    use std::sync::Mutex;

    /// A `WorkBoardReader` stub returning a canned board — lets the
    /// projection be driven without a daemon (mirrors
    /// `positron_wall_source::tests::StubReader`).
    struct StubReader {
        board: Mutex<BoardSnapshot>,
        fail: Mutex<bool>,
    }

    impl StubReader {
        fn new(cards: Vec<WorkCard>, lanes: Vec<LaneRecord>) -> Arc<Self> {
            Arc::new(Self {
                board: Mutex::new(board(cards, lanes)),
                fail: Mutex::new(false),
            })
        }
        /// Flip the reader into failure mode — the next `work_board()`
        /// returns a terminal-shaped `AircError`.
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl WorkBoardReader for StubReader {
        async fn work_board(&self) -> Result<BoardSnapshot, airc_lib::AircError> {
            if *self.fail.lock().unwrap() {
                return Err(airc_lib::AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.board.lock().unwrap().clone())
        }
    }

    /// An empty `BoardSnapshot` with the given cards/lanes — the other
    /// (workspace/PR/hygiene) collections stay empty, since the kanban view
    /// projects only cards + lanes.
    fn board(cards: Vec<WorkCard>, lanes: Vec<LaneRecord>) -> BoardSnapshot {
        BoardSnapshot {
            submission_reviews: Vec::new(),
            cards,
            lanes,
            workspaces: Vec::new(),
            repo_tracking: Vec::new(),
            pull_requests: Vec::new(),
            manager_hats: Vec::new(),
            agent_availability: Vec::new(),
            hygiene_reports: Vec::new(),
        }
    }

    fn repo() -> RepoId {
        RepoId::new("CambrianTech/continuum").expect("valid repo id")
    }

    fn card(
        room: RoomId,
        creator: PeerId,
        title: &str,
        state: CardState,
        owner: Option<PeerId>,
    ) -> WorkCard {
        let _ = room; // cards carry repo, not room_id; room comes from the projector
        WorkCard {
            card_id: WorkCardId::new(),
            repo: repo(),
            title: title.to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state,
            owner,
            claim_provenance: None,
            claim_id: None,
            claim_expires_at_ms: None,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: creator,
            created_at_ms: 1_700_000_000_000,
            updated_at_ms: 1_700_000_000_000,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        }
    }

    fn lane(title: &str, state: LaneState, card_ids: Vec<WorkCardId>) -> LaneRecord {
        LaneRecord {
            lane_id: LaneId::new(),
            repo: repo(),
            title: title.to_string(),
            state,
            card_ids,
            created_by: PeerId::new(),
            created_at_ms: 1_700_000_000_000,
            updated_at_ms: 1_700_000_000_000,
        }
    }

    fn presence_one(room: Uuid, member: Uuid, name: &str, kind: &str) -> serde_json::Value {
        // Serialize the REAL typed slot via the shared helper — never a
        // hand-authored JSON literal, so the test wire can't drift from
        // `RosterSlotView`'s field names.
        let kind: SenderKind = serde_json::from_value(json!({ "kind": kind })).unwrap();
        let mut integrations = std::collections::BTreeMap::new();
        integrations.insert("continuum.persona_id".to_string(), "asha-1".to_string());
        test_presence_payload(
            room,
            vec![RosterSlotView {
                integrations,
                provenance: Provenance {
                    runtime: "claude".to_string(),
                },
                ..test_roster_slot(member, name, kind)
            }],
        )
    }

    fn current_kanban(substrate: &Substrate) -> KanbanViewState {
        let env = substrate
            .cache()
            .get(KanbanViewState::KIND)
            .expect("a kanban envelope must be stored");
        serde_json::from_value(env.payload.clone()).expect("payload is a KanbanViewState")
    }

    // what this catches (one board, ONE-BOARD-THROUGH-POSITRON.md slice 1): a card's verdict
    // reaching the board as a bare outcome, or the wrong review. The projector drops nothing of
    // the fold: each card carries the LATEST review of its LATEST submission, with the reviewer's
    // finding as written, and an older submission's review never stands in for it.
    #[tokio::test]
    async fn a_card_carries_the_latest_review_of_its_latest_submission_with_its_finding() {
        let substrate = Substrate::new();
        let room = RoomId::new();
        let owner = PeerId::new();
        let reviewer = PeerId::new();
        let mut reviewed = card(room, PeerId::new(), "Make the change", CardState::Review, Some(owner));
        reviewed.claim_id = Some(airc_work::ClaimId::new());
        let artifact: airc_work::SubmissionArtifact =
            serde_json::from_value(json!({ "hash": "a".repeat(64), "size_bytes": 5, "mime": "text/x-diff" }))
                .expect("test: artifact");
        let submission = |n: u64| airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::new(),
            card_id: reviewed.card_id,
            claim_id: reviewed.claim_id.expect("test: claimed"),
            instance: "ordinary".into(),
            base_sha: airc_work::GitObjectId::new("b".repeat(40)).expect("test: sha"),
            artifact: artifact.clone(),
            publisher: owner,
            submitted_at_ms: n,
        };
        let (latest, older) = (submission(20), submission(10));
        // `latest_submission` reads the first: airc lists a card's submissions newest first.
        reviewed.submissions = vec![latest.clone(), older.clone()];
        let review = |of: &airc_work::WorkSubmission, outcome, finding: &str, at: u64| airc_work::WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(),
            card_id: reviewed.card_id,
            submission_id: of.submission_id,
            artifact: artifact.clone(),
            review_card_id: WorkCardId::new(),
            review_claim_id: airc_work::ClaimId::new(),
            reviewer,
            outcome,
            evidence: artifact.clone(),
            evidence_text: Some(finding.into()),
            reviewed_at_ms: at,
        };
        let unreviewed = card(room, PeerId::new(), "Nothing submitted", CardState::Open, None);
        let mut snapshot = board(vec![reviewed.clone(), unreviewed], vec![]);
        snapshot.submission_reviews = vec![
            review(&older, airc_work::WorkReviewOutcome::Passed, "the older attempt", 90),
            review(&latest, airc_work::WorkReviewOutcome::Passed, "superseded verdict", 30),
            review(&latest, airc_work::WorkReviewOutcome::Failed, "execute() checks only the id", 40),
        ];
        let reader = Arc::new(StubReader { board: Mutex::new(snapshot), fail: Mutex::new(false) });
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;

        let view = current_kanban(&substrate);
        let carried = view.cards.iter().find(|c| c.card_id == reviewed.card_id.as_uuid()).expect("test: card");
        let verdict = carried.review.as_ref().expect("the reviewed card carries its verdict");
        assert_eq!(verdict.submission_id, latest.submission_id.as_uuid(), "the latest submission's review, never the older one's");
        assert_eq!(verdict.outcome, KanbanReviewOutcome::Failed, "the latest review of it");
        assert_eq!(verdict.finding.as_deref(), Some("execute() checks only the id"));
        assert_eq!(verdict.reviewer_id, reviewer.as_uuid());
        assert!(view.cards.iter().any(|c| c.title == "Nothing submitted" && c.review.is_none()));
    }

    #[tokio::test]
    async fn reload_projects_the_board_into_the_substrate() {
        // what this catches: regression where a board re-read does not reach
        // the substrate as a KanbanViewState — the whole point of the kanban
        // projection. With no presence yet, the creator renders provisionally
        // (short peer-id label, neutral Human) — the honest pending-truth
        // state, never a fabricated author. Mirrors
        // positron_wall_source::tests::reload_projects_the_board_into_the_substrate.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(
            room,
            creator,
            "Wire the kanban projector",
            CardState::Open,
            None,
        );
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;

        let view = current_kanban(&substrate);
        assert_eq!(view.room_id, room.as_uuid());
        assert_eq!(view.cards.len(), 1);
        assert_eq!(view.cards[0].title, "Wire the kanban projector");
        assert_eq!(view.cards[0].state, KanbanCardState::Open);
        assert_eq!(view.cards[0].creator_id, creator.as_uuid());
        assert_eq!(view.cards[0].creator_kind, SenderKind::Human);
        assert!(
            view.cards[0].creator_name.starts_with("peer-"),
            "provisional creator label until presence, got {}",
            view.cards[0].creator_name
        );
        assert!(view.cards[0].integrations.is_empty());
        // Open, unclaimed card: no assignee, no PR.
        assert!(view.cards[0].assignee_id.is_none());
        assert!(view.cards[0].assignee_name.is_none());
        assert!(view.cards[0].pull_request.is_none());
    }

    #[tokio::test]
    async fn creator_upgrades_in_place_when_presence_folds_the_card() {
        // what this catches: the attribution-woven-in contract — a card filed
        // before its author's presence card arrives renders provisionally,
        // then UPGRADES (name + kind + badges + provenance) the instant
        // presence folds the card. A regression that dropped the re-project on
        // presence would leave the creator stuck as "peer-xxxx"/Human forever,
        // unattributable. [[positron-identity-security-first-class]]. Mirrors
        // the wall projector's author-upgrade test.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "File the card", CardState::Open, None);
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        assert_eq!(
            current_kanban(&substrate).cards[0].creator_kind,
            SenderKind::Human
        );

        // Card arrives via presence: Agent named Asha carrying a badge.
        let presence = presence_one(room.as_uuid(), creator.as_uuid(), "Asha", "agent");
        match classify(PRESENCE_UPDATED, &presence).expect("presence classifies") {
            KanbanInput::Presence(rid, roster) => {
                assert_eq!(rid, room.as_uuid());
                p.apply_roster(roster);
            }
            _ => panic!("presence:updated must classify as Presence"),
        }
        let view = current_kanban(&substrate);
        assert_eq!(view.cards[0].creator_name, "Asha");
        assert_eq!(view.cards[0].creator_kind, SenderKind::Agent);
        assert_eq!(
            view.cards[0]
                .integrations
                .get("continuum.persona_id")
                .map(String::as_str),
            Some("asha-1"),
            "opaque badge resolved from the card"
        );
        assert_eq!(
            view.cards[0].provenance.runtime, "claude",
            "accountability provenance resolved from the card"
        );
    }

    #[tokio::test]
    async fn claimed_card_resolves_assignee_name_from_roster() {
        // what this catches: the assignee (owner) status-pointer contract — a
        // claimed card carries a resolved assignee id + display name (from the
        // roster), distinct from the richer creator identity. A regression
        // that dropped assignee resolution, or filled it for an unclaimed
        // card, would mislabel who's working a card. The assignee is resolved
        // to NAME only (a status pointer), not the full creator identity axes.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let owner = PeerId::new();
        let c = card(room, creator, "Claim me", CardState::Claimed, Some(owner));
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        // Roster knows the owner as "BigMama".
        let presence = presence_one(room.as_uuid(), owner.as_uuid(), "BigMama", "agent");
        if let KanbanInput::Presence(_, roster) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_roster(roster);
        }
        let view = current_kanban(&substrate);
        assert_eq!(view.cards[0].state, KanbanCardState::Claimed);
        assert_eq!(view.cards[0].assignee_id, Some(owner.as_uuid()));
        assert_eq!(view.cards[0].assignee_name.as_deref(), Some("BigMama"));
    }

    #[tokio::test]
    async fn a_lapsed_lease_projects_as_takeable_even_though_the_column_still_says_claimed() {
        // what this catches: the human board's half of the stale-lease defect.
        // A claim is a LEASE; when it expires the holder stopped and the card
        // is takeable — but `state` still reads Claimed, so a view carrying
        // only the column shows dead work as active work. Measured 2026-08-06:
        // 17 of 19 leases expired and six citizens reported nothing to do.
        // The renderer can only grey a lapsed hold if the substrate SAYS it,
        // and this asserts the substrate says it — through the SAME predicate
        // the persona's board line uses, so the two cannot disagree.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let owner = PeerId::new();

        let mut live = card(
            room,
            PeerId::new(),
            "Live hold",
            CardState::Claimed,
            Some(owner),
        );
        live.claim_id = Some(airc_work::ClaimId::from_uuid(Uuid::new_v4()));
        live.claim_expires_at_ms = Some(u64::MAX);

        let mut lapsed = card(
            room,
            PeerId::new(),
            "Lapsed hold",
            CardState::Claimed,
            Some(owner),
        );
        lapsed.claim_id = Some(airc_work::ClaimId::from_uuid(Uuid::new_v4()));
        lapsed.claim_expires_at_ms = Some(1_000_000); // 1970-adjacent — long expired

        let open = card(room, PeerId::new(), "Nobody's", CardState::Open, None);

        let reader = StubReader::new(vec![live, lapsed, open], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        let view = current_kanban(&substrate);

        // The column says Claimed for BOTH held cards — that is exactly why the
        // column alone cannot carry the fact.
        assert_eq!(view.cards[0].state, KanbanCardState::Claimed);
        assert_eq!(view.cards[1].state, KanbanCardState::Claimed);
        // The hold tells them apart.
        assert_eq!(view.cards[0].hold, KanbanHold::Held);
        assert_eq!(view.cards[1].hold, KanbanHold::Lapsed);
        assert_eq!(view.cards[2].hold, KanbanHold::Unclaimed);
        // And the lapsed card still names who held it, so the renderer can say
        // whom to ask rather than who is busy.
        assert_eq!(view.cards[1].assignee_id, Some(owner.as_uuid()));
    }

    #[tokio::test]
    async fn lane_projects_with_state_and_card_ids() {
        // what this catches: the lane projection — a lane's id, title, mapped
        // state, and card_ids must reach the view so a renderer can group
        // cards into swimlanes. A regression in the LaneRecord→KanbanLaneView
        // map (dropped card_ids, wrong state) would break swimlane grouping.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "Laned card", CardState::InProgress, None);
        let card_id = c.card_id;
        let l = lane("positron define-once", LaneState::Active, vec![card_id]);
        let lane_id = l.lane_id;
        let reader = StubReader::new(vec![c], vec![l]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;

        let view = current_kanban(&substrate);
        assert_eq!(view.lanes.len(), 1);
        assert_eq!(view.lanes[0].lane_id, lane_id.as_uuid());
        assert_eq!(view.lanes[0].title, "positron define-once");
        assert_eq!(view.lanes[0].state, KanbanLaneState::Active);
        assert_eq!(view.lanes[0].card_ids, vec![card_id.as_uuid()]);
    }

    #[tokio::test]
    async fn empty_board_is_projected_not_an_error() {
        // what this catches: a room with no work projects an empty board (not
        // a skipped store, not an error) — the honest-empty contract the
        // KanbanViewState type documents. A renderer subscribed to kind=kanban
        // must still receive a snapshot so it can clear a stale board. Mirrors
        // the wall projector's empty-board test.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let reader = StubReader::new(vec![], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        let view = current_kanban(&substrate);
        assert_eq!(view.room_id, room.as_uuid());
        assert!(view.cards.is_empty());
        assert!(view.lanes.is_empty());
    }

    #[test]
    fn foreign_room_and_malformed_events_are_skipped() {
        // what this catches: regression where a non-kanban event, or a
        // kanban:changed missing the room_id, gets classified into a spurious
        // re-read. Per [[fallbacks-are-illegal-fail-loud]]: an event that
        // can't deserialize into the contract is not-a-kanban-event → None.
        // (Room-mismatch filtering is enforced in the loop, tested via the
        // room_id the Changed/Presence inputs carry.)
        assert!(classify("media:frame", &json!({ "bytes": 4 })).is_none());
        assert!(classify(KANBAN_CHANGED, &json!({ "nope": true })).is_none());
        let room = Uuid::from_u128(0xa);
        match classify(KANBAN_CHANGED, &json!({ "roomId": room })).expect("classifies") {
            KanbanInput::Changed(rid) => assert_eq!(rid, room),
            _ => panic!("kanban:changed must classify as Changed"),
        }
    }

    #[tokio::test]
    async fn revision_advances_monotonically_across_stores() {
        // what this catches: regression where successive kanban projections
        // stamp a stale or non-monotonic revision — the session-protocol
        // last_seen replay routes by revision, so a flat revision would stop
        // the kanban widget from seeing updates. Mirrors the wall projector's
        // revision test.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "v1", CardState::Open, None);
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader);
        p.reload().await;
        let r1 = substrate
            .cache()
            .get(KanbanViewState::KIND)
            .unwrap()
            .revision;
        // A presence fold re-projects → a second store, revision advances.
        let presence = presence_one(room.as_uuid(), creator.as_uuid(), "Asha", "agent");
        if let KanbanInput::Presence(_, roster) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_roster(roster);
        }
        let r2 = substrate
            .cache()
            .get(KanbanViewState::KIND)
            .unwrap()
            .revision;
        assert!(r2 > r1, "revision must advance: {r1:?} -> {r2:?}");
    }

    #[tokio::test]
    async fn read_error_keeps_the_last_good_board() {
        // what this catches: the projector's DISTINCT resilience contract — a
        // `work_board()` read failure must keep the last-good board on the
        // widget, never blink it empty. A regression that cleared cards/lanes
        // on error, or stored an empty view, would flash the board blank on
        // every transient airc hiccup ([[fallbacks-are-illegal-fail-loud]]:
        // resilience, never a fabricated empty substitute). Mirrors the wall
        // projector's last-good-board test.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let c = card(room, creator, "v1", CardState::Open, None);
        let reader = StubReader::new(vec![c], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader.clone());
        p.reload().await;
        assert_eq!(current_kanban(&substrate).cards.len(), 1);

        // The reader goes down mid-stream; the re-read fails.
        reader.set_fail(true);
        p.reload().await;

        let view = current_kanban(&substrate);
        assert_eq!(view.cards.len(), 1, "last-good board survives a read error");
        assert_eq!(view.cards[0].title, "v1");
    }

    #[tokio::test]
    async fn lagged_re_reads_and_closed_stops_and_foreign_room_ignored() {
        // what this catches: three otherwise trust-me loop-guard branches (the
        // loop is an infinite recv). (1) On `Lagged` (fell behind the buffer)
        // it RE-READS the authoritative board rather than continuing on a
        // stale one. (2) A `kanban:changed` for a DIFFERENT room is NOT
        // re-read — the defensive room guard drops it so a foreign room's
        // churn can't thrash this projector's board. (3) A `Closed` bus stops
        // the loop (Stop) — no spin. Drives the exact loop decisions via
        // `fold_recv` (no racing a live channel). Mirrors the wall projector's
        // fold_recv guard tests.
        let substrate = Substrate::new();
        let room = RoomId::new();
        let creator = PeerId::new();
        let reader = StubReader::new(vec![], vec![]);
        let mut p = KanbanProjection::new(substrate.clone(), room.as_uuid(), reader.clone());
        p.reload().await;
        assert!(current_kanban(&substrate).cards.is_empty());

        // Board gains a card while the loop was behind; a Lagged arrives.
        reader.board.lock().unwrap().cards.push(card(
            room,
            creator,
            "caught up",
            CardState::Open,
            None,
        ));
        let step = fold_recv(&mut p, room.as_uuid(), Err(RecvError::Lagged(3))).await;
        assert_eq!(step, LoopStep::Continue);
        assert_eq!(
            current_kanban(&substrate).cards.len(),
            1,
            "Lagged must trigger an authoritative re-read, not a stale continue"
        );

        // A change for some OTHER room must not re-read our board.
        let other = Uuid::from_u128(0xdead);
        let foreign = BusEvent {
            name: KANBAN_CHANGED.to_string(),
            payload: std::sync::Arc::new(json!({ "roomId": other })),
        };
        reader.board.lock().unwrap().cards.clear();
        let step = fold_recv(&mut p, room.as_uuid(), Ok(foreign)).await;
        assert_eq!(step, LoopStep::Continue);
        assert_eq!(
            current_kanban(&substrate).cards.len(),
            1,
            "a foreign room's kanban:changed must not re-read our board"
        );

        // A closed bus stops the loop.
        let step = fold_recv(&mut p, room.as_uuid(), Err(RecvError::Closed)).await;
        assert_eq!(step, LoopStep::Stop);
    }
