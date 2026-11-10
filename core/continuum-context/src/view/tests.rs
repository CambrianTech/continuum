use super::*;
use crate::cause::{Cause, Content, ElementId};
use crate::fixtures::{item, Fake};
use crate::source::Item;

fn all_full(claims: &[Claim]) -> Vec<usize> {
    claims.iter().map(|claim| claim.full).collect()
}

fn rendered(view: &View<String>) -> String {
    view.render().map(String::as_str).collect::<Vec<_>>().join("\n")
}

// what this catches: the M5 on 10-21, where only 4-8% of each request matched the one
// before because the results ring grew at its end AHEAD of every other block. Here the
// ring grows by one result an act and the volatile sources change every act, and each
// act's view still starts with the previous act's view.
#[test]
fn every_act_of_an_epoch_only_appends() -> Result<(), GrantMismatch> {
    let mut board = Fake::new("board", 3);
    board.push("card", "card 1b232595 claimed");
    let mut actions = Fake::new("actions", 5);
    let mut notices = Fake::new("notices", 2);
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["identity".into()], &[&board, &actions, &notices], all_full)?;
    for act in 1..=6 {
        let before = rendered(&view);
        actions.push(&format!("#{act}"), &format!("code/read result {act}"));
        notices.push(&format!("n{act}"), &format!("notice {act}"));
        view.advance(&[&board, &actions, &notices]);
        assert!(rendered(&view).starts_with(&before), "act {act} rewrote the prefix");
    }
    Ok(())
}

// what this catches: an act's allocation granting a live epoch's source less than the epoch
// already renders for it, so the epoch turns over and the engine re-prefills every token behind
// it (the M5, 2026-11-10: 65% of re-prefilled tokens, an epoch every second act). A live view's
// claim holds what its last sent prompt carried, superseded versions included (resident too); an
// epoch never sent, or a start's claim, holds nothing (a whole unsent receipt took the newest
// room update's room otherwise).
#[test]
fn a_live_claim_holds_what_its_epoch_already_renders() -> Result<(), GrantMismatch> {
    let mut views = Fake::new("open-views", 4);
    views.push("a.rs", &"a".repeat(5_000));
    let (mut view, _) = View::start(&crate::cost::Chars, vec![], &[&views], all_full)?;
    assert_eq!(view.claims(&crate::cost::Chars, &[&views])[0].held, 0, "an epoch the engine has not received holds nothing");
    views.push("a.rs", "a.rs lines 1-40");
    view.advance(&[&views]);
    view.sent();
    let rendered: usize = view.render().map(|block| block.chars()).sum();
    let live = view.claims(&crate::cost::Chars, &[&views]);
    assert_eq!(live[0].held, rendered, "the epoch's whole rendering of the source, both versions");
    assert!(live[0].held > live[0].full, "held counts the superseded version no want counts");
    assert_eq!(claims(&crate::cost::Chars, &[&views])[0].held, 0, "a start recomputes: it holds nothing");
    Ok(())
}

// what this catches: a source that grew by one item re-sending all of itself (the
// snapshot rendering that sank #5210 live: 0.37 reuse).
#[test]
fn an_act_appends_only_the_items_that_changed() -> Result<(), GrantMismatch> {
    let mut views = Fake::new("open-views", 4);
    views.push("a.rs", &"a".repeat(5_000));
    views.push("b.rs", &"b".repeat(5_000));
    let (mut view, _) = View::start(&crate::cost::Chars, vec![], &[&views], all_full)?;
    views.push("b.rs", "b.rs lines 1-40");
    let advance = view.advance(&[&views]);
    assert_eq!(advance, Advance { items: 1, chars: "b.rs lines 1-40".len(), repeats: 0 });
    assert_eq!(view.advance(&[&views]), Advance::default(), "nothing changed, nothing appended");
    Ok(())
}

// what this catches (card ef0b04e4): a whole history re-appended on one advance (2,195 items, 57.9M
// chars after a restart) with no way to say which source did it. The breakdown names each source
// that appended, with its items and characters, and nothing for a source that appended nothing.
#[test]
fn an_advance_names_each_source_that_appended() -> Result<(), GrantMismatch> {
    let mut views = Fake::new("open-views", 4);
    let mut actions = Fake::new("actions", 1);
    views.push("a.rs", "aaa");
    actions.push("#1", "first");
    let (mut view, _) = View::start(&crate::cost::Chars, vec![], &[&views, &actions], all_full)?;
    actions.push("#2", "second");
    actions.push("#3", "third");
    view.advance(&[&views, &actions]);
    assert_eq!(
        view.last_advance_by_source(),
        &[SourceAdvance { source: "actions".into(), items: 2, chars: "second".len() + "third".len() }],
        "only the source that appended, with its own counts"
    );
    view.advance(&[&views, &actions]);
    assert!(view.last_advance_by_source().is_empty(), "nothing appended, nothing named");
    Ok(())
}

// what this catches: a collapsed item that comes back in full on the next act (rendered
// differently, so "changed"), the collapse-then-uncollapse churn the sticky ring collapse
// was patched for; and her next act's result being born collapsed because its source was.
#[test]
fn a_collapsed_item_stays_collapsed_and_a_new_one_arrives_in_full() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 1);
    actions.push("#1", &"x".repeat(9_000));
    let (mut view, start) = View::start(&crate::cost::Chars, vec![], &[&actions], |claims| vec![0; claims.len()])?;
    assert_eq!(start.collapsed, 1);
    let at_start = view.chars();
    actions.push("#2", "short");
    let advance = view.advance(&[&actions]);
    assert_eq!(advance, Advance { items: 1, chars: "short".len(), repeats: 0 }, "only the new result, in full");
    assert!(view.chars() - at_start < 100, "the old result did not come back in full");
    Ok(())
}

// what this catches: a grant spent on the oldest items (she reads stale results in full and
// her latest ones as pointers), or a grant overspent past what the allocator gave.
#[test]
fn a_grant_keeps_the_newest_items_in_full() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 1);
    for act in 1..=3 {
        actions.push(&format!("#{act}"), &"r".repeat(1_000));
    }
    let pointer = |item: &Item<String>| item.pointer.as_ref().map_or(0, String::len);
    let floor: usize = actions.items().iter().map(pointer).sum();
    let grant = floor + 2 * (1_000 - pointer(&actions.items()[0]));
    let (view, start) = View::start(&crate::cost::Chars, vec![], &[&actions], |_| vec![grant])?;
    assert_eq!(start.collapsed, 1);
    assert!(rendered(&view).starts_with("[#1:"), "the oldest collapsed, the newest two whole");
    assert!(view.chars() <= grant);
    Ok(())
}

// what this catches: an allocator that answers for fewer sources than it was asked about,
// silently dropping a source from her view.
#[test]
fn a_grant_for_the_wrong_number_of_sources_is_refused() {
    let a = Fake::new("a", 1);
    let b = Fake::new("b", 1);
    let refused = View::start(&crate::cost::Chars, vec![], &[&a, &b], |_| vec![usize::MAX]).err();
    assert_eq!(refused, Some(GrantMismatch { claims: 2, grants: 1 }));
}

// what this catches: a turnover that keeps an item the source no longer offers, or one
// that loses the allocator's choice; and a retired item that silently vanishes mid-epoch.
#[test]
fn turnover_renders_only_what_the_sources_offer_now() -> Result<(), GrantMismatch> {
    let mut views = Fake::new("open-views", 4);
    views.push("a.rs", "a.rs contents");
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["head".into()], &[&views], all_full)?;
    views.items.clear();
    views.retired.push(("a.rs".into(), "[closed a.rs]".into()));
    view.advance(&[&views]);
    assert!(rendered(&view).ends_with("[closed a.rs]"), "the close is appended, not a rewrite");
    views.retired.clear();
    views.push("b.rs", "b.rs contents");
    let start = view.turn_over(&crate::cost::Chars, vec!["head".into()], &[&views], all_full)?;
    assert_eq!(start.epoch, 2);
    assert_eq!(rendered(&view), "head\nb.rs contents");
    assert_eq!(view.superseded_chars(), 0);
    Ok(())
}

// what this catches: a growth reserve sized from one outlier act instead of the epoch's
// typical act.
#[test]
fn appended_per_act_is_the_median_act_of_the_epoch() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 1);
    let (mut view, _) = View::start(&crate::cost::Chars, vec![], &[&actions], all_full)?;
    assert_eq!(view.appended_per_act(), None);
    for (act, size) in [(1, 100), (2, 120), (3, 9_000)] {
        actions.push(&format!("#{act}"), &"r".repeat(size));
        view.advance(&[&actions]);
    }
    assert_eq!(view.appended_per_act(), Some(120));
    Ok(())
}

// what this catches: a collapsed item with nothing to leave behind still costing the view (a
// short conversation turn whose one-line pointer weighs what the turn does), or rendering an
// empty message.
#[test]
fn an_item_without_a_pointer_leaves_the_view_when_collapsed() -> Result<(), GrantMismatch> {
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

        fn fold(&self, _folded: &[Item<String>]) -> Option<String> {
            None // a fixture that does not exercise folding
        }
    }
    let mut turns = Turns(vec!["one", "two", "three"]);
    let (mut view, start) = View::start(&crate::cost::Chars, vec![], &[&turns], |_| vec!["three".len()])?;
    assert_eq!((start.collapsed, rendered(&view)), (2, "three".to_owned()));
    turns.0.push("four");
    view.advance(&[&turns]);
    assert_eq!(rendered(&view), "three\nfour", "a new turn arrives whole; the collapsed stay gone");
    Ok(())
}

// what this catches: a restart rebuilding her view from scratch. Restored, the view must render
// the same bytes (the engine's saved page for it still matches) and keep its epoch's choices:
// a collapsed item stays collapsed, and the next act only appends.
#[test]
fn a_restored_view_renders_the_same_bytes_and_keeps_appending() -> Result<(), Box<dyn std::error::Error>> {
    let mut actions = Fake::new("actions", 1);
    actions.push("#1", &"x".repeat(9_000));
    actions.push("#2", "short");
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["head".into()], &[&actions], |claims| vec![claims[0].floor + 10])?;
    view.advance(&[&actions]);
    let saved = serde_json::to_string(&view)?; // boundary: disk format, the checkpoint a restored view reads back
    let mut restored: View<String> = serde_json::from_str(&saved)?; // boundary: disk format, the checkpoint read back
    assert_eq!(rendered(&restored), rendered(&view));
    let before = rendered(&restored);
    actions.push("#3", "next");
    restored.advance(&[&actions]);
    assert!(rendered(&restored).starts_with(&before), "the next act only appends");
    assert!(!rendered(&restored).contains(&"x".repeat(9_000)), "a collapsed item stays collapsed");
    Ok(())
}

// what this catches: an oversized arrival forcing a turnover. A whole-file read arrived in full
// and restarted the epoch (Aiko, the M5, 2026-10-25: 11 turnovers in 43 acts); admitted as its
// pointer, the prompt before it is unchanged.
#[test]
fn an_oversized_arrival_is_admitted_as_its_pointer_and_the_prefix_holds() -> Result<(), GrantMismatch> {
    let mut results = Fake::new("results", 1);
    results.push("r1", "small result");
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&results as &dyn Source<String>], |c| c.iter().map(|c| c.full).collect())?;
    let before: Vec<String> = view.render().cloned().collect();
    results.push("r2", &"x".repeat(51_827));
    view.advance(&[&results]);
    let limit = before.iter().map(String::len).sum::<usize>() + 200;
    assert!(view.readmit(&crate::cost::Chars, &[&results], limit, limit / 2).is_some(), "the unsent act is held");
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

    fn fold(&self, _folded: &[Item<String>]) -> Option<String> {
        None // a fixture that does not exercise folding
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
    let (view, start) = View::start(&crate::cost::Chars, Vec::new(), &[&reads as &dyn Source<String>], |_| vec![200])?;
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
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&results as &dyn Source<String>], |c| c.iter().map(|c| c.full).collect())?;
    results.push("small", &"s".repeat(300));
    view.advance(&[&results]);
    let limit = 1_000;
    assert!(view.readmit(&crate::cost::Chars, &[&results], limit, limit / 2).is_some());
    assert_eq!(view.render().last().map(String::len), Some(300), "the small newest read stays whole");
    assert!(view.chars() > limit, "still over: the caller turns over, shrinking the oldest");
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&results as &dyn Source<String>], |c| c.iter().map(|c| c.full).collect())?;
    results.push("big", &"b".repeat(5_000));
    view.advance(&[&results]);
    assert!(view.readmit(&crate::cost::Chars, &[&results], 2_000, 1_000).is_some());
    assert_eq!(view.render().last().map(String::as_str), Some("[big: 5000 chars, reopen by key]"));
    Ok(())
}

// what this catches: a rewind past tokens the engine already holds. Once the act's prompt is
// sent, there is no unsent act to redo.
#[test]
fn a_sent_act_cannot_be_readmitted() -> Result<(), GrantMismatch> {
    let mut results = Fake::new("results", 1);
    results.push("r1", "one");
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&results as &dyn Source<String>], |c| c.iter().map(|c| c.full).collect())?;
    results.push("r2", "two");
    view.advance(&[&results]);
    view.sent();
    assert!(view.readmit(&crate::cost::Chars, &[&results], 0, 0).is_none());
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
    let (view, start) = View::start(&crate::cost::Chars, Vec::new(), &[&reads as &dyn Source<String>], |_| vec![300])?;
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
    let (view, _) = View::start(&crate::cost::Chars, vec![], &[&actions], |_| vec![floor + 10])?;
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
    let (view, _) = View::start(&crate::cost::Chars, vec![], &[&actions], |_| vec![squeezed])?;
    assert!(!view.renders("actions", "#1"), "the squeezed start collapsed the big item");
    assert!(view.start_would_restore(&crate::cost::Chars, &[&actions], all_full)?, "room for it: a start gives it back");
    assert!(!view.start_would_restore(&crate::cost::Chars, &[&actions], |_| vec![squeezed])?, "the same grants: nothing to restore");
    // what this catches: an allocator answering for the wrong number of sources read as "nothing
    // to restore" (Codex on #5564). It is the start's own typed refusal.
    assert_eq!(view.start_would_restore(&crate::cost::Chars, &[&actions], |_| vec![]), Err(GrantMismatch { claims: 1, grants: 0 }));
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

    fn fold(&self, _folded: &[Item<String>]) -> Option<String> {
        None // a fixture that does not exercise folding
    }
}

// what this catches: a body evicted from RAM priced and rendered as its resident pointer, so
// storage decided what she saw (FOCUS-CONTRACT rule 7; on the M5 2026-10-29 working memory
// deleted result bodies by a window share while the window had room for them). Its claim
// prices the WHOLE (full and the newest end), and a grant that holds it renders the
// loaded body, not the pointer.
#[test]
fn an_evicted_body_is_priced_whole_and_rendered_whole_when_granted() -> Result<(), GrantMismatch> {
    let body = "line of her newest result\n".repeat(200);
    let spilled = Spilled { body: body.clone() };
    let claims = claims(&crate::cost::Chars, &[&spilled]);
    assert_eq!(claims[0].newest, body.chars(), "the newest end is her whole result");
    assert_eq!(claims[0].full, body.chars() + "an older result".to_owned().chars());
    let (view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&spilled], all_full)?;
    assert!(rendered(&view).contains(&body), "granted whole, it renders the loaded body");
    let (tight, _) = View::start(&crate::cost::Chars, Vec::new(), &[&spilled], |claims: &[Claim]| claims.iter().map(|c| c.floor).collect())?;
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
    let claim = &claims(&crate::cost::Chars, &[&actions])[0];
    assert!(claim.floor < 100, "the floor is one block, not {} chars of pointers", actions.pointers());
    assert_eq!(claim.full, 100 * 1_000, "a folded item still wants its whole");
    let grant = claim.floor + 3 * 1_000;
    assert!(grant < actions.pointers());
    let (view, start) = View::start(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![grant])?;
    let shown: Vec<String> = view.render().cloned().collect();
    assert_eq!(fold_blocks(&view), 1);
    assert!(shown[0].starts_with("[actions #1-#97: 97 earlier acts"), "{}", shown[0]);
    assert_eq!(&shown[1..], ["r".repeat(1_000), "r".repeat(1_000), "r".repeat(1_000)]);
    assert_eq!((start.folded, start.collapsed), (97, 0));
    assert!(view.chars() <= grant);
    assert!(!view.renders("actions", "1"), "a folded item is not read");
    Ok(())
}

// what this catches: a mind's resident log keeping every act she ever took because nothing told
// its owner which acts her view no longer shows (the M5, 2026-11-10: 4,522-5,686 records per
// resident, whole, in RAM and in every checkpoint). A start names the newest item it folded; the
// owner lets the run up to it leave, and `release` forgets their ids while the fold block's bytes,
// and so the prefix, stay where they were.
#[test]
fn a_start_names_its_fold_edge_and_a_released_run_leaves_the_view_unmoved() -> Result<(), GrantMismatch> {
    let mut actions = Folding::new(100, 1_000);
    let grant = claims(&crate::cost::Chars, &[&actions])[0].floor + 3 * 1_000;
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![grant])?;
    assert_eq!(view.fold_edge("actions"), Some("97"), "the newest folded act is the start's measured edge");
    assert_eq!(view.fold_edge("thoughts"), None, "a source that folded nothing names no edge");
    let before: Vec<String> = view.render().cloned().collect();
    actions.items.retain(|(seq, _, _)| *seq > 97);
    view.release("actions", |key| key.parse::<u64>().is_ok_and(|seq| seq <= 97));
    assert!(view.folded.is_empty(), "no id is held for an act that left");
    assert!(view.log.iter().all(|entry| entry.covers.is_empty()), "the log's fold block covers nothing it must keep");
    assert_eq!(view.advance(&[&actions]), Advance::default(), "the released run arrives as nothing");
    assert_eq!(view.render().cloned().collect::<Vec<_>>(), before, "her view's bytes did not move");
    view.turn_over(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![grant])?;
    assert!(view.render().any(|block| block.starts_with("[actions #1-#97")), "the logged fold block still names the range");
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
    let claim = &claims(&crate::cost::Chars, &[&actions])[0];
    for grant in (claim.floor..=claim.full).step_by(97) {
        let (view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![grant])?;
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
    assert_eq!(claims(&crate::cost::Chars, &[&actions])[0].floor, pointers);
    let (view, start) = View::start(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![pointers])?;
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
    let floor = claims(&crate::cost::Chars, &[&actions])[0].floor;
    let (mut view, start) = View::start(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![floor])?;
    assert_eq!(start.folded, 20, "a floor grant folds the whole run");
    let pointers = actions.pointers();
    let (pointed, _) = View::start(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![pointers])?;
    assert_eq!(fold_blocks(&pointed), 0, "precondition: the pointer grant raises every item to its pointer");
    assert!(!view.start_would_restore(&crate::cost::Chars, &[&actions], |_| vec![pointers])?, "pointers only: no turnover");
    assert!(view.start_would_restore(&crate::cost::Chars, &[&actions], all_full)?, "room for them whole: a start gives them back");
    let raised = view.turn_over(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![pointers])?;
    assert_eq!((raised.restored_items, raised.restored_chars), (0, 0), "pointers are not restored content");
    Ok(())
}

// what this catches: the fold block rewriting itself act to act (it is fixed at its start, so
// the prefix holds), a folded item arriving on its own mid-epoch, and a floor that grows with her
// next result (Cormac). One more act appends one result, keeps the floor one block of the same
// size, and under the same grant is no reason to turn over.
#[test]
fn an_act_appends_one_result_and_neither_turns_over_nor_grows_the_floor() -> Result<(), GrantMismatch> {
    let mut actions = Folding::new(100, 1_000);
    let floor = claims(&crate::cost::Chars, &[&actions])[0].floor;
    let grant = floor + 3 * 1_000;
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&actions], |_| vec![grant])?;
    let before = rendered(&view);
    actions.push(1_000);
    assert_eq!(view.advance(&[&actions]), Advance { items: 1, chars: 1_000, repeats: 0 });
    assert!(rendered(&view).starts_with(&before), "the act rewrote the prefix");
    assert_eq!(fold_blocks(&view), 1, "the fold block is not re-rendered mid-epoch");
    assert_eq!(claims(&crate::cost::Chars, &[&actions])[0].floor, floor, "one more act, the same one-block floor");
    assert!(!view.start_would_restore(&crate::cost::Chars, &[&actions], |_| vec![grant])?, "the same grant restores nothing");
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
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&results as &dyn Source<String>], |_| vec![100])?;
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

mod repeats;

// Atlas 2026-11-06: a large wall append preceded a fitting-view restore. This
// exercises the real View owner, not the cognition turnover predicate alone.
#[test]
fn wall_append_preserves_prefix_until_restore_recovers_collapsed_content() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 1);
    actions.push("old-result", &"x".repeat(9_000));
    let mut wall = Fake::new("room-wall", 2);
    wall.push("wall", "original wall");
    let sources: [&dyn Source<String>; 2] = [&actions, &wall];
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["identity".into()], &sources, |claims| {
        claims.iter().map(|claim| if claim.source == "actions" { 100 } else { claim.full }).collect()
    })?;
    assert!(!view.renders("actions", "old-result"));
    let before = rendered(&view);
    wall.push("wall", &"w".repeat(35_500));
    let sources: [&dyn Source<String>; 2] = [&actions, &wall];
    view.advance(&sources);
    let appended = rendered(&view);
    assert!(appended.starts_with(&before), "the wall advance must preserve the held prefix");
    assert!(view.start_would_restore(&crate::cost::Chars, &sources, all_full)?);
    assert_eq!(rendered(&view), appended, "the restore probe cannot mutate the view");
    let restored = view.turn_over(&crate::cost::Chars, vec!["identity".into()], &sources, all_full)?;
    assert_eq!(restored.restored_items, 1, "the changed wall is not restored content");
    assert_eq!(restored.restored_chars, 9_000);
    assert!(view.renders("actions", "old-result"), "restore must actually recover the old result");
    assert!(view.renders("room-wall", "wall"));
    assert!(!rendered(&view).starts_with(&appended), "restoring content changes the prior prefix");
    assert!(!view.start_would_restore(&crate::cost::Chars, &sources, all_full)?, "unchanged full grants must not restore twice");
    let repeated = view.turn_over(&crate::cost::Chars, vec!["identity".into()], &sources, all_full)?;
    assert_eq!(repeated.restored_items, 0);
    assert_eq!(repeated.restored_chars, 0);
    Ok(())
}

#[test]
fn turnover_receipt_counts_promotions_but_not_new_items() -> Result<(), GrantMismatch> {
    let mut reads = Leveled(vec![("old", "x".repeat(1000), Some("outline".into()))]);
    let (mut view, start) = View::start(&crate::cost::Chars, Vec::new(), &[&reads], |_| vec![0])?;
    assert_eq!((start.restored_items, start.restored_chars), (0, 0));
    // Logged as its pointer at the squeezed start, it rises with room for its summary beside the
    // pointer her log keeps (5 + 7 chars).
    assert!(view.turn_over(&crate::cost::Chars, Vec::new(), &[&reads], |_| vec![7])?.restored_items == 0, "no room beside its log entry");
    let summary = view.turn_over(&crate::cost::Chars, Vec::new(), &[&reads], |_| vec![12])?;
    assert_eq!((summary.restored_items, summary.restored_chars), (1, 7));
    reads.0.push(("new", "newly arrived".into(), None));
    let before = rendered(&view);
    assert!(view.turn_over(&crate::cost::Chars, Vec::new(), &[&reads], |_| vec![]).is_err());
    assert_eq!(rendered(&view), before, "a refused turnover retains the old owner");
    let full = view.turn_over(&crate::cost::Chars, Vec::new(), &[&reads], all_full)?;
    assert_eq!((full.restored_items, full.restored_chars), (1, 1000));
    Ok(())
}

// what this catches: the M5's tail re-prefill (2026-11-06, 2,822 s over 78 acts). Her act's tail
// (the clock, her call, its result) was sent once as native protocol and re-rendered the next act
// as a `[result #n]` item of her actions, so the previous prompt was never the next one's prefix.
// Appended as sent, the tail is the next prompt's prefix; the act it showed never arrives a second
// time from its source; a start re-renders it from the source and drops the tail.
#[test]
fn a_sent_tail_is_the_next_acts_prefix_and_what_it_showed_never_arrives_twice() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 5);
    actions.push("#1", "result 1");
    let (mut view, _) = View::start(&crate::cost::Chars, vec!["identity".into()], &[&actions], all_full)?;
    view.advance(&[&actions]);
    let sent = [rendered(&view), "[now 12:00]".into(), "call #2".into(), "result 2 as sent".into()].join("\n");
    view.append_sent("tail", vec!["[now 12:00]".into(), "call #2".into(), "result 2 as sent".into()], &[("actions", "#2")]);
    assert_eq!(rendered(&view), sent, "the view now ends with the tail exactly as it was sent");
    assert!(view.covers("actions", "#2") && view.renders("actions", "#2"), "she read #2 whole, in the tail");
    assert!(view.holds_sent("tail", &"[now 12:00]".to_string()), "a piece already sent is known");
    actions.push("#2", "result 2 as the actions source renders it");
    actions.push("#3", "result 3");
    view.advance(&[&actions]);
    let next = rendered(&view);
    assert!(next.starts_with(&sent), "the next act's prompt extends the sent one");
    assert!(!next.contains("as the actions source renders it"), "#2 arrived twice:\n{next}");
    assert!(next.ends_with("result 3"), "{next}");
    view.turn_over(&crate::cost::Chars, vec!["identity".into()], &[&actions], all_full)?;
    let started = rendered(&view);
    assert!(started.contains("as the actions source renders it") && !started.contains("as sent"), "a start renders from the source:\n{started}");
    assert!(!view.covers("actions", "#2"), "a start covers nothing");
    Ok(())
}

// what this catches: a state kept as if it still held. A perception fact ("my last two acts
// repeated") that stopped holding stayed in her view as its last word, and when it fired again
// with the same bytes it read as unchanged at the place it first fired, never at the newest end.
#[test]
fn a_state_the_source_no_longer_offers_is_ended_once_and_returns_at_the_newest_end() -> Result<(), GrantMismatch> {
    let mut facts = Fake::states("facts", 5);
    let mut actions = Fake::new("actions", 5);
    facts.push("repetition", "[repetition] the same call twice");
    let (mut view, _) = View::start(&crate::cost::Chars, vec![], &[&facts, &actions], all_full)?;
    facts.drop_item("repetition");
    actions.push("#1", "result 1");
    view.advance(&[&facts, &actions]);
    view.advance(&[&facts, &actions]);
    assert_eq!(rendered(&view).matches("[repetition ended]").count(), 1, "ended once:\n{}", rendered(&view));
    facts.push("repetition", "[repetition] the same call twice");
    view.advance(&[&facts, &actions]);
    assert!(rendered(&view).ends_with("[repetition] the same call twice"), "it returns at the newest end:\n{}", rendered(&view));
    actions.drop_item("#1");
    view.advance(&[&facts, &actions]);
    assert!(!rendered(&view).contains("[#1 ended]"), "an event source keeps what happened");
    Ok(())
}

// what this catches: an epoch start that shares only the system head with the prompt before it
// (the M5, 2026-11-06: 13 turnovers, every one resumed at 11,198 / 9,078 tokens) because its
// order moved (two blocks traded places) or a count in a block's first line changed. A start
// lays down the previous epoch's blocks first, in that epoch's order, for as long as each is
// unchanged; a changed block ends the reuse; a source's items never pass one another.
#[test]
fn a_start_leads_with_every_unchanged_block_the_epoch_before_it_led_with() -> Result<(), GrantMismatch> {
    let mut work = Fake::new("my-work", 6);
    let mut kanban = Fake::new("kanban", 6);
    let mut log = Fake::new("log", 5);
    work.push("card", "card 1b23 in progress");
    kanban.push("body", &"card line\n".repeat(50));
    log.push("#1", "result 1");
    log.push("#2", "result 2");
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&work, &kanban, &log], all_full)?;
    let before = rendered(&view);
    // The caller now offers the sources in another order (a measured re-rank, a new block):
    // every block is unchanged, so the start is the epoch it replaces, byte for byte.
    view.turn_over(&crate::cost::Chars, Vec::new(), &[&kanban, &log, &work], all_full)?;
    assert_eq!(rendered(&view), before, "an unchanged view starts as the prompt before it");
    // A changed block ends the reuse where it stood; everything before it is kept in place.
    log.push("#2", "result 2, rewritten");
    log.push("#3", "result 3");
    view.turn_over(&crate::cost::Chars, Vec::new(), &[&log, &kanban, &work], all_full)?;
    let after = rendered(&view);
    let kept = ["card 1b23 in progress", &"card line\n".repeat(50), "result 1", "result 2, rewritten"].join("\n");
    assert!(after.starts_with(&kept), "the unchanged lead is reused in its old order, the changed block where it stood:\n{after}");
    assert!(after.find("result 2, rewritten") < after.find("result 3"), "a source's items keep their order:\n{after}");
    Ok(())
}

// what this catches: the start's lay-down rule itself, whatever the measured order. The previous
// epoch's unchanged blocks lead in its order even when the start ranks their sources the other
// way; the first changed block ends the reuse; and an item never passes an earlier item of its
// own source to be reused (her conversation and her acts stay in their order).
#[test]
fn lead_with_what_held_reuses_the_previous_order_until_the_first_change() {
    let block = |source: usize, id: &str, text: &str| Planned { source, id: SourceId(id.into()), element: None, level: Level::Full, text: text.to_owned() };
    let ids = |laid: &[Planned<String>]| laid.iter().map(|b| b.id.0.clone()).collect::<Vec<_>>();
    let mut previous = Epoch::new(1, Vec::new());
    for (id, text) in [("wall/b", "W"), ("kanban/b", "K"), ("log/1", "one")] {
        previous.observe(SourceId(id.into()), None, Level::Full, text.to_owned());
    }
    let ranked_otherwise = vec![block(0, "log/1", "one"), block(1, "kanban/b", "K"), block(2, "wall/b", "W")];
    assert_eq!(ids(&lead_with_what_held(ranked_otherwise, Some(&previous))), ["wall/b", "kanban/b", "log/1"], "the old order leads");
    let changed = vec![block(0, "log/1", "one"), block(1, "kanban/b", "K2"), block(2, "wall/b", "W")];
    assert_eq!(ids(&lead_with_what_held(changed, Some(&previous))), ["wall/b", "log/1", "kanban/b"], "reuse ends at the change; a changed block the order moved back stays back");
    let next_anyway = vec![block(0, "wall/b", "W"), block(1, "kanban/b", "K2"), block(2, "log/1", "one")];
    assert_eq!(ids(&lead_with_what_held(next_anyway, Some(&previous))), ["wall/b", "kanban/b", "log/1"], "laid where it stood when the order puts it next");
    let earlier_first = vec![block(0, "log/0", "zero"), block(0, "log/1", "one"), block(1, "wall/b", "W"), block(2, "kanban/b", "K")];
    assert_eq!(ids(&lead_with_what_held(earlier_first, Some(&previous))), ["wall/b", "kanban/b", "log/0", "log/1"], "log/1 never passes log/0");
}

/// Characters two renders share from the start.
fn shared(before: &str, after: &str) -> usize {
    before.chars().zip(after.chars()).take_while(|(a, b)| a == b).count()
}

// what this catches: a start re-deciding her collapsed history, so a handle or a fold block
// re-renders differently and the turnover misses everything after it (the M5, 2026-11-06: all 13
// turnovers resumed from the system head). Collapsed once, an item's handle is laid down byte for
// byte at every later start; the next start appends what collapses then, after it.
#[test]
fn a_collapsed_item_is_laid_down_again_byte_for_byte_at_every_later_start() -> Result<(), GrantMismatch> {
    let mut actions = Fake::new("actions", 5);
    for n in 1..=6 {
        actions.push(&format!("#{n}"), &format!("result {n} {}", "x".repeat(400)));
    }
    // A squeezed start: the oldest results collapse to their handles.
    let squeeze = |claims: &[Claim]| claims.iter().map(|claim| claim.full / 2).collect::<Vec<_>>();
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&actions], squeeze)?;
    let handles: Vec<String> = view.render().filter(|block| block.starts_with("[#")).cloned().collect();
    assert!(!handles.is_empty(), "precondition: the squeezed start collapsed her oldest results");
    for n in 7..=12 {
        actions.push(&format!("#{n}"), &format!("result {n} {}", "x".repeat(400)));
        view.advance(&[&actions]);
    }
    let start = view.turn_over(&crate::cost::Chars, Vec::new(), &[&actions], squeeze)?;
    let now: Vec<String> = view.render().cloned().collect();
    assert_eq!(now[..handles.len()], handles[..], "the old log leads the new start as it was");
    assert!(now.len() > handles.len() && now[handles.len()].starts_with("[#"), "what collapsed now appends after it: {now:?}");
    assert_eq!(start.log_kept_chars, handles.iter().map(String::len).sum::<usize>());
    assert!(!start.log_folded);
    Ok(())
}

// what this catches (the adversarial check): an order that never learns. A block that held
// stable for epochs then changes at every act must drift behind a block that still holds, within
// a bounded number of starts, and the prefix each start shares with the epoch before must recover
// past the stable block; an order that keeps the volatile block in front loses it at every start.
#[test]
fn a_source_that_turns_volatile_drifts_tailward_and_the_shared_prefix_recovers() -> Result<(), GrantMismatch> {
    let mut wall = Fake::new("wall", 6);
    let mut map = Fake::new("map", 6);
    wall.push("w", &"wall ".repeat(200));
    map.push("m", &"map ".repeat(400));
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&wall, &map], all_full)?;
    // Stable epochs: both hold; the caller's order (wall first) stands.
    for _ in 0..3 {
        for _ in 0..4 {
            view.advance(&[&wall, &map]);
        }
        view.turn_over(&crate::cost::Chars, Vec::new(), &[&wall, &map], all_full)?;
    }
    assert!(rendered(&view).starts_with("wall"), "precondition: stable, it keeps its place");
    // The wall turns volatile: it changes at every act from now on.
    let mut recovered = None;
    for epoch in 1..=4 {
        for act in 0..4 {
            wall.push("w", &format!("wall v{epoch}.{act} {}", "wall ".repeat(200)));
            view.advance(&[&wall, &map]);
        }
        let before = rendered(&view);
        view.turn_over(&crate::cost::Chars, Vec::new(), &[&wall, &map], all_full)?;
        if shared(&before, &rendered(&view)) >= map.items[0].1.len() {
            recovered.get_or_insert(epoch);
        }
    }
    let recovered = recovered.expect("the shared prefix never recovered past the stable block"); // JUSTIFIED: test assertion
    assert!(recovered <= 2, "it recovered only at start {recovered}");
    assert!(rendered(&view).starts_with("map"), "the stable block leads");
    Ok(())
}

/// A source absent from a start takes its logged history with it.
mod absent_source;
/// The collapse depth from her measured re-reads, end to end.
mod depth;
/// A source a start shed waits for a start that funds it.
mod shed;

// what this catches: her sources splitting the window by flat constants, so her acts (whose reads
// she keeps asking for again) got the same share as a 300k-token conversation she does not (the
// M5, 2026-11-09: 70-90% of her file reads re-read one 3-5 acts old). A source's weight is its
// priority scaled by her measured re-reads of what it would collapse; unmeasured, its priority.
#[test]
fn a_source_she_re_reads_from_weighs_more_and_an_unmeasured_one_keeps_its_priority() {
    use crate::depth::Collapsible;
    use crate::rereads::{Hazard, Tally};
    let claim = |hazard: Hazard| Claim {
        source: "actions".into(),
        priority: 5,
        floor: 100,
        full: 10_000,
        newest: 500,
        logged: 0,
        held: 0,
        instrumented: true,
        collapsible: (0..4).map(|age| Collapsible { whole: 2_000, saves: 1_900, age, hazard }).collect(),
    };
    assert_eq!(claim(Hazard::Unmeasured).weight(), 5, "unmeasured: its priority, never a guessed rate");
    // each of four items asked for again (1 + 1) / (3 + 1) = 0.5 of acts: two re-reads per act
    let measured = claim(Hazard::Measured { tally: Tally { exposure: 3.0, events: 1.0 }, prior: 1.0 });
    assert_eq!(measured.weight(), 15, "priority x (1 + 2 re-reads per act)");
}
