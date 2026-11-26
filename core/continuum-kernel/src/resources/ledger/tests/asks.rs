#![cfg(test)]

//! The authority's asks, carves and core entries (UNIFIED-PAGING.md step 3): an ask waits instead of being
//! refused, a stage is carved from a granted reserve in one transition, and every lease is a core entry.

use super::*;
// what this catches (the 5090, 2026-11-26: a learning seat refused 5.7 GB with 5 MB free, and nothing
// ever woke it): an ask waits instead, a foreground acquire still takes what is free now without
// queueing behind it, and releasing what serving holds grants the ask as its lease, unasked again.
#[test]
fn an_ask_waits_and_becomes_its_lease_when_room_is_released() {
    let mut l = ResourceLeaseLedger::new();
    l.set_capacity(ResourceKind::Vram, 10_000);
    l.acquire(&req("serving", ResourceKind::Vram, 9_980, ReclaimPolicy::Pinned), "serving-1".into(), 0).expect("test: serving fits");
    let Ok(Asked::Waiting { ask, short }) = l.ask(&req("learning:her", ResourceKind::Vram, 5_000, ReclaimPolicy::Graceful), "seat", 1) else {
        panic!("test: the seat waits")
    };
    assert_eq!(short, 4_980, "the wait names what it lacks");
    assert!(l.take_granted(ask).is_none(), "nothing granted yet");
    assert!(l.acquire(&req("vision", ResourceKind::Vram, 10, ReclaimPolicy::Graceful), "vision".into(), 2).is_ok(), "the foreground is never queued behind an ask");
    l.release("serving-1").expect("test: serving lets go");
    let granted = l.take_granted(ask).expect("test: granted on the release, without being asked again");
    assert_eq!((granted.lease_id.as_str(), granted.bytes), ("seat", 5_000));
    assert!(l.lease("seat").is_some(), "it is a lease like any other");
    assert_eq!(l.available(ResourceKind::Vram), 10_000 - 10 - 5_000);
}

// what this catches: a seat that stopped wanting its memory still granted it, holding memory no one
// uses.
#[test]
fn a_withdrawn_ask_is_never_granted() {
    let mut l = ResourceLeaseLedger::new();
    l.set_capacity(ResourceKind::Vram, 100);
    l.acquire(&req("serving", ResourceKind::Vram, 100, ReclaimPolicy::Pinned), "serving-1".into(), 0).expect("test: serving fits");
    let Ok(Asked::Waiting { ask, .. }) = l.ask(&req("learning:her", ResourceKind::Vram, 50, ReclaimPolicy::Graceful), "seat", 1) else {
        panic!("test: the seat waits")
    };
    assert!(l.withdraw(ask));
    l.release("serving-1").expect("test: serving lets go");
    assert!(l.take_granted(ask).is_none());
    assert_eq!(l.available(ResourceKind::Vram), 100);
    // granted, then withdrawn before its owner took it: released, never held for no one
    l.acquire(&req("serving", ResourceKind::Vram, 100, ReclaimPolicy::Pinned), "serving-2".into(), 2).expect("test: serving fits");
    let Ok(Asked::Waiting { ask, .. }) = l.ask(&req("learning:her", ResourceKind::Vram, 50, ReclaimPolicy::Graceful), "seat-2", 3) else {
        panic!("test: the seat waits")
    };
    l.release("serving-2").expect("test: serving lets go");
    assert!(l.withdraw(ask), "a granted, untaken ask is withdrawn");
    assert!(l.lease("seat-2").is_none() && l.available(ResourceKind::Vram) == 100, "and its lease released");
}

// what this catches: a seat's stage lease carved from its granted reserve losing the bytes to a waiting
// ask in between (the shrink told place and the wait took them), or a carve past the reserve.
#[test]
fn a_stage_carved_from_its_reserve_keeps_the_bytes_from_every_wait() {
    let mut l = ResourceLeaseLedger::new();
    l.set_capacity(ResourceKind::Vram, 10_000);
    l.acquire(&req("learning:her", ResourceKind::Vram, 5_000, ReclaimPolicy::Graceful), "reserve".into(), 0).expect("test: the reserve fits");
    l.acquire(&req("serving", ResourceKind::Vram, 5_000, ReclaimPolicy::Pinned), "serving-1".into(), 0).expect("test: serving fits");
    let Ok(Asked::Waiting { ask, .. }) = l.ask(&req("learning:him", ResourceKind::Vram, 1_000, ReclaimPolicy::Graceful), "his", 1) else {
        panic!("test: his seat waits")
    };
    let stage = l.carve("reserve", &req("learning:her", ResourceKind::Vram, 3_000, ReclaimPolicy::Graceful), "weights", 2).expect("test: carved");
    assert_eq!(stage.bytes, 3_000);
    assert_eq!(l.lease("reserve").map(|lease| lease.bytes), Some(2_000), "the reserve keeps the rest");
    assert!(l.take_granted(ask).is_none(), "the wait never saw the carved bytes");
    assert!(l.carve("reserve", &req("learning:her", ResourceKind::Vram, 2_001, ReclaimPolicy::Graceful), "over", 3).is_err(), "never past the reserve");
    l.release("reserve").expect("test: the rest goes back");
    assert!(l.take_granted(ask).is_some(), "what the reserve gives back is granted to the wait");
}

// what this catches (Codex on 07427e2fe): a stage carved from a reserve past its term, given a fresh term
// while the lapsed reserve's charge waits for its fenced release: refused, changing nothing.
#[test]
fn nothing_is_carved_from_a_lapsed_reserve() {
    let mut l = ResourceLeaseLedger::new();
    l.set_capacity(ResourceKind::Vram, 10_000);
    let reserve = LeaseRequest { ttl_ms: 100, ..req("learning:her", ResourceKind::Vram, 5_000, ReclaimPolicy::Graceful) };
    l.acquire(&reserve, "reserve".into(), 0).expect("test: the reserve fits");
    let before = l.available(ResourceKind::Vram);
    let stage = req("learning:her", ResourceKind::Vram, 1_000, ReclaimPolicy::Graceful);
    assert!(matches!(l.carve("reserve", &stage, "weights", 100), Err(CarveError::Lease(LeaseError::ExpiredLease { .. }))), "at its term");
    assert!(matches!(l.carve("reserve", &stage, "weights", 500), Err(CarveError::Lease(LeaseError::ExpiredLease { .. }))), "after it");
    assert_eq!((l.lease("reserve").map(|lease| lease.bytes), l.available(ResourceKind::Vram)), (Some(5_000), before), "the charge is unchanged");
    assert!(l.lease("weights").is_none());
}

// what this catches (step 3, Fable's correctness point): a kernel lease that is not an entry in the
// core's table, which the law's shrink pass could never reach; and a released or shrunk-away lease left
// in the table forever.
#[test]
fn every_kernel_lease_is_an_entry_in_the_core_and_leaves_with_it() {
    let mut l = ResourceLeaseLedger::new();
    l.set_capacity(ResourceKind::Vram, 10_000);
    l.acquire(&req("serving", ResourceKind::Vram, 4_000, ReclaimPolicy::Graceful), "a".into(), 0).expect("test: fits");
    l.acquire(&req("embed", ResourceKind::Vram, 1_000, ReclaimPolicy::Graceful), "b".into(), 0).expect("test: fits");
    let ask_of = |l: &ResourceLeaseLedger, id: &str| l.leases.get(id).map(|held| held.ask).expect("test: held");
    let (a, b) = (ask_of(&l, "a"), ask_of(&l, "b"));
    assert!(matches!(l.core.status(a), Some(continuum_sage::place::Status::Committed)), "a lease is a committed entry the shrink pass reaches");
    l.release("a").expect("test: release");
    assert!(l.core.status(a).is_none(), "released: its entry leaves the table");
    l.shrink("b", 1_000).expect("test: shrink to nothing");
    assert!(l.core.status(b).is_none(), "shrunk to nothing: its entry leaves too");
    assert_eq!(l.available(ResourceKind::Vram), 10_000);
}
