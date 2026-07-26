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

use super::expert_residency::{plan_expert_residency, ExpertActivationProfile, ExpertResidencyPlan};
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
    // Substitute the BUDGETED VRAM for raw live-free: the pager plans within the same
    // serving headroom as everything else, so hot experts + the serving lane + the OS
    // share one budget. RAM tier keeps its real free (uncontended warm tier).
    let budgeted = DeviceCapacity {
        gpu_free_bytes_live: profile.serving_budget_bytes(),
        ..profile.capacity
    };
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
            hits.insert(ExpertId { layer: 0, expert: e }, (n - e) as u64 * 100);
        }
        ExpertActivationProfile {
            gate_magnitude: HashMap::new(),
            hits,
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
        assert!(!budgeted.hot.is_empty(), "some experts should be hot with a 24 GiB budget");
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
}
