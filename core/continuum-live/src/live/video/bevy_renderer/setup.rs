//! Setup systems — render slot allocation, signal ready, readback entity spawning.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::gpu_readback::{Readback, ReadbackComplete};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use std::collections::HashMap;

use super::api::gpu_manager;
use super::scene;
use super::types::*;
use super::{AVATAR_HEIGHT, AVATAR_WIDTH, HD_HEIGHT, HD_WIDTH, MAX_AVATAR_SLOTS, MAX_HD_SLOTS};
use crate::gpu::make_entry;
use crate::gpu::memory_manager::{GpuPriority, GpuSubsystem};
use crate::live::avatar::RgbaFrame;
use crate::{clog_info, clog_warn};

pub(super) fn setup_render_slots(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut registry: ResMut<SlotRegistry>,
    frame_channels: Res<FrameChannels>,
) {
    // Ambient light — low baseline so nothing is pure black.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 800.0,
        affects_lightmapped_meshes: false,
    });

    // Global 3-point portrait lights — shared across all avatar scenes.
    scene::spawn_global_lights(&mut commands, MAX_AVATAR_SLOTS);

    for slot in 0..MAX_AVATAR_SLOTS {
        let size = Extent3d {
            width: AVATAR_WIDTH,
            height: AVATAR_HEIGHT,
            depth_or_array_layers: 1,
        };
        let mut rt_image = Image::new_fill(
            size,
            TextureDimension::D2,
            &[26, 26, 46, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        rt_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::COPY_DST
            | TextureUsages::TEXTURE_BINDING;
        let rt_handle = images.add(rt_image);

        let readback_entity =
            spawn_readback_entity_opt(&mut commands, rt_handle.clone(), slot, false);

        // Register slot for wgpu GPU compute (RGBA→I420 on GPU).
        // On macOS, Metal IOSurface bridge takes priority (readback.rs handles exclusion).
        // On non-macOS (Windows/Linux), this is the PRIMARY GPU path.
        #[cfg(not(target_os = "macos"))]
        if let Some(sender) = frame_channels.0.get(slot as usize) {
            crate::live::video::wgpu_gpu_convert::register_slot(
                slot,
                rt_handle.id(),
                sender.clone(),
                AVATAR_WIDTH,
                AVATAR_HEIGHT,
            );
        }

        registry.slots.insert(
            slot,
            scene::RenderSlot::new(slot, readback_entity, rt_handle),
        );
    }

    // Pre-allocate HD render target pool
    let mut hd_targets = Vec::with_capacity(MAX_HD_SLOTS);
    for _ in 0..MAX_HD_SLOTS {
        let hd_size = Extent3d {
            width: HD_WIDTH,
            height: HD_HEIGHT,
            depth_or_array_layers: 1,
        };
        let mut hd_image = Image::new_fill(
            hd_size,
            TextureDimension::D2,
            &[26, 26, 46, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        hd_image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::COPY_SRC
            | TextureUsages::COPY_DST
            | TextureUsages::TEXTURE_BINDING;
        hd_targets.push(images.add(hd_image));
    }
    commands.insert_resource(HdRenderTargetPool {
        available: hd_targets,
        assigned: HashMap::new(),
    });

    // Track total render target VRAM allocation
    let lowres_bytes = MAX_AVATAR_SLOTS as u64 * AVATAR_WIDTH as u64 * AVATAR_HEIGHT as u64 * 4;
    let hd_bytes = MAX_HD_SLOTS as u64 * HD_WIDTH as u64 * HD_HEIGHT as u64 * 4;
    let total_rt_bytes = lowres_bytes + hd_bytes;
    if let Some(mgr) = gpu_manager() {
        match mgr.allocate(
            GpuSubsystem::Rendering,
            total_rt_bytes,
            GpuPriority::Realtime,
        ) {
            Ok(guard) => {
                mgr.eviction_registry.register(make_entry(
                    "render:targets",
                    "Render Targets (pre-allocated)",
                    GpuPriority::Realtime,
                    total_rt_bytes,
                ));
                commands.insert_resource(GpuGuards {
                    _render_targets: Some(guard),
                    model_guards: HashMap::new(),
                });
                clog_info!(
                    "🎨 GPU: allocated {:.1}MB for render targets",
                    total_rt_bytes as f64 / 1_048_576.0
                );
            }
            Err(e) => {
                clog_warn!(
                    "🎨 GPU: render target allocation failed ({}), proceeding untracked",
                    e
                );
            }
        }
    }

    clog_info!(
        "🎨 Bevy renderer ready: {} slots x {}x{} @{}fps ({} HD targets pooled at {}x{})",
        MAX_AVATAR_SLOTS,
        AVATAR_WIDTH,
        AVATAR_HEIGHT,
        super::AVATAR_FPS,
        MAX_HD_SLOTS,
        HD_WIDTH,
        HD_HEIGHT
    );
}

pub(super) fn signal_ready(flag: Res<ReadyFlag>) {
    flag.0.store(true, std::sync::atomic::Ordering::Release);
}

pub(super) fn spawn_readback_entity_opt(
    commands: &mut Commands,
    rt_handle: Handle<Image>,
    slot_id: u8,
    start_active: bool,
) -> Entity {
    let mut entity_cmds = if start_active {
        commands.spawn((
            Readback::texture(rt_handle),
            AvatarSlotId(slot_id),
            ReadbackMarker,
        ))
    } else {
        commands.spawn((AvatarSlotId(slot_id), ReadbackMarker))
    };
    entity_cmds
        .observe(
            move |event: On<ReadbackComplete>,
                  channels: Res<FrameChannels>,
                  notifiers: Res<FrameNotifiers>,
                  health: Res<SlotHealthStatus>,
                  mut snapshots: ResMut<SnapshotTracker>,
                  slot_dims: Res<SlotDimensions>| {
                let pixel_bytes: &[u8] = &event.data;

                let (slot_w, slot_h) = slot_dims.dims
                    .get(&slot_id)
                    .copied()
                    .unwrap_or((AVATAR_WIDTH, AVATAR_HEIGHT));

                static FIRST_READBACK: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
                #[allow(clippy::declare_interior_mutable_const)]
                const ATOMIC_ZERO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                #[allow(clippy::borrow_interior_mutable_const)]
                static FRAME_COUNTER: [std::sync::atomic::AtomicU32; 16] = [ATOMIC_ZERO; 16];
                let mask = 1u16 << slot_id;
                let prev = FIRST_READBACK.fetch_or(mask, std::sync::atomic::Ordering::Relaxed);
                if prev & mask == 0 {
                    clog_info!("🎨 Slot {}: first ReadbackComplete ({} bytes, {}x{})", slot_id, pixel_bytes.len(), slot_w, slot_h);
                }

                let frame_n = FRAME_COUNTER[slot_id as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Skip early frames — GPU readback often returns corrupted data
                if frame_n < 30 {
                    return;
                }

                if frame_n == 150 || frame_n == 300 {
                    let test_frame = RgbaFrame {
                        width: slot_w,
                        height: slot_h,
                        data: pixel_bytes.to_vec(),
                    };
                    let analysis = crate::live::avatar::frame_analysis::analyze(&test_frame);
                    let verdict = analysis.verdict();
                    match verdict {
                        crate::live::avatar::HealthVerdict::Healthy => {
                            clog_info!("🎨 Slot {} frame {}: HEALTHY — coverage={:.0}%, colors={}, roughness={:.1}, symmetry={:.2}",
                                slot_id, frame_n, analysis.coverage * 100.0, analysis.color_diversity,
                                analysis.edge_roughness, analysis.symmetry);
                        }
                        _ => {
                            clog_warn!("🎨 Slot {} frame {}: {:?} — coverage={:.0}%, colors={}, roughness={:.1}, white={:.0}%, symmetry={:.2}",
                                slot_id, frame_n, verdict, analysis.coverage * 100.0, analysis.color_diversity,
                                analysis.edge_roughness, analysis.white_ratio * 100.0, analysis.symmetry);
                            if let Some(model_path) = health.model_paths.get(&slot_id) {
                                clog_warn!("🎨 Slot {}: model '{}' rendered unhealthy", slot_id, model_path);
                            }
                        }
                    }
                }

                // Opportunistic snapshot
                if let Some(identity) = health.identities.get(&slot_id) {
                    if snapshots.needs_capture(slot_id, identity) {
                        snapshots.capture_background(
                            slot_id,
                            identity.clone(),
                            slot_w,
                            slot_h,
                            pixel_bytes.to_vec(),
                        );
                    }
                }

                if let Some(tx) = channels.0.get(slot_id as usize) {
                    match tx.try_send(RgbaFrame {
                        width: slot_w,
                        height: slot_h,
                        data: pixel_bytes.to_vec(),
                    }) {
                        Ok(()) => {
                            if let Some(notify) = notifiers.0.get(slot_id as usize) {
                                notify.notify_one();
                            }
                        }
                        Err(crossbeam_channel::TrySendError::Full(_)) => {
                            #[allow(clippy::declare_interior_mutable_const)]
                            const DROP_ZERO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                            #[allow(clippy::borrow_interior_mutable_const)]
                            static DROP_COUNTS: [std::sync::atomic::AtomicU32; 16] = [DROP_ZERO; 16];
                            let count = DROP_COUNTS[slot_id as usize].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if count.is_multiple_of(150) {
                                clog_warn!("🎨 Slot {}: {} frames dropped (channel full)", slot_id, count + 1);
                            }
                        }
                        Err(crossbeam_channel::TrySendError::Disconnected(_)) => {}
                    }
                }
            },
        )
        .id()
}
