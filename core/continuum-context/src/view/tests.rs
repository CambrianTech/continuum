use super::*;
use crate::source::Item;

/// A source shaped like the M5's: items keyed, each renderable in full or as a pointer.
struct Fake {
    name: &'static str,
    priority: u8,
    items: Vec<(String, String)>,
    retired: Vec<(String, String)>,
}

impl Fake {
    fn new(name: &'static str, priority: u8) -> Self {
        Self { name, priority, items: Vec::new(), retired: Vec::new() }
    }
    fn push(&mut self, key: &str, text: &str) {
        match self.items.iter_mut().find(|(k, _)| k == key) {
            Some(item) => item.1 = text.to_owned(),
            None => self.items.push((key.to_owned(), text.to_owned())),
        }
    }
}

impl Source<String> for Fake {
    fn name(&self) -> &str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }
    fn items(&self) -> Vec<Item<String>> {
        self.items
            .iter()
            .map(|(key, text)| Item {
                key: key.clone(),
                full: text.clone(),
                pointer: Some(format!("[{key}: {} chars, reopen by key]", text.len())),
            })
            .collect()
    }
    fn retired(&self) -> Vec<Item<String>> {
        self.retired
            .iter()
            .map(|(key, text)| Item { key: key.clone(), full: text.clone(), pointer: Some(text.clone()) })
            .collect()
    }
}

fn all_full(claims: &[Claim]) -> Vec<usize> {
    claims.iter().map(|claim| claim.full_chars).collect()
}

fn rendered(view: &View<String>) -> String {
    view.render().map(String::as_str).collect::<Vec<_>>().join("\n")
}

// what this catches: the M5 on 10-21, where only 4-8% of each request matched the one
// before because the results ring grew at its end AHEAD of every other block. Here the
// ring grows by one result an act and the volatile sources change every act, and each
// act's view still starts with the previous act's view.
#[test]
fn every_act_of_an_epoch_only_appends() {
    let mut board = Fake::new("board", 3);
    board.push("card", "card 1b232595 claimed");
    let mut actions = Fake::new("actions", 5);
    let mut notices = Fake::new("notices", 2);
    let (mut view, _) = View::start(vec!["identity".into()], &[&board, &actions, &notices], all_full).unwrap();
    for act in 1..=6 {
        let before = rendered(&view);
        actions.push(&format!("#{act}"), &format!("code/read result {act}"));
        notices.push(&format!("n{act}"), &format!("notice {act}"));
        view.advance(&[&board, &actions, &notices]);
        assert!(rendered(&view).starts_with(&before), "act {act} rewrote the prefix");
    }
}

// what this catches: a source that grew by one item re-sending all of itself (the
// snapshot rendering that sank #5210 live: 0.37 reuse).
#[test]
fn an_act_appends_only_the_items_that_changed() {
    let mut views = Fake::new("open-views", 4);
    views.push("a.rs", &"a".repeat(5_000));
    views.push("b.rs", &"b".repeat(5_000));
    let (mut view, _) = View::start(vec![], &[&views], all_full).unwrap();
    views.push("b.rs", "b.rs lines 1-40");
    let advance = view.advance(&[&views]);
    assert_eq!(advance, Advance { items: 1, chars: "b.rs lines 1-40".len() });
    assert_eq!(view.advance(&[&views]), Advance::default(), "nothing changed, nothing appended");
}

// what this catches: a collapsed item that comes back in full on the next act (rendered
// differently, so "changed"), the collapse-then-uncollapse churn the sticky ring collapse
// was patched for; and her next act's result being born collapsed because its source was.
#[test]
fn a_collapsed_item_stays_collapsed_and_a_new_one_arrives_in_full() {
    let mut actions = Fake::new("actions", 1);
    actions.push("#1", &"x".repeat(9_000));
    let (mut view, start) = View::start(vec![], &[&actions], |claims| vec![0; claims.len()]).unwrap();
    assert_eq!(start.collapsed, 1);
    let at_start = view.chars();
    actions.push("#2", "short");
    let advance = view.advance(&[&actions]);
    assert_eq!(advance, Advance { items: 1, chars: "short".len() }, "only the new result, in full");
    assert!(view.chars() - at_start < 100, "the old result did not come back in full");
}

// what this catches: a grant spent on the oldest items (she reads stale results in full and
// her latest ones as pointers), or a grant overspent past what the allocator gave.
#[test]
fn a_grant_keeps_the_newest_items_in_full() {
    let mut actions = Fake::new("actions", 1);
    for act in 1..=3 {
        actions.push(&format!("#{act}"), &"r".repeat(1_000));
    }
    let pointer = |item: &Item<String>| item.pointer.as_ref().map_or(0, String::len);
    let floor: usize = actions.items().iter().map(pointer).sum();
    let grant = floor + 2 * (1_000 - pointer(&actions.items()[0]));
    let (view, start) = View::start(vec![], &[&actions], |_| vec![grant]).unwrap();
    assert_eq!(start.collapsed, 1);
    assert!(rendered(&view).starts_with("[#1:"), "the oldest collapsed, the newest two whole");
    assert!(view.chars() <= grant);
}

// what this catches: an allocator that answers for fewer sources than it was asked about,
// silently dropping a source from her view.
#[test]
fn a_grant_for_the_wrong_number_of_sources_is_refused() {
    let a = Fake::new("a", 1);
    let b = Fake::new("b", 1);
    let refused = View::start(vec![], &[&a, &b], |_| vec![usize::MAX]).err();
    assert_eq!(refused, Some(GrantMismatch { claims: 2, grants: 1 }));
}

// what this catches: a turnover that keeps an item the source no longer offers, or one
// that loses the allocator's choice; and a retired item that silently vanishes mid-epoch.
#[test]
fn turnover_renders_only_what_the_sources_offer_now() {
    let mut views = Fake::new("open-views", 4);
    views.push("a.rs", "a.rs contents");
    let (mut view, _) = View::start(vec!["head".into()], &[&views], all_full).unwrap();
    views.items.clear();
    views.retired.push(("a.rs".into(), "[closed a.rs]".into()));
    view.advance(&[&views]);
    assert!(rendered(&view).ends_with("[closed a.rs]"), "the close is appended, not a rewrite");
    views.retired.clear();
    views.push("b.rs", "b.rs contents");
    let start = view.turn_over(vec!["head".into()], &[&views], all_full).unwrap();
    assert_eq!(start.epoch, 2);
    assert_eq!(rendered(&view), "head\nb.rs contents");
    assert_eq!(view.superseded_chars(), 0);
}

// what this catches: a growth reserve sized from one outlier act instead of the epoch's
// typical act.
#[test]
fn appended_per_act_is_the_median_act_of_the_epoch() {
    let mut actions = Fake::new("actions", 1);
    let (mut view, _) = View::start(vec![], &[&actions], all_full).unwrap();
    assert_eq!(view.appended_per_act(), None);
    for (act, size) in [(1, 100), (2, 120), (3, 9_000)] {
        actions.push(&format!("#{act}"), &"r".repeat(size));
        view.advance(&[&actions]);
    }
    assert_eq!(view.appended_per_act(), Some(120));
}

// what this catches: a collapsed item with nothing to leave behind still costing the view (a
// short conversation turn whose one-line pointer weighs what the turn does), or rendering an
// empty message.
#[test]
fn an_item_without_a_pointer_leaves_the_view_when_collapsed() {
    struct Turns(Vec<&'static str>);
    impl Source<String> for Turns {
        fn name(&self) -> &str {
            "conversation"
        }
        fn priority(&self) -> u8 {
            1
        }
        fn items(&self) -> Vec<Item<String>> {
            self.0.iter().map(|t| Item { key: (*t).to_owned(), full: (*t).to_owned(), pointer: None }).collect()
        }
    }
    let mut turns = Turns(vec!["one", "two", "three"]);
    let (mut view, start) = View::start(vec![], &[&turns], |_| vec!["three".len()]).unwrap();
    assert_eq!((start.collapsed, rendered(&view)), (2, "three".to_owned()));
    turns.0.push("four");
    view.advance(&[&turns]);
    assert_eq!(rendered(&view), "three\nfour", "a new turn arrives whole; the collapsed stay gone");
}

// what this catches: a restart rebuilding her view from scratch. Restored, the view must render
// the same bytes (the engine's saved page for it still matches) and keep its epoch's choices:
// a collapsed item stays collapsed, and the next act only appends.
#[test]
fn a_restored_view_renders_the_same_bytes_and_keeps_appending() {
    let mut actions = Fake::new("actions", 1);
    actions.push("#1", &"x".repeat(9_000));
    actions.push("#2", "short");
    let (mut view, _) = View::start(vec!["head".into()], &[&actions], |claims| vec![claims[0].pointer_chars + 10]).unwrap();
    view.advance(&[&actions]);
    let saved = serde_json::to_string(&view).unwrap();
    let mut restored: View<String> = serde_json::from_str(&saved).unwrap();
    assert_eq!(rendered(&restored), rendered(&view));
    let before = rendered(&restored);
    actions.push("#3", "next");
    restored.advance(&[&actions]);
    assert!(rendered(&restored).starts_with(&before), "the next act only appends");
    assert!(!rendered(&restored).contains(&"x".repeat(9_000)), "a collapsed item stays collapsed");
}
