use super::*;
use crate::cause::{Cause, Content, ElementId};
use crate::source::Item;

/// A fixture occurrence at `key`: its id from the key, its cause unrecorded (these tests are about
/// levels and starts, not chains).
fn item(key: impl Into<String>, full: String, summary: Option<String>, pointer: Option<String>) -> Item<String> {
    let key = key.into();
    let mut item = Item::whole(ElementId::of(&[key.as_bytes()]), Cause::Unrecorded, key, full);
    item.summary = summary;
    item.pointer = pointer;
    item
}

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
            .map(|(key, text)| item(key.clone(), text.clone(), None, Some(format!("[{key}: {} chars, reopen by key]", text.len()))))
            .collect()
    }
    fn retired(&self) -> Vec<Item<String>> {
        self.retired
            .iter()
            .map(|(key, text)| item(key.clone(), text.clone(), None, Some(text.clone())))
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
    assert_eq!(advance, Advance { items: 1, chars: "b.rs lines 1-40".len(), repeats: 0 });
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
    assert_eq!(advance, Advance { items: 1, chars: "short".len(), repeats: 0 }, "only the new result, in full");
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
            self.0.iter().map(|t| item(*t, (*t).to_owned(), None, None)).collect()
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
    let (mut view, _) = View::start(vec!["head".into()], &[&actions], |claims| vec![claims[0].floor_chars + 10]).unwrap();
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

// what this catches: an oversized arrival forcing a turnover. A whole-file read arrived in full
// and restarted the epoch (Aiko, the M5, 2026-10-25: 11 turnovers in 43 acts); admitted as its
// pointer, the prompt before it is unchanged.
#[test]
fn an_oversized_arrival_is_admitted_as_its_pointer_and_the_prefix_holds() -> Result<(), GrantMismatch> {
    let mut results = Fake::new("results", 1);
    results.push("r1", "small result");
    let (mut view, _) = View::start(Vec::new(), &[&results as &dyn Source<String>], |c| c.iter().map(|c| c.full_chars).collect())?;
    let before: Vec<String> = view.render().cloned().collect();
    results.push("r2", &"x".repeat(51_827));
    view.advance(&[&results]);
    let limit = before.iter().map(String::len).sum::<usize>() + 200;
    assert!(view.readmit(&[&results], limit, limit / 2).is_some(), "the unsent act is held");
    let after: Vec<String> = view.render().cloned().collect();
    assert!(after.starts_with(&before), "admission rewrote the prefix");
    assert!(view.chars() <= limit, "the view exceeds what the act granted: {}", view.chars());
    assert_eq!(after.last().map(String::as_str), Some("[r2: 51827 chars, reopen by key]"));
    Ok(())
}

/// A source whose items carry a summary level, like a file read's outline.
struct Leveled(Vec<(&'static str, String, Option<String>)>);

impl Source<String> for Leveled {
    fn name(&self) -> &str {
        "reads"
    }
    fn priority(&self) -> u8 {
        1
    }
    fn items(&self) -> Vec<Item<String>> {
        self.0
            .iter()
            .map(|(key, full, summary)| item(*key, full.clone(), summary.clone(), Some(format!("[{key}]"))))
            .collect()
    }
}

// what this catches: one item too large to keep whole collapsing every item with it (Aiko, the
// M5, 2026-10-25: 9 whole reads of one 51,827-char file in 43 acts), and the opposite error,
// older items kept more whole than a newer one, which doubled every start (median 14,238 ->
// 27,696 chars, the median act 83-111 s -> 140 s). The file keeps its outline; nothing older is
// more detailed than it.
#[test]
fn an_item_too_large_to_keep_whole_takes_its_summary_and_caps_the_older_ones() -> Result<(), GrantMismatch> {
    let reads = Leveled(vec![
        ("older", "an older result".to_owned(), Some("older".to_owned())),
        ("small", "a small result".to_owned(), Some("small".to_owned())),
        ("file", "x".repeat(51_827), Some("outline of the file".to_owned())),
    ]);
    let (view, start) = View::start(Vec::new(), &[&reads as &dyn Source<String>], |_| vec![200])?;
    let rendered: Vec<String> = view.render().cloned().collect();
    assert_eq!(rendered, ["older", "small", "outline of the file"]);
    assert_eq!((start.collapsed, start.summarized), (0, 3));
    Ok(())
}

// what this catches: admission shrinking her NEWEST arrival because older items filled the
// view (goal 7; Cormac on #5422). A small read stays whole and the caller turns over, which
// shrinks the oldest; only an arrival larger than a start would grant it is reduced.
#[test]
fn only_an_oversized_arrival_is_reduced_and_a_small_one_stays_whole() -> Result<(), GrantMismatch> {
    let mut results = Fake::new("results", 1);
    results.push("old", &"o".repeat(900));
    let (mut view, _) = View::start(Vec::new(), &[&results as &dyn Source<String>], |c| c.iter().map(|c| c.full_chars).collect())?;
    results.push("small", &"s".repeat(300));
    view.advance(&[&results]);
    let limit = 1_000;
    assert!(view.readmit(&[&results], limit, limit / 2).is_some());
    assert_eq!(view.render().last().map(String::len), Some(300), "the small newest read stays whole");
    assert!(view.chars() > limit, "still over: the caller turns over, shrinking the oldest");
    let (mut view, _) = View::start(Vec::new(), &[&results as &dyn Source<String>], |c| c.iter().map(|c| c.full_chars).collect())?;
    results.push("big", &"b".repeat(5_000));
    view.advance(&[&results]);
    assert!(view.readmit(&[&results], 2_000, 1_000).is_some());
    assert_eq!(view.render().last().map(String::as_str), Some("[big: 5000 chars, reopen by key]"));
    Ok(())
}

// what this catches: a rewind past tokens the engine already holds. Once the act's prompt is
// sent, there is no unsent act to redo.
#[test]
fn a_sent_act_cannot_be_readmitted() -> Result<(), GrantMismatch> {
    let mut results = Fake::new("results", 1);
    results.push("r1", "one");
    let (mut view, _) = View::start(Vec::new(), &[&results as &dyn Source<String>], |c| c.iter().map(|c| c.full_chars).collect())?;
    results.push("r2", "two");
    view.advance(&[&results]);
    view.sent();
    assert!(view.readmit(&[&results], 0, 0).is_none());
    Ok(())
}

// what this catches: a short production result (no summary: it is its own) falling to its pointer
// under a big read's summary ceiling and collapsing every older item with it, the original
// re-read loop (Cormac on #5426). Production-shaped: most items carry no summary.
#[test]
fn a_short_item_without_a_summary_stays_whole_under_a_summary_ceiling() -> Result<(), GrantMismatch> {
    let reads = Leveled(vec![
        ("oldest", "a short old result".to_owned(), None),
        ("big", "y".repeat(5_000), Some("outline of the big result".to_owned())),
        ("short", "a short result".to_owned(), None),
        ("file", "x".repeat(51_827), Some("outline of the file".to_owned())),
    ]);
    let (view, start) = View::start(Vec::new(), &[&reads as &dyn Source<String>], |_| vec![300])?;
    let rendered: Vec<String> = view.render().cloned().collect();
    assert_eq!(rendered, ["a short old result", "outline of the big result", "a short result", "outline of the file"]);
    assert_eq!((start.collapsed, start.summarized), (0, 2));
    Ok(())
}

// what this catches: a read receipt naming a turn she never saw. `renders` answers true only
// for an item her view shows whole (or summarized) this epoch, never for one collapsed to its
// pointer or one the view never held.
#[test]
fn renders_is_true_only_for_what_her_view_shows() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 1);
    actions.push("#1", &"x".repeat(9_000));
    actions.push("#2", "short");
    let pointer = |item: &Item<String>| item.pointer.as_ref().map_or(0, String::len);
    let floor: usize = actions.items().iter().map(pointer).sum();
    let (view, _) = View::start(vec![], &[&actions], |_| vec![floor + 10])?;
    assert!(!view.renders("actions", "#1"), "collapsed to its pointer: not read");
    assert!(view.renders("actions", "#2"), "whole: read");
    assert!(!view.renders("actions", "#3"), "never held");
    assert!(!view.renders("conversation", "#2"), "another source's key");
    Ok(())
}


// what this catches: a squeezed start keeping her collapsed for the rest of the epoch. The view
// only appends, so items a tight start collapsed stayed handles while the window had room (Aiko,
// the M5, 2026-10-29: her last fifteen results as handles in a 42k-char prompt on a 55k-token
// window). A start under grants with room restores them; under the same grants it does not, so a
// fitting view is never turned over for nothing.
#[test]
fn a_start_with_room_restores_what_a_squeezed_start_collapsed() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 1);
    actions.push("#1", &"x".repeat(9_000));
    actions.push("#2", "short");
    let pointer = |item: &Item<String>| item.pointer.as_ref().map_or(0, String::len);
    let floor: usize = actions.items().iter().map(pointer).sum();
    let squeezed = floor + 10;
    let (view, _) = View::start(vec![], &[&actions], |_| vec![squeezed])?;
    assert!(!view.renders("actions", "#1"), "the squeezed start collapsed the big item");
    assert!(view.start_would_restore(&[&actions], all_full)?, "room for it: a start gives it back");
    assert!(!view.start_would_restore(&[&actions], |_| vec![squeezed])?, "the same grants: nothing to restore");
    // what this catches: an allocator answering for the wrong number of sources read as "nothing
    // to restore" (Codex on #5564). It is the start's own typed refusal.
    assert_eq!(view.start_would_restore(&[&actions], |_| vec![]), Err(GrantMismatch { claims: 1, grants: 0 }));
    Ok(())
}

/// A source whose newest body was evicted from RAM: resident it holds only its pointer, the
/// whole is stored and loads through `Source::load`.
struct Spilled {
    body: String,
}

impl Source<String> for Spilled {
    fn name(&self) -> &str {
        "results"
    }
    fn priority(&self) -> u8 {
        5
    }
    fn items(&self) -> Vec<Item<String>> {
        let pointer = "[result #3341: saved, reopen by its handle]".to_owned();
        vec![
            item("3340", "an older result".to_owned(), None, Some("[result #3340]".to_owned())),
            {
                let mut stored = item("3341", pointer.clone(), None, Some(pointer));
                stored.stored = Some(crate::source::StoredWhole { chars: self.body.chars(), handle: "spill/3341".to_owned(), content: Content::Absent });
                stored.content = Content::Absent;
                stored
            },
        ]
    }
    fn load(&self, item: &Item<String>) -> Option<String> {
        (item.stored.as_ref()?.handle == "spill/3341").then(|| self.body.clone())
    }
}

// what this catches: a body evicted from RAM priced and rendered as its resident pointer, so
// storage decided what she saw (FOCUS-CONTRACT rule 7; on the M5 2026-10-29 working memory
// deleted result bodies by a window share while the window had room for them). Its claim
// prices the WHOLE (full_chars and the newest end), and a grant that holds it renders the
// loaded body, not the pointer.
#[test]
fn an_evicted_body_is_priced_whole_and_rendered_whole_when_granted() -> Result<(), GrantMismatch> {
    let body = "line of her newest result\n".repeat(200);
    let spilled = Spilled { body: body.clone() };
    let claims = claims(&[&spilled]);
    assert_eq!(claims[0].newest_chars, body.chars(), "the newest end is her whole result");
    assert_eq!(claims[0].full_chars, body.chars() + "an older result".to_owned().chars());
    let (view, _) = View::start(Vec::new(), &[&spilled], all_full)?;
    assert!(rendered(&view).contains(&body), "granted whole, it renders the loaded body");
    let (tight, _) = View::start(Vec::new(), &[&spilled], |claims: &[Claim]| claims.iter().map(|c| c.floor_chars).collect())?;
    assert!(!rendered(&tight).contains(&body) && rendered(&tight).contains("reopen by its handle"), "granted only its floor, it is its pointer");
    Ok(())
}

/// A source shaped like her actions after #5595 kept a record for every saved result: numbered
/// items, each with a ~190-char pointer, folding its oldest run into one line.
struct Folding {
    items: Vec<(u64, String, Option<String>)>,
}

impl Folding {
    fn new(count: u64, size: usize) -> Self {
        Self { items: (1..=count).map(|seq| (seq, "r".repeat(size), None)).collect() }
    }
    fn push(&mut self, size: usize) {
        let seq = self.items.len() as u64 + 1;
        self.items.push((seq, "r".repeat(size), None));
    }
    fn pointers(&self) -> usize {
        self.items().iter().map(|item| item.pointer.as_ref().map_or(0, String::len)).sum()
    }
}

impl Source<String> for Folding {
    fn name(&self) -> &str {
        "actions"
    }
    fn priority(&self) -> u8 {
        1
    }
    fn items(&self) -> Vec<Item<String>> {
        self.items
            .iter()
            .map(|(seq, full, summary)| item(seq.to_string(), full.clone(), summary.clone(), Some(format!("[result #{seq}; room r; operation code/read; output handle: tool/output {seq:0>16} — collapsed; {}]", "p".repeat(80)))))
            .collect()
    }
    fn fold(&self, folded: &[Item<String>]) -> Option<String> {
        let first = folded.first()?;
        let last = folded.last()?;
        Some(format!("[actions #{}-#{}: {} earlier acts folded; reopen one by #n]", first.key, last.key, folded.len()))
    }
}

fn fold_blocks(view: &View<String>) -> usize {
    view.render().filter(|text| text.starts_with("[actions #")).count()
}

// what this catches: a floor that is the sum of every pointer she ever held (Aris, the M5,
// 2026-10-29 22:00Z: 97 results, 2 whole, ~95 pointers x ~190 chars before anything whole fit).
// A grant far below that sum still shows her newest items whole, and the oldest run is ONE block,
// rendered where its oldest item stood; the claim's floor is that block, its want every whole.
#[test]
fn a_hundred_results_under_a_tight_grant_fold_into_one_block_and_the_newest_stay_whole() -> Result<(), GrantMismatch> {
    let actions = Folding::new(100, 1_000);
    let claim = &claims(&[&actions])[0];
    assert!(claim.floor_chars < 100, "the floor is one block, not {} chars of pointers", actions.pointers());
    assert_eq!(claim.full_chars, 100 * 1_000, "a folded item still wants its whole");
    let grant = claim.floor_chars + 3 * 1_000;
    assert!(grant < actions.pointers());
    let (view, start) = View::start(Vec::new(), &[&actions], |_| vec![grant])?;
    let shown: Vec<String> = view.render().cloned().collect();
    assert_eq!(fold_blocks(&view), 1);
    assert!(shown[0].starts_with("[actions #1-#97: 97 earlier acts"), "{}", shown[0]);
    assert_eq!(&shown[1..], ["r".repeat(1_000), "r".repeat(1_000), "r".repeat(1_000)]);
    assert_eq!((start.folded, start.collapsed), (97, 0));
    assert!(view.chars() <= grant);
    assert!(!view.renders("actions", "1"), "a folded item is not read");
    Ok(())
}

// what this catches: an older item kept more detailed than a newer one under any grant (goal 7:
// shrink oldest first), which with folding would leave a fold block AFTER a pointer or a whole.
// Under every grant from the floor to the whole, the folded items are the oldest run and the
// pointers the run after it. (Above the pointer an item with no summary is its own summary, so
// a short whole may sit older than a big read's outline; that rule is tested above.)
#[test]
fn no_folded_item_is_newer_than_a_pointer_summary_or_whole_item() -> Result<(), GrantMismatch> {
    let mut actions = Folding::new(0, 0);
    for size in [400, 3_000, 250, 9_000, 600, 1_200, 300, 5_000, 700, 450] {
        actions.push(size);
    }
    for (index, item) in actions.items.iter_mut().enumerate() {
        if index % 3 == 1 {
            item.2 = Some("s".repeat(item.1.len() / 5));
        }
    }
    let claim = &claims(&[&actions])[0];
    for grant in (claim.floor_chars..=claim.full_chars).step_by(97) {
        let (view, _) = View::start(Vec::new(), &[&actions], |_| vec![grant])?;
        let ranks: Vec<u8> = actions.items().iter().map(|item| rank(&view.held_level(&key("actions", &item.key))).min(rank(&Level::Summary))).collect();
        assert!(ranks.windows(2).all(|pair| pair[0] <= pair[1]), "grant {grant}: levels oldest -> newest {ranks:?}");
        assert!(fold_blocks(&view) <= 1);
    }
    Ok(())
}

// what this catches: folding changing a source that never opted in. Without a fold its floor is
// the sum of its pointers and a floor grant collapses every item to its pointer, as before.
#[test]
fn a_source_without_a_fold_floors_at_the_sum_of_its_pointers() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 1);
    for act in 1..=5 {
        actions.push(&format!("#{act}"), &"r".repeat(1_000));
    }
    let pointers: usize = actions.items().iter().map(|item| item.pointer.as_ref().map_or(0, String::len)).sum();
    assert_eq!(claims(&[&actions])[0].floor_chars, pointers);
    let (view, start) = View::start(Vec::new(), &[&actions], |_| vec![pointers])?;
    assert_eq!((start.collapsed, start.folded), (5, 0));
    assert_eq!(view.chars(), pointers);
    Ok(())
}

// what this catches: a squeezed start keeping her results folded while a later window has room
// for them whole, and the opposite churn (Cormac): a start that would raise folded items only to
// their pointers is not a restore, or every act that affords one more pointer turns the view over
// and loses the prefix for a line the fold block already covers.
#[test]
fn only_a_rise_past_the_pointer_restores_what_a_squeezed_start_folded() -> Result<(), GrantMismatch> {
    let actions = Folding::new(20, 100_000);
    let floor = claims(&[&actions])[0].floor_chars;
    let (view, start) = View::start(Vec::new(), &[&actions], |_| vec![floor])?;
    assert_eq!(start.folded, 20, "a floor grant folds the whole run");
    let pointers = actions.pointers();
    let (pointed, _) = View::start(Vec::new(), &[&actions], |_| vec![pointers])?;
    assert_eq!(fold_blocks(&pointed), 0, "precondition: the pointer grant raises every item to its pointer");
    assert!(!view.start_would_restore(&[&actions], |_| vec![pointers])?, "pointers only: no turnover");
    assert!(view.start_would_restore(&[&actions], all_full)?, "room for them whole: a start gives them back");
    Ok(())
}

// what this catches: the fold block rewriting itself act to act (it is fixed at its start, so
// the prefix holds), a folded item arriving on its own mid-epoch, and a floor that grows with her
// next result (Cormac). One more act appends one result, keeps the floor one block of the same
// size, and under the same grant is no reason to turn over.
#[test]
fn an_act_appends_one_result_and_neither_turns_over_nor_grows_the_floor() -> Result<(), GrantMismatch> {
    let mut actions = Folding::new(100, 1_000);
    let floor = claims(&[&actions])[0].floor_chars;
    let grant = floor + 3 * 1_000;
    let (mut view, _) = View::start(Vec::new(), &[&actions], |_| vec![grant])?;
    let before = rendered(&view);
    actions.push(1_000);
    assert_eq!(view.advance(&[&actions]), Advance { items: 1, chars: 1_000, repeats: 0 });
    assert!(rendered(&view).starts_with(&before), "the act rewrote the prefix");
    assert_eq!(fold_blocks(&view), 1, "the fold block is not re-rendered mid-epoch");
    assert_eq!(claims(&[&actions])[0].floor_chars, floor, "one more act, the same one-block floor");
    assert!(!view.start_would_restore(&[&actions], |_| vec![grant])?, "the same grant restores nothing");
    Ok(())
}

// what this catches: a receipt that cannot say which occurrence she was shown, or at what level,
// or in what order (the acknowledgement of a task's end reads exactly this, HER-TASKS; the
// allocator reads it to learn what a grant bought).
#[test]
fn the_receipt_names_each_shown_occurrence_and_its_level_in_prompt_order() -> Result<(), GrantMismatch> {
    let mut results = Fake::new("results", 1);
    results.push("r1", &"a".repeat(3_000));
    results.push("r2", "two");
    let (mut view, _) = View::start(Vec::new(), &[&results as &dyn Source<String>], |_| vec![100])?;
    results.push("r3", "three");
    view.advance(&[&results]);
    let receipt = view.receipt();
    let id = |k: &str| Some(ElementId::of(&[k.as_bytes()]));
    let shown: Vec<_> = receipt.items.iter().map(|r| (r.element, r.level)).collect();
    assert_eq!(shown[shown.len() - 2..], [(id("r2"), Level::Full), (id("r3"), Level::Full)], "{shown:?}");
    match shown[0] {
        (element, Level::Pointer) => assert_eq!(element, id("r1")),
        (None, Level::Folded) => {}
        other => panic!("the oldest, too large for the grant, is not whole: {other:?}"),
    }
    assert_eq!(receipt.epoch, view.epoch_id());
    assert_eq!(receipt.items.iter().map(|r| r.chars).sum::<usize>(), view.chars(), "the receipt accounts for every block after the head");
    assert_eq!(view.take_submitted(), None, "nothing was sent yet");
    view.sent();
    assert_eq!(view.take_submitted(), Some(receipt), "the sent prompt's receipt, taken once");
    assert_eq!(view.take_submitted(), None);
    Ok(())
}

/// MIND-CONTEXT invariant 4: an unchanged observation arrives as a repeat only while the whole it
/// repeats is in front of her.
mod repeats {
    use super::*;
    use crate::cause::{Act, ContentHash, Fingerprint, ToolEffect};

    /// Her acts as one chain, each caused by the one before; each act's content is its raw
    /// output's hash, never its labelled rendering (two equal outputs compare equal).
    struct Acts(Vec<(&'static str, &'static str, ToolEffect, String)>);

    impl Acts {
        fn push(&mut self, verb: &'static str, args: &'static str, effect: ToolEffect, output: &str) {
            self.0.push((verb, args, effect, output.to_owned()));
        }
    }

    impl Source<String> for Acts {
        fn name(&self) -> &str {
            "acts"
        }
        fn priority(&self) -> u8 {
            5
        }
        fn items(&self) -> Vec<Item<String>> {
            let id = |seq: usize| ElementId::of(&[b"act", &seq.to_be_bytes()]);
            self.0
                .iter()
                .enumerate()
                .map(|(i, (verb, args, effect, output))| {
                    let seq = i + 1;
                    let cause = if seq == 1 { Cause::Root(crate::cause::Origin::ActivityStart) } else { Cause::Of(id(seq - 1)) };
                    let act = Act { fingerprint: Fingerprint::of([(*verb, args.as_bytes())]), effect: *effect };
                    Item::rendered(id(seq), cause, seq.to_string(), format!("[#{seq} {verb}] {output}"), Content::Hashed(ContentHash::of(output)))
                        .with_pointer(format!("[#{seq} {verb}: collapsed; reopen with tool/output act={seq}]"))
                        .with_act(act)
                })
                .collect()
        }
        fn repeat(&self, item: &Item<String>, ancestor: &Item<String>) -> Option<String> {
            Some(format!(
                "[#{} repeat of #{}, outcome unchanged; reopen with tool/output act={}]",
                item.key(),
                ancestor.key(),
                ancestor.key()
            ))
        }
    }

    /// A file she reads whole: far larger than its collapsed pointer, as a real read is.
    fn body(name: &str) -> String {
        format!("// {name}\n") + &"fn parse(&mut self) -> Result<Tree, Error> { self.step()?; }\n".repeat(60)
    }

    fn floors(claims: &[Claim]) -> Vec<usize> {
        claims.iter().map(|claim| claim.floor_chars).collect()
    }

    // what this catches: the M5 2026-10-30 read loop, the same unchanged file arriving whole
    // again and again while the first copy is still in front of her: the second read arrives as
    // a repeat naming #1, is counted, and never counts as reading a whole.
    #[test]
    fn an_unchanged_read_repeated_mid_epoch_arrives_as_a_repeat() {
        let mut acts = Acts(Vec::new());
        let (mut view, _) = View::start(vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
        acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
        assert_eq!(view.advance(&[&acts]).repeats, 0);
        acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
        let advance = view.advance(&[&acts]);
        assert_eq!(advance.repeats, 1);
        let text = rendered(&view);
        assert_eq!(text.matches(&body("a.rs")).count(), 1, "the whole arrives once");
        assert!(text.contains("[#2 repeat of #1, outcome unchanged; reopen with tool/output act=1]"), "{text}");
        assert!(view.renders("acts", "1"));
        assert!(!view.renders("acts", "2"), "a repeat is never a read of a whole");
        // The receipt says so too: #2 was shown as a repeat of #1, never at `Full` (a reader asking
        // "was this result shown whole?" matches `Full` only).
        let id = |seq: usize| ElementId::of(&[b"act", &seq.to_be_bytes()]);
        let levels: Vec<_> = view.receipt().items.iter().filter(|r| r.element.is_some()).map(|r| (r.element, r.level)).collect();
        assert_eq!(levels, [(Some(id(1)), Level::Full), (Some(id(2)), Level::Repeat { of: id(1) })], "{levels:?}");
    }

    // what this catches: the same output from a tool that changes the world read as "nothing
    // happened" (Codex: two appends that both answer OK are two appends), and a tool that
    // declares nothing, or a reopen, ever rendered as a repeat.
    #[test]
    fn only_a_declared_observation_can_repeat() {
        for effect in [ToolEffect::Changes, ToolEffect::Reopens, ToolEffect::Undeclared] {
            let mut acts = Acts(Vec::new());
            let (mut view, _) = View::start(vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
            acts.push("tool/x", r#"{"line":"a"}"#, effect, "OK");
            view.advance(&[&acts]);
            acts.push("tool/x", r#"{"line":"a"}"#, effect, "OK");
            assert_eq!(view.advance(&[&acts]).repeats, 0, "{effect:?}");
            assert!(view.renders("acts", "1") && view.renders("acts", "2"), "{effect:?}: both occurrences whole");
        }
    }

    // what this catches: a repeat pointing at a whole that is not in front of her (collapsed by
    // the start, or gone with a turnover), whose reopen would answer another repeat and trap her
    // (Codex and Fable on the design). The re-read arrives whole; and a turnover renders an
    // earlier repeat as itself.
    #[test]
    fn a_repeat_needs_its_ancestor_whole_in_this_epoch() {
        let mut acts = Acts(Vec::new());
        acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
        acts.push("code/read", r#"{"path":"b.rs"}"#, ToolEffect::Observes, &body("b.rs"));
        let (mut view, start) = View::start(vec!["identity".into()], &[&acts], floors).unwrap(); // unwrap: one grant per claim cannot mismatch
        assert_eq!(start.collapsed + start.folded, 2, "the start holds both at their floor");
        assert!(!view.renders("acts", "1"));
        acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
        assert_eq!(view.advance(&[&acts]).repeats, 0, "#1 is collapsed: the re-read arrives whole");
        assert!(view.renders("acts", "3"));
        acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
        assert_eq!(view.advance(&[&acts]).repeats, 1, "#3 is whole now: #4 repeats it");
        assert!(rendered(&view).contains("[#4 repeat of #3,"));
        view.turn_over(vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
        let text = rendered(&view);
        assert!(!text.contains("repeat of"), "a turnover re-renders every item as itself: {text}");
        assert!(view.renders("acts", "4"));
    }

    // what this catches: an item already sent whole rewritten as a repeat on a later act, which
    // changes text she was already sent and breaks the prefix every act appends to. Two equal
    // reads arriving in one append both arrive whole (the first was not yet held), and the second
    // keeps its whole rendering for the epoch.
    #[test]
    fn a_rendering_already_sent_is_never_turned_into_a_repeat() {
        let mut acts = Acts(Vec::new());
        let (mut view, _) = View::start(vec!["identity".into()], &[&acts], all_full).unwrap(); // unwrap: one grant per claim cannot mismatch
        acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
        acts.push("code/read", r#"{"path":"a.rs"}"#, ToolEffect::Observes, &body("a.rs"));
        assert_eq!(view.advance(&[&acts]).repeats, 0);
        let before = rendered(&view);
        acts.push("code/read", r#"{"path":"b.rs"}"#, ToolEffect::Observes, &body("b.rs"));
        let advance = view.advance(&[&acts]);
        assert_eq!((advance.items, advance.repeats), (1, 0), "only the new read arrives");
        assert!(rendered(&view).starts_with(&before));
    }
}
