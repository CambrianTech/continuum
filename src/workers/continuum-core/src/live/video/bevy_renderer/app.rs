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
        )))
        ;
        // GPU bridge: Metal compute shader for zero-copy RGBA→NV12 on macOS.
        // Non-macOS uses CPU readback path (ReadbackComplete observer).
        #[cfg(target_os = "macos")]
        app.add_plugins(super::super::metal_gpu_convert::GpuConvertPlugin);
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
        // Animation systems — Query-driven, no run_if gate needed.
        // Each system only runs if matching entities exist (Query is empty = no-op).
        .add_systems(
            Update,
            (
                animation::manage_render_cadence,
                ensure_continuous_readback,
                request_snapshot_readback,
                animation::discover_morph_targets,
                animation::animate_idle,
                animation::animate_speaking,
                animation::animate_expression,
                animation::animate_blinking,
                animation::animate_breathing,
                animation::animate_idle_gestures,
                animation::animate_eye_gaze,
                animation::drive_cognitive_gestures,
                animation::animate_body_gestures,
            ),
        )
        .add_systems(
            PostUpdate,
            force_light_visibility
                .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
        )
        .run();
}
