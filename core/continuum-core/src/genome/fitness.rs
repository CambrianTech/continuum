//! `genome::fitness` — value-density fitness of a genome LoRA layer: the pure,
//! measured decision core the self-evolving genome turns on.
//!
//! This is the "one thing none of the prior Academy docs pinned down" that
//! `docs/genome/SELF-EVOLVING-GENOME.md` §3 defines: a layer's worth is not
//! declared, it is MEASURED —
//!
//! ```text
//! fitness(L) = (lift(L) × demand(L)) / (cost(L) × redundancy(L))
//! ```
//!
//! - **lift** — the measured A/B improvement (base vs base+L on a held-out set).
//!   The only honest quality signal; EVERYTHING gates on it. A layer with
//!   `lift ≤ 0` has ZERO fitness regardless of the other terms — it made the
//!   persona no better, so it is never minted / must be retired
//!   ([[genome-loop-first-positive-lift]], [[eval-measures-the-true-full-being-not-a-stripped-copy]]).
//! - **demand** — how often the layer is actually paged in / requested. A
//!   high-lift layer nobody uses dies (`demand → 0`).
//! - **cost** — VRAM bytes to keep it resident. A layer is kept only while its
//!   lift pays for its footprint.
//! - **redundancy** — overlap of its competence with what other resident layers
//!   already cover (`≥ 1`; `1` = unique, `→ ∞` = a perfect duplicate). A brilliant
//!   duplicate dies (`redundancy → ∞`).
//!
//! ## Why this file is PURE (the daemon split)
//! Per the concurrency style guide, the decide-logic is split from the tick so it
//! is unit-testable against a fake. The forthcoming `GenomeFitnessSentinel` (a
//! `ServiceModule` daemon — own `tokio::time::interval`, watch snapshot, slow
//! cadence, exactly like `training_completion_sentinel` and the serving daemon)
//! calls [`LayerFitness::value_density`] + [`retire_verdict`] on its tick. No
//! substrate thresholds are env-tuned here — the retire floor is the governor's
//! knob, passed in, not read from the environment ([[no-hardcoded-heuristics-to-steer-cognition]]).
//!
//! ## Scope of THIS slice (SELF-EVOLVING-GENOME §6 slice 2)
//! The value-density score + the KEEP/RETIRE eviction verdict — the measured,
//! geometry-free half. The MINT / REFINE / MERGE decisions (§4) are online
//! clustering in capability-space (a new dense cluster far from every layer = mint;
//! inside a layer's region = refine; drifted-together centroids = merge) and land
//! in a later slice with the neural embedding geometry — deliberately NOT guessed
//! here.

/// The measured inputs to a layer's value-density fitness. All four are observed
/// quantities (never declared): `lift` from the A/B eval, `demand` from page-in
/// telemetry, `cost_bytes` from the layer footprint, `redundancy` from capability-
/// space overlap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerFitness {
    /// Measured A/B pass-rate delta (base+L minus base) on the layer's held-out
    /// set, in [-1.0, 1.0]. The gate: `≤ 0` ⇒ fitness 0 no matter the rest.
    pub lift: f64,
    /// Usage frequency in [0.0, 1.0] — the fraction of eligible turns/requests that
    /// actually paged this layer in. `0` ⇒ dead weight ⇒ fitness 0.
    pub demand: f64,
    /// Resident VRAM cost in bytes (weights of the LoRA layer). Strictly > 0 in
    /// practice; a 0 is floored to 1 byte so a degenerate input can't divide-by-zero.
    pub cost_bytes: u64,
    /// Competence overlap with other resident layers: `1.0` = unique, `> 1.0` =
    /// increasingly duplicated, `→ ∞` = a perfect duplicate. Floored to 1.0
    /// (nothing is MORE than unique).
    pub redundancy: f64,
}

impl LayerFitness {
    /// Value-density fitness — the measured worth per resident byte, discounted by
    /// duplication. `lift ≤ 0` OR `demand ≤ 0` collapses it to 0.0 (a layer that
    /// doesn't improve the persona, or that nobody uses, has no worth — the
    /// corollaries the formula enforces "for free"). Higher = keep; near-zero =
    /// evict. Cost is per-GB so the number stays human-scaled (lift·demand per GB,
    /// over redundancy).
    pub fn value_density(&self) -> f64 {
        if self.lift <= 0.0 || self.demand <= 0.0 {
            return 0.0;
        }
        let cost_gb = (self.cost_bytes.max(1) as f64) / 1e9;
        let redundancy = self.redundancy.max(1.0);
        (self.lift * self.demand) / (cost_gb * redundancy)
    }
}

/// The eviction half of the mint/refine/merge/retire algorithm (§4): a layer is
/// RETIRED when its value-density falls at or below the governor-set floor — it is
/// dominated / unused / too costly for its lift. Cheap to evict: content-addressed
/// layers are re-pullable from the grid, so a wrongly-retired layer is recoverable.
/// The floor is a passed-in governor knob, NEVER an env-tuned substrate constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitnessVerdict {
    /// Fitness pays for the footprint — keep it resident.
    Keep,
    /// Dominated / unused / lift ≤ 0 — evict (re-pullable from the grid if needed).
    Retire,
}

/// Decide keep-vs-retire from a layer's measured fitness against the governor's
/// `retire_below` floor. `> floor` keeps; `≤ floor` retires. A `lift ≤ 0` layer has
/// fitness 0 and so retires at any non-negative floor — the honest default.
pub fn retire_verdict(fitness: f64, retire_below: f64) -> FitnessVerdict {
    if fitness > retire_below {
        FitnessVerdict::Keep
    } else {
        FitnessVerdict::Retire
    }
}

/// Rank layers by descending value-density — the sentinel's page-in / eviction
/// priority order (SELF-EVOLVING-GENOME §6 slice-2 gate: "fitness ranks layers
/// sensibly on a known set"). Returns indices into `layers`, best first. Stable on
/// ties (input order preserved) since fitness is a derived float and `PeerId`-style
/// deterministic ordering is not available for layers here.
pub fn rank_by_fitness(layers: &[LayerFitness]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..layers.len()).collect();
    idx.sort_by(|&a, &b| {
        layers[b]
            .value_density()
            .partial_cmp(&layers[a].value_density())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(lift: f64, demand: f64, cost_gb: f64, redundancy: f64) -> LayerFitness {
        LayerFitness {
            lift,
            demand,
            cost_bytes: (cost_gb * 1e9) as u64,
            redundancy,
        }
    }

    // what this catches: EVERYTHING gates on lift — a layer that doesn't improve the
    // persona (lift ≤ 0) has ZERO fitness no matter how cheap/unique/used it is, and
    // so retires. This is the [[genome-loop-first-positive-lift]] invariant; a
    // regression here would let the machine "confidently accumulate garbage and
    // report improvement" — the worst failure the doc names (§5).
    #[test]
    fn zero_or_negative_lift_has_zero_fitness_and_retires() {
        assert_eq!(layer(0.0, 1.0, 0.01, 1.0).value_density(), 0.0);
        assert_eq!(layer(-0.2, 1.0, 0.01, 1.0).value_density(), 0.0);
        assert_eq!(
            retire_verdict(layer(0.0, 1.0, 0.01, 1.0).value_density(), 0.0),
            FitnessVerdict::Retire
        );
    }

    // what this catches: a high-lift layer nobody uses dies (demand → 0), and a
    // brilliant duplicate dies (redundancy → ∞) — the two corollaries §3 says the
    // formula enforces "for free."
    #[test]
    fn unused_and_duplicate_layers_collapse_to_worthless() {
        assert_eq!(layer(0.5, 0.0, 0.01, 1.0).value_density(), 0.0, "unused → 0");
        let unique = layer(0.5, 0.8, 0.01, 1.0).value_density();
        let duplicate = layer(0.5, 0.8, 0.01, 1000.0).value_density();
        assert!(duplicate < unique / 100.0, "a near-perfect duplicate is worth ~nothing next to the unique one");
    }

    // what this catches: fitness ranks layers SENSIBLY (the §6 slice-2 gate). A
    // high-lift, cheap, unique, well-used layer outranks a marginal one, which
    // outranks a zero-lift one. The sentinel pages in / evicts by this order, so a
    // wrong ranking pages the wrong brain.
    #[test]
    fn fitness_ranks_layers_sensibly() {
        let layers = vec![
            layer(0.30, 0.9, 0.02, 1.0), // 0: strong, cheap, unique, used → best
            layer(0.05, 0.5, 0.05, 2.0), // 1: marginal lift, pricier, some overlap → middle
            layer(0.00, 1.0, 0.01, 1.0), // 2: no lift → worthless
        ];
        assert_eq!(rank_by_fitness(&layers), vec![0, 1, 2]);
    }

    // what this catches: a layer whose lift pays for its footprint is KEPT; the
    // retire floor is a passed-in governor knob (not env-tuned). Two layers, same
    // lift/demand/redundancy, differ only in cost → the pricier one can fall under a
    // floor the cheaper one clears.
    #[test]
    fn retire_floor_gates_on_value_density_not_lift_alone() {
        let cheap = layer(0.1, 1.0, 0.005, 1.0).value_density(); // 0.1/0.005 = 20
        let pricey = layer(0.1, 1.0, 0.5, 1.0).value_density(); //  0.1/0.5   = 0.2
        assert!(cheap > pricey);
        // A floor of 1.0 keeps the cheap high-density layer, retires the pricey one —
        // same lift, different value-density.
        assert_eq!(retire_verdict(cheap, 1.0), FitnessVerdict::Keep);
        assert_eq!(retire_verdict(pricey, 1.0), FitnessVerdict::Retire);
    }
}
