//! The **Will** — a persona's constraint on the single effort/resolution axis,
//! expressed the way a camera SDK expresses a capture request.
//!
//! A persona never names a lane, a model, or a number of seconds (that is the
//! hardcoding trap — see [`super::serving_plan`] and the de-hardcoding contract in
//! `docs/architecture/WILL-DRIVEN-RESOLUTION.md` §6). It expresses *intent + an
//! acceptable range* on one normalized axis, exactly like
//! `getUserMedia({ width: { min, ideal, max } })`, and the scheduler negotiates the
//! operating point against whatever live capacity exists (2 lanes or 20, local or
//! grid — identical logic).
//!
//! This is the value object of that request. It carries no policy; the
//! [`super::resolution`] escalator turns a `Will` + a *verifier* into an operating
//! point over live capacity, and climbs when the verifier demonstrates the current
//! resolution was insufficient. **Necessity always overrides the persona's own
//! guess** — the verifier can push the actual operating point above the whole band
//! (WILL-DRIVEN-RESOLUTION.md §2, §3).
//!
//! Doctrine: [[intelligence-is-a-resolution-field-shared-across-the-mesh]],
//! [[conversational-latency-is-a-misdirection-budget]],
//! [[model-fit-is-the-priority-single-machine-first]].

/// How far above `target` the confidence band is allowed to reach per unit of
/// `uncertainty`. A fully-uncertain persona (`uncertainty == 1.0`) starts a full
/// `HEADROOM` below `target` and lets the verifier feel the real requirement out;
/// a fully-confident one (`uncertainty == 0.0`) starts exactly at `target`. This is
/// the ONLY tuning constant in the value object, and it shapes the *starting* point
/// only — it never bounds where a failing verifier may climb to.
const HEADROOM: f32 = 0.35;

/// A persona's felt request on the effort/resolution axis. All three scalars are
/// normalized to `[0.0, 1.0]`, where `0.0` is the cheapest revisable draft and `1.0`
/// is the most capable resolution live capacity can afford. The mapping from this
/// abstract axis to concrete warm models / compute is the escalator's job, never the
/// persona's (the camera-SDK inversion: you ask for resolution, the framework picks
/// the format).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Will {
    /// The ideal operating point — "what I'd ask for with no contention." The
    /// persona feeling task complexity ("my 80%").
    target: f32,
    /// The minimum acceptable resolution = **stakes**. A hard lower bound the
    /// scheduler defends under contention and the verifier bar a draft must clear.
    /// Orthogonal to `target`: a one-line config change *feels* trivial (`target`
    /// low) yet can be high-stakes (`floor` high) — so `floor` may exceed `target`,
    /// and when it does the persona operates at `floor` regardless of the low felt
    /// complexity.
    floor: f32,
    /// Confidence in `target`, as `1.0 - confidence` → the ± band. High uncertainty
    /// ("80% but I'm unsure") starts the draft lower and leans on escalation; low
    /// uncertainty ("I've seen this exact class") starts straight at `target`. The
    /// camera auto-mode analog: ambiguous scene → bracket and adapt; confident scene
    /// → lock the format.
    uncertainty: f32,
}

impl Will {
    /// Construct a `Will`, clamping every scalar into `[0.0, 1.0]`. Clamping (not
    /// rejecting) is deliberate: the felt-stance projection that produces a `Will`
    /// is soft, and an out-of-range value is a saturated intent, not an error to
    /// surface to an operator.
    pub fn new(target: f32, floor: f32, uncertainty: f32) -> Self {
        Self {
            target: clamp_unit(target),
            floor: clamp_unit(floor),
            uncertainty: clamp_unit(uncertainty),
        }
    }

    /// The bootstrap stance used before the learned effort-predictor head exists
    /// (WILL-DRIVEN-RESOLUTION.md §8 step 1): a modest ideal, a low floor, and wide
    /// uncertainty — i.e. *start cheap and let the verifier pull depth up*. This is a
    /// UNIFORM default, applied identically to every turn — the legal "start at a
    /// revisable low resolution" of §2, NOT a per-turn substrate judgement of whether
    /// this particular turn is light or heavy (forbidden-move #98).
    pub fn bootstrap() -> Self {
        Self::new(0.5, 0.1, 0.9)
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn floor(&self) -> f32 {
        self.floor
    }

    pub fn uncertainty(&self) -> f32 {
        self.uncertainty
    }

    /// The resolution to DRAFT at first: `target` pulled down by the uncertainty
    /// band, but never below `floor`. Confident wills start at `target`; uncertain
    /// wills start cheaper and rely on the verifier to escalate.
    pub fn start_point(&self) -> f32 {
        let pulled = self.target - self.uncertainty * HEADROOM;
        clamp_unit(pulled.max(self.floor))
    }

    /// The top of the persona's own confidence band: `target` plus the uncertainty
    /// headroom. This is where the persona *guesses* the ceiling is — the escalator
    /// may still be pushed ABOVE it by a verifier that keeps failing (necessity
    /// overrides the guess), so this is a scheduling hint, not a cap.
    pub fn confident_ceiling(&self) -> f32 {
        clamp_unit((self.target + self.uncertainty * HEADROOM).max(self.floor))
    }

    /// Does an available operating point `res` satisfy this will's hard floor? The
    /// scheduler uses this to reuse an already-warm model that lands within a
    /// persona's acceptable range rather than paging in its exact ideal (the
    /// anti-thrash degree of freedom, §4).
    pub fn accepts(&self, res: f32) -> bool {
        res + f32::EPSILON >= self.floor
    }
}

impl Default for Will {
    /// The bootstrap stance. See [`Will::bootstrap`].
    fn default() -> Self {
        Self::bootstrap()
    }
}

fn clamp_unit(v: f32) -> f32 {
    // NaN-safe clamp into [0,1]: a NaN felt-stance collapses to the cautious 0.0
    // rather than propagating.
    if v.is_nan() {
        0.0
    } else {
        v.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the three scalars stay in [0,1] and a NaN collapses to 0 —
    // a felt-stance projection can produce garbage and must never propagate it into
    // the scheduler as an out-of-range resolution.
    #[test]
    fn new_clamps_every_scalar_and_neutralizes_nan() {
        let w = Will::new(1.7, -0.3, 2.0);
        assert_eq!(w.target(), 1.0);
        assert_eq!(w.floor(), 0.0);
        assert_eq!(w.uncertainty(), 1.0);

        let nanny = Will::new(f32::NAN, 0.5, f32::NAN);
        assert_eq!(nanny.target(), 0.0);
        assert_eq!(nanny.floor(), 0.5);
        assert_eq!(nanny.uncertainty(), 0.0);
    }

    // what this catches: uncertainty pulls the DRAFT point below target (start cheap,
    // let the verifier feel it out) but the hard floor always wins — a confident will
    // drafts at target, an uncertain one lower, and neither ever drafts below stakes.
    #[test]
    fn start_point_respects_uncertainty_then_floor() {
        // Confident: uncertainty 0 → draft exactly at target.
        let confident = Will::new(0.8, 0.1, 0.0);
        assert!((confident.start_point() - 0.8).abs() < 1e-6);

        // Uncertain: pulled below target by the band.
        let unsure = Will::new(0.8, 0.1, 1.0);
        assert!(unsure.start_point() < 0.8, "wide band drafts cheaper");
        assert!(unsure.start_point() >= 0.1, "never below floor");

        // High-stakes, low-complexity: floor exceeds target → operate at floor even
        // though the task feels trivial.
        let config_change = Will::new(0.2, 0.7, 0.5);
        assert!((config_change.start_point() - 0.7).abs() < 1e-6);
    }

    // what this catches: accepts() gates whether an already-warm model at resolution
    // `res` clears the will's floor — the anti-thrash reuse decision. A model at or
    // above floor is acceptable; below is not.
    #[test]
    fn accepts_is_the_floor_gate() {
        let w = Will::new(0.6, 0.4, 0.3);
        assert!(w.accepts(0.4), "exactly at floor is acceptable");
        assert!(w.accepts(0.9), "above floor is acceptable");
        assert!(!w.accepts(0.39), "below floor is rejected");
    }
}
