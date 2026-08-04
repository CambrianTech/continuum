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
        self.fraction(64)
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

    /// Echo of ONE argument value back into the recency channel. The tightest bound, and
    /// deliberately so: she WROTE these one generation ago, so echoing a whole file's
    /// `content` back at her buys nothing and costs the window.
    pub fn echoed_arg_chars(&self) -> usize {
        self.fraction(64)
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
    /// So this is a RED TEST, not a doc. It scans the cognition tree for the shape — a
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
    fn no_new_hardcoded_context_or_prompt_size_constant_in_cognition() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cognition");
        let named = |n: &str| {
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
                    stack.push(path);
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
                    // Declared non-bound: the line above says why.
                    if i > 0 && lines[i - 1].contains("context-budget-exempt:") {
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
                    // Only sizes. A &str / bool / Duration named "...CONTEXT..." is not a bound.
                    if !["u32", "usize", "u64", "i32", "i64"].contains(&ty.trim()) {
                        continue;
                    }
                    // A bare decimal literal is the defect. An expression built from another
                    // named bound (`MIN_SERVE_CTX * 8`) is a derivation, not a fresh guess.
                    let v = value.trim().trim_end_matches(';').trim();
                    if v.chars().all(|c| c.is_ascii_digit() || c == '_') && !v.is_empty() {
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
