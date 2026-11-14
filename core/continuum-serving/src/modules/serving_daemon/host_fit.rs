//! The engine's one pool fits the HOST, never only the minds: the minds' needs are the demand, and
//! the host's own reading decides how much of it the launch takes.
//!
//! The M5 (64 GB, Qwen3.6-35B-A3B), canary 7a29474a9, 2026-11-14 00:57Z: the engine launched at
//! `-c 623070` (`launch_pool` took the plan's 3 x 207,691, sized against a 46.0 GB budget: the
//! board's 25.6 GB plus the incumbent's weights credited back) beside `--cache-ram 8717`. Its
//! measured cost at that pool (`state/lane-footprint.json`: 15,163 B per token, 24.97 GB anonymous)
//! plus 20.7 GB of weights is ~45.7 GB, while the host's reading at boot was 52.1 GB available with
//! 1.3 GB compressed and 5.9 GB swapped: room for 44.9 GB without displacing anyone. Seventeen
//! minutes later 0.1 GB were free, the compressor held 34 GB, swap was 94%, and the kernel read
//! pressure; the residents decoded at ~10.6 tok/s. No term of the launch read the host.
//!
//! The room is the host's own reading (`continuum_host::host_memory`): what the OS can give without
//! displacing anyone (free plus reclaimable), less what it has ALREADY displaced (compressed plus
//! swapped: that working set wants its pages back before an engine takes them), plus what this
//! launch releases (the running engine's own bytes, when it replaces one). The engine's cost is the
//! plan's one arithmetic (`ModelFootprint::window_within`, which carries the measured per-token
//! cost), with its host prompt cache taken out first. When the minds need more than that, the pool
//! is what fits and their grants share it by measured need (`grants.rs`); the launch says so.
//!
//! The device is a second room (01:22Z the same night: all five slots busy, Aris with four requests
//! and none completed, and the embedder's 186,908,672 B lease refused at VRAM available 0, so
//! embeddings lost relevance). Weights, KV and compute must also fit the board's governed device
//! budget, which already nets every other consumer's reserved floor; it is read only when that
//! budget holds serving's own bytes (`OwnCredit::Included`), the one view in which it is a reading
//! of what the launch may have.

use continuum_host::host_memory::HostMemoryReading;

use crate::inference::planning::serving_plan::{ModelFootprint, SENT_HEADROOM};
use crate::inference::planning::MIN_SERVE_CTX;

/// What the host can give an engine now, every term from its own reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HostRoom {
    /// Free plus reclaimable: what the OS gives without displacing anyone.
    pub(super) available: u64,
    /// Compressed plus swapped: working set the OS already pushed out of plain RAM.
    pub(super) displaced: u64,
    /// The running engine's own bytes, returned when this launch replaces it.
    pub(super) released: u64,
}

impl HostRoom {
    pub(super) fn of(reading: &HostMemoryReading, swap_used: u64, released: u64) -> Self {
        Self { available: reading.available_bytes(), displaced: reading.compressed_bytes.saturating_add(swap_used), released }
    }

    pub(super) fn bytes(self) -> u64 {
        self.available.saturating_add(self.released).saturating_sub(self.displaced)
    }
}

/// One seated mind's claim on the pool, from her requirement (`window_allocator::Measure`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MindClaim {
    /// A held turn measured her floor: the pool holds it with the growth margin her grant uses
    /// (`Demand::of`'s target, `SENT_HEADROOM` over her need).
    Held { need: u64 },
    /// No held turn yet: her own measured wish or declared size is her floor, as is.
    Own { target: u64 },
}

/// What the seated minds need of the pool: each claim (capped at the trained window), plus the
/// planner's least window for every sequence no mind claims (the scratch traffic, minds leased in).
/// `None` when no mind claims anything (a boot that remembers nobody): the plan's pool stands.
///
/// The size is the need, never the room: the M5, 2026-11-14 00:57Z, held 623,070 tokens for needs of
/// 382,706, and a pool filled to the room (566,990 at the boot's reading) leaves the core, the
/// embedder's lease, compute buffers and the OS nothing; the 01:22Z receipts are that shortfall.
/// Nothing holds what it is not using; a need that grows past the pool is the floor-sized rule's.
pub(super) fn need_with_growth(claims: &[MindClaim], sequences: u32, trained_window: u32) -> Option<u64> {
    if claims.is_empty() {
        return None;
    }
    let trained = u64::from(trained_window);
    let claimed: u64 = claims
        .iter()
        .map(|claim| match *claim {
            MindClaim::Held { need } => (need as f64 * SENT_HEADROOM) as u64,
            MindClaim::Own { target } => target,
        })
        .map(|tokens| tokens.min(trained))
        .sum();
    let unclaimed = u64::from(sequences).saturating_sub(claims.len() as u64);
    Some(claimed + unclaimed * u64::from(MIN_SERVE_CTX))
}

/// Which room bounded the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Bound {
    /// The host's RAM: weights, KV, compute and the host prompt cache past what the OS can give.
    Host(HostRoom),
    /// The device's governed working set (the board's replace-myself budget, net of every other
    /// consumer's reserved floor, the embedder's among them): weights, KV and compute past it.
    Device { room: u64 },
}

/// The launch pool against the machine: the demand (`launch::launch_pool`, from the plan and the
/// minds' needs) and what the host's and the device's rooms hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PoolFit {
    /// No room was read (no host reading on this OS and no settled device budget), or no footprint
    /// to cost the pool with: the demand stands. Unknown is never calm, but a launch that waits on
    /// a reading that cannot come leaves every mind dark.
    Unmeasured { demand: u64 },
    /// The demand fits every room read.
    Fits { demand: u64, fit: u64 },
    /// The demand exceeds a room: the pool is what fits, never below the planner's least window per
    /// sequence (the engine refuses a pool with no room for one prompt).
    Bounded { demand: u64, fit: u64, by: Bound },
}

impl Bound {
    /// The bytes of the room that bounded the pool.
    pub(super) fn room_bytes(self) -> u64 {
        match self {
            Self::Host(room) => room.bytes(),
            Self::Device { room } => room,
        }
    }
}

impl PoolFit {
    /// The demand, and what the rooms hold (`None` when none was read).
    pub(super) fn demand_and_fit(self) -> (u64, Option<u64>) {
        match self {
            Self::Unmeasured { demand } => (demand, None),
            Self::Fits { demand, fit } | Self::Bounded { demand, fit, .. } => (demand, Some(fit)),
        }
    }

    /// The pool the engine launches with, in tokens.
    pub(super) fn pool(self, sequences: u32) -> u64 {
        match self {
            Self::Unmeasured { demand } | Self::Fits { demand, .. } => demand,
            Self::Bounded { demand, fit, .. } => fit.max(u64::from(MIN_SERVE_CTX) * u64::from(sequences)).min(demand),
        }
    }
}

/// The rooms a launch fits into, each `None` when not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rooms {
    pub(super) host: Option<HostRoom>,
    /// The device budget in bytes, read only when it already holds serving's own bytes.
    pub(super) device: Option<u64>,
}

/// Fit `demand` tokens over `sequences` into every room read: the host's after the engine's host
/// prompt cache, the device's as is (the cache lives in host RAM).
pub(super) fn pool_fit(demand: u64, footprint: Option<&ModelFootprint>, rooms: Rooms, host_cache_bytes: u64, sequences: u32, trained_window: u32) -> PoolFit {
    let Some(footprint) = footprint else {
        return PoolFit::Unmeasured { demand };
    };
    let tokens = |bytes: u64| u64::from(footprint.window_within(bytes, sequences).min(trained_window)) * u64::from(sequences);
    let host = rooms.host.map(|room| (tokens(room.bytes().saturating_sub(host_cache_bytes)), Bound::Host(room)));
    let device = rooms.device.map(|room| (tokens(room), Bound::Device { room }));
    let Some((fit, by)) = [host, device].into_iter().flatten().min_by_key(|(fit, _)| *fit) else {
        return PoolFit::Unmeasured { demand };
    };
    match demand <= fit {
        true => PoolFit::Fits { demand, fit },
        false => PoolFit::Bounded { demand, fit, by },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn room(available: u64, displaced: u64, released: u64) -> HostRoom {
        HostRoom { available, displaced, released }
    }

    // what this catches: a pool sized to fill the room instead of the minds' need (the M5, 00:57Z:
    // 623,070 for needs of 382,706), a growth margin other than the grants', and a boot that
    // remembers nobody launched at the floor (the 2,048-token dark node of card 8569b03d).
    #[test]
    fn the_need_is_each_claim_with_the_grants_growth_and_a_least_window_per_free_sequence() {
        let claims = [MindClaim::Held { need: 100_000 }, MindClaim::Own { target: 900_000 }];
        assert_eq!(need_with_growth(&claims, 4, 262_144), Some(125_000 + 262_144 + 2 * u64::from(MIN_SERVE_CTX)));
        assert_eq!(need_with_growth(&[], 5, 262_144), None, "nobody remembered: the plan's pool stands");
    }

    // what this catches: the room's terms read with the wrong sign. Displaced pages are owed, a
    // replaced engine's bytes come back, and a host displacing more than it has free has no room.
    #[test]
    fn the_room_is_available_plus_released_less_displaced() {
        assert_eq!(room(52 * GB, 7 * GB, 0).bytes(), 45 * GB);
        assert_eq!(room(10 * GB, 7 * GB, 30 * GB).bytes(), 33 * GB, "the engine being replaced returns its bytes");
        assert_eq!(room(GB, 34 * GB, 0).bytes(), 0, "a starved host has no room");
    }

    // what this catches: a demand that fits shrunk for slack, an unknown host read as a reason to
    // leave minds dark, and a bounded pool below one prompt per sequence.
    #[test]
    fn the_pool_is_the_demand_until_the_host_bounds_it() {
        let fp = crate::inference::planning::serving_plan::footprint::footprint_from_parts("m", 20 * GB, 262_144, true, None).expect("test: a footprint");
        let host = |r| Rooms { host: Some(r), device: None };
        assert_eq!(pool_fit(100_000, Some(&fp), Rooms { host: None, device: None }, 0, 5, 262_144).pool(5), 100_000, "no reading: the demand stands");
        assert!(matches!(pool_fit(100_000, Some(&fp), host(room(60 * GB, 0, 0)), 0, 5, 262_144), PoolFit::Fits { .. }));
        let starved = pool_fit(100_000, Some(&fp), host(room(GB, 0, 0)), 0, 5, 262_144);
        assert!(matches!(starved, PoolFit::Bounded { fit: 0, by: Bound::Host(_), .. }));
        assert_eq!(starved.pool(5), 5 * u64::from(MIN_SERVE_CTX), "never below one least window per sequence");
    }

    // what this catches (the M5, 2026-11-14 01:22Z: the embedder's 186,908,672 B lease refused at
    // VRAM available 0 beside a 623k pool): a pool that fits host RAM but not the device's governed
    // working set. The smaller room bounds it, and says which.
    #[test]
    fn the_device_room_bounds_a_pool_the_host_would_hold() {
        let fp = crate::inference::planning::serving_plan::footprint::footprint_from_parts("m", 20 * GB, 262_144, true, None).expect("test: a footprint");
        let rooms = Rooms { host: Some(room(60 * GB, 0, 0)), device: Some(25 * GB) };
        let fit = pool_fit(1_000_000, Some(&fp), rooms, 0, 5, 262_144);
        assert!(matches!(fit, PoolFit::Bounded { by: Bound::Device { room }, .. } if room == 25 * GB), "{fit:?}");
        let host_only = pool_fit(1_000_000, Some(&fp), Rooms { device: None, ..rooms }, 0, 5, 262_144);
        assert!(fit.pool(5) < host_only.pool(5), "the device's room is the smaller");
    }
}
