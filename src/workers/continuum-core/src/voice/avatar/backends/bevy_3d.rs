//! Bevy 3D VRM renderer — wraps BevyAvatarSystem channel as AvatarRenderer.
//!
//! Receives RGBA frames from BevyAvatarSystem's GPU readback pipeline.
//! Falls back to dark background if Bevy hasn't produced output yet.

use crate::clog_info;
use crate::voice::avatar::backend::{RenderBackend, AvatarError, ModelFormat};
use crate::voice::avatar::frame::{RgbaFrame, AvatarConfig};
use crate::voice::avatar::renderer::AvatarRenderer;
use crate::voice::avatar::catalog::avatar_model_path;
use crate::voice::avatar::types::AvatarModel;

/// 3D VRM model renderer backed by the Bevy headless rendering system.
/// Receives RGBA frames from BevyAvatarSystem's GPU readback pipeline.
/// Falls back to dark background if Bevy is not ready or the channel dies.
pub struct BevyChannelRenderer {
    config: AvatarConfig,
    frame_rx: crossbeam_channel::Receiver<RgbaFrame>,
    slot: u8,
    /// Fallback frame for when Bevy hasn't produced output yet
    fallback_frame: Vec<u8>,
    /// Last successfully received frame — prevents blinking between 3D and fallback
    last_frame: Option<Vec<u8>>,
    /// Track consecutive receive failures to detect dead renderer
    consecutive_failures: u32,
}

impl BevyChannelRenderer {
    /// Create a BevyChannelRenderer for a given slot.
    /// The caller must have already called `bevy_system.load_model(slot, ...)`.
    pub fn new(config: AvatarConfig, frame_rx: crossbeam_channel::Receiver<RgbaFrame>, slot: u8) -> Self {
        let size = (config.width * config.height * 4) as usize;
        // Dark background fallback (no circle) — shown only until first real Bevy frame.
        // Using the same dark color as Bevy's clear color to prevent a visible flash.
        let mut fallback_frame = vec![0u8; size];
        for pixel in fallback_frame.chunks_exact_mut(4) {
            pixel[0] = 26;   // R  (matches clear color 0.1)
            pixel[1] = 26;   // G
            pixel[2] = 46;   // B  (matches clear color 0.18)
            pixel[3] = 255;  // A
        }
        clog_info!(
            "BevyChannelRenderer: slot {} for '{}' ({}x{} @{}fps)",
            slot, config.identity, config.width, config.height, config.fps
        );
        Self {
            config,
            frame_rx,
            slot,
            fallback_frame,
            last_frame: None,
            consecutive_failures: 0,
        }
    }
}

impl AvatarRenderer for BevyChannelRenderer {
    fn render_frame(&mut self) -> RgbaFrame {
        // Try to receive a frame from Bevy (non-blocking)
        match self.frame_rx.try_recv() {
            Ok(frame) => {
                self.consecutive_failures = 0;
                self.last_frame = Some(frame.data.clone());
                frame
            }
            Err(_) => {
                self.consecutive_failures += 1;
                // Return last good Bevy frame if we have one, otherwise the procedural fallback.
                // This prevents blinking between 3D model and colored circle.
                let data = self.last_frame.clone()
                    .unwrap_or_else(|| self.fallback_frame.clone());
                RgbaFrame {
                    width: self.config.width,
                    height: self.config.height,
                    data,
                }
            }
        }
    }

    fn set_speaking(&mut self, speaking: bool) {
        // Forward to BevyAvatarSystem if alive
        if let Some(bevy_system) = crate::voice::bevy_renderer::try_get() {
            bevy_system.set_speaking(self.slot, speaking);
        }
    }
}

// =============================================================================
// Bevy3DBackend — RenderBackend implementation (GPU VRM/glTF rendering)
// =============================================================================

/// Backend factory for the Bevy 3D renderer.
/// Handles VRM and glTF models via the BevyAvatarSystem GPU pipeline.
pub struct Bevy3DBackend {
    initialized: bool,
}

impl Bevy3DBackend {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

/// Atomic slot counter shared across all Bevy3DBackend instances.
static NEXT_SLOT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

impl RenderBackend for Bevy3DBackend {
    fn name(&self) -> &'static str { "bevy_3d" }

    fn description(&self) -> &'static str {
        "GPU-rendered 3D VRM/glTF models via Bevy headless"
    }

    fn supported_formats(&self) -> &[ModelFormat] {
        &[ModelFormat::Vrm0x, ModelFormat::Gltf]
    }

    fn is_initialized(&self) -> bool { self.initialized }

    fn initialize(&mut self) -> Result<(), AvatarError> {
        // Check if Bevy app is ready (non-blocking — the app starts on its own thread)
        let bevy_system = crate::voice::bevy_renderer::get_or_init();
        if bevy_system.is_ready() {
            self.initialized = true;
            Ok(())
        } else {
            // Not ready yet — caller can retry later. This is not an error,
            // just means Bevy hasn't finished initializing.
            self.initialized = false;
            Err(AvatarError::NotInitialized("Bevy app not ready yet".into()))
        }
    }

    fn create_renderer(
        &self,
        model: &AvatarModel,
        config: &AvatarConfig,
    ) -> Result<Box<dyn AvatarRenderer>, AvatarError> {
        let vrm_path = avatar_model_path(model.filename);
        let vrm_path_str = vrm_path.to_string_lossy().to_string();

        if !vrm_path.exists() {
            return Err(AvatarError::ModelNotFound(
                format!("VRM model not found: {}", vrm_path_str)
            ));
        }

        let bevy_system = crate::voice::bevy_renderer::get_or_init();
        if !bevy_system.is_ready() {
            return Err(AvatarError::NotInitialized("Bevy app not ready".into()));
        }

        let slot = NEXT_SLOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if slot >= crate::voice::bevy_renderer::MAX_AVATAR_SLOTS {
            return Err(AvatarError::RenderFailed(
                format!("All {} Bevy render slots taken", crate::voice::bevy_renderer::MAX_AVATAR_SLOTS)
            ));
        }

        bevy_system.load_model(slot, &vrm_path_str, &config.display_name, &config.identity);
        bevy_system.register_identity(&config.identity, slot);

        let frame_rx = bevy_system.frame_receiver(slot)
            .ok_or_else(|| AvatarError::RenderFailed(
                format!("No frame receiver for slot {}", slot)
            ))?;

        clog_info!(
            "🎨 Bevy3DBackend: created renderer for '{}' (slot {}, model: {})",
            config.identity, slot, vrm_path_str
        );
        Ok(Box::new(BevyChannelRenderer::new(config.clone(), frame_rx.clone(), slot)))
    }
}
