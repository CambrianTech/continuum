#![cfg(test)]
use super::*;

// what this catches: one act kept as three copies that disagree. The results ring, the
// steps ledger and the collapsed-pointer index each evicted on their own rule, so an act
// could be a full result in one, gone from another and a pointer in the third. Here every
// act is one record: the ledger and the ring name the same #n, and a collapse turns the
// result into its pointer while its ledger line stays.
#[test]
fn an_act_is_one_record_that_collapses_but_keeps_its_ledger_line() {
    let wm = WorkingMemory::new(8);
    wm.set_served_window(16_384);
    let cap = 2 * BODY;
    // A short first line per result: that line is the ledger's, the body the ring's.
    wm.record_receipt_typed(&[], &format!("first\n{}", "a".repeat(cap / 2)), None, saved("first-handle"));
    for act in 2..=5 {
        record_saved(&wm, &format!("act {act}\n{}", "s".repeat(cap / 2)));
    }
    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    let ledger: Vec<String> = wm.receipt_archive().into_iter().map(|(_, head)| head).collect();
    assert!(ledger.first().is_some_and(|head| head.starts_with("[action #1]")), "{ledger:?}");
    let ring = wm.recent_results_messages().join("\n");
    assert!(ring.contains("[result #1; room unrecorded; operation unrecorded; output handle: tool/output first-handle — collapsed]"), "{ring}");
    assert!(!ring.contains(&"a".repeat(cap / 2)), "its whole result left the ring");
}

// what this catches: a checkpoint from the build before the log losing what a collapsed
// result reopens. Its pointer lines and ledger lines carry no seq column; the #n the old
// owner wrote keys them, and the pointer's saved whole stays reachable for the spill guard.
#[test]
fn a_checkpoint_from_before_the_log_restores_its_pointers_and_ledger() {
    let mut legacy = WorkingMemory::new(8).snapshot();
    legacy.receipt_heads = vec!["[action #7] code/read(\"a.rs\")".into(), "[action #8] code/shell(\"ls\")".into()];
    legacy.receipt_head_rooms = vec![None, None];
    legacy.recent_results = vec![(8, None, "a.rs\nb.rs".into(), "code/shell".into(), Some("h8".into()))];
    legacy.evicted_pointers = vec![("[result #7; room unrecorded; operation code/read; output handle: tool/output h7 — collapsed]".into(), "h7".into())];
    legacy.next_action_seq = 9;
    let reachable = legacy.reachable_handles();
    assert!(reachable.contains("h7") && reachable.contains("h8"), "{reachable:?}");
    let wm = WorkingMemory::new(8);
    wm.set_served_window(16_384);
    wm.restore(legacy);
    let ledger: Vec<String> = wm.receipt_archive().into_iter().map(|(_, head)| head).collect();
    assert_eq!(ledger, ["[action #7] code/read(\"a.rs\")", "[action #8] code/shell(\"ls\")"]);
    let ring = wm.recent_results_messages().join("\n");
    // Its pointer keeps the exact text the older build wrote: operation and all.
    assert!(ring.contains("[result #7; room unrecorded; operation code/read; output handle: tool/output h7 — collapsed]"), "{ring}");
    assert!(ring.contains("a.rs\nb.rs"), "{ring}");
}

// what this catches: a restart changing what she reads of her own acts (Cormac on #5254: a
// collapsed pointer re-rendered as "operation unrecorded" with no workspace, because the
// checkpoint kept a pointer only as text). Every record is checkpointed whole, so her ring
// and her ledger read the same after a restore as before it.
#[test]
fn a_restart_keeps_every_act_as_she_read_it() {
    let wm = WorkingMemory::new(8);
    wm.set_served_window(16_384);
    wm.set_scope(Some("/work/foo".into()));
    let room = Uuid::new_v4();
    let cap = 2 * BODY;
    wm.record_receipt_typed(&[], &format!("first\n{}", "a".repeat(cap / 2)), Some(room), saved("first-handle"));
    for act in 2..=5 {
        wm.record_receipt_typed(&[], &format!("act {act}\n{}", "s".repeat(cap / 2)), Some(room), saved("ring-test"));
    }
    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    // The pointer index and the ledger. (Which act counts as in flight after a restore is
    // the restore's own rule, unchanged here, so the retained results are not compared.)
    let pointers = |memory: &WorkingMemory| -> Vec<String> {
        memory.recent_results_messages().into_iter().filter(|m| m.starts_with("Older results")).collect()
    };
    let (index, ledger) = (pointers(&wm), wm.receipt_archive());
    assert!(index.join("\n").contains("workspace foo — collapsed]"), "precondition: a collapsed pointer with its workspace: {index:?}");
    let restored = WorkingMemory::new(8);
    restored.set_served_window(16_384);
    restored.set_scope(Some("/work/foo".into()));
    restored.restore(wm.snapshot());
    assert_eq!(pointers(&restored), index);
    assert_eq!(restored.receipt_archive(), ledger);
}

// what this catches: a rollback to the build before the log restoring her blank. The
// checkpoint keeps the wire shape that build reads beside the records it now restores from.
#[test]
fn a_checkpoint_keeps_the_shape_the_previous_build_reads() {
    let wm = WorkingMemory::new(8);
    wm.set_served_window(16_384);
    let cap = 2 * BODY;
    wm.record_receipt_typed(&[], &format!("first\n{}", "a".repeat(cap / 2)), None, saved("first-handle"));
    for act in 2..=5 {
        record_saved(&wm, &format!("act {act}\n{}", "s".repeat(cap / 2)));
    }
    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    let snap = wm.snapshot();
    assert!(snap.receipt_heads.first().is_some_and(|head| head.starts_with("[action #1]")));
    assert_eq!(snap.evicted_pointers.first().map(|(_, handle)| handle.as_str()), Some("first-handle"));
    assert_eq!(snap.recent_results.last().map(|(seq, ..)| *seq), Some(5));
    assert_eq!(snap.actions.len(), 5);
}

// what this catches (card d624a9a1): an act's line cutting an argument mid-value. A cut path
// reads as a real one, and a small model copies it and loops (IntelMac 2026-10-22: a cut
// '.../citizens/pe' became three refused reads). Short values render whole, a long or
// multi-line string becomes an explicit size marker, and no rendered value is a cut prefix of
// what was sent.
#[test]
fn an_acts_arguments_are_whole_or_plainly_elided_never_cut() {
    let path = "/home/resident/.continuum/citizens/peers/cf6b4df6-ab13-4fd8-81fb-bcb115215468/workspace/src/ipc/mod.rs";
    let body = "fn main() {}\n".repeat(200);
    let args = serde_json::json!({"file_path": path, "content": body, "lines": [1, 40], "nested": {"mode": "append"}});
    let line = crate::cognition::working_memory::ledger::call_args(&args, path.chars().count());
    assert!(line.contains(path), "a path within the budget renders whole: {line}");
    assert!(line.contains(&format!("<{} chars>", body.chars().count())), "a multi-line body is a marker: {line}");
    assert!(line.contains("[1,40]") && line.contains("\"mode\":\"append\""), "{line}");
    let tight = crate::cognition::working_memory::ledger::call_args(&args, 10);
    assert!(tight.contains(&format!("<{} chars>", path.chars().count())), "a path over budget is a marker, not a prefix: {tight}");
    assert!(!tight.contains("/home/resident/"), "no prefix of the path survives: {tight}");
}
