#![cfg(test)]
use super::*;

// what this catches (M5, 2026-09-26): the start/reboot receipt promised "the web build
// lands in the background" on eight consecutive supervised deploys while no dist was
// configured and none was being built. An unconfigured desktop must be NAMED, with the
// verb that fixes it; the promise is reserved for a configured dist that is still in flight.
#[test]
fn the_desktop_receipt_never_promises_a_build_the_launch_path_is_not_making() {
    let up = super::desktop_receipt(true, false, 8975);
    assert!(up.contains("http://127.0.0.1:8975/"), "{up}");
    let in_flight = super::desktop_receipt(false, true, 8975);
    assert!(in_flight.contains("lands in the background"), "{in_flight}");
    let unconfigured = super::desktop_receipt(false, false, 8975);
    assert!(unconfigured.contains("not configured") && unconfigured.contains("CONTINUUM_UI_DIST"), "{unconfigured}");
    assert!(!unconfigured.contains("lands in the background"), "no promise without a dist: {unconfigured}");
}

// what this catches (Astra's second objection, 2026-09-22): deciding a kill failed
// by probing liveness on the next line. SIGTERM and taskkill are asynchronous, so a
// healthy core is still in the table immediately after — an immediate probe would
// have reported "kill refused" on every successful deploy. The decider is the
// bounded deadline: a pid still present when it expires, and only then.
#[tokio::test]
async fn only_outliving_the_deadline_is_evidence_that_a_kill_did_not_take() {
    use super::exited_within;
    use std::time::Duration;
    // Our own pid cannot exit, so it stands in for a core that refused to go.
    let me = std::process::id() as i32;
    let refused = exited_within(me, Duration::from_millis(300))
        .await
        .expect_err("a pid still alive at the deadline is a failed teardown");
    assert!(refused.contains(&me.to_string()), "the surviving pid is named: {refused}");
    assert!(
        refused.contains("pidfile is left in place"),
        "the claim must survive the process it names: {refused}"
    );
    // And a pid that is gone returns without spending the deadline.
    let started = std::time::Instant::now();
    assert!(exited_within(i32::MAX, Duration::from_secs(30)).await.is_ok());
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "an already-dead pid must not wait out the deadline"
    );
}
