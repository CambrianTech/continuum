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
    let cap = wm.budget().recent_results_chars();
    // A short first line per result: that line is the ledger's, the body the ring's.
    wm.record_receipt_typed(&[], &format!("first\n{}", "a".repeat(cap / 2)), None, saved("first-handle"));
    for act in 2..=5 {
        record_saved(&wm, &format!("act {act}\n{}", "s".repeat(cap / 2)));
    }
    wm.record_settlement("done", None);
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
    assert!(ring.contains("tool/output h7 — collapsed]") && ring.contains("a.rs\nb.rs"), "{ring}");
}

// what this catches: a rollback to the build before the log restoring her blank. The
// checkpoint keeps the wire shape that build reads, now with each line's seq beside it.
#[test]
fn a_checkpoint_keeps_the_shape_the_previous_build_reads() {
    let wm = WorkingMemory::new(8);
    wm.set_served_window(16_384);
    let cap = wm.budget().recent_results_chars();
    // A short first line per result: that line is the ledger's, the body the ring's.
    wm.record_receipt_typed(&[], &format!("first\n{}", "a".repeat(cap / 2)), None, saved("first-handle"));
    for act in 2..=5 {
        record_saved(&wm, &format!("act {act}\n{}", "s".repeat(cap / 2)));
    }
    wm.record_settlement("done", None);
    let snap = wm.snapshot();
    assert_eq!(snap.receipt_heads.len(), snap.receipt_head_seqs.len());
    assert_eq!(snap.receipt_head_seqs.first(), Some(&1));
    assert_eq!(snap.evicted_pointers.first().map(|(_, handle)| handle.as_str()), Some("first-handle"));
    assert_eq!(snap.evicted_pointer_seqs.first(), Some(&1));
    assert_eq!(snap.recent_results.last().map(|(seq, ..)| *seq), Some(5));
}
