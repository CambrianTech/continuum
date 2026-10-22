#![cfg(test)]

use super::*;

#[test]
fn overlapping_demand_overrides_release_in_any_order() {
    let cell = Arc::new(AtomicU32::new(0));
    let state = LaneDemandState {
        cell: cell.clone(),
        base: std::sync::Mutex::new(4),
        overrides: std::sync::Mutex::new(Vec::new()),
        next_id: std::sync::atomic::AtomicU64::new(1),
    };
    state.recompute();
    assert_eq!(
        cell.load(Ordering::Relaxed),
        4,
        "base floor before any lease"
    );

    // Overlap: eval quiesce_all (active 0 → floored 1), then a solve's
    // lease_measured_work (active 1) while the first is still held.
    let eval = state.acquire(0);
    assert_eq!(cell.load(Ordering::Relaxed), 1);
    let solve = state.acquire(1);
    assert_eq!(cell.load(Ordering::Relaxed), 1);

    // The killer interleaving: release in ACQUISITION order. The solve still
    // holds its lease, so demand must STAY at the measurement value...
    state.release(eval);
    assert_eq!(
        cell.load(Ordering::Relaxed),
        1,
        "first release must not restore the base over a still-held lease"
    );
    // ...and only the LAST release restores the base floor.
    state.release(solve);
    assert_eq!(
        cell.load(Ordering::Relaxed),
        4,
        "last release restores the base — never a stale override"
    );

    // Idempotent: releasing an unknown/stale id changes nothing.
    state.release(solve);
    assert_eq!(cell.load(Ordering::Relaxed), 4);

    // Concurrent measurements with different needs: never starve the larger one.
    let a = state.acquire(2);
    let b = state.acquire(1);
    assert_eq!(cell.load(Ordering::Relaxed), 2, "max of held overrides");
    state.release(a);
    assert_eq!(cell.load(Ordering::Relaxed), 1);
    state.release(b);
    assert_eq!(cell.load(Ordering::Relaxed), 4);
}

// what this catches (Cormac's review of #4197, 2026-09-18): a burst of leased-in calls
// on ONE tick must not move the lane count. `--parallel` is a launch arg — a lane-count
// change is a new llama-server and every in-flight generation dies with the old one —
// so a raw per-tick peak would relaunch the seat and kill the very calls that raised
// the count, then downshift, then repeat: the 718 flap through a new input. The
// planner may act only on a load sustained for DOWNSHIFT_SUSTAINED_TICKS.
#[test]
fn a_burst_of_leased_in_calls_on_one_tick_does_not_move_the_lane_count() {
    let w = DOWNSHIFT_SUSTAINED_TICKS as usize;
    assert!(w >= 2, "the guard only means something with a window");
    assert_eq!(
        sustained_leased_in(1, 3),
        0,
        "a fresh gauge never relaunches"
    );
    assert_eq!(
        sustained_leased_in(3, 0),
        0,
        "a burst that vanished is not demand"
    );
    assert_eq!(sustained_leased_in(3, 0), 0, "two ticks is still a burst");
    assert_eq!(
        sustained_leased_in(3, 3),
        3,
        "three consecutive ticks: a load"
    );
    assert_eq!(
        sustained_leased_in(3, 3),
        3,
        "the window is the LAST w ticks: [0, 3, 3, 3] keeps [3, 3, 3]"
    );
    assert_eq!(
        sustained_leased_in(3, 2),
        2,
        "the minimum, not the max: a dip is honored"
    );
    assert_eq!(
        sustained_leased_in(3, 1),
        1,
        "a falling load falls at once: [3, 3, 3, 1] keeps [3, 3, 1]"
    );
}
