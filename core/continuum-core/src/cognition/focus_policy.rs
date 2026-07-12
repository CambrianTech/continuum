//! Focus → thresholds: the adapter junction where attention temperature
//! becomes numbers.
//!
//! Design: docs/cognition/FOCUS-AS-ATTENTION-TEMPERATURE.md (Joel, 2026-07-11).
//! Focus is `(target, intensity)` on the persona's concern, interpreted as
//! inverse temperature on the attention economy. This module is deliberately a
//! TRAIT AT A JUNCTION — Joel: "I like to sometimes code in a trait/adapter at
//! a junction, especially if I figure I might replace it later with far more
//! complex algorithms or ML. Pick what's feasible first, or even the constants
//! if lazy as your first adapter." The seam signature never changes across the
//! three maturity stages: constants → formula → learned policy; the persona's
//! own `focus/nudge` verb (the kernel's existing agency seam) and per-recipe
//! mode defaults (exam=1.0, dream→0) feed the SAME inputs.
//!
//! Every method's return at [`crate::persona::focus::RESTING_FOCUS`] must equal the calibrated
//! constant the seam replaced — installing this junction changes NOTHING until
//! something moves the dial. That identity is pinned by test.

/// The focus SCALAR this junction consumes is the persona focus kernel's —
/// [`crate::persona::focus::FocusState::focus`] (0..1, resting setpoint
/// [`crate::persona::focus::RESTING_FOCUS`]). One dial, one home
/// ([[compression|one logical decision, one place]]): the kernel owns the
/// state (scalar + sticky cursor + mutes, set via `focus/nudge` etc.); this
/// module owns only the PROJECTION of that scalar into cognition thresholds —
/// the "perceptual consumer" the kernel's docs anticipated.

/// The policy junction: focus in, thresholds out. One method per seam the
/// design doc names; each grows here as its seam is wired (recall first —
/// highest leverage, one number on `SignificanceRanker`).
pub trait FocusPolicy: Send + Sync {
    /// Stable id, stamped into probes/captures so an A/B is attributable.
    fn id(&self) -> &'static str;

    /// Recall significance bar in σ vs the space's measured unrelated-null
    /// (`SignificanceRanker.sigma`). At high focus only statistically
    /// exceptional memories intrude — the smoke-alarm property is inherent:
    /// truly exceptional salience clears any finite bar. At low focus weak
    /// associations surface (incubation).
    fn recall_sigma(&self, focus: f32) -> f32;
}

/// The conventional recall significance bar — 3σ vs the measured null, the
/// value `SignificanceRanker::new` shipped with before this junction existed.
/// A statistical convention, not an embedder constant.
pub const NEUTRAL_RECALL_SIGMA: f32 = 3.0;

/// Adapter A — the calibrated constants, regardless of focus. The honest
/// "lazy first adapter": installing the junction with this default changes
/// live behavior by exactly nothing, which is what makes the install safe to
/// ship mid-campaign.
#[derive(Debug, Default)]
pub struct CalibratedConstants;

impl FocusPolicy for CalibratedConstants {
    fn id(&self) -> &'static str {
        "constants"
    }

    fn recall_sigma(&self, _focus: f32) -> f32 {
        NEUTRAL_RECALL_SIGMA
    }
}

/// Adapter B — the outlier that proves the interface: a monotone linear map
/// anchored at the constants at neutral. `sigma = neutral + span·(2·intensity − 1)`,
/// clamped to a floor so a fully-diffuse mind still gates pure noise. At
/// intensity 1.0 the bar sits at `neutral + span` (only exceptional memories
/// intrude); at 0.0 it sits near the floor (weak associations flow —
/// incubation). A learned policy later replaces the line with a trained curve
/// behind the same signature.
#[derive(Debug)]
pub struct LinearBeta {
    /// σ at [`crate::persona::focus::RESTING_FOCUS`] — the calibrated anchor.
    pub neutral_sigma: f32,
    /// How far the bar travels from neutral to either extreme, in σ.
    pub span: f32,
    /// Hard floor: even a dreaming mind rejects similarity indistinguishable
    /// from the measured null at this bar.
    pub floor: f32,
}

impl Default for LinearBeta {
    fn default() -> Self {
        Self {
            neutral_sigma: NEUTRAL_RECALL_SIGMA,
            span: 2.0,
            floor: 0.5,
        }
    }
}

impl FocusPolicy for LinearBeta {
    fn id(&self) -> &'static str {
        "linear-beta"
    }

    fn recall_sigma(&self, focus: f32) -> f32 {
        let f = focus.clamp(0.0, 1.0);
        (self.neutral_sigma + self.span * (2.0 * f - 1.0)).max(self.floor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::persona::focus::RESTING_FOCUS;

    // what this catches: the junction's install contract — at NEUTRAL focus
    // every adapter must return the calibrated constant the seam replaced, so
    // installing the junction is a behavioral no-op until something moves the
    // dial. Adapter B (the formula outlier) additionally proves the interface:
    // monotone in intensity, anchored at neutral, floored below.
    #[test]
    fn adapters_anchor_at_the_calibrated_constants() {
        let a = CalibratedConstants;
        let b = LinearBeta::default();

        // Install contract: neutral == the pre-junction constant, both adapters.
        assert_eq!(a.recall_sigma(RESTING_FOCUS), NEUTRAL_RECALL_SIGMA);
        assert_eq!(b.recall_sigma(RESTING_FOCUS), NEUTRAL_RECALL_SIGMA);

        // Adapter A is focus-blind by design (the lazy-first adapter).
        assert_eq!(
            a.recall_sigma(1.0),
            NEUTRAL_RECALL_SIGMA
        );

        // Adapter B: tunnel focus raises the bar; diffuse focus lowers it to
        // the floor, never below (a dreaming mind still gates pure noise).
        assert!(b.recall_sigma(1.0) > NEUTRAL_RECALL_SIGMA);
        let dreaming = b.recall_sigma(0.0);
        assert!(dreaming < NEUTRAL_RECALL_SIGMA && dreaming >= b.floor);

        // Monotone: more focus never lowers the bar.
        let mut last = f32::MIN;
        for i in 0..=10 {
            let s = b.recall_sigma(i as f32 / 10.0);
            assert!(s >= last, "monotone in intensity");
            last = s;
        }

        // Out-of-range intensity clamps rather than extrapolating.
        assert_eq!(
            b.recall_sigma(7.0),
            b.recall_sigma(1.0)
        );
    }
}
