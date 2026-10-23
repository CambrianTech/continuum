//! `animation::animator` — the seam a VLM plugs into.
//!
//! Bevy resists "one object owns and returns a pose" (a `Box<dyn>` can't hold
//! the multi-`Query` mutable borrows a Bevy system needs). So the seam inverts
//! it: an [`Animator`] is a pure *producer of a value* — it takes a read-only
//! [`AnimatorContext`] snapshot and returns an [`AnimatorOutput`]. One ECS system
//! ([`super::registry::drive_animators`]) calls it; another ([`super::pose::apply_external_pose`])
//! applies any produced [`SkeletonPose`]. The animator never touches the `World`.
//!
//! Two maximally-different outliers validate this trait (outlier-then-STOP):
//! - [`super::procedural::ProceduralAnimator`] — pure, synchronous, deterministic,
//!   in-tick, returns [`AnimatorOutput::Builtin`] (the 8 built-in systems compute).
//! - `VlaAnimator` (Slice 3) — async, stochastic, network-fed, out-of-tick,
//!   `try_recv`s remote poses. If both fit without forcing, the trait is proven.

use super::components::{Emotion, Gesture};
use super::pose::SkeletonPose;
use super::prng::SlotRng;
use crate::live::avatar::RgbaFrame;
use crate::live::session::cognitive_animation::CognitiveState;

/// A read-only snapshot of an entity's animation intent for one tick — the
/// distilled ECS state an [`Animator`] may consult. Entity-free (like
/// [`SkeletonPose`]) so a remote animator could be handed the same snapshot.
#[derive(Debug, Clone)]
pub struct MotionIntent {
    /// Is the entity currently speaking (mouth should move)?
    pub speaking: bool,
    /// The active emotion and how strongly it's expressed [0,1].
    pub emotion: Emotion,
    pub emotion_weight: f32,
    /// The active body gesture (may be `Gesture::None`).
    pub gesture: Gesture,
    /// The AI cognitive state driving gesture selection, if any.
    pub cognitive: Option<CognitiveState>,
}

/// Everything an [`Animator::animate`] call may read for one entity, one tick.
/// All borrowed — the context outlives no tick and owns nothing.
///
/// These fields are the seam's forward contract: the procedural outlier (Slice 2)
/// validates the trait by *deferring* (`Builtin`) and so reads none of them; the
/// VLA outlier (Slice 3) reads all of them (POV frame, utterance, intent). The
/// `allow` marks that intentional producer/consumer gap, not dead code.
#[allow(dead_code)]
pub struct AnimatorContext<'a> {
    /// Seconds since renderer start (the animation clock).
    pub time_secs: f32,
    /// Which render slot this entity occupies.
    pub slot: u8,
    /// The distilled animation intent for this tick.
    pub intent: &'a MotionIntent,
    /// The avatar's own last rendered frame — only populated when the animator
    /// declares [`Animator::needs_pov_frame`] (a VLA that watches its own body).
    /// `None` for procedural (no readback cost incurred).
    pub pov_frame: Option<&'a RgbaFrame>,
    /// The utterance being spoken this tick, if any (a VLA may gesture to words).
    pub utterance: Option<&'a str>,
}

/// What an [`Animator`] produced for one tick.
pub enum AnimatorOutput {
    /// A concrete pose to apply this tick (drives [`super::pose::apply_external_pose`]).
    Pose(SkeletonPose),
    /// "Let the built-in animation systems compute this entity" — the procedural
    /// path. No [`super::pose::ExternalPose`] is written; the 8 gated systems run.
    Builtin,
    /// "I have nothing new this tick" — hold the last applied pose. Honest
    /// "thinking"; NEVER invent motion, NEVER a per-frame fallback.
    Pending,
}

/// An animator's operational health — reported to the supervisor
/// ([`super::registry::drive_animators`]) which decides detach/revert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimatorHealth {
    /// Producing poses (or deliberately deferring to built-ins) normally.
    Ready,
    /// Failed; the string names the cause (fail-loud). The supervisor logs it
    /// and reverts the slot to procedural — a recovery, not a silent fallback.
    Unhealthy(String),
}

/// A producer of avatar motion, selected per-slot at `Load`. See the module doc
/// for why this is a value-producer, not a Bevy system.
pub trait Animator: Send + Sync {
    /// Stable identifier (for logs, health surfacing, deterministic selection).
    fn id(&self) -> &str;

    /// Does this animator need the avatar's own last frame in [`AnimatorContext`]?
    /// Default `false` so procedural animators incur ZERO readback cost — only a
    /// self-watching VLA opts in. Consulted by the supervisor's readback wiring in
    /// Slice 3; the default keeps procedural free until then.
    #[allow(dead_code)]
    fn needs_pov_frame(&self) -> bool {
        false
    }

    /// Produce this tick's output. `rng` is **lent, not owned**, so procedural
    /// determinism is byte-identical across replays (the seed lives with the slot,
    /// not the animator). Must not touch the `World` or do blocking I/O.
    fn animate(&mut self, ctx: AnimatorContext<'_>, rng: &mut SlotRng) -> AnimatorOutput;

    /// Current health — the supervisor consults this to decide detach/revert.
    fn health(&self) -> AnimatorHealth;
}
