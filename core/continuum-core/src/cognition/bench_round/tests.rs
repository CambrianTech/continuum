#![cfg(test)]
    // what this catches: the sensor 2026-09-01 lacked — `working 0/8` was
    // pixel-identical for three hours of workspace-less thrash and a healthy
    // grind. The verdict must pronounce: no artifacts anywhere → `unstarted`;
    // a recent act on an unsettled card → `grinding`; artifacts gone silent
    // past the stall window → `stalled`; and an unstarted card must RENDER
    // (state "unstarted"), never vanish for lack of a run ledger.
    // what this catches (card 8a0ee845): a fresh card inheriting an OLDER round's verdict on
    // the same instance. The run names its card; a run for card A never enriches card B,
    // even when B's round opened after A's run was last active.
    #[test]
    fn a_run_enriches_only_the_card_it_belongs_to() {
        use super::{enrich_rounds, CardRunFacts, RoundCardSnapshot, RoundSnapshot};
        let now_ms = 10_000_000u64;
        let card_a = Uuid::new_v4().to_string();
        let card_b = Uuid::new_v4().to_string();
        let card = |id: &str| RoundCardSnapshot {
            card_id: id.to_string(),
            instance: "matplotlib__matplotlib-22719".to_string(),
            assignee: String::new(),
            solve_room_name: String::new(),
            state: String::new(),
            acts: None,
            patch_bytes: None,
            last_act_secs: None,
            resolved: None,
            board_state: String::new(),
            owner: String::new(),
            created_at_ms: None,
            updated_at_ms: None,
            graded_at_ms: None,
        };
        let round = |cards: Vec<RoundCardSnapshot>| RoundSnapshot {
            opened_at_ms: now_ms - 60_000,
            round_id: Uuid::new_v4().to_string(),
            run_room_name: String::new(),
            benchmark: "swe-bench-verified".into(),
            stage: "working".into(),
            dispatched: cards.len(),
            settled: 0,
            remaining: cards.len(),
            driver: "citizen".into(),
            cards,
            verdict: String::new(),
            idle_secs: None,
        };
        let run_for = |id: &str| CardRunFacts {
            card_id: Some(id.to_string()),
            instance: "matplotlib__matplotlib-22719".into(),
            solver: Some("Atlas".into()),
            phase: "resolved".into(),
            acts: Some(9),
            patch_bytes: Some(120),
            last_activity_ms: now_ms - 10_000,
            resolved: Some(true),
        };
        let mut rounds = vec![round(vec![card(&card_b)])];
        enrich_rounds(&mut rounds, &[run_for(&card_a)], now_ms);
        assert_ne!(rounds[0].cards[0].state, "resolved", "card B never inherits card A's run");
        let mut rounds2 = vec![round(vec![card(&card_b)])];
        enrich_rounds(&mut rounds2, &[run_for(&card_b)], now_ms);
        assert_eq!(rounds2[0].cards[0].state, "resolved", "the run that names card B enriches it");
    }

    #[test]
    fn round_verdict_separates_thrash_from_grind() {
        use super::{enrich_rounds, CardRunFacts, RoundCardSnapshot, RoundSnapshot};
        let now_ms: u64 = 10_000_000_000;
        let card = |instance: &str| RoundCardSnapshot {
            card_id: Uuid::new_v4().to_string(),
            instance: instance.to_string(),
            assignee: String::new(),
            solve_room_name: format!("swe--{instance}--abcd1234"),
            state: "unstarted".to_string(),
            acts: None,
            patch_bytes: None,
            last_act_secs: None,
            resolved: None,
            board_state: String::new(),
            owner: String::new(),
            created_at_ms: None,
            updated_at_ms: None,
            graded_at_ms: None,
        };
        let round = |cards: Vec<RoundCardSnapshot>| RoundSnapshot {
            opened_at_ms: 0,
            round_id: Uuid::new_v4().to_string(),
            run_room_name: String::new(),
            benchmark: "swe-bench-verified".into(),
            stage: "working".into(),
            dispatched: cards.len(),
            settled: 0,
            remaining: cards.len(),
            driver: "citizen".into(),
            cards,
            verdict: String::new(),
            idle_secs: None,
        };
        let mut rounds = vec![
            round(vec![card("sympy__sympy-12481"), card("django__django-11211")]),
            round(vec![card("astropy__astropy-12907")]),
            round(vec![card("scikit__scikit-1")]),
        ];
        let facts = vec![
            // Fresh act 60s ago → its round grinds.
            CardRunFacts {
                card_id: None,
                instance: "astropy__astropy-12907".into(),
                solver: Some("Kira".into()),
                phase: "active".into(),
                acts: Some(7),
                patch_bytes: Some(400),
                last_activity_ms: now_ms - 60_000,
                resolved: None,
            },
            // Artifact exists but silent 2h → its round stalled.
            CardRunFacts {
                card_id: None,
                instance: "scikit__scikit-1".into(),
                solver: None,
                phase: "quiet".into(),
                acts: Some(3),
                patch_bytes: None,
                last_activity_ms: now_ms - 7_200_000,
                resolved: None,
            },
        ];
        enrich_rounds(&mut rounds, &facts, now_ms);
        assert_eq!(rounds[0].verdict, "unstarted");
        assert_eq!(rounds[0].idle_secs, None);
        // The unstarted cards RENDER — the whole point.
        assert!(rounds[0].cards.iter().all(|c| c.state == "unstarted"));
        assert_eq!(rounds[1].verdict, "grinding");
        assert_eq!(rounds[1].idle_secs, Some(60));
        assert_eq!(rounds[1].cards[0].assignee, "Kira");
        assert_eq!(rounds[1].cards[0].acts, Some(7));
        assert_eq!(rounds[2].verdict, "stalled");
        assert_eq!(rounds[2].cards[0].state, "quiet");
    }

    // what this catches: A CITIZEN-driven card leaves NO detached-solve ledger
    // entry, so before the held-work freshness signal the round read "unstarted"
    // while she was actively working it — and the standing autopilot, seeing no
    // working round, piled up duplicate rounds (measured 2026-09-02: 21 rounds,
    // 53 unstarted, 4 citizens). This pins that a held-work turn makes the round
    // read "working"/"grinding" from the ephemeral CARD_LAST_ACT signal alone
    // (empty run ledger), so the autopilot HOLDS instead of over-dispatching.
    // what this catches: the review gate's state machine on the tracker — a gated
    // card requires review until a review card registered for it settles as PASSED;
    // a rejected review leaves it required; review cards appear on the pull deck
    // ahead of Open cards and never count toward the round's remaining.
    /// what this catches (ONE-RESIDENT §10.2; Fable's agency ask on coursework step 1): a
    /// coursework round is an invitation, never an assignment, and never outranks real work.
    /// A citizen standing in both rounds is offered the real card first even though the lesson
    /// deck is fuller (the fullest-deck-first order would have put lessons ahead); a citizen who
    /// left the lesson room (declined) is offered none of its cards.
    #[test]
    fn coursework_is_offered_only_to_who_stays_and_never_ahead_of_real_work() {
        let real = Uuid::new_v4();
        let lessons = Uuid::new_v4();
        let real_card = Uuid::new_v4();
        let lesson_cards = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        open_round(real, "swe-bench-verified", WorkDriver::Citizen);
        add_card(real, real_card);
        open_round(lessons, "coursework-0123456789ab", WorkDriver::Citizen);
        for card in lesson_cards {
            add_card(lessons, card);
        }
        let peer = Uuid::new_v4();

        let in_both: std::collections::HashSet<Uuid> = [real, lessons].into_iter().collect();
        let deck = pullable_cards(peer, &in_both);
        assert_eq!(deck[0].card, real_card, "real work leads a fuller lesson deck");
        assert_eq!(deck.len(), 4, "the lessons are still offered, after it");

        let declined: std::collections::HashSet<Uuid> = [real].into_iter().collect();
        assert!(
            pullable_cards(peer, &declined).iter().all(|c| !lesson_cards.contains(&c.card)),
            "a citizen who left the lesson room is offered none of its cards"
        );
        let mut rounds = ROUNDS.lock().unwrap(); // test: cleanup of the rounds opened above
        rounds.remove(&real);
        rounds.remove(&lessons);
    }

    /// what this catches (ONE-RESIDENT §10.2; Fable's provenance ask on coursework step 1): a
    /// lesson turn told from real work by data. The staged row reads its set from here, and
    /// the settle path keeps lesson verdicts out of gene-trial credit on that field alone, so a
    /// lesson card (and its review card) must name its set and a real card must name none.
    #[test]
    fn a_lesson_card_names_its_set_and_real_work_names_none() {
        let real = Uuid::new_v4();
        let lessons = Uuid::new_v4();
        let (real_card, lesson_card, lesson_review) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        open_round(real, "swe-bench-verified", WorkDriver::Citizen);
        add_card(real, real_card);
        open_round(lessons, "coursework-00112233aabb", WorkDriver::Citizen);
        add_card(lessons, lesson_card);
        assert_eq!(register_review_card(lesson_card, lesson_review), Some(lessons));

        let set = |card| coursework_set_of(card).map(|s| s.as_str().to_string());
        assert_eq!(set(lesson_card).as_deref(), Some("coursework-00112233aabb"));
        assert_eq!(set(lesson_review).as_deref(), Some("coursework-00112233aabb"));
        assert_eq!(coursework_set_of(real_card), None, "real work carries no set");
        assert_eq!(coursework_set_of(Uuid::new_v4()), None, "an untracked card is not a lesson");
        let mut rounds = ROUNDS.lock().unwrap(); // test: cleanup of the rounds opened above
        rounds.remove(&real);
        rounds.remove(&lessons);
    }

    #[test]
    fn a_gated_card_requires_review_until_its_review_card_passes() {
        let round_id = Uuid::new_v4();
        let card = Uuid::new_v4();
        open_round(round_id, "swe-bench-verified", WorkDriver::Citizen);
        add_card(round_id, card);
        assert!(!review_required(card), "no gate, no review");
        set_review_gate(round_id, true);
        assert!(review_required(card));

        let review = Uuid::new_v4();
        assert_eq!(register_review_card(card, review), Some(round_id));
        assert_eq!(review_parent(review), Some(card));
        let resident: std::collections::HashSet<Uuid> = [round_id].into_iter().collect();
        let deck = pullable_cards(Uuid::new_v4(), &resident);
        assert_eq!(deck[0].card, review, "the review card leads the deck");
        assert_eq!(
            ROUNDS.lock().unwrap().get(&round_id).unwrap().remaining(),  // test: the round was opened above
            1,
            "a review card is not a round card"
        );

        assert_eq!(settle_review_card(review, false), Some(card));
        assert!(review_required(card), "a rejection keeps the gate shut");
        let again = Uuid::new_v4();
        register_review_card(card, again);
        assert_eq!(settle_review_card(again, true), Some(card));
        assert!(!review_required(card), "a pass opens the gate for the parent's done");
        ROUNDS.lock().unwrap().remove(&round_id);  // test: cleanup of the round opened above
    }

    #[test]
    fn a_citizen_held_work_turn_makes_the_round_read_working() {
        use super::{card_last_act_ms, enrich_rounds, record_card_worked, RoundCardSnapshot, RoundSnapshot};
        let now_ms: u64 = 20_000_000_000;
        let card_uuid = Uuid::new_v4();
        // She drove a held-work turn 30s ago — no run-ledger fact exists.
        // Freshness lives ON THE ROUND (durable): a card outside any round has none,
        // a card in a live round remembers its latest held-work turn.
        let stray = Uuid::new_v4();
        record_card_worked(stray, now_ms);
        assert_eq!(card_last_act_ms(stray), None, "no round, nowhere to keep freshness");
        let round_id = Uuid::new_v4();
        open_round(round_id, "swe-bench-verified", WorkDriver::Citizen);
        add_card(round_id, card_uuid);
        record_card_worked(card_uuid, now_ms - 30_000);
        assert_eq!(card_last_act_ms(card_uuid), Some(now_ms - 30_000));

        let mut rounds = vec![RoundSnapshot {
            opened_at_ms: 0,
            round_id: Uuid::new_v4().to_string(),
            run_room_name: String::new(),
            benchmark: "swe-bench-verified".into(),
            stage: "working".into(),
            dispatched: 1,
            settled: 0,
            remaining: 1,
            driver: "citizen".into(),
            cards: vec![RoundCardSnapshot {
                card_id: card_uuid.to_string(),
                instance: "django__django-12273".into(),
                assignee: String::new(),
                solve_room_name: String::new(),
                state: "unstarted".into(),
                acts: None,
                patch_bytes: None,
                last_act_secs: None,
                resolved: None,
                board_state: String::new(),
                owner: String::new(),
                created_at_ms: None,
                updated_at_ms: None,
                graded_at_ms: None,
            }],
            verdict: String::new(),
            idle_secs: None,
        }];
        // EMPTY run ledger — the detached-solve facts a Citizen card never produces.
        enrich_rounds(&mut rounds, &[], now_ms);

        assert_eq!(
            rounds[0].cards[0].state, "working",
            "held-work makes the card read working, not a false 'unstarted'"
        );
        assert_eq!(
            rounds[0].idle_secs,
            Some(30),
            "the round's idle clock reflects her held-work turn"
        );
        assert_eq!(
            rounds[0].verdict, "grinding",
            "the round is visibly being worked"
        );
    }

    // what this catches: the round advancing only by TIMEOUT, and the loop the
    // naive fix creates. Measured 2026-08-28 — a run can finish WITHOUT settling
    // its card (no diff, env absent, cached env would not re-point). Those emit
    // no card-settled event, so the settle edge never fires and the round sat
    // idle behind one finished solve until the 5-minute becalmed watchdog
    // noticed (01:58 and 03:30 both did exactly that).
    //
    // The exclusion IS the loop guard: after a non-settling run the card is
    // still unsettled and no longer live, so the unguarded decision hands back
    // the very card that just failed — forever. Both halves are pinned here.
    #[test]
    fn the_non_settling_edge_advances_past_the_card_that_just_failed() {
        fn round_with(cards: &[(Uuid, Uuid)]) -> BenchRound {
            BenchRound {
                opened_at_ms: 0,
                run_room_name: String::new(),
                round_id: Uuid::new_v4(),
                benchmark: "swe-bench-lite".into(),
                cards: cards.iter().map(|(c, _)| (*c, None)).collect(),
                stage: RoundStage::Working,
                driver: WorkDriver::DetachedSolve,
                card_activities: Default::default(),
                card_assignees: cards.iter().copied().collect(),
                card_instances: Default::default(),
                card_last_act_ms: HashMap::new(),
                card_idle_since: HashMap::new(),
            review_gate: false,
            review_cards: HashMap::new(),
            reviews_passed: Default::default(),
                team: Vec::new(),
            team_names: Vec::new(),
            coursework: None,
            }
        }
        let live = std::collections::HashSet::new();
        let who = Uuid::new_v4();
        let failed = Uuid::new_v4();
        let next = Uuid::new_v4();

        // THE LOOP GUARD. One card left, and it is the one that just failed:
        // unguarded the decision hands it straight back (re-firing forever);
        // excluded, the chain honestly ends.
        let only = round_with(&[(failed, who)]);
        assert_eq!(
            first_unworked(&only, &live).map(|n| n.card),
            Some(failed),
            "sanity: unguarded, the just-failed card is handed back — the tight loop"
        );
        assert!(
            first_unworked_excluding(&only, &live, Some(failed)).is_none(),
            "the last card failing must end the chain, never re-fire itself"
        );

        // THE ADVANCE. Real work remains, so the round moves NOW rather than
        // waiting out the 5-minute becalmed watchdog.
        let two = round_with(&[(failed, who), (next, who)]);
        assert_eq!(
            first_unworked_excluding(&two, &live, Some(failed)).map(|n| n.card),
            Some(next),
            "the non-settling edge must advance PAST the card that just failed"
        );
    }


    use super::*;


    /// what this catches: an idle claim's clock restarting at every boot (a card
    /// claimed 11 h with zero acts waited 3 h more after a deploy), or surviving a
    /// change of hands.
    #[test]
    fn the_idle_clock_is_on_the_round_clears_on_an_act_and_restarts_for_a_new_holder() {
        let round = Uuid::new_v4();
        open_round(round, "swe-bench-verified", WorkDriver::Citizen);
        let card = Uuid::new_v4();
        add_card(round, card);
        let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
        assert_eq!(idle_clock_for(card, a, 1_000), 1_000, "first sight starts the clock");
        assert_eq!(idle_clock_for(card, a, 5_000), 1_000, "later passes keep it (persisted on the round)");
        assert_eq!(idle_clock_for(card, b, 6_000), 6_000, "a new holder restarts it");
        record_card_worked(card, 7_000);
        assert_eq!(idle_clock_for(card, b, 8_000), 7_000, "an act is the clock and clears the mark");
    }
    /// what this catches: a resolved instance whose board card sat in REVIEW (or whose
    /// review card nobody pulled) counting as unsettled forever — the round never
    /// settles, the standing autopilot never advances (2026-09-14: django-12663 resolved
    /// 18 h earlier, round 7/8, standing skipped every tick).
    #[test]
    fn a_verdicted_card_the_board_never_closed_is_named_for_the_reconciler() {
        let round = Uuid::new_v4();
        open_round(round, "swe-bench-verified", WorkDriver::Citizen);
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        add_card(round, a);
        add_card(round, b);
        record_card_instance(a, "django__django-12663");
        record_card_instance(b, "django__django-11211");
        let review = Uuid::new_v4();
        register_review_card(a, review);
        assert_eq!(review_card_of(a), Some(review));
        assert_eq!(review_card_of(b), None);
        let hits = unsettled_cards_with(|i| (i == "django__django-12663").then_some(true));
        assert_eq!(hits.iter().filter(|(c, _, _)| *c == a).count(), 1, "the verdicted, unsettled card is named");
        assert!(hits.iter().all(|(c, _, _)| *c != b), "no verdict, not named");
        settle_card_direct(a, "closed");
        assert!(unsettled_cards_with(|_| Some(true)).iter().all(|(c, _, _)| *c != a), "settled = not named again");
    }


    /// what this catches: a working round's open card missing from the reconciler's
    /// list (it would never be settled from the board), or a settled one still on it.
    #[test]
    fn unsettled_cards_lists_exactly_the_open_cards_of_working_rounds() {
        let round = Uuid::new_v4();
        open_round(round, "swe-bench-verified", WorkDriver::Citizen);
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        add_card(round, a);
        add_card(round, b);
        record_card_instance(a, "sympy__sympy-1");
        settle_card_direct(b, "closed");
        let mine: Vec<_> = unsettled_cards().into_iter().filter(|(r, _, _)| *r == round).collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].1, a);
        assert_eq!(mine[0].2, "sympy__sympy-1");
    }

    /// what this catches: a round change that never reaches the bundle (the
    /// tracker's second copy shadowing the room, card c9ddb911), and a restore
    /// that overwrites this process's own state with an older bundle.
    #[test]
    fn a_persisted_change_is_dirty_for_the_bundle_and_a_restore_never_overwrites_a_known_round() {
        let id = Uuid::new_v4();
        open_round(id, "swe-bench-verified", WorkDriver::Citizen);
        set_run_room_name(id, "bench-test-bundle");
        let dirty = take_dirty_bundles();
        let mine = dirty.iter().find(|d| d.round_id == id).expect("the opened round is dirty"); // JUSTIFIED: the test opened it
        assert_eq!(mine.run_room_name, "bench-test-bundle");
        assert_eq!(mine.json["run_room_name"], "bench-test-bundle");
        assert!(take_dirty_bundles().iter().all(|d| d.round_id != id), "drained");
        assert_eq!(restore_round_from_bundle(&mine.json).expect("a round"), None, "known round: own state wins"); // JUSTIFIED: the value came from this tracker
        let mut foreign = mine.json.clone();
        let other = Uuid::new_v4();
        foreign["round_id"] = Value::String(other.to_string());
        assert_eq!(restore_round_from_bundle(&foreign).expect("a round"), Some(other)); // JUSTIFIED: a copy of a value this tracker produced
        assert!(live_rounds().iter().any(|r| r.round_id == other.to_string()), "restored round is live");
        assert!(restore_round_from_bundle(&serde_json::json!({"not": "a round"})).is_err());
    }
    fn cards(n: usize) -> Vec<Uuid> {
        (0..n).map(|_| Uuid::new_v4()).collect()
    }

    // what this catches: the reboot amnesia measured twice on 2026-08-21 — a live
    // round answered `rounds: []` after restart, and (silent, worse) driver_for_card
    // fell back to DetachedSolve for a Citizen round's remaining cards, quietly
    // rebuilding the parallel runner the round was configured to avoid. A round must
    // round-trip its FULL deciding state: driver, card set, and which cards settled.
    #[test]
    fn a_round_survives_a_core_restart_with_driver_and_settles_intact() {
        let dir = std::env::temp_dir().join(format!("bench-rounds-test-{}", Uuid::new_v4()));
        let (id, cs) = (Uuid::new_v4(), cards(3));
        let mut round = BenchRound::new(id, "swe-bench-lite", &cs, WorkDriver::Citizen);
        assert!(matches!(round.settle_card(cs[0], "closed"), SettleOutcome::Settled { remaining: 2 }));
        persist_round_in(&dir, &round);

        // "reboot": reload from disk into a fresh map.
        let reloaded = load_rounds_in(&dir);
        let r = reloaded.get(&id).expect("in-flight round must reload");
        assert_eq!(r.driver, WorkDriver::Citizen, "driver reverting is the silent bug");
        assert_eq!(r.dispatched(), 3);
        assert_eq!(r.remaining(), 2, "the settled card must stay settled across the restart");
        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: mint-or-rejoin losing its memory across a reboot. The
    // card's solve ACTIVITY (its per-instance room + assignee) is recorded at
    // first mint and must survive persistence — a re-fire that can't find it
    // would mint a SECOND room for the same work, stranding the first room's
    // transcript and cold-starting the KV slot the activity had warmed
    // (Joel 2026-08-26: "benchmarks without new activities (unless rejoining)").
    #[test]
    fn card_activity_survives_a_restart_for_rejoin() {
        let dir = std::env::temp_dir().join(format!("bench-rounds-test-{}", Uuid::new_v4()));
        let (id, cs) = (Uuid::new_v4(), cards(2));
        let mut round = BenchRound::new(id, "swe-bench-verified", &cs, WorkDriver::DetachedSolve);
        let act = CardActivity {
            room_name: String::new(),
            teammates: Vec::new(), // test row: solo

            solve_room: Uuid::from_u128(0xA11CE),
            assignee: Uuid::from_u128(0xBEE),
        };
        round.card_activities.insert(cs[0], act.clone()); // clone: the assert below compares against the original
        persist_round_in(&dir, &round);

        let reloaded = load_rounds_in(&dir);
        let r = reloaded.get(&id).expect("round reloads");
        let got = r.card_activities.get(&cs[0]).cloned().expect("activity survives");
        assert_eq!(got.solve_room, act.solve_room, "rejoin returns the SAME room");
        assert_eq!(got.assignee, act.assignee);
        assert!(r.card_activities.get(&cs[1]).is_none(), "unminted card has no activity yet");
        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: the ONE driver decision. A settled card must yield the
    // round's next unworked card (with its staged assignee and the run room),
    // a fully-fired round must yield None, and a Citizen-driver round must
    // never be driven by the follow-on (citizens drive themselves via claims).
    // regression for the solve_cap starvation: dispatched − solves_fired used
    // to be PERMANENT — cards 2..N were posted, kicked off, and never worked.
    #[test]
    fn next_unworked_after_walks_the_round_to_empty() {
        let (id, cs) = (Uuid::new_v4(), cards(3));
        register_round(id, "swe-bench-verified", &cs);
        {
            let mut rounds = ROUNDS.lock().unwrap();
            let r = rounds.get_mut(&id).unwrap();
            r.driver = WorkDriver::DetachedSolve;
            for (i, c) in cs.iter().enumerate() {
                r.card_assignees.insert(*c, Uuid::from_u128(100 + i as u128));
            }
        }
        // Settle card 0 → the driver owes one of the remaining two.
        {
            let mut rounds = ROUNDS.lock().unwrap();
            rounds.get_mut(&id).unwrap().settle_card(cs[0], "closed");
        }
        let next = next_unworked_after(cs[0]).expect("two cards remain");
        assert!(cs[1..].contains(&next.card), "next is an UNSETTLED card");
        assert_eq!(next.run_room, id, "the dispatch rejoins the run room's board");
        let expected_assignee = {
            let rounds = ROUNDS.lock().unwrap();
            *rounds.get(&id).unwrap().card_assignees.get(&next.card).unwrap()
        };
        assert_eq!(next.assignee, expected_assignee, "the card's STAGED assignee carries");
        // Settle everything → the driver goes quiet.
        {
            let mut rounds = ROUNDS.lock().unwrap();
            let r = rounds.get_mut(&id).unwrap();
            r.settle_card(cs[1], "closed");
            r.settle_card(cs[2], "closed");
        }
        // The round is Done and REMOVED by observe_card_event in production; here the
        // map still holds it, which is exactly what lets us assert the quiet case.
        assert!(next_unworked_after(cs[0]).is_none(), "a finished round owes nothing");

        // A Citizen round is never follow-on driven.
        let (cid, ccs) = (Uuid::new_v4(), cards(2));
        register_round(cid, "swe-bench-verified", &ccs);
        assert!(next_unworked_after(ccs[0]).is_none(), "citizen rounds drive themselves");
    }

    // what this catches: the state dir growing without an eviction story (the
    // 2026-07-13 rule). Done is terminal — its file is removed at settle, and a
    // Done file that survives a crash window is dropped (and deleted) on reload
    // rather than resurrected as a round no event can ever settle.
    #[test]
    fn done_rounds_never_reload_and_their_files_self_evict() {
        let dir = std::env::temp_dir().join(format!("bench-rounds-test-{}", Uuid::new_v4()));
        let (id, cs) = (Uuid::new_v4(), cards(1));
        let mut round = BenchRound::new(id, "swe-bench-lite", &cs, WorkDriver::Citizen);
        persist_round_in(&dir, &round);
        assert!(matches!(round.settle_card(cs[0], "closed"), SettleOutcome::RoundDone));
        // Crash window: the round reached Done but its file removal never ran.
        persist_round_in(&dir, &round);
        let reloaded = load_rounds_in(&dir);
        assert!(reloaded.is_empty(), "a Done round must not resurrect");
        assert!(
            !dir.join(format!("{id}.json")).exists(),
            "the Done file must be evicted on the reload that drops it"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Open a round and add its cards — the dispatch sequence, for tests that only care
    /// about the finished set.
    fn register_round(round_id: Uuid, benchmark: &str, card_ids: &[Uuid]) {
        open_round(round_id, benchmark, WorkDriver::default());
        for c in card_ids {
            add_card(round_id, *c);
        }
        seal_round(round_id);
    }

    // what this catches: the window between "card posted" and "round registered". Dispatch
    // sends a kickoff inside its card loop, so a citizen can claim card 1 while card 2 is
    // still being posted — and `work/claim` asks `driver_for_card` whether to fire the
    // detached solver. If the driver were only readable after the whole loop, a
    // Citizen-driven round would fire the solver on its first card and defeat itself
    // silently. The driver must be right from the first `add_card`, before `seal_round`.
    #[test]
    fn the_driver_is_readable_the_instant_a_card_can_be_claimed() {
        let round_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        open_round(round_id, "swe-bench-lite", WorkDriver::Citizen);
        add_card(round_id, first);
        // Mid-loop: the round is NOT sealed, a second card is not posted yet.
        assert_eq!(
            driver_for_card(first),
            WorkDriver::Citizen,
            "a claim landing mid-dispatch must see the round's real driver"
        );
        seal_round(round_id);
        assert_eq!(driver_for_card(first), WorkDriver::Citizen);
        ROUNDS.lock().unwrap().remove(&round_id);  // test: cleanup of the round opened above
    }

    // what this catches: the default biting the wrong way — INVERTED 2026-08-31
    // (PREMISE CHANGE with the WorkDriver default flip). A card belonging to no
    // live round — human-claimed, undirected, or a leftover claimed after its
    // round ended — drives CITIZEN: the claimer works it in her own service
    // loop, turns feed the flywheel. Detached is the per-round diagnostic
    // opt-in, never what an ordinary claim silently falls into (four detached
    // rounds ran invisible all night on the old default).
    #[test]
    fn a_card_in_no_round_drives_by_citizen() {
        assert_eq!(driver_for_card(Uuid::new_v4()), WorkDriver::Citizen);
    }

    // what this catches: a round that dispatched nothing must END, not sit in the map
    // forever waiting for an event that can never arrive (no cards = no card events).
    #[test]
    fn an_empty_round_seals_straight_to_done_and_leaves_no_entry() {
        let round_id = Uuid::new_v4();
        open_round(round_id, "swe-bench-lite", WorkDriver::DetachedSolve);
        seal_round(round_id);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_none(),
            "an empty round must not remain tracked"
        );
    }

    // what this catches: the ZOMBIE round (found live 2026-09-01) — a core
    // reload leaves a round WORKING with no live run and no settle edge ever
    // coming, and resume_round used to no-op on anything not Paused, so the
    // one verb whose doc promises "the driver is kicked immediately" left the
    // exact stalled state it exists for. A Working round with no live run must
    // hand back its first unworked card.
    #[test]
    fn resume_kicks_a_working_idle_round() {
        let ids = cards(2);
        let round_id = Uuid::new_v4();
        let mut r = BenchRound::new(round_id, "swe-bench-lite", &ids, WorkDriver::DetachedSolve);
        for c in &ids {
            r.card_assignees.insert(*c, Uuid::new_v4());
        }
        ROUNDS.lock().unwrap().insert(round_id, r);
        let next = resume_round(round_id);
        assert!(
            next.as_ref().is_some_and(|n| ids.contains(&n.card)),
            "a WORKING round with no live run must kick its first unworked card: {next:?}"
        );
        ROUNDS.lock().unwrap().remove(&round_id);  // test: cleanup of the round opened above
    }

    // what this catches: the END transition firing more than once. All-settled must yield
    // RoundDone exactly once — a re-delivered terminal event after Done must be a no-op,
    // or the room would get N "round over" announcements for one round.
    #[test]
    fn all_settled_transitions_to_done_exactly_once() {
        let ids = cards(3);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.stage(), RoundStage::Working);
        assert_eq!(r.settle_card(ids[0], "closed"), SettleOutcome::Settled { remaining: 2 });
        assert_eq!(r.settle_card(ids[1], "merged"), SettleOutcome::Settled { remaining: 1 });
        assert_eq!(r.settle_card(ids[2], "closed"), SettleOutcome::RoundDone);
        assert_eq!(r.stage(), RoundStage::Done);
        // Any further event on an already-settled card is inert — never a second Done.
        assert_eq!(r.settle_card(ids[2], "closed"), SettleOutcome::AlreadySettled);
        assert_eq!(r.stage(), RoundStage::Done);
    }

    // what this catches: the projection disagreeing with the entity it projects — a
    // "settled" count that drifts from the round's own card map is worse than no query at
    // all, because a driver would BELIEVE it. Also pins that progress is legible mid-round:
    // the whole point of #371 is answering "how far along" without reading a log.
    // what this catches (2026-09-13): a card the board reopened staying settled in the
    // tracker, so the pull never offers it again and a finished round never resumes.
    #[test]
    fn a_board_reopen_returns_a_settled_card_to_the_deck() {
        let round_id = Uuid::new_v4();
        let ids = cards(2);
        open_round(round_id, "swe-bench-verified", WorkDriver::Citizen);
        for id in &ids {
            add_card(round_id, *id);
        }
        // Settle ONE card: the round stays Working (a finished round leaves the tracker,
        // and a card of a finished round is a new dispatch, not a reopen).
        settle_card_direct(ids[0], "closed");
        let mid = live_rounds().into_iter().find(|r| r.round_id == round_id.to_string()).expect("tracked");
        assert_eq!((mid.settled, mid.remaining), (1, 1));
        assert!(reopen_card(ids[0]), "the tracker held it settled");
        assert!(!reopen_card(ids[0]), "already unsettled: nothing to undo");
        let again = live_rounds().into_iter().find(|r| r.round_id == round_id.to_string()).expect("tracked");
        assert_eq!((again.stage.as_str(), again.settled, again.remaining), ("working", 0, 2));
        let resident: std::collections::HashSet<Uuid> = [round_id].into_iter().collect();
        assert!(
            pullable_cards(Uuid::new_v4(), &resident).iter().any(|c| c.card == ids[0]),
            "the reopened card is offered by the pull again"
        );
        ROUNDS.lock().unwrap().remove(&round_id); // test: cleanup of the round opened above
    }

    #[test]
    fn a_round_in_flight_reports_its_own_progress_honestly() {
        let round_id = Uuid::new_v4();
        let ids = cards(3);
        open_round(round_id, "swe-bench-lite", WorkDriver::Citizen);
        for id in &ids {
            add_card(round_id, *id);
        }

        let before = live_rounds();
        let row = before
            .iter()
            .find(|r| r.round_id == round_id.to_string())
            .expect("a dispatched round must be QUERYABLE — that is the whole fix");
        assert_eq!(row.stage, "working");
        assert_eq!((row.dispatched, row.settled, row.remaining), (3, 0, 3));
        assert_eq!(
            row.driver, "citizen",
            "the driver decides whether this round teaches anybody anything — it must be readable"
        );

        // Settle one, and the projection must move WITH the entity, not lag it.
        ROUNDS
            .lock()
            .unwrap()
            .get_mut(&round_id)
            .expect("still in flight")
            .settle_card(ids[0], "closed");
        let mid = live_rounds();
        let row = mid
            .iter()
            .find(|r| r.round_id == round_id.to_string())
            .expect("two of three cards are still working, so the round is still in flight");
        assert_eq!(
            (row.dispatched, row.settled, row.remaining),
            (3, 1, 2),
            "settled is reported, never left to subtraction — an absence must not become a guess"
        );

        ROUNDS.lock().unwrap().remove(&round_id);  // test: cleanup of the round opened above
    }

    // what this catches: a duplicate terminal event double-counting a card. The bridge
    // dedupes by event id, but a card can legitimately transition closed→merged; the
    // second settle must not decrement `remaining` a second time (a round declared Done
    // with cards still working is a lying END).
    #[test]
    fn a_card_settling_twice_does_not_double_count() {
        let ids = cards(2);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.settle_card(ids[0], "closed"), SettleOutcome::Settled { remaining: 1 });
        assert_eq!(r.settle_card(ids[0], "merged"), SettleOutcome::AlreadySettled);
        assert_eq!(r.remaining(), 1, "the duplicate must not consume the other card's slot");
        assert_eq!(r.stage(), RoundStage::Working);
    }

    // what this catches: a card from OUTSIDE the round's dispatched set (another round's
    // card, a hand-written work card) advancing this round. Membership is the set
    // dispatch posted, nothing else.
    #[test]
    fn a_card_outside_the_set_is_ignored() {
        let ids = cards(1);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.settle_card(Uuid::new_v4(), "closed"), SettleOutcome::NotOurs);
        assert_eq!(r.remaining(), 1);
        assert_eq!(r.stage(), RoundStage::Working);
    }

    // what this catches: the wire seam. `observe_card_event` must parse the #450 bridge
    // payload contract ({card_id: hyphenated uuid, state: serde CardState}) into the
    // state machine — and a non-terminal transition (in_progress) must not settle. Uses
    // the REAL global registry with a unique round, so it also pins register→observe→
    // remove-on-done end to end.
    #[test]
    fn observe_parses_the_bridge_payload_and_removes_the_round_on_done() {
        let ids = cards(1);
        let round_id = Uuid::new_v4();
        register_round(round_id, "humaneval-rs", &ids);
        let in_progress = serde_json::json!({
            "card_id": ids[0].to_string(),
            "state": "in_progress",
            "room_id": round_id.to_string(),
        });
        observe_card_event(&in_progress);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_some_and(|r| r.remaining() == 1),
            "a non-terminal transition must not settle the card"
        );
        let closed = serde_json::json!({
            "card_id": ids[0].to_string(),
            "state": "closed",
            "room_id": round_id.to_string(),
        });
        observe_card_event(&closed);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_none(),
            "the round completed and must be removed — done can never fire twice"
        );
    }

    // what this catches: regression for the #371 no-END zombie — a `Working` round left on
    // disk by a dead core is reaped at boot (evicted + reported with benchmark+remaining),
    // so benchmark/rounds stops counting it in_flight forever (measured live: in_flight: 4
    // with a stopped exam lease). A `Done` file is NOT the reaper's job (load_rounds_in
    // drops those). Idempotent: a second boot reap finds nothing.
    #[test]
    fn working_rounds_survive_the_boot_reap_and_only_ttl_expiry_removes_them() {
        // what this catches: the A5 continuity contract. The old reaper DELETED
        // every Working round at boot — destroying exactly the state (room, cards,
        // assignees, activity rooms) the resume needs to rejoin. A Working round
        // must SURVIVE the reap; only a TTL-stale abandoned file is expired (the
        // state dir's eviction story, 2026-07-13 rule).
        let dir = std::env::temp_dir().join(format!("bench-reap-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let working =
            BenchRound::new(Uuid::new_v4(), "swe-bench-lite", &cards(3), WorkDriver::Citizen);
        persist_round_in(&dir, &working);

        // Default TTL: a fresh Working round is untouchable.
        assert!(
            reap_orphaned_rounds_in(&dir).is_empty(),
            "a live Working round SURVIVES the boot reap — continuity is the default"
        );
        assert_eq!(
            std::fs::read_dir(&dir).expect("readdir").flatten().count(),
            1,
            "the round file is still on disk for the resume to rejoin"
        );

        // TTL zero: the same file is expired (abandoned-round backstop).
        let expired = reap_with_ttl(&dir, std::time::Duration::ZERO);
        assert_eq!(expired, vec![("swe-bench-lite".to_string(), 3)]);
        assert_eq!(
            std::fs::read_dir(&dir).expect("readdir").flatten().count(),
            0,
            "an expired round's file is removed"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
