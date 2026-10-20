use super::*;

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

/// PURE: a plan's per-slot window is a persona seat only if it holds what the residents
/// REQUIRE. The requirement is not a number in this file: today it is the residents'
/// typical prompt with headroom (`ServingDemand::typical_prompt_floor`, the same measured
/// demand the plan targets, sealed against a starved window by #4256); when a role
/// declares its requirement (card 2eec3977, the allocator) it is that declaration. Joel,
/// 2026-09-20, reading the 5090 at 2 × 2,048 refusing 2,932 prompts (p50 56k): "2k context
/// is a complete waste of a lane" — and, on a fixed floor: "you're making rigid laws that
/// concern me". So the seam holds a comparison, never a constant: `None` requirement (no
/// prompt ever measured or declared) imposes nothing beyond the engine's runnable floor.
/// The per-lane floor a typical sent prompt implies: the prompt with headroom, never
/// under the serve floor. PURE; the one arithmetic behind `typical_prompt_floor` and the
/// placement rule's requirement.
pub fn prompt_floor_of(sent_median: u32) -> u32 {
    ((sent_median as f64 * SENT_HEADROOM) as u32).max(MIN_SERVE_CTX)
}

pub fn persona_lane_holds(served_context_window: u32, requirement: Option<u32>) -> bool {
    match requirement {
        Some(r) => served_context_window >= r,
        None => served_context_window >= MIN_SERVE_CTX,
    }
}

/// What the minds on this host are asking the serving lane for.
///
/// Both axes of demand in ONE value, because they are one question — "how much
/// serving does the work on this box actually need" — and passing them as two loose
/// `u32`s next to each other is how a caller silently swaps them.
/// Passed by REFERENCE everywhere (`&ServingDemand`): built once per tick by the
/// daemon and handed to the plan, never copied per call (Joel, 2026-09-20).
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The per-slot window this host served this model at LAST time (the
    /// served-window store). When it still fits, the plan serves it VERBATIM so the
    /// KV page geometry — `<model>--c<window>` — is the same across boots and the
    /// citizens' pages carry over. `None` = no memory (first serve of this model).
    pub sticky_window: Option<u32>,
    /// Concurrent lanes this seat serves for minds hosted on OTHER nodes — the `ai/generate`
    /// a peer sends here when its placement spills a mind onto this seat — measured at the
    /// seat's own admission as the peak per plan tick
    /// ([`crate::cognition::resource_admission::take_leased_in_peak`]). Added to `lanes` when
    /// the plan sizes the slot count: a seat's demand is the minds it SERVES, not only the
    /// minds it HOSTS. Measured 2026-09-18 (card c84d885a): the 5090's 27B seat served the
    /// M5's four coders and IntelMac's eight on ONE slot — lane wait p50 216 s, p90 1,387 s,
    /// max 7,058 s — while its plan counted only its own roster and nothing counted these.
    /// `0` is the honest value on a seat nobody leases into, and on every seat until the
    /// inbound-generate seam feeds the gauge (the wire slice of the same card).
    pub leased_in: u32,
    /// The measured knee of the model this seat serves — the tighter of the decode knee, the
    /// slot count past which every stream slows (`inference::decode_knee`), and the prefill
    /// knee, the slots its shared prefill rate can serve inside the turn budget
    /// (`inference::prefill_knee`, card d4d2ef4a) — when one is known. It bounds the
    /// slot count the plan SERVES: residents, scratch AND leased-in together. Measured
    /// 2026-09-19 23:14Z on the M5: the roster was clamped to the knee (17 → 2) before
    /// `leased_in` (1) was added, so the plan said 3 lanes against a knee of 2 and the
    /// [health] hour read "AT THE KNEE: 16 minds on 3 lanes, the measured decode knee is 2".
    pub knee: Option<u32>,
    /// The largest prompt any resident actually SENT (post-fit), when measured. The
    /// served window follows THIS with [`SENT_HEADROOM`], never the untrimmed
    /// `window_tokens` (which is the whole assembled context and saturates at
    /// 225k–505k): a 25k working set provisions ~32k per slot, leaving the host RAM
    /// for more lanes and the prompt-cache tier instead of a 137k slot nobody fills.
    pub sent_tokens: Option<u32>,
    /// The residents' TYPICAL sent prompt (median of sent peaks). The per-lane floor
    /// follows this so lanes multiply to the roster; `sent_tokens` (the ceiling)
    /// still shapes the window target when it fits.
    pub sent_median: Option<u32>,
    /// Per-mind lane requirements (see `with_requirements`). Empty = the legacy
    /// median/bootstrap derivation below, kept for callers that have not migrated
    /// (card 2eec3977: to be deleted with them).
    pub requirements: Vec<crate::cognition::window_allocator::LaneRequirement>,
}

/// Headroom over the largest sent prompt: a mind held at a too-small window fills it,
/// so a window sized from what-was-sent must GROW past it to escape the clamp — 25%
/// per plan, geometric, until the demand is met. The untrimmed demand still caps it.
pub const SENT_HEADROOM: f64 = 1.25;

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
            sticky_window: None,
            sent_tokens: None,
            sent_median: None,
            requirements: Vec::new(),
            leased_in: 0,
            knee: None,
        }
    }
    /// Fold in the minds this seat serves for other nodes (see `leased_in`).
    pub fn with_leased_in(mut self, leased_in: u32) -> Self {
        self.leased_in = leased_in;
        self
    }
    /// The model's measured decode knee on this box (see `knee`); `None` = unmeasured.
    pub fn with_knee(mut self, knee: Option<u32>) -> Self {
        self.knee = knee;
        self
    }
    pub fn with_sent_median(mut self, median: Option<u32>) -> Self {
        self.sent_median = median.filter(|m| *m > 0);
        self
    }
    /// What each mind on this seat REQUIRES of a lane (`window_allocator`). When
    /// present, lanes × window come from the allocator over these — the governed
    /// size — and the sent-median / bootstrap path below is not consulted.
    pub fn with_requirements(mut self, requirements: Vec<crate::cognition::window_allocator::LaneRequirement>) -> Self {
        self.requirements = requirements;
        self
    }
    /// The per-lane floor the residents actually need: their typical sent prompt with
    /// headroom. `None` until a turn has been sent — the bootstrap prior stands then.
    pub fn typical_prompt_floor(&self) -> Option<u32> {
        self.sent_median.map(prompt_floor_of)
    }

    /// Post-plan validation only needs the legacy floor when no per-mind
    /// requirements were supplied. Otherwise the window allocator already chose
    /// a feasible subset; a fleet-wide historical floor must not veto that choice.
    pub fn publication_prompt_floor(&self, plan: &ServingPlan) -> Option<u32> {
        self.typical_prompt_floor().filter(|_| {
            self.requirements.is_empty() || !plan.fits_on_gpu || plan.lanes == 0
        })
    }

    /// The largest SENT prompt among residents (see `sent_tokens`).
    pub fn with_sent_tokens(mut self, sent: Option<u32>) -> Self {
        self.sent_tokens = sent.filter(|s| *s > 0);
        self
    }

    /// The window a plan should TARGET per slot: the roster's TYPICAL sent prompt with
    /// headroom when one is known (measured this boot, or remembered from the last
    /// serve), else the largest sent prompt with headroom, else the untrimmed demand —
    /// always capped by the untrimmed demand, an upper bound by construction.
    ///
    /// The typical, not the outlier (2026-09-19 19:37Z, the M5, card 29e4ab34): the
    /// lanes already follow the typical prompt (the per-lane floor), but the window
    /// followed the PEAK — one 144,891-token prompt ever sent made the target 181,114
    /// on every boot, the remembered 67,584 could not "cover" it and was discarded, and
    /// under the boot's transient budget the plan launched 2 × 179k (44 GB of serving on
    /// a box whose steady budget is 27 GB), then Eco, then the collapse. A single outlier
    /// prompt is that turn's cost (it is trimmed to the window); it does not size the box.
    pub fn window_target(&self) -> u32 {
        let with_headroom = |t: u32| (t as f64 * SENT_HEADROOM) as u32;
        match self.sent_median.map(with_headroom).or(self.sent_tokens.map(with_headroom)) {
            Some(t) => t.min(self.window_tokens).max(MIN_SERVE_CTX),
            None => self.window_tokens,
        }
    }

    /// Remember last serve's per-slot window for this model (see `sticky_window`).
    pub fn with_sticky_window(mut self, w: Option<u32>) -> Self {
        self.sticky_window = w.filter(|w| *w >= MIN_SERVE_CTX);
        self
    }

}

/// The served per-slot window: the window this host served LAST time when it
/// still fits AND still covers the measured demand — the KV page geometry then
/// holds across boots — else the demand itself (the law above: provision for what
/// the minds use, never for what RAM allows), clamped to what fits and the
/// runnable floor. A demand that outgrows the remembered window re-keys the pages
/// once and is remembered in turn. Pure.
pub fn choose_served_window(fits: u32, demand_tokens: u32, sticky: Option<u32>) -> u32 {
    let wanted = match sticky {
        Some(s) if s <= fits && s >= demand_tokens => s,
        _ => demand_tokens,
    };
    wanted.min(fits).max(MIN_SERVE_CTX)
}

/// Hysteresis margin for switching UP to a more capable model: it must fit
/// within `(1 - SWITCH_UP_HEADROOM)` of the budget — i.e. with headroom to
/// spare — before we abandon the incumbent for it. Stops transient budget
/// bumps near a model's edge from flapping the served model (the live-budget
/// thrash: free memory jitters, the "best fit" flips, the model reloads).
pub const SWITCH_UP_HEADROOM: f64 = 0.10;

/// The prompt cache's grant headroom (#4216): a cache FILLS to its grant, so the grant
/// is the affordable figure less this fraction, never the whole of it.
///
/// This constant was born as the PLAN's co-consumer headroom (Joel 2026-07-16: the
/// spawn-time free-VRAM snapshot could not see a browser or a game spike, so the window
/// was sized to 85% of it). Two things have since made that reservation a second copy
/// of itself: the governor's board subtracts every other holder's LIVE residency before
/// serving is handed its budget (`budget_for_replacing`, 2026-08-19), and the power mode
/// withholds its own fraction of that budget (`PowerMode::serving_fraction`: Eco 0.55,
/// Comfort 0.80). Stacked, the plan kept 0.80 × 0.85 = 68% of a budget that already
/// excluded the co-consumers. Measured 2026-09-19 on the M5 (64 GB, Comfort): board
/// budget 37 GB after 18.7 GB of other holders → 25 GB for weights + lanes → ONE lane
/// under a 19 GB model, while 2 × 67k lanes physically fit in 27.4 GB with 26% of the
/// budget to spare. The plan now sizes to the budget the mode hands it — the mode IS
/// the headroom policy, one place — and only the cache grant keeps this fraction.
pub const CACHE_GRANT_HEADROOM: f64 = 0.15;

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
    /// Fixed residency per lane LEARNED from the live process beyond the header's KV rate
    /// and the compute floor: the draft model's KV, the vision projector, engine buffers.
    /// Measured 2026-09-19 on the M5: (55,925 − 32,768) B/token × 67,340 ≈ 1.56 GB per
    /// lane that a header-only plan would drop. Counted in [`Self::compute_buffer_per_lane`].
    /// Runtime-learned, never on the wire: the bindings and the row stay as they are.
    #[serde(skip)]
    #[ts(skip)]
    pub fixed_per_lane_bytes: u64,
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
    /// The largest per-slot window `lanes` lanes of this model fit inside
    /// `usable_bytes` (after co-consumer headroom, weights, and the per-lane compute
    /// floor + window-scaled reserve), capped at the model's trained window. The ONE
    /// arithmetic the plan sizes windows with — and the bound every "keep the bigger
    /// window" law on the launch path must respect (2026-09-17: a 144k window sized
    /// against a fictional 54 GB budget was kept by the sticky and wedge-heal laws
    /// across a relaunch onto a 26 GB one). 0 lanes = 0.
    pub fn window_within(&self, usable_bytes: u64, lanes: u32) -> u32 {
        let l = lanes as u64;
        if l == 0 {
            return 0;
        }
        if self.kv_per_token == 0 {
            return self.context_window;
        }
        // The budget as handed: the mode's fraction and the board's live subtraction of
        // every other holder are the headroom (see `CACHE_GRANT_HEADROOM` for the copy
        // this used to keep on top of them).
        let after_weights = usable_bytes.saturating_sub(self.weights_bytes);
        let compute_floor = self.compute_buffer_per_lane();
        let compute_rate = self.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
        let per_token_cost = self.kv_per_token.saturating_add(compute_rate).max(1);
        let after_compute = after_weights.saturating_sub(compute_floor.saturating_mul(l));
        (((after_compute / l) / per_token_cost).min(u32::MAX as u64) as u32)
            .min(self.context_window)
    }

    /// What a lane costs beyond its KV: the base compute buffer plus the fixed residency a
    /// measurement attributed to it ([`Self::fixed_per_lane_bytes`]). What the plan sizes with.
    pub fn compute_buffer_per_lane(&self) -> u64 {
        self.base_compute_buffer_per_lane().saturating_add(self.fixed_per_lane_bytes)
    }

    /// The compute buffer a lane is assumed to need before anything is measured: what a
    /// MEASUREMENT of the lane subtracts. Never [`Self::compute_buffer_per_lane`] there:
    /// the fixed term is derived FROM that measurement, so subtracting it too feeds each
    /// reading's excess into the next one's floor. With C the lane's true fixed residency,
    /// fixed(n+1) = C − fixed(n), alternating forever: the M5 on 2026-10-10 19:58–20:01Z
    /// read 1.93 GB and 4.40 GB of per-lane compute on alternate ticks and its plan flipped
    /// between 2 lanes and 1 every tick.
    pub fn base_compute_buffer_per_lane(&self) -> u64 {
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
    /// WHAT decided `lanes`, from the planner's own decision rather than inferred from the
    /// result afterwards (card eb8d1839). The daemon's `bound_by=host-fit` meant only
    /// "the window came out below demand", whatever budget did it, so a one-lane plan on the
    /// 5090 (2026-10-12, build 6160: `demand_lanes=2`, `fits_on_gpu=true`) could not say
    /// whether the GPU, the per-lane floor or a cold boot's memory chose one lane.
    pub lanes_bound: LanesBound,
}

/// What decided a plan's lane count. Each variant is a term the planner evaluated, so a
/// lane count can always be traced to the number that set it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/serving/LanesBound.ts")]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LanesBound {
    /// No candidate fit one lane on the budget: the whole demand is off-box.
    NoFit,
    /// The requirements allocator chose lanes and window together. It would consider at most
    /// `lane_cap` lanes, a cap `lane_cap_by` set before any packing. With `lanes < lane_cap` the
    /// budget packed fewer (the geometry costs `device_bytes` of `budget_bytes`); with
    /// `lanes == lane_cap` the cap decided, and `unserved` minds were never offered a lane,
    /// whatever the budget (Codex on #4971).
    #[serde(rename_all = "camelCase")]
    Allocator {
        served: u32,
        unserved: u32,
        window: u32,
        lane_cap: u32,
        lane_cap_by: LaneCapBy,
        #[ts(type = "number")]
        device_bytes: u64,
        #[ts(type = "number")]
        budget_bytes: u64,
    },
    /// Every demanded lane was granted (residents plus leased-in, bounded by the decode knee).
    Demand,
    /// The host's performance cores capped the lanes below demand.
    Cores,
    /// The `MAX_LANES` safety ceiling capped the lanes below demand.
    MaxLanes,
    /// One more lane would have left each lane a window below the per-lane floor, so the
    /// budget stopped there. The evaluated candidate is reported, not just the verdict:
    /// `rejected_lanes` lanes would each have had `rejected_window` tokens, against
    /// `floor_tokens`, which `floor_term` set.
    #[serde(rename_all = "camelCase")]
    PerLaneFloor {
        floor_tokens: u32,
        floor_term: FloorTerm,
        rejected_lanes: u32,
        rejected_window: u32,
    },
}

/// What capped the lanes the requirements allocator would consider, before packing: the
/// smallest of the four wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/serving/LaneCapBy.ts")]
#[serde(rename_all = "camelCase")]
pub enum LaneCapBy {
    /// One lane per mind with a requirement: no more minds to seat.
    Requirements,
    /// The host's performance cores.
    Cores,
    /// The `MAX_LANES` safety ceiling.
    MaxLanes,
    /// The demanded lane count (residents plus leased-in, knee-bounded).
    Demand,
}

/// Which term set the per-lane floor: the largest of the two wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/serving/FloorTerm.ts")]
#[serde(rename_all = "camelCase")]
pub enum FloorTerm {
    /// `BOOTSTRAP_WORKING_SET`, one full turn: no larger term applied.
    Bootstrap,
    /// The residents' typical sent prompt, with headroom.
    TypicalPrompt,
}
