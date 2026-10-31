#![cfg(test)]
use super::*;

// what this catches (card c8303c32, Kimi 2026-09-21; Astra's block on #4315): a checkpoint
// that rooted her at a card is restored and the board no longer counts the card as hers.
// The wake carries the believed card until the board ANSWERS (peek does not consume; take
// does), and the notice says what the board said — held by a peer, lapsed and open,
// moved on to Review/Merged/Closed, or UNKNOWN because no board she stands in shows it —
// never "nobody holds it" from missing visibility, never "X took it" for an expired
// owner field that is merely history. Two work turns, then gone.
#[test]
fn a_claim_gone_while_she_slept_is_said_from_the_boards_verdict_and_unknown_is_not_vacant() {
    let believed = uuid::Uuid::new_v4();
    let wm = WorkingMemory::new(3);
    let mut snap = wm.snapshot();
    snap.acting_card = Some(believed);
    let woke = WorkingMemory::new(3);
    woke.restore(snap);
    assert_eq!(woke.peek_restored_acting_card(), Some(believed), "peek keeps the belief for a read with no verdict");
    assert_eq!(woke.peek_restored_acting_card(), Some(believed));
    assert_eq!(woke.take_restored_acting_card(), Some(believed), "take consumes it once the board answered");
    assert_eq!(woke.take_restored_acting_card(), None, "and only once");

    let peer = uuid::Uuid::new_v4();
    let facts = |wm: &WorkingMemory| wm.pinned_facts().join("\n");

    woke.note_claim_gone(believed, ClaimGone::HeldBy { peer, expires_at_ms: Some(1_790_000_000_000) });
    let f = facts(&woke);
    assert!(f.contains("NO LONGER YOURS") && f.contains(&airc_core::shown_id::shown_form(peer)) && f.contains("lease edge"), "{f}");
    assert!(f.contains("board now shows") && !f.contains("took it") && !f.contains("lapsed"), "observed state, not a story: {f}");
    assert!(f.contains("only when it reads claimable") && f.contains("untouched"), "{f}");

    woke.note_claim_gone(believed, ClaimGone::Lapsed { expired_at_ms: None });
    let f = facts(&woke);
    assert!(f.contains("nobody holds it") && f.contains(&format!("work/claim {believed}")), "{f}");
    assert!(!f.contains("lease edge"), "no edge invented: {f}");

    woke.note_claim_gone(believed, ClaimGone::MovedOn { state: "review" });
    let f = facts(&woke);
    assert!(f.contains("shows it in review") && f.contains("not a lapse") && !f.contains("work/claim"), "{f}");

    woke.note_claim_gone(believed, ClaimGone::Unknown);
    let f = facts(&woke);
    assert!(f.contains("no board you stand in shows it") && f.contains("before assuming"), "{f}");
    assert!(!f.contains("nobody holds it") && !f.contains("took it"), "unknown is not vacant and names no thief: {f}");

    // Two work turns, then gone — the wake's turn and the one that decides the pull.
    woke.end_of_work_turn();
    assert!(facts(&woke).contains("[claim]"), "still there for the pull decision");
    woke.end_of_work_turn();
    assert!(!facts(&woke).contains("[claim]"), "and not a standing condition");

    // A snapshot rooted nowhere has nothing to compare.
    let home = WorkingMemory::new(3);
    home.restore(WorkingMemory::new(3).snapshot());
    assert_eq!(home.peek_restored_acting_card(), None);
}

// what this catches: a rooting fact aging out of the three-deep window mid-turn.
// Pinned facts survive any number of recorded entries, are replaced by key (a
// re-root overwrites, never duplicates), and leave when unpinned — while the
// FIFO keeps aging out as before.
#[test]
fn a_pinned_fact_survives_the_window_and_is_replaced_by_key() {
    let wm = WorkingMemory::new(3);
    wm.pin_fact("hands", "[hands] rooted at /a");
    wm.pin_fact("env", "[env] python at /a/env/bin/python");
    for i in 0..12 {
        wm.record_fact(&format!("chatty fact {i}"));
    }
    assert_eq!(
        wm.entries.lock().len(),
        3,
        "the FIFO still ages out at capacity"
    );
    assert_eq!(
        wm.pinned_facts(),
        vec![
            "[hands] rooted at /a".to_string(),
            "[env] python at /a/env/bin/python".to_string()
        ]
    );
    wm.pin_fact("hands", "[hands] rooted at /b");
    assert_eq!(
        wm.pinned_facts()[0],
        "[hands] rooted at /b",
        "replaced by key, never duplicated"
    );
    assert_eq!(wm.pinned_facts().len(), 2);
    wm.unpin_fact("hands");
    wm.unpin_fact("env");
    assert!(wm.pinned_facts().is_empty());
    // The input projection uses the same scope predicate as the memory trail.
    wm.set_scope(Some("/a".into()));
    {
        let mut entries = wm.entries.lock();
        entries[0].scope = Some("/a".into());
        entries[1].scope = Some("/b".into());
    }
    assert_eq!(wm.notices(), vec!["chatty fact 9", "chatty fact 11"]);
    wm.set_scope(Some("/b".into()));
    assert_eq!(wm.notices(), vec!["chatty fact 10", "chatty fact 11"]);
    // Turn lifetimes: a 1-turn pin dies at the first turn end, a 2-turn pin at the second.
    wm.pin_fact("hands", "[hands] rooted at /c");
    wm.pin_fact_for_turns("released", "[released] card x", 2);
    wm.end_of_work_turn();
    assert_eq!(
        wm.pinned_facts(),
        vec!["[released] card x".to_string()],
        "the release reaches the next turn"
    );
    wm.end_of_work_turn();
    assert!(wm.pinned_facts().is_empty(), "and leaves after it");
    assert!(!wm.notices().iter().any(|text| text.contains("[released]")));
}
