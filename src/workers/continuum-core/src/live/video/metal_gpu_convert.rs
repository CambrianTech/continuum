//! Metal GPU RGBA→NV12 Compute Pipeline + Bevy Render World Plugin
//!
//! Converts Bevy's RGBA render target textures to NV12 format directly on the GPU
//! using a Metal compute shader. The NV12 output is written to IOSurface-backed
//! textures, which the GpuBridgePublisher reads for LiveKit video encoding.
//!
//! This eliminates ALL CPU pixel work for macOS GPU bridge slots:
//!   - No GPU readback (Bevy's Readback component not inserted for these slots)
//!   - No CPU RGBA→NV12 conversion
//!   - No CPU→IOSurface copy (GPU writes directly to IOSurface memory)
//!
//! On Apple Silicon, GPU and CPU share unified memory. IOSurface is a kernel-managed
//! shared buffer — the GPU compute shader writes NV12 directly to the same physical
//! memory that LiveKit's VideoToolbox encoder reads from.
//!
//! Architecture:
//!   Bevy renders RGBA → wgpu Texture (GPU memory)
//!   Render-world system: Metal compute RGBA texture → NV12 IOSurface (GPU memory)
//!   GpuBridgePublisher: NV12 IOSurface → CVPixelBuffer → LiveKit (zero-copy)
//!
//! The entire pixel pipeline stays on GPU. CPU only orchestrates (dispatch + signal).

// cfg(target_os = "macos") is applied at the mod declaration in video/mod.rs

use bevy::asset::AssetId;
use bevy::prelude::*;
use bevy::render::Extract;
use bevy::render::render_asset::RenderAssets;
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::GpuImage;
use bevy::render::{ExtractSchedule, Render, RenderApp, RenderSystems};

use metal::{
    CommandQueue, CompileOptions, ComputePipelineState, Device as MetalDevice,
    DeviceRef as MetalDeviceRef, MTLSize,
};
use objc::runtime::Object;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::live::avatar::publishers::gpu_bridge;
use crate::{clog_info, clog_warn};

/// IOSurface FFI type alias
type IOSurfaceRef = *mut std::ffi::c_void;

/// MTLPixelFormat constants
const MTL_PIXEL_FORMAT_R8_UNORM: u64 = 10;
const MTL_PIXEL_FORMAT_RG8_UNORM: u64 = 30;

/// Metal Shading Language source for RGBA→NV12 compute kernel.
///
/// BT.601 video range conversion, matching the CPU implementation in native_buffer.rs.
/// Each thread processes one pixel for Y, and every 2×2 block contributes one UV sample.
///
/// Texture bindings:
///   [[texture(0)]] — source RGBA8 texture (from Bevy render target)
///   [[texture(1)]] — destination Y plane (R8Unorm, full resolution)
///   [[texture(2)]] — destination UV plane (RG8Unorm, half resolution)
const MSL_RGBA_TO_NV12: &str = r#"
#include <metal_stdlib>
using namespace metal;

kernel void rgba_to_nv12(
    texture2d<float, access::read>  src    [[texture(0)]],
    texture2d<float, access::write> dst_y  [[texture(1)]],
    texture2d<float, access::write> dst_uv [[texture(2)]],
    uint2 gid [[thread_position_in_grid]]
) {
    uint w = src.get_width();
    uint h = src.get_height();
    if (gid.x >= w || gid.y >= h) return;

    float4 rgba = src.read(gid);
    float r = rgba.r * 255.0;
    float g = rgba.g * 255.0;
    float b = rgba.b * 255.0;

    // BT.601 video range: Y = (66R + 129G + 25B + 128) >> 8 + 16
    float y = ((66.0 * r + 129.0 * g + 25.0 * b + 128.0) / 256.0 + 16.0) / 255.0;
    dst_y.write(float4(y, 0, 0, 1), gid);

    // UV: only write for top-left pixel of each 2x2 block (4:2:0 subsampling)
    if ((gid.x & 1u) == 0u && (gid.y & 1u) == 0u) {
        float u_val = ((-38.0 * r - 74.0 * g + 112.0 * b + 128.0) / 256.0 + 128.0) / 255.0;
        float v_val = ((112.0 * r - 94.0 * g - 18.0 * b + 128.0) / 256.0 + 128.0) / 255.0;
        dst_uv.write(float4(u_val, v_val, 0, 1), uint2(gid.x / 2, gid.y / 2));
    }
}
"#;

// =============================================================================
// MetalGpuConverter — compiled compute pipeline
// =============================================================================

/// Compiled Metal compute pipeline for RGBA→NV12 conversion.
///
/// Created once from the Metal device, reused for every frame.
#[derive(Resource)]
pub struct MetalGpuConverter {
    pipeline: ComputePipelineState,
    command_queue: CommandQueue,
    device: MetalDevice,
    threadgroup_size: MTLSize,
    frame_count: std::sync::atomic::AtomicU64,
    created_at: std::time::Instant,
}

// SAFETY: Metal pipeline state and command queue are thread-safe kernel objects.
unsafe impl Send for MetalGpuConverter {}
unsafe impl Sync for MetalGpuConverter {}

impl MetalGpuConverter {
    /// Create a new converter by compiling the MSL compute shader.
    pub fn new(device: &MetalDeviceRef) -> Result<Self, String> {
        let options = CompileOptions::new();

        let library = device
            .new_library_with_source(MSL_RGBA_TO_NV12, &options)
            .map_err(|e| format!("Failed to compile RGBA→NV12 MSL shader: {}", e))?;

        let function = library
            .get_function("rgba_to_nv12", None)
            .map_err(|e| format!("Failed to get rgba_to_nv12 function: {}", e))?;

        let pipeline = device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| format!("Failed to create compute pipeline: {}", e))?;

        let command_queue = device.new_command_queue();

        clog_info!("GPU RGBA→NV12 compute pipeline compiled");

        Ok(Self {
            pipeline,
            command_queue,
            device: device.to_owned(),
            threadgroup_size: MTLSize::new(16, 16, 1),
            frame_count: std::sync::atomic::AtomicU64::new(0),
            created_at: std::time::Instant::now(),
        })
    }

    /// Convert an RGBA texture to NV12 on the GPU and write to the slot's IOSurface.
    ///
    /// # Safety
    /// `src_texture` must be a valid Metal texture with RGBA8 pixel format.
    unsafe fn convert(
        &self,
        src_texture: &metal::TextureRef,
        slot_id: u8,
        width: u32,
        height: u32,
    ) -> bool {
        let pair = match gpu_bridge::get_bridge_pair(slot_id) {
            Some(p) => p,
            None => return false,
        };

        let (iosurface, write_idx) = pair.current_write_surface();

        // Create MTLTexture views for the Y and UV planes of the IOSurface.
        let y_texture =
            self.create_iosurface_texture(iosurface, 0, MTL_PIXEL_FORMAT_R8_UNORM, width, height);
        let uv_texture = self.create_iosurface_texture(
            iosurface,
            1,
            MTL_PIXEL_FORMAT_RG8_UNORM,
            width / 2,
            height / 2,
        );

        if y_texture.is_null() || uv_texture.is_null() {
            if !y_texture.is_null() {
                let _: () = objc::msg_send![y_texture, release];
            }
            if !uv_texture.is_null() {
                let _: () = objc::msg_send![uv_texture, release];
            }
            return false;
        }

        // Create command buffer and compute encoder
        let command_buffer = self.command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline);
        encoder.set_texture(0, Some(src_texture));

        // Convert raw objc pointers to metal crate TextureRef references
        let y_ref: &metal::TextureRef = &*(y_texture as *const metal::TextureRef);
        let uv_ref: &metal::TextureRef = &*(uv_texture as *const metal::TextureRef);
        encoder.set_texture(1, Some(y_ref));
        encoder.set_texture(2, Some(uv_ref));

        let grid_size = MTLSize::new(width as u64, height as u64, 1);
        encoder.dispatch_threads(grid_size, self.threadgroup_size);
        encoder.end_encoding();

        command_buffer.commit();
        // Compute dispatch for 640×360 takes ~50-100µs on Apple Silicon.
        command_buffer.wait_until_completed();

        // Release IOSurface texture views
        let _: () = objc::msg_send![y_texture, release];
        let _: () = objc::msg_send![uv_texture, release];

        // Signal new frame to publisher
        pair.signal_gpu_frame_written(write_idx);

        // Periodic logging
        let count = self.frame_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count == 1 || count.is_multiple_of(450) {
            let elapsed = self.created_at.elapsed().as_secs_f64();
            let fps = if elapsed > 0.0 {
                count as f64 / elapsed
            } else {
                0.0
            };
            clog_info!(
                "GPU compute RGBA→NV12: {} frames ({:.1} fps avg)",
                count,
                fps
            );
        }

        true
    }

    /// Create an MTLTexture view into a specific plane of an IOSurface.
    ///
    /// Uses `[MTLDevice newTextureWithDescriptor:iosurface:plane:]` via Objective-C runtime
    /// (not wrapped by the `metal` crate). Returns raw objc pointer; caller must release.
    unsafe fn create_iosurface_texture(
        &self,
        iosurface: IOSurfaceRef,
        plane: u64,
        pixel_format: u64,
        width: u32,
        height: u32,
    ) -> *mut Object {
        let desc_class = objc::runtime::Class::get("MTLTextureDescriptor")
            .expect("MTLTextureDescriptor class");
        let desc: *mut Object = objc::msg_send![desc_class, new];

        let _: () = objc::msg_send![desc, setTextureType: 2u64]; // MTLTextureType2D
        let _: () = objc::msg_send![desc, setPixelFormat: pixel_format];
        let _: () = objc::msg_send![desc, setWidth: width as u64];
        let _: () = objc::msg_send![desc, setHeight: height as u64];
        let _: () = objc::msg_send![desc, setStorageMode: 1u64]; // MTLStorageModeShared
        let _: () = objc::msg_send![desc, setUsage: 2u64]; // MTLTextureUsageShaderWrite

        let device_ptr = &*self.device as *const MetalDeviceRef as *mut Object;
        let texture: *mut Object = objc::msg_send![
            device_ptr,
            newTextureWithDescriptor: desc
            iosurface: iosurface
            plane: plane
        ];

        let _: () = objc::msg_send![desc, release];

        if texture.is_null() {
            static WARN_ONCE: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARN_ONCE.swap(true, Ordering::Relaxed) {
                clog_warn!(
                    "Failed to create IOSurface-backed MTLTexture (plane={}, {}×{}, fmt={})",
                    plane,
                    width,
                    height,
                    pixel_format
                );
            }
        }

        texture
    }
}

// =============================================================================
// Bevy Render World Plugin — dispatches GPU compute after each frame
// =============================================================================

/// Bevy plugin that adds GPU RGBA→NV12 conversion to the render pipeline.
///
/// For each active slot with a GPU bridge, this plugin:
///   1. Extracts slot→render_target mapping from the main world
///   2. After rendering, accesses the GPU texture via wgpu HAL
///   3. Dispatches a Metal compute shader to convert RGBA→NV12
///   4. Writes NV12 directly to the IOSurface for LiveKit consumption
///
/// Slots with GPU bridge do NOT get Bevy's Readback component — no GPU→CPU copy.
/// Slots without GPU bridge (non-macOS fallback) still use Readback + CPU conversion.
pub struct GpuConvertPlugin;

impl Plugin for GpuConvertPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<ExtractedGpuBridgeSlots>()
                .init_resource::<ExtractedSlotDimensions>()
                .init_resource::<ExtractedFrameNotifiers>()
                .add_systems(ExtractSchedule, extract_gpu_bridge_data)
                .add_systems(
                    Render,
                    init_metal_converter
                        .run_if(not(resource_exists::<MetalGpuConverter>))
                        .in_set(RenderSystems::Cleanup),
                )
                .add_systems(
                    Render,
                    gpu_convert_system
                        .run_if(resource_exists::<MetalGpuConverter>)
                        .in_set(RenderSystems::Cleanup),
                );
        }
    }
}

/// Extracted data: which slots have GPU bridges and their render target asset IDs.
#[derive(Resource, Default)]
struct ExtractedGpuBridgeSlots {
    /// (slot_id, render_target_asset_id)
    slots: Vec<(u8, AssetId<Image>)>,
}

/// Extracted slot dimensions for compute dispatch.
#[derive(Resource, Default)]
struct ExtractedSlotDimensions {
    dims: HashMap<u8, (u32, u32)>,
}

/// Extracted frame notifiers for signaling video loops after GPU compute.
#[derive(Resource, Default)]
struct ExtractedFrameNotifiers {
    notifiers: Vec<Arc<tokio::sync::Notify>>,
}

/// Extract GPU bridge slot data from the main world during ExtractSchedule.
fn extract_gpu_bridge_data(
    mut extracted_slots: ResMut<ExtractedGpuBridgeSlots>,
    mut extracted_dims: ResMut<ExtractedSlotDimensions>,
    mut extracted_notifiers: ResMut<ExtractedFrameNotifiers>,
    registry: Extract<Res<super::bevy_renderer::SlotRegistry>>,
    slot_dims: Extract<Res<super::bevy_renderer::SlotDimensions>>,
    notifiers: Extract<Res<super::bevy_renderer::FrameNotifiers>>,
) {
    extracted_slots.slots.clear();
    extracted_dims.dims.clear();

    for (slot, state) in &registry.slots {
        if state.active && state.model_loaded && gpu_bridge::has_bridge(*slot) {
            extracted_slots
                .slots
                .push((*slot, state.render_target_id()));
            if let Some(&dims) = slot_dims.dims.get(slot) {
                extracted_dims.dims.insert(*slot, dims);
            }
        }
    }

    // Extract frame notifiers (only once — they're Arc'd and don't change)
    if extracted_notifiers.notifiers.is_empty() {
        extracted_notifiers.notifiers = notifiers.0.clone();
    }
}

/// Initialize the Metal GPU converter using the render device.
/// Runs once when the render device is first available.
fn init_metal_converter(mut commands: Commands, render_device: Res<RenderDevice>) {
    unsafe {
        let wgpu_device = render_device.wgpu_device();
        let hal_device = match wgpu_device.as_hal::<wgpu::hal::api::Metal>() {
            Some(d) => d,
            None => {
                clog_warn!("Metal HAL not available — GPU compute disabled");
                return;
            }
        };

        let raw_device = hal_device.raw_device().lock();
        match MetalGpuConverter::new(&raw_device) {
            Ok(converter) => {
                commands.insert_resource(converter);
                clog_info!("Metal GPU RGBA→NV12 converter initialized");
            }
            Err(e) => {
                clog_warn!("Failed to init Metal GPU converter: {} — falling back to CPU", e);
            }
        }
    }
}

/// Render-world system: dispatch Metal compute for each GPU bridge slot.
///
/// Runs in RenderSystems::Cleanup, after the main render pass completes.
/// Accesses the rendered texture via RenderAssets<GpuImage> and wgpu HAL.
fn gpu_convert_system(
    converter: Res<MetalGpuConverter>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    bridge_slots: Res<ExtractedGpuBridgeSlots>,
    slot_dims: Res<ExtractedSlotDimensions>,
    notifiers: Res<ExtractedFrameNotifiers>,
) {
    for (slot_id, asset_id) in &bridge_slots.slots {
        let gpu_image = match gpu_images.get(*asset_id) {
            Some(img) => img,
            None => continue, // Image not yet extracted to render world
        };

        let (width, height) = slot_dims
            .dims
            .get(slot_id)
            .copied()
            .unwrap_or((super::bevy_renderer::AVATAR_WIDTH, super::bevy_renderer::AVATAR_HEIGHT));

        unsafe {
            // Access the raw Metal texture via wgpu HAL
            let hal_texture = match gpu_image.texture.as_hal::<wgpu::hal::api::Metal>() {
                Some(t) => t,
                None => continue,
            };

            let mtl_texture = hal_texture.raw_handle();

            // Dispatch GPU compute: RGBA texture → NV12 IOSurface
            if converter.convert(mtl_texture, *slot_id, width, height) {
                // Signal the video loop — frame arrival is the clock
                if let Some(notify) = notifiers.notifiers.get(*slot_id as usize) {
                    notify.notify_one();
                }
            }
        }
    }
}
