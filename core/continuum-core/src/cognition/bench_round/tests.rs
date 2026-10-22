#![cfg(test)]
    use super::*;

    fn cards(n: usize) -> Vec<Uuid> {
        (0..n).map(|_| Uuid::new_v4()).collect()
    }

    // what this catches: the END transition firing more than once. All-settled must yield
    // RoundDone exactly once — a re-delivered terminal event after Done is a no-op.
    #[test]
    fn all_settled_transitions_to_done_exactly_once() {
        let ids = cards(3);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids);
        assert_eq!(r.settle_card(ids[0], "closed"), SettleOutcome::Settled { remaining: 2 });
        assert_eq!(r.settle_card(ids[1], "merged"), SettleOutcome::Settled { remaining: 1 });
        assert_eq!(r.settle_card(ids[2], "closed"), SettleOutcome::RoundDone);
        assert_eq!(r.stage, RoundStage::Done);
        assert_eq!(r.settle_card(ids[2], "closed"), SettleOutcome::AlreadySettled);
        assert_eq!(r.stage, RoundStage::Done);
    }

    // what this catches: a duplicate terminal event double-counting a card, or a card
    // from outside the set advancing the round.
    #[test]
    fn a_card_settles_once_and_only_its_own_round() {
        let ids = cards(2);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids);
        assert_eq!(r.settle_card(ids[0], "closed"), SettleOutcome::Settled { remaining: 1 });
        assert_eq!(r.settle_card(ids[0], "merged"), SettleOutcome::AlreadySettled);
        assert_eq!(r.settle_card(Uuid::new_v4(), "closed"), SettleOutcome::NotOurs);
        assert_eq!(r.remaining(), 1);
        assert_eq!(r.stage, RoundStage::Working);
    }

    // what this catches: the wire seam — `observe_card_event` parses the bridge payload,
    // a non-terminal transition does not settle, and the round leaves on Done; an empty
    // round seals straight to done.
    #[test]
    fn observe_parses_the_bridge_payload_and_removes_the_round_on_done() {
        let empty = Uuid::new_v4();
        open_round(empty, "swe-bench-lite");
        seal_round(empty);
        assert!(rounds().get(&empty).is_none(), "an empty round must not remain tracked");

        let card = Uuid::new_v4();
        let round_id = Uuid::new_v4();
        open_round(round_id, "humaneval-rs");
        add_card(round_id, card);
        seal_round(round_id);
        observe_card_event(&serde_json::json!({"card_id": card.to_string(), "state": "in_progress"}));
        assert!(rounds().get(&round_id).is_some_and(|r| r.remaining() == 1));
        observe_card_event(&serde_json::json!({"card_id": card.to_string(), "state": "closed"}));
        assert!(rounds().get(&round_id).is_none(), "done can never fire twice");
    }

    // what this catches (2026-09-13): a card the board reopened staying settled in the
    // tracker; and a verdicted card the board never closed missing from the reconciler.
    #[test]
    fn a_reopened_or_verdicted_card_is_unsettled_again() {
        let round_id = Uuid::new_v4();
        let ids = cards(2);
        open_round(round_id, "swe-bench-verified");
        for id in &ids {
            add_card(round_id, *id);
        }
        record_card_instance(ids[0], "django__django-12663");
        record_card_instance(ids[1], "django__django-11211");
        settle_card_direct(ids[0], "closed");
        assert!(reopen_card(ids[0]), "the tracker held it settled");
        assert!(!reopen_card(ids[0]), "already unsettled: nothing to undo");
        let hits = unsettled_cards_with(|i| (i == "django__django-12663").then_some(true));
        assert_eq!(hits.iter().filter(|(c, _, _)| *c == ids[0]).count(), 1);
        assert!(hits.iter().all(|(c, _, _)| *c != ids[1]), "no verdict, not named");
        assert_eq!(cards_for_instance("django__django-11211"), vec![ids[1]]);
        rounds().remove(&round_id);
    }
