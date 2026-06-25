//! serving_plan — honest hardware → persona-serving decision.
//!
//! A deterministic, **classification-based** planner (NOT an LLM — per Joel:
//! "I even considered an LLM cpu-only just to make these decisions… that's
//! probably silly because we can do better with just classification"). Given
//! THIS host's honest memory budget and the candidate model footprints, it
//! answers — with **no grid assumed** — three questions:
//!
//!   1. which base model do we serve? (the most capable one that fits on GPU)
//!   2. how many continuous-batching lanes? (`n_seq_max`)
//!   3. how many distinct models do we keep resident (warm)?
//!
//! It degrades gracefully across the whole hardware range the substrate must
//! run on: an 8 GB M2 Air (Joel's wife's laptop) figures out *something* to
//! run; a 64 GB M5 Pro runs the most capable model it can at up to
//! [`MAX_LANES`] lanes with several models warm.
//!
//! **Footprint-aware.** A rich coding model (a sentinel, a full persona)
//! costs more memory, so fewer fit and fewer lanes run — the plan reflects
//! that, it doesn't pretend every model is the same size.
//!
//! **GPU-residency-first.** A model that cannot fit even ONE lane on the
//! GPU/UMA budget is not silently CPU-served here; the plan reports
//! `fits_on_gpu = false` and names the smallest option, leaving the
//! CPU-exception (a lone Intel Mac) or grid-routing decision to the caller.
//! This honors the "no silent CPU fallback" bar.
//!
//! ## Composition seam (this is a DECISION, not a scheduler)
//!
//! It loads nothing and schedules nothing. Its output feeds two existing
//! primitives: the spawner's base-model pick, and
//! [`crate::cognition::adaptive_throughput`] lane budgets
//! (`lanes` → `ThroughputLaneBudget::max_concurrency`). Re-run it whenever
//! the host budget changes — a model evicted under pressure, a LoRA paged in,
//! GPU pressure shifting. Pure function, same shape as `adaptive_throughput`
//! and `model_resolver`.

/// Hard ceiling on continuous-batching lanes for a single base model on one
/// node. Joel's number for the M5 Pro: "you can run 2 lanes of a gguf 4b
/// model or even 4 on here." Past this, KV-cache contention and per-token
/// batch cost stop paying off before the grid should share the load.
pub const MAX_LANES: u32 = 4;

/// Bare-minimum served window for a model to be runnable at ALL — a hardware
/// reality floor, NOT a serving target or a cheapening cap. The served window is
/// sized UP from here to the largest that fits the host budget, capped only by
/// the model's own trained `context_window`. A model whose weights + KV at even
/// this floor won't fit the GPU budget is simply not a serving option on this
/// host (→ `fits_on_gpu = false`, honest degrade — never a silent shrink).
pub const MIN_SERVE_CTX: u32 = 2048;

/// Hysteresis margin for switching UP to a more capable model: it must fit
/// within `(1 - SWITCH_UP_HEADROOM)` of the budget — i.e. with headroom to
/// spare — before we abandon the incumbent for it. Stops transient budget
/// bumps near a model's edge from flapping the served model (the live-budget
/// thrash: free memory jitters, the "best fit" flips, the model reloads).
pub const SWITCH_UP_HEADROOM: f64 = 0.10;

/// The honest, already-netted serving memory budget for THIS host — VRAM on
/// a discrete GPU, the unified-memory serving slice on Apple Silicon. The
/// caller subtracts OS + non-inference headroom before building this, so this
/// number is the single source of truth for "what is actually ours to serve
/// from."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostBudget {
    pub usable_bytes: u64,
    /// Performance-core count — caps useful lane parallelism (one batch
    /// driver can't usefully outrun the compute that feeds it).
    pub perf_cores: u32,
}

/// One candidate model's memory cost — footprint-aware so a coding sentinel
/// and a small chat model are sized differently. Carries the per-token KV RATE
/// and the model's trained `context_window` (its ceiling), NOT a pre-collapsed
/// per-lane KV at some assumed window: the served window is DERIVED in
/// [`plan_serving`] from `(context_window ∩ host budget / lanes / kv_per_token)`,
/// never a hardcoded constant ([[pass-the-model-struct-no-param-hell]]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFootprint {
    pub model_id: String,
    /// On-device weight bytes (the GGUF quant resident on GPU/UMA).
    pub weights_bytes: u64,
    /// KV-cache bytes per token for ONE sequence (lane) — the rate. Per-lane KV
    /// at a given window = `kv_per_token × window`, computed where the window is
    /// decided, so no assumed-window constant leaks into the footprint.
    pub kv_per_token: u64,
    /// The model's trained context ceiling (`Model.context_window`) — the cap the
    /// served window can never exceed. The model's stated capability, carried so
    /// the planner caps to it without re-fetching the model.
    pub context_window: u32,
    /// Higher = more capable. The planner prefers the most capable model that
    /// still fits at least one lane — "give them the most powerful persona we
    /// can," never tiering down for its own sake.
    pub capability_rank: u8,
}

impl ModelFootprint {
    /// KV-cache bytes for ONE lane at `ctx` tokens.
    pub fn kv_at(&self, ctx: u32) -> u64 {
        self.kv_per_token.saturating_mul(ctx as u64)
    }
}

/// The serving decision for this host.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServingPlan {
    /// The base model to serve (shared across lanes).
    pub base_model_id: String,
    /// The host-fit served window for the chosen model: the largest context that
    /// fits `(budget − weights) / lanes` worth of KV, capped at the model's own
    /// trained `context_window`, floored at [`MIN_SERVE_CTX`]. THE single source
    /// of truth for the effective serving window (task #50) — flows to
    /// llama-server's `-c`, the persona's `context_length`, and the deliberation
    /// prompt budget. Never a hardcoded constant.
    pub served_context_window: u32,
    /// Continuous-batching lanes (`n_seq_max`). ≥ 1.
    pub lanes: u32,
    /// How many distinct models to keep resident (warm), including the base.
    pub resident_models: u32,
    /// True when the chosen base fits at least one lane on the GPU/UMA budget.
    /// False signals the caller to CPU-serve (Intel-Mac exception) or route to
    /// a grid peer — this planner never silently CPU-falls.
    pub fits_on_gpu: bool,
    /// Honest, loggable explanation of the decision.
    pub rationale: String,
}

/// Decide how to serve persona inference on `host` given the `candidates`.
/// Returns `None` only when there are no candidates to choose from.
///
/// The decision is pure classification on memory arithmetic — no model is
/// loaded, no inference is run.
pub fn plan_serving(host: HostBudget, candidates: &[ModelFootprint]) -> Option<ServingPlan> {
    if candidates.is_empty() {
        return None;
    }

    // GPU-viable = weights + at least one lane's KV fit the honest budget.
    // "At least one lane" is the floor: a model we can't run even single-laned
    // on the GPU is not a serving option on this host.
    let fits_one_lane = |m: &ModelFootprint| {
        m.weights_bytes.saturating_add(m.kv_at(MIN_SERVE_CTX)) <= host.usable_bytes
    };

    // Prefer the MOST CAPABLE model that fits a lane. Ties broken toward the
    // larger model (more headroom spent = the more capable variant), then by
    // id descending for deterministic selection.
    let best = candidates
        .iter()
        .filter(|m| fits_one_lane(m))
        .max_by(|a, b| {
            a.capability_rank
                .cmp(&b.capability_rank)
                .then(a.weights_bytes.cmp(&b.weights_bytes))
                .then(b.model_id.cmp(&a.model_id))
        });

    let Some(model) = best else {
        // Nothing fits a lane on the GPU budget. Degrade honestly: name the
        // smallest candidate, single lane, fits_on_gpu = false. The caller
        // owns the CPU-exception / grid-routing choice; we do not silently
        // CPU-serve.
        let smallest = candidates
            .iter()
            .min_by(|a, b| {
                a.weights_bytes
                    .cmp(&b.weights_bytes)
                    .then(a.model_id.cmp(&b.model_id))
            })
            .expect("candidates non-empty checked above");
        return Some(ServingPlan {
            base_model_id: smallest.model_id.clone(),
            served_context_window: MIN_SERVE_CTX,
            lanes: 1,
            resident_models: 1,
            fits_on_gpu: false,
            rationale: format!(
                "no candidate fits the {:.1}GB GPU budget; smallest is {} ({:.1}GB) — \
                 caller must CPU-serve (Intel-Mac exception) or route to a grid peer",
                bytes_gb(host.usable_bytes),
                smallest.model_id,
                bytes_gb(smallest.weights_bytes),
            ),
        });
    };

    // Lanes: how many MINIMUM-window lanes' KV fit the budget left after
    // weights, capped by perf cores and the MAX_LANES ceiling, floored at 1. A
    // fatter per-token KV cache → fewer lanes on the same budget. The window is
    // sized UP per lane below; this only decides HOW MANY sequences run.
    let after_weights = host.usable_bytes.saturating_sub(model.weights_bytes);
    let kv_floor = model.kv_at(MIN_SERVE_CTX).max(1);
    let kv_lanes = (after_weights / kv_floor) as u32;
    let lanes = kv_lanes.min(host.perf_cores.max(1)).min(MAX_LANES).max(1);

    // Served window: expand each lane to the LARGEST context its share of the
    // post-weights budget affords, capped at the model's own trained ceiling and
    // floored at MIN_SERVE_CTX. Derived from (model ∩ host), never a constant —
    // this is the serving window everyone downstream reads (task #50).
    let per_lane_budget = after_weights / lanes as u64;
    let fit_ctx = if model.kv_per_token == 0 {
        model.context_window
    } else {
        (per_lane_budget / model.kv_per_token).min(u32::MAX as u64) as u32
    };
    let served_context_window = fit_ctx.min(model.context_window).max(MIN_SERVE_CTX);

    // Resident models: pack the smallest other candidates (each at its minimum
    // runnable footprint) into whatever is left after the chosen base + its
    // lanes' KV at the served window. "Keep as many models alive, practically" —
    // without overcommitting the budget.
    let chosen_cost = model.weights_bytes.saturating_add(
        model
            .kv_at(served_context_window)
            .saturating_mul(lanes as u64),
    );
    let mut left = host.usable_bytes.saturating_sub(chosen_cost);
    let mut resident = 1u32;
    let mut others: Vec<&ModelFootprint> = candidates
        .iter()
        .filter(|m| m.model_id != model.model_id)
        .collect();
    others.sort_by(|a, b| {
        a.weights_bytes
            .cmp(&b.weights_bytes)
            .then(a.model_id.cmp(&b.model_id))
    });
    for m in others {
        let cost = m.weights_bytes.saturating_add(m.kv_at(MIN_SERVE_CTX));
        if cost <= left {
            left = left.saturating_sub(cost);
            resident += 1;
        }
    }

    Some(ServingPlan {
        base_model_id: model.model_id.clone(),
        served_context_window,
        lanes,
        resident_models: resident,
        fits_on_gpu: true,
        rationale: format!(
            "most-capable model fitting {:.1}GB GPU budget: {} ({:.1}GB weights, rank {}), \
             {} lane(s) @ {} ctx, {} model(s) warm",
            bytes_gb(host.usable_bytes),
            model.model_id,
            bytes_gb(model.weights_bytes),
            model.capability_rank,
            lanes,
            served_context_window,
            resident,
        ),
    })
}

/// Hysteresis wrapper around [`plan_serving`]: stops model THRASH from live-
/// budget jitter. Keeps the `incumbent` model as long as it still fits the
/// budget — switching DOWN only when the incumbent no longer fits (forced
/// eviction) and UP only when a strictly more capable model fits with
/// [`SWITCH_UP_HEADROOM`] to spare. Lanes + resident count always re-track the
/// current budget. No incumbent (or it's gone / no longer fits) → plain
/// [`plan_serving`]. Use this for the ONGOING serving loop; boot uses
/// `plan_serving` directly (no incumbent yet).
pub fn plan_serving_stable(
    host: HostBudget,
    candidates: &[ModelFootprint],
    incumbent: Option<&str>,
) -> Option<ServingPlan> {
    let fresh = plan_serving(host, candidates)?;
    let Some(inc_id) = incumbent else {
        return Some(fresh);
    };
    // Fresh already chose the incumbent → nothing to stabilize.
    if fresh.base_model_id == inc_id {
        return Some(fresh);
    }
    // Is the incumbent still present AND still fits at least one lane?
    let inc = candidates.iter().find(|m| {
        m.model_id == inc_id
            && m.weights_bytes.saturating_add(m.kv_at(MIN_SERVE_CTX)) <= host.usable_bytes
    });
    let Some(inc) = inc else {
        // Incumbent gone or no longer fits → forced switch to `fresh`.
        return Some(fresh);
    };
    // Incumbent still fits. Switch UP to `fresh` ONLY if it is strictly more
    // capable AND fits with headroom (so a transient budget bump doesn't flap).
    let headroom_budget = (host.usable_bytes as f64 * (1.0 - SWITCH_UP_HEADROOM)) as u64;
    let upgrade_worth_it = candidates
        .iter()
        .find(|c| c.model_id == fresh.base_model_id)
        .is_some_and(|f| {
            f.capability_rank > inc.capability_rank
                && f.weights_bytes.saturating_add(f.kv_at(MIN_SERVE_CTX)) <= headroom_budget
        });
    if upgrade_worth_it {
        return Some(fresh);
    }
    // Keep the incumbent: re-rank it to the top so `plan_serving` selects it
    // and recomputes lanes + resident against the live budget — reusing all the
    // fit/lane/pack logic instead of duplicating it here.
    let mut promoted: Vec<ModelFootprint> = candidates.to_vec();
    if let Some(m) = promoted.iter_mut().find(|m| m.model_id == inc_id) {
        m.capability_rank = u8::MAX;
    }
    plan_serving(host, &promoted)
}

fn bytes_gb(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    // kv_per_token in BYTES; ctx is the model's trained ceiling. The served
    // window is DERIVED from (footprint ∩ host) by the planner, never passed in.
    fn fp(id: &str, weights_gb: u64, kv_per_token: u64, ctx: u32, rank: u8) -> ModelFootprint {
        ModelFootprint {
            model_id: id.to_string(),
            weights_bytes: weights_gb * GB,
            kv_per_token,
            context_window: ctx,
            capability_rank: rank,
        }
    }

    fn candidates() -> Vec<ModelFootprint> {
        vec![
            fp("qwen2.5-0.5b", 1, 4_000, 32_768, 1), // tiny chat
            fp("qwen3.5-4b", 3, 30_000, 262_144, 2), // good general (47 tok/s on M5)
            fp("coder-sentinel-14b", 9, 90_000, 262_144, 3), // rich coding model — more RAM each
        ]
    }

    // what this catches: an 8GB Air must NOT be handed the 14B (won't fit) and
    // must NOT be left with nothing — it picks the most capable model that
    // actually fits on the GPU budget. "Figure out something to run."
    #[test]
    fn tiny_box_picks_most_capable_that_fits_not_the_biggest() {
        // ~5.5GB usable after OS headroom on an 8GB Air.
        let host = HostBudget { usable_bytes: 5 * GB + 500 * 1_000_000, perf_cores: 4 };
        let plan = plan_serving(host, &candidates()).unwrap();
        assert!(plan.fits_on_gpu, "must fit a real model on GPU: {}", plan.rationale);
        assert_eq!(plan.base_model_id, "qwen3.5-4b", "14B can't fit 5.5GB; 4B is the most capable that does");
        assert!(plan.lanes >= 1);
    }

    // what this catches: the M5 Pro must use its headroom — pick the most
    // capable model (the 14B coding sentinel), run multiple lanes, AND size the
    // served window UP from the budget instead of clamping it to a constant.
    // The "stop dumbing down / use the machine / don't cheapen the window" case.
    #[test]
    fn big_box_picks_most_capable_runs_lanes_sizes_window_up() {
        // ~45GB usable on a 64GB M5 Pro after headroom.
        let host = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let plan = plan_serving(host, &candidates()).unwrap();
        assert_eq!(plan.base_model_id, "coder-sentinel-14b", "most capable, fits easily");
        assert!(plan.lanes >= 2, "M5 Pro has the budget for multiple lanes, got {}", plan.lanes);
        // The window is derived from the per-lane budget, not a constant — on a
        // 45GB box that's far above any 8k/32k clamp we used to cheapen it with.
        assert!(
            plan.served_context_window > 32_768,
            "served window must scale with the budget, got {}",
            plan.served_context_window,
        );
        assert!(
            plan.served_context_window <= 262_144,
            "but never exceed the model's trained ceiling, got {}",
            plan.served_context_window,
        );
    }

    // what this catches: the MAX_LANES ceiling holds even with absurd budget —
    // a single node doesn't run unbounded lanes (grid shares load past that).
    #[test]
    fn lanes_capped_at_max() {
        let host = HostBudget { usable_bytes: 500 * GB, perf_cores: 64 };
        let plan = plan_serving(host, &candidates()).unwrap();
        assert_eq!(plan.lanes, MAX_LANES);
    }

    // what this catches: footprint-awareness — a model with a fatter per-token
    // KV cache yields fewer lanes on the same budget than a lean one (lanes are
    // sized at the MIN window; a fatter floor lane fits fewer times).
    #[test]
    fn fatter_kv_means_fewer_lanes() {
        // Budget chosen so KV (not the MAX_LANES cap or perf cores) is the
        // binding constraint: 3GB total, 2GB weights → 1GB left for KV.
        // lean ~300MB/floor-lane → 3 lanes; fat ~920MB/floor-lane → 1 lane.
        let host = HostBudget { usable_bytes: 3 * GB, perf_cores: 8 };
        let lean = plan_serving(host, &[fp("lean", 2, 150_000, 32_768, 5)]).unwrap();
        let fat = plan_serving(host, &[fp("fat", 2, 450_000, 32_768, 5)]).unwrap();
        assert!(lean.lanes > fat.lanes, "lean {} should beat fat {}", lean.lanes, fat.lanes);
    }

    // what this catches: GPU-residency-first — when nothing fits, the plan
    // says so (fits_on_gpu=false) and names the smallest, instead of silently
    // claiming a CPU plan. The caller owns the CPU/grid decision.
    #[test]
    fn nothing_fits_degrades_honestly_no_silent_cpu() {
        let host = HostBudget { usable_bytes: 300 * 1_000_000, perf_cores: 2 }; // 0.3GB
        let plan = plan_serving(host, &candidates()).unwrap();
        assert!(!plan.fits_on_gpu, "must report the GPU budget can't hold any candidate");
        assert_eq!(plan.base_model_id, "qwen2.5-0.5b", "names the smallest as the only option");
        assert_eq!(plan.lanes, 1);
    }

    // what this catches: no candidates → no plan (caller must supply a registry).
    #[test]
    fn no_candidates_is_none() {
        let host = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        assert!(plan_serving(host, &[]).is_none());
    }

    // ── hysteresis (plan_serving_stable) ──────────────────────────────────

    // small chat (rank 1, ~1GB) vs big coder (rank 3, ~9.7GB) — the pair that
    // exercises switch-up/down decisions.
    fn pair() -> Vec<ModelFootprint> {
        vec![
            fp("small", 1, 4_000, 32_768, 1),
            fp("big", 9, 90_000, 262_144, 3),
        ]
    }

    // what this catches: no incumbent → identical to plain plan_serving (boot).
    #[test]
    fn stable_with_no_incumbent_equals_plain() {
        let host = HostBudget { usable_bytes: 20 * GB, perf_cores: 6 };
        assert_eq!(
            plan_serving_stable(host, &pair(), None),
            plan_serving(host, &pair())
        );
    }

    // what this catches: THE thrash guard — fresh prefers `big` (more capable,
    // fits) but it only fits without headroom, so we KEEP the incumbent `small`
    // rather than flap the served model on a transient budget bump.
    #[test]
    fn stable_keeps_incumbent_when_upgrade_lacks_headroom() {
        // 10GB: big (9.7GB) fits a lane but exceeds the 0.9*10=9GB headroom bar.
        let host = HostBudget { usable_bytes: 10 * GB, perf_cores: 6 };
        assert_eq!(plan_serving(host, &pair()).unwrap().base_model_id, "big", "fresh would pick big");
        let stable = plan_serving_stable(host, &pair(), Some("small")).unwrap();
        assert_eq!(stable.base_model_id, "small", "hysteresis keeps incumbent — no flap");
        assert!(stable.lanes >= 1, "lanes still re-tracked for the kept model");
    }

    // what this catches: a genuine upgrade DOES happen when the better model
    // fits with headroom — hysteresis isn't a permanent lock-in.
    #[test]
    fn stable_upgrades_when_better_model_fits_with_headroom() {
        let host = HostBudget { usable_bytes: 20 * GB, perf_cores: 6 }; // big 9.7 << 0.9*20=18
        let stable = plan_serving_stable(host, &pair(), Some("small")).unwrap();
        assert_eq!(stable.base_model_id, "big", "more capable + ample headroom → upgrade");
    }

    // what this catches: forced switch DOWN — when the incumbent no longer fits
    // the (shrunken) budget, we drop to what fits instead of clinging to it.
    #[test]
    fn stable_forced_down_when_incumbent_no_longer_fits() {
        let host = HostBudget { usable_bytes: 2 * GB, perf_cores: 2 }; // big (9.7) can't fit
        let stable = plan_serving_stable(host, &pair(), Some("big")).unwrap();
        assert_eq!(stable.base_model_id, "small", "incumbent evicted — forced down to what fits");
    }
}
