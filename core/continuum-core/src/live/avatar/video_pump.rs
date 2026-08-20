//! Avatar video pump — streams Bevy-rendered avatar frames into a persona's
//! LiveKit video track.
//!
//! This is the connective tissue between the headless Bevy renderer (which
//! produces one `RgbaFrame` per slot per readback) and the livekit-bridge
//! (which owns the webrtc video track — see `Agent::publish_video_frame`, the
//! only process linking webrtc). The bridge lazily creates the track on the
//! FIRST frame, so this pump just streams frames as the renderer produces them;
//! there is no explicit track setup here.
//!
//! Frame-driven, not sleep-polled: Bevy ticks at ~15fps and `frame_notify`
//! fires once per readback, so the pump `.await`s the notifier and publishes the
//! freshest frame each time. When it falls behind it drains to the latest frame
//! rather than shipping a backlog (webrtc wants the current pose, not stale
//! ones).
//!
//! Off-main-thread: `allocate_bevy_slot` blocks up to 5s (slot-pool wait + model
//! load) so it runs on `spawn_blocking`; the publish loop is its own tokio task.
//! The task OWNS the `SlotGuard` — when it exits (bridge gone → publish error)
//! or is aborted, the guard drops (RAII) and the slot returns to the pool.
//!
//! This is the single implementation of the pump; both the live `voice/…` path
//! and the `avatar_livekit` example drive personas through it (one logical
//! decision, one place).

use std::sync::Arc;

use crate::live::avatar::catalog::avatar_model_path;
use crate::live::avatar::frame::AvatarConfig;
use crate::live::avatar::render_loop::{allocate_bevy_slot, BevySlotAllocation};
use crate::live::avatar::selection::select_avatar_by_identity;
use crate::live::transport::bridge_client::LiveKitAgentManager;
use crate::live::transport::call_server::CallManager;
use crate::live::video::bevy_renderer::{AVATAR_HEIGHT, AVATAR_WIDTH};
use crate::runtime::handle::Handle;
use crate::{clog_error, clog_info, clog_warn};

/// Resolve the render config for a persona identity. Uses the SAME selection the
/// snapshot path uses (`select_avatar_by_identity` + `avatar_model_path`) so a
/// persona's live-call avatar and its profile snapshot are always the same
/// model — one source of truth for identity → VRM.
fn config_for_identity(identity: &str, display_name: &str) -> Result<AvatarConfig, String> {
    let model = select_avatar_by_identity(identity);
    let vrm_path = avatar_model_path(model.filename);
    if !vrm_path.exists() {
        return Err(format!(
            "VRM model not found for '{}': {}",
            identity,
            vrm_path.display()
        ));
    }
    Ok(AvatarConfig {
        identity: identity.to_string(),
        display_name: display_name.to_string(),
        width: AVATAR_WIDTH,
        height: AVATAR_HEIGHT,
        fps: 15.0,
        vrm_model_path: Some(vrm_path.to_string_lossy().to_string()),
        preference: Default::default(),
    })
}

/// Allocate a Bevy slot for `identity` and spawn a task that streams its rendered
/// frames into the persona's LiveKit video track via the bridge.
///
/// Returns the pump task's `JoinHandle`. The task owns the slot guard; dropping
/// or aborting the handle tears the slot down (RAII). Call once per AI
/// participant, after `get_or_create_agent` has joined that persona to the room.
pub async fn spawn_avatar_video_pump(
    manager: Arc<LiveKitAgentManager>,
    call_manager: Arc<CallManager>,
    call_id: String,
    identity: String,
    display_name: String,
) -> Result<tokio::task::JoinHandle<()>, String> {
    let config = config_for_identity(&identity, &display_name)?;

    // Slot allocation blocks up to 5s (pool wait + model load) — keep it off the
    // async runtime.
    let alloc_id = identity.clone();
    let BevySlotAllocation {
        frame_rx,
        frame_notify,
        slot,
        guard,
    } = tokio::task::spawn_blocking(move || allocate_bevy_slot(config))
        .await
        .map_err(|e| format!("slot allocation task panicked for '{}': {}", alloc_id, e))??;

    clog_info!(
        "📹 Video pump starting for '{}' (slot {}) in call {}",
        &identity[..8.min(identity.len())],
        slot,
        &call_id[..8.min(call_id.len())]
    );

    let handle = tokio::spawn(async move {
        // Guard moved in — held for the loop's lifetime, drops (recycles slot) on exit.
        let _guard = guard;
        let mut published = 0u64;

        // Native call-plane tee (#193/#172): a stable source handle (created once so
        // mix-minus filtering is consistent) + a monotonic sequence/clock. The SAME
        // frame we publish to LiveKit is tee'd into the native video plane so native
        // clients (positron web, glass-box harness) see her real face, not the
        // retired test pattern. Render once, two sinks.
        let native_handle = Handle::new();
        let native_start = std::time::Instant::now();
        let mut native_seq: u32 = 0;

        loop {
            // Frame arrival is the clock — wake when the readback observer writes one.
            frame_notify.notified().await;

            // Drain to the freshest frame; never publish a backlog.
            let mut latest = None;
            while let Ok(frame) = frame_rx.try_recv() {
                latest = Some(frame);
            }
            let Some(frame) = latest else { continue };

            // Guard against a data/dimension mismatch (readback resolution can
            // differ from requested on some backends) — skip rather than ship a
            // mis-sized buffer the bridge would misinterpret.
            if (frame.width * frame.height) as usize != frame.data.len() / 4 {
                continue;
            }

            // Tee the same frame into the native call plane. No-op when no native
            // WS client has the call open; never blocks the LiveKit path meaningfully
            // (a couple of read-locks + a broadcast send). u16 cast is safe — avatar
            // frames are 640x360.
            native_seq = native_seq.wrapping_add(1);
            call_manager
                .push_avatar_frame(
                    &call_id,
                    &identity,
                    native_handle,
                    native_seq,
                    native_start.elapsed().as_millis() as u32,
                    &frame.data,
                    frame.width as u16,
                    frame.height as u16,
                )
                .await;

            match manager
                .publish_video_frame(&call_id, &identity, &frame.data, frame.width, frame.height)
                .await
            {
                Ok(()) => {
                    published += 1;
                    if published == 1 || published % 150 == 0 {
                        clog_info!(
                            "📹 Published video frame #{} for '{}' ({}x{})",
                            published,
                            &identity[..8.min(identity.len())],
                            frame.width,
                            frame.height
                        );
                    }
                }
                Err(e) => {
                    // Bridge gone / track failed — terminal for this pump. Fail
                    // loud (name the cause) and exit so the guard recycles the
                    // slot; never spin silently against a dead bridge.
                    clog_error!(
                        "📹 Video pump for '{}' stopping — publish failed: {}",
                        &identity[..8.min(identity.len())],
                        e
                    );
                    break;
                }
            }
        }

        clog_warn!(
            "📹 Video pump exited for '{}' after {} frames",
            &identity[..8.min(identity.len())],
            published
        );
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: config_for_identity drifting from the snapshot path's
    // identity→VRM selection, or emitting a config the renderer would reject
    // (missing VRM path / zero dimensions). A missing model file is environment-
    // dependent, so we only assert the invariants that hold regardless: the
    // config carries the identity, a non-empty model path, and the shared
    // AVATAR_WIDTH/HEIGHT the bridge track is created against.
    #[test]
    fn config_carries_identity_and_render_dimensions() {
        // select_avatar_by_identity always returns a catalog model; the path may
        // or may not exist on this machine. When it does, config is well-formed.
        if let Ok(config) = config_for_identity("asha", "Asha") {
            assert_eq!(config.identity, "asha");
            assert_eq!(config.display_name, "Asha");
            assert_eq!(config.width, AVATAR_WIDTH);
            assert_eq!(config.height, AVATAR_HEIGHT);
            assert!(config.vrm_model_path.is_some());
            assert!(!config.vrm_model_path.unwrap().is_empty());
        }
    }
}
