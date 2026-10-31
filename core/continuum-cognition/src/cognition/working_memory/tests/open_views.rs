//! Open views: paging cursors over a large file that survive the history fit.
#![cfg(test)]

use super::*;
use crate::ai::types::{ResultCustody, ToolCall, ToolResult};
use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};

const PATH: &str = "core/source-hygiene/src/lib.rs";

/// A `code/read` of `start..=end` of a `total`-line file, as the executor renders it
/// (`spill::result_text` over the command's `ReadResult`). `stored_head` = how many
/// lines of that text she was shown when it was saved whole as `handle`.
fn read(start: u32, end: u32, total: u32, stored_head: Option<(usize, &str)>) -> Observation {
    let body = (start..=end).map(|n| format!("{n:>6} | line {n}")).collect::<Vec<_>>().join("\n");
    let value = serde_json::to_value(crate::code::types::ReadResult {
        success: true,
        content: Some(body),
        file_path: PATH.into(),
        total_lines: total,
        lines_returned: end - start + 1,
        start_line: start,
        end_line: end,
        size_bytes: 22_000,
        error: None,
    })
    .expect("test: encode");
    let full = crate::cognition::tool_executor::spill::result_text(&value);
    let (content, custody) = match stored_head {
        None => (full, ResultCustody::Whole { handle: "small".into() }),
        Some((lines, handle)) => {
            let head: Vec<&str> = full.lines().take(lines).collect();
            (
                format!("[this result is saved whole as output `{handle}`]\n{}\n[the rest is not shown]", head.join("\n")),
                ResultCustody::Pointer { handle: handle.into() },
            )
        }
    };
    Observation {
        call: ToolCall {
            id: format!("read-{start}"),
            name: "code/read".into(),
            input: serde_json::json!({ "file_path": PATH, "start_line": start, "end_line": end }),
        },
        output: ToolOutput {
            image: None,
            additional_images: None,
            result: ToolResult { tool_use_id: format!("read-{start}"), content, is_error: None, custody },
            verb: ToolVerb::classify("code/read"),
            paths: Vec::new(),
            verdict: Default::default(),
        },
        status: ActStatus::Executed,
    }
}

fn record(wm: &WorkingMemory, act: Observation) {
    wm.record_receipt_typed(&[act], "[action] code/read", None, saved("open-view-receipt"));
}

// Intent is retained as detail, but cannot truthfully be described as understanding of
// a result that had not arrived when the intent was written.
#[test]
fn a_view_keeps_pre_read_intent_without_claiming_it_is_understanding() {
    let wm = WorkingMemory::new(3);
    record(&wm, read(1, 498, 498, None));
    wm.annotate_open_views("each rule is a module with a predicate over SourceFile, registered in scan()");
    let block = wm.open_views_block().expect("an open view renders");
    assert!(block.contains("intent before this read: \"each rule is a module with a predicate over SourceFile, registered in scan()\""), "{block}");
    record(&wm, read(1, 80, 498, None));
    wm.annotate_open_views("the Violation struct is the report shape");
    let block = wm.open_views_block().expect("still open");
    assert!(block.contains("the Violation struct is the report shape") && !block.contains("registered in scan()"), "the newest read's note replaces the older: {block}");
}

// what this catches: a read's range living only in conversation history, which the
// fit drops first (Benchy, the M5, 2026-10-17: re-read lib.rs from line 1 every turn
// at 66k tokens asked against a 34,560 window). The receipt itself must open a view
// with the exact range the read showed and the file's size, and the rendered block
// must say what is still unread.
#[test]
fn a_code_read_opens_a_view_with_the_range_it_showed() {
    let wm = WorkingMemory::new(3);
    record(&wm, read(1, 250, 498, None));
    let views = wm.snapshot().open_views;
    assert_eq!(views.len(), 1, "{views:?}");
    assert_eq!(views[0].subject, crate::cognition::open_views::ViewSubject::File { path: PATH.into() });
    assert_eq!((views[0].total_lines, views[0].seen.clone(), views[0].handle.clone()), (Some(498), vec![(1, 250)], None));
    let block = wm.open_views_block().expect("an open view renders");
    assert!(block.contains(&format!("- {PATH}, 498 lines: read 1-250; not yet read 251-498")), "{block}");
}

// what this catches: a second, overlapping page of the same file opening a second
// view or listing both ranges side by side, so the cursor grows with every page
// instead of saying how far she has got. Overlapping and touching ranges merge; the
// stored read's cursor comes from the lines she was SHOWN (its head), not from the
// range she asked for, and it names the handle the rest is saved under.
#[test]
fn overlapping_reads_merge_and_a_stored_read_counts_only_its_shown_head() {
    let wm = WorkingMemory::new(3);
    record(&wm, read(1, 250, 498, None));
    record(&wm, read(200, 300, 498, None));
    assert_eq!(wm.snapshot().open_views[0].seen, vec![(1, 300)], "1-250 and 200-300 are one range");
    // Asked for 301-498, saved whole, shown the pointer, the header and 120 numbered lines.
    record(&wm, read(301, 498, 498, Some((121, "abc123"))));
    let views = wm.snapshot().open_views;
    assert_eq!(views.len(), 1, "one file is one view: {views:?}");
    assert_eq!(views[0].seen, vec![(1, 420)], "only 301-420 were shown");
    assert_eq!(views[0].handle.as_deref(), Some("abc123"));
    let block = wm.open_views_block().expect("renders");
    assert!(block.contains("read 1-420; not yet read 421-498 (that whole read is saved as output `abc123`)"), "{block}");
    record(&wm, read(421, 498, 498, None));
    let block = wm.open_views_block().expect("renders");
    assert!(block.contains(&format!("- {PATH}, 498 lines: read 1-498 (all of it)")), "{block}");
}

// what this catches: the cursor dying with the process. A restart that forgets what
// she has read sends her back to line 1 exactly as the dropped history did; the view
// must ride the checkpoint's own encoding, render byte-identically after restore
// (a stable block keeps the KV prefix), and keep the handle it names resolvable.
#[test]
fn an_open_view_survives_checkpoint_and_restore() {
    let wm = WorkingMemory::new(3);
    record(&wm, read(1, 120, 498, Some((121, "feedface00000001"))));
    let before = wm.open_views_block().expect("renders");
    let snapshot = wm.snapshot();
    assert!(
        snapshot.reachable_handles()
            .contains("feedface00000001"),
        "the checkpoint keeps the view's saved read reachable"
    );
    let written = serde_json::to_string(&snapshot).expect("test: encode");
    let read_back: VolatileSnapshot = serde_json::from_str(&written).expect("test: decode");
    let restored = WorkingMemory::new(3);
    restored.restore(read_back);
    assert_eq!(restored.open_views_block().as_deref(), Some(before.as_str()));
    assert!(before.contains(PATH) && before.contains("498 lines") && before.contains("read 1-120") && before.contains("`feedface00000001`"), "{before}");
    // A checkpoint written before this field has no views and loads.
    let legacy = written.replace(&format!(",\"open_views\":{}", serde_json::to_string(&snapshot.open_views).expect("test: encode")), "");
    assert!(!legacy.contains("open_views"), "the field was stripped");
    let legacy: VolatileSnapshot = serde_json::from_str(&legacy).expect("a pre-field checkpoint decodes");
    assert!(legacy.open_views.is_empty());
}

// what this catches (Codex on #5376): her view keeping a view her hands cannot see. When her
// hands move to another checkout, a view opened in the first is retired from her context view
// (keyed by its checkout, so the same relative path in two checkouts is two views), and it is
// offered again when her hands return.
#[test]
fn a_view_in_a_checkout_her_hands_left_is_retired_from_her_view() {
    use continuum_context::source::Source;
    use continuum_context::view::{Claim, View};

    let wm = WorkingMemory::new(3);
    wm.set_scope(Some("/checkouts/a".into()));
    record(&wm, read(1, 40, 498, None));
    let views = crate::cognition::context_streams::open_views::OpenViews { memory: &wm, persona: uuid::Uuid::nil() };
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (mut view, _) = match View::start(Vec::new(), &[&views as &dyn Source<_>], whole) {
        Ok(started) => started,
        Err(mismatch) => panic!("test: {mismatch}"),
    };
    let text = |view: &View<crate::cognition::context_streams::ContextMessage>| {
        view.render().map(|m| m.text.clone()).collect::<Vec<_>>().join("\n")
    };
    assert!(text(&view).contains(PATH), "the view she opened in checkout a");

    wm.set_scope(Some("/checkouts/b".into()));
    assert!(wm.open_view_lines().is_empty(), "not visible from checkout b");
    view.advance(&[&views as &dyn Source<_>]);
    assert!(text(&view).contains("[not visible from here] /checkouts/a/file/"), "{}", text(&view));
    assert_eq!(view.advance(&[&views as &dyn Source<_>]).chars, 0, "retired once");

    wm.set_scope(Some("/checkouts/a".into()));
    let back = view.advance(&[&views as &dyn Source<_>]);
    assert_eq!(back.items, 1, "offered again when her hands return");
}

// Capture57: eight cursor records carried 25,534 characters of copied pre-read intent
// as mandatory pointers. A larger note must increase demand, never the cursor floor.
// Exercise the actual source and epoch turnover; no per-source truncation or cap.
#[test]
fn planning_detail_yields_to_cursor_and_returns_when_granted() {
    use continuum_context::source::Source;
    use continuum_context::view::{claims, Claim, View};

    let wm = WorkingMemory::new(3);
    record(&wm, read(1, 498, 498, None));
    let source = crate::cognition::context_streams::open_views::OpenViews { memory: &wm, persona: uuid::Uuid::nil() };
    let sources: &[&dyn Source<_>] = &[&source];
    let cursor_floor = claims(sources)[0].floor_chars;
    let intent = "I need to inspect the implementation before deciding what to change. ".repeat(64);
    wm.annotate_open_views(&intent);
    let demand = claims(sources);
    assert_eq!(demand[0].floor_chars, cursor_floor, "intent is not mandatory input");
    assert!(demand[0].full_chars > cursor_floor + intent.len());
    let whole = |claims: &[Claim]| claims.iter().map(|c| c.full_chars).collect();
    let floor = |claims: &[Claim]| claims.iter().map(|c| c.floor_chars).collect();
    let (mut view, _) = View::start(Vec::new(), sources, whole).expect("matching grants");
    let text = |view: &View<crate::cognition::context_streams::ContextMessage>| {
        view.render().map(|m| m.text.clone()).collect::<Vec<_>>().join("\n")
    };
    assert!(text(&view).contains(intent.trim()));
    view.turn_over(Vec::new(), sources, floor).expect("matching grants");
    let reduced = text(&view);
    assert!(reduced.contains(PATH) && reduced.contains("read 1-498 (all of it)"), "{reduced}");
    assert!(reduced.contains("code/read") && reduced.contains("start_line/end_line"), "retrieval remains resolvable: {reduced}");
    assert!(!reduced.contains("I need to inspect"), "pre-read planning must yield: {reduced}");
    assert_eq!(claims(sources), demand, "allocation does not rewrite stored wants");

    // Checkpointing preserves the complete note even while its visible level is a cursor.
    let restored = WorkingMemory::new(3);
    restored.restore(wm.snapshot());
    let restored_source = crate::cognition::context_streams::open_views::OpenViews { memory: &restored, persona: uuid::Uuid::nil() };
    view.turn_over(Vec::new(), &[&restored_source], whole).expect("matching grants");
    assert!(text(&view).contains(intent.trim()), "the allocation never deleted the note");
}
