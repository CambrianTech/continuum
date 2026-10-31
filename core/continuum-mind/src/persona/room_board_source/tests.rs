#![cfg(test)]
    use super::*;
    use airc_work::{CardState, Priority, RepoId, WorkCard, WorkCardId};
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    fn card(title: &str, state: CardState, owner: Option<airc_core::PeerId>) -> WorkCard {
        // An OWNED fixture card carries a LIVE lease by default — matching how a
        // real claim exists on the board (claim_id + unexpired claim_expires_at_ms).
        // An owner with NO live lease is the LAPSED shape, built via
        // `lapse(card(...))` below.
        let claimed = owner.is_some();
        WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("acme/continuum").expect("valid repo id in fixture"),
            title: title.to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state,
            owner,
            claim_provenance: None,
            claim_id: claimed.then(|| airc_work::ClaimId::from_uuid(uuid::Uuid::new_v4())),
            claim_expires_at_ms: claimed.then(|| continuum_base::time::EpochMs::now().millis() + 60_000),
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

    /// Turn an owned fixture card into the stale-lease shape: claim still on the
    /// card, lease long expired — what the live board showed for all 17 claims
    /// on 2026-08-03.
    fn lapse(mut c: WorkCard) -> WorkCard {
        c.claim_expires_at_ms = Some(1_000_000); // 1970-adjacent — long expired
        c
    }

    /// Stamp a fixture card's creation time — the ordering key the available-work
    /// lead samples by, so a test can express "this one was dispatched just now".
    fn created_at(mut c: WorkCard, ms: u64) -> WorkCard {
        c.created_at_ms = ms;
        c
    }

    // what this catches: a card dispatched MINUTES AGO being structurally unshowable.
    // Only five titles are ever rendered; taken in board order a fresh card lands at the
    // end and never appears, so the citizen reads an honest "N claimable" above five
    // titles that are all old. Measured live 2026-08-07: three freshly dispatched bench
    // cards sat in every citizen's board cache and in NONE of their prompt windows while
    // the count was correct throughout — the count was never the lie, the sample was.
    #[tokio::test]
    async fn freshly_dispatched_work_is_shown_not_merely_counted() {
        let mut cards: Vec<WorkCard> = (0..8)
            .map(|i| {
                created_at(
                    card(&format!("old backlog card {i}"), CardState::Open, None),
                    1_000_000 + i as u64,
                )
            })
            .collect();
        // Dispatched just now, and LAST in board order — the exact live shape.
        cards.push(created_at(
            card("bench coder-write-eval: sum_evens", CardState::Open, None),
            9_000_000_000,
        ));

        let reader = Arc::new(StubReader::new(snapshot(cards)));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        let lead = delivery
            .items
            .iter()
            .find(|i| i.metadata["kind"] == "available-work-lead")
            .expect("available-work lead");

        assert!(
            lead.content.contains("bench coder-write-eval"),
            "the just-dispatched card must be VISIBLE, not just counted:\n{}",
            lead.content
        );
        // The count stays honest — it always was. This fix is about the sample.
        assert_eq!(lead.metadata["open_count"], 9);
    }

    fn snapshot(cards: Vec<WorkCard>) -> BoardSnapshot {
        BoardSnapshot {
            cards,
            submission_reviews: Vec::new(),
            lanes: Vec::new(),
            workspaces: Vec::new(),
            repo_tracking: Vec::new(),
            pull_requests: Vec::new(),
            manager_hats: Vec::new(),
            agent_availability: Vec::new(),
            hygiene_reports: Vec::new(),
        }
    }

    struct StubReader {
        board: BoardSnapshot,
        /// When seeded, model the production reader's subscription boundary.
        board_room: Option<uuid::Uuid>,
        fail: Mutex<bool>,
        /// Published aliases the stub knows, standing in for airc's durable
        /// alias store. Empty by default — an owner with no published name
        /// renders as its short id, the honest degradation.
        names: std::collections::HashMap<airc_core::PeerId, String>,
        /// The `room` param of the LAST `work_board` call — how the
        /// turn-parametric tests (#443) assert WHICH room's board was read,
        /// since the stub itself serves one board regardless.
        last_room: Mutex<Option<Option<uuid::Uuid>>>,
    }

    impl StubReader {
        fn new(board: BoardSnapshot) -> Self {
            Self {
                board,
                board_room: None,
                fail: Mutex::new(false),
                names: std::collections::HashMap::new(),
                last_room: Mutex::new(None),
            }
        }
        fn with_name(mut self, peer: airc_core::PeerId, name: &str) -> Self {
            self.names.insert(peer, name.to_string());
            self
        }
        fn in_room(mut self, room: uuid::Uuid) -> Self {
            self.board_room = Some(room);
            self
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl RoomBoardReader for StubReader {
        async fn work_board(&self, room: Option<uuid::Uuid>) -> Result<BoardSnapshot, AircError> {
            *self.last_room.lock().unwrap() = Some(room);
            if self.board_room.is_some() && room != self.board_room {
                return Err(AircError::NotSubscribed(format!("{room:?}")));
            }
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(airc_core::PeerId::new()));
            }
            Ok(self.board.clone())
        }

        async fn peer_names(
            &self,
            peers: &[airc_core::PeerId],
        ) -> std::collections::HashMap<airc_core::PeerId, String> {
            peers
                .iter()
                .filter_map(|p| self.names.get(p).map(|n| (*p, n.clone())))
                .collect()
        }
    }

    // what this catches: the whole room board surfaces as a delivery the brain
    // renders into the [room-kanban] grounding block — every card with its
    // column, title, priority, and owner. This is the Observer perceiving the
    // board (task #117 O6), distinct from active-work's own-claims-only view.
    // what this catches: the board delivering a SLICE. Sources no longer take a size or a
    // count cap (the one allocation in her prompt view picks each item's level), so a
    // 41-card board delivers 41 keyed cards in relevance order — her OWN card first even
    // when it is the oldest — each with a pointer to collapse to, and the headline still
    // reports the true totals.
    #[tokio::test]
    async fn a_large_board_delivers_every_card_keyed_with_her_own_first() {
        let mut cards: Vec<WorkCard> = Vec::new();
        let mut mine = card("MY held card", CardState::InProgress, Some(
            airc_core::PeerId::from_uuid(persona()),
        ));
        mine.created_at_ms = 1; // oldest on the board
        cards.push(mine);
        for i in 0..40 {
            let mut c = card(&format!("filler card {i}"), CardState::Open, None);
            c.created_at_ms = 1_000_000 + i as u64; // all newer than hers
            cards.push(c);
        }
        let total = cards.len();

        let reader = Arc::new(StubReader::new(snapshot(cards)));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source.deliver(&ctx()).await;

        let rendered: Vec<&RagItem> = delivery
            .items
            .iter()
            .filter(|i| i.key.starts_with("card:"))
            .collect();
        assert_eq!(rendered.len(), total, "every card is delivered, none dropped for size");
        assert!(
            rendered[0].content.contains("MY held card"),
            "her OWN card leads, even as the oldest on the board: {}",
            rendered[0].content
        );
        let keys: std::collections::HashSet<_> = rendered.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys.len(), total, "one stable key per card");
        assert!(rendered.iter().all(|i| i.pointer.as_deref().is_some_and(|p| p.contains("work/list"))));
        assert!(!delivery.truncated);
        let headline = &delivery.items[0];
        assert_eq!(headline.key, "headline");
        assert_eq!(headline.metadata["kind"], "board-headline");
        assert_eq!(headline.metadata["held_count"], 1);
        assert_eq!(headline.metadata["open_count"], 40);
        assert!(headline.content.contains("40 claimable"), "{}", headline.content);
    }

    // what this catches: settled history crowding out her live work (the M5, 2026-10-31: 840
    // closed cards, 31,185 chars of her prompt, while her own results collapsed to handles).
    // Live cards are delivered; merged and closed ones are one counted line naming work/list.
    #[tokio::test]
    async fn settled_cards_are_one_counted_history_line_not_cards() {
        let mut cards = vec![
            card("live open", CardState::Open, None),
            card("live review", CardState::Review, None),
        ];
        for i in 0..5 {
            cards.push(card(&format!("closed {i}"), CardState::Closed, None));
        }
        cards.push(card("merged", CardState::Merged, None));
        let reader = Arc::new(StubReader::new(snapshot(cards)));
        let delivery = RoomBoardSource::new(persona(), reader).deliver(&ctx()).await;
        let titles: Vec<&str> = delivery.items.iter().filter(|i| i.key.starts_with("card:")).map(|i| i.content.as_str()).collect();
        assert_eq!(titles.len(), 2, "only live cards are delivered as cards: {titles:?}");
        assert!(titles.iter().all(|t| !t.contains("closed") && !t.contains("merged")), "{titles:?}");
        let history = delivery.items.iter().find(|i| i.key == "history").expect("a history line counts the settled cards"); // expect: the board above holds six settled cards
        assert_eq!(history.metadata["settled_count"], 6);
        assert!(history.content.contains("work/list"), "{}", history.content);
    }

    #[tokio::test]
    async fn whole_board_surfaces_with_owner_and_state() {
        let holder = airc_core::PeerId::new();
        let reader = Arc::new(StubReader::new(snapshot(vec![
            card("Wire the projector", CardState::InProgress, Some(holder)),
            card("Review the PR", CardState::Open, None),
        ])));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        // Headline first, then the available-work lead, then the card list.
        assert_eq!(delivery.items.len(), 4);
        assert_eq!(delivery.items[0].metadata["kind"], "board-headline");
        assert_eq!(delivery.items[1].metadata["kind"], "available-work-lead");
        assert_eq!(delivery.items[1].key, "lead:available");
        let cards: Vec<&RagItem> = delivery
            .items
            .iter()
            .filter(|i| i.metadata.get("card_id").is_some())
            .collect();
        assert_eq!(cards.len(), 2);
        assert!(cards[0].content.contains("Wire the projector"));
        assert!(cards[0].content.contains("[InProgress]"));
        // No alias published for this holder → said as an unnamed peer, never an id inside
        // the sentence (Joel, 2026-10-06), and NEVER "someone".
        let owner8: String = holder.as_uuid().to_string().chars().take(8).collect();
        assert!(cards[0].content.contains(crate::cognition::card_holder::UNNAMED_HOLDER), "{}", cards[0].content);
        assert!(!cards[0].content.contains(&owner8), "{}", cards[0].content);
        assert!(!cards[0].content.contains("someone"));
        // An unclaimed card is surfaced as such — all owners visible on the board.
        assert!(cards[1].content.contains("unclaimed"));
        // serde, not Debug — one canonical wire form for the enum (see the json! above).
        assert_eq!(cards[1].metadata["state"], "open");
        assert!(!delivery.truncated);
    }

    // what this catches: the board's two facts getting SEVERED by a prefix-take.
    // Read out of Asha's own capture 2026-08-06: her window fit exactly ONE of 31
    // board units, divisibility handed her the longest fitting prefix — "[your work]
    // you HOLD 1 card" — and "[available work] 8 claimable" was unit two and died.
    // She then reported "no open tasks" and was CORRECT about what she could see,
    // in a room with 8 claimable cards. Four citizens looped on that all day.
    // The FIRST unit must therefore state BOTH counts as one keyed, tiny item (the
    // allocation collapses oldest-first, and the headline is never the oldest).
    #[tokio::test]
    async fn the_headline_leads_and_states_both_counts_in_one_keyed_item() {
        let me = persona();
        let mut held = card(
            "The card I hold",
            CardState::Claimed,
            Some(airc_core::PeerId::from_uuid(me)),
        );
        held.claim_expires_at_ms = Some(continuum_base::time::EpochMs::now().millis() + 60_000);
        let open_a = card("Claimable one", CardState::Open, None);
        let open_b = card("Claimable two", CardState::Open, None);

        let cards = vec![held, open_a, open_b];
        let reader = Arc::new(StubReader::new(snapshot(cards.clone())));
        let source = RoomBoardSource::new(me, reader);
        let delivery = source
            .deliver(&ctx())
            .await;

        let first = &delivery.items[0];
        assert_eq!(
            first.metadata["kind"], "board-headline",
            "the headline must LEAD"
        );
        assert_eq!(first.key, "headline");
        assert!(first.pointer.is_none(), "the headline is its own shortest statement");
        assert!(first.content.contains("hold 1"), "{}", first.content);
        assert!(first.content.contains("2 claimable"), "{}", first.content);
        // Cheap enough that any budget delivering grounding at all delivers BOTH
        // facts — the detailed leads are ~10x this and are what should degrade.
        assert!(
            first.tokens <= 32,
            "headline must stay tiny, was {}",
            first.tokens
        );

        // Rooted hands hide the available-work lead, never actual eligibility.
        // Also cover work rooted elsewhere: this room has no local held card.
        for local_hold in [true, false] {
            let local_cards = cards
                .iter()
                .filter(|card| local_hold || card.owner != Some(airc_core::PeerId::from_uuid(me)))
                .cloned()
                .collect();
            let source = RoomBoardSource::new(me, Arc::new(StubReader::new(snapshot(local_cards))));
            let full = source
                .deliver_with_workspace(&ctx(), true)
                .await;
            let headline = &full.items[0];
            assert_eq!(headline.metadata["kind"], "board-headline");
            assert_eq!(headline.metadata["held_count"], usize::from(local_hold));
            assert_eq!(headline.metadata["open_count"], 2);
            assert_eq!(headline.metadata["open_lead_suppressed"], true);
            assert!(
                headline.content.contains("2 claimable"),
                "{}",
                headline.content
            );
            assert!(!full
                .items
                .iter()
                .any(|item| item.metadata["kind"] == "available-work-lead"));
            assert_eq!(
                full.items
                    .iter()
                    .filter(|item| item.metadata.get("card_id").is_some())
                    .count(),
                if local_hold { 3 } else { 2 }
            );
        }
    }

    // what this catches: THE rule Joel set on 2026-08-06 — "should never say
    // taken by 'someone', tell them WHO, otherwise they can't reach out. And a
    // persona could go down too." A peer-held card must render that peer's
    // PUBLISHED NAME, and a peer whose lease lapsed must still be named so a
    // citizen can coordinate instead of silently stealing the card. Board
    // owners resolve through the durable alias store, NOT the presence roster,
    // precisely so a teammate who went down still holding work stays nameable.
    #[tokio::test]
    async fn a_peers_card_names_the_peer_live_and_lapsed_never_bare_hex() {
        let asha = airc_core::PeerId::new();
        let mut live = card("Wire the projector", CardState::InProgress, Some(asha));
        live.claim_expires_at_ms = Some(continuum_base::time::EpochMs::now().millis() + 60_000);
        let mut lapsed = card("Windows blocker #1", CardState::Claimed, Some(asha));
        lapsed.claim_expires_at_ms = Some(1_000_000); // 1970-adjacent — long expired

        let reader =
            Arc::new(StubReader::new(snapshot(vec![live, lapsed])).with_name(asha, "Asha"));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        let cards: Vec<&RagItem> = delivery
            .items
            .iter()
            .filter(|i| i.metadata.get("card_id").is_some())
            .collect();
        assert_eq!(cards.len(), 2);

        let asha8: String = asha.as_uuid().to_string().chars().take(8).collect();
        // Live hold: named, and the raw hex is GONE from the line.
        assert!(
            cards[0].content.contains("owner Asha"),
            "{}",
            cards[0].content
        );
        assert!(!cards[0].content.contains(&asha8), "{}", cards[0].content);
        // Lapsed hold: still names WHO held it (so she can reach out) AND says
        // it is takeable — the two facts that were missing while six citizens
        // read stale claims as active work and announced "no open tasks".
        assert!(cards[1].content.contains("Asha"), "{}", cards[1].content);
        assert!(
            cards[1].content.contains("claimable"),
            "{}",
            cards[1].content
        );
        assert!(!cards[1].content.contains(&asha8), "{}", cards[1].content);
    }

    // what this catches: the available-work salience lead (#122). Unclaimed cards
    // must be surfaced PROMINENTLY (first item, naming them + how to claim) so a
    // persona perceives available work instead of drifting to idle chatter with an
    // open card sitting on the board. A board with NO open cards adds no lead.
    #[tokio::test]
    async fn open_cards_get_an_available_work_lead() {
        let holder = airc_core::PeerId::new();
        let reader = Arc::new(StubReader::new(snapshot(vec![
            card("Compile wordstats", CardState::Open, None),
            card("sha256 exercise", CardState::Open, None),
            card("Already mine", CardState::InProgress, Some(holder)),
        ])));
        let source = RoomBoardSource::new(persona(), reader);
        let d = source
            .deliver(&ctx())
            .await;
        // Found by KIND, not by index: the headline now leads, and a test that
        // pins position breaks every time the delivery grows a unit.
        let lead = d
            .items
            .iter()
            .find(|i| i.metadata["kind"] == "available-work-lead")
            .expect("open cards must produce an available-work lead");
        assert_eq!(lead.metadata["open_count"], 2);
        assert!(lead.content.contains("[available work]"));
        assert!(lead.content.contains("Compile wordstats"));
        assert!(lead.content.contains("work/claim"));

        // A board with only claimed cards → no lead, just the card list.
        let claimed_only = Arc::new(StubReader::new(snapshot(vec![card(
            "Held",
            CardState::InProgress,
            Some(holder),
        )])));
        let d2 = RoomBoardSource::new(persona(), claimed_only)
            .deliver(&ctx())
            .await;
        assert!(d2.items.iter().all(|i| i.metadata.get("kind").is_none()));
    }

    // what this catches: a room with NO cards renders no block (backwards-
    // compatible; a fresh room's board is empty).
    #[tokio::test]
    async fn empty_board_delivers_nothing() {
        let reader = Arc::new(StubReader::new(snapshot(vec![])));
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
        assert!(!delivery.truncated);
    }

    // what this catches: a read failure degrades to empty, never panics —
    // cognition stays up if the work subsystem is degraded.
    #[tokio::test]
    async fn read_error_returns_empty_no_panic() {
        let reader = Arc::new(StubReader::new(snapshot(vec![card(
            "x",
            CardState::Open,
            None,
        )])));
        reader.set_fail(true);
        let source = RoomBoardSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: cross-persona ctx gets nothing (defense in depth).
    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let reader = Arc::new(StubReader::new(snapshot(vec![card(
            "x",
            CardState::Open,
            None,
        )])));
        let source = RoomBoardSource::new(persona(), reader);
        let alien = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(&RagContext::for_persona(alien, 1_000_000))
            .await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: the your-work salience (2026-07-11, "did they wander
    // off and forget they can work on things?") — a card the perceiving persona
    // HOLDS must lead the delivery as "[your work] you HOLD…" and render as
    // "owner YOU", never as her own unrecognizable hex prefix. Another peer's
    // claim stays hex-attributed and produces no your-work lead.
    #[tokio::test]
    async fn accepted_current_review_reaches_activity_board_but_not_other_room() {
        let me = persona();
        let room = uuid::Uuid::new_v4();
        let mut mine = card("revise submitted work", CardState::Claimed, Some(airc_core::PeerId::from_uuid(me)));
        let artifact: airc_work::SubmissionArtifact = serde_json::from_value(json!({
            "hash": "a".repeat(64), "size_bytes": 5, "mime": "text/x-diff"
        })).unwrap();
        let candidate = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::new(), card_id: mine.card_id,
            claim_id: mine.claim_id.unwrap(), instance: "ordinary".into(),
            base_sha: airc_work::GitObjectId::new("b".repeat(40)).unwrap(),
            artifact: artifact.clone(), publisher: airc_core::PeerId::from_uuid(me), submitted_at_ms: 10,
        };
        mine.submissions.push(candidate.clone());
        let review = airc_work::WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(), card_id: mine.card_id,
            submission_id: candidate.submission_id, artifact: artifact.clone(),
            review_card_id: airc_work::WorkCardId::new(), review_claim_id: airc_work::ClaimId::new(),
            reviewer: airc_core::PeerId::new(), outcome: airc_work::WorkReviewOutcome::Failed,
            evidence: artifact, evidence_text: Some("remaining trait callers do not compile".into()), reviewed_at_ms: 20,
        };
        let mut old = review.clone();
        old.review_id = airc_work::WorkReviewId::new();
        old.submission_id = airc_work::SubmissionId::new();
        old.outcome = airc_work::WorkReviewOutcome::Passed;
        let mut board = snapshot(vec![mine]);
        board.submission_reviews = vec![old.clone()];
        for _ in 0..32 {
            board.submission_reviews.push(airc_work::WorkSubmissionReview { review_id: airc_work::WorkReviewId::new(), ..review.clone() });
        }
        // Reproduce the title-only starvation with the actual held-task title size:
        // the focused signed guidance fits, but repeating the title/details does not.
        board.cards[0].title = "Continuation-resume deletion PR: remove deliver_continuation machinery + positional cursor (ruling a) — caller inventory attached, closes prior review with record standing".into();
        let reader = Arc::new(StubReader::new(board).in_room(room));
        let source = RoomBoardSource::new(me, reader.clone()).for_room(room);
        let activity = RagContext::for_persona_in_room(me, ctx().now_ms, room);
        let delivery = source.deliver(&activity).await;
        // what this catches: the focused signed verdict is its own keyed item with a
        // pointer, and the held card still delivers whole beside it (no packing).
        let focused = delivery.items.iter()
            .find(|item| item.metadata.get("kind").and_then(|v| v.as_str()) == Some("focused-submission"))
            .expect("focused submission item");
        assert_eq!(focused.key, format!("submission:{}", candidate.card_id.as_uuid()));
        assert!(focused.content.contains("Revise the work"));
        assert!(focused.pointer.is_some());
        assert!(delivery.items.iter().any(|item| item.content.contains("Continuation-resume deletion PR")));
        let content = delivery.items.iter().map(|item| item.content.as_str()).collect::<Vec<_>>().join("\n");
        assert!(content.contains("Revise the work"));
        assert!(content.contains("all 32 signed verdicts, reviewers, and evidence"));
        assert!(content.contains(&candidate.submission_id.to_string()));
        assert!(content.contains("work/submission"));
        assert_eq!(content.matches("Revise the work").count(), 1);
        assert!(!content.contains(&old.review_id.to_string()));
        assert_eq!(*reader.last_room.lock().unwrap(), Some(Some(room)));
        let elsewhere = RagContext::for_persona_in_room(me, ctx().now_ms, uuid::Uuid::new_v4());
        assert!(source.deliver(&elsewhere).await.items.is_empty());
        assert_eq!(*reader.last_room.lock().unwrap(), Some(elsewhere.airc_room.map(|room| room.as_uuid())));

        // Regression 04dc5e95: the production deferred wrapper must preserve the
        // resolved activity before RagSourceFaculty selects this board's room.
        use crate::cognition::{
            deferred_faculty::DeferredFaculty,
            rag_source_faculty::{RagSourceFaculty, SaliencePolicy},
            workspace::{CycleId, Faculty, Workspace},
        };
        let deferred = DeferredFaculty::spawn(Arc::new(
            RagSourceFaculty::new(me, Arc::new(source), SaliencePolicy::StandingFraming)
                .with_clock(Arc::new(|| ctx().now_ms)),
        ));
        let wake = uuid::Uuid::new_v4();
        let mut ws = Workspace::in_room("ambient wake in another room", wake).with_cycle(CycleId(1));
        ws.activity_room_id = room;
        let cold = deferred.contribute(&ws).await.expect("cold activity board");
        assert!(cold.content.contains("Revise the work"));
        *reader.last_room.lock().unwrap() = None;
        // Observe an actual background read, not elapsed time. The fixture's
        // reader is ready immediately, so this also lets its publish complete.
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                tokio::task::yield_now().await;
                if reader.last_room.lock().unwrap().is_some() {
                    break;
                }
            }
        })
        .await
        .expect("deferred board worker read");
        assert_eq!(*reader.last_room.lock().unwrap(), Some(Some(room)));
        let cached = deferred.contribute(&ws).await.expect("background activity board");
        assert!(cached.content.contains("Revise the work"));
        assert!(cached.content.contains(&candidate.submission_id.to_string()));
        assert_eq!(ws.room_id, wake, "task grounding must not change wake attribution");
        ws.activity_room_id = wake;
        assert!(deferred.contribute(&ws).await.is_none(), "old task board withheld after scope switch");
    }

    #[tokio::test]
    async fn held_cards_lead_as_your_work_and_render_owner_you() {
        let me = persona();
        let mine = card(
            "reverse a string",
            CardState::Claimed,
            Some(airc_core::PeerId::from_uuid(me)),
        );
        let theirs = card(
            "sha256 exercise",
            CardState::Claimed,
            Some(airc_core::PeerId::new()),
        );
        let reader = Arc::new(StubReader::new(snapshot(vec![mine, theirs])));
        let source = RoomBoardSource::new(me, reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        let all: String = delivery
            .items
            .iter()
            .map(|i| i.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all.contains("[your work] you HOLD 1 card(s)"),
            "her held card leads as a your-work fact: {all}"
        );
        assert!(
            all.contains("reverse a string"),
            "the lead names HER card: {all}"
        );
        assert!(all.contains("owner YOU"), "her claim renders as YOU: {all}");
        // The peer's card keeps hex attribution and never enters her lead.
        assert!(
            !all.contains("you HOLD 2"),
            "another peer's claim is not hers: {all}"
        );
    }

    // what this catches: the stale-lease loop attractor (glass-boxed 2026-08-03:
    // all 17 board claims stale, residents re-orienting on "held" conway/wordstats
    // cards every wake forever). A card whose claim lease EXPIRED must (a) never
    // count in "[your work] you HOLD", (b) surface as claimable in the
    // available-work lead, and (c) render its lapsed state — agreeing with the
    // #156 lost-claim fact instead of contradicting it every turn.
    #[tokio::test]
    async fn lapsed_lease_reads_claimable_not_held() {
        let me = persona();
        let stale_mine = lapse(card(
            "conway game of life",
            CardState::Claimed,
            Some(airc_core::PeerId::from_uuid(me)),
        ));
        let live_mine = card(
            "wordstats tests",
            CardState::Claimed,
            Some(airc_core::PeerId::from_uuid(me)),
        );
        let closed_mine = lapse(card(
            "finished work",
            CardState::Closed,
            Some(airc_core::PeerId::from_uuid(me)),
        ));
        let reader = Arc::new(StubReader::new(snapshot(vec![stale_mine, live_mine, closed_mine])));
        let source = RoomBoardSource::new(me, reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        let all: String = delivery
            .items
            .iter()
            .map(|i| i.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all.contains("[your work] you HOLD 1 card(s)"),
            "only the LIVE lease is held: {all}"
        );
        assert!(
            all.contains("wordstats tests") && !all.contains("you HOLD 2"),
            "the lapsed card must not count as held: {all}"
        );
        assert!(
            all.contains("[available work] 1 card(s)"),
            "the lapsed card is claimable work: {all}"
        );
        assert!(
            all.contains("claim lapsed (was YOURS) — claimable"),
            "the lapsed card renders its lapse honestly: {all}"
        );
        // A settled card is counted in the history line, not rendered as a card (#5677).
        assert!(!all.contains("finished work"), "the closed card is not a card line: {all}");
        assert!(
            all.contains("[board history] 1 card(s) merged or closed"),
            "the closed card is counted in the history line: {all}"
        );
    }

    // what this catches: the exam-bleed regression (glass-boxed live 2026-07-10,
    // Hermes-8B OURS 38% < RAW 52%) — a room-BOUND board delivering into a turn
    // in a DIFFERENT context. A turn stamped with another room (or the eval
    // fork's nil room) must get an empty delivery, never this room's cards +
    // work/claim invitations; the SAME room still delivers; an UNSTAMPED ctx
    // (None — background/legacy) keeps pre-gate behavior.
    #[tokio::test]
    async fn board_reads_are_turn_parametric_and_exam_contexts_get_nothing() {
        // what this catches (#443, the round-killer's layer 6): a stamped turn
        // must read the board OF ITS OWN ROOM — a citizen in a per-run bench
        // room used to hit the bound-room abstain and take a work turn with NO
        // board surface. Pins: (1) same-room turn reads the turn room; (2) a
        // turn in a DIFFERENT room reads THAT room, never the bound one; (3)
        // the eval fork's synthetic nil context gets NOTHING and never falls
        // back to the bound room (the exam-bleed pin, unchanged); (4) an
        // unstamped ctx keeps legacy behavior — bound-room fallback.
        let home = uuid::Uuid::new_v4();
        let p = persona();
        let reader = Arc::new(StubReader::new(snapshot(vec![card(
            "write wordstats",
            CardState::Open,
            None,
        )])));
        let source = RoomBoardSource::new(p, Arc::clone(&reader) as Arc<dyn RoomBoardReader>)
            .for_room(home);

        // (1) Turn stamped with the SAME room → delivers, read keyed on it.
        let same = RagContext::for_persona_in_room(p, 1_000, home);
        assert!(
            !source
                .deliver(&same)
                .await
                .items
                .is_empty(),
            "same-room turn must receive the board"
        );
        assert_eq!(*reader.last_room.lock().unwrap(), Some(Some(home)));

        // (2) Turn stamped with a DIFFERENT room → delivers THAT room's board
        // (the reader is asked for the TURN room; production's reader refuses
        // rooms the scope has not joined, which is the leak guarantee).
        let bench = uuid::Uuid::new_v4();
        let other = RagContext::for_persona_in_room(p, 1_000, bench);
        assert!(
            !source
                .deliver(&other)
                .await
                .items
                .is_empty(),
            "a bench-room turn must receive a board — the bench room's own"
        );
        assert_eq!(
            *reader.last_room.lock().unwrap(),
            Some(Some(bench)),
            "the read must be keyed on the TURN room, never the bound one"
        );

        // (3) The eval fork's synthetic nil context → NOTHING, and the reader
        // is never consulted (no bound-room fallback for an exam).
        *reader.last_room.lock().unwrap() = None;
        let exam = RagContext::for_persona_in_room(p, 1_000, uuid::Uuid::nil());
        assert!(
            source
                .deliver(&exam)
                .await
                .items
                .is_empty(),
            "a synthetic exam context must NOT receive any board"
        );
        assert_eq!(
            *reader.last_room.lock().unwrap(),
            None,
            "an exam context must not even reach the reader"
        );

        // (4) Unstamped ctx (None) → pre-gate behavior: bound-room fallback.
        let unstamped = RagContext::for_persona(p, 1_000);
        assert!(
            !source
                .deliver(&unstamped)
                .await
                .items
                .is_empty(),
            "an unstamped ctx keeps legacy behavior"
        );
        assert_eq!(*reader.last_room.lock().unwrap(), Some(Some(home)));
    }

    // what this catches (card 31a9fe2b; the M5, 2026-10-08/09: 131 `find /` and `ls ~` walks
    // by citizens hunting their SWE checkout): a held card's line in [your work] must carry
    // the absolute path its work lives at, and must claim no location when this node has none.
    #[test]
    fn a_held_cards_line_names_where_its_work_lives_and_only_when_it_exists() {
        let held = card("[bench swe] django__django-16136: async view", CardState::Claimed, Some(airc_core::PeerId::from_uuid(persona())));
        let checkout = std::path::Path::new("/home/x/.continuum/citizens/peers/aaa/workspace/swe/django__django-16136");
        let with = held_card_line(&held, Some(checkout));
        assert!(with.contains(&checkout.display().to_string()), "the absolute checkout is in her view: {with}");
        assert!(with.starts_with(&held_card_line(&held, None)), "the card's own line is unchanged above it");
        let without = held_card_line(&held, None);
        assert!(!without.contains("work is at"), "no checkout on this node, no location claimed: {without}");
    }
