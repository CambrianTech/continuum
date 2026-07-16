//! Capacity fabric — the live, never-block, grid-elastic compute allocator.
//!
//! Design: `docs/architecture/CAPACITY-FABRIC-AND-GOVERNOR.md`. This module SEEDS it:
//! the deterministic **simulator** (= VDD gate = training gym) and the allocation-fit that
//! reproduces-and-kills the 2026-07-16 compute-buffer OOM WITHOUT hardware.
//!
//! The one invariant: usable compute is a LIVE, ever-changing quantity — never a fact
//! established at init. Consumers hold it loosely; the allocator's grant is DERIVED from
//! the live snapshot, never a constant. Tonight's OOM was a static reserve blind to live
//! external GPU pressure (a game/browser); this module is the shape that makes that
//! impossible AND the thing that would have caught it in CI, deterministically.
//!
//! ## The RANSAC shape (why it's infinitely extensible)
//! Many competing considerations (latency, coding quality, avatar smoothness, thrash, OOM)
//! fold into ONE scalar [`Score`]. The [`AllocationPolicy`] optimizer — deterministic now,
//! learned/persona later — only sees that scalar. Add considerations to the score; the
//! optimizer stays untouched and swappable. The score is the contract.
//!
//! ## Sim == prod (the trust property)
//! The allocator reads a [`DeviceCapacity`] it can't trace the origin of. In prod it comes
//! from live Metal/CUDA probes + airc gossip; in the [`sim`] it comes from a scenario
//! timeline played on a virtual clock. Same allocator, swapped world — so a sim scenario
//! IS a real regression test.

pub mod consumer;
pub mod score;
pub mod sim;

/// A LIVE reading of one device's usable compute, external consumers already subtracted.
/// NOT a boot classification — re-taken continuously. `gpu_free_bytes_live` is the
/// load-bearing number: what is free THIS INSTANT after the model's residency, the OS, and
/// consumers we don't own (a game, the browser).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCapacity {
    /// Total GPU / UMA-serving-slice bytes on this device.
    pub gpu_total_bytes: u64,
    /// Free GPU bytes RIGHT NOW — after weights + KV residency AND external (unowned) load.
    pub gpu_free_bytes_live: u64,
    /// Free system RAM — the CPU-serve fallback budget (the 4GB-Radeon path).
    pub system_ram_free_bytes: u64,
}

/// One consumer's ask for concurrent execution. The concrete quantity the OOM turned on:
/// how many prefill spikes may run at once, given each spike's transient compute-buffer
/// cost. `want` is the ideal (≈ resident persona count); the grant fits it to live free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseRequest {
    pub consumer: String,
    /// Ideal concurrent prefill spikes (each a persona wanting to run at once).
    pub want_concurrency: u32,
    /// Transient compute-buffer bytes ONE concurrent prefill spike draws from free GPU.
    /// The window-scaled, MEASURED term (calibrated from the benchmark ledger later) — the
    /// thing tonight's static `weights/16` reserve got wrong. In the sim it is a scenario
    /// parameter so we can probe the whole range.
    pub spike_bytes: u64,
}

/// What the allocator grants for THIS snapshot. Derived, never a constant; re-granted when
/// the snapshot changes (shrink OR grow). `concurrency` is the safety valve that OOM'd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub concurrency: u32,
}

/// The RANSAC-style objective: many considerations collapsed to scalars an optimizer fits.
/// `oom_count` is the hard-fail; `mean_experience` is the perception reward the gym maximizes
/// (see [`consumer::QualityModel`]); the design's remaining metrics (avatar dropped-frames,
/// coding pass-rate-under-budget, fairness) land here as more scalars without changing the
/// [`AllocationPolicy`] interface.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Score {
    /// Times a granted concurrency exceeded what live free GPU could hold. Hard-fail: any
    /// OOM makes a policy unacceptable regardless of every other metric.
    pub oom_count: u32,
    /// Times the grant changed — the thrash signal (cheap knob may flex; expensive ones
    /// need hysteresis). Zero considerations optimize on it yet; the slot exists.
    pub grant_changes: u32,
    /// Mean per-tick experience score (0..1) from the consumer's [`consumer::QualityModel`] —
    /// the perceived-quality reward. THIS is what a learned policy climbs: a policy that sheds
    /// load to stay responsive beats one that holds and crashes, because a crash zeroes the
    /// experience via the critical-faculty gate. Higher is better.
    pub mean_experience: f32,
}

/// The optimizer seam. Deterministic bootstrap now; a learned net or a persona-in-charge
/// later — all see only the [`DeviceCapacity`] + the [`LeaseRequest`] and emit a [`Grant`].
/// Swapping the optimizer never touches the world model or the score.
pub trait AllocationPolicy: Send + Sync {
    fn grant(&self, cap: &DeviceCapacity, req: &LeaseRequest) -> Grant;
    /// Name for scenario reports / the training ledger.
    fn name(&self) -> &'static str;
}

/// The OOM in a policy: grant concurrency by persona count, blind to live capacity — the
/// exact shape of the pre-fix `MAX_LANES`-style static reserve. Kept as the negative
/// control (outlier the fit policy must beat) so the sim proves the fix, not just asserts it.
pub struct StaticConcurrencyPolicy {
    pub fixed: u32,
}

impl AllocationPolicy for StaticConcurrencyPolicy {
    fn grant(&self, _cap: &DeviceCapacity, req: &LeaseRequest) -> Grant {
        // Blind to `cap` — the bug. Grants the ideal regardless of what's free RIGHT NOW.
        Grant { concurrency: req.want_concurrency.min(self.fixed).max(1) }
    }
    fn name(&self) -> &'static str {
        "static-concurrency"
    }
}

/// Deterministic bootstrap: concurrency = how many spikes fit live free GPU after a safety
/// margin. Derived from the LIVE snapshot every call, so shrink (game opens) and grow (game
/// closes) both fall out for free. This is the fit that kills the OOM.
pub struct FitPolicy {
    /// Reserve kept free below `gpu_free_bytes_live` — headroom for measurement error and
    /// unowned jitter. Derived from the device budget by the caller, not a global constant.
    pub safety_margin_bytes: u64,
}

impl AllocationPolicy for FitPolicy {
    fn grant(&self, cap: &DeviceCapacity, req: &LeaseRequest) -> Grant {
        let usable = cap.gpu_free_bytes_live.saturating_sub(self.safety_margin_bytes);
        let fits = if req.spike_bytes == 0 {
            req.want_concurrency
        } else {
            (usable / req.spike_bytes) as u32
        };
        // Never below 1: a loaded model must be able to run at least one prefill (else the
        // model shouldn't have been resident — that's a residency decision, not a
        // concurrency one). Never above what the mind actually demands.
        Grant {
            concurrency: fits.clamp(1, req.want_concurrency.max(1)),
        }
    }
    fn name(&self) -> &'static str {
        "fit"
    }
}

/// True when a grant would overflow the live free GPU — the OOM condition, in the sim.
/// `concurrency` transient spikes of `spike_bytes` each must fit `gpu_free_bytes_live`.
pub fn grant_would_oom(cap: &DeviceCapacity, req: &LeaseRequest, grant: &Grant) -> bool {
    (grant.concurrency as u64).saturating_mul(req.spike_bytes) > cap.gpu_free_bytes_live
}
