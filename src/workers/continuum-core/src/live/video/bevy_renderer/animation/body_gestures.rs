//! Body gesture animation — cognitive gesture driver + arm/shoulder bone poses.

use bevy::prelude::*;

use super::components::*;
use super::super::scene::animation::{AnimationConfig, PORTRAIT_PROFILE};

/// Cognitive gesture driver — selects and triggers gestures from cognitive state.
pub(in crate::live::video::bevy_renderer) fn drive_cognitive_gestures(
    time: Res<Time>,
    mut query: Query<(Entity, &mut CognitiveGesture, Has<Speaking>, Has<GestureAnimation>)>,
    mut commands: Commands,
) {
    use crate::live::session::cognitive_animation::{select_weighted_gesture, CognitiveState};

    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();

    for (entity, mut cog, is_speaking, has_gesture) in &mut query {
        cog.time_since_reroll += dt;

        if is_speaking || has_gesture {
            continue;
        }

        if cog.time_since_reroll < cog.config.reroll_interval_secs {
            continue;
        }

        cog.time_since_reroll = 0.0;

        let table = match cog.state {
            CognitiveState::Evaluating => &cog.config.evaluating,
            CognitiveState::Generating => &cog.config.generating,
            CognitiveState::Idle => continue,
        };

        if let Some((gesture, duration_ms)) = select_weighted_gesture(table, elapsed, 0) {
            if gesture != crate::live::video::bevy_renderer::Gesture::None {
                // Map from the public Gesture enum to our component Gesture enum
                let g = match gesture {
                    crate::live::video::bevy_renderer::Gesture::Wave => Gesture::Wave,
                    crate::live::video::bevy_renderer::Gesture::Think => Gesture::Think,
                    crate::live::video::bevy_renderer::Gesture::Nod => Gesture::Nod,
                    crate::live::video::bevy_renderer::Gesture::Shrug => Gesture::Shrug,
                    crate::live::video::bevy_renderer::Gesture::Point => Gesture::Point,
                    crate::live::video::bevy_renderer::Gesture::OpenHands => Gesture::OpenHands,
                    crate::live::video::bevy_renderer::Gesture::None => Gesture::None,
                };
                commands.entity(entity).insert(GestureAnimation {
                    gesture: g,
                    phase: GesturePhase::Attack,
                    duration_secs: duration_ms as f32 / 1000.0,
                    elapsed: 0.0,
                    weight: 0.0,
                });
            }
        }
    }
}

/// Body gesture animation — drives bones through gesture poses.
pub(in crate::live::video::bevy_renderer) fn animate_body_gestures(
    time: Res<Time>,
    mut query: Query<(Entity, &mut GestureAnimation, &Skeleton, Option<&AnimationConfig>)>,
    mut transforms: Query<&mut Transform>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();

    for (entity, mut anim, skeleton, anim_cfg) in &mut query {
        anim.elapsed += dt;

        let attack_end = GESTURE_EASE_SECS;
        let sustain_end = anim.duration_secs - GESTURE_EASE_SECS;
        let total_end = anim.duration_secs;

        if anim.elapsed >= total_end {
            commands.entity(entity).remove::<GestureAnimation>();
            continue;
        }

        anim.weight = if anim.elapsed < attack_end {
            anim.phase = GesturePhase::Attack;
            smoothstep(anim.elapsed / GESTURE_EASE_SECS)
        } else if anim.elapsed < sustain_end {
            anim.phase = GesturePhase::Sustain;
            1.0
        } else {
            anim.phase = GesturePhase::Release;
            let release_progress = (anim.elapsed - sustain_end) / GESTURE_EASE_SECS;
            1.0 - smoothstep(release_progress)
        };

        let profile = anim_cfg
            .map(|c| &c.profile)
            .unwrap_or(&PORTRAIT_PROFILE);

        let w = anim.weight;
        let t = now;

        match anim.gesture {
            Gesture::Wave => {
                if let Some(ref rua) = skeleton.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let up_angle = -1.2 * w;
                        transform.rotation = rua.rest_rotation * Quat::from_rotation_z(up_angle);
                    }
                }
                if let Some(ref rla) = skeleton.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        let wave = if anim.phase == GesturePhase::Sustain {
                            (t * 2.0 * std::f32::consts::TAU).sin() * 0.35
                        } else {
                            0.0
                        };
                        let bend = (-0.5 + wave) * w;
                        transform.rotation = rla.rest_rotation * Quat::from_rotation_z(bend);
                    }
                }
            }
            Gesture::Think => {
                if let Some(ref rua) = skeleton.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let forward = Quat::from_rotation_x(-0.8 * w);
                        let inward = Quat::from_rotation_z(-0.3 * w);
                        transform.rotation = rua.rest_rotation * forward * inward;
                    }
                }
                if let Some(ref rla) = skeleton.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        transform.rotation = rla.rest_rotation * Quat::from_rotation_z(2.0 * w);
                    }
                }
                if let Some(ref head) = skeleton.head {
                    if let Ok(mut transform) = transforms.get_mut(head.entity) {
                        let tilt = Quat::from_euler(
                            EulerRot::XYZ,
                            profile.gesture_think_head_tilt * w,
                            0.0,
                            profile.gesture_think_head_roll * w,
                        );
                        transform.rotation = head.rest_rotation * tilt;
                    }
                }
            }
            Gesture::Nod => {
                if let Some(ref head) = skeleton.head {
                    if let Ok(mut transform) = transforms.get_mut(head.entity) {
                        let nod = (t * 1.5 * std::f32::consts::TAU).sin() * profile.gesture_nod_amplitude * w;
                        transform.rotation = head.rest_rotation * Quat::from_rotation_x(nod);
                    }
                }
            }
            Gesture::Shrug => {
                if let Some(ref ls) = skeleton.left_shoulder {
                    if let Ok(mut transform) = transforms.get_mut(ls.entity) {
                        transform.translation.y = ls.rest_translation.y + 0.01 * w;
                    }
                }
                if let Some(ref rs) = skeleton.right_shoulder {
                    if let Ok(mut transform) = transforms.get_mut(rs.entity) {
                        transform.translation.y = rs.rest_translation.y + 0.01 * w;
                    }
                }
                if let Some(ref lua) = skeleton.left_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(lua.entity) {
                        transform.rotation = lua.rest_rotation * Quat::from_rotation_z(-0.35 * w);
                    }
                }
                if let Some(ref rua) = skeleton.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        transform.rotation = rua.rest_rotation * Quat::from_rotation_z(0.35 * w);
                    }
                }
            }
            Gesture::Point => {
                if let Some(ref rua) = skeleton.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        transform.rotation = rua.rest_rotation * Quat::from_rotation_x(-1.05 * w);
                    }
                }
                if let Some(ref rla) = skeleton.right_lower_arm {
                    if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                        transform.rotation = rla.rest_rotation * Quat::from_rotation_z(0.26 * w);
                    }
                }
            }
            Gesture::OpenHands => {
                if let Some(ref lua) = skeleton.left_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(lua.entity) {
                        let out = Quat::from_rotation_z(-0.4 * w);
                        let forward = Quat::from_rotation_x(-0.3 * w);
                        transform.rotation = lua.rest_rotation * forward * out;
                    }
                }
                if let Some(ref rua) = skeleton.right_upper_arm {
                    if let Ok(mut transform) = transforms.get_mut(rua.entity) {
                        let out = Quat::from_rotation_z(0.4 * w);
                        let forward = Quat::from_rotation_x(-0.3 * w);
                        transform.rotation = rua.rest_rotation * forward * out;
                    }
                }
                if anim.phase == GesturePhase::Sustain {
                    if let Some(ref lla) = skeleton.left_lower_arm {
                        if let Ok(mut transform) = transforms.get_mut(lla.entity) {
                            let osc = (t * 0.5).sin() * 0.05 * w;
                            transform.rotation = lla.rest_rotation * Quat::from_rotation_x(osc);
                        }
                    }
                    if let Some(ref rla) = skeleton.right_lower_arm {
                        if let Ok(mut transform) = transforms.get_mut(rla.entity) {
                            let osc = (t * 0.5 + 0.5).sin() * 0.05 * w;
                            transform.rotation = rla.rest_rotation * Quat::from_rotation_x(osc);
                        }
                    }
                }
            }
            Gesture::None => {}
        }
    }
}
