#![cfg(test)]

    // what this catches (cards 4cfd4530 and 7c8c44a1; the 5090 2026-10-11): Kimi lifted a staged
    // revision from ANOTHER card's ledger into her submit, because every ledger read only
    // `[card-ledger]`; and 10 of her 14 ledgers were for merged or closed cards, re-prefilled
    // every turn. A concluded card's ledger leaves her context (her call); every other ledger
    // leads with the card it is about, or that it is off this room's board. Order is kept; other
    // categories and an unreadable ledger pass through verbatim; an unread board changes nothing.
    #[test]
    fn each_card_ledger_names_its_card_and_a_concluded_cards_ledger_leaves_the_context() {
        use crate::experience::ledger::{CardLedger, LEDGER_WALL_CATEGORY};
        let mine = uuid::Uuid::parse_str("27f133e4-0000-4000-8000-000000000001").unwrap();
        let other = uuid::Uuid::parse_str("1f58fc16-0000-4000-8000-000000000002").unwrap();
        let gone = uuid::Uuid::parse_str("deadbeef-0000-4000-8000-000000000003").unwrap();
        let reviewed = uuid::Uuid::parse_str("4972ba91-0000-4000-8000-000000000004").unwrap();
        let merged = uuid::Uuid::parse_str("d61513e4-0000-4000-8000-000000000005").unwrap();
        let ledger = |card_id| {
            serde_json::to_string(&CardLedger { card_id, next_test: "run it".into(), ..Default::default() }).unwrap()
        };
        let posts = vec![
            post("rules", "be kind"),
            post(LEDGER_WALL_CATEGORY, &ledger(mine)),
            post(LEDGER_WALL_CATEGORY, &ledger(other)),
            post(LEDGER_WALL_CATEGORY, &ledger(gone)),
            post(LEDGER_WALL_CATEGORY, "not json"),
            post(LEDGER_WALL_CATEGORY, &ledger(reviewed)),
            post(LEDGER_WALL_CATEGORY, &ledger(merged)),
        ];
        let mut labels = CardLabels::new();
        labels.insert(mine, ("RenameIndex crashes".to_string(), airc_work::CardState::InProgress));
        labels.insert(other, ("Q() objects".to_string(), airc_work::CardState::Closed));
        labels.insert(reviewed, ("under review".to_string(), airc_work::CardState::Review));
        labels.insert(merged, ("landed".to_string(), airc_work::CardState::Merged));
        let out = ledgers_for_context(posts.clone(), Some(&labels));
        // the closed (index 2) and merged (index 6) cards' ledgers leave; order is kept
        let kept = [0usize, 1, 3, 4, 5];
        assert_eq!(
            out.iter().map(|p| p.post_id).collect::<Vec<_>>(),
            kept.iter().map(|&i| posts[i].post_id).collect::<Vec<_>>(),
            "concluded cards' ledgers leave; everything else stays, in order"
        );
        assert_eq!(out[0].body, "be kind", "other categories pass through verbatim");
        assert!(out[1].body.starts_with("ledger of card 27f133e4 \"RenameIndex crashes\" (in_progress)\n{"), "{}", out[1].body);
        assert!(out[2].body.starts_with("ledger of card deadbeef (not on this room's board)\n"), "{}", out[2].body);
        assert_eq!(out[3].body, "not json", "an unreadable ledger renders as it is");
        assert!(
            out[4].body.starts_with("ledger of card 4972ba91 \"under review\" (review)\n"),
            "a card in REVIEW is live work: its ledger is her resumption state and stays: {}",
            out[4].body
        );
        assert!(out[1].body.ends_with(&posts[1].body), "the ledger itself is unchanged after its label line");
        // BigMama on #4959: a board that could not be read says nothing about any card, never
        // "not on this room's board" for all of them
        let unread = ledgers_for_context(posts.clone(), None);
        assert_eq!(unread.iter().map(|p| &p.body).collect::<Vec<_>>(), posts.iter().map(|p| &p.body).collect::<Vec<_>>(), "an unread board renders every post verbatim");
        // and a board that was read but holds no cards does say so
        let empty = ledgers_for_context(posts.clone(), Some(&CardLabels::new()));
        assert!(empty[1].body.starts_with("ledger of card 27f133e4 (not on this room's board)\n"), "{}", empty[1].body);
    }
    use super::*;
    use airc_core::PeerId;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    fn ctx() -> RagContext {
        RagContext::for_persona(persona(), 1_000_000)
    }

    fn post(category: &str, body: &str) -> WallPostPublished {
        WallPostPublished {
            room_id: airc_core::RoomId::new(),
            post_id: Uuid::new_v4(),
            category: category.to_string(),
            body: body.to_string(),
            supersedes: None,
            published_by: PeerId::new(),
            published_at_ms: 1_000_000,
        }
    }

    struct StubReader {
        posts: Vec<WallPostPublished>,
        /// The board this reader answers `card_labels` with: `None` = it has no board.
        board: Option<CardLabels>,
        fail: Mutex<bool>,
        /// The room argument of the LAST read, so a test can assert which room
        /// the source actually asked for — the whole point of the bound-room
        /// seam is unobservable from the returned posts alone.
        asked_for: Mutex<Option<Option<uuid::Uuid>>>,
    }

    impl StubReader {
        fn new(posts: Vec<WallPostPublished>) -> Self {
            Self {
                posts,
                board: None,
                fail: Mutex::new(false),
                asked_for: Mutex::new(None),
            }
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }

    #[async_trait]
    impl WallReader for StubReader {
        async fn wall_posts(
            &self,
            room: Option<uuid::Uuid>,
        ) -> Result<Vec<WallPostPublished>, AircError> {
            // safe: test-local Mutex, no panic while held, so it cannot be poisoned.
            *self.asked_for.lock().unwrap() = Some(room);
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.posts.clone())
        }

        async fn card_labels(&self, _room: Option<uuid::Uuid>) -> Option<CardLabels> {
            self.board.clone()
        }
    }

    // what this catches (the 5090, 2026-10-12, build 6153): the labels and the concluded-ledger
    // drop wired only in a helper, never through deliver(), the path a persona's turn takes. The
    // helper-level test passed while production readers answered "board unread" and Kimi's wall
    // kept a merged card's ledger, unlabelled. This drives the READER's board through deliver().
    #[tokio::test]
    async fn deliver_applies_the_readers_board_to_card_ledgers() {
        use crate::experience::ledger::{CardLedger, LEDGER_WALL_CATEGORY};
        let live = Uuid::from_u128(0x27f1_33e4);
        let merged = Uuid::from_u128(0x1f58_fc16);
        let ledger = |card_id| {
            post(
                LEDGER_WALL_CATEGORY,
                &serde_json::to_string(&CardLedger { card_id, next_test: "run it".into(), ..Default::default() }).unwrap(),
            )
        };
        let mut board = CardLabels::new();
        board.insert(live, ("RenameIndex crashes".to_string(), airc_work::CardState::InProgress));
        board.insert(merged, ("landed".to_string(), airc_work::CardState::Merged));
        let mut reader = StubReader::new(vec![ledger(live), ledger(merged)]);
        reader.board = Some(board);
        let delivery = WallSource::new(persona(), Arc::new(reader))
            .deliver(&ctx())
            .await;
        let text = delivery.items.iter().map(|i| i.content.as_str()).collect::<Vec<_>>().join("\n");
        assert!(text.contains(&format!("ledger of card {} \"RenameIndex crashes\" (in_progress)", airc_core::shown_id::shown_form(live))), "{text}");
        assert!(!text.contains(&merged.to_string()), "a merged card's ledger leaves the context: {text}");
        assert_eq!(delivery.items.len(), 1, "only the live card's ledger is delivered");
    }

    /// what this catches (#3874): an id that names a thing it does not read. This
    /// source reads the WALL and announced itself as `room-board`, while
    /// `room_board_source.rs` -- the work board -- emits `room-kanban`. An id-first
    /// reader landed in the wrong file every time, which cost a wrong row in a
    /// published census and a landmine warning to a citizen mid-task.
    ///
    /// The assertion is on the CONSTANT rather than a rendered block because the
    /// constant IS the block header (`[<source_id>]`, generic projection) -- what a
    /// citizen reads above the pinned posts. A test on prose would not have caught
    /// the original, since the prose was accurate about the wall all along; only the
    /// id lied.
    #[test]
    fn the_wall_announces_itself_as_the_wall() {
        assert_eq!(
            WALL, "room-wall",
            "this source reads the WALL; `room-board` named the work board and sent              readers to room_board_source.rs (#3874)"
        );
        assert_ne!(
            WALL, "room-kanban",
            "and it must never collide with the work board's id -- two sources under              one header would make the grounding block ambiguous to the citizen"
        );
    }

    // what this catches: fixing ledger lookup while generic wall delivery still
    // spends its context budget on obsolete revisions of the same card.
    #[tokio::test]
    async fn wall_delivery_projects_ledger_versions_before_budgeting() {
        let card = Uuid::from_u128(7);
        let ledger = |step: &str| {
            let mut record = post(
                crate::experience::ledger::LEDGER_WALL_CATEGORY,
                &serde_json::to_string(&crate::experience::ledger::CardLedger {
                    card_id: card,
                    next_test: step.into(),
                    ..Default::default()
                })
                .unwrap(),
            );
            record.room_id = airc_core::RoomId::from_uuid(Uuid::from_u128(9));
            record
        };
        let source = WallSource::new(
            persona(),
            Arc::new(StubReader::new(vec![
                ledger("obsolete step"),
                post("plan", "shared activity plan"),
                ledger("current step"),
            ])),
        );
        let delivery = source
            .deliver(&ctx())
            .await;
        assert_eq!(delivery.items.len(), 2);
        let text = delivery
            .items
            .iter()
            .map(|i| i.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("shared activity plan"));
        assert!(text.contains("current step"));
        assert!(!text.contains("obsolete step"));
    }

    /// what this catches (#3883): the wall reading the room it was BOUND to instead
    /// of the room the turn is in. `supervisor.rs` binds it with
    /// `.for_room(identity.default_room)`, so before this fix a citizen working
    /// anywhere else was grounded on her default room's wall — or, once the shared
    /// gate refused it, on nothing. Measured live after #3879 landed: with the roster
    /// fixed and silent, THIS was the sole surviving `rag.room_gate.abstain`,
    /// bound=#general turn=#continuum.
    ///
    /// The room argument is the only observable proof — the returned posts say
    /// nothing about which room they came from — which is why StubReader records it.
    #[tokio::test]
    async fn a_roomed_turn_reads_the_turns_wall_not_the_bound_one() {
        let reader = Arc::new(StubReader::new(vec![post("plan", "body")]));
        // safe: literals, valid UUIDs — a parse failure here is a typo in this test.
        let bound = Uuid::parse_str("00000000-0000-0000-0000-0000000000cc").unwrap();
        let turn = Uuid::parse_str("00000000-0000-0000-0000-0000000000dd").unwrap();
        assert_ne!(bound, turn, "fixture is degenerate if these match");
        let source = WallSource::new(persona(), reader.clone()).for_room(bound);
        let delivery = source
            .deliver(&RagContext::for_persona_in_room(persona(), 1_000_000, turn))
            .await;
        assert_eq!(
            // safe: test-local Mutex, single-threaded here.
            *reader.asked_for.lock().unwrap(),
            Some(Some(turn)),
            "the wall must ask for the TURN's room; `Some(bound)` is #3883 and              `None` is the pre-bind fall-through"
        );
        assert!(
            !delivery.items.is_empty(),
            "and it must actually DELIVER — a fix that only changed the room asked              for while still abstaining would pass the assertion above"
        );
    }

    /// what this catches: exam bleed. A synthetic nil room (the eval fork's context)
    /// must get NOTHING and must not fall back to the bound room — otherwise a
    /// proctored run reads another room's pinned instructions. Copied deliberately
    /// from `room_board_source`'s nil arm rather than reinvented.
    #[tokio::test]
    async fn a_nil_room_gets_no_wall_and_never_falls_back_to_the_bound_room() {
        let reader = Arc::new(StubReader::new(vec![post("plan", "body")]));
        // safe: literal, valid UUID.
        let bound = Uuid::parse_str("00000000-0000-0000-0000-0000000000cc").unwrap();
        let source = WallSource::new(persona(), reader.clone()).for_room(bound);
        let delivery = source
            .deliver(&RagContext::for_persona_in_room(persona(), 1_000_000, Uuid::nil()))
            .await;
        assert!(delivery.items.is_empty(), "a nil room must render no wall");
        assert_eq!(
            *reader.asked_for.lock().unwrap(),
            None,
            "and must not READ at all — asking for the bound room is the exam bleed"
        );
    }

    // what this catches: pinned wall posts surface as a delivery the brain
    // renders into the [room-board] grounding block — the shared-layer
    // grounding that lets a persona read the room's plan/instructions.
    #[tokio::test]
    async fn pinned_posts_surface() {
        let reader = Arc::new(StubReader::new(vec![
            post("plan", "Ship the wall grounding slice."),
            post("rules", "Be concise; cite the post you follow."),
        ]));
        let source = WallSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        assert_eq!(delivery.items.len(), 2);
        assert!(delivery.items[0]
            .content
            .contains("Ship the wall grounding"));
        assert!(delivery.items[0].content.contains("[plan]"));
        assert_eq!(delivery.items[1].metadata["category"], "rules");
        assert!(!delivery.truncated);
    }

    // what this catches: a room with NO pinned posts renders no block
    // (backwards-compatible; most rooms start with an empty wall).
    #[tokio::test]
    async fn empty_wall_delivers_nothing() {
        let reader = Arc::new(StubReader::new(vec![]));
        let source = WallSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        assert!(delivery.items.is_empty());
        assert_eq!(delivery.tokens_used, 0);
        assert!(!delivery.truncated);
    }

    // what this catches: a read failure degrades to empty, never panics —
    // cognition stays up if the wall subsystem is degraded.
    #[tokio::test]
    async fn read_error_returns_empty_no_panic() {
        let reader = Arc::new(StubReader::new(vec![post("plan", "body")]));
        reader.set_fail(true);
        let source = WallSource::new(persona(), reader);
        let delivery = source
            .deliver(&ctx())
            .await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: cross-persona ctx gets nothing (defense in depth).
    #[tokio::test]
    async fn a_bound_source_reads_the_room_it_was_bound_to_never_the_default() {
        // what this catches: the wall read silently falling back to "whatever room
        // this handle currently points at". Before 2026-09-06 this source called
        // `Airc::wall_posts`, whose first act is `current_room().await` — so a
        // citizen acting in room X was grounded on the wall of her DEFAULT room, and
        // nothing in the returned posts says which room they came from. airc's own
        // doc (airc.rs:1285-1297) says continuum needs the `_in` variant for exactly
        // this reason. The room argument is the only observable proof, which is why
        // StubReader records it.
        let reader = Arc::new(StubReader::new(vec![post("plan", "body")]));
        // safe: a literal, valid UUID — a parse failure here is a typo in this test.
        let bound = Uuid::parse_str("00000000-0000-0000-0000-0000000000cc").unwrap();
        let source = WallSource::new(persona(), reader.clone()).for_room(bound);
        let _ = source
            .deliver(&RagContext::for_persona(persona(), 1_000_000))
            .await;
        assert_eq!(
            // safe: test-local Mutex; the only writer is the stub above, single-threaded here.
            *reader.asked_for.lock().unwrap(),
            Some(Some(bound)),
            "a bound WallSource must ask for ITS room; None here means the read fell              through to the handle's current_room and grounded the wrong wall"
        );
    }

    #[tokio::test]
    async fn cross_persona_ctx_returns_empty() {
        let reader = Arc::new(StubReader::new(vec![post("plan", "body")]));
        let source = WallSource::new(persona(), reader);
        let alien = Uuid::parse_str("00000000-0000-0000-0000-000000000bbb").unwrap();
        let delivery = source
            .deliver(&RagContext::for_persona(alien, 1_000_000))
            .await;
        assert!(delivery.items.is_empty());
    }

    // what this catches: a large board is delivered WHOLE, one item per post keyed by its
    // post id, each with a `[category] <first line>` summary and a persona/wall/list
    // pointer, `truncated` false and no truncation marker; the view, not the source, sizes it.
    #[tokio::test]
    async fn every_post_is_delivered_whole_with_levels() {
        let body = format!("first line\n{}", "y".repeat(10_000));
        let posts = vec![post("a", &body), post("b", &body), post("c", &body)];
        let ids: Vec<String> = posts.iter().map(|p| p.post_id.to_string()).collect();
        let source = WallSource::new(persona(), Arc::new(StubReader::new(posts)));

        let d = source.deliver(&ctx()).await;
        assert_eq!(d.items.len(), 3);
        assert!(!d.truncated);
        for (item, id) in d.items.iter().zip(&ids) {
            assert_eq!(&item.key, id);
            assert!(item.content.ends_with(&body), "post body whole, never cut");
            assert!(!item.content.contains("truncated"));
            let cat = item.metadata["category"].as_str().unwrap_or_default();
            assert_eq!(item.summary.as_deref(), Some(format!("[{cat}] first line").as_str()));
            let pointer = item.pointer.as_deref().unwrap_or_default();
            assert!(pointer.contains(id.as_str()) && pointer.contains("persona/wall/list"), "{pointer}");
        }
    }
