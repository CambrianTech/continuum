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
//! | echoed args         | 600 †        | ~1/64  (768)                   |
//!
//! † the echoed-args bound is the one value that was NOT battle-tuned — it was invented
//! (by me) on 2026-08-03, the same day this module was written to delete it. It shares the
//! trail-head fraction because it plays the same role: a recognizable stub of something she
//! already has in full elsewhere.
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

use super::deliberation_budget::GUARD_CHARS_PER_TOKEN;

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
}

impl ContextBudget {
    /// Bounds derived from a live served context window, in tokens.
    pub fn from_window(context_window: u32) -> Self {
        Self {
            total_chars: (context_window > 0)
                .then(|| (context_window as usize).saturating_mul(GUARD_CHARS_PER_TOKEN)),
        }
    }

    /// Bounds from the window the local lane is serving RIGHT NOW — for seams that have no
    /// `WorkspaceCycle` to read a model binding off (a command handler, a bus listener).
    /// Not-ready or not-yet-served resolves to [`Self::unknown`], never to a stand-in number.
    pub fn live() -> Self {
        let serving = crate::inference::llama_server::current_serving();
        if serving.ready && serving.served_context_window > 0 {
            Self::from_window(serving.served_context_window)
        } else {
            Self::unknown()
        }
    }

    /// Like [`Self::live`], but an unknown window resolves to the substrate's own declared
    /// minimum ([`MIN_SERVE_CTX`](crate::cognition::serving_plan::MIN_SERVE_CTX)) instead of
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
            Self::from_window(crate::cognition::serving_plan::MIN_SERVE_CTX)
        }
    }

    /// The honest "I don't know the window" budget: nothing is folded. See the module doc's
    /// unknown-window contract — the deliberation guard is the real backstop.
    pub fn unknown() -> Self {
        Self { total_chars: None }
    }

/// Share of the served window recall may spend. See [`ContextBudget::recall_tokens`].
const RECALL_DENOM: usize = 10;

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
    pub fn working_memory_steps(&self) -> usize {
        (TRAIL_HEAD_DENOM / TRAIL_TOTAL_DENOM).max(1)
    }

    /// The FULL most-recent-action block. This one rides in the volatile prompt tail and
    /// re-prefills every turn it is present, so it is the biggest latency lever in the
    /// prompt — a quarter of the window is the balance between "she can work with what her
    /// hands just fetched" and "one `code/tree` dump costs 30s of re-prefill per turn".
    pub fn latest_action_chars(&self) -> usize {
        self.fraction(4)
    }

    /// A single tool result folded into the next perception / engram. The most generous
    /// bound: a traceback she must READ to self-correct has to survive intact.
    pub fn result_fold_chars(&self) -> usize {
        self.fraction(3)
    }

    /// A dispatched command's result (a compile log, a sentinel's report) folded back in.
    /// Bounded harder than a foreground result because several can land at once and the
    /// full text stays recoverable through the command's own handle.
    pub fn dispatch_result_chars(&self) -> usize {
        self.fraction(12)
    }

    /// A rendered tool-output slice (`tool/output` grep windows). Kept under
    /// [`Self::result_fold_chars`] so a normal investigation is not itself re-spilled.
    pub fn render_slice_chars(&self) -> usize {
        self.fraction(4)
    }

    /// ONE tool's one-line summary in the catalog she is offered. A catalog lists ~100 tools,
    /// so an unbounded description per line re-creates the dump this bound exists to prevent —
    /// but on a roomy window she can afford a fuller clause per tool, which is real PX. At a
    /// 16k lane this is exactly the 96 chars the old constant hard-coded.
    pub fn catalog_summary_chars(&self) -> usize {
        self.fraction(512)
    }

    /// Default CEILING on one response's generated length, in TOKENS (not chars) — the role's
    /// fallback budget when it declares none. An eighth of the window: enough that a real
    /// answer (a function, a diff, a traceback walk) is never truncated mid-thought, while
    /// still leaving the prompt its room. #45's doctrine is that the adapter owns generation
    /// length; this is the fallback for a role that says nothing, and like every other bound
    /// here it must scale — a `512` default silently truncates every model's answers equally,
    /// whether it holds 4k or 1M.
    pub fn default_response_tokens(&self) -> u32 {
        match self.total_chars {
            // total_chars = window * GUARD_CHARS_PER_TOKEN, so /8 in tokens is /(8*ratio) here.
            Some(total) => ((total / GUARD_CHARS_PER_TOKEN) / 8).min(u32::MAX as usize) as u32,
            None => u32::MAX,
        }
    }

    /// Tokens recall may spend surfacing past memories.
    ///
    /// Recall is ONE perception faculty among many (roster, doctrine, working memory)
    /// plus the room transcript and the identity prompt — it must never crowd them
    /// out. `1/RECALL_DENOM` keeps the live message dominant while still carrying the
    /// relevant past, and it SCALES: a 1M-context model gets proportionally more
    /// recall instead of a fixed 4k-shaped slice.
    ///
    /// Moved here 2026-08-20 from `recall_faculty.rs`, where it lived as
    /// `RECALL_WINDOW_FRACTION: f32 = 0.10` — a bare float that this module's own
    /// guard could not see, because the guard's type list stopped at the integers.
    /// Widening it to floats surfaced this on the first run.
    pub fn recall_tokens(&self) -> usize {
        match self.total_chars {
            Some(total) => (total / GUARD_CHARS_PER_TOKEN) / Self::RECALL_DENOM,
            None => usize::MAX,
        }
    }

    /// Echo of ONE argument value back into the recency channel. The tightest bound, and
    /// deliberately so: she WROTE these one generation ago, so echoing a whole file's
    /// `content` back at her buys nothing and costs the window.
    pub fn echoed_arg_chars(&self) -> usize {
        self.fraction(64)
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
    /// STORED buffer, and (per [`Self::working_memory_steps`]'s precedent) an unbounded
    /// buffer is not an honest no-bound — it is a leak.
    pub fn receipt_archive_chars(&self) -> usize {
        match self.total_chars {
            Some(_) => self.fraction(16),
            None => {
                Self::from_window(crate::cognition::serving_plan::MIN_SERVE_CTX).fraction(16)
            }
        }
    }

    /// The steps-taken LEDGER's rendered share — how much of the receipt archive the
    /// perception fact re-injects per turn. One trail-head-equivalent (`1/64`): head
    /// lines only, newest first to fit, so the ledger shows a citizen her recent act
    /// HISTORY without re-creating the double-payment the #324 dedup removed. Render
    /// bound, not storage: unknown window keeps the honest no-bound (the deliberation
    /// guard trims downstream), same contract as [`Self::trail_head_chars`].
    pub fn steps_ledger_chars(&self) -> usize {
        self.fraction(TRAIL_HEAD_DENOM)
    }

    /// The RECENT-RESULTS buffer share — the last few acts' result TAILS, kept across
    /// subsequent acts and turn boundaries. The `last_action` slot keeps only the
    /// LATEST result and any act overwrites it, and the pinned block is settlement-
    /// gated — so a finding survived exactly until the next trivial act or spoken
    /// turn (measured 2026-08-16: Anwen's compile error was overwritten by a
    /// `list_files` one act later; her next three turns wandered discovery tools and
    /// she re-ran the same unfixed code — the missing-feedback loop). `1/32` of the
    /// window holds a handful of ~300-char tails on a 16k lane, more on a big one.
    /// Storage bound (same leak-honesty contract as [`Self::receipt_archive_chars`]).
    pub fn recent_results_chars(&self) -> usize {
        match self.total_chars {
            Some(_) => self.fraction(32),
            None => {
                Self::from_window(crate::cognition::serving_plan::MIN_SERVE_CTX).fraction(32)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        near(b.latest_action_chars(), 12_000); // WM_ACTION_FULL_MAX_CHARS
        near(b.result_fold_chars(), 16_000); // RESULT_FOLD_MAX_CHARS
        near(b.dispatch_result_chars(), 4_000); // DISPATCH_RESULT_MAX_CHARS
        near(b.render_slice_chars(), 12_000); // RENDER_BUDGET_CHARS
        near(b.catalog_summary_chars(), 96); // SUMMARY_MAX_CHARS
                                             // Echoed args share the trail-head fraction (see the module doc's † note — the old
                                             // 600 was invented, not tuned, so it is not a calibration target).
        assert_eq!(b.echoed_arg_chars(), b.trail_head_chars());
    }

    /// what this catches: the whole point of the module — a big window must actually GET a
    /// big budget. A regression that re-pins any bound to a constant fails here, because a
    /// 1M-context lane would keep returning the 16k-lane number.
    #[test]
    fn bounds_scale_with_the_window_never_pinned_to_a_constant() {
        let small = ContextBudget::from_window(16_384);
        let large = ContextBudget::from_window(1_048_576);
        assert_eq!(
            large.result_fold_chars(),
            small.result_fold_chars() * 64,
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
        let small = ContextBudget::from_window(4_096);
        let lane = ContextBudget::from_window(16_384);
        let huge = ContextBudget::from_window(1_000_000);

        assert_eq!(
            lane.working_memory_steps(),
            TRAIL_HEAD_DENOM / TRAIL_TOTAL_DENOM
        );
        assert_eq!(
            small.working_memory_steps(),
            lane.working_memory_steps(),
            "depth is a RATIO of two window fractions, so it does not shrink on a small lane"
        );
        assert_eq!(
            huge.working_memory_steps(),
            lane.working_memory_steps(),
            "nor grow on a huge one — a bigger window buys richer steps, not more of them"
        );
        assert!(
            lane.working_memory_steps() > 3,
            "must be deeper than the 3-step scratchpad that lost 19 of 21 steps"
        );
        // An unknown window must still yield a USABLE depth. Unlike a char bound, an
        // unbounded COUNT is not "no bound" — it is an unbounded buffer, so the honest
        // answer here is the derived ratio, not usize::MAX.
        assert_eq!(
            ContextBudget::unknown().working_memory_steps(),
            lane.working_memory_steps()
        );
    }

    #[test]
    fn an_unknown_window_folds_nothing_rather_than_inventing_a_number() {
        let b = ContextBudget::unknown();
        assert_eq!(b.result_fold_chars(), usize::MAX);
        assert_eq!(b.trail_head_chars(), usize::MAX);
        assert_eq!(b.echoed_arg_chars(), usize::MAX);
    }

    /// what this catches: THE SIXTH TIME. This defect has been fixed and has come back at
    /// least five times — #45 (max_tokens clamps), #46 (per-tier context caps), #50 (single-
    /// source the effective window), #124 (`EVAL_LANE_CONTEXT = 16_384`, deleted), and then
    /// `EXAM_LANE_CTX = 16384` reintroduced the *same* constant in the *same file* three
    /// weeks later under a determinism justification. Doc comments and task entries did not
    /// stop it; each new author re-derives a "reasonable" number in good faith.
    ///
    /// So this is a RED TEST, not a doc. It scans the whole crate for the shape — a
    /// constant whose name says window/context/token/prompt/chars and whose value is a bare
    /// literal — and fails on a new one. Adding a bound is still allowed: put it on
    /// [`ContextBudget`] as a FRACTION, where it scales with the served window. If you are
    /// here because this test failed, that is the fix; do not add your name to the list above.
    ///
    /// A constant that genuinely is NOT a window-relative bound (a phrase length, a dedup key
    /// prefix, another model's stated REQUIREMENT) declares itself with a
    /// `context-budget-exempt: <why>` comment on the line above. Exemptions are cheap to write
    /// and permanently visible in review — which is the point. A silent exemption is not
    /// available, because silence is how this came back five times.
    ///
    /// Weakening this test is the same act as re-introducing the constant.
    #[test]
    fn no_new_hardcoded_context_or_prompt_size_constant_anywhere_in_the_crate() {
        // Crate-wide, not just cognition/: the 4096/2048 adapter floor (#46's own surface) sat
        // outside cognition and would not have been caught by a cognition-only scan.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let named = |n: &str| {
            // Categorically not size bounds, so they don't need to spend an exemption line:
            // a duration (`*_MS`), an identifier (`*_ID`), or a vocabulary offset (`*_OFFSET`).
            if n.ends_with("_MS") || n.ends_with("_ID") || n.ends_with("_OFFSET") {
                return false;
            }
            ["WINDOW", "CONTEXT", "CTX", "TOKEN", "PROMPT", "CHARS"]
                .iter()
                .any(|k| n.contains(k))
        };
        let mut offenders = Vec::new();
        let mut stack = vec![dir];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    // Vendored upstream code (candle/llama ports) is not ours to restyle; its
                    // constants are the upstream model's own defaults.
                    if !path.ends_with("vendored") {
                        stack.push(path);
                    }
                    continue;
                }
                if path.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                // This module IS the sanctioned home for the calibration; its doc table
                // quotes the very constants it replaced.
                if path.file_name().is_some_and(|f| f == "context_budget.rs") {
                    continue;
                }
                let Ok(src) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let lines: Vec<&str> = src.lines().collect();
                for (i, line) in lines.iter().enumerate() {
                    let t = line.trim_start();
                    // Declared non-bound: the marker appears anywhere in the contiguous
                    // comment block directly above (a reason worth writing often needs two
                    // lines, and a doc comment may sit between it and the const).
                    let exempt = lines[..i]
                        .iter()
                        .rev()
                        .take_while(|l| {
                            let t = l.trim_start();
                            t.starts_with("//") || t.is_empty()
                        })
                        .any(|l| l.contains("context-budget-exempt:"));
                    if exempt {
                        continue;
                    }
                    let Some(rest) = t.strip_prefix("const ").or_else(|| {
                        t.strip_prefix("pub const ")
                            .or_else(|| t.strip_prefix("pub(super) const "))
                            .or_else(|| t.strip_prefix("pub(crate) const "))
                    }) else {
                        continue;
                    };
                    let Some((name, tail)) = rest.split_once(':') else {
                        continue;
                    };
                    if !named(name.trim()) {
                        continue;
                    }
                    let Some((ty, value)) = tail.split_once('=') else {
                        continue;
                    };
                    // Only sizes and RATIOS. A &str / bool / Duration named "...CONTEXT..." is
                    // not a bound.
                    //
                    // The float types are here because of a hole this test had for its whole
                    // life, found 2026-08-20 the first time anyone wrote a window-relative
                    // constant that wasn't a token COUNT: `WINDOW_COMPARABILITY_FACTOR: f64 =
                    // 2.0` (#2339, mine). Name matched, value was a bare literal, and it
                    // sailed through — because the type list stopped at the integers. A guard
                    // against invented numbers that a `f64` annotation defeats teaches exactly
                    // one lesson: type your magic number as a float. A ratio over the window is
                    // as much a fresh guess as a count of its tokens, and is caught the same way.
                    if !["u32", "usize", "u64", "i32", "i64", "f32", "f64"].contains(&ty.trim()) {
                        continue;
                    }
                    // A bare literal is the defect — decimal or float, since both are equally
                    // guessed. An expression built from another named bound (`MIN_SERVE_CTX * 8`)
                    // is a derivation, not a fresh guess.
                    let v = value.trim().trim_end_matches(';').trim();
                    let bare_literal = !v.is_empty()
                        && v.chars().all(|c| c.is_ascii_digit() || c == '_' || c == '.')
                        // `.` only as a decimal point: `1.5` yes, `A.b` / `1..2` no.
                        && v.matches('.').count() <= 1
                        && v.starts_with(|c: char| c.is_ascii_digit());
                    if bare_literal {
                        offenders.push(format!(
                            "{}:{} — const {}: {} = {}",
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            i + 1,
                            name.trim(),
                            ty.trim(),
                            v
                        ));
                    }
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "hardcoded context/prompt size constant(s) reintroduced in cognition — express \
             these as a fraction on ContextBudget so they scale with the SERVED window \
             (a 4k-shaped constant makes a 1M-context model useless and deletes the reason \
             to run MoE at all):\n  {}",
            offenders.join("\n  ")
        );
    }
}
