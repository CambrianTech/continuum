#![cfg(test)]

//! An engine's exit as an event: the waiter wakes on the reaper's announcement, never on a timer.

use super::super::*;
use futures::FutureExt;

async fn generation(dir: &SlotDirectory, root: &str) -> EngineGeneration {
    let endpoint = dir.endpoint(root);
    let transition = endpoint.transition().await;
    transition.start_generation().expect("test: a first generation")
}

// what this catches: an exit wait that polls or times out instead of waking on the owner's
// announcement. It stays pending while the child runs and completes on `observed_exit`.
#[tokio::test]
async fn the_owners_exit_announcement_wakes_the_waiter() {
    let dir = SlotDirectory { endpoints: dashmap::DashMap::new() };
    let engine = generation(&dir, "test://exit-wakes").await;
    let mut wait = Box::pin(engine.exit().confirmed());
    assert!((&mut wait).now_or_never().is_none(), "the child runs: no outcome yet");
    engine.observed_exit();
    assert_eq!(wait.await, Ok(()));
    assert_eq!(engine.exit().confirmed().now_or_never(), Some(Ok(())), "a late waiter sees the exit at once");
}

// what this catches: a caller giving up read as the exit. Dropping a waiter leaves the
// generation unexited, its custody with the owner, and a new waiter still waits for the child.
#[tokio::test]
async fn a_cancelled_waiter_releases_nothing() {
    let dir = SlotDirectory { endpoints: dashmap::DashMap::new() };
    let engine = generation(&dir, "test://exit-cancel").await;
    let mut wait = Box::pin(engine.exit().confirmed());
    assert!((&mut wait).now_or_never().is_none());
    drop(wait);
    assert!(!engine.has_exited(), "abandoning the wait is not the child's exit");
    let mut again = Box::pin(engine.exit().confirmed());
    assert!((&mut again).now_or_never().is_none(), "custody still awaits the real exit");
    engine.observed_exit();
    assert_eq!(again.await, Ok(()));
}

// what this catches: an owner that vanishes before its child reported exit leaving the waiter
// hung or told the child exited. It is the typed OwnerGone.
#[tokio::test]
async fn an_owner_gone_before_the_exit_is_a_typed_outcome() {
    let dir = SlotDirectory { endpoints: dashmap::DashMap::new() };
    let engine = generation(&dir, "test://exit-owner-gone").await;
    let mut wait = Box::pin(engine.exit().confirmed());
    assert!((&mut wait).now_or_never().is_none());
    drop(engine);
    drop(dir);
    assert_eq!(wait.await, Err(EngineExitError::OwnerGone));
}
