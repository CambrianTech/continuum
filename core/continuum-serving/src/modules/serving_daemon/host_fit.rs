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
//! embeddings lost relevance). Weights, KV and compute must also fit the device. Steady (the board
//! holds serving's own bytes, `OwnCredit::Included`), the room is the governed replace-myself
//! budget, which nets every other consumer's floor. At a boot or a gap it is computed from the
//! other consumers' claims (`device_budget::boot_device_room`), never read off a board still
//! carrying a leaving engine: the 03:35Z boot the next night sized with no device room and the
//! embed lane was refused for the rest of the hour.

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
    pub(super) fn pool(self, sequences: u32, minimum_window: u32) -> u64 {
        match self {
            Self::Unmeasured { demand } | Self::Fits { demand, .. } => demand,
            Self::Bounded { demand, fit, .. } => fit.max(u64::from(minimum_window) * u64::from(sequences)).min(demand),
        }
    }
}

/// The rooms a launch fits into, each `None` when not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rooms {
    pub(super) host: Option<HostRoom>,
    /// The device room in bytes: the governed budget when it holds serving's own bytes, else the
    /// capacity beside every other consumer's claim; `None` while the pool has not reported.
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

/// The Core's streams: the most minds, up to `roster`, whose whole trained windows all fit every room read, so the
/// engine decodes them in one step (THE-LANE-ON-SAGE.md, open question 1); another stream is held only for another
/// whole window, never by dividing one. One when not even two whole windows fit, or no room was read to say so.
/// How many streams the Core holds, and the whole window each holds: the window it serves (the footprint's verified
/// window, at most the trained one), never a smaller share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WholeWindows {
    pub(super) streams: u32,
    pub(super) window: u32,
}

impl Rooms {
    pub(super) fn whole_window_streams(self, footprint: Option<&ModelFootprint>, host_cache_bytes: u64, roster: u32, trained: u32) -> WholeWindows {
        // a footprint with no measured KV a token prices no stream: window_within answers the whole context for it
        // before charging the weights or a stream's workspace (Codex on 8aab69b666), so it holds one
        let Some(footprint) = footprint.filter(|footprint| footprint.kv_per_token > 0) else { return WholeWindows { streams: 1, window: trained } };
        // the window a stream serves: the verified one where it sits below the trained (Codex on 8aab69b666: demand at
        // the trained window against a fit capped at the verified one never qualified a second stream)
        let window = trained.min(footprint.context_window);
        let fits = |streams: u32| matches!(pool_fit(u64::from(streams) * u64::from(window), Some(footprint), self, host_cache_bytes, streams, window), PoolFit::Fits { .. });
        let streams = (2..=roster).rev().find(|&streams| fits(streams)).unwrap_or(1); // unwrap_or: no second whole window fits, so the Core holds its one stream
        WholeWindows { streams, window }
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
        assert_eq!(pool_fit(100_000, Some(&fp), Rooms { host: None, device: None }, 0, 5, 262_144).pool(5, MIN_SERVE_CTX), 100_000, "no reading: the demand stands");
        assert!(matches!(pool_fit(100_000, Some(&fp), host(room(60 * GB, 0, 0)), 0, 5, 262_144), PoolFit::Fits { .. }));
        let starved = pool_fit(100_000, Some(&fp), host(room(GB, 0, 0)), 0, 5, 262_144);
        assert!(matches!(starved, PoolFit::Bounded { fit: 0, by: Bound::Host(_), .. }));
        assert_eq!(starved.pool(5, MIN_SERVE_CTX), 5 * u64::from(MIN_SERVE_CTX), "never below one least window per sequence");
    }

    // what this catches: the Core charged for a second stream it does not hold (2026-11-21: two
    // streams' fixed bytes, and half the context each). Its one stream's fixed bytes are charged once:
    // a room below them fits nothing, and a room the old second stream would have starved now fits.
    #[test]
    fn native_pool_charges_the_fixed_bytes_of_its_one_stream() {
        use crate::inference::llama_server::StreamLayout;
        let fp = crate::inference::planning::serving_plan::footprint::footprint_from_parts("m", 20 * GB, 262_144, true, None).expect("test: a footprint");
        let layout = StreamLayout::PersonaScheduler;
        let sequences = layout.sequences(1);
        let pool_in = |room| pool_fit(48_000, Some(&fp), Rooms { host: None, device: Some(room) }, 0, sequences, fp.context_window).pool(sequences, layout.minimum_window(MIN_SERVE_CTX));
        assert_eq!(pool_in(fp.weights_bytes + fp.compute_buffer_per_lane() / 2), 0, "below one stream's fixed bytes");
        assert!(pool_in(fp.weights_bytes + fp.compute_buffer_per_lane() * 3 / 2) > 0, "one stream's fixed bytes, and room for her tokens");
    }

    // what this catches (S6): batching that divides a mind's window to seat another (2026-11-21: two streams halved
    // the Core's context and seven turns died at the half's end), or one admitted without the memory for it. A stream
    // beyond the first is held only when a whole trained window more fits the room, up to the roster; with no room
    // read, the Core keeps one.
    #[test]
    fn the_core_holds_another_stream_only_for_another_whole_window() {
        let fp = crate::inference::planning::serving_plan::footprint::footprint_from_parts("m", 20 * GB, 262_144, true, None).expect("test: a footprint");
        let room_for = |windows: u32| fp.weights_bytes + u64::from(windows) * (fp.compute_buffer_per_lane() + fp.kv_per_token * 262_144 * 2);
        let streams = |room: Option<u64>, roster| Rooms { host: None, device: room }.whole_window_streams(Some(&fp), 0, roster, fp.context_window).streams;
        assert_eq!(streams(None, 5), 1, "no room read: one stream");
        assert_eq!(streams(Some(fp.weights_bytes + fp.compute_buffer_per_lane()), 5), 1, "not two whole windows: one stream");
        let four = streams(Some(room_for(4)), 5);
        assert!((2..=5).contains(&four), "room for about four whole windows holds several streams: {four}");
        assert!(matches!(pool_fit(u64::from(four) * 262_144, Some(&fp), Rooms { host: None, device: Some(room_for(4)) }, 0, four, 262_144), PoolFit::Fits { .. }), "and every one of them whole");
        assert_eq!(streams(Some(room_for(8)), 2), 2, "never more than the roster");
        // what this catches (Codex on 8aab69b666): a footprint priced at zero KV a token, whose window_within answers
        // the whole context before charging anything, admitting the whole roster in any room; and a room that holds
        // several streams' fixed workspace but not their windows
        let mut free = fp.clone();
        free.kv_per_token = 0;
        assert_eq!(Rooms { host: None, device: Some(room_for(8)) }.whole_window_streams(Some(&free), 0, 5, fp.context_window).streams, 1, "zero KV a token prices no stream");
        assert_eq!(Rooms { host: None, device: Some(room_for(8)) }.whole_window_streams(None, 0, 5, fp.context_window).streams, 1, "no footprint, no second stream");
        let workspace_only = fp.weights_bytes + 5 * fp.compute_buffer_per_lane();
        assert_eq!(streams(Some(workspace_only), 5), 1, "fixed cost for five, windows for none: one stream");
        // a verified window below the trained one: whole windows are the verified ones, so ample memory holds several
        let mut verified = fp.clone();
        verified.context_window = 131_072;
        let held = Rooms { host: None, device: Some(room_for(8)) }.whole_window_streams(Some(&verified), 0, 5, 262_144);
        assert_eq!(held.window, 131_072, "each stream holds the verified window");
        assert!(held.streams > 1, "and the room holds more than one of them: {held:?}");
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
        assert!(fit.pool(5, MIN_SERVE_CTX) < host_only.pool(5, MIN_SERVE_CTX), "the device's room is the smaller");
    }
}
