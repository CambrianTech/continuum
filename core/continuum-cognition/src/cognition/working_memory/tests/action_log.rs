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
    assert!(ring.contains("[result #1; room unrecorded; operation unrecorded; saved: read_output({\"outputId\":\"first-handle\"}) — collapsed]"), "{ring}");
    assert!(!ring.contains(&"a".repeat(cap / 2)), "its whole result left the ring");
}

// what this catches: the collapsed line and its reader disagreeing (before 2026-11-06 the index
// emitted "… — collapsed]" while the reader accepted only "… — collapsed; query tool/output to
// expand]", so diagnostic replay refused every pointer it was meant to omit). One type renders
// and reads it: every collapsed line she is shown reads back, in a workspace or not, and a whole
// result or an edited pointer does not.
#[test]
fn every_collapsed_line_she_is_shown_reads_back_and_nothing_else_does() {
    let wm = WorkingMemory::new(8);
    let cap = 2 * BODY;
    wm.record_receipt_typed(&[], &"a".repeat(cap / 2), None, saved("first-handle"));
    for act in 2..=5 {
        record_saved(&wm, &format!("act {act}\n{}", "s".repeat(cap / 2)));
    }
    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    let messages = wm.recent_results_messages();
    let lines: Vec<&str> = messages.iter().flat_map(|m| m.lines()).collect();
    let collapsed: Vec<&str> = lines.iter().copied().filter(|l| l.ends_with(" — collapsed]")).collect();
    assert!(!collapsed.is_empty(), "the shed collapsed a result: {messages:?}");
    for line in &collapsed {
        assert!(CollapsedReceipt::read(line).is_some(), "{line}");
        assert!(CollapsedReceipt::read(&format!("{line} ")).is_none(), "an edited line is not a pointer");
    }
    for other in lines.iter().filter(|l| !l.ends_with(" — collapsed]")) {
        assert!(CollapsedReceipt::read(other).is_none(), "{other}");
    }
}

// what this catches: a restart changing what she reads of her own acts (Cormac on #5254: a
// collapsed pointer re-rendered as "operation unrecorded" with no workspace, because the
// checkpoint kept a pointer only as text). Every record is checkpointed whole, so her ring
// and her ledger read the same after a restore as before it. Her thoughts are records the same
// way: a turn of more acts than the entry ring holds keeps every one, and a restart keeps them
// under the same occurrences (an older checkpoint's ring thoughts: `provenance`).
#[test]
fn a_restart_keeps_every_act_as_she_read_it() {
    let wm = WorkingMemory::new(8);
    wm.set_scope(Some("/work/foo".into()));
    let room = Uuid::new_v4();
    let cap = 2 * BODY;
    wm.record("the parser drops the index");
    wm.record_receipt_typed(&[], &format!("first\n{}", "a".repeat(cap / 2)), Some(room), saved("first-handle"));
    for act in 2..=12 {
        wm.record_receipt_typed(&[], &format!("act {act}\n{}", "s".repeat(cap / 2)), Some(room), saved("ring-test"));
    }
    wm.record("so the fix is at checks.py:118");
    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    // The pointer index and the ledger. (Which act counts as in flight after a restore is
    // the restore's own rule, unchanged here, so the retained results are not compared.)
    let pointers = |memory: &WorkingMemory| -> Vec<String> {
        memory.recent_results_messages().into_iter().filter(|m| m.starts_with("Older results")).collect()
    };
    let (index, lines) = (pointers(&wm), ledger(&wm));
    assert!(index.join("\n").contains("workspace foo — collapsed]"), "precondition: a collapsed pointer with its workspace: {index:?}");
    let thoughts = wm.thoughts();
    assert_eq!(thoughts.len(), 2, "a turn of twelve acts on an eight-entry ring keeps both thoughts");
    let restored = WorkingMemory::new(8);
    restored.set_scope(Some("/work/foo".into()));
    restored.restore(wm.snapshot());
    assert_eq!(pointers(&restored), index);
    assert_eq!(ledger(&restored), lines);
    assert_eq!(restored.thoughts(), thoughts, "every thought, under its occurrence");
    restored.record("next thought");
    assert!(restored.thoughts().windows(2).all(|pair| pair[0].occurrence < pair[1].occurrence), "the sequence resumes past them");
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

// what this catches (Codex on #5637): a ledger line outside the memory owner's account. A
// single-line argument renders whole in her line however long, and the line outlives every
// shed, so an account of bodies alone reported a write's bytes shed while its payload stayed
// in RAM in the line. Every byte a record keeps is charged; a shed releases its body and the
// line stays as pinned charge, through a checkpoint roundtrip with its handle reachable.
#[test]
fn a_whole_single_line_argument_stays_in_the_memory_owners_account_after_a_shed() {
    let payload = "x".repeat(16 * BODY);
    let mut act = crate::cognition::act_observe::test_support::act("code/write", Some("/w/a.txt"));
    act.call.input = serde_json::json!({"file_path": "/w/a.txt", "content": payload});
    let wm = WorkingMemory::new(8);
    wm.record_receipt_typed(&[act], &"wrote\n".repeat(BODY), None, saved("write-handle"));
    record_saved(&wm, "newest");
    let before = wm.resident_results();
    let write = before.candidates.iter().find(|c| c.handle == "write-handle").expect("test: the write is sheddable");
    assert!(before.resident_bytes >= payload.len() as u64 + write.bytes, "line and body both charged: {before:?}");
    shed(&wm, 0);
    let after = wm.resident_results();
    assert!(after.candidates.iter().all(|c| c.handle != "write-handle"), "its body moved to its handle");
    assert!(after.resident_bytes >= payload.len() as u64, "the line is still charged: {after:?}");
    assert_eq!(before.resident_bytes - after.resident_bytes, write.bytes, "a shed releases exactly its body");
    let restored = WorkingMemory::new(8);
    restored.restore(wm.snapshot());
    assert_eq!(restored.resident_results().resident_bytes, after.resident_bytes, "the charge survives a restart");
    assert!(restored.actions.lock().records().any(|r| r.handle.as_deref() == Some("write-handle")), "its handle is reachable");
}

// what this catches (Cormac on #5782): a write-less count over every room she acts in and over a
// restored log from before this hold, so a reboot or another room's reading raised a spurious
// checkpoint; and a count that read verbs out of a display label. It counts this room's acts since
// the hold's start, reset by an act whose commands declare they change the world.
#[test]
fn the_write_less_stretch_counts_this_rooms_acts_since_the_hold_and_resets_on_a_declared_change() {
    use continuum_base::tool_effect::ToolEffect;
    let (room, other) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let act = |seq: u64, room: Uuid, at: u64, effect: ToolEffect| super::super::action_log::ActionRecord {
        seq,
        room: Some(room),
        scope: None,
        operation: "code/read, code/edit".into(),
        head: None,
        result: None,
        handle: None,
        summary: None,
        stored_chars: None,
        stored_content: None,
        received_ms: Some(at),
        act: Some(continuum_context::cause::Act { fingerprint: continuum_context::cause::Fingerprint::of([("x", seq.to_be_bytes().as_slice())]), effect }),
    };
    let log = super::super::action_log::ActionLog::from_records(vec![
        act(1, room, 10, ToolEffect::Observes), // before the hold: a restored act, never counted
        act(2, room, 100, ToolEffect::Observes),
        act(3, other, 110, ToolEffect::Observes), // another room's act
        act(4, room, 120, ToolEffect::Undeclared),
        act(5, room, 130, ToolEffect::Observes),
    ]);
    assert_eq!(log.acts_since_change(room, 100), 3, "this room's acts since the hold, whatever their label");
    let mut changed = log.records().cloned().collect::<Vec<_>>();
    changed.push(act(6, room, 140, ToolEffect::Changes));
    changed.push(act(7, room, 150, ToolEffect::Observes));
    assert_eq!(super::super::action_log::ActionLog::from_records(changed).acts_since_change(room, 100), 1, "a declared change resets it");
}

// what this catches: an evicted observation hashed as `Absent`, so it was no occurrence and a
// re-read of it never matched: her view measured zero re-reads while Atlas re-read an identical
// file 103 times in 300 acts (the M5, 2026-11-09), so collapse looked free and her reads
// thrashed. Evicted, a read keeps the outcome hash it had resident, across a restart too.
#[test]
fn an_evicted_read_keeps_the_outcome_hash_it_had_resident() {
    let wm = WorkingMemory::new(8);
    let cap = 2 * BODY;
    let body = format!("first\n{}", "a".repeat(cap / 2));
    wm.record_receipt_typed(&[], &body, None, saved("first-handle"));
    for act in 2..=5 {
        record_saved(&wm, &format!("act {act}\n{}", "s".repeat(cap / 2)));
    }
    // read once later acts exist: her newest act is in flight and not yet rendered
    let resident = wm.action_renderings().0.first().map(|act| act.content);
    assert!(matches!(resident, Some(continuum_context::cause::Content::Hashed(_))), "precondition: resident, hashed: {resident:?}");
    shed(&wm, cap / 2);
    assert!(wm.actions.lock().records().next().is_some_and(|first| first.result.is_none()), "precondition: the first read was evicted");
    let evicted = wm.action_renderings().0.first().map(|act| act.content);
    assert_eq!(evicted, resident, "an evicted read is the same occurrence it was resident");
    assert!(matches!(evicted, Some(continuum_context::cause::Content::Hashed(_))), "never Absent: {evicted:?}");
    // through the checkpoint's disk form, as a restart reads it
    let Ok(saved) = serde_json::to_string(&wm.snapshot()) else { panic!("test: the checkpoint serializes") };
    let Ok(snapshot) = serde_json::from_str(&saved) else { panic!("test: the checkpoint reads back") };
    let restored = WorkingMemory::new(8);
    restored.restore(snapshot);
    assert_eq!(restored.action_renderings().0.first().map(|act| act.content), resident, "and across a restart");
}
