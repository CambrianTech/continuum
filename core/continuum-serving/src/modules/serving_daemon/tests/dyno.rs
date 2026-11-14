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

// Dyno 9: boot during memory settle (the M5, 2026-11-13, card 8569b03d). 19:36Z: the boot plan read
// usable 16 GB while the stopped engine's memory was still being returned, launched -c 2045, and 15 s
// later planned 2 x 134,690 at 25 GB; nothing applied it. The four residents' full-detail
// requirements were ~68-110k each. Driven through the daemon's own pool sizing (`launch_pool`), the
// settle gate (`settle::launch_gate`) and the floor-sized rule (`floor_sized::floor_sized`).
mod boot_during_memory_settle {
    use super::super::super::floor_sized::floor_sized;
    use super::super::super::launch::{launch_pool, Seated};
    use super::super::super::settle::{launch_gate, HostSettle, LaunchGate};
    use crate::inference::planning::serving_plan::ModelFootprint;
    use crate::inference::planning::MIN_SERVE_CTX;
    use continuum_host::host_memory::HostMemoryReading;

    const GB: u64 = 1_000_000_000;
    const GIB: u64 = 1 << 30;
    /// Qwen3.6-35B-A3B's trained window, as the M5 serves it.
    const TRAINED: u32 = 262_144;
    /// The remembered residents' held needs (Aiko, Aris, Atlas, Benchy: ~68-110k).
    const NEEDS: [u64; 4] = [68_000, 82_000, 95_000, 110_000];
    /// Four residents and the scratch sequence.
    const SEQUENCES: u32 = 5;
    /// The boot plan's reading at t0, and the plan's 15 s later.
    const SETTLING: u64 = 16 * GB;
    const SETTLED: u64 = 25 * GB;
    /// The plan at the settled reading: 2 x 134,690.
    const GROWN_WINDOW: u32 = 134_690;

    fn m5() -> ModelFootprint {
        let mut fp = crate::inference::planning::serving_plan::footprint::calibrated_from_parts("ggml-org/Qwen3.6-35B-A3B-GGUF", 20_690_000_000, TRAINED, true, None)
            .expect("test: a footprint from the weights");
        fp.kv_per_token = 8_317;
        fp,
    }

    fn compressed(bytes: u64) -> HostMemoryReading {
        HostMemoryReading { free_bytes: 0, reclaimable_bytes: 0, compressed_bytes: bytes, swapped_in_bytes: 0, swapped_out_bytes: 0, free_target_bytes: 0 }
    }

    // what this catches: the recorded boot, reproduced. The boot's demand held no resident (need 0),
    // the plan at a reading below the weights named one MIN_SERVE_CTX lane, and the launch took it:
    // -c 2045, a pool no remembered mind's need fits in.
    #[test]
    fn before_a_boot_with_no_residents_launches_at_the_floor() {
        let pool = launch_pool(Some(&m5()), SETTLING, MIN_SERVE_CTX, 1, SEQUENCES, TRAINED, Seated { need: 0, proven: None });
        assert_eq!(pool, u64::from(MIN_SERVE_CTX), "today's launch, reproduced (-c 2045)");
        assert!(NEEDS.iter().all(|&need| need > pool), "every mind dark: no need fits the pool");
    }

    // what this catches: the fix. With the remembered residents in the demand, a launch below their
    // need waits while the compressor shrinks; where this model ran before, the gap reading launches
    // at the proven pool (every single need held); once the host settles the grown plan launches
    // with every need inside it; and an engine already at the floor is retired once for it.
    #[test]
    fn after_the_boot_waits_for_the_settle_and_launches_what_the_minds_need() {
        let fp = m5();
        let need: u64 = NEEDS.iter().sum();
        let returning = HostSettle::default().observe(Some(&compressed(34 * GIB))).observe(Some(&compressed(30 * GIB)));
        let at_t0 = launch_pool(Some(&fp), SETTLING, MIN_SERVE_CTX, 1, SEQUENCES, TRAINED, Seated { need, proven: None });
        assert!(matches!(launch_gate(returning, at_t0, need), LaunchGate::AwaitSettle { .. }), "no launch below the need while memory comes back: {at_t0}");

        let proven = 247_040;
        let at_t0_proven = launch_pool(Some(&fp), SETTLING, MIN_SERVE_CTX, 1, SEQUENCES, TRAINED, Seated { need, proven: Some(proven) });
        assert_eq!(at_t0_proven, proven, "a gap reading launches at the pool this model ran at, never the transient fit");
        assert!(NEEDS.iter().all(|&n| n <= at_t0_proven), "every remembered need fits the proven pool");

        let settled = returning.observe(Some(&compressed(30 * GIB)));
        let at_t15 = launch_pool(Some(&fp), SETTLED, GROWN_WINDOW, 2, SEQUENCES, TRAINED, Seated { need, proven: None });
        assert_eq!(launch_gate(settled, at_t15, need), LaunchGate::Launch, "a settled host launches");
        assert!(at_t15 >= 2 * u64::from(GROWN_WINDOW), "the settled plan's pool: {at_t15}");
        assert!(NEEDS.iter().all(|&n| n <= at_t15), "every remembered need fits the launch: {at_t15}");

        let applied = floor_sized(MIN_SERVE_CTX, &NEEDS, at_t15).expect("an engine already at the floor is floor-sized");
        assert_eq!((applied.held_now, applied.held_then), (0, NEEDS.len()), "the grown plan is applied: none held, then all");
    }
}

// Dyno 10: a leased-in mind (a mind this seat serves for another node). Her prompt reaches this seat;
// her requirement does not: the generate wire carries no `LaneRequirement`, so the seat sizes her
// from a prompt size (a measure a window bounds), and the grid seam that would carry it is Codex's.
#[test]
#[ignore = "card 8569b03d step 3: a leased-in mind's requirement crosses the grid seam (Codex owns grid)"]
fn a_leased_in_mind_is_sized_from_her_requirement_never_a_prompt_size() {
    panic!("{}", MissingSeam::LeasedInRequirement.describe());
}
