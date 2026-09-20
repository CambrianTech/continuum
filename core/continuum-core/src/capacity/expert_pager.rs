//! Expert pager — the bridge from [`SystemProfile`] to expert residency planning.
//!
//! The K3 capability-axis on-ramp: a Mixture-of-Experts model whose full weight set
//! can't fit VRAM (K3 = 896 experts, ~594 GB) serves LOCALLY by keeping the HOT expert
//! subset resident and paging the cold tail from the cold drive
//! ([`ArtifactSource::Mapped`](crate::genome::blob::ArtifactSource) laid down by
//! [`ingest_expert_sets`](crate::genome::expert_ingest::ingest_expert_sets)). WHICH
//! experts are hot is learned live from the router — M5's `ffn_moe_topk` eval-callback
//! increments [`ExpertActivationProfile::hits`]; this is the sentinel-PGO signal.
//!
//! The residency PLAN itself is already computed by
//! [`plan_expert_residency`](super::expert_residency::plan_expert_residency) (hot→VRAM,
//! warm→RAM, cold→D:). This module's ONE job is the bridge the rest of the substrate
//! was missing: plan expert residency against the **budgeted** VRAM — the same
//! `SystemProfile.serving_budget_bytes()` (0.80 `vram_headroom`, config-inherited) that
//! serving uses — instead of raw live-free VRAM. So the expert pager can never fill
//! VRAM past the operator's headroom and starve the co-consumers (serving lane, Bevy,
//! the OS); the pager and the serving daemon fit within ONE budget, not two.
//!
//! The observe→re-plan→execute control LOOP (decay, cadence, driving the genome
//! working-set swap) composes on top of this bridge and M5's `cb_eval` executor — that
//! is the joint next slice, not this one.

use super::expert_residency::{
    plan_expert_residency, ExpertActivationProfile, ExpertResidencyPlan,
};
use super::{DeviceCapacity, SystemProfile};

/// Plan expert residency against this box's BUDGETED VRAM (the shared 0.80 serving
/// headroom via [`SystemProfile::serving_budget_bytes`]), not raw live-free VRAM.
///
/// `expert_bytes` = per-expert on-device weight size (from the ingested layout —
/// [`ArtifactBlob::expert_size_bytes`](crate::genome::blob::ArtifactBlob)); `0` ⇒ the
/// planner can't size residency and warms everything (its safe cold-start). `margin_bytes`
/// is the eviction hysteresis the caller carries. RAM stays at its true live-free (the
/// warm tier isn't the contended one — VRAM is); only the VRAM tier is budgeted, which
/// is where the headroom bug bites.
pub fn plan_expert_residency_budgeted(
    profile: &SystemProfile,
    activation: &ExpertActivationProfile,
    expert_bytes: u64,
    margin_bytes: u64,
) -> ExpertResidencyPlan {
    plan_expert_residency_with_resident(profile, activation, expert_bytes, margin_bytes, 0, 0, 0)
}

/// [`plan_expert_residency_budgeted`] with SELF-OCCUPANCY ADD-BACK (#269, the
/// 2026-07-30 audit finding): the capacities handed to the planner are LIVE
/// free-byte readings, which EXCLUDE the bytes already-promoted experts
/// legitimately hold — so every re-plan under-counted each tier by its own
/// resident set and progressively demoted the tail (slow thrash-to-cold under
/// UNCHANGED demand). Adding the resident bytes back makes an unchanged
/// demand profile a FIXED POINT of the plan. Count resident bytes from the
/// pager's own ledger, never the OS (page-cache accounting lies both ways).
///
/// Also carries the WASTE Gate-5 cliff as a LOUD probe: when the total fast
/// budget (VRAM + RAM) is at or below ONE TOKEN'S working set, cross-token
/// reuse is structurally ZERO — the plan still returns (residency placement
/// is still better than nothing for latency), but the probe names the cliff
/// so nobody debugs a "cache bug" that is arithmetic (two nodes, three days).
/// `activated_per_token` comes from the model's ARCH PROFILE (#231 — routed
/// experts × active MoE layers), never inferred from a partial hits ledger;
/// `0` = unknown ⇒ the cliff probe is skipped (no guessed arithmetic).
pub fn plan_expert_residency_with_resident(
    profile: &SystemProfile,
    activation: &ExpertActivationProfile,
    expert_bytes: u64,
    margin_bytes: u64,
    resident_vram_bytes: u64,
    resident_ram_bytes: u64,
    activated_per_token: u32,
) -> ExpertResidencyPlan {
    // Substitute the BUDGETED VRAM for raw live-free: the pager plans within the same
    // serving headroom as everything else, so hot experts + the serving lane + the OS
    // share one budget. RAM tier keeps its real free (uncontended warm tier).
    // Both tiers get their own resident experts' bytes ADDED BACK — a live
    // reading minus what we ourselves hold is not capacity, it's a countdown.
    // ORDER MATTERS: the add-back goes on the RAW live reading BEFORE the
    // serving fraction — budget = frac × (free + resident), never
    // frac × free + resident (the latter over-credits by (1−frac) × resident;
    // caught by the fixed-point test growing the hot set by one).
    let adjusted = {
        let mut p = profile.clone();
        p.capacity.gpu_free_bytes_live = p
            .capacity
            .gpu_free_bytes_live
            .saturating_add(resident_vram_bytes);
        p
    };
    let budgeted = DeviceCapacity {
        gpu_free_bytes_live: adjusted.serving_budget_bytes(),
        system_ram_free_bytes: profile
            .capacity
            .system_ram_free_bytes
            .saturating_add(resident_ram_bytes),
        ..profile.capacity
    };

    // Cliff visibility (never a gate here — the ecache's EcacheBudget::derive
    // is the refusing seam; the PLANNER stays total so placement still helps
    // first-token latency even below the reuse cliff).
    if expert_bytes > 0 {
        let _ = activation; // profile drives the plan below; cliff uses arch facts
        if activated_per_token > 0 {
            let one_token_ws = crate::capacity::expert_ecache::EcacheBudget::one_token_working_set(
                activated_per_token,
                expert_bytes,
            );
            let fast_total = budgeted
                .gpu_free_bytes_live
                .saturating_add(budgeted.system_ram_free_bytes);
            if fast_total <= one_token_ws {
                crate::probe!(
                    class = "expert_pager.below_cliff",
                    fast_total_bytes = fast_total,
                    one_token_ws_bytes = one_token_ws,
                    activated_per_token = activated_per_token,
                    "fast tiers hold less than one token's working set — cross-token \
                     reuse will be structurally ZERO (WASTE Gate 5); shrink records \
                     (#268 container) before debugging cache logic"
                );
            }
        }
    }

    plan_expert_residency(activation, &budgeted, expert_bytes, margin_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::expert_residency::ExpertId;
    use crate::governor::types::{HardwareClass, PowerSource, TargetSilicon, ThermalClass};
    use std::collections::HashMap;

    const GB: u64 = 1024 * 1024 * 1024;

    /// A 5090-shaped box: 32 GiB VRAM, 30 GiB live-free → 24 GiB *budgeted* (0.80).
    fn bigmama() -> SystemProfile {
        SystemProfile::from_parts(
            HardwareClass {
                silicon: TargetSilicon::NvidiaCuda,
                silicon_model: "test".into(),
                vram_mb: 32 * 1024,
                system_ram_mb: 128 * 1024,
                power_source: PowerSource::Plugged,
                thermal_class: ThermalClass::Workstation,
                battery_pct: None,
                thermal_headroom_pct: None,
            },
            DeviceCapacity {
                gpu_total_bytes: 32 * GB,
                gpu_free_bytes_live: 30 * GB,
                system_ram_free_bytes: 100 * GB,
            },
            vec![],
            24,
        )
    }

    fn profile_with_hot_experts(n: u32) -> ExpertActivationProfile {
        let mut hits = HashMap::new();
        // n experts on layer 0, descending hit counts → a clear hot→cold ranking.
        for e in 0..n {
            hits.insert(
                ExpertId {
                    layer: 0,
                    expert: e,
                },
                (n - e) as u64 * 100,
            );
        }
        ExpertActivationProfile {
            gate_magnitude: HashMap::new(),
            hits,
            predicted: HashMap::new(),
        }
    }

    // what this catches: THE HEADROOM BRIDGE — the one thing THIS module owns. The
    // pager must plan within the 0.80 serving budget, NOT raw live-free VRAM, or it
    // fills VRAM to the brim and starves the serving lane / OS (the exact co-consumer
    // OOM the serving budget exists to prevent). Proven by comparison: planning against
    // the budgeted 24 GiB can never fit MORE hot experts than planning against the raw
    // 30 GiB free, and it respects the 24 GiB / 4 GiB-per-expert ≤6 bound. Regression
    // here = the pager and serving daemon fighting over VRAM with two budgets.
    #[test]
    fn pager_plans_within_the_serving_budget_not_raw_vram() {
        use super::super::expert_residency::plan_expert_residency;
        let profile = bigmama();
        assert_eq!(profile.serving_budget_bytes(), 24 * GB); // 0.80 × 30 GiB

        let activation = profile_with_hot_experts(10);
        let budgeted = plan_expert_residency_budgeted(&profile, &activation, 4 * GB, 0);
        let raw = plan_expert_residency(&activation, &profile.capacity, 4 * GB, 0);

        assert!(
            budgeted.hot.len() <= raw.hot.len(),
            "budgeted (0.80 = 24 GiB) must not fit MORE hot experts than raw free (30 GiB): budgeted={} raw={}",
            budgeted.hot.len(),
            raw.hot.len()
        );
        assert!(
            budgeted.hot.len() <= 6,
            "hot set must fit the BUDGETED VRAM (24 GiB / 4 GiB = ≤6), got {}",
            budgeted.hot.len()
        );
        assert!(
            !budgeted.hot.is_empty(),
            "some experts should be hot with a 24 GiB budget"
        );
    }

    // what this catches: unsized experts (expert_bytes = 0, a model whose layout hasn't
    // been scanned) cold-start to all-warm — the planner's safe fallback — never a
    // wrong/empty residency that would blank the model.
    #[test]
    fn unsized_experts_cold_start_to_warm_never_wrong() {
        let plan = plan_expert_residency_budgeted(&bigmama(), &profile_with_hot_experts(5), 0, 0);
        assert!(plan.hot.is_empty());
        assert_eq!(plan.warm.len(), 5, "all experts warm on cold-start");
        assert!(plan.cold.is_empty());
    }

    // what this catches: SELF-OCCUPANCY ADD-BACK (#269, the 2026-07-30 audit).
    // Capacities fed to the planner are LIVE free-byte readings that EXCLUDE
    // bytes already-promoted experts hold, so re-planning under unchanged
    // demand under-counted each tier by its own resident set and slowly
    // demoted the tail to cold. With add-back, an unchanged demand profile is
    // a FIXED POINT: plan → apply (free drops by resident bytes) → re-plan
    // with resident add-back ⇒ the identical plan. Without add-back the
    // second plan visibly shrinks — the thrash this test pins dead.
    #[test]
    fn unchanged_demand_is_a_fixed_point_with_resident_add_back() {
        let profile = bigmama();
        let activation = profile_with_hot_experts(8);
        let expert_bytes = 4 * GB;

        let first = plan_expert_residency_budgeted(&profile, &activation, expert_bytes, 0);
        assert!(!first.hot.is_empty(), "test needs a non-empty hot set");
        let hot_bytes = first.hot.len() as u64 * expert_bytes;
        let warm_bytes = first.warm.len() as u64 * expert_bytes;

        // Simulate APPLYING the plan: live-free drops by exactly what we
        // promoted. (Budget derives from live-free, so both tiers shrink.)
        let mut applied = profile.clone();
        applied.capacity.gpu_free_bytes_live = applied
            .capacity
            .gpu_free_bytes_live
            .saturating_sub(hot_bytes);
        applied.capacity.system_ram_free_bytes = applied
            .capacity
            .system_ram_free_bytes
            .saturating_sub(warm_bytes);

        // Without add-back: the re-plan shrinks (the audited thrash).
        let naive = plan_expert_residency_budgeted(&applied, &activation, expert_bytes, 0);
        assert!(
            naive.hot.len() < first.hot.len(),
            "sanity: the bug is real — live-free-only re-plan shrinks the hot set"
        );

        // With add-back: byte-identical plan — the fixed point.
        let second = plan_expert_residency_with_resident(
            &applied,
            &activation,
            expert_bytes,
            0,
            hot_bytes,
            warm_bytes,
            0,
        );
        assert_eq!(second.hot, first.hot, "hot set is a fixed point");
        assert_eq!(second.warm, first.warm, "warm set is a fixed point");
        assert_eq!(
            second.cold, first.cold,
            "nothing demoted under unchanged demand"
        );
    }
}
