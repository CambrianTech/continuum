#![cfg(test)]

use super::*;
use crate::inference::llama_server::LaneAdmission;

fn refusal(a: LaneAdmission) -> String {
    match a {
        LaneAdmission::Refuse(r) => r,
        LaneAdmission::Admit => panic!("expected a refusal"),
    }
}
fn is_admit(a: &LaneAdmission) -> bool {
    matches!(a, LaneAdmission::Admit)
}

// what this catches: THE bug. Three vision sidecars co-existed on one GPU because
// the only question asked before spawning was `first_free_port` — is a port free.
// A second engine that does not fit beside the live lane must be refused. If this
// ever admits, ports are deciding device capacity again and the box goes back to
// 3.1-8.6 tok/s per engine.
#[test]
fn a_second_engine_that_does_not_fit_is_refused() {
    // 24 GB budget, a 19 GB live lane already up, another 19 GB engine asked for.
    let d = lane_admission_decision(24_000_000_000, 19_000_000_000, 19_000_000_000, 0, 0);
    let why = refusal(d);
    for term in ["19.0 GB", "24.0 GB", "5.0 GB"] {
        assert!(
            why.contains(term),
            "the refusal must state its terms: {why}"
        );
    }
    assert!(
        why.contains("time-slice"),
        "and must say WHY co-residence is not extra capacity: {why}"
    );
}

// what this catches: the sign of the incumbent. A pin SWAPS, so `pin_fit_decision`
// credits the incumbent back into the budget; an ephemeral lane JOINS, so it must
// be spent. Flip this and a full device reads as empty — which is precisely the
// second-engine admission this gate exists to stop.
#[test]
fn the_live_lane_is_spent_not_credited() {
    let budget = 24_000_000_000;
    let want = 19_000_000_000;
    // Nothing resident: the same engine fits.
    assert!(
        is_admit(&lane_admission_decision(budget, want, 0, 0, 0)),
        "with an empty device this engine fits"
    );
    // Same engine, same budget, live lane up: it must NOT fit. If the incumbent
    // were credited back (the pin arithmetic) this would admit.
    assert!(
        !is_admit(&lane_admission_decision(budget, want, 19_000_000_000, 0, 0)),
        "co-residence spends the incumbent — crediting it back is the pin's rule, \
                 not this one"
    );
}

// what this catches: siblings must weigh. The measured failure was the THIRD
// sidecar — each one individually small enough to look affordable, the set of
// them not. An admission that ignores existing ephemeral lanes admits N engines
// one at a time.
#[test]
fn ephemeral_siblings_weigh_on_the_decision() {
    let budget = 24_000_000_000;
    let want = 6_000_000_000;
    assert!(
        is_admit(&lane_admission_decision(budget, want, 6_000_000_000, 0, 0)),
        "one sibling's worth of room is there"
    );
    // Two 6 GB siblings already up beside a 6 GB live lane: 18 committed, 6 free —
    // still exactly affordable.
    assert!(
        is_admit(&lane_admission_decision(
            budget,
            want,
            6_000_000_000,
            12_000_000_000,
            2
        )),
        "exactly-affordable must admit — this gate refuses overflow, not company"
    );
    // A third sibling tips it over.
    let why = refusal(lane_admission_decision(
        budget,
        want,
        6_000_000_000,
        18_000_000_000,
        3,
    ));
    assert!(
        why.contains("3 ephemeral sibling(s)"),
        "the refusal names how many were already there: {why}"
    );
}

// what this catches (2026-09-15 00:17Z): the vision sidecar was admitted on a live
// free-RAM read taken inside the main lane's relaunch window (weights + KV
// momentarily unwired) and the box swapped 26 GB. The PLANNED headroom after the
// main lane's working set — full weights, KV at the served window × lanes, the
// compute reserve, the prompt cache, the OS floor — must say NO on a 64 GB box
// serving a 21 GB model over 6 lanes at 59k, whatever the live read says.
#[test]
fn the_sidecar_headroom_is_planned_not_a_relaunch_window_free_read() {
    let mut model = fake_model("ornith-ai/Ornith-1.5-35B-A3B-GGUF");
    model.weights_bytes = Some(21 * 1024 * 1024 * 1024);
    model.context_window = 262_144;
    let target = ServingTarget {
        host_prompt_cache_mib: 2505,
        model,
        context_window: 59_392,
        lanes: 6,
        adapters: Vec::new(),
        placement: crate::inference::llama_server::main_lane_placement(),
        expert_placement: None,
        resident_override: None,
        vision_sidecar: false,
    };
    let physical = 64 * 1024 * 1024 * 1024;
    let headroom = sidecar_planned_headroom_bytes(&target, 59_392, physical);
    let need = 22_131_161_622; // the 27B sidecar's measured need on 2026-09-15
    assert!(headroom < need, "planned headroom {headroom} must refuse a {need}-byte sidecar beside a 21 GB main lane on 64 GB");
    // Not a blanket refusal: the same lane on a 256 GB box leaves the sidecar room.
    let big = sidecar_planned_headroom_bytes(&target, 59_392, 256 * 1024 * 1024 * 1024);
    assert!(
        big >= need,
        "on 256 GB the planned headroom {big} admits the sidecar"
    );
    // A row without a footprint (no stamped weights, no resolvable GGUF) yields ZERO
    // headroom — the live read alone never admits.
    let mut blind = target.model.clone();
    blind.weights_bytes = None;
    let blind_target = ServingTarget {
        model: blind,
        ..target.clone()
    };
    assert_eq!(
        sidecar_planned_headroom_bytes(&blind_target, 59_392, physical),
        0
    );
}

// what this catches: the authority going UNWIRED — which is not hypothetical, it
// is the whole reason this gate had to be written. `EphemeralServingLane::spawn`
// carried a doc comment for months saying budget-gating "is the ResourceGovernor's
// concern (#56)", and `ResourceGovernor` was never built: all eight of its mentions
// in the tree are doc comments promising a future owner. So the decision was
// delegated to nobody and made by `first_free_port`.
//
// A source assertion because installation happens inside the daemon's async
// `initialize`, and the seam it writes to is a process-wide `OnceLock` a unit test
// cannot un-set between runs. Same shape as the `source_hygiene` ratchets: cheap,
// and it fails the moment someone deletes the call.
#[test]
fn the_serving_daemon_installs_the_admission_authority() {
    let src = production_source();
    assert!(
                src.contains("install_lane_admission("),
                "serving_daemon must INSTALL the device-budget authority — without it                  EphemeralServingLane::spawn admits every engine on a free-port check,                  which is how three sidecars once shared one GPU"
            );
    assert!(
                src.contains("fn lane_admission_authority("),
                "and must build it from the same live budget the planner uses, so the gate                  and the planner cannot disagree about what fits"
            );
}

// what this catches: an over-committed device must not wrap. `saturating_sub` on
// headroom is what keeps a device already past its budget from reporting a huge
// free number and admitting everything — the arithmetic failure mode that turns a
// guard into an accelerator.
#[test]
fn an_already_overcommitted_device_refuses_rather_than_wrapping() {
    let why = refusal(lane_admission_decision(
        8_000_000_000,
        1_000_000_000,
        20_000_000_000,
        0,
        0,
    ));
    assert!(
        why.contains("headroom 0.0 GB"),
        "past the budget, headroom floors at zero: {why}"
    );
}
