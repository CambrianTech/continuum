//! Snapshot regression fixtures.
#![cfg(test)]
use super::*;
/// what this catches (2026-08-11, the solve-prelude keyhole): the settle wait
/// must resolve on the FIRST ready snapshot whose layout differs from the
/// pre-quiesce one (event-gated on the watch, no polling), and a planner that
/// keeps its layout must end as `Unchanged` at the bound — a legitimate
/// outcome that proceeds, never a park. A non-ready layout change must NOT
/// resolve it (mid-relaunch snapshots are transient, not a settled plan).
#[tokio::test]
async fn await_snapshot_resettle_resolves_on_ready_layout_change_and_bounds_out_otherwise() {
    use super::{await_snapshot_resettle, ServingSnapshot, SnapshotSettle};
    use std::time::Duration;

    // Resettle: a ready snapshot with a NEW layout resolves the wait.
    let (tx, rx) = tokio::sync::watch::channel(ServingSnapshot::empty());
    let waiter = tokio::spawn(await_snapshot_resettle(
        rx,
        4,
        16384,
        Duration::from_secs(5),
    ));
    // Transient mid-relaunch publish (not ready) must be ignored…
    let mut transitional = ServingSnapshot::empty();
    transitional.lanes = 1;
    transitional.served_context_window = 32768;
    transitional.ready = false;
    tx.send(transitional.clone()).unwrap();
    // …and the settled ready layout resolves it.
    transitional.ready = true;
    tx.send(transitional).unwrap();
    assert_eq!(
        waiter.await.unwrap(),
        SnapshotSettle::Resettled {
            lanes: 1,
            window: 32768
        }
    );

    // Unchanged, EARLY: two ready publishes with the same layout mean the
    // planner ran and held — resolves well before the (long) bound, so a
    // no-change solve pays reconcile-tick latency, never the full backstop.
    let (tx2, rx2) = tokio::sync::watch::channel(ServingSnapshot::empty());
    let waiter2 = tokio::spawn(await_snapshot_resettle(
        rx2,
        4,
        16384,
        Duration::from_secs(30),
    ));
    let mut same = ServingSnapshot::empty();
    same.lanes = 4;
    same.served_context_window = 16384;
    same.ready = true;
    tx2.send(same.clone()).unwrap();
    tokio::task::yield_now().await;
    tx2.send(same).unwrap();
    let early = tokio::time::timeout(Duration::from_secs(2), waiter2)
        .await
        .expect("two unchanged ready publishes must resolve early, not ride the bound")
        .unwrap();
    assert_eq!(early, SnapshotSettle::Unchanged);

    // Unchanged, BACKSTOP: a daemon that stops publishing ends at the bound.
    let (_tx3, rx3) = tokio::sync::watch::channel(ServingSnapshot::empty());
    let waiter3 = tokio::spawn(await_snapshot_resettle(
        rx3,
        4,
        16384,
        Duration::from_millis(80),
    ));
    assert_eq!(waiter3.await.unwrap(), SnapshotSettle::Unchanged);
}

/// what this catches (#350): the two states an EMPTY serving snapshot can mean
/// collapsing back into one. `ServingSnapshot::empty()` is published from process
/// start (install_serving_state runs in the daemon's `initialize()`, before the first
/// reconcile), so "we have not looked yet" and "we looked and nothing is serving" are
/// byte-identical. Readers that cannot tell them apart shout about a serving fault
/// during every boot: measured 116 false alarms in 38 bursts over 3 days.
///
/// CORRECTION (2026-08-07) to what this comment first claimed. It said the daemon
/// "published `active=<none>` ZERO times in 12 hours", which read as: empty means boot,
/// full stop. It does not. `serving_daemon` publishes `empty()` on EVERY lane teardown
/// — no servable plan, a re-home, and `declare_lane_wedged` (#175 self-heal) — the
/// original count simply had no probe on those publish sites to see them. Live receipts
/// the same day: three personas read `serving: <none>` 59s into a wedge relaunch, 350s
/// after this latch was set. So `has_reconciled()` answers "has a lane EVER come up",
/// never "is one up now" — for the latter, read `ServingSnapshot::ready` live.
///
/// The empty snapshot deliberately stays unchanged — it is still the honest "nothing
/// live" value. What must exist is a SEPARATE signal for whether anyone has looked.
#[test]
fn an_empty_snapshot_cannot_itself_distinguish_startup_from_a_serving_fault() {
    let boot = super::ServingSnapshot::empty();
    assert_eq!(boot.active_model, None);
    assert!(!boot.ready);
    // The point: this value is IDENTICAL whether the daemon has reconciled or not,
    // which is exactly why `has_reconciled()` is a separate signal and not something
    // a reader can infer from the snapshot. If someone later adds a field that makes
    // the two distinguishable here, this assertion documents why they must ALSO keep
    // `has_reconciled()` correct rather than silently replacing it.
    assert_eq!(boot, super::ServingSnapshot::empty());
}

/// what this catches: `mark_first_reconcile` becoming non-idempotent or
/// `has_reconciled` reading the wrong cell. A OnceLock set twice must not panic —
/// the daemon calls it on EVERY reconcile, not just the first.
///
/// NOTE this test can only observe the post-mark state: `FIRST_RECONCILE` is a
/// process-global OnceLock, so once any test in this binary marks it, it stays
/// marked. Asserting `!has_reconciled()` first would be order-dependent — the exact
/// flake shape that made tests pass locally and fail in CI
/// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
#[test]
fn marking_the_first_reconcile_is_idempotent_and_observable() {
    super::mark_first_reconcile();
    assert!(super::has_reconciled());
    super::mark_first_reconcile(); // must not panic
    assert!(super::has_reconciled());
}

/// what this catches: the health heartbeat going back to probe-always. `decode_smoke_ok`
/// is a REAL generation through the live slots, so on a saturated lane it cannot get one,
/// the miss counts as "no decode", and two misses relaunch a lane that is merely busy —
/// which is what killed SWE run v13 (`serving.health {ok:false} x2 -> relaunch`, then every
/// generate refused with `serving: <none>`). Real tokens are the honest liveness evidence;
/// this pins the accessor the heartbeat gates on.
#[test]
fn real_token_delivery_is_recorded_as_liveness_evidence() {
    // Fresh process: nothing observed yet ⇒ None, so the caller falls through and probes
    // (a boot with no traffic must still be verified — absence of evidence is not health).
    // NB: process-global, so only assert the post-record contract if a prior test recorded.
    note_real_decode();
    let since = ms_since_real_decode().expect("a recorded decode must be visible");
    assert!(
        since < 5_000,
        "a decode recorded just now must read as recent, got {since}ms"
    );
}
