//! A source absent from a start (shed for want of its floor, a stream that ended, a room she
//! left): its logged history leaves with it, charged to no one and never laid down (Codex on #5805).

use super::*;

/// `s`'s five reads, the oldest three collapsed into her log by a tight start, beside a kept source.
fn logged() -> (View<String>, Fake, Fake) {
    let mut s = Fake::new("s", 5);
    for n in 1..=5 {
        s.push(&format!("s{n}"), &format!("s{n} body ").repeat(40));
    }
    let mut k = Fake::new("k", 5);
    k.push("k1", "k1 body");
    let (mut view, _) = View::start(vec!["head".into()], &[&s, &k], |claims| claims.iter().map(|claim| claim.full_chars).collect()).unwrap(); // unwrap: one grant per claim cannot mismatch
    // Room for the floor and the newest two reads: the oldest three collapse into her log.
    let tight = |claims: &[Claim]| claims.iter().map(|claim| claim.floor_chars + 2 * claim.newest_chars).collect();
    view.turn_over(vec!["head".into()], &[&s, &k], tight).unwrap(); // unwrap: one grant per claim cannot mismatch
    assert!(rendered(&view).contains("[s1: "), "s1 stands in her log as its pointer");
    (view, s, k)
}

// what this catches: a start re-emitting a log entry of a source it was not given. Its chars were
// charged to no claim, so the log could outgrow the window with content nobody paid for, and the
// entry's item stayed marked collapsed, so the source's return showed her only pointers.
#[test]
fn an_absent_sources_log_leaves_with_it_and_comes_back_whole() {
    let (mut view, mut s, k) = logged();
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let start = view.turn_over(vec!["head".into()], &[&k], whole).unwrap(); // unwrap: one grant per claim cannot mismatch
    let text = rendered(&view);
    assert!(!text.contains("[s1: ") && !text.contains("s1 body"), "nothing of s is laid down: {text}");
    assert!(start.log_dropped_chars > 0, "the drop is receipted");
    assert_eq!(view.chars(), "head".len() + "k1 body".len(), "the start holds only what it charged");
    // s returns: its items arrive as themselves, none held collapsed by a log entry that left.
    s.push("s6", "s6 body");
    view.advance(&[&s, &k]);
    assert!(view.renders("s", "s1"), "s1 arrives whole, not as a stale pointer");
}

// what this catches: a start given no sources (or no grant) still laying down the whole log.
#[test]
fn a_start_with_no_sources_or_no_grant_lays_down_no_uncharged_log() {
    let (mut view, s, k) = logged();
    view.turn_over(vec!["head".into()], &[], |_| Vec::new()).unwrap(); // unwrap: no claims, no grants
    assert_eq!(rendered(&view), "head", "no source, no log");
    let (mut view, _, _) = logged();
    let start = view.turn_over(vec!["head".into()], &[&s, &k], |claims| vec![0; claims.len()]).unwrap(); // unwrap: one grant per claim cannot mismatch
    assert!(start.log_folded, "a log over its grant folds");
    assert!(view.chars() <= "head".len() + s.items().iter().chain(k.items().iter()).filter_map(|item| item.pointer().map(String::len)).sum::<usize>(), "no more than the floor");
}
