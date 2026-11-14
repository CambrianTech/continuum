//! The DEFICIT: how many bytes the host needs given back to come out of memory pressure. It is
//! the one quantity every shedder sheds against (`PressureSnapshot::deficit_at` and
//! `share_of_deficit`), and the shed path runs while it is positive.
//!
//! Why: on the M5 on 2026-11-13, 60 MB were free, ~34 GB sat in the OS compressor and swap was
//! 8.6 of 10 GB, prefill ran at half speed, and NOTHING was shed: the only shed amount was RAM
//! "used" past a fixed fraction of total, and a compressing, swapping host reads "used" under
//! every gate because the compressor packs the pressure into fewer pages. When the OS compresses
//! or swaps, our resident caches cost every mind decode speed, so what the OS has displaced is
//! what we owe.
//!
//! Why the OS decides when it is clean: on the M5 on 2026-11-13 (23:40Z, canary 0ed0d231b) 0.4 GB
//! were free, 42 GB sat in the compressor and swap was 14.5 of 15 GB, yet the deficit read 0 on
//! almost every poll. Under STANDING starvation the kernel holds free pages at its small free
//! target by compressing, so the compressor and swap stop growing and the free target is met: the
//! first cut of this file called that poll "clean" (no shortfall, no growth) and took the whole
//! 42 GB displaced as its baseline. Starvation read as normal. Only the OS knows whether it is
//! under pressure, and it says so: its own pressure verdict (`OsMemoryPressure`, read by the host
//! adapter: macOS `kern.memorystatus_vm_pressure_level`, Linux PSI).
//!
//! The formula, per poll, from the host reading (`continuum_host::host_memory`):
//!
//! ```text
//! displaced = compressed + swap_used                 bytes the OS has pushed out of plain RAM
//! shortfall = max(0, free_target - (free + reclaimable))   what the OS still needs to reach its
//!                                                           own free target
//! OS Normal       => baseline = displaced; Calm { headroom = available - max(floor, free_target) }
//! OS Warn/Critical => Owed(max((displaced - baseline) + shortfall, free_target));
//!                     floor = the most available on any poll of this pressure episode
//!                     (no OS-normal poll seen yet, e.g. a restart into pressure: baseline = 0;
//!                      no baseline is ever taken while the OS reports pressure)
//! no verdict      => NoVerdict, no baseline (unknown, never calm: the RAM gate and the swap
//!                     axis still decide)
//! ```
//!
//! Taking memory back is decided by measured room, never by a count of calm polls. The M5,
//! 2026-11-14 01:06:52Z (canary 7a29474a9): the deficit lowered the engine's prompt cache 8,717 ->
//! 1 MiB, and 9 s later two OS-normal polls raised it straight back to 8,717; the kernel read
//! pressure again within minutes. The OS read pressure with up to `floor` bytes available, so it
//! can give only what is available beyond that: a brief normal reading inside the range it read
//! pressure at offers nothing. Each reporter claims at most the headroom and the monitor takes its
//! claim off before the next (`MemoryReporter::take_again`); the next pressured poll lowers it
//! again and raises the floor.
//!
//! While the OS reports pressure the deficit is never 0: when nothing grew since the last
//! OS-normal poll (standing starvation that began at a displaced level the OS once tolerated),
//! the deficit is the kernel's own free target, so every poll gives back one free target's worth
//! more until the OS reads normal again. Every number is the machine's own: the verdict and the
//! free target are the kernel's (macOS `vm.vm_page_free_target`, Linux `min_free_kbytes`), and
//! "grew" is a comparison with the OS-normal reading, never a fraction. Inert swap left by an old
//! build does not pin the deficit: the next OS-normal poll takes it into the baseline.

use std::num::NonZeroU64;

use continuum_host::host_memory::{HostMemoryReading, OsMemoryPressure};

use super::{MemoryPriority, PressureLevel, PressureSnapshot};

/// One poll's verdict on the host, from its own reading: never one number for two meanings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostDeficitReading {
    /// No reading, or the OS gave no pressure verdict: unknown, never calm. Nothing is owed by this
    /// axis and nothing may be taken back.
    NoVerdict,
    /// The OS reads itself normal. `headroom` is what it can give without re-entering pressure: what
    /// is available now beyond the most it had available while it last read pressure (and never
    /// below its own free target). Zero = calm, but no room to take anything back.
    Calm { headroom: u64 },
    /// The OS reads pressure: these bytes are owed.
    Owed(NonZeroU64),
}

impl HostDeficitReading {
    /// The bytes every shedder sheds against: what is owed, none otherwise.
    pub fn owed_bytes(self) -> u64 {
        match self {
            Self::Owed(owed) => owed.get(),
            Self::NoVerdict | Self::Calm { .. } => 0,
        }
    }

    /// What a reporter that gave memory back may take again this poll; `None` while anything is
    /// owed, unknown, or there is no room.
    pub fn headroom(self) -> Option<NonZeroU64> {
        match self {
            Self::Calm { headroom } => NonZeroU64::new(headroom),
            Self::NoVerdict | Self::Owed(_) => None,
        }
    }
}

/// Where the host stands between pressure episodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Episode {
    /// No pressure seen since boot.
    #[default]
    Unseen,
    /// The OS reads pressure now; `floor` is the most it had available on any poll of this episode.
    Pressured { floor: u64 },
    /// The OS read pressure at up to `floor` available, and reads normal since.
    Recovered { floor: u64 },
}

/// The deficit across polls: what the OS held displaced at its last OS-normal poll, and the most
/// memory it had available while it last read pressure (the regrowth half's margin).
#[derive(Debug, Default)]
pub struct HostDeficit {
    baseline: Option<u64>,
    episode: Episode,
}

impl HostDeficit {
    /// Fold one poll in. `swap_used` is the swap occupancy the monitor reads beside the reading.
    pub fn observe(&mut self, reading: Option<&HostMemoryReading>, swap_used: u64) -> HostDeficitReading {
        let Some((reading, pressure)) = reading.and_then(|reading| reading.pressure.map(|pressure| (reading, pressure))) else {
            return HostDeficitReading::NoVerdict;
        };
        let displaced = reading.compressed_bytes.saturating_add(swap_used);
        let available = reading.available_bytes();
        match pressure {
            OsMemoryPressure::Normal => {
                self.baseline = Some(displaced);
                let floor = match self.episode {
                    Episode::Unseen => None,
                    Episode::Pressured { floor } | Episode::Recovered { floor } => Some(floor),
                };
                if let Some(floor) = floor {
                    self.episode = Episode::Recovered { floor };
                }
                let margin = floor.map_or(reading.free_target_bytes, |floor| floor.max(reading.free_target_bytes)); // map_or: no pressure seen yet, the kernel's own free target is the margin
                HostDeficitReading::Calm { headroom: available.saturating_sub(margin) }
            }
            OsMemoryPressure::Warn | OsMemoryPressure::Critical => {
                self.episode = match self.episode {
                    Episode::Pressured { floor } => Episode::Pressured { floor: floor.max(available) },
                    Episode::Unseen | Episode::Recovered { .. } => Episode::Pressured { floor: available },
                };
                let grown = self.baseline.map_or(displaced, |baseline| displaced.saturating_sub(baseline));
                let owed = grown.saturating_add(reading.shortfall_bytes()).max(reading.free_target_bytes);
                // A kernel reporting a zero free target and nothing grown still reads pressure: at
                // least one byte owed keeps the shed path running, never a calm 0.
                HostDeficitReading::Owed(NonZeroU64::MIN.saturating_add(owed.saturating_sub(1)))
            }
        }
    }
}

impl PressureLevel {
    /// The deficit axis: a positive deficit is at least Warning, so the shed path runs; how much
    /// is shed is the deficit itself, never the level.
    pub(crate) fn from_deficit(deficit_bytes: u64) -> Self {
        match deficit_bytes {
            0 => Self::Normal,
            _ => Self::Warning,
        }
    }
}

/// Should the monitor call the shedders this poll? After two polls at a level (hysteresis), once
/// at Warning and High; every poll at sustained Critical; and every poll while the host has a
/// deficit, because each poll's shares are recomputed from what is still held and still owed.
pub(crate) fn shed_due(level: PressureLevel, consecutive_at_level: u32, deficit_bytes: u64) -> bool {
    if deficit_bytes > 0 && consecutive_at_level >= 2 {
        return true;
    }
    match level {
        PressureLevel::Critical => consecutive_at_level >= 2,
        PressureLevel::High | PressureLevel::Warning => consecutive_at_level == 2,
        PressureLevel::Normal => false,
    }
}

impl PressureSnapshot {
    /// Bytes every consumer at `priority` gives back between them: the larger of the RAM overrun
    /// past `priority`'s gate and the host deficit left after every lower priority (which yields
    /// first) gives back all it holds. Realtime is stopped only by OOM, so the deficit never
    /// reaches it. Zero on a clean box.
    pub fn deficit_at(&self, priority: MemoryPriority) -> u64 {
        let used = self.total_bytes.saturating_sub(self.available_bytes);
        let gate = (self.total_bytes as f64 * priority.pressure_gate()) as u64;
        let ram_overrun = used.saturating_sub(gate);
        let host = match priority {
            MemoryPriority::Realtime => 0,
            _ => {
                let yields_first: u64 = self
                    .held_by_priority
                    .iter()
                    .filter(|held| held.priority > priority)
                    .map(|held| held.bytes)
                    .sum();
                self.deficit_bytes.saturating_sub(yields_first)
            }
        };
        ram_overrun.max(host)
    }

    /// Bytes every reporter at `priority` held at this snapshot's poll.
    pub fn held_at(&self, priority: MemoryPriority) -> u64 {
        self.held_by_priority
            .iter()
            .filter(|held| held.priority == priority)
            .map(|held| held.bytes)
            .sum()
    }

    /// The part of `priority`'s deficit a consumer holding `held` bytes gives back: its share in
    /// proportion to what it holds among every reporter at that priority (the monitor's own count
    /// of its reporters, published in the snapshot), so one pressure event frees it once.
    pub fn share_of_deficit(&self, priority: MemoryPriority, held: u64) -> u64 {
        let total = self.held_at(priority).max(held);
        if total == 0 {
            return 0;
        }
        (u128::from(self.deficit_at(priority)) * u128::from(held) / u128::from(total)) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_resources::memory_pressure::HeldAtPriority;

    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;
    /// The M5's free target: `vm.vm_page_free_target` 4,000 pages of 16 KiB (2026-11-13).
    const M5_FREE_TARGET: u64 = 4_000 * 16_384;

    fn reading(free: u64, reclaimable: u64, compressed: u64, pressure: OsMemoryPressure) -> HostMemoryReading {
        HostMemoryReading {
            free_bytes: free,
            reclaimable_bytes: reclaimable,
            compressed_bytes: compressed,
            swapped_in_bytes: 0,
            swapped_out_bytes: 0,
            free_target_bytes: M5_FREE_TARGET,
            pressure: Some(pressure),
        }
    }

    /// The first incident poll: 60 MB free, the file cache already dropped, 34 GB compressed, and
    /// the kernel reading itself under pressure.
    fn incident() -> HostMemoryReading {
        reading(60 * MB, 0, 34 * GB, OsMemoryPressure::Warn)
    }

    /// The 23:40Z poll: 0.4 GB free (above the 64 MB target, so no shortfall), the compressor
    /// flat at 2,565,898 pages of 16 KiB (42 GB), the kernel at critical.
    fn standing_starvation() -> HostMemoryReading {
        reading(400 * MB, 0, 2_565_898 * 16_384, OsMemoryPressure::Critical)
    }

    fn snapshot(deficit_bytes: u64, held: Vec<HeldAtPriority>) -> PressureSnapshot {
        PressureSnapshot {
            // sysinfo's "used" is 48 of 64 GB, 75%: under the Interactive gate (80%).
            total_bytes: 64 * GB,
            available_bytes: 16 * GB,
            deficit_bytes,
            held_by_priority: held,
            ..PressureSnapshot::default()
        }
    }

    // what this catches: the 2026-11-13 M5 incident. Free 60 MB, compressor 34 GB, swap 8.6 of
    // 10 GB, RAM "used" under the old gate: the deficit is positive (after an OS-normal baseline,
    // and after a restart straight into the pressure), the level is at least Warning, the shed
    // path runs on every poll while it lasts, and a reporter at Interactive is asked to give back
    // bytes although the RAM overrun alone is zero.
    #[test]
    fn the_m5_incident_has_a_deficit_and_the_reporters_shed() {
        let swap_used = 8_600 * MB;
        let mut host = HostDeficit::default();
        assert_eq!(host.observe(Some(&reading(20 * GB, 8 * GB, 2 * GB, OsMemoryPressure::Normal)), GB).owed_bytes(), 0, "OS normal: baseline 3 GB");
        let deficit = host.observe(Some(&incident()), swap_used).owed_bytes();
        let shortfall = M5_FREE_TARGET - 60 * MB;
        assert_eq!(deficit, 34 * GB + swap_used - 3 * GB + shortfall, "displaced past the OS-normal baseline, plus the free target");

        let mut restarted = HostDeficit::default();
        assert_eq!(
            restarted.observe(Some(&incident()), swap_used).owed_bytes(),
            34 * GB + swap_used + shortfall,
            "no OS-normal poll since the restart: everything displaced is owed, from the first poll"
        );

        assert_eq!(PressureLevel::from_deficit(deficit), PressureLevel::Warning);
        assert!(shed_due(PressureLevel::Warning, 2, deficit) && shed_due(PressureLevel::Warning, 9, deficit));
        let held = vec![
            HeldAtPriority { priority: MemoryPriority::Interactive, bytes: 2 * GB },
            HeldAtPriority { priority: MemoryPriority::Background, bytes: GB },
        ];
        let incident_snapshot = snapshot(deficit, held.clone());
        assert_eq!(snapshot(0, held).deficit_at(MemoryPriority::Interactive), 0, "the old RAM-only figure: nothing");
        assert!(
            incident_snapshot.share_of_deficit(MemoryPriority::Interactive, 2 * GB) >= 2 * GB,
            "mind results give back all they can"
        );
        assert_eq!(incident_snapshot.deficit_at(MemoryPriority::Realtime), 0, "only OOM stops Realtime");
    }

    // what this catches: standing starvation read as normal (the M5, 2026-11-13 23:40Z). The
    // compressor (42 GB) and swap (14.5 of 15 GB) are flat and free memory sits above the
    // kernel's target, so the old counter rule called every poll clean, took the whole 42 GB as
    // its baseline and read 0. The kernel says critical: no baseline is taken, the deficit is what
    // the OS has displaced since it last read normal, and it never drops to 0 while the kernel
    // says pressure, even with nothing growing past a baseline the OS once tolerated.
    #[test]
    fn standing_starvation_is_owed_while_the_os_says_pressure() {
        let swap_used = 14_500 * MB;
        let displaced = 2_565_898 * 16_384 + swap_used;
        let mut host = HostDeficit::default();
        assert_eq!(host.observe(Some(&reading(20 * GB, 8 * GB, 2 * GB, OsMemoryPressure::Normal)), GB).owed_bytes(), 0, "OS normal at boot: baseline 3 GB");
        for poll in 0..30 {
            let deficit = host.observe(Some(&standing_starvation()), swap_used).owed_bytes();
            assert_eq!(deficit, displaced - 3 * GB, "poll {poll}: flat counters, still owed, the baseline never moves");
            assert!(host.observe(Some(&standing_starvation()), swap_used).headroom().is_none());
        }

        let mut tolerated = HostDeficit::default();
        tolerated.observe(Some(&reading(400 * MB, 0, 2_565_898 * 16_384, OsMemoryPressure::Normal)), swap_used).owed_bytes();
        assert_eq!(
            tolerated.observe(Some(&standing_starvation()), swap_used).owed_bytes(),
            M5_FREE_TARGET,
            "nothing grew since the OS read normal: the kernel's own free target, never 0"
        );
    }

    // what this catches: memory taken back on a poll count instead of measured room (#5981's
    // `calm_polls >= 2`, the 9 s flap on the M5 2026-11-14 01:07Z). Calm with room is the only
    // reading that offers headroom; the margin is the most the OS had available while it last read
    // pressure, so a brief normal reading inside that range offers nothing, and no verdict is never
    // calm.
    #[test]
    fn headroom_is_room_above_where_the_os_last_read_pressure() {
        let mut host = HostDeficit::default();
        assert_eq!(host.observe(Some(&reading(20 * GB, 8 * GB, 2 * GB, OsMemoryPressure::Normal)), 0).headroom().map(|h| h.get()), Some(28 * GB - M5_FREE_TARGET), "no pressure seen: the free target is the margin");
        for available in [4_914 * MB, 5_979 * MB, 5_400 * MB] {
            assert!(matches!(host.observe(Some(&reading(available, 0, 15 * GB, OsMemoryPressure::Warn)), 0), HostDeficitReading::Owed(_)));
        }
        let brief = host.observe(Some(&reading(5_500 * MB, 0, 15 * GB, OsMemoryPressure::Normal)), 0);
        assert_eq!(brief, HostDeficitReading::Calm { headroom: 0 }, "calm inside the pressure range: no room");
        assert_eq!(brief.headroom(), None);
        let room = host.observe(Some(&reading(9 * GB, 0, 15 * GB, OsMemoryPressure::Normal)), 0);
        assert_eq!(room.headroom().map(|h| h.get()), Some(9 * GB - 5_979 * MB), "room above the episode's most available");
        let mut no_verdict = reading(20 * GB, 8 * GB, 2 * GB, OsMemoryPressure::Normal);
        no_verdict.pressure = None;
        assert_eq!(host.observe(Some(&no_verdict), 0), HostDeficitReading::NoVerdict, "no verdict: no claim either way");
        assert_eq!(host.observe(None, 0), HostDeficitReading::NoVerdict);
    }

    // what this catches: a clean box shedding. While the OS reads itself normal, a small
    // compressor and inert swap a build left behind are no deficit on any poll, Normal, and no
    // shed.
    #[test]
    fn a_clean_box_has_no_deficit_and_sheds_nothing() {
        let mut host = HostDeficit::default();
        for _ in 0..5 {
            let deficit = host.observe(Some(&reading(12 * GB, 6 * GB, 500 * MB, OsMemoryPressure::Normal)), 1_700 * MB).owed_bytes();
            assert_eq!(deficit, 0);
            assert_eq!(PressureLevel::from_deficit(deficit), PressureLevel::Normal);
            assert!(!shed_due(PressureLevel::Normal, 5, deficit));
        }
        let clean = snapshot(0, vec![HeldAtPriority { priority: MemoryPriority::Interactive, bytes: GB }]);
        assert_eq!(clean.share_of_deficit(MemoryPriority::Interactive, GB), 0, "75% used is under Interactive's gate");
        assert_eq!(host.observe(None, 0).owed_bytes(), 0, "no reading is no deficit claim");
    }

    // what this catches: every priority shedding the whole deficit at once. A lower priority
    // yields first: Interactive owes only what Batch and Background cannot cover.
    #[test]
    fn lower_priorities_give_back_first() {
        let held = vec![
            HeldAtPriority { priority: MemoryPriority::Batch, bytes: 3 * GB },
            HeldAtPriority { priority: MemoryPriority::Background, bytes: 2 * GB },
        ];
        // RAM use is nothing here, so only the host deficit decides.
        let quiet_ram = |deficit_bytes| PressureSnapshot { available_bytes: 64 * GB, ..snapshot(deficit_bytes, held.clone()) };
        let small = quiet_ram(GB);
        assert_eq!(small.deficit_at(MemoryPriority::Batch), GB);
        assert_eq!(small.deficit_at(MemoryPriority::Interactive), 0, "Batch covers it");
        let large = quiet_ram(10 * GB);
        assert_eq!(large.deficit_at(MemoryPriority::Background), 7 * GB);
        assert_eq!(large.deficit_at(MemoryPriority::Interactive), 5 * GB);
    }

    // what this catches: every consumer at a priority shedding the WHOLE overrun, so N caches free
    // N times what the host needed and rebuildable entries thrash: the shares of one overrun across
    // a priority's reporters add up to it, in proportion to what each holds.
    #[test]
    fn one_deficit_is_shared_across_a_priority_not_repeated() {
        let priority = MemoryPriority::Background;
        let snapshot = PressureSnapshot {
            total_bytes: 1000,
            available_bytes: 300,
            held_by_priority: vec![HeldAtPriority { priority, bytes: 400 }],
            ..PressureSnapshot::default()
        };
        assert_eq!(snapshot.deficit_at(priority), 100, "used 700 of 1000 is 100 past Background's 0.60 gate");
        let (a, b) = (snapshot.share_of_deficit(priority, 300), snapshot.share_of_deficit(priority, 100));
        assert_eq!((a, b), (75, 25), "in proportion to what each holds");
    }
}
