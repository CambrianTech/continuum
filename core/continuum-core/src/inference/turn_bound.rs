//! The bound a turn is owed on the lane — sized from the MEASURED work, never a constant.
//!
//! The law (Joel, 2026-09-20: "timeouts are where AIs ruin a project"): a bound is sized
//! from the measured work, and BUSY IS NOT DEAD. Two fixed bounds shorter than a slow
//! box's prefill killed live turns and read them as dead (card ba82d0a0):
//!
//! - [`crate::inference::lane_send::PRE_STREAM_HEADER_TIMEOUT_SECS`] (300 s) — the wait
//!   for response headers after the POST, and the SSE stream's queue budget before the
//!   slot shows any work.
//! - [`crate::inference::airc_remote::transport::REMOTE_INFERENCE_DEADLINE`] (600 s) — a
//!   remote turn, end to end.
//!
//! Measured on the M5 the night this landed (`inference.prefill.complete`):
//! `total=27547 cached=0 fresh=27547 ingest_ms=389376 ingest_tok_per_s=70` and
//! `total=30672 cached=0 ingest_ms=331447 ingest_tok_per_s=92` — 331–389 s before the
//! first byte on the serving node itself, past the 300 s pre-stream bound. The IntelMac
//! prefills at ~25 tok/s: a 30k prompt is ~1200 s before the first byte, past both.
//!
//! The mind already computes what a turn is expected to cost
//! (`LlmDeliberationFaculty::expected_occupancy`: the uncached prompt at this box's
//! measured prefill rate plus her last output at its measured decode rate). This module
//! turns that expectation into the turn's bound ([`from_expectation`]) — it rides the
//! request as `TextGenerationRequest::turn_bound`, across the wire too, so the SERVING
//! peer's lane honours it — and applies it at every waiting seam ([`effective_bound`]).
//! The constants keep their names and their job (releasing ETERNAL holds); they become
//! FLOORS. Nothing here shortens a wait.

use std::time::Duration;

/// Headroom over the expected occupancy. The expectation is a MEDIAN-shaped estimate —
/// an EMA of measured rates applied to last turn's shape — so half of all turns run
/// longer than it, and a co-tenant's prefill queued ahead of ours adds its own. Twice
/// the expectation covers one turn-sized co-tenant ahead in the queue plus the EMA's
/// variance; a turn past twice its own expectation on a lane showing no progress is the
/// eternal-hold class the floors exist for.
pub const TURN_BOUND_HEADROOM: u32 = 2;

/// Which of the two candidates a waiting seam ended up bound by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundSource {
    /// The constant floor governed: the request carried no turn bound, or one below it.
    Floor,
    /// The request's own bound governed: the measured expectation with headroom.
    TurnBound,
}

impl BoundSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Floor => "floor",
            Self::TurnBound => "turn_bound",
        }
    }
}

/// A turn's bound from its expected occupancy: expectation × [`TURN_BOUND_HEADROOM`].
/// `None` in = `None` out — an absence is not a number, and the floor then governs
/// alone. A zero expectation is an absence too (nothing was measured).
pub fn from_expectation(expected: Option<Duration>) -> Option<Duration> {
    expected
        .filter(|e| !e.is_zero())
        .map(|e| e.saturating_mul(TURN_BOUND_HEADROOM))
}

/// The expectation a turn bound was derived from — the inverse of [`from_expectation`],
/// so a tripped-bound receipt can say what the mind expected without a second field
/// riding the wire.
pub fn expectation_behind(turn_bound: Duration) -> Duration {
    turn_bound / TURN_BOUND_HEADROOM
}

/// PURE: the bound in force at a waiting seam. The floor is a FLOOR: a turn bound below
/// it never tightens the wait (the floor's job is releasing eternal holds, and a small
/// expectation is still a guess), and a turn bound above it IS the wait — the measured
/// work, not the constant, decides when busy becomes dead.
pub fn effective_bound(floor: Duration, turn_bound: Option<Duration>) -> Duration {
    effective_bound_with_source(floor, turn_bound).0
}

/// [`effective_bound`] with the winner named, for the tripped receipt.
pub fn effective_bound_with_source(
    floor: Duration,
    turn_bound: Option<Duration>,
) -> (Duration, BoundSource) {
    match turn_bound {
        Some(bound) if bound > floor => (bound, BoundSource::TurnBound),
        _ => (floor, BoundSource::Floor),
    }
}

/// How many TURN-LENGTHS an ACT is owed. An act is not one generation: it is the wait
/// for a lane PLUS the generation that follows it (plus the tools it runs). The lane
/// wait is itself bounded by one turn bound ([`effective_bound`] at the serving gate),
/// so two turn bounds is the smallest honest cover for the pair — anything less and the
/// act's own deadline reaps a generation that was still inside its stated bound.
pub const ACT_BOUND_TURNS: u32 = 2;

/// PURE: the bound one ACT is owed, from the mind's measured expectation, with `floor`
/// (the named act deadline) as a FLOOR and never a ceiling.
///
/// Measured on the M5 2026-09-20 (card ebce2ba0): five generations were dropped mid-flight
/// at 1,207,290 / 1,228,814 / 1,491,253 / 1,492,283 / 1,492,456 ms — all of them the 25-minute
/// act deadline expiring, minus however far into the act the generation had started. A turn
/// whose own stated bound exceeds the act's cannot finish inside the act; the act must be
/// the larger of the two, or the substrate reaps its own healthy work.
pub fn act_bound_with_source(
    floor: Duration,
    expected: Option<Duration>,
) -> (Duration, BoundSource) {
    effective_bound_with_source(
        floor,
        from_expectation(expected).map(|b| b.saturating_mul(ACT_BOUND_TURNS)),
    )
}

/// The receipt when a bound trips: what bound, who set it, what the mind expected, and
/// where. `at` names the waiting seam (`pre_stream_headers`, `stream_queue`,
/// `remote_deadline`); `name` is the lane / model / peer the turn was on. A row whose
/// lane was busy, not dead, says the bound was undersized — that is the number to fix.
pub fn probe_tripped(
    at: &'static str,
    name: &str,
    bound: Duration,
    source: BoundSource,
    turn_bound: Option<Duration>,
) {
    crate::probe!(
        class = "inference.bound.tripped",
        at = at,
        name = name,
        bound_secs = bound.as_secs(),
        source = source.as_str(),
        turn_bound_secs = turn_bound.map(|b| b.as_secs()).unwrap_or(0), // unwrap_or: 0 = the request carried no bound; the floor governed alone
        expected_secs = turn_bound.map(|b| expectation_behind(b).as_secs()).unwrap_or(0), // unwrap_or: 0 = no expectation (her first turn, or an unmeasured box)
        "a wait bound tripped — the turn reads as dead from here; if the lane was busy, the bound was undersized"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOOR: Duration = Duration::from_secs(300);

    // what this catches (card ba82d0a0): a floor is a FLOOR. No turn bound → the floor;
    // a turn bound below the floor → the floor (a small expectation never tightens a
    // wait); a turn bound above → the turn bound (the measured work decides when busy
    // becomes dead). A regression to a fixed 300 s kills a 30k prompt on a 25 tok/s box.
    #[test]
    fn the_floor_governs_without_or_below_a_turn_bound_and_the_turn_bound_above_it() {
        assert_eq!(effective_bound(FLOOR, None), FLOOR);
        assert_eq!(effective_bound(FLOOR, Some(Duration::from_secs(120))), FLOOR);
        assert_eq!(effective_bound(FLOOR, Some(FLOOR)), FLOOR, "equal is the floor, named as such");
        let long = Duration::from_secs(2_400);
        assert_eq!(effective_bound(FLOOR, Some(long)), long);
        assert_eq!(effective_bound_with_source(FLOOR, None).1, BoundSource::Floor);
        assert_eq!(effective_bound_with_source(FLOOR, Some(FLOOR)).1, BoundSource::Floor);
        assert_eq!(effective_bound_with_source(FLOOR, Some(long)).1, BoundSource::TurnBound);
    }

    // what this catches (card ebce2ba0): an ACT covers a lane wait AND the generation
    // after it, so its bound is TWO turn bounds — and the named act deadline is a FLOOR.
    // The M5's five mid-flight drops (1,207,290 / 1,228,814 / 1,491,253 / 1,492,283 /
    // 1,492,456 ms elapsed) were the 25-minute deadline reaping generations that were
    // still inside their own stated bound. A regression that clamps the act DOWN to the
    // constant reinstates exactly that.
    #[test]
    fn an_act_is_two_turn_bounds_with_the_named_deadline_as_a_floor() {
        const ACT_FLOOR: Duration = Duration::from_secs(25 * 60);
        assert_eq!(
            act_bound_with_source(ACT_FLOOR, None),
            (ACT_FLOOR, BoundSource::Floor),
            "unmeasured = the named deadline, never a guess"
        );
        // A fast box: 2 minutes a turn. 4 x 120 s = 480 s, under the floor — the floor holds.
        assert_eq!(
            act_bound_with_source(ACT_FLOOR, Some(Duration::from_secs(120))),
            (ACT_FLOOR, BoundSource::Floor)
        );
        // The M5's measured shape: ~7 min a turn. 4 x 424 s = 1,696 s — ABOVE the 1,500 s
        // deadline that was reaping her, so the act now covers the turn it asked for.
        let (bound, source) = act_bound_with_source(ACT_FLOOR, Some(Duration::from_secs(424)));
        assert_eq!(source, BoundSource::TurnBound);
        assert!(bound > ACT_FLOOR, "a turn the deadline cannot hold RAISES the deadline");
        assert_eq!(bound, Duration::from_secs(424) * TURN_BOUND_HEADROOM * ACT_BOUND_TURNS);
        // And the act always outlasts the lane wait its own turn bound sets, so the gate
        // can never eat the whole act: 2x the turn bound vs 1x at the gate.
        let gate = from_expectation(Some(Duration::from_secs(424))).expect("a bound");
        assert!(bound >= gate.saturating_mul(2), "act >= lane wait + generation");
    }

    // what this catches: the bound is the expectation WITH headroom (an expectation is a
    // median, not a ceiling), an absence stays an absence (never a fabricated bound), and
    // the receipt can recover the expectation from the bound alone.
    #[test]
    fn a_turn_bound_is_the_expectation_with_headroom_and_an_absence_stays_absent() {
        assert_eq!(from_expectation(None), None);
        assert_eq!(from_expectation(Some(Duration::ZERO)), None);
        let expected = Duration::from_secs(1_280); // 30k uncached at 25 tok/s + 800 out at 10 tok/s
        let bound = from_expectation(Some(expected)).expect("an expectation yields a bound");
        assert_eq!(bound, expected * TURN_BOUND_HEADROOM);
        assert_eq!(expectation_behind(bound), expected);
        assert!(bound > FLOOR, "the IntelMac's 30k prompt outgrows the 300 s floor");
        assert!(bound > Duration::from_secs(600), "…and the 600 s remote floor");
    }
}
