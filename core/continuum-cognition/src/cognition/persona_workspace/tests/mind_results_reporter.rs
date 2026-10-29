#![cfg(test)]
use std::sync::Arc;

use uuid::Uuid;

use crate::ai::types::ResultCustody;
use crate::cognition::persona_workspace::mind_results_reporter::{
    over_gate, resident_bytes, shed_to, Minds,
};
use crate::cognition::working_memory::WorkingMemory;

/// A test body's size in bytes; production has no such number (the memory owner sheds).
const BODY: usize = 1_000;

fn receipt(memory: &WorkingMemory, name: &str, custody: ResultCustody) {
    memory.record_receipt_typed(
        &[],
        &format!("{name}{}", "b".repeat(BODY - name.len())),
        None,
        custody,
    );
    // Distinct receipt times, so "oldest across minds" is the order they were received in.
    std::thread::sleep(std::time::Duration::from_millis(2));
}

fn saved(handle: &str) -> ResultCustody {
    ResultCustody::Whole {
        handle: handle.into(),
    }
}

/// Which of a mind's results are resident, by handle (`unsaved` for none).
fn resident(memory: &WorkingMemory) -> Vec<String> {
    memory
        .snapshot()
        .actions
        .iter()
        .filter(|r| r.result.is_some())
        .map(|r| r.handle.clone().unwrap_or_else(|| "unsaved".into()))
        .collect()
}

// what this catches: a shed that moves the wrong bodies. It must take the OLDEST received
// across every mind (not each mind's oldest in turn), stop at the allocation, and never move a
// mind's newest result, an unsaved one, or one whose saved output is missing (the last copy).
// A moved body keeps its handle and its measured length, so her view still reloads its whole.
#[test]
fn the_shed_moves_the_oldest_across_minds_down_to_the_allocation_and_keeps_last_copies() {
    let (a, b) = (
        Arc::new(WorkingMemory::new(8)),
        Arc::new(WorkingMemory::new(8)),
    );
    receipt(&a, "a1", saved("a1"));
    receipt(&b, "b1", ResultCustody::Unsaved);
    receipt(&b, "b2", saved("b2"));
    receipt(&b, "b3", saved("missing"));
    receipt(&a, "a2", saved("a2"));
    receipt(&b, "b4", saved("b4"));
    receipt(&a, "a3", saved("a3"));
    let minds: Minds = vec![
        (Uuid::new_v4(), Arc::clone(&a)),
        (Uuid::new_v4(), Arc::clone(&b)),
    ];
    let vouched = |_: Uuid, handle: &str| handle != "missing";
    let total = resident_bytes(&minds);
    assert_eq!(total, 7 * BODY as u64, "precondition: every body resident");

    shed_to(&minds, total - 2 * BODY as u64, &vouched);
    assert_eq!(
        resident_bytes(&minds),
        5 * BODY as u64,
        "exactly down to the allocation"
    );
    assert_eq!(resident(&a), ["a2", "a3"], "a1 was the oldest received");
    assert_eq!(
        resident(&b),
        ["unsaved", "missing", "b4"],
        "b2 was next, before a2, though a2 is a's oldest left"
    );
    let moved = b
        .snapshot()
        .actions
        .into_iter()
        .find(|r| r.handle.as_deref() == Some("b2"))
        .expect("test: b2 kept its record");
    assert_eq!(
        moved.stored_chars,
        Some(BODY),
        "a moved body keeps its measured length"
    );

    shed_to(&minds, 0, &vouched);
    assert_eq!(resident(&a), ["a3"], "a's newest stays");
    assert_eq!(
        resident(&b),
        ["unsaved", "missing", "b4"],
        "unsaved, unreloadable and newest stay through any shed"
    );
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
    let gate = (total as f64 * MemoryPriority::Background.pressure_gate()) as u64;
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
