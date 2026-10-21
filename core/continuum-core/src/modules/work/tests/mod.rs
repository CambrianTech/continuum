#![cfg(test)]

    use super::*;

    // what this catches (Kimi, 2026-10-05, three refusals): the words lead with HER one
    // argument and an example she can copy, name the repos she can see, and put the
    // operator's option second.
    #[test]
    pub(super) fn the_repo_refusal_leads_with_her_action_and_names_what_she_can_see() {
        let with = super::repo_refusal("cambriantech", &["CambrianTech/continuum".into(), "CambrianTech/career-wrangler".into()]);
        assert!(with.starts_with("work/create: add repo to this call, e.g. repo=\"CambrianTech/continuum\"."), "{with}");
        assert!(with.contains("Repos of cards you hold: CambrianTech/continuum, CambrianTech/career-wrangler."), "{with}");
        assert!(with.find("add repo").unwrap() < with.find("activity/spawn").unwrap(), "her action before the operator's");
        let without = super::repo_refusal("cambriantech", &[]);
        assert!(without.starts_with("work/create: add repo to this call, e.g. repo=\"owner/name\"."), "{without}");
        assert!(!without.contains("Repos of cards you hold"), "{without}");
    }

    // what this catches: card 2609fd66 — a board walk that FAILS (daemon outage) must
    // not report as "no cards exist". Three empty-horizon shapes, three different next
    // moves, three distinct sentences: unreadable → a read failure to RETRY (and it is an
    // Internal error, not a NotFound, so the caller's error class tells the truth too);
    // no subscriptions → JOIN; readable-but-empty → the card is in a room not joined.
    // Regression for Kimi's `work/get 31c241e2` during the 2026-09-21 airc update.
    #[test]
    pub(super) fn an_empty_board_walk_says_which_kind_of_empty_it_was() {
        let rooms = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let outage = not_found_in(
            &rooms(&["academy"]),
            &[("seed10".to_string(), "daemon socket refused".to_string())],
            "card",
            "31c241e2",
        );
        let msg = outage.to_string();
        assert!(matches!(outage, CommandError::Internal(_)), "a read failure is not a NotFound: {msg}");
        assert!(msg.contains("FAILED in 1 of 2 subscribed room(s)"), "{msg}");
        assert!(msg.contains("seed10 (daemon socket refused)"), "{msg}");
        assert!(msg.contains("readable boards searched: academy"), "{msg}");
        assert!(msg.contains("retry"), "{msg}");
        assert!(!msg.contains("no cards exist"), "{msg}");

        let unjoined = not_found_in(&[], &[], "card", "31c241e2");
        let msg = unjoined.to_string();
        assert!(matches!(unjoined, CommandError::NotFound(_)), "{msg}");
        assert!(msg.contains("subscribed to no rooms"), "{msg}");
        assert!(msg.contains("join the room"), "{msg}");

        let elsewhere = not_found_in(&rooms(&["academy", "continuum"]), &[], "card", "31c241e2");
        // The claim path (release/heartbeat) is the same walk with the other label.
        let claim = not_found_in(
            &rooms(&["academy"]),
            &[("seed10".to_string(), "x".to_string())],
            "claim",
            "e0ec486b",
        );
        let msg = claim.to_string();
        assert!(matches!(claim, CommandError::Internal(_)), "{msg}");
        assert!(msg.starts_with("[internal] claim e0ec486b: board read FAILED"), "{msg}");
        // A claim that is simply gone (readable, empty) names expiry and the way out —
        // work/heartbeat's one job is a lease, so its miss must say "lease".
        let gone = not_found_in(&rooms(&["academy"]), &[], "claim", "e0ec486b").to_string();
        assert!(gone.contains("EXPIRED") && gone.contains("work/claim"), "{gone}");
        let hinted = claim_gone_hint("no claim matches id prefix 'e0ec486b'".into());
        assert!(hinted.contains("EXPIRED") && hinted.contains("work/claim"), "{hinted}");
        let msg = elsewhere.to_string();
        assert!(matches!(elsewhere, CommandError::NotFound(_)), "{msg}");
        assert!(msg.contains("any room you are in (academy, continuum)"), "{msg}");
        assert!(msg.contains("room you have not joined"), "{msg}");
    }

    // what this catches: the card-transition event NAME is the contract between the
    // emitter (`bridge_wire_work_event`) and every subscriber (SWE grade-on-done, board
    // freshness, auto-close). A silent rename here would unsubscribe every reactor — the
    // event would fire into the void, undetectably. Pin the string. (Event-based system
    // law: the grade REACTS to this, it never polls the board.)
    #[test]
    pub(super) fn card_state_changed_event_name_is_the_stable_emitter_subscriber_contract() {
        assert_eq!(WORK_CARD_STATE_CHANGED, "work.card.state_changed");
    }

    // what this catches: the grade tail now has TWO feeders — the `work/state` verb
    // (in-process, delivery-proof) and the wire echo bridge (external writers). They
    // both see the same transition whenever wire delivery is healthy, and a subscriber
    // that grades twice would write two receipts for one card. The (card,state) ring is
    // the ONLY thing making that one publish; the wire-event-id ring cannot cover it
    // because the verb path has no wire event. Regression for the 2026-08-17 grade tail
    // fix (two Closed cards, zero grades, #434 starving the single wire feeder).
    #[test]
    pub(super) fn one_transition_publishes_once_no_matter_which_feeder_sees_it_first() {
        let card = uuid::Uuid::new_v4().to_string();
        assert!(
            first_transition_sighting(&card, "closed"),
            "first sighting publishes"
        );
        assert!(
            !first_transition_sighting(&card, "closed"),
            "the second feeder for the SAME transition must not publish again"
        );
        assert!(
            first_transition_sighting(&card, "merged"),
            "a DIFFERENT state on the same card is a different transition and must publish"
        );
    }

    /// Build a wire transcript event through the REAL producer (`encode_work_event`),
    /// so a header/codec contract drift fails these tests instead of shipping.
    pub(super) fn wire_work_event(event: airc_work::WorkEvent, event_id: u128) -> airc_core::TranscriptEvent {
        let (headers, body) =
            airc_work::encode_work_event(&event).expect("work event encodes for the fixture");
        airc_core::TranscriptEvent {
            event_id: airc_core::EventId::from_u128(event_id),
            room_id: airc_core::RoomId::from_u128(2),
            peer_id: airc_core::PeerId::from_u128(3),
            client_id: airc_core::ClientId::from_u128(4),
            kind: airc_core::TranscriptKind::System,
            occurred_at_ms: 100,
            lamport: 1,
            target: airc_core::MentionTarget::All,
            headers,
            body: Some(body),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // what this catches: the grade-on-done tail was DEAF to every writer except the
    // continuum `work/state` verb — a card closed via `airc work state` (operator) or a
    // remote peer changed the board and graded NOTHING (live 2026-08-15: Asha's real
    // rle_roundtrip artifact, operator close, zero grade). The bridge decodes the wire
    // echo into the exact payload contract the grader speaks: full hyphenated card UUID
    // + snake_case state that `is_terminal`/`parse_state` accept.
    #[test]
    pub(super) fn wire_card_state_change_decodes_to_the_grader_payload_contract() {
        let card_id = airc_work::WorkCardId::new();
        let event = wire_work_event(
            airc_work::WorkEvent::CardStateChanged(airc_work::CardStateChanged {
                card_id,
                state: airc_work::CardState::Closed,
                changed_by: airc_core::PeerId::from_u128(3),
                changed_at_ms: 100,
            }),
            0xA1,
        );
        let payload = wire_card_state_payload(&event).expect("card-state event must decode");
        assert_eq!(
            payload["card_id"].as_str().unwrap(),
            card_id.as_uuid().to_string(),
            "card_id must be the FULL hyphenated UUID (the grader prefix-matches it)"
        );
        assert_eq!(
            payload["state"].as_str().unwrap(),
            "closed",
            "state must be the snake_case CardState serde form — the is_terminal vocabulary"
        );
        assert_eq!(
            payload["room_id"].as_str().unwrap(),
            airc_core::RoomId::from_u128(2).as_uuid().to_string(),
            "room_id must be the card's OWN room — the grader scopes its board read and \
             verdict post to it (#345 wrong-room trap, hit live 2026-08-15)"
        );
        // parse_state round-trip: the bridged state string is valid work/state input.
        assert_eq!(parse_state("closed").unwrap(), CardState::Closed);
    }

    // what this catches: a NON-state work event (a claim) or a non-work event must
    // never fabricate a state-change publish — a claim firing the grader would grade
    // half-finished work the moment it was picked up.
    #[test]
    pub(super) fn non_state_work_events_and_non_work_events_do_not_bridge() {
        let claim = wire_work_event(
            airc_work::WorkEvent::CardClaimed(airc_work::WorkCardClaimed {
                selected_at_ms: None,
                origin: airc_work::ClaimOrigin::Unknown,
                card_id: airc_work::WorkCardId::new(),
                claim_id: airc_work::ClaimId::from_uuid(uuid::Uuid::new_v4()),
                owner: airc_core::PeerId::from_u128(3),
                ttl_ms: 200,
                claimed_at_ms: 100,
            }),
            0xA2,
        );
        assert!(wire_card_state_payload(&claim).is_none());

        let mut chat = wire_work_event(
            airc_work::WorkEvent::CardStateChanged(airc_work::CardStateChanged {
                card_id: airc_work::WorkCardId::new(),
                state: airc_work::CardState::Closed,
                changed_by: airc_core::PeerId::from_u128(3),
                changed_at_ms: 100,
            }),
            0xA3,
        );
        chat.headers = airc_core::Headers::default(); // strip the work body-hint
        assert!(
            wire_card_state_payload(&chat).is_none(),
            "without the work body-hint header the event must not decode as work"
        );
    }

    // what this catches: every resident persona's subscribe stream yields the SAME
    // room event once — without first-sighting dedup the bridge would publish N
    // copies for N residents and the grader would grade (and post verdicts) N times.
    // Fresh uuids per call keep this parallel-safe against the process-global ring.
    #[test]
    pub(super) fn first_sighting_admits_once_per_event_id() {
        let id = Uuid::new_v4();
        assert!(first_sighting(id), "first delivery must win");
        assert!(!first_sighting(id), "the second resident's echo must dedup");
        assert!(first_sighting(Uuid::new_v4()), "a different event is fresh");
    }

    /// what this catches (#357): `work/claim` inventing a holder out of a stale
    /// `owner` field, then `claim_rejections::record` burning that fabrication
    /// into perception for ten minutes.
    ///
    /// Expiry NEVER clears owner/claim_id — airc-work's projection clears them
    /// only on release or on a new claim — so a card claimed once carries an
    /// owner forever. Keying contention off the bare field relabelled EVERY
    /// refusal ("settled work is not claimable", wrong-room, transport faults)
    /// as "is held by peer X".
    ///
    /// Measured on the live boards 2026-08-07: ~50 of 61 cards on #general and
    /// 4 of 12 on #k3-serving carried a stale owner with a lease expired 134h+.
    /// (#k3-serving is RETIRED — it was named for a SUBSYSTEM, so it had no lifetime
    /// and outlived its purpose by a month. Kept here only as the incident record.)
    /// Two citizens spent the day quoting the resulting sentence back at us —
    /// "expired leases or the cards being held by others" is BOTH HALVES of that
    /// one format string.
    #[test]
    pub(super) fn a_lapsed_lease_is_not_a_person_holding_the_card() {
        use airc_core::PeerId;
        use airc_work::{CardState, Priority, RepoId, WorkCard, WorkCardId};

        let owner = PeerId::new();
        let card = |claimed: bool, expires_ms: Option<u64>| WorkCard {
            card_id: WorkCardId::new(),
            repo: RepoId::new("CambrianTech/continuum").expect("valid repo id in fixture"),
            title: "a card".to_string(),
            body: None,
            priority: Priority::P2,
            lane_id: None,
            state: CardState::Claimed,
            owner: Some(owner),
            claim_provenance: None,
            claim_id: claimed.then(|| airc_work::ClaimId::from_uuid(uuid::Uuid::new_v4())),
            claim_expires_at_ms: expires_ms,
            last_heartbeat_at_ms: None,
            pull_request: None,
            created_by: PeerId::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
            reviews: None,
            submissions: Vec::new(),
            last_submission_rejection: None,
        };
        // A realistic epoch-ms clock: the 134h subtraction below is a real
        // observed lease age, and a toy `now` would underflow it.
        let now = 1_786_000_000_000u64;

        // A LIVE lease is a real person on a real card — still reported.
        assert_eq!(
            live_holder(&card(true, Some(now + 60_000)), now),
            Some(owner),
            "a live claim IS contention and must still name the holder"
        );

        // A lapsed lease: the owner field survives, the hold does not. Whatever
        // refused the claim, it was not a person.
        assert_eq!(
            live_holder(&card(true, Some(now - 1)), now),
            None,
            "an expired lease must NOT be reported as someone holding the card"
        );

        // The exact live shape: owner set, claim long expired. This is the ~50
        // of 61 cards on #general.
        assert_eq!(
            live_holder(&card(true, Some(now - 134 * 60 * 60 * 1000)), now),
            None,
            "a lease expired 134h ago is not a holder"
        );

        // Owner set with no claim at all — the shape a release leaves behind in
        // some projections. Also not a hold.
        assert_eq!(
            live_holder(&card(false, None), now),
            None,
            "an owner with no claim is not a live hold"
        );
        // A future lease must not turn a settled-state refusal into an attempted
        // claim selection, or mislabel the old owner as a competing worker.
        for state in [CardState::Review, CardState::Merged, CardState::Closed] {
            let mut settled = card(true, Some(now + 60_000));
            settled.state = state;
            let holder = live_holder(&settled, now).map(|p| p.as_uuid());
            assert_eq!(classify_refusal(holder, Some(owner.as_uuid())), ClaimRefusal::Fault);
            assert_eq!(classify_refusal(holder, Some(Uuid::new_v4())), ClaimRefusal::Fault);
            // Returning a live review to revision preserves its owner's claim;
            // this exception must never grant another caller or reopen done work.
            assert_eq!(resumes_held_review(&settled, CardState::InProgress, owner, now), state == CardState::Review);
            assert!(!resumes_held_review(&settled, CardState::InProgress, PeerId::new(), now));
            assert!(!resumes_held_review(&settled, CardState::Claimed, owner, now));
            assert!(!resumes_held_review(&settled, CardState::InProgress, owner, now + 60_000));
        }
    }

    // what this catches: work/claim id resolution (#161) still rescues the exact
    // live corruptions after the prefix/near-miss logic moved to the shared
    // continuum_base::id_resolve primitive (#164). Regression pin for the CARD side: the two
    // glass-boxed 2026-07-13 corruptions (9-char first group; a 33-hex variant)
    // classify as a prefix on the intact leading-8 short id, and a bare 8-char short
    // id (what the board shows) does too — so resolve_card_id will expand them
    // against the live board. The pure primitive's full contract is tested in
    // continuum_base::id_resolve; this proves the card verb delegates to it correctly.
    #[test]
    pub(super) fn card_ids_rescue_mistyped_forms_via_shared_id_resolve() {
        use continuum_base::id_resolve::{normalize, IdMatch};
        assert!(matches!(
            normalize("d7cfe47e-8e39-41f5-bb2a-4e5d36e558e1"),
            IdMatch::Full(_)
        ));
        assert_eq!(
            normalize("d7cfe47e0-8e39-41f5-bb2a-4e5d36e558e1"),
            IdMatch::Prefix("d7cfe47e".to_string())
        );
        assert_eq!(
            normalize("d7cfe47e08e3941f5bb2a4e5d36e558e1"), // 33 hex chars
            IdMatch::Prefix("d7cfe47e".to_string())
        );
        assert_eq!(
            normalize("08ece9e8"),
            IdMatch::Prefix("08ece9e8".to_string())
        );
    }

    // what this catches (#321, measured live 2026-08-06): the board lying by OMISSION.
    // A claim carries a lease; when it expires the substrate already treats the card as
    // reclaimable — `airc work next` offered EIGHT that night. But `work/list` rendered only
    // `state` + `owner`, so an expired hold read as `Claimed owner=Anwen`, indistinguishable
    // from someone actively working it. Six citizens read the board CORRECTLY, concluded there
    // was nothing to take, and spent the night announcing they had nothing to do. 19 cards,
    // 17 expired leases, 2 open.
    //
    // Pins the three cases she has to tell apart: open → take it; expired hold → take it
    // (the one that was invisible); live hold → someone else's, leave it.
    #[test]
    pub(super) fn an_expired_lease_reads_as_claimable_and_a_live_one_does_not() {
        // A realistic epoch-ms clock: the 134h subtraction below is a real
        // observed lease age, and a toy `now` would underflow it.
        let now = 1_786_000_000_000u64;
        let claimable_of = |state: CardState, expires: Option<u64>| {
            let expired = expires.is_some_and(|e| e <= now);
            (
                state == CardState::Open || expired,
                expires.map(|_| if expired { "expired" } else { "held" }),
            )
        };

        assert_eq!(
            claimable_of(CardState::Open, None),
            (true, None),
            "an open card is takeable"
        );
        assert_eq!(
            claimable_of(CardState::Claimed, Some(now - 1)),
            (true, Some("expired")),
            "an EXPIRED hold is takeable — this is the fact the board was hiding"
        );
        assert_eq!(
            claimable_of(CardState::Claimed, Some(now + 60_000)),
            (false, Some("held")),
            "a LIVE hold stays someone else's — the guard against stealing active work"
        );
    }

    // what this catches (#337, measured 2026-08-06 from the residents' own captures):
    // the COLUMN filter answering the AVAILABILITY question with a silent zero. Every
    // board query the citizens made was `work/list(state=open)` — 84 of them, every one
    // returning `{"cards":[]}` — on a board of 61 cards with 59 lapsed leases and 0 cards
    // in the Open column. The `[board]` grounding block in the SAME prompt said "59
    // claimable". Both surfaces were right; the citizens reported "no open tasks" all day
    // and were reading their tool correctly. #321 fixed how a lapsed claim RENDERS; this
    // is the same defect one layer down, in what the filter MATCHES.
    //
    // Pins the three things that make that impossible now: the availability axis exists
    // and finds the lapsed cards, the column axis still means the column (no silent
    // widening), and an empty result carries the board's real counts so it can never
    // again be read as an empty board.
    #[test]
    pub(super) fn a_column_filter_can_never_again_report_an_empty_board_as_no_work() {
        // No clock here: this test asserts over the FILTER axes, and states its
        // lease outcomes directly as `claimable_flags`. (It previously carried a
        // `now` and a comment about "the 134h subtraction below" — both copied
        // from its sibling `a_lapsed_lease_is_not_a_person_holding_the_card`,
        // which is the one that actually ages a lease. Dead variable, stale
        // comment; the compiler was right to warn.)
        // The live shape: nothing in the Open column, every claim lapsed.
        let claimable_flags = [true, true, true]; // 3 claimed cards, all leases expired
        let states = [CardState::Claimed, CardState::Claimed, CardState::Claimed];

        let total_on_board = states.len();
        let claimable_now = claimable_flags.iter().filter(|c| **c).count();

        // The query she actually made, 84 times: filter on the COLUMN.
        let by_column: Vec<usize> = (0..total_on_board)
            .filter(|i| states[*i] == CardState::Open)
            .collect();
        assert!(
            by_column.is_empty(),
            "state=open still means the Open COLUMN — the filter is not silently widened"
        );

        // The note that has to accompany that zero.
        let note_fires = by_column.is_empty() && total_on_board > 0;
        assert!(
            note_fires,
            "a zero on a non-empty board must explain itself"
        );
        assert_eq!(
            claimable_now, 3,
            "and the receipt must carry the count she was actually looking for"
        );

        // The query the new axis gives her.
        let by_availability: Vec<usize> = (0..total_on_board)
            .filter(|i| claimable_flags[*i])
            .collect();
        assert_eq!(
            by_availability.len(),
            3,
            "claimable=true finds the lapsed-lease work the column filter misses"
        );
    }

    /// what this catches: the self-explaining summary being serialized AFTER `cards`, where
    /// the receipt truncator severs it from every result a citizen actually reads.
    ///
    /// regression for 2026-08-06: `note` + `total_on_board` + `claimable_now` shipped that
    /// afternoon and were measured that evening to be ABSENT from all four live `work/list`
    /// receipts in Anwen's prompt — computed, serialized, then cut off by
    /// `act_observe::truncate_chars` (cap `fold_at.min(4096)`) because a real board's `cards`
    /// array is far longer than the cap. A field emitted after an unbounded list is a field
    /// nobody will ever see.
    #[test]
    pub(super) fn the_summary_survives_a_truncated_receipt_because_it_precedes_the_card_list() {
        let cards: Vec<WorkListCard> = (0..60)
            .map(|i| WorkListCard {
                id: format!("card-{i:04}"),
                title: "x".repeat(200), // realistic titles — this is what blows the cap
                state: "claimed".to_string(),
                owner: Some("Benchy".to_string()),
                owner_id: None,
                claimable: true,
                lease: Some("expired".to_string()),
                priority: "p2".to_string(),
                age_hours: 3,
            })
            .collect();

        let json = serde_json::to_string(&WorkListResult {
            total_on_board: 60,
            claimable_now: 58,
            note: Some("the board is NOT empty".to_string()),
            cards,
        })
        .expect("WorkListResult serializes");

        // The receipt path keeps a PREFIX. Anything past the cap is severed.
        const RECEIPT_CAP: usize = 4096;
        assert!(
            json.len() > RECEIPT_CAP,
            "this test is only meaningful when the payload actually exceeds the cap ({} bytes)",
            json.len()
        );
        let receipt: String = json.chars().take(RECEIPT_CAP).collect();

        for field in ["total_on_board", "claimable_now", "note"] {
            assert!(
                receipt.contains(field),
                "`{field}` must survive truncation — declare the summary BEFORE `cards`, \
                 or citizens read a severed result and correctly conclude there is no work"
            );
        }
    }

    /// The self-vs-other half of the refusal rule. `live_holder` (tested above,
    /// #357) answers "is a person on this card"; these answer "is that person YOU".
    pub(super) mod refusal_identity {
        use super::*;

        /// what this catches: work/claim telling a citizen that SHE holds her own
        /// card — rendered as a stranger. Glass-boxed live 2026-08-14: Anon
        /// (a20b3ada) held 13 bench cards and was told `card 17531483 ... is held
        /// by peer a20b3ada`. She read her own id as a rival, concluded the work
        /// was taken, and went silent holding thirteen claims — with the board in
        /// her prompt and [Working Presence] telling her a quiet room is not a stop
        /// sign. The substrate outranked both, because it spoke in specifics.
        ///
        /// This is the benchmark loop's stall: dispatch stages a card, she claims
        /// it, a re-claim tells her it is someone else's, and no work happens.
        #[test]
        fn the_caller_is_never_a_rival_for_her_own_card() {
            let her = Uuid::new_v4();
            assert_eq!(
                classify_refusal(Some(her), Some(her)),
                ClaimRefusal::AlreadyYours,
                "a citizen holding her own card must be told she holds it — naming her \
                 own peer id as the rival is how thirteen claimed cards went unworked"
            );
        }

        /// what this catches: over-correcting into "self" and swallowing real
        /// contention — the failure the #357 fix was careful to avoid. A DIFFERENT
        /// live holder is a normal shared-board outcome and must still be named.
        #[test]
        fn a_different_live_holder_is_still_reported_as_contention() {
            let her = Uuid::new_v4();
            let peer = Uuid::new_v4();
            assert_eq!(
                classify_refusal(Some(peer), Some(her)),
                ClaimRefusal::HeldByPeer(peer),
                "someone else's live claim is real contention and must name them"
            );
        }

        /// what this catches: an unidentified caller silently classifying as
        /// "yours". Identity absent is NOT identity matched — without a caller we
        /// cannot claim the card is hers, so the holder is reported as a peer.
        #[test]
        fn an_unknown_caller_is_not_treated_as_the_holder() {
            let holder = Uuid::new_v4();
            assert_eq!(
                classify_refusal(Some(holder), None),
                ClaimRefusal::HeldByPeer(holder),
                "no caller identity means no self-match — never assume it is hers"
            );
        }

        /// what this catches: regression of the #357 rule through the new arm —
        /// nobody on the card is a fault, never a fabricated holder, whether or
        /// not we know who is asking.
        #[test]
        fn nobody_on_the_card_is_a_fault_not_a_holder() {
            assert_eq!(
                classify_refusal(None, Some(Uuid::new_v4())),
                ClaimRefusal::Fault
            );
            assert_eq!(classify_refusal(None, None), ClaimRefusal::Fault);
        }
    }

    /// what this catches (Kimi on the 5090, 2026-10-06 22:5xZ): parallel `work/state` closes
    /// of cards on DIFFERENT boards, issued from a third room. Each used to refuse in the
    /// current room and "follow" its card by moving the scope's one current-room pointer, so
    /// parallel follows flipped it under each other and 7 of her 8 closes refused. A card on a
    /// visible board is mutated on that board, and the pointer never moves.
    #[tokio::test]
    pub(super) async fn parallel_closes_across_boards_land_without_moving_the_current_room() {
        let home = tempfile::tempdir().expect("temp airc home");
        let airc = Arc::new(
            Airc::open_with_wire_root_for_test(home.path(), home.path())
                .await
                .expect("a local airc scope opens without a daemon"),
        );
        let mut ids = Vec::new();
        for room in ["continuum", "cambriantech"] {
            airc.join(room).await.expect("join a board's room");
            let made = WorkCreate::create(
                &airc,
                WorkCreateParams {
                    room: room.to_string(),
                    repo: Some("github.com/CambrianTech/continuum".to_string()),
                    title: format!("a duplicate review card on {room}"),
                    body: None,
                    priority: None,
                },
            )
            .await
            .expect("card created");
            ids.push(WorkCardId::from_uuid(Uuid::parse_str(&made.card_id).expect("card_id is a uuid")));
        }
        let lobby = airc.join("general").await.expect("focus moves to a third room");

        let (a, b) = tokio::join!(
            raw_advance(&airc, ids[0], CardState::Closed, "test"),
            raw_advance(&airc, ids[1], CardState::Closed, "test"),
        );
        assert!(a.is_ok(), "the #continuum card closes: {a:?}");
        assert!(b.is_ok(), "the #cambriantech card closes: {b:?}");
        for id in &ids {
            let (_, card) = card_in_subscribed_rooms(&airc, *id).await.expect("the card is on a visible board");
            assert_eq!(card.state, CardState::Closed);
        }
        assert_eq!(
            airc.current_room().await.expect("current room").channel,
            lobby.channel,
            "closing a card on another board never moves her current room"
        );
    }

mod part_2;
