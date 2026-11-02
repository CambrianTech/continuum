#![cfg(test)]
use std::sync::Arc;

use uuid::Uuid;

use crate::ai::types::ResultCustody;
use crate::cognition::persona_workspace::mind_results_reporter::{
    budget_of, prove, resident_bytes, shed_to, LastCopy, Minds, ProvenCopies,
};
use crate::cognition::tool_executor::spill::{self, LivePin};
use crate::cognition::working_memory::WorkingMemory;
use parking_lot::Mutex;

/// A test body's size in bytes; production has no such number (the memory owner sheds).
const BODY: usize = 1_000;

fn body(name: &str) -> String {
    format!("{name}{}", "b".repeat(BODY - name.len()))
}

fn receipt(memory: &WorkingMemory, name: &str, custody: ResultCustody) {
    memory.record_receipt_typed(&[], &body(name), None, custody);
    // Distinct receipt times, so "oldest across minds" is the order they were received in.
    std::thread::sleep(std::time::Duration::from_millis(2));
}

/// Record `name` saved for real through the spill owner; its handle.
fn kept(memory: &WorkingMemory, persona: Uuid, pin: &LivePin, name: &str) -> String {
    let custody = pin.retain(persona, &body(name));
    let handle = custody.handle().expect("test: saved").to_owned();
    receipt(memory, name, custody);
    handle
}

fn saved(handle: &str) -> ResultCustody {
    ResultCustody::Whole {
        handle: handle.into(),
    }
}

/// The bytes no shed releases: every record's ledger line and summary across `minds`. The memory
/// owner charges them as pinned (Codex on #5637), and they stay constant through every shed.
fn lines(minds: &Minds) -> u64 {
    minds
        .iter()
        .flat_map(|(_, memory)| memory.snapshot().actions)
        .map(|r| [r.head, r.summary].into_iter().flatten().map(|t| t.len() as u64).sum::<u64>())
        .sum()
}

/// Which of a mind's results are resident, by the name each body starts with.
fn resident(memory: &WorkingMemory) -> Vec<String> {
    memory
        .snapshot()
        .actions
        .iter()
        .filter_map(|r| r.result.as_ref().map(|body| body[..2].to_owned()))
        .collect()
}

// what this catches: a shed that moves the wrong bodies. It must take the OLDEST received
// across every mind (not each mind's oldest in turn), stop at the allocation, and never move a
// mind's newest result or an unsaved one. A moved body keeps its handle and its measured
// length, and its saved copy loads back to exactly what she read.
#[test]
fn the_shed_moves_the_oldest_across_minds_down_to_the_allocation() {
    let (pa, pb) = (Uuid::new_v4(), Uuid::new_v4());
    let pin = LivePin::new();
    let (a, b) = (
        Arc::new(WorkingMemory::new(8)),
        Arc::new(WorkingMemory::new(8)),
    );
    kept(&a, pa, &pin, "a1");
    receipt(&b, "b1", ResultCustody::Unsaved);
    let b2 = kept(&b, pb, &pin, "b2");
    kept(&b, pb, &pin, "b3");
    kept(&a, pa, &pin, "a2");
    kept(&b, pb, &pin, "b4");
    kept(&a, pa, &pin, "a3");
    let minds: Minds = vec![(pa, Arc::clone(&a)), (pb, Arc::clone(&b))];
    let proven = Mutex::new(ProvenCopies::default());
    let total = resident_bytes(&minds);
    assert_eq!(total, 7 * BODY as u64 + lines(&minds), "precondition: every body resident");

    shed_to(&minds, total - 2 * BODY as u64, &proven);
    assert_eq!(
        resident_bytes(&minds),
        5 * BODY as u64 + lines(&minds),
        "exactly down to the allocation"
    );
    assert_eq!(resident(&a), ["a2", "a3"], "a1 was the oldest received");
    assert_eq!(
        resident(&b),
        ["b1", "b3", "b4"],
        "b2 was next, before a2, though a2 is a's oldest left"
    );
    let moved = b
        .snapshot()
        .actions
        .into_iter()
        .find(|r| r.handle.as_deref() == Some(b2.as_str()))
        .expect("test: b2 kept its record");
    assert_eq!(
        moved.stored_chars,
        Some(BODY),
        "a moved body keeps its measured length"
    );
    assert_eq!(
        spill::saved_whole(pb, &b2).expect("test: b2 loads"),
        body("b2"),
        "and loads back to what she read"
    );

    shed_to(&minds, 0, &proven);
    assert_eq!(resident(&a), ["a3"], "a's newest stays");
    assert_eq!(
        resident(&b),
        ["b1", "b4"],
        "unsaved and newest stay through any shed"
    );
}

// what this catches (Codex and Cormac on #5595): a saved copy counted as safe because its FILE
// EXISTS. A file that is there but does not load back to the body (wrong content, a truncated
// write), or is gone, is not a copy: moving the body would lose the last one, and her view's
// load would render a pointer. Real files through the spill owner, never an injected verdict.
#[test]
fn a_copy_that_cannot_reproduce_the_body_keeps_it_resident() {
    let persona = Uuid::new_v4();
    let pin = LivePin::new();
    let memory = Arc::new(WorkingMemory::new(8));
    let wrong = kept(&memory, persona, &pin, "c1");
    let truncated = kept(&memory, persona, &pin, "c2");
    let gone = kept(&memory, persona, &pin, "c3");
    kept(&memory, persona, &pin, "c4");
    kept(&memory, persona, &pin, "c5");
    let path = |handle: &str| spill::resolve(persona, handle).expect("test: path");
    std::fs::write(path(&wrong), "not what she read").expect("test: overwrite");
    std::fs::write(path(&truncated), &body("c2")[..BODY / 2]).expect("test: truncated write");
    std::fs::remove_file(path(&gone)).expect("test: remove");

    let candidates = memory.resident_results().candidates;
    let verdict = |handle: &str| {
        let candidate = candidates
            .iter()
            .find(|c| c.handle == handle)
            .expect("test: a candidate");
        prove(persona, &memory, candidate).expect("test: still resident")
    };
    assert!(
        matches!(verdict(&truncated), Err(LastCopy::Mismatch)),
        "a truncated copy is a mismatch"
    );
    assert_eq!(
        verdict(&truncated).err().map(|e| e.reason()),
        Some("mismatch"),
        "the wm.results.last_copy_kept probe's reason"
    );
    assert!(matches!(verdict(&wrong), Err(LastCopy::Mismatch)));
    assert!(
        matches!(verdict(&gone), Err(LastCopy::Unreadable(_))),
        "a missing copy is unreadable"
    );

    let minds: Minds = vec![(persona, Arc::clone(&memory))];
    shed_to(&minds, 0, &Mutex::new(ProvenCopies::default()));
    assert_eq!(
        resident(&memory),
        ["c1", "c2", "c3", "c5"],
        "unprovable copies keep their bodies; c4 moved"
    );
    assert_eq!(resident_bytes(&minds), 4 * BODY as u64 + lines(&minds));
}

// what this catches (Cormac on #5595): `budget()` touching the filesystem. The monitor calls it
// under its reporters lock on every snapshot, so a stat per resident record ran inside the
// monitor. Proofs are made at shed time and cached; with the spill directory gone, budget()
// answers the same from the cache, and an unproven body counts as pinned.
#[test]
fn budget_answers_from_the_proof_cache_without_the_filesystem() {
    let persona = Uuid::new_v4();
    let pin = LivePin::new();
    let memory = Arc::new(WorkingMemory::new(8));
    for name in ["d1", "d2", "d3", "d4"] {
        kept(&memory, persona, &pin, name);
    }
    let minds: Minds = vec![(persona, Arc::clone(&memory))];
    let proven = Mutex::new(ProvenCopies::default());
    let unproven = budget_of(&minds, &proven);
    assert_eq!(
        (unproven.min_bytes, unproven.max_bytes),
        (4 * BODY as u64 + lines(&minds), 4 * BODY as u64 + lines(&minds)),
        "nothing proven: all pinned"
    );

    shed_to(&minds, 3 * BODY as u64 + lines(&minds), &proven);
    assert_eq!(
        resident(&memory),
        ["d2", "d3", "d4"],
        "the oldest moved once proven"
    );
    let handle = memory.resident_results().candidates[0].handle.clone();
    let dir = spill::resolve(persona, &handle)
        .expect("test: path")
        .parent()
        .expect("test: dir")
        .to_owned();
    std::fs::remove_dir_all(&dir).expect("test: remove the spill dir");
    let cached = budget_of(&minds, &proven);
    assert_eq!(cached.max_bytes, 3 * BODY as u64 + lines(&minds));
    assert_eq!(
        cached.min_bytes, BODY as u64 + lines(&minds),
        "d2 and d3 proven from the cache; only the newest pinned"
    );
}

// what this catches: the memory owner reading a wrong total, so it sheds too little or too
// much. `report()` is the sum of every record's BYTES across every mind (bodies, lines, summaries) (not characters: a
// multi-byte body counts its bytes), and moves with every receipt and shed.
#[test]
fn report_sums_resident_bytes_across_minds() {
    let (a, b) = (
        Arc::new(WorkingMemory::new(8)),
        Arc::new(WorkingMemory::new(8)),
    );
    let minds: Minds = vec![
        (Uuid::new_v4(), Arc::clone(&a)),
        (Uuid::new_v4(), Arc::clone(&b)),
    ];
    assert_eq!(resident_bytes(&minds), 0);
    receipt(&a, "a1", saved("a1"));
    receipt(&a, "a2", saved("a2"));
    let wide = "é".repeat(BODY);
    b.record_receipt_typed(&[], &wide, None, saved("b1"));
    assert_eq!(
        resident_bytes(&minds),
        2 * BODY as u64 + wide.len() as u64 + lines(&minds),
        "bytes, not chars"
    );
    assert_eq!(
        resident_bytes(&minds),
        a.resident_result_bytes() + b.resident_result_bytes()
    );
}

