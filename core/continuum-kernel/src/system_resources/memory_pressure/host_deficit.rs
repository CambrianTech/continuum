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
//! The formula, per poll, from the host reading (`continuum_host::host_memory`):
//!
//! ```text
//! displaced = compressed + swap_used                 bytes the OS has pushed out of plain RAM
//! shortfall = max(0, free_target - (free + reclaimable))   what the OS still needs to reach its
//!                                                           own free target
//! clean     = shortfall == 0 AND no swap-out AND the compressor did not grow since the last poll
//! clean  => baseline = displaced; deficit = 0
//! else   => deficit = (displaced - baseline) + shortfall
//!           (no clean poll seen yet, e.g. a restart into pressure: baseline = 0)
//! ```
//!
//! Every threshold is the machine's own: the free target is the kernel's (macOS
//! `vm.vm_page_free_target`, Linux `min_free_kbytes`), and "grew" is a comparison with the last
//! reading, never a fraction. Inert swap left by an old build does not pin the deficit: the next
//! clean poll takes it into the baseline. A clean box is zero by construction. No reading (an OS
//! without an adapter, a failed call) is no deficit claim: the RAM gate and the swap axis still
//! decide there.

use continuum_host::host_memory::HostMemoryReading;

use super::{MemoryPriority, PressureLevel, PressureSnapshot};

/// The deficit across polls: the last reading (to see growth) and what the OS held displaced at
/// the last clean poll.
#[derive(Debug, Default)]
pub(crate) struct HostDeficit {
    last: Option<(u64, u64)>,
    baseline: Option<u64>,
}

impl HostDeficit {
    /// Fold one poll in and return its deficit in bytes. `swap_used` is the swap occupancy the
    /// monitor reads beside the reading. The first reading has nothing to compare with: 0.
    pub(crate) fn observe(&mut self, reading: Option<&HostMemoryReading>, swap_used: u64) -> u64 {
        let Some(reading) = reading else {
            self.last = None;
            return 0;
        };
        let now = (reading.compressed_bytes, reading.swapped_out_bytes);
        let Some((compressed_before, swapped_out_before)) = self.last.replace(now) else {
            return 0;
        };
        let displaced = reading.compressed_bytes.saturating_add(swap_used);
        let shortfall = reading.shortfall_bytes();
        let compressing = reading.compressed_bytes > compressed_before;
        let swapping_out = reading.swapped_out_bytes > swapped_out_before;
        if shortfall == 0 && !compressing && !swapping_out {
            self.baseline = Some(displaced);
            return 0;
        }
        let grown = self.baseline.map_or(displaced, |baseline| displaced.saturating_sub(baseline));
        grown.saturating_add(shortfall)
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

    fn reading(free: u64, reclaimable: u64, compressed: u64, swapped_out: u64) -> HostMemoryReading {
        HostMemoryReading {
            free_bytes: free,
            reclaimable_bytes: reclaimable,
            compressed_bytes: compressed,
            swapped_in_bytes: 0,
            swapped_out_bytes: swapped_out,
            free_target_bytes: M5_FREE_TARGET,
        }
    }

    /// The incident poll: 60 MB free, the file cache already dropped, 34 GB compressed, and the
    /// OS still swapping out.
    fn incident(swapped_out: u64) -> HostMemoryReading {
        reading(60 * MB, 0, 34 * GB, swapped_out)
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
    // 10 GB, RAM "used" under the old gate: the deficit is positive (after a clean baseline, and
    // after a restart straight into the pressure), the level is at least Warning, the shed path
    // runs on every poll while it lasts, and a reporter at Interactive is asked to give back
    // bytes although the RAM overrun alone is zero.
    #[test]
    fn the_m5_incident_has_a_deficit_and_the_reporters_shed() {
        let swap_used = 8_600 * MB;
        let mut host = HostDeficit::default();
        assert_eq!(host.observe(Some(&reading(20 * GB, 8 * GB, 2 * GB, 100 * GB)), GB), 0, "the first reading");
        assert_eq!(host.observe(Some(&reading(20 * GB, 8 * GB, 2 * GB, 100 * GB)), GB), 0, "clean: baseline 3 GB");
        let deficit = host.observe(Some(&incident(101 * GB)), swap_used);
        let shortfall = M5_FREE_TARGET - 60 * MB;
        assert_eq!(deficit, 34 * GB + swap_used - 3 * GB + shortfall, "displaced past the clean baseline, plus the free target");

        let mut restarted = HostDeficit::default();
        restarted.observe(Some(&incident(101 * GB)), swap_used);
        assert_eq!(
            restarted.observe(Some(&incident(102 * GB)), swap_used),
            34 * GB + swap_used + shortfall,
            "no clean poll since the restart: everything displaced is owed"
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

    // what this catches: a clean box shedding. A compressor that is small and steady, no swap
    // growth and free memory above the kernel's target is no deficit on any poll, Normal, and no
    // shed; inert swap a build left behind does not count.
    #[test]
    fn a_clean_box_has_no_deficit_and_sheds_nothing() {
        let mut host = HostDeficit::default();
        for _ in 0..5 {
            let deficit = host.observe(Some(&reading(12 * GB, 6 * GB, 500 * MB, 7 * GB)), 1_700 * MB);
            assert_eq!(deficit, 0);
            assert_eq!(PressureLevel::from_deficit(deficit), PressureLevel::Normal);
            assert!(!shed_due(PressureLevel::Normal, 5, deficit));
        }
        let clean = snapshot(0, vec![HeldAtPriority { priority: MemoryPriority::Interactive, bytes: GB }]);
        assert_eq!(clean.share_of_deficit(MemoryPriority::Interactive, GB), 0, "75% used is under Interactive's gate");
        assert_eq!(host.observe(None, 0), 0, "no reading is no deficit claim");
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
