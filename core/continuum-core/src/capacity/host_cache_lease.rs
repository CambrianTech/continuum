//! The governed host-cache lease — #287, the derivation behind
//! `plan_file.budget_bytes` (docs/architecture/MOE-SERVING-GOVERNED-BUDGET.md,
//! seam answered in-doc 2026-08-01; EXPERT-PAGING-CONTROL-LAW.md §5).
//!
//! One principle: the governor owns the budget, nothing else touches it.
//! The pinned expert cache gets what remains of physical RAM after the
//! serve's REAL working set — and that working set is a LIVE quantity:
//! KV grows as slots fill, so this derivation runs on the governor tick,
//! never once at spawn. Both halves of the 2026-08-01 incident are
//! encoded here as arithmetic:
//!
//! - The hardcoded-40GB overcommit (95.9GB commit on a 63GB box → 33GB
//!   pagefile → fetch collapsed 2.5GB/s→205MB/s): the lease is derived,
//!   and commit can never exceed physical (the pagefile thrashes
//!   SILENTLY instead of OOMing loud, so the ceiling must be explicit).
//! - The static-budget-ignoring-KV-growth thrash (her corrected
//!   measurement): governed flex holding total "within a GB" yields
//!   ~6GB pinned ≈ 1 token's set ≈ ~40% retention with misses at the
//!   recovered 2.5GB/s — retention is exactly what the flex buys.
//!
//! The mmap subtlety (why no input here reads the free-memory monitor
//! for the weights term): file-backed mmap'd weight pages are reported
//! "available" by the OS while they are load-bearing — evicting them
//! re-fetches weights, which IS the bandwidth collapse wearing a
//! healthy free-RAM number. The weights term is therefore explicit
//! (the planner's `weights_bytes`), never inferred.

/// Inputs to one lease derivation — every term explicit, none read from
/// a free-memory monitor at this layer (the CALLER samples live values
/// on its tick and passes them; this stays a pure, replayable law).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostCacheLeaseInputs {
    /// Total physical RAM (raw, not the conservative API figure — the
    /// #216 lesson).
    pub physical_bytes: u64,
    /// The served model's host-resident weight footprint (mmap; the
    /// planner's `weights_bytes` minus any device-offloaded share).
    /// Explicit — see the module doc for why this must never come from
    /// the free-memory monitor.
    pub weights_host_bytes: u64,
    /// LIVE total KV bytes — `kv_per_token × served_ctx × lanes`, read
    /// from serving state on each tick because it GROWS as slots fill
    /// (the term whose omission thrashed the 63GB box to 64GB).
    pub live_kv_bytes: u64,
    /// Compute/scratch buffers the serve holds (ubatch buffers etc.).
    pub compute_buffer_bytes: u64,
    /// What the OS + everything-else-running needs to keep breathing.
    /// A floor the lease never eats into.
    pub os_floor_bytes: u64,
    /// Windows only: current total commit charge of the system, when
    /// known. The lease is additionally clamped so commit + lease never
    /// exceeds physical — pagefile overcommit thrashes silently instead
    /// of failing loud, so the ceiling must be enforced here. `None` on
    /// platforms where commit is not the binding regime (macOS/Linux).
    pub commit_charge_bytes: Option<u64>,
}

/// The lease law: what remains of physical after the live working set,
/// clamped by the commit ceiling. Saturating throughout — an over-full
/// box yields a ZERO lease (cache off, serve survives), never a wrap.
pub fn host_cache_lease_bytes(i: &HostCacheLeaseInputs) -> u64 {
    let working_set = i
        .weights_host_bytes
        .saturating_add(i.live_kv_bytes)
        .saturating_add(i.compute_buffer_bytes)
        .saturating_add(i.os_floor_bytes);
    let headroom = i.physical_bytes.saturating_sub(working_set);
    match i.commit_charge_bytes {
        // Commit regime: the lease may also never push total commit past
        // physical. Take the tighter of the two bounds.
        Some(commit) => headroom.min(i.physical_bytes.saturating_sub(commit)),
        None => headroom,
    }
}

/// Sticky publication of the lease — the never-thrash layer. The raw
/// derivation flutters with every KV sample; the PUBLISHED budget moves
/// only when the change is material, and shrinks faster than it grows:
///
/// - SHRINK immediately when the new lease is materially below the
///   published value (safety: overcommit risk is now).
/// - GROW only when the new lease exceeds the published value by the
///   hysteresis band (comfort: reclaiming cache costs evictions, so a
///   transient dip's recovery must not oscillate the pool). This is the
///   #214 grow-back lesson: recompute UP as well as down, but sticky.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StickyLease {
    published_bytes: u64,
    /// Band as 1/N of the published value (N=8 → move on >12.5% change).
    band_divisor: u64,
}

impl StickyLease {
    pub fn new(band_divisor: u64) -> Self {
        Self {
            published_bytes: 0,
            band_divisor: band_divisor.max(1),
        }
    }

    pub fn published_bytes(&self) -> u64 {
        self.published_bytes
    }

    /// Fold one derived lease in; returns `Some(new_published)` when the
    /// published value changed (the caller writes the plan file), `None`
    /// when it held (no write, no mtime churn on her per-token poll).
    pub fn observe(&mut self, derived_bytes: u64) -> Option<u64> {
        let band = self.published_bytes / self.band_divisor;
        let changed = if derived_bytes < self.published_bytes {
            // Material shrink → act now. Sub-band jitter holds.
            self.published_bytes.saturating_sub(derived_bytes) > band
        } else {
            derived_bytes.saturating_sub(self.published_bytes) > band
        };
        if changed {
            self.published_bytes = derived_bytes;
            Some(derived_bytes)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    fn her_box(live_kv: u64) -> HostCacheLeaseInputs {
        // The 63GB/5090 shape from the 2026-08-01 measurements.
        HostCacheLeaseInputs {
            physical_bytes: 63 * GB,
            weights_host_bytes: 40 * GB,
            live_kv_bytes: live_kv,
            compute_buffer_bytes: 4 * GB,
            os_floor_bytes: 6 * GB,
            commit_charge_bytes: None,
        }
    }

    /// what this catches (#287 the law itself): the lease is physical
    /// minus the LIVE working set — on the incident box shape it lands
    /// in the ~6GB-class band her corrected measurement predicts, and
    /// KV growth SHRINKS it (the term whose omission caused the
    /// 64GB thrash).
    #[test]
    fn lease_tracks_live_kv_growth() {
        let cold = host_cache_lease_bytes(&her_box(4 * GB));
        let warm = host_cache_lease_bytes(&her_box(8 * GB));
        assert_eq!(cold, 9 * GB);
        assert_eq!(warm, 5 * GB);
        assert!(warm < cold, "KV growth must shrink the lease");
    }

    /// what this catches: the Windows commit ceiling — a lease that fits
    /// free RAM but would push commit past physical is clamped (pagefile
    /// overcommit thrashes silently; 95.9GB-commit-on-63GB is the
    /// incident this pins). And an over-committed box leases ZERO,
    /// never wraps.
    #[test]
    fn commit_ceiling_clamps_and_saturates() {
        let mut i = her_box(4 * GB);
        i.commit_charge_bytes = Some(60 * GB);
        assert_eq!(host_cache_lease_bytes(&i), 3 * GB, "commit-bound, not headroom-bound");
        i.commit_charge_bytes = Some(70 * GB);
        assert_eq!(host_cache_lease_bytes(&i), 0, "past-physical commit → zero lease");
        i.commit_charge_bytes = None;
        i.live_kv_bytes = 200 * GB;
        assert_eq!(host_cache_lease_bytes(&i), 0, "over-full working set saturates to zero");
    }

    /// what this catches (never-thrash + #214 grow-back): sub-band
    /// jitter never republishes (no plan-file mtime churn on her
    /// per-token poll); a material shrink publishes immediately; growth
    /// DOES come back (not frozen at the floor) once past the band.
    #[test]
    fn sticky_lease_holds_jitter_moves_on_material_change() {
        let mut s = StickyLease::new(8);
        assert_eq!(s.observe(8 * GB), Some(8 * GB), "first observation publishes");
        assert_eq!(s.observe(8 * GB + GB / 2), None, "sub-band growth holds");
        assert_eq!(s.observe(8 * GB - GB / 2), None, "sub-band shrink holds");
        assert_eq!(s.observe(5 * GB), Some(5 * GB), "material shrink publishes now");
        assert_eq!(s.observe(9 * GB), Some(9 * GB), "grow-back publishes past the band");
        assert_eq!(s.published_bytes(), 9 * GB);
    }
}
