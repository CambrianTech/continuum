#![cfg(test)]

use super::*;
use crate::inference::llama_server::ServingSnapshot;

fn serving(active: &str) -> ServingSnapshot {
    ServingSnapshot {
        active_model: Some(active.to_string()),
        ready: true,
        ..ServingSnapshot::empty()
    }
}

// what this catches: the WAIT-vs-REFUSE split collapsing. This is the decision that
// separates a turn we can still save from one we cannot, and the two branches read
// almost identically at the call site — so the predicate itself is pinned here.
// Live 2026-08-07: the lane flipped not-ready at +374s and republished ready at
// +436s; three citizens' turns landed inside that 62s window and were refused 9
// seconds before the lane returned. Every case below except the last is waitable.
#[test]
fn only_a_resident_ready_other_model_is_worth_refusing_immediately() {
    // boot / teardown — nothing resident: wait, the daemon is mid-flight
    assert!(!settled_on_another_model(&ServingSnapshot::empty()));
    // a lane is coming up (model named, decode not yet verified): wait
    let warming = ServingSnapshot {
        active_model: Some("m".into()),
        ready: false,
        ..ServingSnapshot::empty()
    };
    assert!(!settled_on_another_model(&warming));
    // the daemon has SETTLED on something else — waiting cannot change that
    assert!(settled_on_another_model(&serving("other")));
}

// what this catches: `snapshot_guarantees` drifting from the guard it replaced. The
// gateway answers every request as its ONE resident model whatever the request says,
// so a false positive here is a silently wrong brain — the failure this whole guard
// exists to prevent. The vision arm is a real guarantee (the daemon verifies the
// sidecar's /props before publishing `vision_ready`), so it must keep passing.
#[test]
fn a_guarantee_needs_ready_plus_this_exact_model_on_either_lane() {
    assert!(snapshot_guarantees(&serving("m"), "m"));
    assert!(!snapshot_guarantees(&serving("other"), "m"));
    assert!(!snapshot_guarantees(&ServingSnapshot::empty(), "m"));
    // named but not decode-ready is NOT a guarantee
    let warming = ServingSnapshot {
        active_model: Some("m".into()),
        ready: false,
        ..ServingSnapshot::empty()
    };
    assert!(!snapshot_guarantees(&warming, "m"));
    // the #106 vision sidecar is its own verified residency
    let with_vision = ServingSnapshot {
        vision_ready: true,
        vision_model: Some("vl".into()),
        ..serving("m")
    };
    assert!(snapshot_guarantees(&with_vision, "vl"));
    // ... but only when the daemon actually verified it
    let unverified = ServingSnapshot {
        vision_ready: false,
        ..with_vision
    };
    assert!(!snapshot_guarantees(&unverified, "vl"));
}

// what this catches: the startup case regressing to the fault sentence. Before the
// daemon's first reconcile every reader borrows the boot placeholder, so a refusal
// here says nothing about the lane's health — 116 false alarms over 3 days came
// from printing the fault wording during ordinary core startup (#350).
#[test]
fn before_the_first_reconcile_it_names_startup_and_invites_a_retry() {
    let msg = unguaranteed_model_refusal("gw", "m", &ServingSnapshot::empty(), false);
    assert!(msg.contains("STARTUP"), "{msg}");
    assert!(msg.contains("retry"), "{msg}");
    assert!(
        !msg.contains("is not the active served model"),
        "startup must not borrow the mismatch wording: {msg}"
    );
}

// what this catches: THE REGRESSION THIS TEST EXISTS FOR. An empty snapshot AFTER
// the first reconcile is still usually a transition, not a fault: the daemon
// republishes `empty()` on every teardown (no plan, re-home, #175 wedge self-heal).
// Live 2026-08-07 three citizens took the fault sentence 59s into a wedge-triggered
// relaunch that completed normally — the latch said "reconciled", so the earlier
// two-way split routed a healthy transition into the fault branch.
#[test]
fn a_reconciled_but_empty_snapshot_reads_as_a_transition_not_a_fault() {
    let msg = unguaranteed_model_refusal("gw", "m", &ServingSnapshot::empty(), true);
    assert!(msg.contains("TRANSITION"), "{msg}");
    assert!(msg.contains("retry"), "{msg}");
    assert!(
        !msg.contains("is not the active served model"),
        "a lane between relaunches is not a model mismatch: {msg}"
    );
}

// what this catches: the genuine mismatch losing its loudness. A DIFFERENT model
// being resident is the one case retrying cannot fix, so it must keep naming both
// models — softening this back into "just retry" would hide a real misroute.
#[test]
fn a_different_resident_model_stays_a_named_mismatch() {
    let msg = unguaranteed_model_refusal("gw", "wanted", &serving("resident"), true);
    assert!(msg.contains("is not the active served model"), "{msg}");
    assert!(msg.contains("wanted") && msg.contains("resident"), "{msg}");
    assert!(
        !msg.contains("TRANSITION") && !msg.contains("STARTUP"),
        "a real mismatch must not be excused as a transition: {msg}"
    );
}
