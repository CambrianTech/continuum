//! Snapshot regression fixtures.
#![cfg(test)]
use super::*;
/// A pending future stays pending on its first poll: the event it waits for has not happened.
async fn assert_pending<F: std::future::Future + Unpin>(future: &mut F, why: &str) {
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(std::pin::Pin::new(&mut *future), cx).is_pending(), "{why}");
        std::task::Poll::Ready(())
    })
    .await;
}

fn live(model: &str) -> ServingSnapshot {
    let mut snap = ServingSnapshot::empty();
    snap.ready = true;
    snap.active_model = Some(model.into());
    snap
}

// what this catches (rule 10): a reader that arrives before the daemon installs its state
// polling the `OnceLock` on a timer instead of awaiting the install. The wait is pending until the
// install signals, and resolves on that signal; nothing here sleeps or reads a clock.
#[tokio::test]
async fn a_reader_before_the_install_awaits_the_install_itself() {
    use super::super::snapshot::installed_receiver;
    let state = std::sync::OnceLock::new();
    let installed = tokio::sync::Notify::new();
    let (tx, rx) = tokio::sync::watch::channel(live("installed-model"));
    let mut waiting = Box::pin(installed_receiver(&state, &installed));
    assert_pending(&mut waiting, "nothing is installed yet").await;
    assert!(state.set(rx).is_ok());
    installed.notify_waiters();
    let received = waiting.await;
    assert_eq!(received.borrow().active_model.as_deref(), Some("installed-model"), "the installed receiver, delivered by the signal");
    drop(tx);
    // Already installed: answered at once, no signal needed.
    assert!(installed_receiver(&state, &installed).await.borrow().is_live());
}

// what this catches: the live wait resolving on a publish that is not live (ready without a
// model), or parking forever when the daemon drops its sender. It resolves on the daemon's first
// live publish, and a dropped sender is the typed `DaemonGone`.
#[tokio::test]
async fn the_live_wait_resolves_on_the_first_live_publish_and_names_a_gone_daemon() {
    use super::super::snapshot::{live_snapshot, ServingNotReady};
    let (tx, rx) = tokio::sync::watch::channel(ServingSnapshot::empty());
    let mut waiting = Box::pin(live_snapshot(rx));
    assert_pending(&mut waiting, "an empty snapshot is not live").await;
    let mut ready_without_a_model = ServingSnapshot::empty();
    ready_without_a_model.ready = true;
    tx.send_replace(ready_without_a_model);
    assert_pending(&mut waiting, "ready without a model is not live").await;
    tx.send_replace(live("published-model"));
    assert_eq!(waiting.await.map(|s| s.active_model), Ok(Some("published-model".into())));

    let (gone, rx) = tokio::sync::watch::channel(ServingSnapshot::empty());
    drop(gone);
    assert_eq!(live_snapshot(rx).await, Err(ServingNotReady::DaemonGone));
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

// what this catches (IntelMac, 2026-11-03 07:32Z): the share divided the slot's `n_ctx`, which
// the fork caps at the trained window under `--kv-unified` (a 65,536 pool reads 32,768), so
// each of 2 minds was published 16,384 instead of 32,768. The pool is `allocated_tokens`.
#[tokio::test]
async fn the_pool_is_the_engines_allocated_tokens_never_the_capped_slot_window() {
    let app = axum::Router::new().route(
        "/props",
        axum::routing::get(|| async {
            axum::Json(serde_json::json!({
                "default_generation_settings": {"n_ctx": 32_768},
                "total_slots": 2,
                "context_geometry": {"allocated_tokens": 65_536, "max_tokens_per_sequence": 32_768, "sequence_capacity": 2}
            }))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let process = LlamaServerProcess::with_root(root);
    assert_eq!(process.served_context_window().await.expect("test: n_ctx"), 32_768);
    assert_eq!(process.served_geometry().await.expect("test: geometry").pool_tokens(), Some(65_536));
    server.abort();
}
