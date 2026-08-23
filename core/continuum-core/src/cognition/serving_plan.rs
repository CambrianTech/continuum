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
pub const MAX_LANES: u32 = 8;
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
//
// ✅ 2026-08-09 RAISED 2 → 8 (#266). The 2026-07-17 revert diagnosed the failure correctly
// (a starved per-slot window) but fixed it in the WRONG place: it clamped the lane CEILING,
// when the real defect was that the lane-COUNT gate below sheds against `MIN_SERVE_CTX`
// (2048, the "runnable at all" floor) — far below the ~9k a live turn needs — so a raised
// ceiling let 4 slots @ 6k through. That is now fixed at the true seam: the shed in
// `plan_serving` requires each slot to clear `BOOTSTRAP_WORKING_SET` (16384 ≈ a full
// assembled turn + generation headroom), the SAME "one full turn" figure the demand cap
// uses. With that usable-window floor as the binding constraint, a slot per resident persona
// is safe BY CONSTRUCTION — the plan only grows lanes toward the resident population while
// each still gets a full turn, and sheds (surfacing `grid_overflow_lanes` + a probe) the
// moment the floor would be breached, never below it. The 2026-07-17 "solve clobber with
// duty-cycling, not lanes" stance was itself the bug this leaves on the table: cross-turn
// KV clobber IS #266's whole latency story (measured: 96.3% of persona compute is prefill;
// two of four citizens at 0.0% cache reuse across 10 turns — the LRU eviction of a warm
// slot the resident count outnumbered). Giving each resident mind its own slot is what
// keeps its prefilled prefix warm. This constant is once again a pure SANITY backstop
// (pathological roster + llama.cpp `--parallel` practicality); the binding constraint is
// the window floor, exactly as the earlier doc always claimed it should be.

/// Bare-minimum served window for a model to be runnable at ALL — a hardware
/// reality floor, NOT a serving target or a cheapening cap. The served window is
/// sized UP from here to the largest that fits the host budget, capped by BOTH
/// the model's own trained `context_window` AND the DEMAND ceiling
/// ([`BOOTSTRAP_WORKING_SET`] / measured p95) — a lane is provisioned for what
/// personas actually USE, never maximized to fill RAM. A model whose weights + KV
/// at even this floor won't fit the GPU budget is simply not a serving option on
/// this host (→ `fits_on_gpu = false`, honest degrade — never a silent shrink).
// context-budget-exempt: the hardware FLOOR the whole serving stack sizes UP from — the one substrate-owned minimum every other bound derives against, and never a cap on anything
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

/// What the minds on this host are asking the serving lane for.
///
/// Both axes of demand in ONE value, because they are one question — "how much
/// serving does the work on this box actually need" — and passing them as two loose
/// `u32`s next to each other is how a caller silently swaps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServingDemand {
    /// How many minds want a concurrent lane.
    pub lanes: u32,
    /// The largest per-turn window any resident mind has actually demanded
    /// ([`crate::cognition::working_set::WorkingSetRegistry::ceiling`]) — measured
    /// UNCLAMPED, so it is free to exceed what is currently served. That excess is the
    /// signal that the window is too small; nothing else in the system can produce it.
    pub window_tokens: u32,
    /// Whether `window_tokens` came from MEASUREMENT or from [`BOOTSTRAP_WORKING_SET`].
    ///
    /// The two are not interchangeable and the plan must not present them as if they
    /// were. Measured 2026-08-20: after a reboot the plan served 16384 and reported
    /// `bound_by=demand` — i.e. "the minds asked for exactly this much" — when no mind
    /// had asked for anything at all, because no turn had been measured yet. That is a
    /// receipt asserting a fact nobody established, the [[#151]]/[[#357]] class, and it
    /// is the more dangerous kind: the probe's own doc says `bound_by` exists so a
    /// window that fails to grow is falsifiable, distinguishing "the host could not fit
    /// more" from "the measured demand did not ask for more". A bootstrap prior wearing
    /// the `demand` label defeats exactly that.
    pub measured: bool,
}

impl ServingDemand {
    /// Demand from live measurement, with the cold-start case named explicitly.
    ///
    /// `measured` is `None` only before ANY turn has been assembled on this host —
    /// a genuine absence of data, not a missing feature. The window then starts at
    /// [`BOOTSTRAP_WORKING_SET`] (one full turn's worth: assembled prompt plus
    /// generation headroom, anchored to `MIN_SERVE_CTX × 8` rather than a second bare
    /// number) and is superseded by measurement on the very next plan, because the
    /// first turn records its demand before it is even sent. This is the one place
    /// that decision lives — the registry deliberately returns `None` rather than
    /// inventing a number every caller would then inherit without noticing.
    pub fn new(lanes: u32, measured: Option<u32>) -> Self {
        Self {
            lanes,
            window_tokens: measured.unwrap_or(BOOTSTRAP_WORKING_SET), // JUSTIFIED unwrap_or: cold start is a real state, not a missing measurement; the substituted value is a declared PRIOR and its provenance is preserved on `measured` below rather than discarded here
            measured: measured.is_some(), // the provenance `unwrap_or` would otherwise destroy — UNKNOWN must stay distinguishable from a quantity

        }
    }
}

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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/serving/ModelFootprint.ts"
)]
#[serde(rename_all = "camelCase")]
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
        self.weights_bytes.saturating_add(
            self.kv_at(served_window)
                .saturating_mul(lanes.max(1) as u64),
        )
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
}

/// The serving decision for this host.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/serving/ServingPlan.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ServingPlan {
    /// The base model to serve (shared across lanes) — the RESOLVED
    /// [`ModelFootprint`], not a name.
    ///
    /// This was `base_model_id: String`, cloned out of the chosen candidate at
    /// plan time. That clone was the bug: the planner HELD the footprint, threw
    /// it away, kept the name, and every consumer that needed a real fact
    /// (capability_rank, weights_bytes, context_window) had to search the
    /// candidate list back by string equality — and got `None` when the name
    /// referred to a model nothing had loaded. A citizen homed that way calls a
    /// model the gateway isn't serving, the call fails with 0 tokens in 0ms, the
    /// turn is skipped "retries next tick", and it retries forever because the
    /// stale name can never self-correct. Measured 2026-08-14: two citizens
    /// (Asha, Anwen) silent on `bartowski/Qwen2.5-Coder-7B-Instruct-GGUF`
    /// against a gateway serving only Devstral, while a third homed correctly
    /// spoke a full 339s turn.
    ///
    /// Carrying the struct makes that state unrepresentable rather than merely
    /// detectable: a plan cannot name a model it does not hold
    /// ([[pass-the-model-struct-no-param-hell]] — the rule this file's own
    /// `ModelFootprint` doc already cited three lines above the clone).
    pub base_model: ModelFootprint,
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
/// The decision is pure classification on memory arithmetic — no model is
/// loaded, no inference is run.
pub fn plan_serving(
    host: HostBudget,
    candidates: &[ModelFootprint],
    demand: ServingDemand,
) -> Option<ServingPlan> {
    let demand_lanes = demand.lanes;
    if candidates.is_empty() {
        return None;
    }

    // GPU-viable = weights + at least one lane's KV fit the honest budget AT a
    // given per-slot window. WHICH window is the whole question (#438-class):
    //
    // This filter used to be hardcoded to `MIN_SERVE_CTX` (2048) — bare survival.
    // That made "viable" mean "can technically hold a 2k window", so selection
    // always crowned the most capable model that cleared a trivial bar and then
    // let the window collapse to it. Glass-boxed live 2026-08-17 on this box:
    // a 27B chosen at `usable_gb=5`, `served_window=2048` against a MEASURED
    // `demand_window=63817` — a mind handed 3% of the context its own turn needs.
    // Every act it took was against a window too small to hold the task statement.
    //
    // The inconsistency was INTERNAL to this function: the lane-count loop below
    // already refuses to add a slot that can't clear `BOOTSTRAP_WORKING_SET`
    // ("one full turn"), because 1 lane @ 30k beats 2 lanes @ 2k. But when even
    // ONE lane couldn't clear that floor it fell back to `.unwrap_or(1)` and
    // called the result "honest starvation, surfaced downstream" — while the
    // MODEL was never reconsidered. Shedding a lane and shedding capability are
    // the same move for the same reason; only the first was implemented. The
    // correct response to "one lane can't hold a turn" is a SMALLER MODEL that
    // can, not the bigger model blinded. A model that fits only at 2048 is not
    // more capable on this host — it is unusable on this host.
    //
    // So viability is now tested at the SAME "one full turn" standard the lane
    // floor uses, with a documented degrade: if NO candidate clears it, fall back
    // to the old `MIN_SERVE_CTX` bar so a genuinely tiny host still serves
    // something rather than nothing (honest starvation is then real, not a
    // selection artifact). Deliberately the STABLE bootstrap constant and NOT the
    // moving measured p95 — coupling model CHOICE to a jittering demand signal is
    // the 718-replan flap that wedged three benchmark runs (see the lane floor's
    // own note). The served window still refines with measurement below.
    let fits_one_lane_at = |m: &ModelFootprint, ctx: u32| {
        m.weights_bytes.saturating_add(m.kv_at(ctx)) <= host.usable_bytes
    };

    // Prefer the MOST CAPABLE model that fits a lane. Ties broken toward the
    // larger model (more headroom spent = the more capable variant), then by
    // id descending for deterministic selection.
    let most_capable_fitting = |ctx: u32| {
        candidates
            .iter()
            .filter(|m| fits_one_lane_at(m, ctx))
            .max_by(|a, b| {
                a.capability_rank
                    .cmp(&b.capability_rank)
                    .then(a.weights_bytes.cmp(&b.weights_bytes))
                    .then(b.model_id.cmp(&a.model_id))
            })
    };
    let best = most_capable_fitting(BOOTSTRAP_WORKING_SET)
        .or_else(|| most_capable_fitting(MIN_SERVE_CTX));

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
            base_model: (*smallest).clone(),
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
        (((after_compute / l) / per_token_cost).min(u32::MAX as u64) as u32)
            .min(model.context_window)
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
    // Each RESIDENT persona wants its OWN warm slot so its prefilled KV survives across turns
    // (no LRU clobber → no ~10k cold re-prefill every turn — #266's whole latency story: 96%
    // prefill, two of four citizens at 0% cache reuse when 4 minds shared 2 slots). Grow the
    // lane count toward the resident population (`demand_lanes`, set from the persona floor),
    // but ONLY while each slot still clears the full-turn usable floor: adding a slot that
    // drops the per-slot window below one assembled turn trades a warm-but-blind mind for a
    // warm one that can't see — the exact 4-lanes-@-6k starvation the 2026-07-17 revert caught
    // (it clamped the CEILING; the real fix is gating the COUNT on this floor). So pick the
    // LARGEST lane count whose per-slot window ≥ the floor; if even one slot can't clear it (a
    // genuinely tiny host), fall back to 1 — honest starvation, surfaced downstream — never
    // below. The floor is `BOOTSTRAP_WORKING_SET` (≈ a ~9k prompt + generation headroom), the
    // SAME "one full turn" constant the demand cap uses (§`BOOTSTRAP_WORKING_SET`), deliberately
    // the STABLE bootstrap value and NOT the moving measured p95: coupling the lane COUNT to a
    // jittering demand signal is the 718-replan lane-flap that wedged three benchmark runs. The
    // served WINDOW still refines with measurement (below); the slot COUNT rests on a fixed
    // floor. THIS floor — not the `MAX_LANES` backstop — is the binding constraint (#266).
    let lanes = (1..=lane_cap)
        .rev()
        .find(|&l| window_for(l as u64) >= BOOTSTRAP_WORKING_SET)
        .unwrap_or(1);
    // Over-subscription: more resident personas than warm slots the window floor permits. The
    // remainder can't get a persistent slot — with N minds on M<N slots the llama.cpp per-slot
    // LRU eviction re-prefills a cold ~10k prefix every time an evicted mind speaks (#266). The
    // honest node ceiling is "how many minds fit warmly at a full-turn window", and exceeding it
    // is a real condition to make VISIBLE, never silently absorb: `grid_overflow_lanes` (below)
    // carries the same count to the governor for off-box placement, and the serving daemon's
    // adopted-plan probe names it so "0% cache reuse" can't go unseen for weeks.
    // NO probe here for oversubscription (#399): this is a PURE planning function
    // and `plan_serving_stable` calls it TWICE per tick with different budgets
    // (fresh + at-rest credit), so any per-call emission — even dedup'd through a
    // static — alternates between the two call sites and floods anyway (measured
    // live 2026-08-16: a single-slot dedup static still emitted 260 rows/3min
    // because the call sites' lane counts, 1 vs 2, defeated it every tick). The
    // condition already rides the RETURNED plan (`grid_overflow_lanes`, and
    // lanes < demand is recomputable from plan + demand); the serving daemon
    // probes it for the ADOPTED plan only, behind its emit-on-change gate.
    // DEMAND cap (M5+BigMama 2026-07-26): provision for what personas USE, not for
    // what RAM allows. `window_for` maximizes the window to fill the budget (94k on a
    // roomy host) → ~33GB pre-allocated KV × lanes → swap/wedge. Cap DOWN to demand.
    //
    // That demand is now MEASURED (`demand.window_tokens`, from
    // [`crate::cognition::working_set`]) rather than the `BOOTSTRAP_WORKING_SET`
    // constant it used to be — which is what this line's own comment promised
    // ("measured p95 later, #234") and never delivered. The constant was doing real
    // damage in the meantime: 16384 across 2 lanes gave each citizen **8192 tokens**
    // of a 128k-capable model on a host that could serve 94k, and measured 2026-08-06
    // that left a median context budget of 55 tokens after framing and conversation —
    // the work board (median 5,364 tokens) reached a prompt zero times in 495.
    //
    // The measurement is DEMAND, not usage, precisely so it can exceed what is
    // currently served; a usage-based signal would re-derive whatever cap produced it.
    // Ordering is unchanged and the safety envelope is identical: `window_for` is
    // already ≤ the model's trained ceiling AND ≤ what the host fits, so `.min` never
    // forces UP past either, and `.max(MIN_SERVE_CTX)` keeps the lane runnable. A
    // citizen who demands more than the machine has simply receives what fits.
    let served_context_window = window_for(lanes as u64)
        .min(demand.window_tokens)
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
        .saturating_add(
            model
                .kv_at(served_context_window)
                .saturating_mul(lanes as u64),
        )
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
        base_model: model.clone(),
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

/// Hysteresis wrapper around [`plan_serving`]: stops model THRASH from live-
/// budget jitter. Keeps the `incumbent` model as long as it still fits the
/// budget — switching DOWN only when the incumbent no longer fits (forced
/// eviction) and UP only when a strictly more capable model fits with
/// [`SWITCH_UP_HEADROOM`] to spare.
///
/// Lanes and window are sized against the AT-REST budget (the live budget with the
/// incumbent's own resident weights credited back) for as long as the incumbent keeps
/// serving — so a model's own KV can never convince the planner to shed the lane that model
/// is currently running. That self-eviction was a real lane flap, not a theoretical one; see
/// the note in the body. No incumbent (or it's gone / no longer fits) → plain
/// [`plan_serving`]. Use this for the ONGOING serving loop; boot uses
/// `plan_serving` directly (no incumbent yet).
pub fn plan_serving_stable(
    host: HostBudget,
    candidates: &[ModelFootprint],
    incumbent: Option<&str>,
    demand: ServingDemand,
) -> Option<ServingPlan> {
    // NB: do NOT `?`-bail here. A deep transient dip can leave `plan_serving`
    // with nothing fitting the depressed budget (`fresh` = None) while a model
    // is STILL resident and serving fine — its memory is its own. Tearing that
    // down to "nothing" is the exact harm we're guarding against, so `fresh` is
    // an Option we fall back to only when the incumbent genuinely can't hold.
    let Some(inc_id) = incumbent else {
        return plan_serving(host, candidates, demand);
    };
    // NOTE (2026-08-04): there used to be an early return here — "fresh already chose the
    // incumbent → nothing to stabilize" — and it was the lane-flap bug. `fresh` is computed
    // against the LIVE budget, which the incumbent's own weights + KV depress while it serves.
    // On the same-model path that meant the planner re-derived lane count from a budget the
    // incumbent itself had eaten, decided it could no longer afford the lane it was already
    // running, dropped to 1, then added it back when the KV freed. Glass-boxed on an IDLE host:
    // 718 replans in one solve, `usable_gb` swinging 26→6, `lanes` oscillating 1↔2, and every
    // flip resizing the live admission semaphore (`set_served_lane_count`) and the prefill
    // throttle under in-flight requests — which is the `no response headers for 300s` lane wedge
    // that killed three benchmark runs.
    //
    // The at-rest credit below already existed for exactly this reason ("so a model's OWN
    // load/residency can never flap it out"); it was simply never applied to the case where the
    // incumbent KEEPS serving. So the paths are unified: whenever a living incumbent is still
    // servable, its plan is sized against the at-rest budget. Same-model is no longer a
    // shortcut — it is the main path. [[never-thrash-sticky-hysteresis-on-every-lane]]
    //
    // Incumbent dropped off disk entirely → honour whatever `fresh` chose
    // (possibly None = nothing servable).
    let Some(inc) = candidates.iter().find(|m| m.model_id == inc_id) else {
        return plan_serving(host, candidates, demand);
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
    // `fresh` is computed against the POST-EVICTION budget, and that is the whole fix.
    //
    // It used to use `host` — the live budget WITH the incumbent still resident — so the
    // planner asked "does the better model fit in the memory left over beside the one already
    // loaded?". That question is wrong, because a swap is never co-resident: `serve()` kills
    // the incumbent llama-server child and THEN launches the candidate, exactly as
    // `pin_fit_decision` documents ("the candidate only needs to fit AFTER the incumbent's
    // VRAM is reclaimed"). So the eviction credit-back was written, proven, and wired to the
    // PIN path only, while the autonomic planner kept asking the co-resident question.
    //
    // MEASURED on this M5 (2026-08-19): `serving.plan` chose Devstral-24B at `usable_gb: 18`
    // on a 64 GB box, because Devstral + the vision lane + embeddings were already resident.
    // Qwen3.8-27B (18.97 GB of weights) cannot fit a lane inside 18 GB, so it never entered
    // the plan. `serving/pin` — same models, same instant, same machine — computed
    // `budget_gb: 34.09` via this credit and the 27B fit with 15 GB to spare. Two answers to
    // one question, and only the operator-driven path could reach the better model.
    //
    // This is the third sighting of ONE defect: the planner could not reason about a budget
    // the incumbent was occupying. #214 fixed it for the WINDOW (grow-back), the pin path had
    // it for the MODEL, and #266's warm slots starve for the same reason — lanes are sized
    // from a budget the resident model has already eaten, so 3 of 4 citizens got no warm slot
    // and re-prefilled cold every turn. One credit, three cards.
    //
    // SAFE because it is still gated: the switch-up below additionally requires strictly
    // greater `capability_rank` AND `SWITCH_UP_HEADROOM` of margin, so a bigger model is
    // adopted only when it is genuinely better AND genuinely fits post-eviction. A transient
    // budget bump cannot flap it. An EXTERNAL squeeze is still not credited back — only the
    // incumbent's own committed weights are — so a real over-commit still forces the
    // down-switch. [[never-thrash-sticky-hysteresis-on-every-lane]]
    let fresh = plan_serving(at_rest, candidates, demand);
    // Even crediting its own residency, can the incumbent still hold a lane? If
    // not, a real squeeze has genuinely evicted it → take `fresh`.
    if inc.weights_bytes.saturating_add(inc.kv_at(MIN_SERVE_CTX)) > at_rest.usable_bytes {
        return fresh;
    }
    // Switch UP to `fresh` ONLY if it is strictly more capable AND fits the POST-EVICTION
    // budget with headroom. This used to test against `host` on the reasoning that "loading a
    // bigger NEW model needs actual free memory" — true for a co-resident load, false for the
    // swap `serve()` actually performs (kill incumbent, then launch). Testing against `host`
    // made the gate unreachable for any model larger than the free remainder, which on a
    // healthy box IS every upgrade worth making. `SWITCH_UP_HEADROOM` still supplies the
    // anti-flap margin; it is now margin on the right budget.
    let headroom_budget = (at_rest.usable_bytes as f64 * (1.0 - SWITCH_UP_HEADROOM)) as u64;
    // The plan HOLDS its model, so this is a field read, not a search. It used to be
    // `candidates.iter().find(|c| c.model_id == f.base_model_id)` — a name lookup that
    // returned None whenever the planned name wasn't in the candidate list, silently
    // making `is_some_and` false and suppressing a legitimate switch-up.
    let upgrade_worth_it = fresh.as_ref().map(|f| &f.base_model).is_some_and(|f| {
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
    plan_serving(at_rest, &promoted, demand)
}

fn bytes_gb(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    // what this catches: a bootstrap prior being reported as measured demand. Live
    // 2026-08-20 the plan served 16384 with `bound_by=demand` and 33 GB of VRAM free —
    // reading as "the minds asked for exactly this", when no turn had been measured at
    // all. The number is fine as a PRIOR; laundering it into a measurement is what made
    // the window's failure to grow unfalsifiable. Pins provenance to survive `unwrap_or`.
    #[test]
    fn a_bootstrap_window_never_claims_to_be_measured_demand() {
        let cold = ServingDemand::new(4, None);
        assert_eq!(cold.window_tokens, BOOTSTRAP_WORKING_SET);
        assert!(
            !cold.measured,
            "no turn has been assembled, so the window is a prior — not something a mind asked for"
        );

        let warm = ServingDemand::new(4, Some(31_834));
        assert_eq!(warm.window_tokens, 31_834);
        assert!(warm.measured);

        // The degenerate case that makes the flag load-bearing rather than cosmetic: a
        // real measurement that happens to EQUAL the bootstrap value is still measured,
        // so the two states can never be told apart by comparing the number alone.
        let coincidence = ServingDemand::new(1, Some(BOOTSTRAP_WORKING_SET));
        assert_eq!(coincidence.window_tokens, cold.window_tokens);
        assert!(
            coincidence.measured && !cold.measured,
            "identical windows, opposite provenance — the value cannot carry this fact"
        );
    }

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
            fp("qwen2.5-0.5b", 1, 4_000, 32_768, 1),         // tiny chat
            fp("qwen3.5-4b", 3, 30_000, 262_144, 2),         // good general (47 tok/s on M5)
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
        let expect_reserve = (f.compute_buffer_per_lane() + compute_rate * c as u64) * lanes as u64;
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
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let plan = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, None),
        )
        .unwrap();
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

    // what this catches: the governor serving a mind a window too small to think in
    // because MODEL CHOICE was floored at bare survival while LANE COUNT was floored at
    // a full turn. Glass-boxed live 2026-08-17: a 27B picked at usable_gb=5 →
    // served_window=2048 against a measured demand_window=63817, and every SWE act ran
    // against a context that could not hold the task statement. The pre-fix filter asked
    // "can this model hold MIN_SERVE_CTX (2048)?", so the biggest model always won and
    // then starved the window to that trivial bar. Shedding a lane and shedding
    // capability are the same move for the same reason (1 lane @ 30k beats 2 lanes @ 2k;
    // a 14B @ 30k beats a 27B @ 2k) — only the first had been implemented.
    #[test]
    fn model_choice_sheds_capability_rather_than_starve_the_window() {
        // Big model clears 2048 but NOT a full turn; small model clears a full turn.
        let big = fp("big-27b", 28, 256 * 1024, 131_072, 5);
        let small = fp("small-14b", 10, 64 * 1024, 131_072, 3);
        let candidates = [big.clone(), small.clone()];
        let demand = ServingDemand::new(1, None);

        // 30GB: big fits ONLY at the survival floor (28 + 0.5 ≤ 30, but 28 + 4.3 > 30).
        // Pre-fix this crowned `big` and served 2048. It must now pick `small`.
        let plan = plan_serving(
            HostBudget { usable_bytes: 30 * GB, perf_cores: 10 },
            &candidates,
            demand,
        )
        .expect("a model fits");
        assert_eq!(
            plan.base_model.model_id, "small-14b",
            "a model that fits only at the 2048 survival floor is not a serving option \
             on this host — shed capability, never starve the window"
        );
        assert!(
            plan.served_context_window >= BOOTSTRAP_WORKING_SET,
            "the served window must clear one full turn ({BOOTSTRAP_WORKING_SET}), got {}",
            plan.served_context_window
        );

        // NEGATIVE CONTROL — the floor must not cost capability when the host can afford
        // it. On a roomy host the 27B clears a full turn and must still win, otherwise
        // this fix would have silently downgraded every capable box.
        let roomy = plan_serving(
            HostBudget { usable_bytes: 64 * GB, perf_cores: 10 },
            &candidates,
            demand,
        )
        .expect("a model fits");
        assert_eq!(
            roomy.base_model.model_id, "big-27b",
            "when the budget clears a full turn, the MOST capable model still wins"
        );

        // DEGRADE PATH — when NO candidate clears a full turn, fall back to the old
        // survival bar so a genuinely tiny host serves something rather than nothing.
        // Honest starvation is then real, not an artifact of the selection rule.
        let tiny = plan_serving(
            HostBudget { usable_bytes: 11 * GB, perf_cores: 4 },
            &candidates,
            demand,
        )
        .expect("the survival fallback still yields a model");
        assert_eq!(tiny.base_model.model_id, "small-14b");
        assert!(tiny.fits_on_gpu, "the fallback still serves on GPU, just narrowly");
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
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3); // ~112 KiB/token KV
                                                                       // Demand = 4 personas. This roomy 48GB host clears the full-turn window floor at 4
                                                                       // slots (#266: a warm slot per resident persona), so all 4 are served — the window is
                                                                       // derived to fit whatever lane count is served, and the fit invariant below holds at
                                                                       // any count.
        let plan = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, None),
        )
        .unwrap();
        assert!(plan.fits_on_gpu, "{}", plan.rationale);
        assert_eq!(
            plan.lanes, 4,
            "roomy host gives each of the 4 resident personas its own warm slot"
        );
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
        assert!(
            c < naive,
            "window-scaled + headroom reserve must shrink the window ({c} < {naive})"
        );
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
        // 26GB: enough for the 14GB weights + 2 warm slots at a full-turn window, but NOT 4 —
        // the window floor (#266) caps warm slots at 2, so 2 of the 4 resident personas can't
        // get a persistent slot locally. That excess is the honest overflow, surfaced (not
        // crammed onto shared slots to thrash) for the governor to place off-box.
        let host = HostBudget {
            usable_bytes: 26 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let over = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, None),
        )
        .unwrap();
        assert_eq!(
            over.lanes, 2,
            "precondition: the full-turn window floor caps warm slots at 2 here"
        );
        assert_eq!(
            over.grid_overflow_lanes,
            4 - over.lanes,
            "demand the local warm slots couldn't absorb must be surfaced for grid placement, not crammed"
        );
        // Demand within local capacity → zero overflow (nothing to place off-box).
        let fits = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(1, None),
        )
        .unwrap();
        assert_eq!(
            fits.lanes, 1,
            "precondition: single demand fits one local lane"
        );
        assert_eq!(
            fits.grid_overflow_lanes, 0,
            "demand ≤ local warm slots → no overflow"
        );
    }

    // what this catches: #266 — the slot count sizes to the RESIDENT PERSONA POPULATION so
    // each mind keeps a persistent warm slot (its prefilled KV survives across turns), clamped
    // by the full-turn window floor. The pre-fix `MAX_LANES = 2` clamp forced 4 resident minds
    // onto 2 slots, and the per-slot LRU eviction re-prefilled a cold ~10k prefix every turn
    // (measured: 96% prefill, two of four citizens at 0% cache reuse). Two branches, both pinned:
    //   (a) a budget that clears the floor at 4 slots → all 4 resident minds get a warm slot;
    //   (b) a budget that clears it at only 2 → 2 warm slots + the excess VISIBLE as overflow,
    //       never silently 4-crammed-onto-2 and never shrunk below the floor to fit more.
    #[test]
    fn slots_size_to_resident_population_capped_by_the_full_turn_window_floor() {
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);

        // (a) Roomy: 48GB clears the full-turn floor at 4 slots → a warm slot per resident mind,
        // zero overflow, no thrash. This is the win the `MAX_LANES = 2` clamp used to forbid.
        let roomy = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let warm = plan_serving(
            roomy,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, None),
        )
        .unwrap();
        assert_eq!(
            warm.lanes, 4,
            "roomy host: one warm slot per resident persona"
        );
        assert_eq!(
            warm.grid_overflow_lanes, 0,
            "all 4 minds hosted locally → nothing over-subscribed"
        );

        // (b) Floor-limited: 26GB clears the floor at only 2 slots. The plan yields 2 (never 4
        // crammed onto 2), surfaces the 2 unslotted minds as overflow (the non-silent
        // over-subscription signal a probe also names at the decision), and — critically — does
        // NOT drop the per-slot window below the floor to squeeze 4 in.
        let tight = HostBudget {
            usable_bytes: 26 * GB,
            perf_cores: 10,
        };
        let capped = plan_serving(
            tight,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, None),
        )
        .unwrap();
        assert_eq!(
            capped.lanes, 2,
            "window floor caps warm slots at 2 — the honest ceiling, not a silent cram"
        );
        assert_eq!(
            capped.grid_overflow_lanes, 2,
            "the 2 minds that couldn't get a warm slot are SURFACED (probe + grid_overflow), never absorbed"
        );
        assert!(
            capped.served_context_window >= BOOTSTRAP_WORKING_SET,
            "each served slot keeps a full-turn window ({}) — the floor is never breached to fit more, got {}",
            BOOTSTRAP_WORKING_SET,
            capped.served_context_window,
        );
    }

    // what this catches: the cap that outlived its own TODO. `BOOTSTRAP_WORKING_SET`
    // was written as a cold-start PRIOR to be superseded by measurement ("measured p95
    // later, #234"); the measurement never arrived, so on a host that could serve 94k
    // of a 131k-capable model every citizen got 16384/lanes = **8192 tokens**, and
    // measured 2026-08-06 that left a median context budget of 55 after framing and
    // conversation — the work board reached a prompt 0 times in 495. A measured demand
    // ABOVE the bootstrap prior must now actually raise the served window, or the
    // constant is still silently in charge and this whole seam is decoration.
    #[test]
    fn a_measured_demand_above_the_cold_start_prior_actually_raises_the_window() {
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);

        let cold = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(1, None),
        )
        .expect("servable");
        assert_eq!(
            cold.served_context_window, BOOTSTRAP_WORKING_SET,
            "with NO measurement the cold-start prior is the honest answer"
        );

        // One mind measured wanting 48k — well past the prior, well under the ceiling.
        let measured = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(1, Some(48_000)),
        )
        .expect("servable");
        assert!(
            measured.served_context_window > cold.served_context_window,
            "measured demand ({}) must RAISE the window above the cold-start prior ({}), \
             got {} — if this is equal, the constant is still the authority",
            48_000,
            cold.served_context_window,
            measured.served_context_window
        );
        assert!(
            measured.served_context_window <= 48_000,
            "…but never above what was actually demanded (got {})",
            measured.served_context_window
        );
    }

    // what this catches: the other direction — demand is a CAP, not a request the host
    // must honor. A mind that wants more than the machine has must receive what fits,
    // never a window the host cannot back with real KV, which is the swap/wedge the
    // demand cap exists to prevent in the first place.
    #[test]
    fn demand_beyond_the_host_is_bounded_by_what_actually_fits() {
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let greedy = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, Some(10_000_000)),
        )
        .expect("servable");
        assert!(
            greedy.served_context_window <= devstral.context_window,
            "never above the model's trained ceiling (got {})",
            greedy.served_context_window
        );
        let unbounded_fit = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, Some(u32::MAX)),
        )
        .expect("servable");
        assert_eq!(
            greedy.served_context_window, unbounded_fit.served_context_window,
            "past the host's real fit, MORE demand changes nothing — the fit is the bound"
        );
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
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        assert!(
            devstral.context_window > BOOTSTRAP_WORKING_SET,
            "precondition: model ceiling must exceed the demand cap, else the ceiling (not the cap) could explain the result"
        );
        let plan = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, None),
        )
        .unwrap();
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
        let plan_tiny = plan_serving(
            host,
            std::slice::from_ref(&tiny),
            ServingDemand::new(1, None),
        )
        .unwrap();
        assert!(
            plan_tiny.served_context_window <= 4_096,
            "demand cap must not inflate a 4k-ceiling model past its ceiling; got {}",
            plan_tiny.served_context_window
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
        let host = HostBudget {
            usable_bytes: 18 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let plan = plan_serving(
            host,
            std::slice::from_ref(&devstral),
            ServingDemand::new(4, None),
        )
        .unwrap();
        assert!(plan.fits_on_gpu, "{}", plan.rationale);
        assert!(
            plan.lanes < 4,
            "tight host must serve fewer than the 4 demanded: got {}",
            plan.lanes
        );
        assert!(plan.lanes >= 1);
        let lanes = plan.lanes as u64;
        let c = plan.served_context_window as u64;
        let compute_floor = devstral.compute_buffer_per_lane();
        let compute_rate = devstral.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
        let footprint = devstral.weights_bytes
            + devstral.kv_at(c as u32) * lanes
            + (compute_floor + compute_rate * c) * lanes;
        let effective = (host.usable_bytes as f64 * (1.0 - CO_CONSUMER_HEADROOM)) as u64;
        assert!(
            footprint <= effective,
            "degraded plan still overcommits: {footprint} > {effective}"
        );
    }

    // what this catches: an 8GB Air must NOT be handed the 14B (won't fit) and
    // must NOT be left with nothing — it picks the most capable model that
    // actually fits on the GPU budget. "Figure out something to run."
    #[test]
    fn tiny_box_picks_most_capable_that_fits_not_the_biggest() {
        // ~5.5GB usable after OS headroom on an 8GB Air.
        let host = HostBudget {
            usable_bytes: 5 * GB + 500 * 1_000_000,
            perf_cores: 4,
        };
        let plan = plan_serving(host, &candidates(), ServingDemand::new(MAX_LANES, None)).unwrap();
        assert!(
            plan.fits_on_gpu,
            "must fit a real model on GPU: {}",
            plan.rationale
        );
        assert_eq!(
            plan.base_model_id, "qwen3.5-4b",
            plan.base_model.model_id, "qwen3.5-4b",
            "14B can't fit 5.5GB; 4B is the most capable that does"
        );
        assert!(plan.lanes >= 1);
    }

    // what this catches: lanes come from DEMAND, never maximum concurrency.
    // 2 personas on a pressured budget must get 2 well-fed lanes, not 4
    // starving ones — the 2026-07-10 starvation served 2 minds through 4 slots
    // at 3633 tokens each and the room degenerated into a greeting loop. Same
    // budget, demand honored → each mind's window roughly doubles.
    #[test]
    fn lanes_track_demand_and_every_unneeded_lane_stops_costing_window() {
        // A pressured budget (benchmark servers breathing next door). Demand a budget-bound
        // window (Some(u32::MAX)) so the "unneeded lane costs window" invariant is visible:
        // under the default demand cap both counts would hit the same cap and the window
        // difference would be masked — the invariant lives in the budget-bound regime.
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        };
        let greedy = plan_serving(
            host,
            &candidates(),
            ServingDemand::new(MAX_LANES, Some(u32::MAX)),
        )
        .unwrap();
        let demand2 =
            plan_serving(host, &candidates(), ServingDemand::new(2, Some(u32::MAX))).unwrap();
        assert_eq!(
            demand2.lanes, 2,
            "2 minds → 2 lanes, never the MAX_LANES ceiling"
        );
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
        let demand99 = plan_serving(host, &candidates(), ServingDemand::new(99, None)).unwrap();
        assert!(demand99.lanes <= MAX_LANES);
        // …and a zero demand is defensively floored at one lane.
        assert_eq!(
            plan_serving(host, &candidates(), ServingDemand::new(0, None))
                .unwrap()
                .lanes,
            1
        );
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
        let squeezed = HostBudget {
            usable_bytes: 19 * GB,
            perf_cores: 6,
        };
        let plan = plan_serving(
            squeezed,
            std::slice::from_ref(&devstral),
            ServingDemand::new(2, None),
        )
        .unwrap();
        assert_eq!(
            plan.lanes, 1,
            "2 lanes would floor → shed to 1 real lane, not 2 @ 2048"
        );
        assert!(
            plan.served_context_window > 4096,
            "the surviving lane gets a window big enough to think in, got {}",
            plan.served_context_window,
        );
        // And when there IS room, demand is honored: concurrency preserved AND each
        // mind at a real, DEMAND-sized window — capped at the working-set bootstrap
        // (2026-07-26), NOT maximized to fill RAM (the 94k→swap/wedge bug). A roomy
        // box buys more LANES (concurrent minds), not a bloated per-lane KV cache.
        let roomy = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let plan2 = plan_serving(
            roomy,
            std::slice::from_ref(&devstral),
            ServingDemand::new(2, None),
        )
        .unwrap();
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
        let host = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let plan = plan_serving(host, &candidates(), ServingDemand::new(MAX_LANES, None)).unwrap();
        assert_eq!(
            plan.base_model_id, "coder-sentinel-14b",
            plan.base_model.model_id, "coder-sentinel-14b",
            "most capable, fits easily"
        );
        assert!(
            plan.lanes >= 2,
            "M5 Pro has the budget for multiple lanes, got {}",
            plan.lanes
        );
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
        let host = HostBudget {
            usable_bytes: 500 * GB,
            perf_cores: 64,
        };
        let plan = plan_serving(host, &candidates(), ServingDemand::new(MAX_LANES, None)).unwrap();
        assert_eq!(plan.lanes, MAX_LANES);
    }

    // what this catches: footprint-awareness — a model with a fatter per-token
    // KV cache yields fewer lanes on the same budget than a lean one (lanes are
    // sized at the MIN window; a fatter floor lane fits fewer times).
    #[test]
    fn fatter_kv_means_fewer_lanes() {
        // Budget chosen so KV (not the MAX_LANES backstop or perf cores) is the binding
        // constraint: each warm slot must clear the full-turn window floor (16384, #266), so a
        // fatter per-token KV rate makes fewer slots clear it. 16GB total, 2GB weights → ~11.6GB
        // (after co-consumer headroom) for (KV + compute buffer) across lanes at a full-turn
        // window: lean (150k/tok) clears the floor at 3 slots; fat (450k/tok, 3× the KV) clears
        // it at only 1. (A 4GB host would floor BOTH to 1 lane — too small to show the effect.)
        let host = HostBudget {
            usable_bytes: 16 * GB,
            perf_cores: 8,
        };
        let lean = plan_serving(
            host,
            &[fp("lean", 2, 150_000, 32_768, 5)],
            ServingDemand::new(MAX_LANES, None),
        )
        .unwrap();
        let fat = plan_serving(
            host,
            &[fp("fat", 2, 450_000, 32_768, 5)],
            ServingDemand::new(MAX_LANES, None),
        )
        .unwrap();
        assert!(
            lean.lanes > fat.lanes,
            "lean {} should beat fat {}",
            lean.lanes,
            fat.lanes
        );
    }

    // what this catches: the plan CAPS lane count at the binding constraint — the full-turn
    // window floor (#266), not the raised MAX_LANES backstop — AND reserves the concurrent
    // compute buffers, so resident KV + those buffers fit the budget (no OOM by construction).
    // The transient prefill compute buffer scales with n_ctx, not weights, so a
    // window-independent reserve under-provisions and 4 concurrent large-window prefills
    // overflow. Whatever the cap, the fit invariant below must hold.
    #[test]
    fn lane_count_respects_the_window_floor_and_reserves_compute_buffers() {
        // 24B-class: 13.6GB weights, kv_per_token ~156KB/token (measured), ~26GB usable.
        let m = fp("devstral-24b", 13, 156_000, 131_072, 9);
        let host = HostBudget {
            usable_bytes: 26 * GB,
            perf_cores: 10,
        };

        // 4 personas demand 4 lanes, but only 2 slots clear the full-turn window floor on this
        // budget — the window floor (below MAX_LANES=8) is the binding cap. The other 2 minds
        // surface as grid_overflow rather than thrashing 4 minds across 2 clobbering slots.
        let plan =
            plan_serving(host, std::slice::from_ref(&m), ServingDemand::new(4, None)).unwrap();
        assert_eq!(
            plan.lanes, 2,
            "the full-turn window floor (not the MAX_LANES backstop) caps warm slots here: {}",
            plan.rationale
        );
        assert_eq!(
            plan.grid_overflow_lanes, 2,
            "the 2 unslotted minds are surfaced for grid placement"
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
        let host = HostBudget {
            usable_bytes: 300 * 1_000_000,
            perf_cores: 2,
        }; // 0.3GB
        let plan = plan_serving(host, &candidates(), ServingDemand::new(MAX_LANES, None)).unwrap();
        assert!(
            !plan.fits_on_gpu,
            "must report the GPU budget can't hold any candidate"
        );
        assert_eq!(
            plan.base_model_id, "qwen2.5-0.5b",
            plan.base_model.model_id, "qwen2.5-0.5b",
            "names the smallest as the only option"
        );
        assert_eq!(plan.lanes, 1);
    }

    // what this catches: no candidates → no plan (caller must supply a registry).
    #[test]
    fn no_candidates_is_none() {
        let host = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        assert!(plan_serving(host, &[], ServingDemand::new(MAX_LANES, None)).is_none());
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
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        };
        assert_eq!(
            plan_serving_stable(host, &pair(), None, ServingDemand::new(MAX_LANES, None)),
            plan_serving(host, &pair(), ServingDemand::new(MAX_LANES, None))
        );
    }

    // what this catches: THE thrash guard — fresh prefers `big` (more capable,
    // fits) but it only fits without headroom, so we KEEP the incumbent `small`
    // rather than flap the served model on a transient budget bump.
    #[test]
    fn stable_keeps_incumbent_when_upgrade_lacks_headroom() {
        // 10GB: big (9.7GB) fits a lane but exceeds the 0.9*10=9GB headroom bar.
        let host = HostBudget {
            usable_bytes: 10 * GB,
            perf_cores: 6,
        };
        assert_eq!(
            plan_serving(host, &pair(), ServingDemand::new(MAX_LANES, None))
                .unwrap()
                .base_model_id,
            "big",
            "fresh would pick big"
        );
        let stable = plan_serving_stable(
            host,
            &pair(),
        // LOCAL fixture, not `pair()`. This test needs `big` to be a LEGITIMATE upgrade
        // target (clears the full-turn window floor, so selection would really pick it)
        // that nonetheless fails the switch-UP headroom bar. With the shared `pair()` at
        // 10GB those two are unsatisfiable together: big (9GB + 90k/tok) needs 10.47GB to
        // clear one full turn, so at any budget where it lacks 0.9x headroom it also
        // isn't full-turn viable — and a model that can only be served blind is not an
        // upgrade worth flapping for. Before the model-choice floor landed, the old
        // MIN_SERVE_CTX bar admitted big here on 184MB of KV and this fixture read as
        // "fresh would pick big" when what fresh actually had was a 2048-token 9GB model.
        // PREMISE CHANGE (2026-08-19), stated as such: this fixture was sized against the
        // UN-CREDITED budget, because the switch-up bar used to be `0.9 * host`. The bar now
        // sits on the POST-EVICTION budget (`host` + the incumbent's own weights), since
        // `serve()` kills the incumbent before launching the candidate and the two are never
        // co-resident. Crediting `small`'s 1GB moved the bar 18.0 → 18.9, and the old `big`
        // (18GB + 0.18 KV = 18.18) slid under it — so the test began asserting the OPPOSITE of
        // its own name. The INVARIANT is untouched: a model that lacks headroom must not be
        // adopted. Only the fixture is restated for the corrected budget.
        //
        // Re-sized so big still clears a full turn at the at-rest budget (19 + 1.47 = 20.47 <=
        // 21) — a legitimate upgrade target selection would really pick — yet still exceeds the
        // headroom bar (19 + 0.18 = 19.18 > 0.9 * 21 = 18.9).
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        };
        let models = vec![
            fp("small", 1, 4_000, 32_768, 1),
            fp("big", 19, 90_000, 262_144, 3),
        ];
        // Setup assertion: big IS what selection would choose, so the refusal below is a real
        // headroom refusal and not "big was never a candidate". This asks the question at the
        // budget `fresh` is now computed against — the at-rest one — because that is where the
        // stabilizer does its selecting. Asking it at `host` would make the setup vacuous: big
        // (19 + 1.47 = 20.47) does not clear a full turn in 20GB at all, so the test would pass
        // for the wrong reason, asserting a refusal of something never on offer.
        let at_rest_for_setup = HostBudget {
            usable_bytes: host.usable_bytes + 1 * GB, // small's credited weights
            perf_cores: host.perf_cores,
        };
        assert_eq!(
            plan_serving(at_rest_for_setup, &models, ServingDemand::new(MAX_LANES, None))
                .unwrap()
                .base_model
                .model_id,
            "big",
            "fresh would pick big at the post-eviction budget — so the refusal below is real"
        );
        let stable = plan_serving_stable(
            host,
            &models,
            Some("small"),
            ServingDemand::new(MAX_LANES, None),
        )
        .unwrap();
        assert_eq!(
            stable.base_model_id, "small",
            stable.base_model.model_id, "small",
            "hysteresis keeps incumbent — no flap"
        );
        assert!(
            stable.lanes >= 1,
            "lanes still re-tracked for the kept model"
        );
    }

    // what this catches: THE LANE FLAP — a serving model self-evicting its own lane.
    // `plan_serving_stable` used to early-return the FRESH plan whenever fresh chose the
    // incumbent, and fresh is computed against the LIVE budget, which the incumbent's own
    // weights + KV depress while it serves. So the planner kept re-deciding it could not
    // afford the lane it was already running, dropped to 1, then re-added it when the KV
    // freed. Measured on an idle host: 718 replans in ONE solve, lanes oscillating 1↔2, each
    // flip resizing the live admission semaphore and prefill throttle under in-flight
    // requests — the `no response headers for 300s` wedge that killed three benchmark runs.
    //
    // Same budget, same model, only difference is whether the incumbent is declared: the
    // stable plan must NOT serve fewer lanes than the plan that boot would have made at rest.
    #[test]
    fn a_serving_model_never_sheds_its_own_lane_to_its_own_residency() {
        // 9GB model on an 18GB host — the measured flap zone. At rest it plans (2 lanes,
        // 16384). Once it is SERVING, its own 9GB reads as "used", and a fresh plan against
        // that depressed budget returns (1 lane, 2048): its own residency costs it a lane AND
        // 87% of its window, so the next tick (KV freed) plans it straight back up. That is
        // the oscillation, and every flip resizes the live admission semaphore + prefill
        // throttle under in-flight requests.
        let models = vec![fp("big", 9, 90_000, 262_144, 3)];
        let at_rest = HostBudget {
            usable_bytes: 18 * GB,
            perf_cores: 6,
        };
        let boot = plan_serving(at_rest, &models, ServingDemand::new(MAX_LANES, None))
            .expect("servable at rest");

        let live = HostBudget {
            usable_bytes: at_rest.usable_bytes - models[0].weights_bytes,
            perf_cores: 6,
        };
        let fresh = plan_serving(live, &models, ServingDemand::new(MAX_LANES, None))
            .expect("still servable");
        // Guard the guard: if this ever stops being a flap, the test below proves nothing.
        assert!(
            fresh.lanes < boot.lanes,
            "fixture no longer reproduces the flap (boot={} fresh={}) — re-derive the budget",
            boot.lanes,
            fresh.lanes
        );

        let stable = plan_serving_stable(
            live,
            &models,
            Some("big"),
            ServingDemand::new(MAX_LANES, None),
        )
        .expect("incumbent still servable");
        assert_eq!(stable.base_model_id, "big");
        assert_eq!(stable.base_model.model_id, "big");
        assert_eq!(
            stable.lanes, boot.lanes,
            "a model's OWN residency must not shrink its OWN lane count (stable={} boot={} \
             fresh={})",
            stable.lanes, boot.lanes, fresh.lanes
        );
        assert_eq!(
            stable.served_context_window, boot.served_context_window,
            "nor its own window (stable={} boot={} fresh={})",
            stable.served_context_window, boot.served_context_window, fresh.served_context_window
        );
    }

    // what this catches: a genuine upgrade DOES happen when the better model
    // fits with headroom — hysteresis isn't a permanent lock-in.
    #[test]
    fn stable_upgrades_when_better_model_fits_with_headroom() {
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        }; // big 9.7 << 0.9*20=18
        let stable = plan_serving_stable(
            host,
            &pair(),
            Some("small"),
            ServingDemand::new(MAX_LANES, None),
        )
        .unwrap();
        assert_eq!(
            stable.base_model_id, "big",
            stable.base_model.model_id, "big",
            "more capable + ample headroom → upgrade"
        );
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
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        };
        let only_small = vec![fp("small", 1, 4_000, 32_768, 1)]; // "big" no longer on disk
        let stable = plan_serving_stable(
            host,
            &only_small,
            Some("big"),
            ServingDemand::new(MAX_LANES, None),
        )
        .unwrap();
        assert_eq!(
            stable.base_model_id, "small",
            stable.base_model.model_id, "small",
            "incumbent gone from disk → serve what's present"
        );
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
        let dipped = HostBudget {
            usable_bytes: 8 * GB,
            perf_cores: 6,
        };
        // Plain plan at the depressed budget WOULD flap: big (9GB) no longer "fits"
        // 8GB, so fresh prefers the smaller model.
        assert_eq!(
            plan_serving(dipped, &pair(), ServingDemand::new(MAX_LANES, None))
                .unwrap()
                .base_model_id,
                .base_model
                .model_id,
            "small",
            "depressed-budget plain plan would flap to the smaller model"
        );
        // With the incumbent credited its own weights back, the resident big stays.
        let stable = plan_serving_stable(
            dipped,
            &pair(),
            Some("big"),
            ServingDemand::new(MAX_LANES, None),
        )
        .unwrap();
        assert_eq!(
            stable.base_model_id, "big",
            stable.base_model.model_id, "big",
            "incumbent survives its OWN load dip — no flap"
        );
        assert!(stable.lanes >= 1, "kept model still gets ≥1 lane");
    }

    // what this catches: the autonomic planner being structurally unable to reach a BETTER
    // model that fits — because it measured the candidate against a budget the incumbent was
    // still occupying, while `serve()` evicts before it launches.
    //
    // These are THIS MacBook's real numbers on 2026-08-19, not invented ones. The live
    // `serving.plan` chose Devstral (24B, ~14 GB) at `usable_gb: 18` on a 64 GB box, so
    // Qwen3.8-27B (18.97 GB) "did not fit" and never entered the plan. `serving/pin`, same
    // instant, credited the incumbent back and reported `budget_gb: 34.09` — the 27B fit with
    // 15 GB spare and served at 17.2 tok/s. Two answers to one question; only the operator
    // path could reach the better model.
    #[test]
    fn a_more_capable_model_that_fits_after_eviction_is_adopted_not_starved() {
        // 18 GB free WITH the ~14 GB incumbent resident — the measured live condition.
        let live = HostBudget {
            usable_bytes: 18 * GB,
            perf_cores: 10,
        };
        let models = vec![
            fp("devstral-24b", 14, 100_000, 131_072, 5),
            fp("qwen3.8-27b", 19, 100_000, 262_144, 9), // strictly more capable
        ];

        // The un-credited question — "fits BESIDE the incumbent?" — answers the wrong thing.
        assert_eq!(
            plan_serving(live, &models, ServingDemand::new(2, None))
                .unwrap()
                .base_model
                .model_id,
            "devstral-24b",
            "against the live budget the 27B cannot fit — this is the state that stranded it"
        );

        // The stabilizer credits the eviction and takes the upgrade.
        let plan = plan_serving_stable(
            live,
            &models,
            Some("devstral-24b"),
            ServingDemand::new(2, None),
        )
        .expect("a plan exists");
        assert_eq!(
            plan.base_model.model_id, "qwen3.8-27b",
            "a strictly more capable model that fits POST-EVICTION must be adopted — the swap \
             kills the incumbent before launching, so the co-resident test was never the right \
             question"
        );
    }

    // what this catches: the credit-back turning into a thrash engine. Crediting eviction must
    // NOT hand the upgrade gate a blank cheque — a marginally-fitting or no-better model still
    // has to lose, or the planner swaps models on budget noise and every flip bounces the live
    // lane under in-flight requests (the wedge that killed three benchmark runs).
    #[test]
    fn the_eviction_credit_still_refuses_a_marginal_or_no_better_swap() {
        let live = HostBudget {
            usable_bytes: 18 * GB,
            perf_cores: 10,
        };

        // (a) Equal capability → never swap, however well it fits.
        let equal = vec![
            fp("devstral-24b", 14, 100_000, 131_072, 5),
            fp("sibling-24b", 14, 100_000, 131_072, 5),
        ];
        assert_eq!(
            plan_serving_stable(live, &equal, Some("devstral-24b"), ServingDemand::new(2, None))
                .unwrap()
                .base_model
                .model_id,
            "devstral-24b",
            "equal capability is not an upgrade — the incumbent holds"
        );

        // (b) More capable but it consumes the ENTIRE post-eviction budget, leaving nothing
        // for KV — SWITCH_UP_HEADROOM must reject it rather than swap into a lane that
        // cannot hold a window.
        let marginal = vec![
            fp("devstral-24b", 14, 100_000, 131_072, 5),
            fp("hog-70b", 32, 100_000, 262_144, 9),
        ];
        assert_eq!(
            plan_serving_stable(
                live,
                &marginal,
                Some("devstral-24b"),
                ServingDemand::new(2, None)
            )
            .unwrap()
            .base_model
            .model_id,
            "devstral-24b",
            "a better model with no headroom left is refused — the credit is not a blank cheque"
        );
    }
}
