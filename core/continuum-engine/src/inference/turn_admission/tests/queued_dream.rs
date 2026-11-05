//! A queued dream holds nothing a resident needs (card 920c43bc).
#![cfg(test)]
use super::*;

// What this catches (Fable, M5 0b2854327 01:24-01:50Z): a dream parked on the single scratch
// slot (`acquire_slot(scratch)`) while holding the adapter permit and a serving lane its
// distiller had taken before generate, so residents waited at the lane gate (Aiko 1,141 s,
// Benchy 132 s) with two slots idle. Queued, a dream now holds no adapter permit and no lane;
// admitted, it holds exactly one adapter permit, the scratch slot, and still no lane.
#[tokio::test]
async fn a_dream_queued_on_the_scratch_slot_holds_no_adapter_permit_and_no_lane() {
    let pool = Arc::new(KvSlotPool::new("test://queued-dream", 3)); // index 2 is scratch
    let scratch = pool.scratch_slot().expect("three slots reserve a scratch slot"); // test: pool contract
    let held = pool.acquire_slot(scratch).await.expect("test: another call holds scratch");
    let sem = Arc::new(Semaphore::new(3));
    let client = reqwest::Client::new();
    let queued = admit_turn(&sem, SlotClass::Dream, None, Some(pool.clone()), &client, "test://queued-dream", 0, std::time::Duration::ZERO);
    tokio::pin!(queued);
    assert!(futures::poll!(&mut queued).is_pending(), "the dream waits for the scratch slot");
    assert_eq!(sem.available_permits(), 3, "a queued dream holds no adapter permit");
    drop(held);
    let admitted = queued.await.expect("test: admitted once scratch is released");
    assert_eq!(admitted.scratch_slot(), Some(scratch));
    assert!(admitted._lane.is_none(), "on a scratch slot a dream takes no resident lane");
    assert_eq!(sem.available_permits(), 2, "admitted, it holds one adapter permit");
}

// What this catches: with no scratch slot a dream borrows a citizen slot, so it must still
// yield to residents through the lane gate (the 2026-07-26 failure: dreams on any lane, a
// directed reply queued behind them), yet a dream QUEUED on a busy citizen slot holds no lane
// and no adapter permit (Fable on #5786: the lane was held across the slot wait). Other
// background traffic (a summary, possibly nested in a lane-holding turn) takes no lane.
// One test, so the process-wide lane gate's count is read with nothing else of ours on it.
#[tokio::test]
async fn without_a_scratch_slot_a_dream_queues_holding_nothing_runs_holding_one_ambient_lane_and_waits_for_the_adapter_holding_no_slot_and_no_lane() {
    use crate::inference::resource_admission::serving_lane_permits_available as lanes_free;
    let pool = Arc::new(KvSlotPool::new("test://dream-borrow", 2));
    assert!(pool.scratch_slot().is_none(), "two lanes reserve no scratch slot");
    let sem = Arc::new(Semaphore::new(2));
    let client = reqwest::Client::new();
    let before = lanes_free();

    let busy = pool.acquire_slot(pool.transient_slot()).await.expect("test: a decode holds the citizen slot");
    let queued = admit_turn(&sem, SlotClass::Dream, None, Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO);
    tokio::pin!(queued);
    assert!(futures::poll!(&mut queued).is_pending(), "the dream waits for the busy citizen slot");
    assert_eq!(sem.available_permits(), 2, "a queued dream holds no adapter permit");
    assert_eq!(lanes_free(), before, "a queued dream holds no serving lane");
    drop(busy);
    let dream = queued.await.expect("test: admitted once the slot is released");
    assert!(dream._lane.is_some(), "a borrowing dream runs holding an ambient lane");
    assert_eq!(lanes_free(), before - 1, "exactly one");
    drop(dream);
    assert_eq!(lanes_free(), before, "and gives it back");

    // The inversion Codex named on #5786: a resident holds the adapter permit while it waits for
    // a slot. A dream waiting for the adapter must then hold no slot, or each waits on the other.
    let one = Arc::new(Semaphore::new(1));
    let resident = one.clone().acquire_owned().await.expect("test: a resident holds the only adapter permit");
    let waiting = admit_turn(&one, SlotClass::Dream, None, Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO);
    tokio::pin!(waiting);
    assert!(futures::poll!(&mut waiting).is_pending(), "the dream waits for the adapter permit");
    assert!(pool.try_acquire_slot(pool.transient_slot()).is_some(), "while it waits for the adapter it holds no slot");
    assert_eq!(lanes_free(), before, "and no serving lane (Codex on #5786)");
    drop(resident);
    let admitted = waiting.await.expect("test: admitted once the resident's permit returns");
    assert!(admitted._lane.is_some());
    drop(admitted);

    // Codex on #5786: the lane follows the ADMITTED generation's pool, never the discovery the
    // call came with. A stale three-slot discovery (scratch) over a two-slot endpoint borrows.
    let root = format!("test://dream-generation-{}", Uuid::new_v4());
    let _admitted_pool = crate::inference::slots::directory().ensure_pool(&root, 2);
    let stale_discovery = Arc::new(KvSlotPool::new(&root, 3));
    let after_transition = admit_turn(&sem, SlotClass::Dream, None, Some(stale_discovery), &client, &root, 0, std::time::Duration::ZERO)
        .await
        .expect("test: the dream borrows a citizen slot of the admitted generation");
    assert!(after_transition._lane.is_some(), "a dream on a no-scratch generation holds its lane");
    drop(after_transition);
    assert_eq!(lanes_free(), before, "and gives it back");

    let summary = admit_turn(&sem, SlotClass::Background, None, Some(pool.clone()), &client, "test://dream-borrow", 0, std::time::Duration::ZERO)
        .await
        .expect("test: background traffic borrows a free citizen slot");
    assert!(summary._lane.is_none(), "other background traffic takes no lane");
}
