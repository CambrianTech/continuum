#![cfg(test)]
use super::*;

// what this catches (M5, 2026-09-26): the start/reboot receipt promised "the web build
// lands in the background" on eight consecutive supervised deploys while no dist was
// configured and none was being built. An unconfigured desktop must be NAMED, with the
// verb that fixes it; the promise is reserved for a configured dist that is still in flight.
#[test]
fn the_desktop_receipt_never_promises_a_build_the_launch_path_is_not_making() {
    let up = super::desktop_receipt(true, false, 8975, None);
    assert!(up.contains("http://127.0.0.1:8975/"), "{up}");
    let in_flight = super::desktop_receipt(false, true, 8975, None);
    assert!(in_flight.contains("lands in the background"), "{in_flight}");
    let unconfigured = super::desktop_receipt(false, false, 8975, None);
    assert!(unconfigured.contains("not configured") && unconfigured.contains("CONTINUUM_UI_DIST"), "{unconfigured}");
    assert!(!unconfigured.contains("lands in the background"), "no promise without a dist: {unconfigured}");
}

// what this catches (2026-09-22, Astra's Windows node): a drain that runs even
// though the teardown after it will be refused. `system/shutdown.rs` deliberately
// leaves the process alive for the CLI to end, and `OpenProcess(25040, TERMINATE)`
// returned NULL/error 5 there — so draining first left a drained core still
// answering ping, with no way to end it. The rule must refuse BEFORE the drain and
// must say that nothing was touched, because an operator who reads "refused" after
// a drain has a different machine in front of them than one who reads it before.
#[test]
fn a_drain_is_refused_when_the_teardown_after_it_would_be() {
    use super::may_begin_shutdown;
    assert!(may_begin_shutdown(Ok(())).is_ok(), "authority present: the stop proceeds");
    let refused = may_begin_shutdown(Err("OpenProcess denied".to_string()))
        .expect_err("no teardown authority must refuse the drain");
    assert!(refused.contains("OpenProcess denied"), "the OS's reason is carried: {refused}");
    assert!(
        refused.contains("No state was touched"),
        "the refusal must say the drain did NOT run: {refused}"
    );
}

// what this catches: a capability probe that refuses on a pid that is merely GONE.
// The pidfile routinely names a core that already exited; if absence read as "no
// authority", every ordinary stop would refuse before doing anything. Only an
// actual permission denial may refuse.
#[cfg(unix)]
#[test]
fn a_pid_that_no_longer_exists_is_not_a_permission_refusal() {
    use super::teardown_authority;
    assert!(
        teardown_authority(std::process::id() as i32).is_ok(),
        "a process this caller owns is tearable down"
    );
    // Max pid + 1 on every supported unix: nothing can be running there.
    assert!(
        teardown_authority(i32::MAX).is_ok(),
        "already gone is nothing to tear down, never a refusal"
    );
}

// what this catches (Astra's review of e275378a2, 2026-09-22): tests that exercise
// helpers instead of the orchestration — "a removed preflight call still passes all
// three". This one is over the decision the stop path ACTUALLY makes, across every
// core it would have to end, and the call itself is now unskippable by construction:
// `request_graceful_stop` takes the `MayDrain` only this function produces, so
// deleting the preflight does not turn a test green, it stops compiling.
#[test]
fn the_drain_is_decided_over_every_core_this_stop_would_have_to_end() {
    use super::preflight_teardown;
    let refuse_25040 = |pid: i32| {
        if pid == 25040 { Err("OpenProcess denied".to_string()) } else { Ok(()) }
    };

    // All endable: drain, nothing to borrow.
    let ok = preflight_teardown(&[10, 11], false, refuse_25040).expect("owned cores drain");
    assert!(ok.borrow_authority_for.is_none());

    // THE ONE THAT MATTERS: the un-endable core is not the first in the list. A
    // preflight that checked only the pidfile's claim — the head of this vector —
    // would pass here and drain a core it cannot end.
    let refused = preflight_teardown(&[10, 25040], false, refuse_25040)
        .err()
        .expect("a core this caller cannot end must refuse the drain");
    assert!(refused.contains("OpenProcess denied"), "{refused}");
    assert!(refused.contains("No state was touched"), "{refused}");

    // Same observation, operator present: carried forward to be borrowed AFTER the
    // drain rather than abandoning the stop.
    let escalated = preflight_teardown(&[10, 25040], true, refuse_25040)
        .expect("an operator present proceeds and borrows the privilege later");
    assert_eq!(escalated.borrow_authority_for, Some(25040));

    // Nothing running at all is not a refusal — there is no core to be unable to end.
    assert!(preflight_teardown(&[], false, |_| Err("x".to_string()))
        .expect("no cores, nothing to preflight")
        .borrow_authority_for
        .is_none());
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
