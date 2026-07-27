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
/// 2026-07-16 RAISED 2 → 4 — the window-scaled compute term the earlier revert
/// demanded now EXISTS. History: raised 2→6, reverted 6→2 same day (OOM) because the
/// `compute_buffer_per_lane` reserve was a window-INDEPENDENT constant, so a 4-lane plan
/// at a large served window under-provisioned the transient prefill buffer (which scales
/// `O(ubatch × n_ctx)`) and 4 concurrent large-window prefills blew the Metal pool
/// (`kIOGPU…OutOfMemory`). The prerequisite named there — "a WINDOW-SCALED compute term
/// (reserve ∝ n_ctx)" — landed in [`plan_serving`] (commit 0925aa1ac): the served window
/// now SOLVES THE FIXPOINT, shrinking so `lanes × (KV(C) + compute(C))` fits every lane,
/// AND reserves co-consumer headroom. So more lanes no longer overcommit — the window
/// auto-shrinks to pay for them (`served_window_footprint_fits_effective_budget…` pins
/// the invariant). 4 = the resident-persona count: each mind gets its OWN warm slot, so a
/// turn reuses ITS prefilled KV instead of re-prefilling ~10k cold after a slot-mate
/// clobbered it (#139's whole latency story). Still a SANITY backstop, not the binding
/// constraint — the real cap is the fit math (`kv_lanes`) ∩ DEMAND ∩ perf cores; this
/// only bounds a pathological roster and llama.cpp `--parallel` practicality. Kept modest
/// (4, not the 6 that OOM'd) pending a LARGE-prompt 4-lane live-GPU burst — the doc's
/// acceptance gate. [[verify-real-device-numbers-not-a-clamp-premise]] [[capacity-fabric-live-never-block-sim-as-gym]]
pub const MAX_LANES: u32 = 2;
// ⚠️ 2026-07-17 REVERTED 4 → 2. Raising to 4 (warm slot per persona) was OOM-SAFE
// (the window-scaled fit shrank -c to fit) but STARVED CONTEXT: splitting the budget 4
// ways dropped the per-slot window to ~6k, and a live persona's assembled prompt is ~9k
// (identity + tools + workspace-map + recall + memory), so every persona got
// budget-truncated to fit — the "120k clamped to 3k → all models suck" regression, plus
// a 145s decode loop on the starved/confused context. Warmth is worthless if the mind
// can't see. The 4-lane premise was wrong: personas are NOT all concurrently active, so
// lanes should follow real concurrent DEMAND (usually 1–2), not the resident count —
// 2 lanes at ~19.8k each beats 4 warm lanes at 6k. Cross-turn clobber (the reason for the
// raise) is the lesser evil vs a starved window; solve it with idle-warmth/duty-cycling,
// NOT by cutting everyone's context. [[no-hardcoded-context-numbers-derive-from-the-live-window]]

/// Bare-minimum served window for a model to be runnable at ALL — a hardware
/// reality floor, NOT a serving target or a cheapening cap. The served window is
/// sized UP from here to the largest that fits the host budget, capped by BOTH
/// the model's own trained `context_window` AND the DEMAND ceiling
/// ([`BOOTSTRAP_WORKING_SET`] / measured p95) — a lane is provisioned for what
/// personas actually USE, never maximized to fill RAM. A model whose weights + KV
/// at even this floor won't fit the GPU budget is simply not a serving option on
/// this host (→ `fits_on_gpu = false`, honest degrade — never a silent shrink).
pub const MIN_SERVE_CTX: u32 = 2048;

/// Cold-start DEMAND ceiling for the served window — the "B-now" half of the
/// demand-derived cap (M5 + BigMama, 2026-07-26). Sizing a lane's window to fill
/// the memory budget instead of to real demand is the cross-node serving-quality
/// bug: on a roomy host `window_for` returns e.g. 94k, whose pre-allocated KV is
/// ~33GB × parallel lanes — it SWAPS a 64GB Mac (5M swapouts → 1.77 tok/s) and
/// WEDGES a 32GB 5090 (decode hangs), while personas fill only ~9k. This is the
/// conservative PRIOR — ~one full persona turn (assembled prompt ~9k + generation
/// headroom) — used until live per-persona working-set telemetry (p95 observed +
/// gen headroom) refines it UP toward measured demand (task #234, the A-next
/// robustness layer): `DEMAND_CEIL = max(BOOTSTRAP_WORKING_SET, p95 + headroom)`.
/// It only ever caps the window DOWN from the budget-max, is itself bounded by the
/// model's trained ceiling in `window_for`, and refines UP with data — never
/// toward filling RAM. This is NOT the forbidden per-tier PROMPT clamp
/// ([[no-hardcoded-context-numbers-derive-from-the-live-window]]): that doctrine
/// bars clamping the prompt to a static tier cap; this is a demand PRIOR on
/// serving-lane residency, superseded by measurement, anchored to the ONE runnable
/// floor (`MIN_SERVE_CTX × 8 = 16384`) rather than a second bare magic number.
pub const BOOTSTRAP_WORKING_SET: u32 = MIN_SERVE_CTX * 8;

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

    /// The concurrent-prefill compute reserve across all lanes AT the served window —
    /// the ONE formula the serving plan's fixpoint sizes against ([`plan_serving`]'s
    /// `compute_reserve`) and the SAME number the board must attribute (E=mc²: one
    /// compute-reserve decision, one place). Per lane the buffer is NOT a constant:
    /// llama.cpp sizes the prefill graph for the FULL served window, so it grows with C
    /// as `compute_floor + compute_rate·C` (`compute_rate = kv_per_token /
    /// PREFILL_COMPUTE_KV_DIVISOR`). Worst case — every lane prefills at once (the
    /// wake-briefing burst) — so multiply by lanes. A window-INDEPENDENT reserve
    /// under-provisions exactly as the window grows (the 53k prefill OOM), so this
    /// window-scales.
    pub fn prefill_compute_reserve(&self, served_window: u32, lanes: u32) -> u64 {
        let compute_rate = self.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
        self.compute_buffer_per_lane()
            .saturating_add(compute_rate.saturating_mul(served_window as u64))
            .saturating_mul(lanes.max(1) as u64)
    }

    /// The PEAK on-device residency serving can hit — [`resident_bytes`] (weights +
    /// per-lane KV) PLUS the concurrent-prefill compute reserve of every lane
    /// ([`prefill_compute_reserve`]). This equals the serving plan's `chosen_cost`
    /// exactly, so it is the honest number serving reports to the resource authority as
    /// its `footprint()` (#56/G5): `resident_bytes` alone omits the transient compute
    /// buffer, so the board over-reports free VRAM by the compute reserve and any OTHER
    /// consumer (the ephemeral eval lane, a LoRA-train job) reads those phantom-free
    /// bytes and grabs exactly what serving needs for its next prefill → the
    /// concurrent-OOM. Reserving the peak is what lets the box run a big coder + a
    /// benchmark lane + training WITHOUT the second consumer stepping on serving's
    /// compute buffer. The plan already sizes the window to fit this reserve within its
    /// budget; this is the CROSS-consumer attribution so the board's `available` tells
    /// everyone else the truth.
    pub fn peak_resident_bytes(&self, served_window: u32, lanes: u32) -> u64 {
        self.resident_bytes(served_window, lanes)
            .saturating_add(self.prefill_compute_reserve(served_window, lanes))
    }

    /// The capacity-fabric [`LeaseRequest`](crate::capacity::LeaseRequest) for serving
    /// this model at `served_window` with `demand_lanes` concurrent minds — the bridge
    /// from the serving plan's MODEL-RESIDENCY view (weights + per-lane KV) to the grid's
    /// CONCURRENCY-SPIKE view, so [`GridPlacementPolicy`](crate::capacity::grid) can place
    /// overflow lanes onto peers (#180 grid spill / [[frontier-is-a-scaling-question-over-misfits-not-a-capability-question]]).
    /// `want_concurrency` = the minds that want a lane; `spike_bytes` = ONE lane's transient
    /// prefill compute buffer at this window (the term the 2026-07-14 OOM turned on), so a
    /// peer is only offered a spill lane it can actually hold. NOTE: this sizes the
    /// CONCURRENCY spike only; a peer must ALSO already hold this model resident — that
    /// residency gate is the gossip-side half of the routing (owned with the grid snapshot),
    /// NOT this pure mapping.
    pub fn grid_lease_request(
        &self,
        served_window: u32,
        demand_lanes: u32,
    ) -> crate::capacity::LeaseRequest {
        crate::capacity::LeaseRequest {
            consumer: self.model_id.clone(),
            want_concurrency: demand_lanes.max(1),
            spike_bytes: self.prefill_compute_reserve(served_window, 1),
        }
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
    /// Personas that DEMANDED a concurrent lane but couldn't get a local one — the
    /// overflow the governor must PLACE on a grid peer (or queue), never cram into
    /// `lanes` and thrash. `demand_lanes − lanes` (saturating) on the GPU path, or
    /// the FULL demand when nothing fits locally (`fits_on_gpu = false`). This is the
    /// honest "over local capacity by N" signal (#234/#56 — the good governor
    /// recognizing its own overload): the caller routes these off-box; `0` means
    /// demand fit locally. Surfaced, never silently dropped
    /// ([[fallbacks-are-illegal-fail-loud]]).
    pub grid_overflow_lanes: u32,
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
/// `demand_ceil` is the LIVE demand ceiling for the served window — the ELASTIC,
/// per-task upper bound. `window_for` sizes the window UP to what the budget
/// allows; this caps it DOWN to what the task actually needs. A hard coding task
/// passes a high ceiling and the window GROWS (up to the budget/model bound); a
/// simple turn passes a low one so more lanes fit. It is NEVER a launch-baked
/// constant — callers thread live per-persona/per-task demand (measured p95 +
/// headroom, or a task's explicit request). [`plan_serving`] supplies
/// [`BOOTSTRAP_WORKING_SET`] only as the cold-start prior until that telemetry
/// exists (#234). OOM-safe: `window_for` already bounds the window to the budget,
/// so a higher ceiling only raises the cap toward that bound, never past it.
/// [[serving-resources-are-elastic-per-task-leases-context-and-model-grow-for-hard-problems]]
///
/// The decision is pure classification on memory arithmetic — no model is
/// loaded, no inference is run.
pub fn plan_serving_with_demand(
    host: HostBudget,
    candidates: &[ModelFootprint],
    demand_lanes: u32,
    demand_ceil: u32,
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
            // Nothing fits the GPU budget → the WHOLE demand is off-box work (the
            // caller CPU-serves or routes every lane to a peer). fits_on_gpu = false
            // is the hard signal; grid_overflow_lanes quantifies "how much to place."
            grid_overflow_lanes: demand_lanes,
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
    let compute_rate = model.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
    let per_token_cost = model.kv_per_token.saturating_add(compute_rate).max(1);

    // The LARGEST context each of `l` lanes can hold once weights + every lane's compute
    // buffer are reserved (the full fixpoint — KV *and* the window-scaled prefill graph,
    // worst case all lanes prefill at once):
    //   after_weights ≥ l · (kv_per_token·C + compute_floor + compute_rate·C)
    //   C ≤ (after_weights − l·compute_floor) / (l · (kv_per_token + compute_rate))
    // Capped at the model's trained ceiling. Over-reserve → smaller window (safe);
    // under-reserve → OOM (fatal), so `compute_rate` rounds UP. `0` when `l` lanes can't
    // even hold one token — adding that lane would starve every lane.
    let window_for = |l: u64| -> u32 {
        if l == 0 {
            return 0;
        }
        if model.kv_per_token == 0 {
            return model.context_window;
        }
        let after_compute = after_weights.saturating_sub(compute_floor.saturating_mul(l));
        (((after_compute / l) / per_token_cost).min(u32::MAX as u64) as u32).min(model.context_window)
    };

    // THE DAEMON'S PURPOSE, made concrete (#213): serve as many concurrent minds as DEMAND
    // wants — but ONLY while each still gets a window big enough to THINK in. Concurrency is
    // worthless if it collapses every lane to the MIN_SERVE_CTX floor: a 2048-token mind
    // can't hold its own memory + the task, so 1 lane @ 30k beats 2 lanes @ 2k. The old
    // math capped lanes by what fit at the FLOOR window (`kv_at(MIN_SERVE_CTX)`), so it
    // admitted N lanes and let the window collapse — a lane-count target chased off a cliff.
    // Instead, pick the LARGEST lane count (≤ demand, ≤ cores, ≤ MAX_LANES) whose per-lane
    // window still clears the floor. Fall back to 1 only when the host is genuinely too small
    // for even one real window — honest starvation (surfaced downstream), never a silent
    // collapse. Same lanes-follow-demand-on-a-roomy-host / degrade-on-a-small-one behavior,
    // but the degrade path now SHEDS LANES (queue) instead of shrinking the mind to nothing
    // ([[never-thrash-sticky-hysteresis-on-every-lane]], [[situation-aware-focuser]]).
    let lane_cap = demand_lanes
        .max(1)
        .min(host.perf_cores.max(1))
        .min(MAX_LANES)
        .max(1);
    let lanes = (1..=lane_cap)
        .rev()
        .find(|&l| window_for(l as u64) > MIN_SERVE_CTX)
        .unwrap_or(1);
    // DEMAND cap (M5+BigMama 2026-07-26): provision for what personas USE, not for
    // what RAM allows. `window_for` maximizes the window to fill the budget (94k on
    // a roomy host) → ~33GB pre-allocated KV × lanes → swap/wedge, while personas
    // fill ~9k. Cap DOWN to the demand ceiling (BOOTSTRAP_WORKING_SET now; measured
    // p95 later, #234). Ordering: window_for already ≤ model ceiling, so `.min`
    // never forces UP past what the model supports; `.max(MIN_SERVE_CTX)` keeps it
    // runnable. On a small host where window_for < BOOTSTRAP the cap is a no-op.
    let served_context_window = window_for(lanes as u64)
        .min(demand_ceil.max(MIN_SERVE_CTX))
        .max(MIN_SERVE_CTX);
    // The honest per-lane compute reserve AT the chosen window (floor + window-scaled),
    // reused by the packing math below AND reported to the board via
    // `peak_resident_bytes` — ONE formula (`prefill_compute_reserve`), never two that
    // could drift so resident accounting can't under-charge it.
    let compute_reserve = model.prefill_compute_reserve(served_context_window, lanes);

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
        // Demand the local lanes couldn't absorb (2 slots < N personas is the misfit
        // norm) → the governor must place these on a grid peer, NOT cram them into
        // `lanes` and pay the ~8s KV cache-swap round-trip per turn. 0 when demand fit.
        grid_overflow_lanes: demand_lanes.saturating_sub(lanes),
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

/// Cold-start / no-live-demand convenience: [`plan_serving_with_demand`] with the
/// [`BOOTSTRAP_WORKING_SET`] prior as the demand ceiling. Callers that do not YET
/// thread live per-task demand use this; the elastic path (a hard task requesting
/// more context, or the #234 p95 telemetry) calls `plan_serving_with_demand` with
/// the measured/requested ceiling so the window ebbs and flows with real demand
/// rather than a baked constant.
pub fn plan_serving(
    host: HostBudget,
    candidates: &[ModelFootprint],
    demand_lanes: u32,
) -> Option<ServingPlan> {
    plan_serving_with_demand(host, candidates, demand_lanes, BOOTSTRAP_WORKING_SET)
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
    demand_ceil: u32,
) -> Option<ServingPlan> {
    // NB: do NOT `?`-bail here. A deep transient dip can leave `plan_serving`
    // with nothing fitting the depressed budget (`fresh` = None) while a model
    // is STILL resident and serving fine — its memory is its own. Tearing that
    // down to "nothing" is the exact harm we're guarding against, so `fresh` is
    // an Option we fall back to only when the incumbent genuinely can't hold.
    let fresh = plan_serving_with_demand(host, candidates, demand_lanes, demand_ceil);
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
    plan_serving_with_demand(at_rest, &promoted, demand_lanes, demand_ceil)
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

    // what this catches: #56/G5 — the footprint serving REPORTS to the board is the PEAK
    // (weights + per-lane KV + per-lane prefill compute reserve), and it equals the plan's
    // `chosen_cost` EXACTLY via ONE shared `prefill_compute_reserve` formula. If peak dropped
    // the compute term (the pre-G5 bug: footprint == resident_bytes), the board would
    // over-report free VRAM by the prefill buffer and a second consumer (eval lane, train
    // job) would grab the bytes serving needs for its next prefill → the concurrent-OOM.
    #[test]
    fn peak_resident_equals_plan_chosen_cost_including_window_scaled_compute() {
        let f = fp("coder-sentinel-14b", 9, 90_000, 262_144, 3);
        let c = 11_008u32;
        let lanes = 4u32;
        // The reserve is window-SCALED: floor + compute_rate·C, times lanes.
        let compute_rate = 90_000u64 / PREFILL_COMPUTE_KV_DIVISOR;
        let expect_reserve =
            (f.compute_buffer_per_lane() + compute_rate * c as u64) * lanes as u64;
        assert_eq!(f.prefill_compute_reserve(c, lanes), expect_reserve);
        // Peak = resident + reserve — strictly greater than resident (the pre-G5 report).
        assert_eq!(
            f.peak_resident_bytes(c, lanes),
            f.resident_bytes(c, lanes) + expect_reserve
        );
        assert!(f.peak_resident_bytes(c, lanes) > f.resident_bytes(c, lanes));

        // The keystone invariant: what serving REPORTS (peak) is what the plan SIZED
        // against (chosen_cost) — never two figures that drift. Plan a real serving shape
        // and assert peak_resident_bytes at the plan's own (window, lanes) reproduces the
        // budget the fixpoint consumed for the chosen model.
        let host = HostBudget { usable_bytes: 48 * GB, perf_cores: 10 };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let plan = plan_serving(host, std::slice::from_ref(&devstral), 4).unwrap();
        let chosen_cost = devstral.weights_bytes
            + devstral.kv_at(plan.served_context_window) * plan.lanes as u64
            + devstral.prefill_compute_reserve(plan.served_context_window, plan.lanes);
        assert_eq!(
            devstral.peak_resident_bytes(plan.served_context_window, plan.lanes),
            chosen_cost,
            "the footprint serving reports == the plan's chosen_cost (one formula)"
        );
        // A lane-less / no-window snapshot still charges the per-lane compute floor.
        assert_eq!(
            f.prefill_compute_reserve(0, 0),
            f.compute_buffer_per_lane(),
            "zero window + zero lanes → one lane's compute floor, never zero"
        );
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
        // Demand = 4 personas, but MAX_LANES caps it (reverted to 2 after 4 lanes starved
        // the window to ~6k < a ~9k persona prompt). The window is derived to fit whatever
        // lane count is served — the invariant below holds at any cap.
        let plan = plan_serving(host, std::slice::from_ref(&devstral), 4).unwrap();
        assert!(plan.fits_on_gpu, "{}", plan.rationale);
        assert_eq!(plan.lanes, MAX_LANES, "demand above the cap clamps to MAX_LANES");
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

    // what this catches: the governor now SURFACES demand it can't serve locally instead
    // of silently cramming it into `lanes` and thrashing (the log-proven 65%-LRU / ~8s
    // KV-cache-swap-per-turn bug, 2026-07-27). Demand 4 > 2 local lanes → the excess 2 are
    // reported as `grid_overflow_lanes` — the honest "over local capacity by N" signal the
    // pooled governor routes to a grid peer (Σdemand vs Σresource, one decision; a single
    // box is the pool-of-one case). Demand within capacity → zero overflow. Regression for
    // #234/#56 — the good governor recognizing its own overload, never a silent clamp.
    #[test]
    fn demand_over_local_capacity_surfaces_grid_overflow_not_silent_cram() {
        let host = HostBudget { usable_bytes: 48 * GB, perf_cores: 10 };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        // 4 personas demand a lane; only MAX_LANES fit locally → the rest is overflow.
        let over = plan_serving(host, std::slice::from_ref(&devstral), 4).unwrap();
        assert_eq!(over.lanes, MAX_LANES, "precondition: local lanes clamp to MAX_LANES");
        assert_eq!(
            over.grid_overflow_lanes,
            4 - over.lanes,
            "demand the local lanes couldn't absorb must be surfaced for grid placement, not crammed"
        );
        // Demand within local capacity → zero overflow (nothing to place off-box).
        let fits = plan_serving(host, std::slice::from_ref(&devstral), 1).unwrap();
        assert_eq!(fits.lanes, 1, "precondition: single demand fits one local lane");
        assert_eq!(fits.grid_overflow_lanes, 0, "demand ≤ local lanes → no overflow");
    }

    // what this catches: the cross-node serving-QUALITY bug (2026-07-26). On a roomy
    // host `window_for` maximizes the served window to fill the memory budget — tens
    // of thousands of tokens (the pre-fix devstral shape sized ~53k; live it was 94k),
    // pre-allocating ~33GB of KV × lanes → SWAP on a 64GB Mac (5M swapouts, 1.77 tok/s)
    // and DECODE-WEDGE on a 32GB 5090 — while personas fill only ~9k. The DEMAND cap
    // (BOOTSTRAP_WORKING_SET now, measured p95 later, #234) must hold the served window
    // at the working set, provisioned for USE not for RAM. The ceiling here (131k) is
    // far above the cap, so a served window ≤ BOOTSTRAP proves the DEMAND cap held it
    // (not the ceiling). Regression for M5+BigMama's 94k→swap/wedge.
    #[test]
    fn demand_cap_holds_served_window_at_working_set_not_budget_max() {
        let host = HostBudget { usable_bytes: 48 * GB, perf_cores: 10 };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        assert!(
            devstral.context_window > BOOTSTRAP_WORKING_SET,
            "precondition: model ceiling must exceed the demand cap, else the ceiling (not the cap) could explain the result"
        );
        let plan = plan_serving(host, std::slice::from_ref(&devstral), 4).unwrap();
        assert!(
            plan.served_context_window <= BOOTSTRAP_WORKING_SET,
            "served window {} must be capped to working-set demand ({}), never budget-maxed toward the ceiling",
            plan.served_context_window,
            BOOTSTRAP_WORKING_SET
        );

        // The cap NEVER inflates a small-ceiling model UP toward BOOTSTRAP — a model
        // whose trained ceiling is below the demand cap is still served at/below its
        // own ceiling (`.min` only ever caps DOWN).
        let tiny = fp("tiny-4k", 3, 30_000, 4_096, 2);
        let plan_tiny = plan_serving(host, std::slice::from_ref(&tiny), 1).unwrap();
        assert!(
            plan_tiny.served_context_window <= 4_096,
            "demand cap must not inflate a 4k-ceiling model past its ceiling; got {}",
            plan_tiny.served_context_window
        );
    }

    // what this catches: the ELASTIC demand ceiling (Joel 2026-07-27: "context window sizes
    // should ebb and flow depending on demands of the task and available resources — if it
    // needs it larger for a moment, don't limit it"). The ceiling is threaded LIVE, not a
    // launch-baked constant: a hard task passing a HIGH demand_ceil grows the served window
    // PAST the BOOTSTRAP prior (up to the budget/model bound); a LOW ceiling shrinks it so
    // more lanes fit. OOM-safe — window_for still bounds it, so a higher ceiling only raises
    // the cap toward the budget bound, never past it. This is the "stop setting it in stone
    // at launch" fix that opens the elastic-lease path.
    // what this catches: the serving→grid bridge (#180 spill) — a model footprint maps to a
    // capacity-fabric LeaseRequest whose want_concurrency is the demanded lanes and whose
    // spike_bytes is ONE lane's transient prefill compute reserve at the served window (the
    // 2026-07-14 OOM term), so GridPlacementPolicy offers a peer only a spill lane it can hold.
    #[test]
    fn grid_lease_request_maps_demand_and_the_prefill_spike() {
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let window = 16_384;
        let lease = devstral.grid_lease_request(window, 3);
        assert_eq!(lease.consumer, "devstral-24b");
        assert_eq!(lease.want_concurrency, 3, "want_concurrency = demanded lanes");
        assert_eq!(
            lease.spike_bytes,
            devstral.prefill_compute_reserve(window, 1),
            "spike_bytes = ONE lane's prefill compute reserve at the served window"
        );
        // Zero demand floors at one lane — never a degenerate 0-concurrency lease.
        assert_eq!(devstral.grid_lease_request(window, 0).want_concurrency, 1);
    }

    #[test]
    fn demand_ceiling_is_elastic_grows_for_a_hard_task_shrinks_for_a_simple_one() {
        let host = HostBudget { usable_bytes: 48 * GB, perf_cores: 10 };
        // Roomy host + high trained ceiling → window_for(1) far exceeds any of these ceilings,
        // so the DEMAND ceiling (not the budget or the model) decides the served window.
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);

        // Cold prior: default plan_serving caps at BOOTSTRAP_WORKING_SET.
        let cold = plan_serving(host, std::slice::from_ref(&devstral), 1).unwrap();
        assert_eq!(cold.served_context_window, BOOTSTRAP_WORKING_SET);

        // Hard task demands more context → the window GROWS past the prior.
        let big_ceil = 64_000;
        let hot =
            plan_serving_with_demand(host, std::slice::from_ref(&devstral), 1, big_ceil).unwrap();
        assert!(
            hot.served_context_window > cold.served_context_window,
            "a higher demand ceiling must GROW the window: hot {} ≤ cold {}",
            hot.served_context_window,
            cold.served_context_window
        );
        assert!(
            hot.served_context_window <= big_ceil,
            "growth stays bounded by the demand ceiling (and the budget), never past it: got {}",
            hot.served_context_window
        );

        // Simple turn demands little → the window SHRINKS below the prior, freeing memory.
        let lean =
            plan_serving_with_demand(host, std::slice::from_ref(&devstral), 1, 8_192).unwrap();
        assert_eq!(
            lean.served_context_window, 8_192,
            "a low demand ceiling shrinks the served window to it"
        );
    }

    // what this catches: lanes DEGRADE on a tight host — MAX_LANES is a sanity backstop,
    // the fit math is the real cap. A 4-persona demand on a budget that can't feed 4 warm
    // slots must serve FEWER (well-fed) lanes, never 4 starving ones that OOM. The window
    // still fits every lane it does serve. Guards the 2→4 raise from re-introducing the
    // OOM on a small box.
    #[test]
    fn lanes_degrade_on_a_tight_host_never_overcommitting() {
        // ~18GB usable — fits the 14GB weights + a couple lanes' KV, not four.
        let host = HostBudget { usable_bytes: 18 * GB, perf_cores: 10 };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let plan = plan_serving(host, std::slice::from_ref(&devstral), 4).unwrap();
        assert!(plan.fits_on_gpu, "{}", plan.rationale);
        assert!(plan.lanes < 4, "tight host must serve fewer than the 4 demanded: got {}", plan.lanes);
        assert!(plan.lanes >= 1);
        let lanes = plan.lanes as u64;
        let c = plan.served_context_window as u64;
        let compute_floor = devstral.compute_buffer_per_lane();
        let compute_rate = devstral.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
        let footprint = devstral.weights_bytes
            + devstral.kv_at(c as u32) * lanes
            + (compute_floor + compute_rate * c) * lanes;
        let effective = (host.usable_bytes as f64 * (1.0 - CO_CONSUMER_HEADROOM)) as u64;
        assert!(footprint <= effective, "degraded plan still overcommits: {footprint} > {effective}");
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

    // what this catches: #213 — the daemon must not chase a lane-count target off a cliff.
    // When a mid-session VRAM squeeze (an eval lane spinning up, a game) leaves room for
    // only ONE real window, demand for 2 concurrent lanes must SHED a lane (queue) rather
    // than collapse BOTH minds to the MIN_SERVE_CTX floor — a 2048-token mind can't hold its
    // own memory + the task. Live specimen 2026-07-20: Devstral re-homed to 2 lanes @ 2048
    // with the eval lane resident, while 1 lane would have served ~30k. The old math capped
    // lanes by what fit at the FLOOR window, so it admitted 2 and let the window collapse;
    // the fix picks the largest lane count whose per-lane window still clears the floor.
    #[test]
    fn a_squeeze_sheds_a_lane_rather_than_flooring_every_mind() {
        let devstral = fp("devstral-24b", 14, 175_000, 131_072, 3);
        // Budget where ONE lane serves a real window but TWO would floor (eval lane resident).
        let squeezed = HostBudget { usable_bytes: 19 * GB, perf_cores: 6 };
        let plan = plan_serving(squeezed, std::slice::from_ref(&devstral), 2).unwrap();
        assert_eq!(plan.lanes, 1, "2 lanes would floor → shed to 1 real lane, not 2 @ 2048");
        assert!(
            plan.served_context_window > 4096,
            "the surviving lane gets a window big enough to think in, got {}",
            plan.served_context_window,
        );
        // And when there IS room, demand is honored: concurrency preserved AND each
        // mind at a real, DEMAND-sized window — capped at the working-set bootstrap
        // (2026-07-26), NOT maximized to fill RAM (the 94k→swap/wedge bug). A roomy
        // box buys more LANES (concurrent minds), not a bloated per-lane KV cache.
        let roomy = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let plan2 = plan_serving(roomy, std::slice::from_ref(&devstral), 2).unwrap();
        assert_eq!(plan2.lanes, 2, "roomy host serves both minds concurrently");
        assert!(
            plan2.served_context_window > 4096
                && plan2.served_context_window <= BOOTSTRAP_WORKING_SET,
            "each mind at a real window, demand-capped not budget-maxed, got {}",
            plan2.served_context_window,
        );
    }

    // what this catches: the M5 Pro must USE its headroom — pick the most capable
    // model (the 14B coding sentinel) and run multiple concurrent lanes. But the
    // per-lane WINDOW sizes to DEMAND, not to the budget (M5+BigMama 2026-07-26):
    // a roomy box buys more MINDS (lanes), not a bloated per-lane KV cache that
    // pre-allocates ~33GB and swaps (the 94k bug). This REPLACES the old
    // "size the window UP from the budget" contract — window scales with demand
    // (BOOTSTRAP_WORKING_SET cold-start, measured p95 later, #234), never RAM.
    // (Supersedes the #216 "don't cheapen the window" framing: right-sizing to a
    // 16k+ working set is not cheapening — flooring below the ~9k turn was. The
    // coding-headroom question — a bigger BOOTSTRAP for coders — is #234 A-next.)
    #[test]
    fn big_box_picks_most_capable_runs_lanes_but_sizes_window_to_demand() {
        // ~45GB usable on a 64GB M5 Pro after headroom.
        let host = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let plan = plan_serving(host, &candidates(), MAX_LANES).unwrap();
        assert_eq!(plan.base_model_id, "coder-sentinel-14b", "most capable, fits easily");
        assert!(plan.lanes >= 2, "M5 Pro has the budget for multiple lanes, got {}", plan.lanes);
        // Window sized to DEMAND (capped at the working-set bootstrap), NOT maximized
        // to fill the 45GB budget — the fix for the cross-node swap/wedge. Still a
        // real window (well above the ~9k persona turn), never exceeds the ceiling.
        assert!(
            plan.served_context_window > 4096
                && plan.served_context_window <= BOOTSTRAP_WORKING_SET,
            "served window sizes to demand (≤ bootstrap cap), not budget-maxed, got {}",
            plan.served_context_window,
        );
        assert!(
            plan.served_context_window <= 262_144,
            "never exceed the model's trained ceiling, got {}",
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
            plan_serving_stable(host, &pair(), None, MAX_LANES, BOOTSTRAP_WORKING_SET),
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
        let stable = plan_serving_stable(host, &pair(), Some("small"), MAX_LANES, BOOTSTRAP_WORKING_SET).unwrap();
        assert_eq!(stable.base_model_id, "small", "hysteresis keeps incumbent — no flap");
        assert!(stable.lanes >= 1, "lanes still re-tracked for the kept model");
    }

    // what this catches: a genuine upgrade DOES happen when the better model
    // fits with headroom — hysteresis isn't a permanent lock-in.
    #[test]
    fn stable_upgrades_when_better_model_fits_with_headroom() {
        let host = HostBudget { usable_bytes: 20 * GB, perf_cores: 6 }; // big 9.7 << 0.9*20=18
        let stable = plan_serving_stable(host, &pair(), Some("small"), MAX_LANES, BOOTSTRAP_WORKING_SET).unwrap();
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
        let stable = plan_serving_stable(host, &only_small, Some("big"), MAX_LANES, BOOTSTRAP_WORKING_SET).unwrap();
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
        let stable = plan_serving_stable(dipped, &pair(), Some("big"), MAX_LANES, BOOTSTRAP_WORKING_SET).unwrap();
        assert_eq!(stable.base_model_id, "big", "incumbent survives its OWN load dip — no flap");
        assert!(stable.lanes >= 1, "kept model still gets ≥1 lane");
    }
}
