//! Stats and monitoring systems — memory stats, idle cadence, load state tracking, light visibility.

use bevy::asset::LoadState;
use bevy::camera::visibility::SetViewVisibility;
use bevy::prelude::*;

use super::scene::SceneLight;
use super::types::*;
use super::{AVATAR_WIDTH, AVATAR_HEIGHT};
use crate::{clog_info, clog_warn};

/// Run condition: returns true when at least one slot is active.
pub(super) fn has_active_slots(registry: Res<SlotRegistry>) -> bool {
    static LOG_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let active = registry.slots.values().any(|s| s.is_active());
    let count = LOG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if count % 300 == 0 {
        let total = registry.slots.len();
        let active_count = registry.slots.values().filter(|s| s.is_active()).count();
        clog_info!("🎨 has_active_slots={} ({}/{} slots active, frame {})", active, active_count, total, count);
    }
    active
}

pub(super) fn update_memory_stats(
    registry: Res<SlotRegistry>,
    pending: Res<PendingLoads>,
    speaking_query: Query<&AvatarSlotId, With<Speaking>>,
    stats: Res<SharedMemoryStats>,
    slot_dims: Res<SlotDimensions>,
) {
    let mut active = 0u8;
    let mut loaded = 0u8;
    let mut rt_bytes = 0u64;

    for (slot_id, slot_data) in &registry.slots {
        if slot_data.scene_root.is_some() {
            active += 1;
        }
        if slot_data.is_active() {
            loaded += 1;
        }
        let (w, h) = slot_dims
            .dims
            .get(slot_id)
            .copied()
            .unwrap_or((AVATAR_WIDTH, AVATAR_HEIGHT));
        rt_bytes += (w as u64) * (h as u64) * 4;
    }

    let speaking = speaking_query.iter().count() as u8;
    let pending_count = pending.gltf_handles.len() + pending.scene_handles.len();

    stats.0.active_slots.store(active, std::sync::atomic::Ordering::Relaxed);
    stats.0.loaded_models.store(loaded, std::sync::atomic::Ordering::Relaxed);
    stats.0.speaking_slots.store(speaking, std::sync::atomic::Ordering::Relaxed);
    stats.0.render_target_bytes.store(rt_bytes, std::sync::atomic::Ordering::Relaxed);
    stats.0.pending_loads.store(pending_count as u32, std::sync::atomic::Ordering::Relaxed);
}

/// Sync idle cadence from the shared atomic (written by MemoryReporter under pressure).
pub(super) fn sync_idle_cadence(stats: Res<SharedMemoryStats>, mut schedule: ResMut<RenderSchedule>) {
    let desired = stats.0.desired_idle_cadence.load(std::sync::atomic::Ordering::Relaxed).max(1);
    if schedule.idle_cadence != desired {
        clog_info!("🎨 Idle cadence {} → {}", schedule.idle_cadence, desired);
        schedule.idle_cadence = desired;
    }
}

pub(super) fn force_light_visibility(
    mut lights: Query<&mut bevy::camera::visibility::ViewVisibility, With<SceneLight>>,
) {
    for mut vis in &mut lights {
        vis.set_visible();
    }
}

pub(super) fn monitor_load_states(
    asset_server: Res<AssetServer>,
    mut pending: ResMut<PendingLoads>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
) {
    static CWD_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !CWD_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        if let Ok(cwd) = std::env::current_dir() {
            clog_info!("🎨 Asset server cwd: {:?}", cwd);
        }
        let test_path = "models/avatars/vroid-female-base.glb";
        clog_info!(
            "🎨 File check '{}': exists={}",
            test_path,
            std::path::Path::new(test_path).exists()
        );
    }

    for entry in pending.gltf_handles.iter_mut() {
        if entry.logged_final {
            continue;
        }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                if let Some(gltf) = gltf_assets.get(entry.handle.id()) {
                    let named: Vec<&Box<str>> = gltf.named_scenes.keys().collect();
                    clog_info!(
                        "🎨 Gltf LOADED slot {}: {} — {} scenes, named: {:?}",
                        entry.slot,
                        entry.path,
                        gltf.scenes.len(),
                        named,
                    );
                } else {
                    clog_info!(
                        "🎨 Gltf LOADED slot {}: {} (not yet in Assets<Gltf>)",
                        entry.slot,
                        entry.path
                    );
                }
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                clog_warn!(
                    "🎨 Gltf FAILED slot {}: {} — error: {:?}",
                    entry.slot,
                    entry.path,
                    err
                );
                entry.logged_final = true;
            }
            _ => {}
        }
    }

    for entry in pending.scene_handles.iter_mut() {
        if entry.logged_final {
            continue;
        }
        match asset_server.load_state(entry.handle.id()) {
            LoadState::Loaded => {
                clog_info!("🎨 Scene LOADED slot {}: {}", entry.slot, entry.path);
                entry.logged_final = true;
            }
            LoadState::Failed(ref err) => {
                clog_warn!(
                    "🎨 Scene FAILED slot {}: {} — error: {:?}",
                    entry.slot,
                    entry.path,
                    err
                );
                entry.logged_final = true;
            }
            _ => {}
        }
    }

    pending.gltf_handles.retain(|e| !e.logged_final);
    pending.scene_handles.retain(|e| !e.logged_final);
}
