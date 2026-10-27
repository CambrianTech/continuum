//! Swap as ACTIVITY, not occupancy (card f26d6568).
//!
//! macOS never drains inert swap: pages a deploy build pushed out an hour ago stay in
//! the swap file until something touches them. The pressure monitor read swap occupancy
//! as a level, so 1.7 GB left in a 3 GB swapfile (55%) held the M5 at High for hours
//! while `vm_stat` showed 22 GB free and inactive. `usable_bytes` stayed at 20 GB
//! against a 30 GB engine, and four minds shared two lanes. The level could only rise.
//!
//! What costs a node is paging, not occupancy: the model server reading weights back
//! from swap (2026-09-08, decode at 1–9 t/s) or a build pushing pages out. So the swap
//! axis is the smoothed rate of bytes swapped in plus out, which decays once the paging
//! stops. Occupancy keeps one role: a swap file nine tenths full has nowhere left to
//! page, and that stays Critical whatever the rate.
//!
//! Where the kernel's counters are unreadable (Windows, a Mach error), the level falls
//! back to occupancy as before. An unknown rate is never read as calm.

use std::time::{Duration, Instant};

/// Cumulative bytes swapped in and out since boot, as the kernel counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SwapTraffic {
    pub swapped_in: u64,
    pub swapped_out: u64,
}

impl SwapTraffic {
    fn total(self) -> u64 {
        self.swapped_in.saturating_add(self.swapped_out)
    }
}

/// The kernel's swap counters: Mach `swapins`/`swapouts` on macOS, `pswpin`/`pswpout`
/// from `/proc/vmstat` on Linux. None where the platform exposes none.
pub(crate) fn read_swap_traffic() -> Option<SwapTraffic> {
    #[cfg(target_os = "macos")]
    {
        crate::gpu::backends::metal::mach_ffi::read_swap_traffic_bytes().map(
            |(swapped_in, swapped_out)| SwapTraffic {
                swapped_in,
                swapped_out,
            },
        )
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/vmstat").ok()?;
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
        parse_proc_vmstat(&text, page_size)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// PURE: the swap counters from `/proc/vmstat` text, in bytes.
#[cfg(any(target_os = "linux", test))]
fn parse_proc_vmstat(text: &str, page_size: u64) -> Option<SwapTraffic> {
    let mut pages_in = None;
    let mut pages_out = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        match (parts.next(), parts.next().and_then(|v| v.parse::<u64>().ok())) {
            (Some("pswpin"), Some(v)) => pages_in = Some(v),
            (Some("pswpout"), Some(v)) => pages_out = Some(v),
            _ => {}
        }
    }
    Some(SwapTraffic {
        swapped_in: pages_in?.saturating_mul(page_size),
        swapped_out: pages_out?.saturating_mul(page_size),
    })
}

/// How fast the smoothed rate forgets: a burst is half gone after this long. Long enough
/// that one quiet 2 s poll inside a paging storm does not read calm, short enough that
/// the level is back to Normal within minutes of a build ending, not hours.
// derived-or-floor: a floor — fifteen polls of the monitor's 2 s cadence.
pub(crate) const SWAP_RATE_HALF_LIFE: Duration = Duration::from_secs(30);

/// Smoothed swap traffic at or above this is Warning: the node is paging, not idle.
// derived-or-floor: a floor — idle macOS and Linux hosts swap ~0 B/s; 4 MiB/s sustained
// over the half-life is ~120 MiB moved in 30 s, which nothing idle does.
pub(crate) const SWAP_WARNING_BYTES_PER_SEC: f64 = 4.0 * 1024.0 * 1024.0;

/// Smoothed swap traffic at or above this is High: the paging competes with the work.
// derived-or-floor: a floor — 32 MiB/s is ~1 GB per half-life, the scale at which a
// model server reading weights back from swap dropped decode to single digits (9/08).
pub(crate) const SWAP_HIGH_BYTES_PER_SEC: f64 = 32.0 * 1024.0 * 1024.0;

/// Occupancy at or above this is Critical whatever the rate: nowhere left to page to.
pub(crate) const SWAP_FULL_FRACTION: f64 = 0.90;

/// The smoothed swap rate, fed one counter reading per poll.
#[derive(Debug, Default)]
pub(crate) struct SwapActivity {
    last: Option<(Instant, SwapTraffic)>,
    smoothed_bytes_per_sec: f64,
}

impl SwapActivity {
    /// Fold one reading in. Returns the smoothed bytes/s, or None while the rate is
    /// unknown: no counters on this platform, or the first reading (nothing to diff).
    pub(crate) fn observe(&mut self, now: Instant, reading: Option<SwapTraffic>) -> Option<f64> {
        let Some(reading) = reading else {
            self.last = None;
            return None;
        };
        let Some((then, previous)) = self.last.replace((now, reading)) else {
            return None;
        };
        let dt = now.saturating_duration_since(then).as_secs_f64();
        if dt <= 0.0 {
            return Some(self.smoothed_bytes_per_sec);
        }
        // A counter that went backwards (a reset) moved nothing we can measure.
        let moved = reading.total().saturating_sub(previous.total()) as f64;
        let instant_rate = moved / dt;
        let keep = 0.5f64.powf(dt / SWAP_RATE_HALF_LIFE.as_secs_f64());
        self.smoothed_bytes_per_sec = self.smoothed_bytes_per_sec * keep + instant_rate * (1.0 - keep);
        Some(self.smoothed_bytes_per_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    fn traffic(total: u64) -> Option<SwapTraffic> {
        Some(SwapTraffic {
            swapped_in: total / 2,
            swapped_out: total - total / 2,
        })
    }

    // what this catches: card f26d6568 — the swap axis as a ratchet. Paging at 100 MiB/s
    // must read above High within a few polls, and once it stops the rate must fall
    // below Warning within minutes, never hold for hours. The first reading and a
    // platform without counters are unknown (None), never calm (0).
    #[test]
    fn swap_activity_rises_with_paging_and_decays_when_it_stops() {
        let mut activity = SwapActivity::default();
        let start = Instant::now();
        let mut moved = 5_000 * MIB;
        assert_eq!(activity.observe(start, traffic(moved)), None, "the first reading has no rate");
        let mut rate = 0.0;
        let mut t = start;
        for _ in 0..30 {
            t += Duration::from_secs(2);
            moved += 200 * MIB;
            rate = activity.observe(t, traffic(moved)).unwrap();
        }
        assert!(rate >= SWAP_HIGH_BYTES_PER_SEC, "a minute of 100 MiB/s paging is High ({rate})");
        for _ in 0..90 {
            t += Duration::from_secs(2);
            rate = activity.observe(t, traffic(moved)).unwrap();
        }
        assert!(rate < SWAP_WARNING_BYTES_PER_SEC, "three quiet minutes read calm ({rate})");
        assert_eq!(activity.observe(t + Duration::from_secs(2), None), None, "no counters is unknown");
    }

    #[test]
    fn proc_vmstat_counters_are_read_in_bytes() {
        let text = "nr_free_pages 1\npswpin 10\npswpout 30\npgfault 9\n";
        assert_eq!(
            parse_proc_vmstat(text, 4096),
            Some(SwapTraffic {
                swapped_in: 10 * 4096,
                swapped_out: 30 * 4096
            })
        );
        assert_eq!(parse_proc_vmstat("pswpin 10\n", 4096), None, "a missing counter is no reading");
    }
}
