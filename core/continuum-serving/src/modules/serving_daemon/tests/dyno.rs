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
        fp
    }

    fn compressed(bytes: u64) -> HostMemoryReading {
        HostMemoryReading { free_bytes: 0, reclaimable_bytes: 0, compressed_bytes: bytes, swapped_in_bytes: 0, swapped_out_bytes: 0, free_target_bytes: 0, pressure: None }
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

// Dyno 12: the pool fits the host (the M5, 2026-11-14 00:57Z, canary 7a29474a9). The boot read 52,122
// MiB available, 1,344 MiB compressed and 5,915 MiB swapped (`memory.pressure` at 1794617812455);
// the stable-arm plan named 3 x 207,691 over a 46,011,570,176 B budget (`serving.launch.plan` at
// 1794617823874), and the engine launched at `-c 623070` (124,614 x 5) beside `--cache-ram 8717`.
// The measured cost at that pool (`state/lane-footprint.json`): 15,163 B per token. Seventeen
// minutes later the kernel read pressure with a 3,915 MiB deficit. Driven through the daemon's own
// demand (`launch_pool`), the minds' need (`host_fit::need_with_growth`) and host fit
// (`host_fit::pool_fit`).
mod the_pool_fits_the_host {
    use super::super::super::host_fit::{need_with_growth, pool_fit, Bound, HostRoom, MindClaim, PoolFit, Rooms};
    use super::super::super::launch::{launch_pool, Seated};
    use crate::inference::planning::serving_plan::ModelFootprint;
    use continuum_host::host_memory::{HostMemoryReading, OsMemoryPressure};

    const MIB: u64 = 1 << 20;
    const TRAINED: u32 = 262_144;
    const SEQUENCES: u32 = 5;
    /// The four residents' held needs at the launch (`serving.launch.await_settle`, need 382,706).
    const NEED: u64 = 382_706;
    const CACHE_MIB: u64 = 8_717;

    fn m5_measured() -> ModelFootprint {
        let mut fp = crate::inference::planning::serving_plan::footprint::calibrated_from_parts("ggml-org/Qwen3.6-35B-A3B-GGUF", 20_690_000_000, TRAINED, true, None)
            .expect("test: a footprint from the weights");
        fp.kv_per_token = 15_163;
        // Fixture: the Devstral calibration at this KV rate, the compute rate the recorded boot planned with.
        fp.compute = crate::inference::planning::serving_plan::ComputeRate::devstral_calibration(fp.kv_per_token);
        fp
    }

    fn boot() -> (HostMemoryReading, u64) {
        let reading = HostMemoryReading {
            free_bytes: 52_122 * MIB,
            reclaimable_bytes: 0,
            compressed_bytes: 1_344 * MIB,
            swapped_in_bytes: 0,
            swapped_out_bytes: 0,
            free_target_bytes: 62 * MIB,
            pressure: Some(OsMemoryPressure::Normal),
        };
        (reading, 5_915 * MIB)
    }

    /// What the engine holds at `pool` by the plan's own cost (weights, the prompt cache, each
    /// sequence's compute floor and the measured per-token cost with its compute rate).
    fn engine_bytes(fp: &ModelFootprint, pool: u64) -> u64 {
        let per_token = fp.kv_per_token + fp.compute.bytes_per_token().map_or(0, |rate| rate);
        fp.weights_bytes + CACHE_MIB * MIB + u64::from(SEQUENCES) * fp.compute_buffer_per_lane() + pool * per_token
    }

    fn demand(fp: &ModelFootprint) -> u64 {
        launch_pool(Some(fp), 25_592_004_608, 207_691, 3, SEQUENCES, TRAINED, Seated { need: NEED, proven: None })
    }

    // what this catches: the recorded launch, reproduced. The demand is the plan's 3 x 207,691,
    // the engine's own cost at it exceeds what the host could give without displacing anyone, and
    // nothing in the launch read the host.
    #[test]
    fn before_the_launch_takes_the_plan_whole() {
        let fp = m5_measured();
        let pool = demand(&fp);
        assert_eq!(pool, 3 * 207_691, "the plan's pool, launched whole");
        assert_eq!(pool / u64::from(SEQUENCES) * u64::from(SEQUENCES), 623_070, "-c 623070");
        let (reading, swap) = boot();
        let room = HostRoom::of(&reading, swap, 0);
        assert!(engine_bytes(&fp, pool) > room.bytes(), "the engine at 623k ({} B) is past the host's room ({} B)", engine_bytes(&fp, pool), room.bytes());
    }

    // what this catches: the fix. The pool is the residents' need with the grants' growth margin,
    // never the room: 4 x need x 1.25 plus one least window for the scratch sequence, 480,430 tokens
    // against a room that holds 566,990, so the launch frees 142,643 tokens of the recorded 623,073
    // (2.16 GB at the measured 15,163 B/token) for the core, the embedder and the OS. When the need
    // exceeds the room the room bounds it; a replaced engine's bytes count; no reading launches the
    // need.
    #[test]
    fn after_the_pool_is_the_need_and_the_room_only_its_ceiling() {
        let fp = m5_measured();
        let (reading, swap) = boot();
        let room = HostRoom::of(&reading, swap, 0);
        let host = |room| Rooms { host: Some(room), device: None };
        // The probe recorded the four held needs' sum (382,706); split evenly here.
        // Fixture: the growth the grants used on that boot, 5/4 (the headroom then live at 958ea3056).
        let growth = crate::inference::planning::window_allocator::Growth::Measured { tail: crate::inference::planning::window_allocator::test_tokens(5), typical: crate::inference::planning::window_allocator::test_tokens(4) };
        let claims = [95_676, 95_676, 95_676, 95_678].map(|need| MindClaim::Held { need, growth });
        let need = need_with_growth(&claims, SEQUENCES, TRAINED).expect("test: four residents claim");
        assert_eq!(need, 478_382 + u64::from(crate::inference::planning::MIN_SERVE_CTX), "the grants' 1.25 over 382,706, plus the scratch sequence");
        let fit = pool_fit(need, Some(&fp), host(room), CACHE_MIB * MIB, SEQUENCES, TRAINED);
        assert_eq!(fit, PoolFit::Fits { demand: need, fit: 566_990 }, "the room would hold 566,990: never filled");
        let pool = fit.pool(SEQUENCES);
        assert_eq!(pool, 480_430);
        let freed = demand(&fp) - pool;
        assert_eq!(freed, 142_643);
        assert_eq!(freed * fp.kv_per_token, 2_162_895_809, "bytes the launch no longer holds at the measured per-token cost");
        assert_eq!(room.bytes() - engine_bytes(&fp, pool), 1_640_613_306, "left beside the engine for the core, the embedder and the OS");

        let starved = HostRoom::of(&HostMemoryReading { free_bytes: 32_000 * MIB, ..reading }, swap, 0);
        let bounded = pool_fit(need, Some(&fp), host(starved), CACHE_MIB * MIB, SEQUENCES, TRAINED);
        assert!(matches!(bounded, PoolFit::Bounded { by: Bound::Host(_), .. }), "need past the room: the room bounds it: {bounded:?}");
        // The engine it replaces returns its measured anonymous bytes (24,969,922,712 B).
        let replacing = HostRoom::of(&HostMemoryReading { free_bytes: 32_000 * MIB, ..reading }, swap, 24_969_922_712);
        assert!(matches!(pool_fit(need, Some(&fp), host(replacing), CACHE_MIB * MIB, SEQUENCES, TRAINED), PoolFit::Fits { .. }), "a replaced engine's bytes come back");
        assert_eq!(pool_fit(need, Some(&fp), Rooms { host: None, device: None }, CACHE_MIB * MIB, SEQUENCES, TRAINED).pool(SEQUENCES), need, "no reading: never a dark node");
    }
}

// Dyno 14: the boot counts the device room (the M5, 2026-11-14 03:35Z, canary 958ea3056). The boot
// launch planned on `OwnCredit::ToApply` (`serving.plan.branch` at 1794627093748: stable, usable 0,
// credited 0) and read no device room, launched -c 661410, and the embed lane's 186,908,672 B leases
// were refused at 0 available 12 times in 26 minutes. The device is 55,662,788,608 B
// (`serving.vram_ceiling_prior.seeded`); the engine's measured cost at that pool is 13,520 B/token
// (`state/lane-footprint.json` at 661,500). Driven through the computed room (the ledger's
// `room_beside_others`, pinned in `resources::ledger::tests::the_embed_floor_survives_a_boot_sizing`)
// and `host_fit::pool_fit`.
mod boot_counts_the_device_room {
    use super::super::super::host_fit::{pool_fit, Bound, PoolFit, Rooms};
    use crate::inference::planning::serving_plan::ModelFootprint;
    use crate::resources::{ResourceKind, ResourceLeaseLedger};

    const DEVICE: u64 = 55_662_788_608;
    const EMBED_FLOOR: u64 = 186_908_672;
    const SEQUENCES: u32 = 5;
    const TRAINED: u32 = 262_144;

    fn m5_measured() -> ModelFootprint {
        let mut fp = crate::inference::planning::serving_plan::footprint::calibrated_from_parts("ggml-org/Qwen3.6-35B-A3B-GGUF", 20_690_000_000, TRAINED, true, None)
            .expect("test: a footprint from the weights");
        fp.kv_per_token = 13_520;
        // Fixture: the Devstral calibration at this KV rate, the compute rate the recorded boot planned with.
        fp.compute = crate::inference::planning::serving_plan::ComputeRate::devstral_calibration(fp.kv_per_token);
        fp
    }

    /// The board at the boot: the device full of a leaving engine's unattributed bytes, the embed
    /// lane's floor reserved.
    fn boot_board() -> ResourceLeaseLedger {
        let mut ledger = ResourceLeaseLedger::new();
        ledger.set_capacity(ResourceKind::Vram, DEVICE);
        ledger.set_physical_used(ResourceKind::Vram, DEVICE);
        ledger.reserve("embed", ResourceKind::Vram, EMBED_FLOOR);
        ledger
    }

    // what this catches: the recorded boot. The only device reading the launch had is the gap's
    // replace-myself budget, 0, so the launch read no device room (sizing from it would launch at the
    // floor), and nothing kept the embed lane's floor out of the pool.
    #[test]
    fn before_the_boot_has_no_device_room_to_read() {
        assert_eq!(boot_board().budget_for_replacing("serving", ResourceKind::Vram), 0);
        let fit = pool_fit(661_410, Some(&m5_measured()), Rooms { host: None, device: None }, 0, SEQUENCES, TRAINED);
        assert_eq!(fit, PoolFit::Unmeasured { demand: 661_410 }, "no room read: the need launched unchecked");
    }

    // what this catches: the fix. At the boot the device room is computed from the other consumers'
    // claims, the embed floor outside it, so the pool is min(need, host, device) from the first launch;
    // a device the need does not fit bounds it. On these receipts the need fits the device (the
    // engine's device cost at 661,410 is 38.3 GB of the 55.5 GB room), so the hour's refusals were not
    // the device room: they were the unified pool's host side (free 0.1 GB) and the embed lane's own
    // 13,547 MiB request (`embedding.neural.failed`).
    #[test]
    fn after_the_boot_sizes_beside_the_embed_floor() {
        let fp = m5_measured();
        let room = boot_board().room_beside_others("serving", ResourceKind::Vram);
        assert_eq!(room, DEVICE - EMBED_FLOOR);
        let fit = pool_fit(661_410, Some(&fp), Rooms { host: None, device: Some(room) }, 0, SEQUENCES, TRAINED);
        assert_eq!(fit, PoolFit::Fits { demand: 661_410, fit: 1_310_720 });
        let crowded = DEVICE - 30_000_000_000;
        let bounded = pool_fit(661_410, Some(&fp), Rooms { host: None, device: Some(crowded) }, 0, SEQUENCES, TRAINED);
        assert!(matches!(bounded, PoolFit::Bounded { by: Bound::Device { .. }, .. }), "a device the need does not fit bounds it: {bounded:?}");
    }
}

// Dyno 11: standing starvation (the M5, 2026-11-13 23:40Z, canary 0ed0d231b, card 0f631c46): 0.4 GB
// free, the compressor flat at 2,565,898 pages of 16 KiB (42 GB), swap flat at 14.5 of 15 GB, the
// kernel at critical, and the engine's host prompt cache launched at --cache-ram 16525 MiB. Every
// poll printed `memory.pressure.shed level=critical deficit_mb=0`, and `serving.reclaim.prompt_cache`
// fired zero times. Driven through the monitor's own deficit (`HostDeficit`) and the cache's own
// reporter (`PromptCacheShedder`), as the monitor composes them each poll.
mod standing_starvation {
    use crate::modules::serving_prompt_cache::tests::{poll, rig, FakeEngineCache, M5_GRANT_MIB, MIB};
    use crate::system_resources::memory_pressure::{HeldAtPriority, HostDeficit};
    use crate::system_resources::MemoryPriority;
    use continuum_host::host_memory::{HostMemoryReading, OsMemoryPressure};

    const GB: u64 = 1_000_000_000;
    const SWAP_USED: u64 = 14_500_000_000;
    /// `vm.vm_page_free_target` on the M5: 4,000 pages of 16 KiB.
    const FREE_TARGET: u64 = 4_000 * 16_384;

    fn reading(free: u64, compressed: u64, pressure: OsMemoryPressure) -> HostMemoryReading {
        HostMemoryReading {
            free_bytes: free,
            reclaimable_bytes: 0,
            compressed_bytes: compressed,
            swapped_in_bytes: 289_360_556 * 16_384,
            swapped_out_bytes: 444_562_085 * 16_384,
            free_target_bytes: FREE_TARGET,
            pressure: Some(pressure),
        }
    }

    /// The 23:40Z poll, the same every poll: nothing grows, the free target is met, the kernel says
    /// critical.
    fn starved() -> HostMemoryReading {
        reading(400_000_000, 2_565_898 * 16_384, OsMemoryPressure::Critical)
    }

    // what this catches: the recorded defect's inputs. On the starved poll the counters read clean
    // (free above the kernel's target, compressor and swap flat), which is all the first deficit rule
    // looked at: it took the whole 42 GB + 14.5 GB displaced as its baseline and read 0. Only the
    // kernel's own verdict tells the two apart.
    #[test]
    fn before_the_counters_read_the_starved_host_as_clean() {
        let (first, second) = (starved(), starved());
        assert_eq!(first.shortfall_bytes(), 0, "0.4 GB free is above the 64 MB target");
        assert_eq!((first.compressed_bytes, first.swapped_out_bytes), (second.compressed_bytes, second.swapped_out_bytes), "nothing grows");
        assert_eq!(first.pressure, Some(OsMemoryPressure::Critical), "the kernel's verdict is the only signal left");
    }

    // what this catches: the fix, end to end. The deficit is what the OS displaced since it last read
    // normal (never 0 while it says critical); the cache at Batch is charged first, so its 16.5 GB
    // comes off the minds' share; the cache's lever is asked and lowers the engine's limit; and once
    // the OS reads normal with room above where it read pressure, the cache is raised by that room,
    // the engine's reply the receipt.
    #[tokio::test]
    async fn after_the_cache_is_asked_first_and_raised_within_the_measured_room() {
        let mut host = HostDeficit::default();
        assert_eq!(host.observe(Some(&reading(20 * GB, 3 * GB, OsMemoryPressure::Normal)), GB).owed_bytes(), 0, "boot: OS normal, baseline 4 GB");
        let displaced = 2_565_898 * 16_384 + SWAP_USED;
        for _ in 0..3 {
            assert_eq!(host.observe(Some(&starved()), SWAP_USED).owed_bytes(), displaced - 4 * GB, "flat starvation stays owed");
        }
        let deficit = host.observe(Some(&starved()), SWAP_USED).owed_bytes();

        let cache = FakeEngineCache::full(M5_GRANT_MIB);
        let (shedder, _serving, pressure) = rig(cache.clone(), M5_GRANT_MIB);
        let minds = [HeldAtPriority { priority: MemoryPriority::Interactive, bytes: 2 * GB }];
        let snapshot = poll(&shedder, deficit, &minds, (48 * GB, 64 * GB));
        let cache_bytes = u64::from(M5_GRANT_MIB) * MIB;
        assert_eq!(snapshot.deficit_at(MemoryPriority::Interactive), deficit - cache_bytes, "the cache is charged before the minds");
        pressure.send_replace(snapshot);
        shedder.shed_now().expect("test: the lever is asked").await.expect("test: the step ran");
        assert_eq!(cache.asked(), vec![1], "a deficit past the whole cache lowers it to the route's floor");

        let calm = host.observe(Some(&reading(20 * GB, 30 * GB, OsMemoryPressure::Normal)), SWAP_USED);
        let room = calm.headroom().expect("test: 20 GB free, the OS read pressure at 0.4 GB");
        assert_eq!(room.get(), 20 * GB - 400_000_000);
        pressure.send_replace(poll(&shedder, 0, &minds, (40 * GB, 64 * GB)));
        let (step, claimed) = shedder.take_again_now(room).expect("test: the cache is raised");
        step.await.expect("test: the step ran");
        assert_eq!(cache.asked(), vec![1, u64::from(M5_GRANT_MIB)], "the room holds the whole grant: back to --cache-ram 16525");
        assert_eq!(claimed, (u64::from(M5_GRANT_MIB) - 1) * MIB, "and it claims what it may now fill");
    }
}

// Dyno 13: the take-again flap (the M5, 2026-11-14 01:06:52Z, canary 7a29474a9, #5981). The deficit
// lowered the engine's prompt cache 8,717 -> 1 MiB (`serving.reclaim.prompt_cache` at
// 1794618412459, held 0 after it), and 9.1 s later (`serving.prompt_cache.take_again` at
// 1794618421570) two OS-normal polls raised it straight back to 8,717; the kernel read pressure
// again within minutes (01:13Z: deficit 3,020-4,160 MiB at 4,914-5,979 MiB available). The polls
// between were not probed (the monitor probes on level change), so the calm readings here sit
// inside the range the OS read pressure at.
mod take_again_flap {
    use crate::modules::serving_prompt_cache::tests::{poll, rig, FakeEngineCache, MIB};
    use crate::system_resources::memory_pressure::{HostDeficit, HostDeficitReading};
    use continuum_host::host_memory::{HostMemoryReading, OsMemoryPressure};

    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;
    const GRANT_MIB: u32 = 8_717;
    const QUIET_RAM: (u64, u64) = (0, 64 * GB);

    fn reading(available: u64, pressure: OsMemoryPressure) -> HostMemoryReading {
        HostMemoryReading { free_bytes: available, reclaimable_bytes: 0, compressed_bytes: 15 * GB, swapped_in_bytes: 0, swapped_out_bytes: 0, free_target_bytes: 62 * MB, pressure: Some(pressure) }
    }

    /// The pressured polls: the receipts' available range.
    fn episode(host: &mut HostDeficit) {
        for available in [4_914 * MB, 5_979 * MB, 5_798 * MB] {
            assert!(matches!(host.observe(Some(&reading(available, OsMemoryPressure::Warn)), 0), HostDeficitReading::Owed(_)));
        }
    }

    // what this catches: the recorded flap's shape. Two OS-normal readings inside the pressure range
    // carry no room above it, so any rule that raises the cache on them (#5981 raised to the whole
    // 8,717 MiB grant) re-enters pressure.
    #[test]
    fn before_a_brief_normal_reading_carries_no_room() {
        let mut host = HostDeficit::default();
        episode(&mut host);
        for _ in 0..2 {
            let calm = host.observe(Some(&reading(5_500 * MB, OsMemoryPressure::Normal)), 0);
            assert!(matches!(calm, HostDeficitReading::Calm { .. }), "the OS reads normal");
            assert_eq!(calm.headroom(), None, "but 5.5 GB is inside the 4.9-6.0 GB it read pressure at");
        }
        assert!(u64::from(GRANT_MIB) * MIB > 5_979 * MB - 5_500 * MB, "the old one-jump raise: 8,717 MiB into no room");
    }

    // what this catches: the fix. Lowered to 1 MiB, then calm polls: inside the pressure range
    // nothing is offered; with room the cache rises by at most that room less what its limit has not
    // filled; offered the same room again it rises no further; and a pressured poll offers nothing
    // (the shed path owns it again). The limit never jumps past the room.
    #[tokio::test]
    async fn after_regrowth_is_monotone_within_measured_room() {
        let mut host = HostDeficit::default();
        let cache = FakeEngineCache::holding(0);
        let (shedder, _serving, pressure) = rig(cache.clone(), GRANT_MIB);
        episode(&mut host);
        pressure.send_replace(poll(&shedder, 26_471_432_192, &[], QUIET_RAM));
        shedder.shed_now().expect("test: the lever is asked").await.expect("test: the step ran");
        assert_eq!(cache.asked(), vec![1], "lowered to the route's floor by the recorded 26,471,432,192 B target");

        let brief = host.observe(Some(&reading(5_500 * MB, OsMemoryPressure::Normal)), 0);
        assert!(brief.headroom().is_none(), "no room: the monitor offers nothing");

        let calm = host.observe(Some(&reading(8 * GB, OsMemoryPressure::Normal)), 0);
        let room = calm.headroom().expect("test: room above the episode");
        assert_eq!(room.get(), 8 * GB - 5_979 * MB);
        pressure.send_replace(poll(&shedder, 0, &[], QUIET_RAM));
        let (step, claimed) = shedder.take_again_now(room).expect("test: raised within the room");
        step.await.expect("test: the step ran");
        assert!(claimed <= room.get(), "never past the room: {claimed}");
        let raised = 1 + ((room.get() - MIB) >> 20);
        assert_eq!(cache.asked(), vec![1, raised]);
        assert!(raised < u64::from(GRANT_MIB), "a step, never the whole grant at once");
        assert!(shedder.take_again_now(room).is_none(), "the same room again: what it was given is unfilled");

        let pressed = host.observe(Some(&reading(5 * GB, OsMemoryPressure::Warn)), 0);
        assert!(matches!(pressed, HostDeficitReading::Owed(_)) && pressed.headroom().is_none(), "pressure: nothing offered, the deficit is owed");
        assert_eq!(cache.asked(), vec![1, raised], "monotone: no raise past the room, no re-entry by the cache");
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
