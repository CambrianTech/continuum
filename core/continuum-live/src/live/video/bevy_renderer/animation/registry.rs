//! `animation::registry` — per-slot animator ownership + the supervisor that
//! drives them.
//!
//! [`AnimatorRegistry`] is a Bevy `Resource` (not per-entity components) because
//! `&mut dyn Animator` can't live in a `Query` — consistent with the existing
//! `GpuGuards` resource pattern. [`select_animator_for_identity`] chooses an
//! animator deterministically per persona (mirroring the scene birther's
//! `deterministic_pick`), but the catalog on a base host contains ONLY
//! `procedural`, so a host with no VLA transport can only ever birth procedural.
//!
//! [`drive_animators`] is the supervisor: each tick it snapshots every slot's
//! [`MotionIntent`], calls its animator's [`Animator::animate`], and dispatches
//! the [`AnimatorOutput`] — writing/clearing [`ExternalPose`] so the built-in
//! systems and [`apply_external_pose`] hand off cleanly. It also enforces
//! [`AnimatorHealth`]: an `Unhealthy` animator is detached (logged, reverted to
//! procedural) — a recovery, never a silent per-frame fallback.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::clog_warn;

use super::animator::{Animator, AnimatorContext, AnimatorHealth, AnimatorOutput, MotionIntent};
use super::components::{
    CognitiveGesture, Emotion, EmotionAnimation, Gesture, GestureAnimation, SlotId, Speaking,
};
use super::pose::{ExternalPose, PoseSource};
use super::prng::SlotRng;
use super::procedural::ProceduralAnimator;
use crate::live::avatar::hash::deterministic_pick;

/// Per-slot animator ownership. Keyed by raw slot id (`SlotId.0`) because
/// `SlotId` is a plain marker component, not a map key.
#[derive(Resource, Default)]
pub struct AnimatorRegistry {
    animators: HashMap<u8, Box<dyn Animator>>,
}

impl AnimatorRegistry {
    /// Assign (or replace) the animator owning a slot.
    pub fn insert(&mut self, slot: u8, animator: Box<dyn Animator>) {
        self.animators.insert(slot, animator);
    }

    /// Remove a slot's animator (on teardown or supervisory detach).
    pub fn remove(&mut self, slot: u8) -> Option<Box<dyn Animator>> {
        self.animators.remove(&slot)
    }

    /// Mutable access to a slot's animator (the supervisor drives it).
    pub fn get_mut(&mut self, slot: u8) -> Option<&mut Box<dyn Animator>> {
        self.animators.get_mut(&slot)
    }

    pub fn len(&self) -> usize {
        self.animators.len()
    }

    pub fn is_empty(&self) -> bool {
        self.animators.is_empty()
    }
}

/// The animator ids available on THIS host. Base host = procedural only; the VLA
/// id joins here in Slice 3 only when a transport is present AND opt-in — never a
/// pure hash outcome, so the base engine's determinism never depends on a GPU/peer.
fn animator_catalog() -> Vec<&'static str> {
    vec![ProceduralAnimator::ID]
}

/// Construct an animator by id. Called only with ids from [`animator_catalog`],
/// so an unknown id is a programming error — fail loud (name the id).
fn build_animator(id: &str) -> Box<dyn Animator> {
    match id {
        ProceduralAnimator::ID => Box::new(ProceduralAnimator::new()),
        other => panic!("no animator registered for id `{other}` (catalog drift)"),
    }
}

/// Deterministically select the animator for a persona identity. Same identity →
/// same choice (mirrors the scene birther). On a base host this is always
/// `procedural` because that's the only catalog entry.
pub fn select_animator_for_identity(identity: &str) -> Box<dyn Animator> {
    let catalog = animator_catalog();
    let choice = deterministic_pick(identity, &catalog, "animator");
    build_animator(choice)
}

/// The supervisor. Runs in `AnimationSet::Intent` (before `Pose`): it decides,
/// per slot, whether the built-ins compute (procedural) or an external pose is
/// applied (VLA). See the module doc.
pub(in crate::live::video::bevy_renderer) fn drive_animators(
    time: Res<Time>,
    mut registry: ResMut<AnimatorRegistry>,
    mut commands: Commands,
    query: Query<(
        Entity,
        &SlotId,
        Has<Speaking>,
        Option<&EmotionAnimation>,
        Option<&GestureAnimation>,
        Option<&CognitiveGesture>,
        Option<&ExternalPose>,
    )>,
) {
    let time_secs = time.elapsed_secs();

    for (entity, slot_id, speaking, emotion, gesture, cognitive, external) in &query {
        let slot = slot_id.0;
        let Some(animator) = registry.get_mut(slot) else {
            continue; // no animator selected for this slot yet
        };

        // Health first: an unhealthy animator is detached (recovery, not fallback).
        if let AnimatorHealth::Unhealthy(cause) = animator.health() {
            clog_warn!(
                "animator `{}` on slot {slot} unhealthy ({cause}); reverting to procedural",
                animator.id()
            );
            registry.insert(slot, Box::new(ProceduralAnimator::new()));
            if external.is_some() {
                commands.entity(entity).remove::<ExternalPose>();
            }
            commands.entity(entity).insert(PoseSource::Procedural);
            continue;
        }

        let intent = MotionIntent {
            speaking,
            emotion: emotion.map(|e| e.current).unwrap_or(Emotion::Neutral),
            emotion_weight: emotion.map(|e| e.current_weight).unwrap_or(0.0),
            gesture: gesture.map(|g| g.gesture).unwrap_or(Gesture::None),
            cognitive: cognitive.map(|c| c.state),
        };

        // `SlotRng` is lent, not owned — seeded per (time, slot) so procedural
        // determinism is byte-identical across replays.
        let mut rng = SlotRng::new(time_secs, slot);
        let ctx = AnimatorContext {
            time_secs,
            slot,
            intent: &intent,
            pov_frame: None, // wired in Slice 3 (readback → animator)
            utterance: None,
        };

        match animator.animate(ctx, &mut rng) {
            AnimatorOutput::Builtin => {
                // Procedural path: the 8 gated systems compute. Only touch the
                // entity if it was previously external (clean revert), else no-op.
                if external.is_some() {
                    commands.entity(entity).remove::<ExternalPose>();
                    commands.entity(entity).insert(PoseSource::Procedural);
                }
            }
            AnimatorOutput::Pose(pose) => {
                let generation = external.map(|e| e.generation + 1).unwrap_or(0);
                commands
                    .entity(entity)
                    .insert(ExternalPose { pose, generation })
                    .insert(PoseSource::External);
            }
            AnimatorOutput::Pending => {
                // Hold the last applied pose — honest "thinking", never invent.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the base-host catalog admitting anything but procedural
    // (which would let a plain Load birth a VLA animator with no transport) and
    // selection non-determinism (same identity must always pick the same animator).
    #[test]
    fn base_host_always_selects_procedural_deterministically() {
        for id in ["asha", "sakurada", "user-123", ""] {
            let a = select_animator_for_identity(id);
            assert_eq!(
                a.id(),
                ProceduralAnimator::ID,
                "base host must birth procedural for `{id}`"
            );
        }
    }

    // what this catches: registry insert/get_mut/remove drifting — the supervisor
    // relies on these to own and drive animators per slot.
    #[test]
    fn registry_owns_animators_per_slot() {
        let mut reg = AnimatorRegistry::default();
        assert!(reg.is_empty());
        reg.insert(3, Box::new(ProceduralAnimator::new()));
        assert_eq!(reg.len(), 1);
        assert!(reg.get_mut(3).is_some());
        assert!(reg.get_mut(9).is_none());
        assert!(reg.remove(3).is_some());
        assert!(reg.is_empty());
    }
}
