//! `working_set_demand` — the DEMAND PRODUCER for the elastic served window (#234).
//!
//! A persona's served context window should ebb and flow with what its turns
//! ACTUALLY use, not a launch-baked constant ([[serving-resources-are-elastic-per-task-leases-context-and-model-grow-for-hard-problems]]).
//! This observes the assembled-prompt token count of recent turns and produces a
//! live demand ceiling that threads into
//! [`plan_serving_with_demand`](super::serving_plan::plan_serving_with_demand) —
//! growing the window for a persona whose tasks got bigger (a hard coding session)
//! and ebbing it back when they get lean again, so many personas stay warm at a
//! lean window and one doing heavy work gets room to think.
//!
//! ## Measured, not guessed — and never truncate the current turn
//!
//! Two signals combine:
//!   - a rolling **p95 baseline** ([`demand_ceil`]) sizes the WARM lane to the
//!     persona's usual work (so it's already big when the next hard turn arrives);
//!   - the **measured current prompt** ([`demand_for`]) guarantees THIS turn is
//!     never truncated — we already assembled the prompt, so we know its exact
//!     size and request precisely that + generation headroom.
//!
//! That is how "if it needs it larger for a moment, don't limit it" holds WITHOUT
//! guessing. The budget fit in `plan_serving_with_demand` still bounds the result
//! above, so an impossibly large prompt degrades honestly, never OOMs. p95 (not
//! max) for the baseline so a single rare spike doesn't pre-allocate a KV that
//! swaps the box — the spike is handled per-turn by the measured path instead.
//!
//! Pure + testable: token counts in, demand ceiling out. No serving loop, no GPU
//! here — this is the measurement that makes the elastic lease honest.

use std::collections::VecDeque;

/// Below this many samples [`demand_ceil`] returns the cold `floor` rather than a
/// p95 off 1–2 points: provisioning a window off a tiny sample swings it turn to
/// turn (thrash). A handful of turns is enough to trust the shape without waiting
/// so long the window never grows within a short session.
const MIN_SAMPLES_FOR_P95: usize = 4;

/// Rolling observer of a persona's per-turn working-set size (assembled-prompt
/// tokens), producing the live demand ceiling for its served window.
pub struct WorkingSetDemand {
    /// Recent assembled-prompt token counts, oldest at the front. Bounded to
    /// `window`, so a past hard session doesn't pin the ceiling forever.
    samples: VecDeque<u32>,
    /// Rolling window length (turns). ≥ 1.
    window: usize,
    /// Cold-start floor AND the hard minimum — the ceiling never drops below this,
    /// so a lull can't starve the next turn's first prompt. Callers pass the
    /// serving cold prior (`serving_plan::BOOTSTRAP_WORKING_SET`).
    floor: u32,
    /// Added to the observed prompt to leave room for the turn's GENERATION (the
    /// prompt is what's assembled; the model still needs to write its answer).
    gen_headroom: u32,
}

impl WorkingSetDemand {
    pub fn new(window: usize, floor: u32, gen_headroom: u32) -> Self {
        let window = window.max(1);
        Self {
            samples: VecDeque::with_capacity(window),
            window,
            floor,
            gen_headroom,
        }
    }

    /// Record one completed turn's assembled-prompt token count. Oldest evicted
    /// past `window` so demand EBBS back — a past hard session doesn't pin the
    /// window forever.
    pub fn observe(&mut self, prompt_tokens: u32) {
        if self.samples.len() == self.window {
            self.samples.pop_front();
        }
        self.samples.push_back(prompt_tokens);
    }

    /// The sustained baseline demand ceiling: `max(floor, p95(recent prompts) +
    /// gen_headroom)`. Floored so a lull can't starve the next turn; p95 (not max)
    /// so a lone spike doesn't over-provision. Returns `floor` until there is
    /// enough evidence ([`MIN_SAMPLES_FOR_P95`]) to trust the shape.
    pub fn demand_ceil(&self) -> u32 {
        if self.samples.len() < MIN_SAMPLES_FOR_P95 {
            return self.floor;
        }
        let mut v: Vec<u32> = self.samples.iter().copied().collect();
        v.sort_unstable();
        let idx = (((v.len() - 1) as f64) * 0.95).round() as usize;
        let p95 = v[idx.min(v.len() - 1)];
        self.floor.max(p95.saturating_add(self.gen_headroom))
    }

    /// The demand ceiling for a SPECIFIC turn whose assembled prompt we've already
    /// MEASURED: never below what this turn actually needs
    /// (`current_prompt_tokens + gen_headroom`), and never below the sustained p95
    /// baseline (so the warm lane stays sized for the persona's usual work). This
    /// is how "don't limit it for a moment" holds without guessing — we measured
    /// the prompt, so we request exactly enough. `plan_serving_with_demand` still
    /// bounds it by the budget above.
    pub fn demand_for(&self, current_prompt_tokens: u32) -> u32 {
        self.demand_ceil()
            .max(current_prompt_tokens.saturating_add(self.gen_headroom))
            .max(self.floor)
    }

    /// How many turns are in the rolling window (for observability / tests).
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The serving cold prior (serving_plan::BOOTSTRAP_WORKING_SET) + one generation
    // of headroom — the values a live caller threads in.
    const FLOOR: u32 = 16_384;
    const HEAD: u32 = 2_048;

    // what this catches: cold start returns the FLOOR — provisioning a window off
    // 1–2 samples would thrash it turn to turn, so it grows only once there's
    // enough evidence to trust the shape.
    #[test]
    fn cold_start_returns_the_floor_until_enough_samples() {
        let mut d = WorkingSetDemand::new(32, FLOOR, HEAD);
        assert_eq!(d.demand_ceil(), FLOOR);
        d.observe(40_000);
        d.observe(40_000);
        assert_eq!(d.demand_ceil(), FLOOR, "still too few samples to trust the shape");
    }

    // what this catches: a sustained hard-coding session GROWS the baseline to
    // p95 + generation headroom — the "if it needs it larger, don't limit it" case,
    // measured not guessed.
    #[test]
    fn sustained_large_working_set_grows_the_ceiling_past_the_floor() {
        let mut d = WorkingSetDemand::new(32, FLOOR, HEAD);
        for _ in 0..10 {
            d.observe(40_000);
        }
        assert_eq!(d.demand_ceil(), 40_000 + HEAD, "grows to p95(40k) + headroom");
        assert!(d.demand_ceil() > FLOOR);
    }

    // what this catches: demand EBBS BACK — a past hard session doesn't pin the
    // window forever; once lean turns roll through the window, the ceiling returns
    // to the floor and frees memory for more lanes.
    #[test]
    fn ceiling_ebbs_back_to_floor_as_lean_turns_roll_through_the_window() {
        let mut d = WorkingSetDemand::new(8, FLOOR, HEAD);
        for _ in 0..8 {
            d.observe(40_000);
        }
        assert!(d.demand_ceil() > FLOOR);
        for _ in 0..8 {
            d.observe(3_000); // fully replaces the window
        }
        assert_eq!(d.demand_ceil(), FLOOR, "lean turns pull the ceiling back to the floor");
    }

    // what this catches: a single rare SPIKE does not over-provision the WARM
    // baseline — p95 (not max) excludes the outlier so one 120k turn among lean
    // ones can't pre-allocate a KV that swaps the box.
    #[test]
    fn single_spike_does_not_over_provision_the_baseline() {
        let mut d = WorkingSetDemand::new(32, FLOOR, HEAD);
        for _ in 0..31 {
            d.observe(3_000);
        }
        d.observe(120_000); // one outlier among 31 lean turns
        assert!(
            d.demand_ceil() <= (3_000 + HEAD).max(FLOOR),
            "the lone spike must not lift the sustained baseline: {}",
            d.demand_ceil()
        );
    }

    // what this catches: the MEASURED current turn is NEVER truncated — a big prompt
    // we already assembled requests exactly its size + headroom even before the p95
    // baseline has grown ("don't limit it for a moment", from measurement not a
    // guess); a small turn still gets at least the warm baseline.
    #[test]
    fn demand_for_never_truncates_the_measured_current_turn() {
        let mut d = WorkingSetDemand::new(32, FLOOR, HEAD);
        for _ in 0..10 {
            d.observe(20_000); // baseline ~22k
        }
        assert_eq!(
            d.demand_for(60_000),
            60_000 + HEAD,
            "a measured 60k turn requests its own size + headroom, above the baseline"
        );
        assert_eq!(
            d.demand_for(1_000),
            d.demand_ceil(),
            "a tiny turn still rides the warm sustained baseline"
        );
    }
}
