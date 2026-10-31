#![cfg(test)]
use super::*;

// what this catches: a receipt made while rooted at one card's checkout
// leaking into perception while she stands at another (the 9/5 "my context
// is confused" opener), and coming back when she returns.
#[test]
fn a_step_is_scoped_to_its_root_and_every_result_is_perceived_labelled_with_its_workspace() {
    let wm = WorkingMemory::new(8);
    wm.set_scope(Some("/peers/x/workspace/swe/django-16899".into()));
    wm.record_receipt("[action #1] ls tests/ordering");
    wm.set_scope(Some("/peers/x/workspace/swe/django-13033".into()));
    wm.record_receipt("[action #2] cat django/db/models/sql/compiler.py");
    let seen: Vec<String> = wm.recent_entries().into_iter().map(|e| e.text).collect();
    assert!(seen.iter().any(|t| t.contains("compiler.py")), "{seen:?}");
    assert!(
        !seen.iter().any(|t| t.contains("tests/ordering")),
        "{seen:?}"
    );
    wm.set_scope(Some("/peers/x/workspace/swe/django-16899".into()));
    let back: Vec<String> = wm.recent_entries().into_iter().map(|e| e.text).collect();
    assert!(
        back.iter().any(|t| t.contains("tests/ordering")),
        "{back:?}"
    );
    // Result custody must obey the same scope even after entry-trail eviction,
    // a shed's collapse and checkpoint restoration. Storage remains intact.
    let results = WorkingMemory::new(2);
    results.set_scope(Some("task-a".into()));
    let body_a = format!("task-a-only {}", "a".repeat(BODY));
    results.record_receipt_typed(&[], &body_a, None, saved("scope-a-output"));
    let original = results.last_action_full();
    results.set_scope(Some("task-b".into()));
    assert!(results.active_action_full().is_none(), "wrong-root full result must not become legacy prose");
    assert_eq!(results.last_action_full(), original, "projection does not discard custody");
    results.record_receipt_typed(&[], "task-b-only", None, saved("scope-b-output"));
    for _ in 0..4 { results.record_fact("unrelated notice"); }
    let from_b = results.recent_results_messages().join("\n");
    assert!(from_b.contains("scope-a-output; workspace task-a"), "one mind: task-a's result, labelled, from task-b: {from_b}");
    assert!(!from_b.contains("task-b-only"), "the in-flight action renders at the tail, not in the ring");
    results.set_scope(Some("task-a".into()));
    assert_eq!(results.recent_results_messages().join("\n"), from_b, "the ring is the same from either root");
    assert!(results.active_action_full().is_none(), "B stays stored but inactive in A");
    results.set_scope(Some("task-b".into()));
    results.record_settlement("reported", None);
    shed(&results, 0);
    let snapshot = results.snapshot();
    assert!(snapshot.action_records().iter().any(|r| r.result.is_none() && r.handle.is_some()), "fixture exercises a collapsed result");
    let encoded = serde_json::to_value(&snapshot).expect("snapshot serializes");
    let restored = WorkingMemory::new(2);
    restored.restore(serde_json::from_value(encoded.clone()).expect("snapshot round trip"));
    let shown = restored.recent_results_messages().join("\n");
    restored.set_scope(Some("task-b".into()));
    assert!(shown.contains("scope-a-output") && restored.recent_results_messages().join("\n") == shown, "{shown}");
    assert_eq!(restored.snapshot().action_records(), snapshot.action_records(), "collapsed results retain custody");
    // Receipt publication, pruning and snapshots share custody: a concurrent
    // checkpoint must never lose a result's root.
    let concurrent = WorkingMemory::new(2);
    concurrent.set_scope(Some("writer-root".into()));
    std::thread::scope(|threads| {
        threads.spawn(|| {
            for _ in 0..128 { concurrent.record_receipt("saved observation"); }
        });
        for _ in 0..128 {
            concurrent.publish_live_handles();
            let snapshot = concurrent.snapshot();
            for record in snapshot.action_records().iter().filter(|r| r.result.is_some()) {
                assert_eq!(record.scope.as_deref(), Some("writer-root"));
            }
        }
    });
}

// what this catches: eviction deleting a saved result a mind still points at (card
// 9197cd01). A handle in the ring, and the same handle after it collapses to a pointer,
// is live to the spill pool's guard; when the mind drops, it is released.
#[test]
fn a_mind_s_saved_handles_are_live_until_it_drops() {
    let handle = format!("{:016x}", Uuid::new_v4().as_u128() as u64);
    let file = std::path::PathBuf::from(format!("/spill/p/{handle}.log"));
    let live = |p: &std::path::Path| crate::cognition::tool_executor::spill::is_live(p);
    let wm = WorkingMemory::new(8);
    let cap = 2 * BODY;
    wm.record_receipt_typed(&[], &"a".repeat(cap / 2), None, saved(&handle));
    assert!(live(&file), "referenced by the ring");
    for _ in 0..4 {
        record_saved(&wm, &"s".repeat(cap / 2));
    }
    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    assert!(
        wm.recent_results_messages().join("\n").contains(&format!("{handle} — collapsed")),
        "it collapsed to a pointer"
    );
    assert!(live(&file), "still referenced by its pointer");
    drop(wm);
    assert!(!live(&file), "released when the mind drops");
}

// what this catches: a body moved to disk losing its last copy. Liveness (and the log itself)
// once covered only the newest 12 collapsed pointers (`POINTER_INDEX`), so a shed that moved
// many bodies left the older handles evictable under disk pressure while her view still priced
// their wholes. Every record that names a handle keeps it live, however many pointers are newer.
#[test]
fn a_moved_body_s_handle_stays_live_behind_any_number_of_newer_pointers() {
    let handle = || format!("{:016x}", Uuid::new_v4().as_u128() as u64);
    let live = |h: &str| crate::cognition::tool_executor::spill::is_live(&std::path::PathBuf::from(format!("/spill/p/{h}.log")));
    let wm = WorkingMemory::new(8);
    let first = handle();
    wm.record_receipt_typed(&[], &"f".repeat(BODY), None, saved(&first));
    for _ in 0..30 {
        record_receipt_with(&wm, &handle());
    }
    shed(&wm, 0);
    for _ in 0..30 {
        record_receipt_with(&wm, &handle());
    }
    shed(&wm, 0);
    assert!(live(&first), "the oldest moved body's handle is still protected");
    let (acts, _) = wm.action_renderings();
    let oldest = acts.first().expect("test: the first act");
    assert_eq!(oldest.stored.as_ref().map(|(_, h)| h.as_str()), Some(first.as_str()), "its record stays, priced and reloadable");
    drop(wm);
    assert!(!live(&first), "released when the mind drops");
}

fn record_receipt_with(wm: &WorkingMemory, handle: &str) {
    wm.record_receipt_typed(&[], &format!("{handle} {}", "s".repeat(BODY)), None, saved(handle));
}

// what this catches: a shed moving a record it never checked. The memory owner vouches for each
// saved output outside her lock, then evicts by `(seq, handle)`; a record that no longer names
// the vouched handle stays resident.
#[test]
fn a_shed_moves_only_the_records_it_vouched_for() {
    let wm = WorkingMemory::new(8);
    wm.record_receipt_typed(&[], &"a".repeat(BODY), None, saved("vouched-a"));
    wm.record_receipt_typed(&[], &"b".repeat(BODY), None, saved("vouched-b"));
    wm.record_receipt_typed(&[], "newest", None, saved("vouched-c"));
    let seq_of = |h: &str| wm.snapshot().actions.iter().find(|r| r.handle.as_deref() == Some(h)).map(|r| r.seq).expect("test: seq");
    let (a, b) = (seq_of("vouched-a"), seq_of("vouched-b"));
    wm.evict_resident_results_over(0, &|seq, handle| (seq == a && handle == "a-changed") || (seq == b && handle == "vouched-b"));
    let resident: Vec<_> = wm.snapshot().actions.into_iter().filter(|r| r.result.is_some()).filter_map(|r| r.handle).collect();
    assert_eq!(resident, ["vouched-a", "vouched-c"], "a changed since it was vouched for; only b moved");
}

// what this catches (Codex on #4987): per-call results reachable only while the ENTRY that
// carried their acts survives. Entries roll over at capacity while the receipt that points
// at those results stays in the ring, then collapses to a pointer, then is checkpointed and
// restored; at every step each child stays reachable, because it belongs to the receipt.
#[test]
fn a_receipt_s_per_call_results_stay_reachable_as_long_as_the_receipt() {
    use crate::ai::types::{ResultCustody, ToolCall, ToolResult};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};
    let id = || format!("{:016x}", Uuid::new_v4().as_u128() as u64);
    let act = |handle: &str| Observation {
        call: ToolCall { id: handle.into(), name: "code/read".into(), input: serde_json::json!({}) },
        output: ToolOutput {
            image: None,
            additional_images: None,
            result: ToolResult {
                tool_use_id: handle.into(),
                content: format!("[saved as output `{handle}`]"),
                is_error: None,
                custody: ResultCustody::Pointer { handle: handle.to_owned() },
            },
            verb: ToolVerb::classify("code/read"),
            paths: Vec::new(),
            verdict: Default::default(),
        },
        status: ActStatus::Executed,
    };
    let live = |h: &str| {
        crate::cognition::tool_executor::spill::is_live(&std::path::PathBuf::from(format!("/spill/p/{h}.log")))
    };
    let (receipt, children) = (id(), [id(), id()]);
    let wm = WorkingMemory::new(2); // two entries: the third receipt rolls the first out
    let cap = 2 * BODY;
    wm.record_receipt_typed(&[act(&children[0]), act(&children[1])], &"r".repeat(cap / 2), None, saved(&receipt));
    for _ in 0..3 {
        record_saved(&wm, &"s".repeat(cap / 2));
    }
    assert!(!wm.entries.lock().iter().any(|e| e.acts.iter().any(|a| a.call.id == children[0])), "its entry rolled out");
    assert!(children.iter().all(|c| live(c)), "the receipt still holds its children");

    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    assert!(wm.recent_results_messages().join("\n").contains(&format!("{receipt} — collapsed")), "collapsed to a pointer");
    assert!(live(&receipt) && children.iter().all(|c| live(c)), "the pointer holds the receipt and its children");

    let snapshot = wm.snapshot();
    let checkpointed = snapshot.reachable_handles();
    assert!(children.iter().all(|c| checkpointed.contains(c)), "the checkpoint names each child");
    // Through the checkpoint's own encoding, not just in memory.
    let written = serde_json::to_string(&snapshot).expect("test: encode");
    let read: VolatileSnapshot = serde_json::from_str(&written).expect("test: decode");
    assert_eq!(read.receipt_children, snapshot.receipt_children, "the dependencies survive the checkpoint's serde");
    drop(wm);
    assert!(children.iter().all(|c| !live(c)), "released with the mind");

    // Real child files through the guarded eviction the spill pool runs under pressure.
    use crate::paging::filesystem_tier_pool::Removal;
    let root = std::env::temp_dir().join(format!("wm-deps-{}", Uuid::new_v4()));
    let dir = root.join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&dir).expect("test: dir");
    let files: Vec<std::path::PathBuf> = children.iter().map(|c| dir.join(format!("{c}.log"))).collect();
    for (file, child) in files.iter().zip(&children) {
        std::fs::write(file, child).expect("test: child file");
    }
    let restored = WorkingMemory::new(2);
    restored.restore(read);
    assert!(children.iter().all(|c| live(c)), "held again by the restored mind");
    for file in &files {
        let evict = crate::cognition::tool_executor::spill::evict_unless_live(file);
        assert!(matches!(evict, Removal::Protected) && file.exists(), "pressure cannot evict a restored receipt's child");
    }
    drop(restored);
    for file in &files {
        let evict = crate::cognition::tool_executor::spill::evict_unless_live(file);
        assert!(matches!(evict, Removal::Removed) && !file.exists(), "released, it is evictable");
    }
    let _ = std::fs::remove_dir_all(&root);
}

// what this catches (Codex on #4987): a receipt with no handle (its save failed) files its children
// under `seq:N`; restore saves it and it takes a handle, so the children must move to that
// key or the next publish prunes them as belonging to no receipt.
#[test]
fn a_restored_handleless_receipt_keeps_its_children_under_its_new_handle() {
    let child = format!("{:016x}", Uuid::new_v4().as_u128() as u64);
    let mut snapshot = WorkingMemory::new(8).snapshot();
    snapshot.actions.push(crate::cognition::working_memory::action_log::ActionRecord {
        seq: 7,
        room: None,
        scope: None,
        operation: "code/read".into(),
        head: None,
        result: Some("a receipt whose save failed".into()),
        handle: None,
        summary: None,
        stored_chars: None,
        received_ms: None,
        act: None,
    });
    snapshot.receipt_children.insert(receipt_key(7, None), vec![child.clone()]);
    let unsaved = snapshot.unsaved_receipts();
    let handle = format!("{:016x}", Uuid::new_v4().as_u128() as u64);
    snapshot.actions[0].handle = Some(handle.clone()); // what a successful save sets
    snapshot.refile_children(&unsaved);
    assert_eq!(snapshot.receipt_children.get(&handle), Some(&vec![child.clone()]), "re-filed under its handle");
    assert!(!snapshot.receipt_children.contains_key("seq:7"));
    let wm = WorkingMemory::new(8);
    wm.restore(snapshot);
    assert!(
        crate::cognition::tool_executor::spill::is_live(&std::path::PathBuf::from(format!("/spill/p/{child}.log"))),
        "and still reachable after the restore's publish"
    );
}

// what this catches (card 12b080c3): a call's own saved result, shown to her as a pointer
// inside its receipt, left out of the live set, so eviction could delete it while the
// receipt still names it, and the act seam's pin released before it was published.
#[test]
fn each_call_s_saved_result_is_live_once_its_receipt_is_recorded() {
    use crate::ai::types::{ResultCustody, ToolCall, ToolResult};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};
    let call_handle = format!("{:016x}", Uuid::new_v4().as_u128() as u64);
    let act = Observation {
        call: ToolCall { id: "c1".into(), name: "code/read".into(), input: serde_json::json!({}) },
        output: ToolOutput {
            image: None,
            additional_images: None,
            result: ToolResult {
                tool_use_id: "c1".into(),
                content: format!("[saved as output `{call_handle}`]"),
                is_error: None,
                custody: ResultCustody::Pointer { handle: call_handle.clone() },
            },
            verb: ToolVerb::classify("code/read"),
            paths: Vec::new(),
            verdict: Default::default(),
        },
        status: ActStatus::Executed,
    };
    let file = std::path::PathBuf::from(format!("/spill/p/{call_handle}.log"));
    let wm = WorkingMemory::new(8);
    wm.record_receipt_typed(&[act], "read it", None, ResultCustody::Unsaved);
    assert!(
        crate::cognition::tool_executor::spill::is_live(&file),
        "published by the record itself, before the act seam drops its pin"
    );
    drop(wm);
    assert!(!crate::cognition::tool_executor::spill::is_live(&file), "released with the mind");
}

// what this catches (BigMama's live receipt of #4963 on the 5090, card df0035bf; FLOW §2d):
// restore folding what she sees by a window or a size. Restore once collapsed against the floor
// budget before her window was known (a 1,536-char share), later against the window itself, and
// then against a size cap of ours (`RESULTS_RESIDENT_CHARS`). Only the memory owner sheds now, so
// restoring with no window or a small one keeps every body the snapshot held resident.
#[test]
fn restore_keeps_every_resident_body_whatever_the_window() {
    let source = WorkingMemory::new(8);
    for i in 0..4 {
        source.record_receipt_typed(&[], &format!("r{i} {}", "w".repeat(64 * BODY)), None, saved(&format!("restore-{i}")));
    }
    let snapshot = source.snapshot();
    let entries = |wm: &WorkingMemory| {
        wm.recent_results_messages().iter().filter(|m| m.starts_with("[result ")).count()
    };
    let held = entries(&source);
    let unknown = WorkingMemory::new(8);
    unknown.restore(snapshot.clone());
    assert_eq!(entries(&unknown), held, "no window yet: nothing moves at restore");
    assert_eq!(unknown.resident_result_bytes(), source.resident_result_bytes(), "every body restored resident");
    let small = WorkingMemory::new(8);
    small.restore(snapshot);
    assert_eq!(entries(&small), held, "a small window changes nothing her view is offered");
}

// what this catches: a size cap of ours deciding what RAM holds (`RESULTS_RESIDENT_CHARS =
// 1 << 20` evicted every mind's oldest bodies at receipt, settlement and restore; Joel
// 2026-10-29: no hand-rigged constants). However large the ring grows, receipts and
// settlements move nothing to its handle: only the memory owner's shed does.
#[test]
fn results_are_never_evicted_at_receipt_however_large_the_ring() {
    let wm = WorkingMemory::new(8);
    let chunk = 64 * BODY;
    for i in 0..40 {
        wm.record_receipt_typed(&[], &format!("r{i} {}", "w".repeat(chunk)), None, saved(&format!("ring-{i}")));
        if i % 10 == 9 {
            wm.record_settlement("a burst done", None);
        }
    }
    assert!(wm.resident_result_bytes() >= 40 * chunk as u64, "all 40 bodies resident: {}", wm.resident_result_bytes());
    let snapshot = wm.snapshot();
    assert!(snapshot.actions.iter().all(|record| record.result.is_some()), "every record holds its body");
}

// what this catches: an unsaved result dropped at collapse (Astra on #4963 at 4840af859:
// a pointer saying "never saved" documents the loss instead of preventing it). Its save
// failed, so nothing else holds its bytes: it stays WHOLE through a shed and restore while
// the saved results around it collapse to pointers.
#[test]
fn an_unsaved_result_keeps_its_bytes_through_a_shed_and_restore() {
    let wm = WorkingMemory::new(8);
    let cap = 2 * BODY;
    let unsaved = format!("UNSAVED {}", "u".repeat(cap / 2));
    wm.record_receipt_typed(&[], &unsaved, None, crate::ai::types::ResultCustody::Unsaved);
    for _ in 0..4 {
        record_saved(&wm, &"s".repeat(cap / 2));
    }
    wm.record_settlement("done", None);
    shed(&wm, cap / 2);
    let whole = |m: &WorkingMemory| m.recent_results_messages().iter().any(|r| r.ends_with(&unsaved));
    assert!(whole(&wm), "kept whole through the shed");
    assert!(
        wm.recent_results_messages().join("\n").contains("ring-test — collapsed"),
        "the saved ones around it collapsed"
    );
    let restored = WorkingMemory::new(8);
    restored.restore(wm.snapshot());
    assert!(whole(&restored), "and through restore");
}

// what this catches (Fable on #5573; Aiko on the M5, 2026-10-29): results an earlier life evicted
// by the old window share carry a handle but no stored size, so after a deploy her view priced
// their whole at nothing and showed them as pointers forever. A restore prices each from its
// saved output; one whose output can't be read stays a pointer, never a guessed size.
#[test]
fn a_restore_prices_a_legacy_evicted_result_from_its_saved_output() {
    let wm = WorkingMemory::new(8);
    let persona = Uuid::new_v4();
    let pin = crate::cognition::tool_executor::spill::LivePin::new();
    let body = "a line of the saved output\n".repeat(50);
    wm.record_receipt_typed(&[], &body, None, pin.retain(persona, &body));
    wm.record_receipt_typed(&[], "a result whose output is gone", None, pin.retain(persona, "a result whose output is gone"));
    wm.record_receipt_typed(&[], "the newest stays resident", None, pin.retain(persona, "the newest stays resident"));
    shed(&wm, 1);
    let mut snapshot = wm.snapshot();
    snapshot.actions = snapshot.action_records();
    // the shape the old share left: body gone, handle kept, no stored size
    let legacy: Vec<_> = snapshot.actions.iter_mut().filter(|r| r.result.is_none()).map(|r| {
        r.stored_chars = None;
        r.handle.clone().expect("test: saved")
    }).collect();
    assert_eq!(legacy.len(), 2, "two evicted, the newest resident");
    let measured = body.trim().chars().count();
    snapshot.price_evicted_wholes(|handle| (handle == legacy[0]).then_some(measured));
    let restored = WorkingMemory::new(8);
    restored.restore(snapshot);
    let (acts, _) = restored.action_renderings();
    let label = restored.action_whole_label(acts[0].seq).expect("test: a label").chars().count();
    assert_eq!(acts[0].stored.as_ref().map(|(chars, _)| *chars), Some(label + measured), "priced from its saved output, label included");
    assert_eq!(acts[1].stored, None, "an output that can't be read stays a pointer");
}

// what this catches: two entries sharing an occurrence after a restore (Codex on card 4ee623ad).
// An occurrence is never checkpointed, so the checkpoint's shape is unchanged; restore numbers
// every entry it brings back from the one counter, and the next append is distinct and newest.
#[test]
fn restored_and_appended_entries_never_share_an_occurrence() {
    let wm = WorkingMemory::new(8);
    wm.record_settlement("first", None);
    wm.record_settlement("second", None);
    let saved = serde_json::to_value(wm.snapshot()).expect("test: a snapshot serializes");
    assert!(!saved.to_string().contains("occurrence"), "no occurrence in the checkpoint: {saved}");
    let restored = WorkingMemory::new(8);
    restored.restore(serde_json::from_value(saved).expect("test: the checkpoint loads"));
    restored.record_settlement("after the restore", None);
    let occurrences: Vec<u64> = restored.recent_entries().iter().map(|entry| entry.occurrence).collect();
    let mut distinct = occurrences.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), occurrences.len(), "every occurrence distinct: {occurrences:?}");
    assert!(occurrences.iter().all(|&o| o > 0), "none left unstamped: {occurrences:?}");
    assert_eq!(occurrences.last(), distinct.last(), "the append is the newest: {occurrences:?}");
}
