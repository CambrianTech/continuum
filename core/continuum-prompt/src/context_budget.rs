//! ONE owner of every "how much may I re-inject into her prompt" bound.
//!
//! # Why this module exists
//!
//! Every re-injection seam in cognition needs a size bound: the tool-result fold, the
//! working-memory trail head, the full latest-action block, a dispatched command's result,
//! a rendered tool-output slice, the echo of her own arguments. Each one was independently
//! given a hand-written constant — `800`, `12_000`, `16_000`, `4_000`, `600` — each with a
//! careful doc comment explaining why THAT number, and none of them aware of the window
//! they were sitting in.
//!
//! That is the defect. A constant cannot know whether it is bounding a 4k lane or a 1M
//! lane. On a misfit grid the window IS the product: the whole serving stack (device_fit
//! governor, demand-derived lane `-c`, `served_context_window`, the reconcile-both-
//! directions fix) exists to compute the window from real memory on THIS machine, and a
//! literal written at one desk silently overrides all of it. Clamping a 1M-context MoE's
//! perception to a few thousand chars makes it incapable of holding a repo, which deletes
//! the reason to page experts at all. Tasks #45, #46 and #50 each closed one instance of
//! this class; it kept coming back as a fresh magic number.
//! [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
//!
//! # The calibration (why these fractions, not invented ones)
//!
//! The replaced constants were not arbitrary — they were tuned against the window the
//! author happened to be running, which was a 16384-token lane. At [`GUARD_CHARS_PER_TOKEN`]
//! (3 chars/token, the same conservative divisor the deliberation guard uses) that lane is
//! ~49k chars of prompt. Read against that total, every one of the old constants lands on a
//! clean fraction:
//!
//! | seam                | old constant | fraction of a 16k-token window |
//! |---------------------|--------------|--------------------------------|
//! | trail head          | 800          | ~1/64  (768)                   |
//! | latest action, full | 12_000       |  1/4   (12_288)                |
//! | tool-result fold    | 16_000       |  1/3   (16_384)                |
//! | dispatch result     | 4_000        |  1/12  (4_096)                 |
//! | rendered tool slice | 12_000       |  1/4   (12_288)                |
//! | tool catalog summary| 96           |  1/512 (96, exact)             |
//!
//! The echoed-args bound (600, invented on 2026-08-03, never tuned) is gone: a result's header
//! shows her call's single-line arguments whole and names its multi-line bodies by shape, and the
//! intent rides as its first line (act_observe::recency::render_call_args, intent_label). A shape
//! rule, never a size, so nothing here sizes it.
//!
//! So the fractions below are not a new guess: they are the tuning that was already there,
//! expressed against the thing it was always implicitly relative to. On a 16k lane every
//! bound is within a few percent of today's behavior (so small machines are unchanged), and
//! on a 128k or 1M lane they scale with it instead of strangling it.
//!
//! # The unknown-window contract
//!
//! [`ContextBudget::unknown`] folds NOTHING. When the live window is not yet known (cold
//! boot, mid-relaunch, no model binding) the correct bound is no bound — the deliberation
//! guard downstream still trims the assembled prompt to fit `n_ctx`, so an unclamped
//! re-injection is trimmed honestly at the one seam that knows the real number. An unknown
//! window must never become an invented one; that is precisely how a guess turns into a
//! clamp that outlives the guess.

use continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN;

/// Per-step size of the rolling trail: `1/64` of the window (the calibrated trail-head
/// fraction from the table above). Named so the COUNT below cannot drift from the SIZE.
const TRAIL_HEAD_DENOM: usize = 64;

/// The trail's TOTAL share of the window: `1/4` — the same allotment
/// [`ContextBudget::latest_action_chars`] gives the single most-recent act, so her history
/// gets parity with her present. Paired with [`TRAIL_HEAD_DENOM`] this yields the step
/// COUNT; changing either alone silently changes the other's meaning.
const TRAIL_TOTAL_DENOM: usize = 4;

/// The re-injection bounds for ONE mind at its CURRENT served window.
///
/// Construct from the live window at the seam that has it — `WorkspaceCycle::model_loadout()`
/// in cognition, or the served-window pin the supervisor reconciles — never from a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudget {
    /// Total prompt chars this window can hold, conservatively estimated. `None` when the
    /// live window is unknown, which means every bound below is "no bound".
    total_chars: Option<usize>,
    /// The served window itself in chars, unbounded by prefill affordability: the floor the
    /// one-result share stands on (see [`Self::latest_action_chars`]).
    served_chars: Option<usize>,
}

impl ContextBudget {
    /// Bounds derived from a live served context window, in tokens — AND from what the
    /// served lane can prefill inside the turn's time-to-first-token budget.
    ///
    /// The served window is KV capacity; it is not what a turn may cost. Measured
    /// 2026-09-16 on the Intel Mac: a 4B prefilled at 20.8 t/s and every render carried
    /// ~18k tokens (9.3k of tool registry, 9k of conversation) — fifteen minutes before the
    /// first token, on a box whose served window said 32k was fine. So the total here is
    /// `min(served window, TTFT_BUDGET × measured prefill t/s)`, floored at the serving
    /// stack's own minimum ([`continuum_engine::inference::prefill_rate::affordable_prompt_tokens`]),
    /// and every fraction below scales with it — the tool menu, recall, the digest. One
    /// owner, one rule: the frontier keeps its window (500 t/s buys it whole), the small
    /// tier gets the prompt it can afford. No trusted rate for the served model = the
    /// served window rules, as before.
    pub fn from_window(context_window: u32) -> Self {
        Self::from_window_for(context_window, crate::audience::current())
    }

    /// [`Self::from_window`] for a named audience instead of the running turn's. For STATE
    /// that outlives a turn (a mind's results ring): budgeted against the turn that happens
    /// to be running, it shrank 3x on every Interactive turn and regrew on the next
    /// Unattended one, rewriting its collapsed-pointer block each swing and re-prefilling
    /// everything after it (M5 2026-10-17: affordable 17-19k vs ~6k tokens alternating; the
    /// pointer block was the first divergence on 40 of 62 turns, ~9k fresh tokens each).
    pub fn from_window_for(context_window: u32, audience: continuum_engine::inference::prefill_rate::Audience) -> Self {
        let affordable = Self::prefill_affordable_window_for(context_window, audience);
        Self {
            total_chars: (affordable > 0)
                .then(|| (affordable as usize).saturating_mul(GUARD_CHARS_PER_TOKEN)),
            served_chars: (context_window > 0)
                .then(|| (context_window as usize).saturating_mul(GUARD_CHARS_PER_TOKEN)),
        }
    }

    /// The window her prompt is fitted to: the knee the dyno measured in her own traffic, never
    /// above the served slot. Stable by construction: it does not move with load, so it never
    /// turns an epoch over mid-flight.
    ///
    /// Why a token count from the dyno and not a rate: the knee is where a smaller window's
    /// turnovers start to cost more than the smaller context saves, and it is counted in
    /// tokens of her own requests, so it does not depend on how fast the engine is. A rate
    /// does, and moves with load. Deriving the window from the live prefill-rate EMA
    /// (f45f090a6, M5 2026-10-28) moved it 48k -> 40k -> 32k as contention lowered the rate,
    /// refused Aris twice at 32k and turned epochs over 74 times in 37 minutes. Rolled back.
    ///
    /// Measured 2026-10-28 (continuum-context replay --append-tail over all four residents'
    /// captures, `~/.continuum/evidence/2026-10-28-working-window/`): at 48k, prefill per
    /// request fell ~3x (Benchy 110 -> 41 s, Aiko 81 -> 24, Aris 113 -> 33, Atlas 94 -> 20)
    /// with zero handles dropped; 32k doubled turnovers. Re-measure by rerunning the replay;
    /// the act inspector tracking the knee continuously is the next step.
    pub fn working_window(served: u32) -> u32 {
        Self::working_window_when(served, continuum_engine::inference::llama_server::current_serving().active_model.is_some())
    }

    /// The served window only — the KV-capacity reading, for seams that must not be
    /// prefill-bounded (a test fixture, a capacity report).
    pub fn from_window_unbounded(context_window: u32) -> Self {
        let chars = (context_window > 0).then(|| (context_window as usize).saturating_mul(GUARD_CHARS_PER_TOKEN));
        Self { total_chars: chars, served_chars: chars }
    }

    /// Bounds from the window the local lane is serving RIGHT NOW — for seams that have no
    /// `WorkspaceCycle` to read a model binding off (a command handler, a bus listener).
    /// Not-ready or not-yet-served resolves to [`Self::unknown`], never to a stand-in number.
    pub fn live() -> Self {
        let serving = continuum_engine::inference::llama_server::current_serving();
        if serving.ready && serving.served_context_window > 0 {
            Self::from_window(serving.served_context_window)
        } else {
            Self::unknown()
        }
    }

    /// Like [`Self::live`], but an unknown window resolves to the substrate's own declared
    /// minimum ([`MIN_SERVE_CTX`](continuum_engine::inference::planning::MIN_SERVE_CTX)) instead of
    /// "no bound".
    ///
    /// Use this ONLY where an unbounded value would break assembly rather than be trimmed
    /// later — the tool catalog is the case: it is built BEFORE the deliberation guard runs,
    /// and its size decides whether the menu fits at all, so "no bound" there produces a
    /// prompt that cannot be assembled (caught by
    /// `tool_surface_is_a_category_index_plus_discovery_pair`). Everywhere the guard trims
    /// downstream, prefer [`Self::live`] and its honest no-bound.
    ///
    /// This is still not an invented number: `MIN_SERVE_CTX` is the one floor the serving
    /// stack already owns. And it only bites when nothing is being served — in which case
    /// there is no model to prompt yet anyway.
    pub fn live_or_floor() -> Self {
        let live = Self::live();
        if live.total_chars.is_some() {
            live
        } else {
            Self::from_window(continuum_engine::inference::planning::MIN_SERVE_CTX)
        }
    }

    /// The honest "I don't know the window" budget: nothing is folded. See the module doc's
    /// unknown-window contract — the deliberation guard is the real backstop.
    pub fn unknown() -> Self {
        Self { total_chars: None, served_chars: None }
    }

/// PURE: the prompt a turn may cost at `rate`, for a lane serving `served` tokens.
///
/// Split out from [`Self::prefill_affordable_window_for`] so the DECISION is pinned by a test
/// rather than by a probe nobody reads until it fires — the caller is all process globals
/// (the serving snapshot, the audience seam, the rates map) and cannot be exercised
/// without them.
///
/// Every rung that carries a number goes through the same
/// [`continuum_engine::inference::prefill_rate::affordable_prompt_tokens`] as before; only the
/// rung-less case is new, and it lands on the bootstrap working set, not the window.
fn affordable_for_rate(
    rate: continuum_engine::inference::prefill_rate::MeasuredRate,
    served: u32,
    audience: continuum_engine::inference::prefill_rate::Audience,
    cache_fraction: Option<f64>,
) -> u32 {
    let affordable = match rate.tps.filter(|t| t.is_finite() && *t > 0.0) {
        Some(tps) => continuum_engine::inference::prefill_rate::affordable_prompt_tokens(
            tps,
            continuum_engine::inference::planning::MIN_SERVE_CTX,
            served,
            audience,
        ),
        // No rung at all — nothing measured on this box, ever. The substrate's own
        // bootstrap working set is what it already assumes an unmeasured mind needs
        // (`UNMEASURED_FILL_FLOOR_TOKENS`), never the window, never a second constant.
        None => u32::try_from(continuum_engine::inference::prefill_rate::UNMEASURED_FILL_FLOOR_TOKENS)
            .unwrap_or(served) // unwrap_or: a floor that cannot be a u32 is not a floor; the window is the only other truth
            .min(served),
    };
    // THE TIME BUDGET IS SPENT ON FRESH TOKENS. The rate above prices every token of the
    // prompt as if the lane had to read it; a warm lane serves most of it from its KV
    // cache. Measured on the M5 2026-10-17: prefix reuse 55-75%, a ~18k-token bound on a
    // 34,560-token lane, so a resident's results ring (a quarter of the bound) could not
    // hold one 320-line read and she re-read the same file every turn. The measured cached
    // fraction scales the bound up, counted at most MAX_CACHE_CREDIT so a turn that does
    // lose its cache stays within a few multiples of its budget.
    let credited = cache_fraction.filter(|f| f.is_finite() && *f > 0.0).map(|f| f.min(Self::MAX_CACHE_CREDIT)).unwrap_or(0.0);
    let affordable = ((affordable as f64) / (1.0 - credited)).min(u32::MAX as f64) as u32;
    // The planning helper may raise a proposed lane to MIN_SERVE_CTX. This
    // owner budgets an already-resolved window: a serving floor cannot enlarge
    // its capacity, including small explicit evaluation windows.
    affordable.min(served)
}

/// The most of the measured cache fraction the bound credits: at 0.75 the bound grows at
/// most 4x, so a turn that misses its cache entirely costs at most four times its
/// time-to-first-token budget. Above it the bound would buy a whole-window cold prefill.
// derived-or-floor: floor — measured prefix reuse on the M5 2026-10-17 ran 0.55-0.75; the
// cap sits at the top of that range so the credit never exceeds what was observed.
const MAX_CACHE_CREDIT: f64 = 0.75;

/// The served window clamped to what the served lane can prefill inside the turn's
/// time-to-first-token budget.
///
/// The rate comes off the ladder ([`continuum_engine::inference::prefill_rate::measured_rate_for`]):
/// fresh, else stale, else the slowest curve this box holds. With no rung at all the
/// bootstrap working set governs — never the served window, which is the one answer that
/// cannot be right when nothing is known (card a2578811). `served` is still returned when
/// there is no served MODEL to have a rate for: an absent subject, not an absent
/// measurement. One row when the clamp binds, lifts, or changes rung; never per call.
fn prefill_affordable_window_for(served: u32, audience: continuum_engine::inference::prefill_rate::Audience) -> u32 {
    if served == 0 {
        return 0;
    }
    let Some(model) = continuum_engine::inference::llama_server::current_serving().active_model else {
        return served;
    };
    // AN UNKNOWN RATE IS NOT PERMISSION FOR THE WHOLE WINDOW (card a2578811). This read
    // was `rate_for` — fresh-only — and its `None` fell through to `served`: the LARGEST
    // prompt the box can be asked for, handed out in exactly the state where nothing is
    // known about how fast it prefills.
    //
    // On a box slow enough to be cancelled at the client floor, that closes a loop. A
    // cancelled generation returns no timings, so `prefill_rate::observe` (one caller, in
    // the adapter, behind `if let Some(t) = &timing`) never fires; the point goes stale
    // past `FRESH_MS`; `rate_for` starts answering `None`; the bound lifts to the whole
    // window; the next prompt is that window and is cancelled again. The correction path
    // needs the very thing it corrects, so it never fires. IntelMac, 2026-09-21: ~12.5
    // prefill t/s against ~26k-token prompts, 945 cancelled slots, 8 residents, 0 acts.
    //
    // The ladder already exists and is the fix: `measured_rate_for` reads fresh → stale →
    // the slowest curve this box holds → nothing, and its own doc says to prefer it over
    // `rate_for` "wherever an absence would otherwise become a constant". Last hour's
    // measured rate is still evidence about this box; the served window never was.
    //
    // THIS NARROWS THE LOOP, IT DOES NOT CLOSE IT, and the difference is measured rather
    // than assumed (Cormac on #4321). A stale rung is LAST hour's rate, and on a box that
    // has since gone into contention it is optimistic by multiples: the IntelMac's
    // persisted rung is 63.6 t/s from 02:46Z against the ~12.5 t/s its engine prints under
    // five concurrent prefills, so 63.6 × UNATTENDED_TTFT still buys 11,448 tokens, still
    // 916 s of prefill, still cancelled at the 300 s floor, still never re-measured. The
    // no-rung arm is worse (16,384 tokens), and a freshly installed slow box never leaves
    // it, because leaving it needs a completed generation.
    //
    // What actually closes it: a cancel at the floor IS a measurement — the box
    // demonstrably did NOT prefill `prompt_tokens` inside the bound, so
    // `rate <= prompt_tokens / bound_s` — fed back as a rung that can only LOWER the EMA,
    // so each cancel shrinks the next prompt until one completes and re-measures for real.
    // The failure becomes the evidence. Card a2578811; not in this change.
    let rate = continuum_engine::inference::prefill_rate::measured_rate_for(&model);
    let affordable = Self::affordable_for_rate(rate, served, audience, continuum_engine::inference::prefill_knee::cache_fraction());
    static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
    let shape = ((served as u64) << 32)
        | (affordable as u64) << 3
        | (rate.source as u64) << 1
        | (audience == continuum_engine::inference::prefill_rate::Audience::Unattended) as u64;
    if LAST.swap(shape, std::sync::atomic::Ordering::Relaxed) != shape {
        continuum_base::probe!(
            class = "cognition.budget.prefill_bound",
            model = %model,
            served_window = served as u64,
            affordable_tokens = affordable as u64,
            // The RUNG that governed, not just the number: "stale" is the row that says
            // the box is coasting on last hour's evidence, "none" that it is on the
            // bootstrap floor. Both used to be indistinguishable from a fresh
            // measurement, because both used to be the served window.
            prefill_tps = rate.tps.unwrap_or(0.0), // unwrap_or: 0.0 renders the absence; `rate_source` is what says it IS one
            rate_source = rate.source.label(),
            audience = audience.as_str(),
            ttft_s = audience.ttft().as_secs(),
            bound = affordable < served,
            "the render budget against the measured prefill rate — a turn costs what the lane can prefill in the TTFT budget"
        );
    }
    affordable
}

/// The working window's measured knee, in tokens (see [`Self::working_window`]).
// derived-or-floor: derived — the dyno's knee over the M5 residents' own captures, 2026-10-28
// (~/.continuum/evidence/2026-10-28-working-window/): 48k cut prefill per request ~3x with
// zero handles dropped; 32k doubled turnovers; 64k kept 1.5-2x more prefill.
const WORKING_WINDOW_TOKENS: u32 = 49_152;

/// PURE: the working window for a served slot of `served` tokens, when a model is `serving`: the
/// measured knee, never above the slot. With nothing served (a fixture) it is the served window.
fn working_window_when(served: u32, serving: bool) -> u32 {
    if served == 0 || !serving {
        return served;
    }
    Self::WORKING_WINDOW_TOKENS.min(served)
}


    /// Whether a window is known: every bound below is a real number, not "no bound".
    pub fn is_known(&self) -> bool {
        self.total_chars.is_some()
    }

    /// The whole prompt this window affords, in tokens: the served window bounded by what
    /// the lane can prefill inside the turn's TTFT budget ([`Self::from_window`]). `None`
    /// when the window is unknown. The receipt a turn's tool and framing cost is read
    /// against (card 3026b83e).
    pub fn affordable_tokens(&self) -> Option<usize> {
        self.total_chars.map(|total| total / GUARD_CHARS_PER_TOKEN)
    }

    /// `1/denom` of the window in chars; `usize::MAX` (no bound) when the window is unknown.
    fn fraction(&self, denom: usize) -> usize {
        match self.total_chars {
            Some(total) => total / denom.max(1),
            None => usize::MAX,
        }
    }

    /// Head of an OLDER tool result kept in the rolling recency trail. Small on purpose: the
    /// latest act is kept whole separately, so the trail only needs enough to recognize what
    /// each past act was.
    pub fn trail_head_chars(&self) -> usize {
        self.fraction(TRAIL_HEAD_DENOM)
    }

    /// How many past steps the rolling trail carries — the COUNT to
    /// [`Self::trail_head_chars`]'s per-step SIZE.
    ///
    /// Derived, not invented: each step costs `1/TRAIL_HEAD_DENOM` of the window, and the
    /// trail as a whole is allotted `1/TRAIL_TOTAL_DENOM` — the same share
    /// [`Self::latest_action_chars`] gives the single most recent act. History gets parity
    /// with the present. So the count is `TRAIL_HEAD_DENOM / TRAIL_TOTAL_DENOM`, and because
    /// both terms scale with the window it is window-INDEPENDENT: a bigger lane buys richer
    /// steps (each head grows), not more of them. Wanting more steps on a big window is a
    /// separate decision, made by changing these denominators together — never by
    /// hardcoding a count beside them.
    ///
    /// ## Why this function exists at all
    ///
    /// It replaces `DEFAULT_WORKING_MEMORY_CAPACITY = 3`, a bare constant that survived the
    /// sweep which made every *character* bound window-derived. The result, measured on a
    /// live SWE-bench run 2026-08-05: a persona took 21 investigative acts and reached her
    /// final turn with `(+19 earlier steps aged out of working memory)` — a 21-step
    /// investigation conducted with a 3-step memory, while her prompt used 5,326 of a
    /// 16,384-token window. She re-issued calls whose results she no longer had, and
    /// restated a finding ("57 matches") whose evidence had aged out. Nothing was wrong with
    /// her reasoning; we threw away her notes with two thirds of the window empty.
    ///
    /// Long-horizon agentic work is exactly the case a 3-deep scratchpad cannot serve.
    /// [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
    /// Window-independent by construction, so it needs no budget instance: a resident's
    /// memory is built before (and outlives) any one served window.
    pub const WORKING_MEMORY_STEPS: usize = if TRAIL_HEAD_DENOM / TRAIL_TOTAL_DENOM > 1 { TRAIL_HEAD_DENOM / TRAIL_TOTAL_DENOM } else { 1 };

    /// The result of her most recent act: WHOLE, up to the window she is served. It used to be
    /// a quarter of a prefill-bounded total (later floored at an eighth of the window), so her
    /// own file reads came back cut and she re-read them in slices for hours (Benchy, the M5,
    /// 2026-10-16 and 10-17, "more than this window's 12960-char share"). Joel 2026-10-17:
    /// "Stop coding in shit to stop them from choosing what to do." A result larger than the
    /// whole window cannot fit and is still saved whole behind its handle (spill); nothing
    /// smaller is cut.
    pub fn latest_action_chars(&self) -> usize {
        self.served_chars.or(self.total_chars).unwrap_or(usize::MAX)
    }

    /// A dispatched command's result (a compile log, a sentinel's report) folded back in.
    /// Bounded harder than a foreground result because several can land at once and the
    /// full text stays recoverable through the command's own handle.
    pub fn dispatch_result_chars(&self) -> usize {
        self.fraction(12)
    }

    /// A rendered tool-output slice (`tool/output` grep windows). Kept under
    /// [`Self::latest_action_chars`] so a normal investigation is not itself re-spilled.
    pub fn render_slice_chars(&self) -> usize {
        self.fraction(4)
    }


    /// Total char share of the PROPRIOCEPTION RECEIPT ARCHIVE — the receipts-ONLY ring in
    /// [`WorkingMemory`](super::working_memory::WorkingMemory) that makes act history
    /// survive the chatty shared window (#414 option b). The shared trail evicts by entry
    /// count and receipts are RARE entries in it, so a citizen with thousands of executed
    /// acts perceived ONE (measured 2026-08-14: Asha, 2,863 acts, one visible, "+2862
    /// aged out" — and she read that starvation as "I have nothing to contribute"). The
    /// archive stores each receipt's HEAD LINE only, so 1/16 of the window holds tens of
    /// acts on a 16k lane and hundreds on a big one.
    ///
    /// Unknown window → the `MIN_SERVE_CTX` floor share, not "no bound": this bounds a
    /// STORED buffer, and (per [`Self::WORKING_MEMORY_STEPS`]'s precedent) an unbounded
    /// buffer is not an honest no-bound — it is a leak.
    pub fn receipt_archive_chars(&self) -> usize {
        match self.total_chars {
            Some(_) => self.fraction(16),
            None => {
                Self::from_window(continuum_engine::inference::planning::MIN_SERVE_CTX).fraction(16)
            }
        }
    }

    /// The RECENT-RESULTS share: the last few acts' results kept WHOLE across subsequent acts
    /// and turn boundaries, newest first, with the older ones collapsed to pointers at
    /// settlement. The `last_action` slot keeps only the LATEST result and any act overwrites
    /// it, and the pinned block is settlement-gated, so a finding survived exactly until the
    /// next trivial act or spoken turn (measured 2026-08-16: Anwen's compile error was
    /// overwritten by a `list_files` one act later). This share used to be `1/32` holding
    /// result TAILS, and a tail of a code read is broken code: on 2026-10-12 Kimi read the same
    /// file 25+ times in four hours and landed no edit. A quarter of the window, the same share
    /// the latest action gets, because a working mind's own results ARE its work.
    /// Storage bound (same leak-honesty contract as [`Self::receipt_archive_chars`]).
    pub fn recent_results_chars(&self) -> usize {
        // Her results stay until her WINDOW is full (Joel 2026-10-17: no machinery choosing
        // for her). The prompt's own fit drops the oldest history when a turn does not fit.
        match self.served_chars.or(self.total_chars) {
            Some(chars) => chars,
            None => Self::from_window(continuum_engine::inference::planning::MIN_SERVE_CTX).served_chars.unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card a2578811): an UNKNOWN prefill rate buying the whole served
    // window. `prefill_affordable_window` read `rate_for` (fresh-only) and fell through to
    // `served` on `None` — so the slowest boxes got the LARGEST prompt, and a slow box is
    // exactly where the rate goes unmeasured, because a generation cancelled at the client
    // floor never returns the timings that measure it. IntelMac, 2026-09-21: ~12.5 prefill
    // t/s, ~26k-token prompts, 945 cancelled slots, 8 residents, 0 acts.
    //
    // The rungs that matter, at the numbers that were measured. If any absence resolves to
    // `served` again, the cancel loop comes back.
    // what this catches: the prefill time bound charging cached tokens as if they had to
    // be read (the M5 2026-10-17: ~100 t/s x 180 s = ~18k on a 34,560 lane with 57% reuse,
    // so a resident's ring could not hold one 320-line read). The measured cache fraction
    // scales the bound, credited at most MAX_CACHE_CREDIT, never past the window.
    #[test]
    fn the_prefill_bound_credits_the_measured_cache() {
        use continuum_engine::inference::prefill_rate::{Audience, MeasuredRate, RateSource};
        let rate = MeasuredRate { tps: Some(100.0), source: RateSource::Fresh };
        let cold = ContextBudget::affordable_for_rate(rate, 34_560, Audience::Unattended, None);
        let half = ContextBudget::affordable_for_rate(rate, 200_000, Audience::Unattended, Some(0.5));
        assert_eq!(half, cold * 2, "half the prompt cached: twice the prompt fits the same fresh-token budget");
        let all = ContextBudget::affordable_for_rate(rate, 200_000, Audience::Unattended, Some(0.99));
        assert_eq!(all, cold * 4, "credited at most MAX_CACHE_CREDIT (4x), whatever the measured fraction");
        assert_eq!(ContextBudget::affordable_for_rate(rate, 34_560, Audience::Unattended, Some(0.9)), 34_560, "never past the served window");
    }

    #[test]
    fn an_unmeasured_prefill_rate_never_buys_the_whole_window() {
        use continuum_engine::inference::prefill_rate::{Audience, MeasuredRate, RateSource};
        let served = 26_000;
        let afford = |r| ContextBudget::affordable_for_rate(r, served, Audience::Interactive, None);

        // A rung with a number governs — even a STALE one: 12.5 t/s x 60 s = 750, under
        // MIN_SERVE_CTX, so the serve floor carries the prompt instead of the window.
        //
        // What this pins is the FUNCTION, not a promise about any box. Whether the
        // resulting prompt finishes inside the client floor depends on what the stale rung
        // SAYS, and a stale rung can be optimistic by multiples (IntelMac: 63.6 persisted
        // vs ~12.5 actual). So this arm narrows the prompt; it does not on its own
        // guarantee a completed generation. See a2578811 for the cancel-as-measurement
        // piece that does.
        let got = afford(MeasuredRate { tps: Some(12.5), source: RateSource::Stale });
        assert_eq!(
            got,
            continuum_engine::inference::planning::MIN_SERVE_CTX,
            "last hour's rate is still evidence about this box — it must bound the prompt, not be discarded"
        );
        assert!(got < served, "the whole window is what the cancel loop was made of");

        // No rung at all: the bootstrap working set, never the window.
        let got = afford(MeasuredRate::UNKNOWN);
        assert_eq!(
            got,
            continuum_engine::inference::prefill_rate::UNMEASURED_FILL_FLOOR_TOKENS as u32,
            "nothing measured on this box ever — the substrate's own bootstrap working set governs"
        );
        assert!(got < served, "an absence must never resolve to the largest prompt available");

        // A fast lane is untouched: 500 t/s x 60 s = 30,000 exceeds the window, so the
        // window still rules and nothing about the M5 changes.
        assert_eq!(
            afford(MeasuredRate { tps: Some(500.0), source: RateSource::Fresh }),
            served,
            "a lane that prefills the window inside the budget keeps the whole window"
        );

        // A window SMALLER than the unmeasured floor clamps to the window: the floor is a
        // floor on the prompt, never a licence to exceed the lane.
        assert_eq!(
            ContextBudget::affordable_for_rate(MeasuredRate::UNKNOWN, 1_024, Audience::Interactive, None),
            1_024
        );

        // Every rung, known or unknown, stays within the lane at the current geometry and a
        // smaller-than-serve-floor window (deterministic policy inputs, not hardware receipts).
        for (window, rate) in [
            (51_712, MeasuredRate { tps: Some(1_000.0), source: RateSource::Fresh }),
            (51_712, MeasuredRate::UNKNOWN),
            (256, MeasuredRate::UNKNOWN),
            (256, MeasuredRate { tps: Some(12.5), source: RateSource::Stale }),
            (0, MeasuredRate { tps: Some(1_000.0), source: RateSource::Fresh }),
        ] {
            assert!(ContextBudget::affordable_for_rate(rate, window, Audience::Interactive, None) <= window);
        }
    }

    /// what this catches: the fractions drifting away from the constants they replaced.
    /// Every bound must still land within a few percent of the hand-tuned value on the 16k
    /// lane those values were tuned against — otherwise this "no-op refactor" silently
    /// changed live behavior on every small machine.
    #[test]
    fn fractions_reproduce_the_replaced_constants_on_a_16k_lane() {
        let b = ContextBudget::from_window(16_384);
        let near = |got: usize, was: usize| {
            let (hi, lo) = (got.max(was) as f64, got.min(was) as f64);
            assert!(
                hi / lo <= 1.05,
                "bound {got} drifted from its tuned value {was}"
            );
        };
        near(b.trail_head_chars(), 800); // WM_ACTION_HEAD_CHARS
        // Her own act's result is no longer a tuned share: it is the whole served window
        // (Joel 2026-10-17), so it is pinned to the window, not to the old 12,000.
        assert_eq!(b.latest_action_chars(), 16_384 * GUARD_CHARS_PER_TOKEN);
        near(b.dispatch_result_chars(), 4_000); // DISPATCH_RESULT_MAX_CHARS
        near(b.render_slice_chars(), 12_000); // RENDER_BUDGET_CHARS
    }

    /// what this catches: the whole point of the module — a big window must actually GET a
    /// big budget. A regression that re-pins any bound to a constant fails here, because a
    /// 1M-context lane would keep returning the 16k-lane number.
    #[test]
    fn bounds_scale_with_the_window_never_pinned_to_a_constant() {
        let small = ContextBudget::from_window(16_384);
        let large = ContextBudget::from_window(1_048_576);
        assert_eq!(
            large.latest_action_chars(),
            small.latest_action_chars() * 64,
            "a 64x window must yield a 64x fold bound"
        );
        assert!(large.latest_action_chars() > 700_000);
    }

    /// what this catches: an unknown window silently becoming an invented one. `unknown()`
    /// must fold NOTHING (the deliberation guard trims downstream), not fall back to some
    /// default window — that fallback is the exact bug this module exists to kill.
    // what this catches: the working-memory DEPTH is derived from the same calibrated
    // fractions as the per-step SIZE, never a bare constant. It replaced
    // `DEFAULT_WORKING_MEMORY_CAPACITY = 3`, which starved a 21-act SWE-bench
    // investigation down to a 3-step memory while two thirds of the window sat unused
    // (measured 2026-08-05: "+19 earlier steps aged out of working memory").
    //
    // The count is window-INDEPENDENT by construction — both terms scale — so a bigger
    // lane buys richer steps, not more of them. If someone changes one denominator
    // without the other, this test says so.
    #[test]
    fn working_memory_depth_is_derived_from_the_budget_not_a_constant() {
        assert_eq!(ContextBudget::WORKING_MEMORY_STEPS, TRAIL_HEAD_DENOM / TRAIL_TOTAL_DENOM);
        assert!(
            ContextBudget::WORKING_MEMORY_STEPS > 3,
            "must be deeper than the 3-step scratchpad that lost 19 of 21 steps"
        );
    }

    #[test]
    fn an_unknown_window_folds_nothing_rather_than_inventing_a_number() {
        let b = ContextBudget::unknown();
        assert_eq!(b.latest_action_chars(), usize::MAX);
        assert_eq!(b.trail_head_chars(), usize::MAX);
    }

    // what this catches: a working window that moves with load (f45f090a6 shrank it 48k ->
    // 32k under contention and refused a resident) or exceeds the slot. With no served model
    // (every test) it is the served window, as before.
    #[test]
    fn the_working_window_is_the_measured_knee_and_never_exceeds_the_slot() {
        assert_eq!(ContextBudget::working_window_when(159_232, true), 49_152, "serving a wide slot: the measured knee");
        assert_eq!(ContextBudget::working_window_when(32_768, true), 32_768, "never above the slot");
        assert_eq!(ContextBudget::working_window_when(159_232, false), 159_232, "nothing served (a fixture): the served window");
        assert_eq!(ContextBudget::working_window_when(0, true), 0);
        assert_eq!(ContextBudget::working_window(159_232), 159_232, "no served model in a test: the served window");
    }
}
