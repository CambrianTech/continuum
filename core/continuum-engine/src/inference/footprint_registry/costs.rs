//! Spill / reload cost heuristics per `ResourceType`.
//!
//! Isolated into its own module so the cost model — which the eviction
//! policy depends on for "what's cheapest to spill" decisions — has its
//! own visible surface and its own tests. When Phase 4.0 telemetry lands
//! and we start refining these from real measurements, this is the file
//! to edit.
//!
//! Why split out:
//!
//! - **Policy invariants are testable.** The eviction algorithm assumes
//!   relative orderings ("KV is cheaper to spill than ModelWeights",
//!   "TokenizerCache is effectively un-evictable"). With the heuristic
//!   in its own module those invariants get explicit tests instead of
//!   being implicit in the eviction integration tests.
//!
//! - **Future replacement is clean.** When real measurements replace
//!   heuristics, only this file changes — the registry's behavior tests
//!   stay untouched because the cost contract (returns spill_us +
//!   reload_us) doesn't change.
//!
//! See §13.4 of `docs/architecture/PERSONA-CONTEXT-PAGING.md` for the
//! design context behind these initial estimates.

use super::types::ResourceType;

/// Default spill/reload cost heuristics keyed on resource type. Returns
/// `(spill_micros, reload_micros)`. Used by `FootprintEntry::new` for the
/// initial cost estimate when a backend hasn't yet supplied measurements.
///
/// **Invariants the eviction policy depends on** (locked in by tests):
///
/// - `KvCache.spill < ModelWeights.spill` — KV is the right thing to evict
///   first under pressure; model weights are last.
/// - `LoraAdapter.spill == 0` — adapters aren't really spilled, they're
///   discarded and re-downloaded; the "spill" concept is a no-op for them.
/// - `TokenizerCache.spill > KvCache.spill * 1000` — tokenizer should
///   never appear in eviction plans; the absurd cost reflects its "permanent"
///   status.
pub(super) fn default_costs_for(resource_type: &ResourceType, bytes: u64) -> (u64, u64) {
    // NVMe write/read: ~1 GB/s sustained on M5 (conservative; real PCIe5
    // hits 14 GB/s but we account for overhead). bytes/1_000 = micros.
    let nvme_micros = bytes / 1_000;
    // GPU upload from CPU: ~5 GB/s on Apple Silicon unified memory.
    let gpu_upload_micros = bytes / 5_000;

    match resource_type {
        ResourceType::KvCache => (
            nvme_micros,                     // spill: raw write
            nvme_micros + gpu_upload_micros, // reload: read + GPU upload
        ),
        ResourceType::LoraAdapter => (
            // Adapters are usually cheaper to evict (re-download from
            // storage) than spill. Treat eviction cost as 0 (storage
            // is fast); reload is HF download + GPU upload.
            0,
            500_000 + gpu_upload_micros, // ~500ms HF roundtrip + upload
        ),
        ResourceType::ModelWeights => (
            // Almost never spillable in practice — model load is
            // multi-second, mmap'd from disk. Mark spill as expensive
            // so the eviction policy avoids it.
            5_000_000,               // 5 seconds (mmap teardown)
            5_000_000 + nvme_micros, // load + read
        ),
        ResourceType::RenderBuffer | ResourceType::AudioPipeline | ResourceType::VideoPipeline => {
            // Pipeline buffers — small, fast to recreate. Effectively
            // free to evict.
            (1_000, 10_000)
        }
        ResourceType::TokenizerCache => (
            // Tokenizer is small (~2MB) and mmap'd; treat as effectively
            // permanent. Spill cost set high so the policy never picks it.
            10_000_000, 10_000_000,
        ),
        ResourceType::Other(_) => (nvme_micros, nvme_micros + gpu_upload_micros),
    }
}

// ─── Tests — policy invariants ──────────────────────────────────────────
//
// These tests don't probe specific numeric values (those are heuristics
// and will change with telemetry). They probe ORDERING invariants that
// the eviction policy depends on. If future telemetry inverts one of
// these orderings, the eviction algorithm's assumptions also need to
// be revisited — a failing test here is a load-bearing signal, not noise.

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: KV cache becoming more expensive to spill than
    /// model weights. The eviction policy picks the cheapest-per-byte to
    /// evict first; if KV ever costs more than model weights, the policy
    /// would evict model weights first under pressure (catastrophic —
    /// model reload is multi-second user-visible latency vs KV reload
    /// which is hidden inside the next prefill).
    ///
    /// Validated 2026-04-21: bumped KvCache spill to 10× ModelWeights
    /// (changed nvme_micros to nvme_micros * 1000), test fails on the
    /// kv < weights assertion; reverted.
    #[test]
    fn kv_cache_spill_is_cheaper_than_model_weights() {
        let bytes = 100_000_000; // 100 MB — same size for fair comparison
        let (kv_spill, _) = default_costs_for(&ResourceType::KvCache, bytes);
        let (mw_spill, _) = default_costs_for(&ResourceType::ModelWeights, bytes);
        assert!(
            kv_spill < mw_spill,
            "KV spill ({kv_spill}us) must be cheaper than ModelWeights spill ({mw_spill}us) — \
             eviction policy depends on this ordering"
        );
    }

    /// What this catches: LoRA adapter spill cost becoming nonzero. The
    /// design treats adapters as "evict by discard, reload by re-download"
    /// — there's no actual spill operation for them. If spill > 0, the
    /// policy would account for a cost that doesn't exist and might
    /// avoid evicting an adapter when it's the right call.
    ///
    /// Validated 2026-04-21: hardcoded LoraAdapter spill to nvme_micros;
    /// test fails on assert(spill == 0); reverted.
    #[test]
    fn lora_adapter_spill_is_zero() {
        let (spill, _reload) = default_costs_for(&ResourceType::LoraAdapter, 50_000_000);
        assert_eq!(
            spill, 0,
            "LoRA adapters aren't spilled — they're discarded + re-downloaded. \
             Spill cost must be 0 to reflect that contract."
        );
    }

    /// What this catches: TokenizerCache slipping into 'evictable' cost
    /// range. Tokenizer is a few MB, mmap'd, effectively permanent — if
    /// its cost is ever cheap enough to appear in an eviction plan, the
    /// model loses its tokenizer mid-decode (catastrophic). The 1000×
    /// margin guards against future heuristic tweaks accidentally lowering
    /// it into the policy's eviction-candidate band.
    ///
    /// Validated 2026-04-21: changed TokenizerCache spill to nvme_micros
    /// (cheap), test fails on the 1000× margin assertion; reverted.
    #[test]
    fn tokenizer_cache_spill_is_effectively_unbounded() {
        let bytes = 2_000_000; // ~2 MB tokenizer
        let (tc_spill, _) = default_costs_for(&ResourceType::TokenizerCache, bytes);
        let (kv_spill, _) = default_costs_for(&ResourceType::KvCache, bytes);
        assert!(
            tc_spill > kv_spill.saturating_mul(1000),
            "TokenizerCache spill ({tc_spill}us) must dwarf KvCache spill ({kv_spill}us) \
             by ≥1000× so the eviction policy never picks it"
        );
    }

    /// What this catches: ModelWeights reload cost dropping below spill
    /// cost. Reload >= spill is a structural invariant (you can't reload
    /// faster than you spilled — both involve the same byte movement
    /// plus extra work). Useful as a sanity check that future telemetry
    /// edits don't invert this.
    ///
    /// Validated 2026-04-21: swapped spill/reload returns for ModelWeights,
    /// test fails on the spill <= reload assertion; reverted.
    #[test]
    fn reload_is_at_least_as_expensive_as_spill_for_each_type() {
        for rt in [
            ResourceType::KvCache,
            ResourceType::LoraAdapter,
            ResourceType::ModelWeights,
            ResourceType::RenderBuffer,
            ResourceType::TokenizerCache,
            ResourceType::Other("custom".to_string()),
        ] {
            let (spill, reload) = default_costs_for(&rt, 100_000_000);
            assert!(
                reload >= spill,
                "ResourceType::{rt:?}: reload ({reload}us) < spill ({spill}us) — \
                 reload should never be cheaper than spill (same bytes + extra work)"
            );
        }
    }

    /// What this catches: cost functions returning the same (spill, reload)
    /// for byte size 0 vs byte size 1MB. Costs MUST scale with bytes for
    /// the bytes-bearing types (KV, ModelWeights, custom Other) — otherwise
    /// the policy can't differentiate "evict this 1KB entry" from "evict
    /// this 1GB entry."
    ///
    /// Validated 2026-04-21: replaced bytes/1_000 with constant 1000,
    /// test fails on the inequality (zero ≠ million bytes producing
    /// different costs); reverted.
    #[test]
    fn cost_scales_with_bytes_for_size_dependent_types() {
        let (zero_spill, _) = default_costs_for(&ResourceType::KvCache, 0);
        let (mil_spill, _) = default_costs_for(&ResourceType::KvCache, 1_000_000);
        assert!(
            mil_spill > zero_spill,
            "KvCache spill should scale with bytes; 0-byte entry: {zero_spill}us, 1MB: {mil_spill}us"
        );
    }
}
