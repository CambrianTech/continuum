//! Avatar render loop — backend-agnostic frame production and factory.
//!
//! Spawns a background thread that renders avatar frames and sends them via channel.
//! The renderer is selected by `create_renderer()` — the loop is backend-agnostic.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::{clog_info, clog_warn};
use super::frame::{RgbaFrame, AvatarConfig};
use super::renderer::AvatarRenderer;
use super::backends::{ProceduralRenderer, BevyChannelRenderer};

/// Create the best available renderer for a persona's avatar.
///
/// Selection order:
/// 1. BevyChannelRenderer (3D VRM) — if VRM model exists AND Bevy is ready
/// 2. ProceduralRenderer (colored circle) — always available fallback
///
/// This is the single factory function. All renderer selection logic lives here.
/// New backends plug in by adding a new branch.
pub fn create_renderer(config: AvatarConfig) -> Box<dyn AvatarRenderer> {
    // Attempt 3D renderer if VRM model path is specified
    if let Some(ref vrm_path) = config.vrm_model_path {
        let exists = std::path::Path::new(vrm_path).exists();
        if exists {
            let bevy_system = crate::voice::bevy_renderer::get_or_init();
            let ready = bevy_system.is_ready();
            if ready {
                static NEXT_SLOT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
                let slot = NEXT_SLOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if slot < crate::voice::bevy_renderer::MAX_AVATAR_SLOTS {
                    bevy_system.load_model(slot, vrm_path, &config.display_name, &config.identity);
                    // Register identity → slot mapping for speaking state routing
                    bevy_system.register_identity(&config.identity, slot);
                    if let Some(frame_rx) = bevy_system.frame_receiver(slot) {
                        clog_info!(
                            "🎨 Using BevyChannelRenderer for '{}' (slot {}, model: {})",
                            config.identity, slot, vrm_path
                        );
                        return Box::new(BevyChannelRenderer::new(config, frame_rx.clone(), slot));
                    }
                } else {
                    clog_warn!("🎨 All {} Bevy render slots taken, falling back to procedural for '{}'",
                        crate::voice::bevy_renderer::MAX_AVATAR_SLOTS, config.identity);
                }
            } else {
                clog_warn!("🎨 Bevy renderer not ready, falling back to procedural for '{}'", config.identity);
            }
        }
    }

    clog_info!("🎨 Using ProceduralRenderer for '{}'", config.identity);
    Box::new(ProceduralRenderer::new(config))
}

/// Spawns a background thread that renders avatar frames and sends them via channel.
/// Returns a crossbeam receiver that the LiveKit video loop consumes, plus an
/// `Arc<AtomicU64>` that controls the sleep interval in nanoseconds (for FPS adaptation).
///
/// The renderer is selected by `create_renderer()` — the loop is backend-agnostic.
/// Any AvatarRenderer implementation plugs in without touching this code.
pub fn spawn_renderer_loop(
    config: AvatarConfig,
) -> (crossbeam_channel::Receiver<RgbaFrame>, Arc<AtomicU64>) {
    let fps = config.fps;
    let interval_nanos = Arc::new(AtomicU64::new(
        (1_000_000_000.0 / fps) as u64
    ));
    let interval_nanos_clone = interval_nanos.clone();

    let (tx, rx) = crossbeam_channel::bounded(2); // Small buffer, drop old frames

    let renderer = create_renderer(config);
    let renderer = Arc::new(std::sync::Mutex::new(renderer));

    std::thread::Builder::new()
        .name("avatar-renderer".into())
        .spawn(move || {
            let mut renderer = renderer.lock().unwrap();
            loop {
                let frame = renderer.render_frame();
                match tx.try_send(frame) {
                    Ok(_) => {}
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        // Consumer is slow, drop this frame
                    }
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                        // Consumer dropped, stop rendering
                        break;
                    }
                }
                let nanos = interval_nanos_clone.load(Ordering::Relaxed);
                std::thread::sleep(std::time::Duration::from_nanos(nanos));
            }
            clog_info!("Avatar renderer loop exited");
        })
        .expect("Failed to spawn avatar renderer thread");

    (rx, interval_nanos)
}
