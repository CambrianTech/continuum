//! Her work in hand through a start (`Item::in_hand`): the file she is editing and her note on
//! what her last change did stay whole, charged to their source's grant, while everything else
//! still collapses oldest first. Shaped on Aris on the M5, 2026-11-14 00:24-00:28Z: two overflow
//! turnovers collapsed the 849-line file she was editing (a ~38k-char read) to its outline, and
//! she read it whole again after each.

use super::*;
use crate::fixtures::item;

/// Her acts: each `(key, body, in hand)`, collapsing to a short pointer, the oldest run folding
/// into one line naming its ends.
struct Work(Vec<(String, String, bool)>);

impl Work {
    /// Her old acts `#0..#old`, then the file she is editing (in hand when `held`), then her newer
    /// acts up to `#newest`, each act `act` chars and the file `file` chars.
    fn editing(old: usize, newest: usize, act: usize, file: usize, held: bool) -> Self {
        let mut items: Vec<(String, String, bool)> = (0..old).map(|n| (format!("#{n}"), format!("act {n} ").repeat(act / 6), false)).collect();
        items.push(("file".to_owned(), "fn actuate() {}\n".repeat(file / 16), held));
        items.extend((old..newest).map(|n| (format!("#{n}"), format!("act {n} ").repeat(act / 6), false)));
        Self(items)
    }

    fn body(&self, key: &str) -> &str {
        self.0.iter().find(|(k, _, _)| k == key).map_or("", |(_, body, _)| body.as_str()) // map_or: a key the fixture lacks has no body
    }
}

impl Source<String> for Work {
    fn name(&self) -> &str {
        "acts"
    }
    fn priority(&self) -> u8 {
        5
    }
    fn items(&self) -> Vec<Item<String>> {
        self.0
            .iter()
            .map(|(key, body, held)| {
                let item = item(key.clone(), body.clone(), None, Some(format!("[{key}: collapsed, reopen by key]")));
                if *held { item.in_hand() } else { item }
            })
            .collect()
    }
    fn fold(&self, folded: &[Item<String>]) -> Option<String> {
        Some(format!("[{}..{}: folded]", folded.first()?.key(), folded.last()?.key()))
    }
}

/// A start of `work` whose one grant is what `grant` answers for its claim.
fn started(work: &Work, grant: impl Fn(&Claim) -> usize) -> View<String> {
    let (view, _) = View::start(&crate::cost::Chars, Vec::new(), &[work as &dyn Source<String>], |claims| claims.iter().map(&grant).collect())
        .expect("one grant per claim"); // expect: the closure answers every claim
    view
}

// what this catches: the turnover that took Aris's file. Under a grant that holds her newest act and
// the file she is editing beside everything else at its floor, the file stays whole although four
// newer acts collapse; the same start without the file in hand spends the room newest first and
// collapses the file. Her claim's newest end counts the file, so the start that sizes the grant
// covers it like her newest.
#[test]
fn a_start_keeps_the_file_she_is_editing_whole_and_collapses_newer_acts() {
    let held = Work::editing(4, 9, 1_200, 4_000, true);
    let claimed = claims(&crate::cost::Chars, &[&held as &dyn Source<String>]);
    let newest = held.body("#8").len();
    assert_eq!(claimed[0].newest, newest + held.body("file").len(), "her newest end is her newest act and the file in hand");
    let grant = |claim: &Claim| claim.floor + claim.newest + 200;

    let view = started(&held, grant);
    let shown = rendered(&view);
    assert!(view.renders("acts", "file") && shown.contains(held.body("file")), "the file she is editing is whole");
    assert!(shown.contains(held.body("#8")), "her newest act is whole");
    assert!(!view.renders("acts", "#5"), "a newer act than the file collapses: the rest keeps oldest-first");

    let unheld = Work::editing(4, 9, 1_200, 4_000, false);
    let view = started(&unheld, |claim| grant(&Claim { newest: claim.newest + held.body("file").len(), ..claim.clone() }));
    assert!(!view.renders("acts", "file"), "without her edit in hand the same room goes to the newest acts and the file collapses");
}

// what this catches: a working item minted budget. A grant that cannot hold the file whole beside
// the rest at their floor gives the file up (its pointer), never a start over its grant.
#[test]
fn a_grant_that_cannot_hold_the_file_gives_it_up_and_never_overspends() {
    let work = Work::editing(4, 9, 1_200, 4_000, true);
    let view = started(&work, |claim| claim.floor + 100);
    assert!(!view.renders("acts", "file"), "the file gave way");
    let floor = claims(&crate::cost::Chars, &[&work as &dyn Source<String>])[0].floor;
    assert!(view.chars() <= floor + 100, "the start kept within its grant: {} chars on {}", view.chars(), floor + 100);
}

// what this catches: holding one read costing the floor of every act after it. Her oldest run still
// folds into one block around the file she holds, acts newer than the file included, while the file
// stands whole where it is; her newest act is whole.
#[test]
fn the_oldest_run_folds_around_her_work_in_hand() {
    let work = Work::editing(6, 10, 1_200, 4_000, true);
    let view = started(&work, |claim| claim.floor + claim.newest);
    assert!(view.renders("acts", "file") && view.renders("acts", "#9"), "the file and her newest act are whole");
    assert_eq!(view.fold_edge("acts"), Some("#8"), "the run folds past the file, every act but her newest");
    assert_eq!(rendered(&view).matches(work.body("file")).count(), 1, "the file once, whole");
}
