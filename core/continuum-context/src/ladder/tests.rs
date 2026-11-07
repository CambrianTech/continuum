use super::*;
use crate::level::Level;

/// An item whole, at a summary, at its pointer.
fn three(full: u32, summary: u32, pointer: u32) -> Rungs {
    Rungs::new([full, summary, pointer]).expect("test: levels") // JUSTIFIED: test fixture
}

// what this catches: a consumer that collapses its most important item (the last) before its
// least important ones, or shows an older item at more detail than a newer one: shrinking must be
// oldest (least important) first, never her newest (goal 7), the view's own rule (`view::allot`).
#[test]
fn reduce_collapses_the_least_important_first_and_never_an_older_item_above_a_newer() {
    let leveled = Leveled::new(vec![three(500, 100, 10), three(500, 100, 10), three(500, 100, 10)]);
    assert_eq!((leveled.want(), leveled.hard_limit()), (1_500, 30));
    assert_eq!(leveled.reduce(1_500).rungs, [0, 0, 0], "everything fits: everything whole");
    let tight = leveled.reduce(1_100);
    assert_eq!(tight.rungs, [1, 0, 0], "the oldest gives way first, to its summary: {tight:?}");
    let tighter = leveled.reduce(700);
    assert_eq!(tighter.rungs, [1, 1, 0], "then the next oldest: {tighter:?}");
    for target in (0..=1_600).step_by(7) {
        let choice = leveled.reduce(target);
        assert!(choice.cost <= target.max(leveled.hard_limit()), "target {target}: {choice:?}");
        let classes: Vec<_> = choice.rungs.iter().zip(leveled.items()).map(|(&rung, item)| item.class(rung)).collect();
        let rank = |l: &Level| match l { Level::Full => 2, Level::Summary => 1, _ => 0 };
        assert!(classes.windows(2).all(|w| rank(&w[0]) <= rank(&w[1])), "target {target}: an older item above a newer: {classes:?}");
    }
    assert_eq!(leveled.reduce(0), leveled.at_limit(), "below the hard limit it answers the hard limit");
}

// what this catches: a ladder of options that skips a reduction (the search could not choose it)
// or repeats a cost (wasted search cells): strictly decreasing, from every item whole to every item
// at its hard limit.
#[test]
fn options_run_from_whole_to_the_hard_limit_strictly_decreasing() {
    let leveled = Leveled::new(vec![three(900, 300, 20), Rungs::whole(40), three(400, 90, 15)]);
    let options = leveled.options();
    assert_eq!(options.first().map(|o| o.cost), Some(leveled.want()));
    assert_eq!(options.last().map(|o| o.cost), Some(leveled.hard_limit()));
    assert!(options.windows(2).all(|w| w[0].cost > w[1].cost), "{options:?}");
    assert!(options.iter().all(|o| o.rungs[1] == 0), "an item with one level is never reduced: {options:?}");
}

// what this catches: a producer's level that saves nothing becoming a rung (a search option that
// costs what the level above it does), or an empty item becoming a consumer with a phantom floor.
#[test]
fn a_level_no_smaller_than_the_one_above_is_not_a_rung() {
    assert_eq!(Rungs::new([300, 300, 120, 200, 40]).map(|r| r.costs().to_vec()), Some(vec![300, 120, 40]));
    assert_eq!(Rungs::new(Vec::new()), None);
    assert_eq!(Leveled::default().options(), vec![Choice::default()], "nothing to show is one option of nothing");
    let (rungs, tags) = Rungs::kept(300, "whole", [(300, "same"), (120, "summary"), (200, "bigger"), (40, "pointer")]);
    assert_eq!((rungs.costs().to_vec(), tags), (vec![300, 120, 40], vec!["whole", "summary", "pointer"]), "one tag per rung, aligned");
}

// what this catches (Cormac on #5426, the view's rule): an item with no level between its whole and
// its pointer collapsing to nothing because a newer item fell to its summary. It is its own summary:
// whole under a summary ceiling when it fits, and the ceiling holds for the items older than it.
#[test]
fn an_item_with_no_summary_stays_whole_under_a_summary_ceiling() {
    let leveled = Leveled::new(vec![three(500, 100, 10), Rungs::new([40, 0]).expect("test: levels"), three(900, 200, 10)]); // JUSTIFIED: test fixture
    let choice = leveled.reduce(200 + 40 + 100);
    assert_eq!(choice.rungs, [1, 0, 1], "the newest at its summary, the atomic item whole, the oldest at its summary: {choice:?}");
}
