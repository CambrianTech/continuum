//! The ONE-ACT dyno's launch reproductions (dyno 1, and dyno 5's launch half): the daemon's own
//! pool sizing (`launch::launch_pool`, the one arithmetic `engine_pool` launches with) driven by
//! what the M5 recorded on 2026-11-04 (`continuum_engine::capacity::dyno::recorded`).
#![cfg(test)]

use crate::capacity::dyno::expected::MissingSeam;
use crate::capacity::dyno::recorded::*;

fn day() -> Recorded {
    Recorded::load().expect("test: the recorded 2026-11-04 data parses")
}

// what this catches (the M5, 2026-11-04 cold boot: board usable 22,785,477,836 B, credited 0,
// with the previous engine's 20,419,565,568 B still wired; the engine launched -c 125,640, and
// 45.6 GB was reclaimable once it released): a launch sized from a reading taken while the
// previous engine's attribution stands. `launch_pool` takes the plan's budget as a bare byte
// count, so nothing can tell that reading from a settled one; the plan made from it carried the
// pool the engine launched with, and `launch_pool` launches it. The design: no launch until the
// resource authority OBSERVED the release (Settled), and the pool is the reserved fit then.
#[test]
#[ignore = "ONE-ACT slice 3: no launch sizes from a reading taken while the previous engine's attribution stands"]
fn no_launch_sizes_from_a_reading_taken_while_the_old_engine_is_wired() {
    let launch_pool = super::super::launch::launch_pool;
    let day = day();
    let footprint = day.qwen36_on_m5().expect("test: the recorded weights are nonzero");
    let (cold_boot, sequences) = (day.cold_boot.value, day.m5_sequences.value);
    let reading = cold_boot.reading;
    let PreviousEngine::StillAttributed { wired_bytes } = reading.previous_engine else {
        panic!("test: the recorded cold boot read the old engine still wired");
    };
    // Precondition: the bytes the reading lacked are the predecessor's, and they matter to the fit.
    assert!(
        footprint.window_within(reading.usable_bytes + wired_bytes, sequences)
            > footprint.window_within(reading.usable_bytes, sequences),
        "test: the still-wired engine's bytes shrink the fit"
    );
    let pool = launch_pool(
        Some(&footprint),
        reading.usable_bytes,
        cold_boot.launched_pool,
        1,
        sequences,
        day.qwen36_trained_window.value,
        super::super::launch::Seated { need: 0, proven: None },
    );
    assert_eq!(pool, u64::from(cold_boot.launched_pool), "test: today's launch, reproduced (-c 125,640)");
    assert_eq!(
        pool, 0,
        "a launch of {pool} tokens was sized from a reading (usable {} B, credited {} B) taken while the \
         previous engine's {wired_bytes} B were still attributed; it must wait for the observed release",
        reading.usable_bytes, reading.credited_bytes,
    );
}

// what this catches (the M5, 2026-11-04: a relaunch at -c 33,040 onto a 33,280-cell pool beside
// residents whose pages were 35,842 and 33,542 tokens, every restore refused): a launch sized below
// the residents' own pages. Not drivable yet: the missing seam is named (the restore half is
// `turn_admission/tests/restore_refusal.rs`).
#[test]
#[ignore = "ONE-ACT slice 3: the launch never sizes below the residents' pages"]
fn the_launch_never_sizes_below_the_residents_pages() {
    let oversize = day().oversize.value;
    let [first, second] = oversize.pages;
    panic!(
        "{} (launched -c {}, pool {} cells, largest resident page {} tokens)",
        MissingSeam::LaunchKnowsResidentPages.describe(),
        oversize.launch_ctx,
        oversize.pool_cells,
        first.max(second),
    );
}
