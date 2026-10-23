//! `animation::procedural` — Outlier A for the [`Animator`] trait: pure,
//! synchronous, deterministic, in-tick.
//!
//! It is deliberately THIN. `animate` returns [`AnimatorOutput::Builtin`]
//! unconditionally — it does NOT relocate the animation math. The 8 built-in
//! ECS systems (`animate_breathing`, `animate_blinking`, …) keep their bodies
//! verbatim and still compute the motion; this type is the *supervisory marker*
//! that selects the procedural path. (Moving the math into `animate` would kill
//! Bevy's per-system parallelism and can't express the multi-`Query` mutable
//! borrows those systems need from the `World`.)
//!
//! "Integrate the procedural animator behind the trait" = the trait *selects*,
//! the systems still *compute*. This is what makes the base engine run exactly
//! as before, but through the seam.

use super::animator::{Animator, AnimatorContext, AnimatorHealth, AnimatorOutput};
use super::prng::SlotRng;

/// The default animator every avatar is born with. Defers to the built-in
/// animation systems and is always healthy — it has nothing that can fail.
pub struct ProceduralAnimator;

impl ProceduralAnimator {
    /// The stable id used in the selection catalog and logs.
    pub const ID: &'static str = "procedural";

    pub fn new() -> Self {
        Self
    }
}

impl Default for ProceduralAnimator {
    fn default() -> Self {
        Self::new()
    }
}

impl Animator for ProceduralAnimator {
    fn id(&self) -> &str {
        Self::ID
    }

    fn animate(&mut self, _ctx: AnimatorContext<'_>, _rng: &mut SlotRng) -> AnimatorOutput {
        // Defer to the built-in systems. They run for every entity that lacks an
        // `ExternalPose`, which is exactly the procedural set — so nothing to do
        // here but declare "built-in owns this slot".
        AnimatorOutput::Builtin
    }

    fn health(&self) -> AnimatorHealth {
        AnimatorHealth::Ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::video::bevy_renderer::animation::animator::MotionIntent;
    use crate::live::video::bevy_renderer::animation::components::{Emotion, Gesture};

    fn intent() -> MotionIntent {
        MotionIntent {
            speaking: false,
            emotion: Emotion::Neutral,
            emotion_weight: 0.0,
            gesture: Gesture::None,
            cognitive: None,
        }
    }

    // what this catches: the procedural outlier ever producing a Pose (which
    // would double-write against the built-in systems) or reporting Unhealthy
    // (which would trigger a spurious supervisory detach). It MUST always defer
    // to the built-ins and always be Ready — the base engine depends on it.
    #[test]
    fn procedural_always_defers_and_is_healthy() {
        let mut a = ProceduralAnimator::new();
        assert!(!a.needs_pov_frame(), "procedural must not force readback");
        assert_eq!(a.health(), AnimatorHealth::Ready);

        let it = intent();
        let mut rng = SlotRng::new(1.0, 0);
        let ctx = AnimatorContext {
            time_secs: 1.0,
            slot: 0,
            intent: &it,
            pov_frame: None,
            utterance: None,
        };
        assert!(
            matches!(a.animate(ctx, &mut rng), AnimatorOutput::Builtin),
            "procedural must always defer to the built-in systems"
        );
    }
}
