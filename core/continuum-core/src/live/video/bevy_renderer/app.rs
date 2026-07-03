//! Bevy app setup and system registration.

use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use crossbeam_channel::Sender;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use super::animation;
use super::commands::process_commands;
use super::readback::{ensure_continuous_readback, request_snapshot_readback};
use super::scene;
use super::setup::{setup_render_slots, signal_ready};
use super::stats::{
    force_light_visibility, has_active_slots, monitor_load_states, sync_idle_cadence,
    update_memory_stats,
};
use super::types::*;
use super::AVATAR_FPS;
use crate::live::avatar::RgbaFrame;

pub(super) fn run_bevy_app(
    command_rx: crossbeam_channel::Receiver<AvatarCommand>,
    frame_senders: Vec<Sender<RgbaFrame>>,
    frame_notifiers: Vec<Arc<tokio::sync::Notify>>,
    ready_flag: Arc<std::sync::atomic::AtomicBool>,
    memory_stats: Arc<BevyMemoryStats>,
) {
    let asset_base = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut app = App::new();
    app.insert_resource(CommandChannel(command_rx))
        .insert_resource(FrameChannels(frame_senders))
        .insert_resource(FrameNotifiers(frame_notifiers))
        .insert_resource(ReadyFlag(ready_flag))
        .insert_resource(SlotRegistry {
            slots: HashMap::new(),
        })
        .insert_resource(PendingLoads::default())
        .insert_resource(ActiveSpeechClips::default())
        .insert_resource(SlotDimensions::default())
        .insert_resource(SlotHealthStatus::default())
        .insert_resource(SnapshotTracker::new())
        .insert_resource(RenderSchedule::default())
        .insert_resource(GpuGuards::default())
        .insert_resource(scene::physics::PhysicsBackendRegistry::default())
        .insert_resource(animation::AnimatorRegistry::default())
        .insert_resource(SharedMemoryStats(memory_stats))
        .add_plugins(
            DefaultPlugins
                .set(bevy::window::WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .set(ImagePlugin::default_nearest())
                .set(bevy::asset::AssetPlugin {
                    file_path: asset_base,
                    ..default()
                }),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
            1.0 / AVATAR_FPS,
        )));
    // GPU bridge: Metal compute shader for zero-copy RGBA→NV12 on macOS.
    #[cfg(all(feature = "livekit-webrtc", target_os = "macos"))]
    app.add_plugins(super::super::metal_gpu_convert::GpuConvertPlugin);
    // wgpu compute shader for GPU RGBA→I420 conversion (cross-platform: Vulkan/DX12/Metal).
    // On macOS this is a fallback behind the Metal IOSurface path.
    // On Windows/Linux this is the PRIMARY GPU path.
    app.add_plugins(super::super::wgpu_gpu_convert::WgpuGpuConvertPlugin);
    app.register_type::<bevy::transform::components::TransformTreeChanged>()
        .add_systems(Startup, (setup_render_slots, signal_ready).chain())
        .add_systems(
            Update,
            (
                process_commands,
                monitor_load_states,
                update_memory_stats,
                sync_idle_cadence,
                scene::room::populate_rooms,
            ),
        )
        // Animation phases run in a total order: Intent (supervisor decides pose
        // vs built-in, morph discovery, cadence) → Pose (built-in writers +
        // apply_external_pose write Transforms/morphs) → Readback (capture the
        // rendered frame). Parallelism is preserved *within* each phase; the
        // `.chain()` only orders the phases. This replaces the old unordered
        // tuple, whose last-writer-wins only worked because the built-in writers
        // happened to touch disjoint bones.
        .configure_sets(
            Update,
            (
                animation::AnimationSet::Intent,
                animation::AnimationSet::Pose,
                animation::AnimationSet::Readback,
            )
                .chain(),
        )
        // Intent: decide per-slot ownership + prepare inputs the writers need.
        .add_systems(
            Update,
            (
                animation::drive_animators,
                animation::drive_cognitive_gestures,
                animation::discover_morph_targets,
                animation::manage_render_cadence,
            )
                .in_set(animation::AnimationSet::Intent),
        )
        // Pose: built-in writers (each gated `Without<ExternalPose>`) compute the
        // procedural set; `apply_external_pose` drives the VLA-owned set. Disjoint
        // entity sets, so they never double-write. Query-driven — empty = no-op.
        .add_systems(
            Update,
            (
                animation::animate_idle,
                animation::animate_speaking,
                animation::animate_expression,
                animation::animate_blinking,
                animation::animate_breathing,
                animation::animate_idle_gestures,
                animation::animate_eye_gaze,
                animation::animate_body_gestures,
                animation::apply_external_pose,
            )
                .in_set(animation::AnimationSet::Pose),
        )
        // Readback: capture the posed frame for streaming / snapshots.
        .add_systems(
            Update,
            (ensure_continuous_readback, request_snapshot_readback)
                .in_set(animation::AnimationSet::Readback),
        )
        .add_systems(
            PostUpdate,
            force_light_visibility
                .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        )
        .run();
}
