#![cfg(test)]
use super::*;

/// Her ledger lines in order, with the room each act was for: every record's line.
fn ledger(wm: &WorkingMemory) -> Vec<(Option<Uuid>, String)> {
    wm.actions.lock().records().filter_map(|record| record.head.clone().map(|head| (record.room, head))).collect()
}

// what this catches: one act kept as three copies that disagree. The results ring, the
// steps ledger and the collapsed-pointer index each evicted on their own rule, so an act
// could be a full result in one, gone from another and a pointer in the third. Here every
// act is one record: the ledger and the ring name the same #n, and a collapse turns the
// result into its pointer while its ledger line stays.
#[test]
fn an_act_is_one_record_that_collapses_but_keeps_its_ledger_line() {
    let wm = WorkingMemory::new(8);
    let cap = 2 * BODY;
    // A short first line per result: that line is the ledger's, the body the ring's.
    wm.record_receipt_typed(&[], &format!("first\n{}", "a".repeat(cap / 2)), None, saved("first-handle"));
    for act in 2..=5 {
        record_saved(&wm, &format!("act {act}\n{}", "s".repeat(cap / 2)));
    }
    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    let lines: Vec<String> = ledger(&wm).into_iter().map(|(_, head)| head).collect();
    assert!(lines.first().is_some_and(|head| head.starts_with("[action #1]")), "{lines:?}");
    let ring = wm.recent_results_messages().join("\n");
    assert!(ring.contains("[result #1; room unrecorded; operation unrecorded; output handle: tool/output first-handle — collapsed]"), "{ring}");
    assert!(!ring.contains(&"a".repeat(cap / 2)), "its whole result left the ring");
}

// what this catches: a restart changing what she reads of her own acts (Cormac on #5254: a
// collapsed pointer re-rendered as "operation unrecorded" with no workspace, because the
// checkpoint kept a pointer only as text). Every record is checkpointed whole, so her ring
// and her ledger read the same after a restore as before it.
#[test]
fn a_restart_keeps_every_act_as_she_read_it() {
    let wm = WorkingMemory::new(8);
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
    let (index, lines) = (pointers(&wm), ledger(&wm));
    assert!(index.join("\n").contains("workspace foo — collapsed]"), "precondition: a collapsed pointer with its workspace: {index:?}");
    let restored = WorkingMemory::new(8);
    restored.set_scope(Some("/work/foo".into()));
    restored.restore(wm.snapshot());
    assert_eq!(pointers(&restored), index);
    assert_eq!(ledger(&restored), lines);
}

// what this catches (card d624a9a1): an act's line cutting an argument mid-value. A cut path
// reads as a real one, and a small model copies it and loops (IntelMac 2026-10-22: a cut
// '.../citizens/pe' became three refused reads). Her recorded line shows a single-line value
// whole however long, a multi-line body by its reference, and never a cut prefix of either.
#[test]
fn an_acts_arguments_are_whole_or_plainly_elided_never_cut() {
    let path = "/home/resident/.continuum/citizens/peers/cf6b4df6-ab13-4fd8-81fb-bcb115215468/workspace/src/ipc/mod.rs";
    let body = "fn main() {}\n".repeat(200);
    let mut act = crate::cognition::act_observe::test_support::act("code/write", Some(path));
    act.call.input = serde_json::json!({"file_path": path, "content": body, "lines": [1, 40]});
    let wm = WorkingMemory::new(8);
    wm.record_receipt_typed(&[act], "wrote", None, crate::ai::types::ResultCustody::Unsaved);
    let (_, line) = ledger(&wm).pop().expect("test: the act's line");
    assert!(line.contains(path), "a path renders whole however long: {line}");
    assert!(!line.contains("fn main() {}"), "a multi-line body is referenced, not repeated: {line}");
    assert!(line.contains("[1,40]"), "{line}");
}
