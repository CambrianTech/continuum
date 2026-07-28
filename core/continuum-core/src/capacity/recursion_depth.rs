//! Recursion depth — the DYNAMIC, RUNTIME, RUST planner for the DEPTH axis of adaptive
//! compute (`docs/architecture/ADAPTIVE-COMPUTE-OVER-FROZEN-BACKBONE.md`). Outlier B to
//! [`super::expert_residency`]'s WIDTH: if ONE planner shape fits both without forcing, the
//! per-token compute-router interface is proven and context + location slot in behind it.
//!
//! Joel 2026-07-25: his transformer arsenal (custom GPT-3, **U-Net skip paths + fractal
//! recursion**) as weapons for frontier-lite-on-misfits. This is that idea, validated: it's
//! the plan side of **Mixture-of-Recursions** (NeurIPS 2025, arxiv 2507.10524) — a frozen,
//! parameter-shared block applied a per-token number of times, a lightweight router deciding
//! how deep each token recurses, easy tokens halting early. 2× throughput at equal accuracy.
//! `never Python`: MoR is the reference; this is the dynamic-runtime-Rust port.
//!
//! ## Why depth is adaptive compute (and why the frontier won't build it)
//!
//! The frontier runs UNIFORM depth: every token pays for every layer, because they can
//! afford it. On a misfit (a MacBook Air, a lone 5090) uniform frontier depth doesn't fit.
//! But most tokens are easy — a comma, a closing brace, the obvious next word — and need one
//! or two passes; a few are hard and deserve many. Spend the depth where the token needs it
//! and a small frozen backbone punches far above its uniform-compute weight. Adaptive depth
//! is the admission ticket, not an optimization.
//!
//! ## The frozen-backbone discipline ([[frozen-borrowed-weights-as-imagenet-backbone-new-cognitive-transformer-on-top]])
//!
//! We do NOT train a new base. MoR's **recursive up-training** converts an existing
//! pretrained model into a recursive one cheaply — the base stays borrowed and frozen; the
//! router that decides depth is the small, cheap, page-in-able overlay. This planner IS that
//! router's brain.
//!
//! ## The two signals (per token, from the router)
//!
//! 1. **Difficulty** — the router's per-token "this token needs more depth" score (the MoR
//!    depth-router logit). The DYNAMIC signal: hard tokens rank high and win extra recursion.
//! 2. **Halt confidence** — per-token convergence (dynamic early-exit): once a token's hidden
//!    state has stabilized, more passes are wasted. A converged token is capped at the floor,
//!    freeing its budget for tokens that are still moving.
//!
//! ## Pure, sim-provable, same INTERFACE as the width planner (the outlier-B finding)
//!
//! `plan_recursion_depth(profile, shape, budget) -> DepthPlan` — reads per-token signals + a
//! model's recursion bounds + a live compute budget, returns a per-token depth assignment.
//! No I/O, no serving touched — deterministic and unit-testable against the same sim gym the
//! allocation policies pass, exactly like [`super::lease::decide_lane`] and
//! [`super::expert_residency::plan_expert_residency`]. The impure half — running the frozen
//! shared block N times per token, reading the router logits, wiring the halt — is BigMama's
//! serving mechanics; this is the brain they consult. The budget itself is DERIVED upstream
//! from the throughput lease / [`super::DeviceCapacity`] (how much compute this lane may
//! spend to hit its latency target) and passed in — never a constant here.
//!
//! Building depth as outlier B to width proved which parts of the compute-router interface
//! are SHARED and which are axis-specific — the real point of the exercise. **Shared:** the
//! shape (a profile of per-unit priority signals + a budget → a graded, degenerate-safe
//! plan). **Axis-specific:** the fill rule. Width FILLS to capacity — a warm expert is free
//! insurance, so residency keeps as many resident as RAM allows. Depth does NOT fill —
//! running extra passes on an easy token is wasted latency, so difficulty sets a DESIRED
//! depth and the budget only SHEDS depth (from the easiest tokens first) under pressure. One
//! interface, two honest fill policies; a future width×depth×location router composes them,
//! it does not pretend they're the same function.

use std::cmp::Ordering;

/// A model's recursion bounds + the calibrated halt threshold. Supplied by the serving side
/// (from the recursive model's config / router calibration) — NOT magic numbers here
/// ([[no-hardcoded-context-numbers-derive-from-the-live-window]]): the planner reads them,
/// the caller owns them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DepthShape {
    /// Minimum passes through the shared block every token takes — the floor below which the
    /// model produces nothing (MoR's ≥1). Mandatory: granted even when the budget can't cover
    /// it, the same "at least one lane must run" invariant as [`super::FitPolicy`].
    pub min_depth: u32,
    /// Maximum recursion depth the shared block supports (the effective deep-model ceiling a
    /// recursive model reaches by looping).
    pub max_depth: u32,
    /// A token whose halt confidence reaches this is treated as converged and capped at
    /// `min_depth`. Calibrated from the router, passed in — a policy knob, never a constant.
    pub halt_threshold: f32,
}

/// The per-token signals the router emits this forward pass. Parallel vectors indexed by
/// token position (the natural shape of a sequence). `halt_confidence` may be shorter/empty
/// on a model without dynamic halting — missing entries read as 0.0 (never converged), so
/// halting simply doesn't fire and difficulty alone drives the plan.
#[derive(Debug, Clone, Default)]
pub struct DepthProfile {
    /// Per-token "needs more depth" score (higher = harder = recurse deeper). Empty ⇒ no
    /// tokens ⇒ empty plan.
    pub difficulty: Vec<f32>,
    /// Per-token convergence (higher = more settled). `>= shape.halt_threshold` caps the
    /// token at the floor. Empty / short ⇒ nothing halts.
    pub halt_confidence: Vec<f32>,
}

/// The live compute budget for ONE forward pass, in (token · recursion-step) units — the
/// total recursion steps this lane may spend. Derived upstream from the throughput lease /
/// device capacity; an input here so the planner stays pure and sim-provable, exactly like
/// `expert_bytes` / `margin_bytes` are inputs to `plan_expert_residency`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepthBudget {
    pub total_steps: u64,
}

impl DepthBudget {
    /// Budget as a FRACTION of uniform-max-depth compute — the explicit adaptive-compute
    /// lever the governor/lease turns. `1.0` = uniform (every token at `max_depth`, no
    /// saving, never wrong); `0.5` = half the compute; `0.0` = only the mandatory floor. This
    /// is where a latency/throughput target becomes a depth budget.
    pub fn from_compute_fraction(num_tokens: usize, max_depth: u32, fraction: f32) -> Self {
        let uniform = (num_tokens as u64) * (max_depth.max(1) as u64);
        let frac = fraction.clamp(0.0, 1.0) as f64;
        DepthBudget {
            total_steps: (uniform as f64 * frac).round() as u64,
        }
    }
}

/// Per-token recursion depth for this pass, plus the accounting the glass box needs to SEE
/// the saving: `spent_steps` (what adaptive compute actually costs) vs `uniform_steps` (what
/// dense uniform depth would have cost). `uniform_steps - spent_steps` = recursion steps
/// saved — the adaptive-compute win, made visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepthPlan {
    /// Assigned recursion depth per token, indexed as `profile.difficulty`.
    pub depths: Vec<u32>,
    /// Total recursion steps this plan spends (`sum(depths)`).
    pub spent_steps: u64,
    /// What uniform max-depth would have cost (`num_tokens * max_depth`) — the baseline the
    /// frontier pays every token.
    pub uniform_steps: u64,
}

/// Decide per-token recursion depth against a live compute budget. The pure brain.
///
/// - **Desired depth from difficulty**: each token wants `min_depth + difficulty·(max−min)` —
///   an easy token (difficulty≈0) wants just the floor, a hard one (≈1) wants max. This is
///   the depth the router asks for; the budget does not push it higher (extra passes on a
///   token that doesn't need them are wasted latency — depth does NOT fill like width does).
/// - **Halt caps convergence**: a token at/above `halt_threshold` is forced to `min_depth`
///   regardless of difficulty — a settled hidden state gains nothing from more passes.
/// - **Shed under pressure only**: if the sum of desired depths exceeds the budget, shed
///   depth from the LOWEST-difficulty tokens first, down to `min_depth`, until it fits. Under
///   an abundant budget nothing is shed and the lane simply finishes faster.
///
/// Degenerate-safe, same contracts as `plan_expert_residency`: empty profile ⇒ empty plan; a
/// budget below the mandatory floor ⇒ every token at `min_depth` (`spent > budget` reported
/// honestly, floor is non-negotiable); indistinguishable difficulty ⇒ stable index-order
/// shedding (arbitrary-but-bounded until the router discriminates).
pub fn plan_recursion_depth(
    profile: &DepthProfile,
    shape: &DepthShape,
    budget: DepthBudget,
) -> DepthPlan {
    let n = profile.difficulty.len();
    let min_d = shape.min_depth.max(1); // ≥1 pass, the "at least one lane runs" invariant
    let max_d = shape.max_depth.max(min_d);

    if n == 0 {
        return DepthPlan {
            depths: Vec::new(),
            spent_steps: 0,
            uniform_steps: 0,
        };
    }

    let span = (max_d - min_d) as f32;

    // Desired depth per token: difficulty scaled across [min,max], halt forces the floor.
    let mut depths: Vec<u32> = (0..n)
        .map(|i| {
            let converged = profile.halt_confidence.get(i).copied().unwrap_or(0.0)
                >= shape.halt_threshold;
            if converged {
                return min_d;
            }
            let diff = profile.difficulty.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            min_d + (diff * span).round() as u32
        })
        .collect();

    let desired_steps: u64 = depths.iter().map(|&d| d as u64).sum();

    // Shed from the easiest tokens first, only if desired overruns the budget.
    if desired_steps > budget.total_steps {
        let mut need_to_shed = desired_steps - budget.total_steps;
        // Lowest difficulty first; stable index tiebreak so the plan is deterministic.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| {
            let da = profile.difficulty.get(a).copied().unwrap_or(0.0);
            let db = profile.difficulty.get(b).copied().unwrap_or(0.0);
            da.partial_cmp(&db).unwrap_or(Ordering::Equal).then(a.cmp(&b))
        });
        for &i in &order {
            if need_to_shed == 0 {
                break;
            }
            let sheddable = (depths[i] - min_d) as u64; // never below the mandatory floor
            let shed = sheddable.min(need_to_shed);
            depths[i] -= shed as u32;
            need_to_shed -= shed;
        }
        // need_to_shed may remain > 0 here: the floor total already exceeds the budget. Every
        // token is at min_depth and we honestly spend more than the budget — the floor wins.
    }

    let spent_steps: u64 = depths.iter().map(|&d| d as u64).sum();
    let uniform_steps = (n as u64) * (max_d as u64);
    DepthPlan {
        depths,
        spent_steps,
        uniform_steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(min: u32, max: u32) -> DepthShape {
        DepthShape {
            min_depth: min,
            max_depth: max,
            halt_threshold: 0.9,
        }
    }

    // what this catches: adaptive depth is the whole point — a workload where a few tokens
    // are hard and most are easy must spend the depth on the hard ones and floor the rest,
    // costing STRICTLY LESS than uniform max-depth. If the difficulty ordering or the budget
    // water-fill drifts, depth stops tracking the workload and we're back to paying uniform
    // compute the misfit can't afford.
    #[test]
    fn hard_tokens_get_the_depth_and_easy_ones_halt() {
        // 8 tokens: 2 hard (0,1), 6 easy. min 1, max 6.
        let mut p = DepthProfile::default();
        p.difficulty = vec![0.95, 0.90, 0.05, 0.05, 0.05, 0.05, 0.05, 0.05];
        let sh = shape(1, 6);
        // Budget = half of uniform (8*6=48 → 24). Floor is 8; 16 extra to distribute.
        let plan = plan_recursion_depth(&p, &sh, DepthBudget::from_compute_fraction(8, 6, 0.5));

        assert_eq!(plan.uniform_steps, 48, "uniform baseline = tokens*max");
        assert!(plan.spent_steps <= 24, "never exceeds the budget");
        assert!(plan.spent_steps < plan.uniform_steps, "adaptive beats uniform");
        // The two hard tokens reach max depth; the easy tail stays at the floor.
        assert_eq!(plan.depths[0], 6, "hardest token recurses to max");
        assert_eq!(plan.depths[1], 6, "second-hardest token recurses to max");
        for i in 2..8 {
            assert_eq!(plan.depths[i], 1, "easy token {i} floored at min_depth");
        }
    }

    // what this catches: dynamic halting overrides raw difficulty. A token that is the
    // HARDEST by difficulty but has CONVERGED (halt_confidence past threshold) must be forced
    // to the floor anyway — a settled hidden state gains nothing from more passes — while a
    // less-hard but still-moving token keeps the depth its difficulty asks for. If halt were
    // ignored, tok0 (difficulty 0.99) would recurse near max and waste the passes.
    #[test]
    fn halt_confidence_overrides_difficulty_and_floors_converged_tokens() {
        let mut p = DepthProfile::default();
        //            tok0 hardest but CONVERGED,  tok1 moving,  tok2 easy
        p.difficulty = vec![0.99, 0.75, 0.10];
        p.halt_confidence = vec![0.95, 0.10, 0.10]; // only tok0 has halted
        let sh = shape(1, 5); // span = 4
        // Abundant budget: nothing is shed, so each token gets exactly its desired depth.
        let plan = plan_recursion_depth(&p, &sh, DepthBudget { total_steps: 15 });

        assert_eq!(plan.depths[0], 1, "converged token floored despite being hardest");
        // tok1 desired = 1 + round(0.75*4) = 1 + 3 = 4 — its difficulty, not forced to max.
        assert_eq!(plan.depths[1], 4, "moving token gets the depth its difficulty asks for");
        assert!(plan.depths[1] > plan.depths[0], "halt beats difficulty");
    }

    // what this catches: the degenerate contracts that keep the planner safe under starvation
    // and abundance — the floor is mandatory even when the budget can't pay for it; an
    // abundant budget does NOT inflate easy tokens to max (adaptive depth saves from
    // DIFFICULTY, not just budget pressure); a maximally-hard workload with room runs full
    // depth; an empty sequence is an empty plan.
    #[test]
    fn floor_is_mandatory_and_abundant_budget_does_not_over_spend() {
        let mut mid = DepthProfile::default();
        mid.difficulty = vec![0.5, 0.5, 0.5, 0.5];
        let sh = shape(2, 8); // floor total = 8, uniform = 32, desired(0.5) = 2+round(3)=5 each

        // Starved: budget below the floor → everyone at min_depth, spent = floor > budget.
        let starved = plan_recursion_depth(&mid, &sh, DepthBudget { total_steps: 3 });
        assert!(starved.depths.iter().all(|&d| d == 2), "floor granted under starvation");
        assert_eq!(starved.spent_steps, 8, "spent is the mandatory floor, honestly over budget");

        // Abundant budget on a MID workload → desired (5 each), NOT max — no wasted passes,
        // and still below uniform (32): the saving comes from difficulty, not budget.
        let abundant = plan_recursion_depth(&mid, &sh, DepthBudget::from_compute_fraction(4, 8, 1.0));
        assert!(abundant.depths.iter().all(|&d| d == 5), "mid difficulty ⇒ mid depth, not inflated");
        assert!(abundant.spent_steps < abundant.uniform_steps, "adaptive saves even with budget to spare");

        // Maximally-hard workload with room → full depth (we never cap below what's needed).
        let mut hard = DepthProfile::default();
        hard.difficulty = vec![1.0, 1.0, 1.0, 1.0];
        let full = plan_recursion_depth(&hard, &sh, DepthBudget::from_compute_fraction(4, 8, 1.0));
        assert!(full.depths.iter().all(|&d| d == 8), "hard workload runs max depth");
        assert_eq!(full.spent_steps, full.uniform_steps);

        // Empty sequence → empty plan.
        let empty = plan_recursion_depth(&DepthProfile::default(), &sh, DepthBudget { total_steps: 99 });
        assert!(empty.depths.is_empty());
        assert_eq!(empty.spent_steps, 0);
    }
}
