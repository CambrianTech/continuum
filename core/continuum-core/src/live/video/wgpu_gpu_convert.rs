//! wgpu GPU RGBA→I420 Compute Pipeline + Bevy Render World Plugin
//!
//! Cross-platform GPU color space conversion using wgpu compute shaders (WGSL).
//! Works on Vulkan (Linux/Windows), DX12 (Windows), and Metal (macOS).
//!
//! Architecture:
//!   Bevy renders RGBA → wgpu Texture (GPU memory)
//!   Render-world system: wgpu compute RGBA texture → I420 storage buffers (GPU memory)
//!   Buffer map: GPU staging buffer → CPU memory (async, one-frame latency)
//!   Publisher: I420 CPU data → LiveKit NativeVideoSource
//!
//! Advantages over CPU path:
//!   - RGBA→I420 conversion runs on GPU (frees CPU for inference/TTS)
//!   - I420 readback is 62% smaller than RGBA (1.5 bytes/pixel vs 4)
//!   - One frame latency (read frame N-1 while computing frame N)
//!
//! Advantages over Metal-only path:
//!   - Works on ALL platforms (Vulkan, DX12, Metal)
//!   - Single WGSL shader, no platform-specific FFI

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderSystems};

use crossbeam_channel::Sender;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::clog_info;
#[allow(unused_imports)]
use crate::clog_warn;
use crate::live::avatar::frame::RgbaFrame;

// =============================================================================
// WGSL Compute Shader — RGBA texture → I420 (Y, U, V separate buffers)
// =============================================================================

const RGBA_TO_I420_SHADER: &str = r#"
@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> y_plane: array<u32>;
@group(0) @binding(2) var<storage, read_write> u_plane: array<u32>;
@group(0) @binding(3) var<storage, read_write> v_plane: array<u32>;
@group(0) @binding(4) var<uniform> dims: vec2<u32>;

// BT.601 fixed-point (matches CPU path for pixel-perfect equivalence)
fn rgb_to_y(r: f32, g: f32, b: f32) -> u32 {
    let ri = u32(r * 255.0);
    let gi = u32(g * 255.0);
    let bi = u32(b * 255.0);
    return clamp(((66u * ri + 129u * gi + 25u * bi + 128u) >> 8u) + 16u, 0u, 255u);
}

fn rgb_to_u(r: f32, g: f32, b: f32) -> u32 {
    let ri = i32(r * 255.0);
    let gi = i32(g * 255.0);
    let bi = i32(b * 255.0);
    return u32(clamp(((-38 * ri - 74 * gi + 112 * bi + 128) >> 8) + 128, 0, 255));
}

fn rgb_to_v(r: f32, g: f32, b: f32) -> u32 {
    let ri = i32(r * 255.0);
    let gi = i32(g * 255.0);
    let bi = i32(b * 255.0);
    return u32(clamp(((112 * ri - 94 * gi - 18 * bi + 128) >> 8) + 128, 0, 255));
}

// Each thread processes 4 consecutive X pixels → packs 4 Y values into one u32.
// For UV: each thread on even rows packs a full u32 (4 UV bytes = 8 source pixels).
// Thread gid.x maps to UV word index directly — no read-modify-write race.
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = dims.x;
    let h = dims.y;
    let base_x = gid.x * 4u;
    let y_coord = gid.y;

    if (base_x >= w || y_coord >= h) {
        return;
    }

    // Y plane: pack 4 bytes into one u32
    var packed_y: u32 = 0u;
    for (var i: u32 = 0u; i < 4u; i = i + 1u) {
        let px = base_x + i;
        if (px >= w) { break; }
        let pixel = textureLoad(input_tex, vec2<i32>(i32(px), i32(y_coord)), 0);
        packed_y = packed_y | (rgb_to_y(pixel.r, pixel.g, pixel.b) << (i * 8u));
    }
    y_plane[(y_coord * w + base_x) / 4u] = packed_y;

    // UV planes: only even rows. Each thread packs a FULL u32 (4 UV bytes)
    // to avoid read-modify-write races between adjacent threads.
    // 4 UV bytes = 4 chroma columns = 8 source pixels in X.
    // So UV thread coverage = gid.x * 4 chroma cols = gid.x * 8 source pixels.
    if (y_coord % 2u == 0u) {
        let cw = (w + 1u) / 2u;
        let cy = y_coord / 2u;
        let uv_word_idx = (cy * cw + gid.x * 4u) / 4u;

        // Pack 4 UV bytes into one u32
        var packed_u: u32 = 0u;
        var packed_v: u32 = 0u;
        for (var i: u32 = 0u; i < 4u; i = i + 1u) {
            let cx = gid.x * 4u + i;
            if (cx >= cw) { break; }
            let px = cx * 2u;
            let pixel = textureLoad(input_tex, vec2<i32>(i32(px), i32(y_coord)), 0);
            packed_u = packed_u | (rgb_to_u(pixel.r, pixel.g, pixel.b) << (i * 8u));
            packed_v = packed_v | (rgb_to_v(pixel.r, pixel.g, pixel.b) << (i * 8u));
        }

        // Single write per thread — no race
        if (gid.x * 4u < cw) {
            u_plane[uv_word_idx] = packed_u;
            v_plane[uv_word_idx] = packed_v;
        }
    }
}
"#;

// =============================================================================
// Bevy Plugin
// =============================================================================

pub struct WgpuGpuConvertPlugin;

impl Plugin for WgpuGpuConvertPlugin {
    fn build(&self, app: &mut App) {
        let render_app = match app.get_sub_app_mut(RenderApp) {
            Some(ra) => ra,
            None => return,
        };

        render_app.init_resource::<WgpuConvertState>().add_systems(
            Render,
            dispatch_compute
                .in_set(RenderSystems::Cleanup)
                .after(bevy::render::view::prepare_windows),
        );
    }
}

// =============================================================================
// Render-world resources
// =============================================================================

struct SlotComputeState {
    y_buffer: wgpu::Buffer,
    u_buffer: wgpu::Buffer,
    v_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    i420_size: u64,
    y_size: u64,
    u_size: u64,
    frame_tx: Sender<RgbaFrame>,
    /// Whether we've submitted a map request and are waiting for it
    pending_map: bool,
    /// Set to true by the map_async callback when mapping completes
    map_ready: Arc<std::sync::atomic::AtomicBool>,
    frame_count: u64,
}

#[derive(Resource, Default)]
struct WgpuConvertState {
    pipeline: Option<wgpu::ComputePipeline>,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    slots: HashMap<u8, SlotComputeState>,
    pending_setup: Vec<(u8, bevy::asset::AssetId<Image>, Sender<RgbaFrame>, u32, u32)>,
}

// =============================================================================
// Compute dispatch
// =============================================================================

fn dispatch_compute(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    mut state: ResMut<WgpuConvertState>,
) {
    // Drain pending registrations from global bridge
    {
        let mut bridge = WGPU_BRIDGE.lock().unwrap();
        state.pending_setup.append(&mut bridge.pending);
    }

    if state.slots.is_empty() && state.pending_setup.is_empty() {
        return;
    }

    let device = render_device.wgpu_device();

    // Lazy-init pipeline
    if state.pipeline.is_none() {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rgba_to_i420_compute"),
            source: wgpu::ShaderSource::Wgsl(RGBA_TO_I420_SHADER.into()),
        });

        let storage_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rgba_to_i420_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                storage_entry(1), // Y plane
                storage_entry(2), // U plane
                storage_entry(3), // V plane
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rgba_to_i420_pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("rgba_to_i420_pipeline"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        state.bind_group_layout = Some(bgl);
        state.pipeline = Some(pipeline);
        clog_info!("📹 WgpuGpuConvert: compute pipeline created (cross-platform WGSL)");
    }

    // Clone pipeline/layout refs to avoid borrow conflicts
    let pipeline = state.pipeline.clone().unwrap();
    let bgl = state.bind_group_layout.clone().unwrap();

    // Setup pending slots
    let pending: Vec<_> = state.pending_setup.drain(..).collect();
    for (slot_id, image_id, frame_tx, width, height) in pending {
        let gpu_image = match gpu_images.get(image_id) {
            Some(img) => img,
            None => {
                state
                    .pending_setup
                    .push((slot_id, image_id, frame_tx, width, height));
                continue;
            }
        };

        let y_size = plane_size(width * height);
        let uv_w = (width + 1) / 2;
        let uv_h = (height + 1) / 2;
        let u_size = plane_size(uv_w * uv_h);
        let v_size = u_size;
        let i420_size = y_size + u_size + v_size;

        let mk_buf = |label: &str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };

        let stor = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;
        let y_buffer = mk_buf(&format!("y_slot_{slot_id}"), y_size, stor);
        let u_buffer = mk_buf(&format!("u_slot_{slot_id}"), u_size, stor);
        let v_buffer = mk_buf(&format!("v_slot_{slot_id}"), v_size, stor);
        let staging_buffer = mk_buf(
            &format!("staging_slot_{slot_id}"),
            i420_size,
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );

        // Dims uniform
        let dims_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("dims_slot_{slot_id}")),
            size: 8,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        {
            let mut view = dims_buffer.slice(..).get_mapped_range_mut();
            view[0..4].copy_from_slice(&width.to_ne_bytes());
            view[4..8].copy_from_slice(&height.to_ne_bytes());
        }
        dims_buffer.unmap();

        let tex_view = gpu_image
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("i420_bg_slot_{slot_id}")),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: y_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: u_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: v_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: dims_buffer.as_entire_binding(),
                },
            ],
        });

        clog_info!(
            "📹 WgpuGpuConvert: slot {} setup ({}×{}, I420 {}KB)",
            slot_id,
            width,
            height,
            i420_size / 1024
        );

        state.slots.insert(
            slot_id,
            SlotComputeState {
                y_buffer,
                u_buffer,
                v_buffer,
                staging_buffer,
                bind_group,
                width,
                height,
                i420_size,
                y_size,
                u_size,
                frame_tx,
                pending_map: false,
                map_ready: Arc::new(AtomicBool::new(false)),
                frame_count: 0,
            },
        );
    }

    if state.slots.is_empty() {
        return;
    }

    // Poll GPU for completed map operations
    let _ = device.poll(wgpu::PollType::Poll);

    // Read back previous frame's results
    for (_slot_id, s) in state.slots.iter_mut() {
        if !s.pending_map {
            continue;
        }

        // Check if the async map callback has fired
        if !s.map_ready.load(Ordering::Acquire) {
            continue; // Not ready yet
        }

        // Map is ready — read the data
        let mapped = s.staging_buffer.slice(..).get_mapped_range();
        let data = mapped.to_vec();
        drop(mapped);
        s.staging_buffer.unmap();
        s.pending_map = false;
        s.map_ready.store(false, Ordering::Release);

        let _ = s.frame_tx.try_send(RgbaFrame {
            width: s.width,
            height: s.height,
            data,
        });

        s.frame_count += 1;
        if s.frame_count == 1 || s.frame_count % 450 == 0 {
            clog_info!(
                "📹 WgpuGpuConvert: slot {} — {} frames ({}×{} GPU I420)",
                _slot_id,
                s.frame_count,
                s.width,
                s.height
            );
        }
    }

    // Dispatch compute + copy to staging
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("i420_compute"),
    });

    let mut any = false;

    for (_slot_id, s) in state.slots.iter_mut() {
        if s.pending_map {
            continue;
        }

        // Clear plane buffers
        encoder.clear_buffer(&s.y_buffer, 0, None);
        encoder.clear_buffer(&s.u_buffer, 0, None);
        encoder.clear_buffer(&s.v_buffer, 0, None);

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &s.bind_group, &[]);
            // Each thread processes 4 X pixels, so divide width by 4 for dispatch
            let wg_x = ((s.width + 3) / 4 + 7) / 8;
            let wg_y = (s.height + 7) / 8;
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        // Copy Y, U, V to staging (contiguous: Y then U then V)
        encoder.copy_buffer_to_buffer(&s.y_buffer, 0, &s.staging_buffer, 0, s.y_size);
        encoder.copy_buffer_to_buffer(&s.u_buffer, 0, &s.staging_buffer, s.y_size, s.u_size);
        encoder.copy_buffer_to_buffer(
            &s.v_buffer,
            0,
            &s.staging_buffer,
            s.y_size + s.u_size,
            s.u_size,
        );

        any = true;
    }

    if !any {
        return;
    }

    render_queue.submit(std::iter::once(encoder.finish()));

    // Request async map for next frame
    for (_slot_id, s) in state.slots.iter_mut() {
        if !s.pending_map {
            let ready_flag = s.map_ready.clone();
            s.staging_buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    if result.is_ok() {
                        ready_flag.store(true, Ordering::Release);
                    }
                });
            s.pending_map = true;
        }
    }
}

// =============================================================================
// Public API — slot registration
// =============================================================================

static WGPU_BRIDGE: std::sync::LazyLock<std::sync::Mutex<WgpuBridgeRegistry>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(WgpuBridgeRegistry::default()));

#[derive(Default)]
struct WgpuBridgeRegistry {
    pending: Vec<(u8, bevy::asset::AssetId<Image>, Sender<RgbaFrame>, u32, u32)>,
    active_slots: std::collections::HashSet<u8>,
}

/// Register a slot for GPU-accelerated I420 conversion.
pub fn register_slot(
    slot_id: u8,
    render_target: bevy::asset::AssetId<Image>,
    frame_tx: Sender<RgbaFrame>,
    width: u32,
    height: u32,
) {
    let mut bridge = WGPU_BRIDGE.lock().unwrap();
    bridge
        .pending
        .push((slot_id, render_target, frame_tx, width, height));
    bridge.active_slots.insert(slot_id);
    clog_info!(
        "📹 WgpuBridge: slot {} registered for GPU I420 ({}×{})",
        slot_id,
        width,
        height
    );
}

/// Check if a slot has a wgpu GPU bridge active.
pub fn has_bridge(slot_id: u8) -> bool {
    WGPU_BRIDGE.lock().unwrap().active_slots.contains(&slot_id)
}

/// Unregister a slot.
pub fn unregister_slot(slot_id: u8) {
    WGPU_BRIDGE.lock().unwrap().active_slots.remove(&slot_id);
}

// =============================================================================
// Helpers
// =============================================================================

/// Round up to 4-byte alignment (wgpu buffer requirement).
fn plane_size(num_bytes: u32) -> u64 {
    ((num_bytes as u64) + 3) & !3
}

/// Total I420 size for given dimensions.
fn i420_buffer_size(width: u32, height: u32) -> u64 {
    let y = plane_size(width * height);
    let uv = plane_size((width + 1) / 2 * ((height + 1) / 2));
    y + uv * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i420_buffer_size() {
        assert_eq!(i420_buffer_size(640, 360), 345600);
        assert_eq!(i420_buffer_size(1280, 720), 1382400);
        assert_eq!(i420_buffer_size(1920, 1080), 3110400);
    }

    #[test]
    fn test_plane_size_alignment() {
        assert_eq!(plane_size(9), 12); // 9 → 12 (next multiple of 4)
        assert_eq!(plane_size(16), 16); // already aligned
        assert_eq!(plane_size(1), 4); // 1 → 4
    }
}
