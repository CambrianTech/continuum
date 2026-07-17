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

/// SAFETY backstop on continuous-batching lanes for a single base model on one
/// node — NOT the binding constraint. The binding constraint is now the
/// compute-buffer-aware fit math ([`ModelFootprint::compute_buffer_per_lane`] +
/// the per-lane floor and window reserve in [`plan_serving`]): the lane count
/// follows persona DEMAND, capped by how many lanes' (KV + concurrent compute
/// buffer) actually fit THIS host's budget. That is the #139/#56 "lane-scaled
/// compute-buffer HEADROOM term so the ceiling derives from model size instead of
/// a constant" — it makes N-lane serving OOM-safe by construction.
///
/// History: this was `2`, a flat clamp added 2026-07-14 after four concurrent
/// prefill compute buffers (a transient ~batch-sized per-slot Metal allocation NOT
/// in the KV arithmetic) blew the pool during a wake-briefing burst
/// (`kIOGPUCommandBufferCallbackErrorOutOfMemory`). But `2` was calibrated for a
/// ~16 GB budget and mis-applied to a 64 GB (≈55 GB Metal working-set) host, where
/// it left ~29 GB idle and forced 4 personas onto 2 slots — the cross-persona KV
/// clobber that is #139's whole latency story (each turn re-prefills ~10k cold
/// because the slot was last written by a different persona's prompt+LoRA). With the
/// compute buffer now RESERVED in the fit math, giving each persona its own warm
/// slot is memory-safe. This ceiling stays only to bound pathological demand (a
/// runaway roster) and llama.cpp `--parallel` practicality; real fit is the fit math.
///
/// ⚠️ 2026-07-16 REVERTED 6 → 2 (OOM regression). Raising to 6 with the
/// `compute_buffer_per_lane` reserve passed a SMALL-prompt 4-way burst but was false
/// confidence: 4 lanes at a large served window (15104) + ~10k-token prompts blew the
/// Metal pool again (`kIOGPU…OutOfMemory`, 35 errors / 120 log lines). The reserve's bug
/// is that the transient prefill compute buffer is NOT a weights-fraction constant — it
/// scales with the WINDOW (prefill attention graph ≈ O(ubatch × n_ctx)), so a
/// window-independent reserve under-provisions exactly as the window grows, and 4
/// concurrent large-window prefills overflow. The real slot-per-persona win needs a
/// WINDOW-SCALED compute term (reserve ∝ n_ctx) validated with LARGE prompts + real GPU
/// contention, not a small-prompt burst. Until then, 2 is the floor of safety; the
/// compute_buffer_per_lane reserve stays (harmless at 2 lanes, correct direction).
/// [[verify-real-device-numbers-not-a-clamp-premise]]
pub const MAX_LANES: u32 = 2;

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

/// Co-consumer headroom (Joel 2026-07-16). The live GPU budget is a SHARED,
/// fluctuating Metal pool — a browser, LiveKit, the compositor, or a game can grab
/// (or, minutes later, RELEASE) memory the spawn-time free-VRAM snapshot can't
/// predict. Sizing to 100% of that snapshot is what let a high-free reading pick a
/// 53k window that then OOM'd (`kIOGPU…OutOfMemory`) when those consumers spiked.
/// Reserve this fraction so serving never fills a pool it doesn't solely own; the
/// ResourceGovernor (#56) re-plans the window UP into VRAM a closing game frees, DOWN
/// under live pressure. Bias: a smaller window is a slower turn; overcommit is an OOM
/// that takes the whole shared lane down. [[capacity-fabric-live-never-block-sim-as-gym]]
pub const CO_CONSUMER_HEADROOM: f64 = 0.15;

/// The transient prefill compute buffer, expressed as a fraction of the KV rate
/// (`kv_per_token / this`). llama.cpp sizes the prefill graph for the FULL served
/// window (`O(ubatch × n_ctx)`), so the buffer GROWS with the window — a
/// window-independent reserve under-provisions exactly as the window grows (the 53k
/// OOM). Calibrated to Devstral-24B Q4 (MTL compute buffer 551 MiB @ ~26k → 1209 MiB
/// @ ~53k ≈ 23.7 KiB/token; KV ≈ 112 KiB/token → ratio ≈ 1/5; rounded to 1/4 to
/// over-reserve). Model-RELATIVE, never an absolute magic byte count — a smaller
/// model's cheaper graph scales down with its KV rate. Refine as more (model, window)
/// GPU-measured points land (the calibration the `MAX_LANES` doc flags as open).
pub const PREFILL_COMPUTE_KV_DIVISOR: u64 = 4;

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

    /// The transient Metal **prefill compute buffer** ONE lane allocates while it is
    /// actively prefilling — the term that is NOT in the KV arithmetic and that the
    /// 2026-07-14 wake-briefing burst blew the pool on (four concurrent prefills ×
    /// this buffer). It is proportional to the graph size (≈ hidden × layers ×
    /// `--ubatch`), so it SCALES WITH MODEL SIZE, not a constant — the exact "lane-
    /// scaled compute-buffer HEADROOM term so the ceiling derives from model size
    /// instead of a constant" the `MAX_LANES` doc names as the #139/#56 deeper fix.
    /// Calibrated to the live measurement (Devstral-24B Q4, `MTL0 compute buffer size
    /// = 551 MiB` at `--ubatch 1024`, weights 13.66 GiB) and rounded UP via a
    /// weights-fraction so the reserve is conservative (over-reserve is a smaller
    /// window; under-reserve is an OOM). Valid at `--ubatch 1024`; a larger ubatch
    /// would scale this linearly. Reserving `lanes × this` before sizing the per-slot
    /// window is what makes N-lane serving fit BOTH the resident KV and the concurrent
    /// compute buffers — so the lane ceiling can follow persona DEMAND on a roomy host
    /// and still stay OOM-safe on a small one, killing the flat `MAX_LANES = 2` clamp.
    pub fn compute_buffer_per_lane(&self) -> u64 {
        // weights / 16 ≈ 854 MiB for the 24B (1.55× the measured 551 MiB — the safety
        // margin), and scales DOWN for smaller models (a 4B ≈ 140 MiB). Floored so a
        // tiny/degenerate footprint still reserves a real buffer, never zero.
        const COMPUTE_BUFFER_FLOOR: u64 = 256 * 1024 * 1024; // 256 MiB
        (self.weights_bytes / 16).max(COMPUTE_BUFFER_FLOOR)
    }

    /// Total on-device residency for a live server: the weights (shared across
    /// lanes) PLUS the KV-cache of every lane at the served per-slot window.
    /// `served_window` is the REAL per-slot context the process serves (the
    /// value on `ServingSnapshot`, read from llama.cpp `/props`), and `lanes` is
    /// the `--parallel` slot count — llama.cpp allocates one full KV window per
    /// slot, so resident KV = `lanes × kv_at(served_window)`. This is the honest
    /// number serving reports to the resource authority as its `footprint()`:
    /// weights-only under-reports, and the missing KV then masquerades as
    /// external/contention on the board. `lanes.max(1)` so a snapshot that has
    /// not yet stamped its lane count still charges one lane's KV, never zero.
    pub fn resident_bytes(&self, served_window: u32, lanes: u32) -> u64 {
        self.weights_bytes
            .saturating_add(self.kv_at(served_window).saturating_mul(lanes.max(1) as u64))
    }
}

/// The serving decision for this host.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/serving/ServingPlan.ts")]
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
/// `demand_lanes` is how many minds actually need a concurrent lane (the
/// persona floor). Lanes come from DEMAND, never from maximum concurrency:
/// llama-server splits the KV budget evenly across slots, so every lane
/// nobody asked for divides every mind's window for nothing. Glass-boxed
/// 2026-07-10: under benchmark memory pressure the old
/// maximize-lanes-first shape served 2 personas through 4 slots at 3633
/// tokens each — framing alone ate the window, recall/board/roster never
/// rendered, and the room degenerated into a greeting loop. 2 slots would
/// have doubled every mind's window with zero lost concurrency.
///
/// The decision is pure classification on memory arithmetic — no model is
/// loaded, no inference is run.
pub fn plan_serving(
    host: HostBudget,
    candidates: &[ModelFootprint],
    demand_lanes: u32,
) -> Option<ServingPlan> {
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

    // Lanes: DEMAND first (how many minds need a concurrent lane), then capped by
    // how many lanes' (minimum KV **+ transient prefill compute buffer**) fit the
    // budget left after weights, by perf cores, and by the MAX_LANES safety ceiling;
    // floored at 1. The per-lane floor now INCLUDES the compute buffer — the term the
    // KV arithmetic used to ignore, which let the plan size N windows to fill the
    // budget with KV and leave no room for the N concurrent compute buffers (the
    // 2026-07-14 OOM). With the buffer accounted, the lane count can follow persona
    // demand on a roomy host (each persona its OWN warm slot → no cross-persona
    // clobber → prefill caches) and still degrade to fewer lanes on a small one.
    // Effective budget = the live snapshot MINUS co-consumer headroom (the shared,
    // fluctuating Metal pool this process doesn't solely own). Everything downstream
    // sizes against THIS, so a mid-session spike from a game/browser can't OOM the
    // window we picked (Joel 2026-07-16; the governor #56 grows it back on reclaim).
    let effective = (host.usable_bytes as f64 * (1.0 - CO_CONSUMER_HEADROOM)) as u64;
    let after_weights = effective.saturating_sub(model.weights_bytes);
    let compute_floor = model.compute_buffer_per_lane();
    let per_lane_floor = model
        .kv_at(MIN_SERVE_CTX)
        .saturating_add(compute_floor)
        .max(1);
    let kv_lanes = (after_weights / per_lane_floor) as u32;
    let lanes = demand_lanes
        .max(1)
        .min(kv_lanes)
        .min(host.perf_cores.max(1))
        .min(MAX_LANES)
        .max(1);

    // Served window: expand each lane to the LARGEST context its share of the
    // post-weights budget affords — AFTER reserving every lane's compute buffer (worst
    // case: all lanes prefill at once, the wake-briefing burst). Capped at the model's
    // own trained ceiling and floored at MIN_SERVE_CTX. Derived from (model ∩ host),
    // never a constant — this is the serving window everyone downstream reads (task #50).
    // SOLVE THE FIXPOINT. The prefill compute buffer is NOT a constant — llama.cpp
    // sizes the graph for the FULL served window, so the buffer grows with C. Model it
    // as `compute_floor + compute_rate·C` per lane and solve for the largest C where KV
    // AND compute fit every lane at once (the worst case: all lanes prefill together):
    //   after_weights ≥ lanes · (kv_per_token·C + compute_floor + compute_rate·C)
    //   C ≤ (after_weights − lanes·compute_floor) / (lanes · (kv_per_token + compute_rate))
    // A window-independent reserve (the old `compute_per_lane × lanes`) under-provisions
    // exactly as the window grows — the 53k prefill OOM. Over-reserve → smaller window
    // (safe); under-reserve → OOM (fatal), so `compute_rate` rounds UP.
    let lanes64 = lanes as u64;
    let compute_rate = model.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
    let per_token_cost = model.kv_per_token.saturating_add(compute_rate).max(1);
    let fit_ctx = if model.kv_per_token == 0 {
        model.context_window
    } else {
        let after_compute_floor = after_weights.saturating_sub(compute_floor.saturating_mul(lanes64));
        ((after_compute_floor / lanes64) / per_token_cost).min(u32::MAX as u64) as u32
    };
    let served_context_window = fit_ctx.min(model.context_window).max(MIN_SERVE_CTX);
    // The honest per-lane compute reserve AT the chosen window (floor + window-scaled),
    // reused by the packing math below so resident accounting can't under-charge it.
    let compute_reserve = compute_floor
        .saturating_add(compute_rate.saturating_mul(served_context_window as u64))
        .saturating_mul(lanes64);

    // Resident models: pack the smallest other candidates (each at its minimum
    // runnable footprint) into whatever is left after the chosen base + its lanes'
    // KV at the served window + its lanes' concurrent compute buffers. "Keep as many
    // models alive, practically" — without overcommitting the budget (the compute
    // reserve is part of the chosen model's real cost, so packing can't claim it).
    let chosen_cost = model
        .weights_bytes
        .saturating_add(model.kv_at(served_context_window).saturating_mul(lanes as u64))
        .saturating_add(compute_reserve);
    let mut left = effective.saturating_sub(chosen_cost);
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
    demand_lanes: u32,
) -> Option<ServingPlan> {
    // NB: do NOT `?`-bail here. A deep transient dip can leave `plan_serving`
    // with nothing fitting the depressed budget (`fresh` = None) while a model
    // is STILL resident and serving fine — its memory is its own. Tearing that
    // down to "nothing" is the exact harm we're guarding against, so `fresh` is
    // an Option we fall back to only when the incumbent genuinely can't hold.
    let fresh = plan_serving(host, candidates, demand_lanes);
    let Some(inc_id) = incumbent else {
        return fresh;
    };
    // Fresh already chose the incumbent (or nothing else fits and fresh IS the
    // incumbent) → nothing to stabilize.
    if fresh.as_ref().map(|p| p.base_model_id.as_str()) == Some(inc_id) {
        return fresh;
    }
    // Incumbent dropped off disk entirely → honour whatever `fresh` chose
    // (possibly None = nothing servable).
    let Some(inc) = candidates.iter().find(|m| m.model_id == inc_id) else {
        return fresh;
    };
    // The incumbent is ALREADY resident: its own weights read as "used" in live
    // free memory, which is exactly what depresses `usable_bytes` while it loads.
    // Measure it against the AT-REST budget — credit its committed weights back —
    // so a model's OWN load/residency can never flap it out for a smaller model.
    // A genuine EXTERNAL squeeze (another consumer grabbing VRAM) is NOT credited
    // back, so a real over-commit still forces the down-switch to `fresh`.
    let at_rest = HostBudget {
        usable_bytes: host.usable_bytes.saturating_add(inc.weights_bytes),
        perf_cores: host.perf_cores,
    };
    // Even crediting its own residency, can the incumbent still hold a lane? If
    // not, a real squeeze has genuinely evicted it → take `fresh`.
    if inc.weights_bytes.saturating_add(inc.kv_at(MIN_SERVE_CTX)) > at_rest.usable_bytes {
        return fresh;
    }
    // Switch UP to `fresh` ONLY if it is strictly more capable AND fits the REAL
    // (un-credited) budget with headroom — loading a bigger NEW model needs actual
    // free memory, so this test uses `host`, not `at_rest`, and the headroom stops
    // a transient budget bump from flapping up.
    let headroom_budget = (host.usable_bytes as f64 * (1.0 - SWITCH_UP_HEADROOM)) as u64;
    let upgrade_worth_it = fresh
        .as_ref()
        .and_then(|f| candidates.iter().find(|c| c.model_id == f.base_model_id))
        .is_some_and(|f| {
            f.capability_rank > inc.capability_rank
                && f.weights_bytes.saturating_add(f.kv_at(MIN_SERVE_CTX)) <= headroom_budget
        });
    if upgrade_worth_it {
        return fresh;
    }
    // Keep the incumbent: re-rank it to the top so `plan_serving` selects it and
    // recomputes lanes + resident against the AT-REST budget (so its own residency
    // doesn't shrink its own window/lanes) — reusing all the fit/lane/pack logic
    // instead of duplicating it here.
    let mut promoted: Vec<ModelFootprint> = candidates.to_vec();
    if let Some(m) = promoted.iter_mut().find(|m| m.model_id == inc_id) {
        m.capability_rank = u8::MAX;
    }
    plan_serving(at_rest, &promoted, demand_lanes)
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

    // what this catches: resident_bytes folds weights + per-lane KV × lanes into
    // ONE honest residency — the number serving reports as its footprint(). If it
    // dropped the lane multiply (or the KV term entirely, the pre-#79 bug), the
    // board would under-count serving and the missing bytes would masquerade as
    // external contention. 9GB weights + 4 lanes × (90_000 × 11_008) KV.
    #[test]
    fn resident_folds_weights_plus_per_lane_kv() {
        let f = fp("coder-sentinel-14b", 9, 90_000, 262_144, 3);
        let per_lane_kv = 90_000u64 * 11_008; // kv_at(11_008)
        assert_eq!(f.kv_at(11_008), per_lane_kv);
        assert_eq!(f.resident_bytes(11_008, 4), 9 * GB + per_lane_kv * 4);
        // A lane-less snapshot (lanes == 0) still charges one lane's KV, never zero.
        assert_eq!(f.resident_bytes(11_008, 0), 9 * GB + per_lane_kv);
        // No window served yet → weights only (KV term is zero).
        assert_eq!(f.resident_bytes(0, 4), 9 * GB);
    }

    // what this catches: the "alive" OOM (2026-07-16). The served window's FULL live
    // footprint — weights + lanes·KV(C) + lanes·(compute_floor + compute_rate·C), every
    // lane prefilling at once — must fit within the EFFECTIVE budget (usable − co-consumer
    // headroom). The pre-fix math reserved a WINDOW-INDEPENDENT compute buffer, so a roomy
    // snapshot sized a huge window whose real (window-scaled) prefill buffer then blew the
    // shared Metal pool (`kIOGPU…OutOfMemory`). Devstral-24B-class model on a 64GB-class
    // budget — the exact shape that picked the OOMing 53k window.
    #[test]
    fn served_window_footprint_fits_effective_budget_including_window_scaled_compute() {
        let host = HostBudget { usable_bytes: 48 * GB, perf_cores: 10 };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3); // ~112 KiB/token KV
        let plan = plan_serving(host, std::slice::from_ref(&devstral), MAX_LANES).unwrap();
        assert!(plan.fits_on_gpu, "{}", plan.rationale);
        let c = plan.served_context_window as u64;
        let lanes = plan.lanes as u64;
        let compute_floor = devstral.compute_buffer_per_lane();
        let compute_rate = devstral.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
        let footprint = devstral.weights_bytes
            + devstral.kv_at(c as u32) * lanes
            + (compute_floor + compute_rate * c) * lanes;
        let effective = (host.usable_bytes as f64 * (1.0 - CO_CONSUMER_HEADROOM)) as u64;
        assert!(
            footprint <= effective,
            "served window {c} overcommits: footprint {footprint} > effective {effective}"
        );
        // And it's strictly smaller than the compute-blind, headroom-blind math would pick
        // (weights + KV filling the FULL usable budget) — the fix demonstrably shrinks it.
        let naive = (host.usable_bytes - devstral.weights_bytes) / lanes / devstral.kv_per_token;
        assert!(c < naive, "window-scaled + headroom reserve must shrink the window ({c} < {naive})");
    }

    // what this catches: an 8GB Air must NOT be handed the 14B (won't fit) and
    // must NOT be left with nothing — it picks the most capable model that
    // actually fits on the GPU budget. "Figure out something to run."
    #[test]
    fn tiny_box_picks_most_capable_that_fits_not_the_biggest() {
        // ~5.5GB usable after OS headroom on an 8GB Air.
        let host = HostBudget { usable_bytes: 5 * GB + 500 * 1_000_000, perf_cores: 4 };
        let plan = plan_serving(host, &candidates(), MAX_LANES).unwrap();
        assert!(plan.fits_on_gpu, "must fit a real model on GPU: {}", plan.rationale);
        assert_eq!(plan.base_model_id, "qwen3.5-4b", "14B can't fit 5.5GB; 4B is the most capable that does");
        assert!(plan.lanes >= 1);
    }

    // what this catches: lanes come from DEMAND, never maximum concurrency.
    // 2 personas on a pressured budget must get 2 well-fed lanes, not 4
    // starving ones — the 2026-07-10 starvation served 2 minds through 4 slots
    // at 3633 tokens each and the room degenerated into a greeting loop. Same
    // budget, demand honored → each mind's window roughly doubles.
    #[test]
    fn lanes_track_demand_and_every_unneeded_lane_stops_costing_window() {
        // A pressured budget (benchmark servers breathing next door).
        let host = HostBudget { usable_bytes: 20 * GB, perf_cores: 6 };
        let greedy = plan_serving(host, &candidates(), MAX_LANES).unwrap();
        let demand2 = plan_serving(host, &candidates(), 2).unwrap();
        assert_eq!(demand2.lanes, 2, "2 minds → 2 lanes, not the ceiling");
        if greedy.lanes > 2 {
            assert!(
                demand2.served_context_window > greedy.served_context_window,
                "fewer lanes must buy every mind a bigger window: {} lanes @ {} vs {} lanes @ {}",
                greedy.lanes,
                greedy.served_context_window,
                demand2.lanes,
                demand2.served_context_window,
            );
        }
        // Demand can never exceed the physical caps (kv/perf/MAX_LANES)…
        let demand99 = plan_serving(host, &candidates(), 99).unwrap();
        assert!(demand99.lanes <= MAX_LANES);
        // …and a zero demand is defensively floored at one lane.
        assert_eq!(plan_serving(host, &candidates(), 0).unwrap().lanes, 1);
    }

    // what this catches: the M5 Pro must use its headroom — pick the most
    // capable model (the 14B coding sentinel), run multiple lanes, AND size the
    // served window UP from the budget instead of clamping it to a constant.
    // The "stop dumbing down / use the machine / don't cheapen the window" case.
    #[test]
    fn big_box_picks_most_capable_runs_lanes_sizes_window_up() {
        // ~45GB usable on a 64GB M5 Pro after headroom.
        let host = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let plan = plan_serving(host, &candidates(), MAX_LANES).unwrap();
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
        let plan = plan_serving(host, &candidates(), MAX_LANES).unwrap();
        assert_eq!(plan.lanes, MAX_LANES);
    }

    // what this catches: footprint-awareness — a model with a fatter per-token
    // KV cache yields fewer lanes on the same budget than a lean one (lanes are
    // sized at the MIN window; a fatter floor lane fits fewer times).
    #[test]
    fn fatter_kv_means_fewer_lanes() {
        // Budget chosen so KV (not the MAX_LANES cap or perf cores) is the binding
        // constraint, ABOVE the per-lane compute-buffer floor now in the fit math:
        // 4GB total, 2GB weights → 2GB for (KV + compute buffer) per lane.
        // lean floor ≈ 307MB KV + 256MB compute ≈ 563MB → 3 lanes;
        // fat floor  ≈ 921MB KV + 256MB compute ≈ 1.18GB → 1 lane.
        let host = HostBudget { usable_bytes: 4 * GB, perf_cores: 8 };
        let lean = plan_serving(host, &[fp("lean", 2, 150_000, 32_768, 5)], MAX_LANES).unwrap();
        let fat = plan_serving(host, &[fp("fat", 2, 450_000, 32_768, 5)], MAX_LANES).unwrap();
        assert!(lean.lanes > fat.lanes, "lean {} should beat fat {}", lean.lanes, fat.lanes);
    }

    // what this catches: the plan CAPS lane count at the MAX_LANES safety ceiling AND
    // reserves the concurrent compute buffers, so resident KV + those buffers fit the
    // budget (no OOM by construction). MAX_LANES was raised to 6 on 2026-07-16 to give
    // each persona a warm slot, then REVERTED to 2 same-day after it re-OOM'd at large
    // windows — the transient prefill compute buffer scales with n_ctx, not weights, so a
    // window-independent reserve under-provisions and 4 concurrent large-window prefills
    // overflow. Whatever the ceiling, the fit invariant below must hold.
    #[test]
    fn lane_count_respects_the_safety_ceiling_and_reserves_compute_buffers() {
        // 24B-class: 13.6GB weights, kv_per_token ~156KB/token (measured), ~26GB usable.
        let m = fp("devstral-24b", 13, 156_000, 131_072, 9);
        let host = HostBudget { usable_bytes: 26 * GB, perf_cores: 10 };

        // 4 personas demand 4 lanes, but the MAX_LANES safety ceiling caps it.
        let plan = plan_serving(host, std::slice::from_ref(&m), 4).unwrap();
        assert_eq!(
            plan.lanes, MAX_LANES,
            "demand is capped to the MAX_LANES safety ceiling: {}",
            plan.rationale
        );

        // The plan FITS: weights + lanes×KV@window + lanes×compute buffer ≤ budget — the
        // invariant the KV-only math violated (it left no room for the buffers).
        let kv = m.kv_at(plan.served_context_window) * plan.lanes as u64;
        let compute = m.compute_buffer_per_lane() * plan.lanes as u64;
        assert!(
            m.weights_bytes + kv + compute <= host.usable_bytes,
            "resident KV + concurrent compute buffers must fit the budget (no OOM)"
        );
    }

    // what this catches: GPU-residency-first — when nothing fits, the plan
    // says so (fits_on_gpu=false) and names the smallest, instead of silently
    // claiming a CPU plan. The caller owns the CPU/grid decision.
    #[test]
    fn nothing_fits_degrades_honestly_no_silent_cpu() {
        let host = HostBudget { usable_bytes: 300 * 1_000_000, perf_cores: 2 }; // 0.3GB
        let plan = plan_serving(host, &candidates(), MAX_LANES).unwrap();
        assert!(!plan.fits_on_gpu, "must report the GPU budget can't hold any candidate");
        assert_eq!(plan.base_model_id, "qwen2.5-0.5b", "names the smallest as the only option");
        assert_eq!(plan.lanes, 1);
    }

    // what this catches: no candidates → no plan (caller must supply a registry).
    #[test]
    fn no_candidates_is_none() {
        let host = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        assert!(plan_serving(host, &[], MAX_LANES).is_none());
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
            plan_serving_stable(host, &pair(), None, MAX_LANES),
            plan_serving(host, &pair(), MAX_LANES)
        );
    }

    // what this catches: THE thrash guard — fresh prefers `big` (more capable,
    // fits) but it only fits without headroom, so we KEEP the incumbent `small`
    // rather than flap the served model on a transient budget bump.
    #[test]
    fn stable_keeps_incumbent_when_upgrade_lacks_headroom() {
        // 10GB: big (9.7GB) fits a lane but exceeds the 0.9*10=9GB headroom bar.
        let host = HostBudget { usable_bytes: 10 * GB, perf_cores: 6 };
        assert_eq!(plan_serving(host, &pair(), MAX_LANES).unwrap().base_model_id, "big", "fresh would pick big");
        let stable = plan_serving_stable(host, &pair(), Some("small"), MAX_LANES).unwrap();
        assert_eq!(stable.base_model_id, "small", "hysteresis keeps incumbent — no flap");
        assert!(stable.lanes >= 1, "lanes still re-tracked for the kept model");
    }

    // what this catches: a genuine upgrade DOES happen when the better model
    // fits with headroom — hysteresis isn't a permanent lock-in.
    #[test]
    fn stable_upgrades_when_better_model_fits_with_headroom() {
        let host = HostBudget { usable_bytes: 20 * GB, perf_cores: 6 }; // big 9.7 << 0.9*20=18
        let stable = plan_serving_stable(host, &pair(), Some("small"), MAX_LANES).unwrap();
        assert_eq!(stable.base_model_id, "big", "more capable + ample headroom → upgrade");
    }

    // what this catches: forced switch when the incumbent has DROPPED OFF DISK
    // (no longer among candidates) — we can't keep serving a model whose weights
    // vanished, so we fall to whatever is actually present. This is the genuine
    // forced-down path now that a RESIDENT incumbent is credited its own weights
    // (memory-pressure eviction of a resident model for a smaller one is provably
    // unreachable: room for the smaller model's full weights implies room for the
    // incumbent's tiny KV floor).
    #[test]
    fn stable_forced_down_when_incumbent_gone_from_disk() {
        let host = HostBudget { usable_bytes: 20 * GB, perf_cores: 6 };
        let only_small = vec![fp("small", 1, 4_000, 32_768, 1)]; // "big" no longer on disk
        let stable = plan_serving_stable(host, &only_small, Some("big"), MAX_LANES).unwrap();
        assert_eq!(stable.base_model_id, "small", "incumbent gone from disk → serve what's present");
    }

    // what this catches: THE boot-load flap (this session's live bug). While the
    // incumbent loads, its own ~9GB of weights read as "used" in live free memory,
    // so the reported budget craters (35GB → 8GB) and a plain plan would drop the
    // incumbent for a smaller model that "fits" the depressed budget — tearing down
    // the healthy lane mid-deliberation (14b → 4b → 14b). Crediting the incumbent's
    // OWN weights back (it is already resident) keeps it selected across its own
    // load. regression for the 14b→4b→14b flap at boot.
    #[test]
    fn stable_survives_its_own_load_dip_no_flap() {
        // Mid-load the monitor reports only 8GB free because the 9GB incumbent's
        // weights are paging in; steady-state would be far higher.
        let dipped = HostBudget { usable_bytes: 8 * GB, perf_cores: 6 };
        // Plain plan at the depressed budget WOULD flap: big (9GB) no longer "fits"
        // 8GB, so fresh prefers the smaller model.
        assert_eq!(
            plan_serving(dipped, &pair(), MAX_LANES).unwrap().base_model_id,
            "small",
            "depressed-budget plain plan would flap to the smaller model"
        );
        // With the incumbent credited its own weights back, the resident big stays.
        let stable = plan_serving_stable(dipped, &pair(), Some("big"), MAX_LANES).unwrap();
        assert_eq!(stable.base_model_id, "big", "incumbent survives its OWN load dip — no flap");
        assert!(stable.lanes >= 1, "kept model still gets ≥1 lane");
    }
}
