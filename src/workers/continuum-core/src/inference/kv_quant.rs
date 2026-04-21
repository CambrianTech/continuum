//! Per-residency KV-cache quantization policy.
//!
//! Different lifecycle stages have different binding constraints:
//!   - Active hot in GPU: latency dominates → F16/F16 (no per-token dequant)
//!   - CpuResident (warm, in CPU unified): RAM tight, latency moderate
//!     → Q8_0/F16 (1.33x compression, V stays high precision for fast resume)
//!   - Idle (spilled to NVMe): file size + write speed dominates
//!     → Q8_0/Q8_0 or Q4_0/Q8_0 (smaller spill files, faster NVMe writes)
//!
//! K is more robust to quantization than V (V errors compound through
//! attention). Standard recommendation: K=Q8_0/V=F16 sweet spot,
//! Q4 only when memory is the binding constraint.
//!
//! The policy is data — declared by the caller (recipe author / persona /
//! adapter user), consumed by the adapter at residency transitions. Per
//! the OOP-adapter rule (CLAUDE.md "compression principle"): one decision
//! lives in one place.
//!
//! See docs/architecture/PERSONA-CONTEXT-PAGING.md §16 for the full design.

use llama::KvCacheType;
use serde::{Deserialize, Serialize};

/// Where a sequence's KV state currently lives. Drives the choice of
/// quant for that sequence — the policy is residency-tier-indexed.
///
/// New variants land here as the paging design matures (§3-4 of the doc).
/// Current variants cover the immediate-term lifecycle. `Cold` (no KV
/// state at all) doesn't appear here because there's no KV to quantize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Residency {
    /// KV pages live in GPU memory. Inference is immediate.
    Active,
    /// KV pages live in CPU/unified memory. Cheap GPU→CPU transition
    /// on Apple Silicon (unified memory); requires a small upload to
    /// re-promote to Active. Acts as the L2 between Active and Idle.
    CpuResident,
    /// KV pages spilled to NVMe via the backend's spill primitive.
    /// Resume cost: ~bytes / NVMe_bandwidth (M5 Pro: ~14 GB/s ≈ 1.7s
    /// per 24 GB). Smaller spill = faster resume, hence aggressive quant.
    Idle,
}

/// Per-residency-tier KV quantization choice. K and V are independent
/// (K tolerates aggressive quant better than V).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvCachePair {
    pub k: KvCacheType,
    pub v: KvCacheType,
}

impl KvCachePair {
    pub const fn new(k: KvCacheType, v: KvCacheType) -> Self {
        Self { k, v }
    }
}

/// The policy: which quant to use at each residency tier. Default values
/// match the recommendations from §16.2 of the paging design doc — each
/// chosen for the binding constraint of its tier.
///
/// Custom policies override per-recipe (a long-context coding task that
/// needs precise long-range recall might force F16/F16 even when spilled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvQuantPolicy {
    pub active: KvCachePair,
    pub cpu_resident: KvCachePair,
    pub spilled: KvCachePair,
}

impl Default for KvQuantPolicy {
    fn default() -> Self {
        Self {
            // Active: max decode tok/s. No dequant cost in hot path.
            // F16/F16 measured fastest on M5 Pro (47.5 vs 44 tok/s with
            // K=Q8_0) — see comment in inference/backends/llamacpp.rs:82.
            active: KvCachePair::new(KvCacheType::F16, KvCacheType::F16),
            // CpuResident: 1.33x compression, V stays high precision so
            // re-promotion to Active doesn't lose quality.
            cpu_resident: KvCachePair::new(KvCacheType::Q8_0, KvCacheType::F16),
            // Spilled: file size dominates. Both K and V quantized;
            // ~halves the spill file vs F16/F16 → halves NVMe write time
            // and storage footprint for idle slots.
            spilled: KvCachePair::new(KvCacheType::Q8_0, KvCacheType::Q8_0),
        }
    }
}

impl KvQuantPolicy {
    /// Look up the quant pair for a given residency tier.
    ///
    /// Pure function. Used by the adapter when transitioning a sequence
    /// between tiers (which is currently only Active for the first
    /// implementation; CpuResident and Idle land with the paging substrate
    /// in Phase 3.x).
    pub fn for_residency(&self, residency: Residency) -> KvCachePair {
        match residency {
            Residency::Active => self.active,
            Residency::CpuResident => self.cpu_resident,
            Residency::Idle => self.spilled,
        }
    }

    /// Caller-side override for the Active tier. Most common reason to
    /// set this: a recipe needs Q8/F16 active (small memory savings vs
    /// minor decode latency cost) because it's running 5+ personas
    /// simultaneously and even Active needs to be compact.
    pub fn with_active(mut self, k: KvCacheType, v: KvCacheType) -> Self {
        self.active = KvCachePair::new(k, v);
        self
    }

    /// Caller-side override for the CpuResident tier.
    pub fn with_cpu_resident(mut self, k: KvCacheType, v: KvCacheType) -> Self {
        self.cpu_resident = KvCachePair::new(k, v);
        self
    }

    /// Caller-side override for the Spilled tier.
    pub fn with_spilled(mut self, k: KvCacheType, v: KvCacheType) -> Self {
        self.spilled = KvCachePair::new(k, v);
        self
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: regression in the default policy (someone
    /// changes Active to Q8_0 thinking it's a memory win without
    /// realizing the per-token dequant cost on M5 Pro is measurable).
    /// The defaults are documented choices grounded in measurement;
    /// changing them requires updating §16.2 of the design doc.
    ///
    /// Validated 2026-04-21: changed default::active to Q8_0/Q8_0,
    /// test fails with "Active default should be F16/F16"; reverted,
    /// passes.
    #[test]
    fn default_active_is_f16_f16_for_max_decode_speed() {
        let p = KvQuantPolicy::default();
        assert_eq!(
            p.active,
            KvCachePair::new(KvCacheType::F16, KvCacheType::F16),
            "Active default should be F16/F16 — minimum dequant cost in hot path"
        );
    }

    /// What this catches: regression in CpuResident default. The K=Q8_0
    /// is the 1.33x compression sweet spot; V=F16 protects the resume
    /// quality (V is more sensitive than K).
    ///
    /// Validated 2026-04-21: changed V to Q8_0, test fails with reason;
    /// reverted, passes.
    #[test]
    fn default_cpu_resident_is_q8k_f16v_for_compression_with_quality() {
        let p = KvQuantPolicy::default();
        assert_eq!(
            p.cpu_resident,
            KvCachePair::new(KvCacheType::Q8_0, KvCacheType::F16),
            "CpuResident default should be Q8_0/F16 — compress K, protect V"
        );
    }

    /// What this catches: regression in Spilled default. Both K and V
    /// quantized because the binding constraint is spill file size,
    /// not in-memory compute speed. ~halves NVMe write time vs F16.
    ///
    /// Validated 2026-04-21: changed K to F16, test fails; reverted, passes.
    #[test]
    fn default_spilled_is_q8_q8_for_minimum_file_size() {
        let p = KvQuantPolicy::default();
        assert_eq!(
            p.spilled,
            KvCachePair::new(KvCacheType::Q8_0, KvCacheType::Q8_0),
            "Spilled default should be Q8_0/Q8_0 — file size is the binding constraint"
        );
    }

    /// What this catches: bug where for_residency returns the wrong
    /// pair for a tier (e.g., off-by-one in the match arm). Each
    /// residency MUST round-trip to its declared pair.
    ///
    /// Validated 2026-04-21: swapped match arms (Active → returns spilled);
    /// each individual assertion fails with the wrong-tier value visible
    /// in the diff; reverted, all pass.
    #[test]
    fn for_residency_dispatches_to_the_correct_tier() {
        let p = KvQuantPolicy::default();
        assert_eq!(p.for_residency(Residency::Active), p.active);
        assert_eq!(p.for_residency(Residency::CpuResident), p.cpu_resident);
        assert_eq!(p.for_residency(Residency::Idle), p.spilled);
    }

    /// What this catches: builder methods (with_active / with_cpu_resident
    /// / with_spilled) silently dropping the override (e.g., assigning to
    /// the wrong field). Each builder must affect ONLY its tier.
    ///
    /// Validated 2026-04-21: made with_active assign to self.spilled;
    /// test fails with active still default. Reverted, passes.
    #[test]
    fn builders_modify_only_their_target_tier() {
        let custom = KvQuantPolicy::default()
            .with_active(KvCacheType::Q8_0, KvCacheType::Q8_0);

        assert_eq!(custom.active, KvCachePair::new(KvCacheType::Q8_0, KvCacheType::Q8_0));
        // Other tiers unchanged from default
        assert_eq!(custom.cpu_resident, KvQuantPolicy::default().cpu_resident);
        assert_eq!(custom.spilled, KvQuantPolicy::default().spilled);

        let custom2 = KvQuantPolicy::default()
            .with_cpu_resident(KvCacheType::F16, KvCacheType::F16);
        assert_eq!(custom2.cpu_resident, KvCachePair::new(KvCacheType::F16, KvCacheType::F16));
        assert_eq!(custom2.active, KvQuantPolicy::default().active);
        assert_eq!(custom2.spilled, KvQuantPolicy::default().spilled);

        let custom3 = KvQuantPolicy::default()
            .with_spilled(KvCacheType::F16, KvCacheType::F16);
        assert_eq!(custom3.spilled, KvCachePair::new(KvCacheType::F16, KvCacheType::F16));
        assert_eq!(custom3.active, KvQuantPolicy::default().active);
        assert_eq!(custom3.cpu_resident, KvQuantPolicy::default().cpu_resident);
    }

    /// What this catches: future addition of a Residency variant
    /// (e.g., NetworkSpill for tiered storage in Phase 6.0) where
    /// for_residency forgets to handle it. Rust's exhaustive match
    /// already protects this at compile time, but this test documents
    /// the intent: every Residency variant MUST map to a quant pair.
    ///
    /// Validated 2026-04-21: added an unreachable variant in dev,
    /// build fails (good — exhaustive match catches it); reverted.
    #[test]
    fn every_residency_variant_resolves_to_a_quant_pair() {
        let p = KvQuantPolicy::default();
        // The exhaustive match in for_residency is the structural
        // guarantee. This test exists to flag the intent for code
        // reviewers: any new Residency variant MUST be handled.
        let _ = p.for_residency(Residency::Active);
        let _ = p.for_residency(Residency::CpuResident);
        let _ = p.for_residency(Residency::Idle);
    }
}
