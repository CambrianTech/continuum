#![cfg(test)]
use std::sync::Arc;

use uuid::Uuid;

use crate::ai::types::ResultCustody;
use crate::cognition::persona_workspace::mind_results_reporter::{
    over_gate, resident_bytes, shed_to, Minds,
};
use crate::cognition::tool_executor::spill::{self, LivePin};
use crate::cognition::working_memory::WorkingMemory;

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
    let total = resident_bytes(&minds);
    assert_eq!(total, 7 * BODY as u64, "precondition: every body resident");

    shed_to(&minds, total - 2 * BODY as u64);
    assert_eq!(
        resident_bytes(&minds),
        5 * BODY as u64,
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

    shed_to(&minds, 0);
    assert_eq!(resident(&a), ["a3"], "a's newest stays");
    assert_eq!(
        resident(&b),
        ["b1", "b4"],
        "unsaved and newest stay through any shed"
    );
}

// what this catches (Codex on #5595): a saved copy counted as safe because its FILE EXISTS. A
// file that is there but does not load back to the body (wrong content), or is gone, is not a
// copy: moving the body would lose the last one, and her view's load would render a pointer.
// Real files through the spill owner, never an injected verdict.
#[test]
fn a_copy_that_cannot_reproduce_the_body_keeps_it_resident() {
    let persona = Uuid::new_v4();
    let pin = LivePin::new();
    let memory = Arc::new(WorkingMemory::new(8));
    let wrong = kept(&memory, persona, &pin, "c1");
    let gone = kept(&memory, persona, &pin, "c2");
    kept(&memory, persona, &pin, "c3");
    kept(&memory, persona, &pin, "c4");
    std::fs::write(
        spill::resolve(persona, &wrong).expect("test: path"),
        "not what she read",
    )
    .expect("test: overwrite");
    std::fs::remove_file(spill::resolve(persona, &gone).expect("test: path"))
        .expect("test: remove");
    let minds: Minds = vec![(persona, Arc::clone(&memory))];

    shed_to(&minds, 0);
    assert_eq!(
        resident(&memory),
        ["c1", "c2", "c4"],
        "wrong content and a missing file keep their bodies; c3 moved"
    );
    assert_eq!(resident_bytes(&minds), 3 * BODY as u64);
}

// what this catches: the memory owner reading a wrong total, so it sheds too little or too
// much. `report()` is the sum of resident body BYTES across every mind (not characters: a
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
        2 * BODY as u64 + wide.len() as u64,
        "bytes, not chars"
    );
    assert_eq!(
        resident_bytes(&minds),
        a.resident_result_bytes() + b.resident_result_bytes()
    );
}

// what this catches: a shed target of our own invention. The bytes shed are how far the host's
// used RAM stands past the monitor's own gate for this priority; nothing before the first poll.
#[test]
fn the_shed_target_is_the_hosts_overrun_past_the_monitors_gate() {
    use continuum_kernel::system_resources::{MemoryPriority, PressureSnapshot};
    assert_eq!(
        over_gate(&PressureSnapshot::default()),
        0,
        "no poll yet: nothing to shed"
    );
    let total = 100 * 1024 * 1024 * 1024u64;
    let gate = (total as f64 * MemoryPriority::Interactive.pressure_gate()) as u64;
    let snapshot = |used: u64| PressureSnapshot {
        total_bytes: total,
        available_bytes: total - used,
        ..Default::default()
    };
    assert_eq!(over_gate(&snapshot(gate)), 0, "at the gate: nothing");
    assert_eq!(
        over_gate(&snapshot(gate + 5 * BODY as u64)),
        5 * BODY as u64,
        "past it: exactly the overrun"
    );
}
