//! Genome commons ranking — **pluggable by design** (Joel 2026-08-25: "make score
//! metrics pluggable. It's kind of one of those things that's subjective").
//!
//! "Best gene" is a value judgment, not a fact. Whoever owns the ranking function
//! owns the commons — so nobody does: ranking is a TRAIT, and a node composes its own
//! [`RankPolicy`] from the signals it values. The default policy exists to be replaced.
//!
//! The signals a policy draws from (each already produced elsewhere; this module
//! invents no metric, it only defines the seam that combines them):
//! - **fitness** — measured lift, UCB-bonused ([`crate::genome::fitness_ledger`]).
//!   Universal only where a benchmark/activity emitted an outcome; neutral elsewhere.
//! - **popularity** — HF downloads/likes/forks (crowd fitness for genes with no
//!   benchmark; the early commons leans here — free from HF's metrics API).
//! - **novelty** — embedding distance from the querent's nearest neighbors
//!   ([`crate::genome::signature`]). This is the DIVERSITY term: it rewards being
//!   FAR, so recall surfaces niches instead of converging on one trunk (a monoculture
//!   is evolutionarily dead and shares one blind spot). Reuses the same novelty
//!   instinct as working-memory recall.
//! - **exploration** — an audition bonus for the young/untried so a fresh fork with
//!   no history isn't buried forever (the local-optimum trap). Untried ≠ bad.
//!
//! The point is NOT the default weights below — they are a starting guess. The point
//! is that a node, a room recipe, or a future governor can supply a different
//! `RankPolicy` without touching the DAG, the signatures, or the crypto. The commons
//! stores everything; the POLICY decides what surfaces, and the policy is subjective
//! on purpose. See `docs/genome/GENOME-COMMONS-TRUST-SPINE.md`.

/// The signals available about ONE candidate gene, relative to the querent's task.
/// Every field is `Option` because a real commons is ragged: a gene may have HF
/// popularity but no benchmark, a novelty distance but no downloads yet. A policy
/// decides how to treat absence — never a silent default baked into the data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeneSignals {
    /// 0..1 measured-fitness factor (`fitness_ledger::outcome_factor`), or `None`
    /// when no outcome index was consulted for this gene.
    pub fitness: Option<f32>,
    /// Crowd signal in [0,1], typically a log-normalized HF download/like/fork count.
    /// `None` before HF stats are fetched.
    pub popularity: Option<f32>,
    /// Embedding distance in [0,1] from the querent's NEAREST already-held gene —
    /// higher = more novel (further from what the querent already has). `None` when
    /// the querent has no neighbors to be far from (an empty local genome).
    pub novelty: Option<f32>,
    /// How many times this gene has been auditioned by anyone. Drives the exploration
    /// bonus; 0 = never tried (deserves a look before it's ranked out).
    pub trials: u32,
}

/// A ranking policy: signals in, a scalar score out (higher = surface sooner). The
/// ONE decision the commons deliberately does not centralize. Implement it to value
/// the commons differently — a node that wants raw performance weights fitness; a
/// node that wants to explore weights novelty; a benchmark harness might ignore
/// popularity entirely.
pub trait RankPolicy: Send + Sync {
    fn score(&self, s: &GeneSignals) -> f32;
    /// A short human-readable name for the policy, so a rendered ranking can say
    /// WHICH lens produced it (a score with no stated policy is an opinion wearing a
    /// number's clothes — the same defect the probe doctrine warns against).
    fn name(&self) -> &'static str;
}

/// The default composite — diversity-preserving by construction, and explicitly a
/// STARTING GUESS. Weights sum to 1 across the present terms; an absent signal drops
/// its term and the remainder renormalizes, so a gene is never penalized for a
/// signal the commons simply hasn't produced yet.
pub struct DefaultCommonsPolicy {
    pub w_fitness: f32,
    pub w_popularity: f32,
    pub w_novelty: f32,
    pub exploration_c: f32,
}

impl Default for DefaultCommonsPolicy {
    fn default() -> Self {
        // A guess, not a law. Fitness leads where it exists; novelty is weighted
        // enough to keep niches alive (NOT so much that noise wins); popularity
        // carries the benchmark-less majority. Tune from real commons behavior —
        // "refine as we go" (Joel).
        Self {
            w_fitness: 0.45,
            w_popularity: 0.30,
            w_novelty: 0.25,
            exploration_c: 0.15,
        }
    }
}

impl RankPolicy for DefaultCommonsPolicy {
    fn score(&self, s: &GeneSignals) -> f32 {
        // Renormalize over the terms that actually have a signal — absence is not 0,
        // it is "this term does not vote".
        let terms: [(f32, Option<f32>); 3] = [
            (self.w_fitness, s.fitness),
            (self.w_popularity, s.popularity),
            (self.w_novelty, s.novelty),
        ];
        let present: f32 = terms.iter().filter(|(_, v)| v.is_some()).map(|(w, _)| *w).sum();
        let base = if present <= f32::EPSILON {
            0.5 // nothing known about this gene yet — neutral, then exploration decides
        } else {
            terms
                .iter()
                .filter_map(|(w, v)| v.map(|v| w * v))
                .sum::<f32>()
                / present
        };
        // Exploration audition: a decaying bonus for the untried, so a fresh fork
        // with zero history still gets surfaced enough to EARN a real score. 1/sqrt
        // shape — big for the never-tried, vanishing once a gene has been auditioned.
        let exploration = self.exploration_c / ((s.trials as f32) + 1.0).sqrt();
        (base + exploration).clamp(0.0, 1.0)
    }
    fn name(&self) -> &'static str {
        "default-commons(fitness+popularity+novelty+explore)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(f: Option<f32>, p: Option<f32>, n: Option<f32>, t: u32) -> GeneSignals {
        GeneSignals { fitness: f, popularity: p, novelty: n, trials: t }
    }

    // what this catches: the ranker's SUBJECTIVITY seam — the same signals under
    // different policies must produce different orderings (else "pluggable" is a lie),
    // and the default must (a) not bury the young, (b) reward novelty enough to keep
    // niches, (c) never penalize a gene for a signal the commons hasn't produced.
    #[test]
    fn ranking_is_pluggable_and_diversity_preserving() {
        let p = DefaultCommonsPolicy::default();

        // Absence ≠ zero: a fit gene with no popularity/novelty yet still scores on
        // fitness alone (renormalized), not dragged down by missing terms.
        let fit_only = p.score(&sig(Some(0.9), None, None, 50));
        let all_low = p.score(&sig(Some(0.2), Some(0.2), Some(0.2), 50));
        assert!(fit_only > all_low, "missing terms must not penalize: {fit_only} vs {all_low}");

        // Novelty is a real vote: two equally-fit genes, the more novel outranks the
        // trunk-hugger — the diversity guarantee.
        let novel = p.score(&sig(Some(0.6), Some(0.5), Some(0.95), 50));
        let samey = p.score(&sig(Some(0.6), Some(0.5), Some(0.05), 50));
        assert!(novel > samey, "novelty must lift a distant gene: {novel} vs {samey}");

        // The young are protected: a never-tried gene with modest signals beats an
        // equally-modest but well-worn one, so fresh forks get auditioned.
        let fresh = p.score(&sig(Some(0.5), Some(0.5), Some(0.5), 0));
        let worn = p.score(&sig(Some(0.5), Some(0.5), Some(0.5), 500));
        assert!(fresh > worn, "exploration must surface the untried: {fresh} vs {worn}");

        // Pluggability is real: a fitness-only policy orders the SAME genes differently.
        struct FitnessOnly;
        impl RankPolicy for FitnessOnly {
            fn score(&self, s: &GeneSignals) -> f32 { s.fitness.unwrap_or(0.5) }
            fn name(&self) -> &'static str { "fitness-only" }
        }
        let fo = FitnessOnly;
        // Under fitness-only, the novel-but-less-fit gene does NOT win — the value
        // judgment changed, and so did the order. That divergence IS the seam working.
        assert!(fo.score(&sig(Some(0.6), Some(0.5), Some(0.95), 50))
            == fo.score(&sig(Some(0.6), Some(0.5), Some(0.05), 50)));
        assert_ne!(p.name(), fo.name());
    }
}
