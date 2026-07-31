//! Expert tier policy — the {tier, residency, prefetch} allocator (#273).
//!
//! The last mechanism→policy gap to the K3 targets (5 tok/s 5090 / 1 tok/s M5
//! / ~10 grid). Everything it composes already exists: the tiered container
//! ([`TierSpec`] ladder, one record size per fidelity tier), tier-in-identity
//! cache keys ([`ExpertKey`](super::expert_ecache::ExpertKey)), the byte-
//! accounted ecache (`touch_sized`/`resident_bytes`), and the residency
//! ranking authority ([`expert_residency`](super::expert_residency)). This
//! module decides, per expert, WHICH quant tier to fetch and WHERE the fetched
//! record lives — precision follows EARNED importance (Joel's control-law
//! doctrine, [[expert-pager-is-classic-control-sim-trained-ml-runs-it]]):
//! high-fidelity all-star experts, decaying quant at the cruft.
//!
//! ## The seam contract (locked with BigMama's EXPERT-PAGING-CONTROL-LAW.md)
//!
//! `policy(importance, recency, quant_sensitivity, budgets) → {tier,
//! residency, prefetch}` — [`TierPolicy`] is the SEAM: [`ClassicTierPolicy`]
//! is the v1 classic-control heuristic; the sim-trained ML policy (v2) swaps
//! in behind the same signature. Nothing downstream may care which is driving.
//!
//! ## The v1 control law (ClassicTierPolicy)
//!
//! Two ORTHOGONAL axes, deliberately not conflated:
//!
//! - **Residency (WHERE)** is driven by access frequency — the existing
//!   [`ranked_experts`] priority (hits ≫ predicted ≫ gate seed). Miss COST is
//!   about how often you touch it, not how sharp it is: a frequently-hit but
//!   quant-insensitive expert still wants residency — at a cheap tier, which
//!   is exactly how cruft tiers multiply cache capacity.
//! - **Fidelity (WHICH tier)** is driven by earned importance: weighted
//!   demand = hits × quant-sensitivity, and the tier is how many DECADES of
//!   weighted demand the expert sits below the current leader (order-of-
//!   magnitude demand decay ⇔ one fidelity step). Self-normalizing — relative
//!   to the live leader, no absolute thresholds to tune per model or corpus.
//!
//! Overrides on the fidelity axis, in precedence order:
//! 1. **Observation window** (`observing`): a newly-activated expert is
//!    served tier 0 unconditionally — small-quanting a NEW active corrupts
//!    the very importance measurement the ladder runs on ("precision follows
//!    information"). The caller owns window membership (first N ticks after
//!    first fire).
//! 2. **Speculative-verify promotion** (`verify_promotions`): the measured
//!    trigger, preferred over logit thresholds. The verify loop increments an
//!    expert's step count while its cheap reconstruction measurably diverges
//!    and stops when it stops — a discrete integrator, self-calibrating; the
//!    policy just applies `base_tier − steps`. No magic divergence threshold
//!    lives here.
//! 3. **Unobserved** experts park at the CHEAPEST tier: distortion is free
//!    until activation. Predicted-only experts (about to fire, never fired)
//!    prefetch CHEAP too — speculation earns no fidelity; the observation
//!    window re-fetches sharp on the first real fire.
//!
//! Recency is not a separate input: the caller decays the hits ledger (the
//! standing profile contract — see [`ExpertActivationProfile::hits`]), so
//! "decaying tail demotes down the tier table" falls out of the decade rule
//! as old hits fade.
//!
//! Pure + sim-provable like every capacity brain: same inputs ⇒ same plan.
//! The #269 self-occupancy contract applies to `residency` budgets — the
//! caller hands free bytes WITH its own resident bytes added back (the
//! `expert_pager` shape), making unchanged demand a fixed point.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::expert_container::TierSpec;
use super::expert_ecache::ExpertKey;
use super::expert_residency::{ranked_experts, ExpertActivationProfile, ExpertId, ResidencyTier};

/// Everything the policy consumes, borrowed — the policy owns no state, so
/// v1-heuristic and v2-sim-trained-ML swap without a migration.
#[derive(Debug, Clone, Copy)]
pub struct TierPolicyInputs<'a> {
    /// Importance + prefetch signals: hits (decayed by the caller = recency),
    /// predicted (prefetch confidence), gate_magnitude (cold-start seed).
    pub profile: &'a ExpertActivationProfile,
    /// The container's fidelity ladder, index 0 = sharpest (the manifest's
    /// [`TierSpec`] table via `effective_tiers()` — v1 containers hand a
    /// single synthesized tier and the whole policy degenerates correctly).
    pub tiers: &'a [TierSpec],
    /// Residency budgets, hottest first, LIVE free bytes WITH the planner's
    /// own resident bytes added back (#269 — see `expert_pager` for the
    /// add-back shape). VRAM entries should already be the SERVING-budgeted
    /// figure, same as `plan_expert_residency_budgeted`.
    pub residency: &'a [ResidencyTier],
    /// Headroom held below each residency tier's free bytes.
    pub margin_bytes: u64,
    /// Per-expert quantization-sensitivity prior (BigMama's trace sensor).
    /// `None` ⇒ frequency-only v1; a present map treats missing experts as
    /// neutral (1.0) — the sensor just hasn't measured them yet.
    pub quant_sensitivity: Option<&'a HashMap<ExpertId, f32>>,
    /// Experts inside their high-precision observation window (new actives).
    pub observing: &'a BTreeSet<ExpertId>,
    /// Speculative-verify integrator: tier STEPS to sharpen per expert,
    /// incremented by the verify loop while divergence persists.
    pub verify_promotions: &'a HashMap<ExpertId, u16>,
}

/// Where the fetched record lives this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedResidency {
    /// Promoted into `inputs.residency[residency_index]`.
    Promoted { residency_index: usize },
    /// Stays on the cold backing store, faulted on a miss.
    Cold,
}

/// One expert's plan: which quant tier to fetch, where it lives, and whether
/// to fetch AHEAD of demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpertAssignment {
    /// Fidelity tier index into the container's ladder (0 = sharpest). This
    /// IS the `tier` field of the fetch key — one identity, no translation.
    pub tier: u16,
    pub residency: PlannedResidency,
    /// Fetch before the first demand miss (predictor-driven, never fired).
    pub prefetch: bool,
}

impl ExpertAssignment {
    /// The container fetch key this assignment resolves to. `None` if the
    /// expert coordinates overflow the container's u16 key space — a
    /// structural mismatch the caller must refuse loudly, never truncate.
    pub fn fetch_key(&self, e: ExpertId) -> Option<ExpertKey> {
        Some(ExpertKey {
            layer: u16::try_from(e.layer).ok()?,
            expert: u16::try_from(e.expert).ok()?,
            tier: self.tier,
        })
    }
}

/// The full plan for one tick. `planned_bytes[i]` is what the plan puts into
/// `inputs.residency[i]` — the number the governor lease reconciles against
/// the ecache's `resident_bytes()` once applied.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExpertTierPlan {
    pub assignments: BTreeMap<ExpertId, ExpertAssignment>,
    pub planned_bytes: Vec<u64>,
}

/// THE seam: v1 classic control below; v2 is the sim-trained ML policy behind
/// this same signature ([[expert-pager-is-classic-control-sim-trained-ml-runs-it]]).
pub trait TierPolicy {
    fn plan(&self, inputs: &TierPolicyInputs<'_>) -> ExpertTierPlan;
}

/// v1: the classic-control heuristic described in the module doc.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClassicTierPolicy;

impl ClassicTierPolicy {
    /// Weighted demand: hits × sensitivity (neutral 1.0 when the sensor is
    /// absent or hasn't measured this expert). Sensitivity 0 = provably
    /// insensitive ⇒ weighted 0 ⇒ cheapest tier, regardless of hits.
    fn weighted_demand(inputs: &TierPolicyInputs<'_>, e: &ExpertId, hits: u64) -> f64 {
        let sensitivity = inputs
            .quant_sensitivity
            .and_then(|m| m.get(e))
            .copied()
            .unwrap_or(1.0)
            .max(0.0) as f64;
        hits as f64 * sensitivity
    }

    /// Fidelity tier for one expert (0 = sharpest), per the precedence in the
    /// module doc. `leader` is the max weighted demand across observed experts
    /// this tick — the self-normalizing reference of the decade rule.
    fn fidelity_tier(
        inputs: &TierPolicyInputs<'_>,
        e: &ExpertId,
        hits: u64,
        leader: f64,
        cheapest: u16,
    ) -> u16 {
        if inputs.observing.contains(e) {
            return 0; // observation window: measure at full precision
        }
        if hits == 0 {
            // Unobserved parks cheapest; predicted-only prefetches cheap too
            // (speculation earns no fidelity — the observation window
            // re-fetches sharp on the first real fire).
            return cheapest;
        }
        let w = Self::weighted_demand(inputs, e, hits);
        let base = if w <= 0.0 || leader <= 0.0 {
            cheapest // provably insensitive (or no leader): cheapest is honest
        } else {
            // Decades of weighted demand below the leader = fidelity steps
            // down the ladder. Leader (ratio 1) ⇒ tier 0; 10× below ⇒ tier 1;
            // clamped to the ladder the container actually shipped.
            let steps = (leader / w).log10().floor().max(0.0);
            (steps as u16).min(cheapest)
        };
        // Speculative-verify promotion: the caller's integrator sharpens the
        // tier one step per persisting-divergence tick — self-calibrating,
        // no divergence threshold in the policy.
        base.saturating_sub(inputs.verify_promotions.get(e).copied().unwrap_or(0))
    }
}

impl TierPolicy for ClassicTierPolicy {
    fn plan(&self, inputs: &TierPolicyInputs<'_>) -> ExpertTierPlan {
        let mut plan = ExpertTierPlan {
            assignments: BTreeMap::new(),
            planned_bytes: vec![0; inputs.residency.len()],
        };
        // An empty ladder is a broken manifest (even v1 containers synthesize
        // one tier via effective_tiers()) — nothing honest to plan.
        if inputs.tiers.is_empty() {
            return plan;
        }
        let cheapest = (inputs.tiers.len() - 1) as u16;

        // The live leader of weighted demand — the decade rule's reference.
        let leader = inputs
            .profile
            .hits
            .iter()
            .map(|(e, h)| Self::weighted_demand(inputs, e, *h))
            .fold(0.0_f64, f64::max);

        let usable: Vec<u64> = inputs
            .residency
            .iter()
            .map(|t| t.free_bytes.saturating_sub(inputs.margin_bytes))
            .collect();

        // Walk residency-priority order (the ONE ranking authority). Byte-fill
        // is FIRST-FIT from the hottest tier per expert — unlike the uniform-
        // size planner's monotonic advance, a later (cheaper, smaller) record
        // may backfill space a big sharp record couldn't use: that is the
        // cruft-packs-denser win, not a fairness bug — every expert got the
        // hottest tier IT fit at its priority turn.
        for e in ranked_experts(inputs.profile) {
            let hits = *inputs.profile.hits.get(&e).unwrap_or(&0);
            let predicted = *inputs.profile.predicted.get(&e).unwrap_or(&0.0);
            let gate = *inputs.profile.gate_magnitude.get(&e).unwrap_or(&0.0);

            let tier = Self::fidelity_tier(inputs, &e, hits, leader, cheapest);
            let record_bytes = inputs.tiers[tier as usize].record_bytes;

            // Zero-signal experts (caller inserted a 0 entry) park cold —
            // never spend fetch bandwidth on an expert nothing points at.
            // A gate-magnitude seed or a prediction IS a signal (cold-start
            // and prefetch promotion are wanted). Unsized records (invalid
            // ladder row) are never falsely pinned.
            let zero_signal = hits == 0 && predicted == 0.0 && gate == 0.0;
            let residency = if zero_signal || record_bytes == 0 {
                PlannedResidency::Cold
            } else {
                match (0..inputs.residency.len()).find(|&i| {
                    plan.planned_bytes[i].saturating_add(record_bytes) <= usable[i]
                }) {
                    Some(i) => {
                        plan.planned_bytes[i] += record_bytes;
                        PlannedResidency::Promoted { residency_index: i }
                    }
                    None => PlannedResidency::Cold,
                }
            };

            let prefetch = matches!(residency, PlannedResidency::Promoted { .. })
                && hits == 0
                && predicted > 0.0;

            plan.assignments.insert(
                e,
                ExpertAssignment {
                    tier,
                    residency,
                    prefetch,
                },
            );
        }
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KB: u64 = 1024;

    fn e(layer: u32, expert: u32) -> ExpertId {
        ExpertId { layer, expert }
    }

    fn tier(id: u16, quant: &str, record_bytes: u64) -> TierSpec {
        TierSpec {
            id,
            quant: quant.into(),
            record_bytes,
        }
    }

    /// A 2-tier ladder: sharp 4 KiB records, cruft 1 KiB records (4× denser).
    fn two_tiers() -> Vec<TierSpec> {
        vec![tier(0, "VQ3R", 4 * KB), tier(1, "IQ1", KB)]
    }

    fn empty_ctx() -> (BTreeSet<ExpertId>, HashMap<ExpertId, u16>) {
        (BTreeSet::new(), HashMap::new())
    }

    fn plan_with(
        profile: &ExpertActivationProfile,
        tiers: &[TierSpec],
        residency: &[ResidencyTier],
        sensitivity: Option<&HashMap<ExpertId, f32>>,
        observing: &BTreeSet<ExpertId>,
        promotions: &HashMap<ExpertId, u16>,
    ) -> ExpertTierPlan {
        ClassicTierPolicy.plan(&TierPolicyInputs {
            profile,
            tiers,
            residency,
            margin_bytes: 0,
            quant_sensitivity: sensitivity,
            observing,
            verify_promotions: promotions,
        })
    }

    use super::super::expert_residency::{ResidencyMedium, ResidencyTier};

    fn vram(free: u64) -> Vec<ResidencyTier> {
        vec![ResidencyTier {
            medium: ResidencyMedium::Vram,
            free_bytes: free,
        }]
    }

    // what this catches (#273 core): the all-star/cruft allocation — high-hit
    // experts land tier 0 (sharp) resident, the ≥10×-below tail lands the
    // cruft tier, the byte budget holds, AND cruft density is real: the same
    // budget fits MORE experts than a sharp-only ladder could. If the decade
    // rule, the byte fill, or the tier→record_bytes mapping drifts, precision
    // stops following earned importance and the density win evaporates.
    #[test]
    fn all_stars_go_sharp_tail_goes_cruft_within_the_byte_budget() {
        let mut p = ExpertActivationProfile::default();
        // 4 all-stars within 10× of the leader (1000..700), 2 tail at 5 hits.
        for (i, hits) in [1000u64, 900, 800, 700].into_iter().enumerate() {
            p.hits.insert(e(0, i as u32), hits);
        }
        p.hits.insert(e(0, 4), 5);
        p.hits.insert(e(0, 5), 5);

        // Budget = exactly 4 sharp + 2 cruft records.
        let budget = 4 * (4 * KB) + 2 * KB;
        let (obs, promo) = empty_ctx();
        let plan = plan_with(&p, &two_tiers(), &vram(budget), None, &obs, &promo);

        for i in 0..4 {
            let a = plan.assignments[&e(0, i)];
            assert_eq!(a.tier, 0, "all-star {i} is sharp");
            assert!(
                matches!(a.residency, PlannedResidency::Promoted { residency_index: 0 }),
                "all-star {i} is resident"
            );
        }
        for i in 4..6 {
            let a = plan.assignments[&e(0, i)];
            assert_eq!(a.tier, 1, "tail {i} decays to the cruft tier");
            assert!(
                matches!(a.residency, PlannedResidency::Promoted { .. }),
                "cruft record still fits — density is the point"
            );
        }
        assert_eq!(plan.planned_bytes[0], budget, "budget exactly consumed");

        // Density proof: a sharp-only ladder fits only 4 of the 6 in the same
        // budget (4×4KiB = 16KiB; the 18KiB budget has no room for a 5th).
        let sharp_only = vec![tier(0, "VQ3R", 4 * KB)];
        let flat = plan_with(&p, &sharp_only, &vram(budget), None, &obs, &promo);
        let resident = flat
            .assignments
            .values()
            .filter(|a| matches!(a.residency, PlannedResidency::Promoted { .. }))
            .count();
        assert_eq!(resident, 4, "sharp-only fits fewer — cruft tiers multiply cache");
        assert!(
            flat.assignments.values().all(|a| a.tier == 0),
            "single-tier ladder degenerates to tier 0 everywhere (v1 container)"
        );
    }

    // what this catches: the byte budget BINDS — when the budget only fits
    // part of the demand, the coldest expert spills to Cold rather than the
    // fill overflowing the tier (a false pin the governor lease would then
    // reconcile against thin air).
    #[test]
    fn over_budget_demand_spills_the_coldest_to_cold() {
        let mut p = ExpertActivationProfile::default();
        for (i, hits) in [1000u64, 900, 5].into_iter().enumerate() {
            p.hits.insert(e(0, i as u32), hits);
        }
        // Fits the two sharp all-stars only; the cruft record (1 KiB) doesn't fit.
        let budget = 2 * (4 * KB);
        let (obs, promo) = empty_ctx();
        let plan = plan_with(&p, &two_tiers(), &vram(budget), None, &obs, &promo);
        assert!(matches!(
            plan.assignments[&e(0, 2)].residency,
            PlannedResidency::Cold
        ));
        assert_eq!(plan.planned_bytes[0], budget);
    }

    // what this catches: "unobserved park cheapest on disk (free)" — an
    // expert with a zero-signal ledger entry gets the cheapest tier AND stays
    // Cold even with budget to spare (no fetch bandwidth for an expert
    // nothing points at). A gate-seeded expert, by contrast, IS promotable
    // (cold-start seeding) — at the cheapest tier, since it never fired.
    #[test]
    fn zero_signal_parks_cold_cheapest_but_gate_seed_promotes() {
        let mut p = ExpertActivationProfile::default();
        p.hits.insert(e(0, 0), 0); // zero-signal entry
        p.gate_magnitude.insert(e(0, 1), 3.0); // gate-seeded, never fired
        let (obs, promo) = empty_ctx();
        let plan = plan_with(&p, &two_tiers(), &vram(64 * KB), None, &obs, &promo);

        let parked = plan.assignments[&e(0, 0)];
        assert_eq!(parked.tier, 1, "unobserved is cheapest");
        assert!(matches!(parked.residency, PlannedResidency::Cold));

        let seeded = plan.assignments[&e(0, 1)];
        assert_eq!(seeded.tier, 1, "unfired seed earns no fidelity");
        assert!(
            matches!(seeded.residency, PlannedResidency::Promoted { .. }),
            "gate seed cold-starts residency"
        );
        assert!(!seeded.prefetch, "gate seed is not a prediction");
    }

    // what this catches: the observation window — a NEW active whose hit
    // count alone would put it deep in the cruft gets tier 0 while observed,
    // because small-quanting a new active corrupts the importance measurement
    // itself (precision follows information). And the predictor's prefetch:
    // a predicted-but-unfired expert is promoted CHEAP with prefetch=true.
    #[test]
    fn observation_window_forces_sharp_and_prediction_prefetches_cheap() {
        let mut p = ExpertActivationProfile::default();
        p.hits.insert(e(0, 0), 1000); // leader
        p.hits.insert(e(0, 1), 2); // new active: 500× below ⇒ cruft by decade rule
        p.predicted.insert(e(0, 2), 0.9); // about to fire

        let mut obs = BTreeSet::new();
        obs.insert(e(0, 1));
        let promo = HashMap::new();
        let plan = plan_with(&p, &two_tiers(), &vram(64 * KB), None, &obs, &promo);

        assert_eq!(plan.assignments[&e(0, 1)].tier, 0, "observed ⇒ sharp");
        let spec = plan.assignments[&e(0, 2)];
        assert_eq!(spec.tier, 1, "speculation earns no fidelity");
        assert!(spec.prefetch, "predicted-only fetches ahead of demand");
        assert!(!plan.assignments[&e(0, 0)].prefetch, "proven hit is not a prefetch");
    }

    // what this catches: the speculative-verify promotion integrator — a
    // cruft-tier expert whose cheap reconstruction measurably diverged gets
    // stepped sharper than its hits alone earn, one step per persisting tick,
    // with no divergence threshold inside the policy (self-calibrating).
    #[test]
    fn verify_divergence_promotes_one_step_sharper() {
        let mut p = ExpertActivationProfile::default();
        p.hits.insert(e(0, 0), 1000);
        p.hits.insert(e(0, 1), 5); // 200× below ⇒ cruft (tier 1)
        let obs = BTreeSet::new();
        let mut promo = HashMap::new();
        promo.insert(e(0, 1), 1u16);
        let plan = plan_with(&p, &two_tiers(), &vram(64 * KB), None, &obs, &promo);
        assert_eq!(
            plan.assignments[&e(0, 1)].tier,
            0,
            "one verify step lifts the diverging expert a tier sharper"
        );
    }

    // what this catches: importance = hits × quant_sensitivity. Two experts
    // with IDENTICAL hit counts split tiers when the sensor says one is
    // sensitive and the other is not; with the sensor absent (None) the
    // policy is frequency-only and both land the same tier — the exact
    // Option-until-her-sensor-lands contract.
    #[test]
    fn sensitivity_splits_equal_hit_experts_and_none_is_frequency_only() {
        let mut p = ExpertActivationProfile::default();
        p.hits.insert(e(0, 0), 1000); // leader
        p.hits.insert(e(0, 1), 100); // 10× below ⇒ tier 1 on frequency alone
        p.hits.insert(e(0, 2), 100); // same hits
        let mut sens = HashMap::new();
        sens.insert(e(0, 1), 10.0f32); // sensitive: weighted 1000 ⇒ tier 0
        sens.insert(e(0, 2), 1.0f32); // neutral: stays tier 1

        let (obs, promo) = empty_ctx();
        let plan = plan_with(&p, &two_tiers(), &vram(64 * KB), Some(&sens), &obs, &promo);
        assert_eq!(plan.assignments[&e(0, 1)].tier, 0, "sensitivity buys fidelity");
        assert_eq!(plan.assignments[&e(0, 2)].tier, 1, "neutral stays on frequency");

        let flat = plan_with(&p, &two_tiers(), &vram(64 * KB), None, &obs, &promo);
        assert_eq!(flat.assignments[&e(0, 1)].tier, 1, "no sensor ⇒ frequency-only");
        assert_eq!(flat.assignments[&e(0, 2)].tier, 1);
    }

    // what this catches: the #269 fixed-point contract extended to tiers —
    // apply the plan (each residency tier's live free drops by the planned
    // bytes), re-plan with the resident bytes added back, and the plan is
    // BYTE-IDENTICAL. Without add-back the re-plan demotes the tail (the
    // slow thrash-to-cold #269 pinned on the uniform planner). This is the
    // caller contract documented on TierPolicyInputs::residency.
    #[test]
    fn unchanged_demand_is_a_fixed_point_under_resident_add_back() {
        let mut p = ExpertActivationProfile::default();
        for (i, hits) in [1000u64, 900, 800, 5, 5].into_iter().enumerate() {
            p.hits.insert(e(0, i as u32), hits);
        }
        let free = 3 * (4 * KB) + 2 * KB; // 3 sharp + 2 cruft, exactly
        let (obs, promo) = empty_ctx();
        let first = plan_with(&p, &two_tiers(), &vram(free), None, &obs, &promo);
        assert_eq!(first.planned_bytes[0], free, "budget fully used");

        // Apply: live free drops by what we promoted.
        let applied_free = free - first.planned_bytes[0];

        // Without add-back the re-plan visibly shrinks (the audited thrash).
        let naive = plan_with(&p, &two_tiers(), &vram(applied_free), None, &obs, &promo);
        assert!(
            naive.planned_bytes[0] < first.planned_bytes[0],
            "sanity: the bug is real without add-back"
        );

        // With add-back: byte-identical plan — the fixed point.
        let second = plan_with(
            &p,
            &two_tiers(),
            &vram(applied_free + first.planned_bytes[0]),
            None,
            &obs,
            &promo,
        );
        assert_eq!(second, first, "unchanged demand is a fixed point");
    }

    // what this catches: the fetch-key wire-through — the assignment's tier
    // IS the ExpertKey tier (one identity, no translation layer), and
    // out-of-range coordinates refuse loudly (None) rather than truncating
    // into some other expert's bank slot.
    #[test]
    fn fetch_key_carries_the_tier_and_refuses_overflow() {
        let a = ExpertAssignment {
            tier: 1,
            residency: PlannedResidency::Cold,
            prefetch: false,
        };
        let key = a.fetch_key(e(3, 42)).expect("in-range key");
        assert_eq!((key.layer, key.expert, key.tier), (3, 42, 1));
        assert!(a.fetch_key(e(0, 70_000)).is_none(), "u16 overflow refuses");
    }

    // what this catches: an empty tier ladder (broken manifest — even v1
    // containers synthesize one tier) plans NOTHING rather than indexing
    // into a phantom ladder.
    #[test]
    fn empty_ladder_plans_nothing() {
        let mut p = ExpertActivationProfile::default();
        p.hits.insert(e(0, 0), 10);
        let (obs, promo) = empty_ctx();
        let plan = plan_with(&p, &[], &vram(64 * KB), None, &obs, &promo);
        assert!(plan.assignments.is_empty());
    }
}
