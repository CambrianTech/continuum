//! Cognitive state and room/participant snapshots.

use super::super::*;

impl VoiceModule {
    pub(super) async fn cmd_set_cognitive_state(&self, p: &Params<'_>) -> Result<CommandResult, String> {
        let user_id = p.str("user_id")?;
        let state_str = p.str("state")?;

        use crate::live::session::cognitive_animation::CognitiveState;
        let state = match state_str {
            "evaluating" => CognitiveState::Evaluating,
            "generating" => CognitiveState::Generating,
            "idle" => CognitiveState::Idle,
            _ => return Err(format!("Invalid cognitive state: {state_str} (expected evaluating|generating|idle)")),
        };

        #[cfg(feature = "avatar-3d")]
        let found = if let Some(bevy_system) = crate::live::video::bevy_renderer::try_get()
        {
            bevy_system.set_cognitive_state_by_identity(user_id, state)
        } else {
            false
        };
        // Built without `avatar-3d`: no 3D avatar exists to carry the state, so
        // nothing was set — the same answer as a renderer that isn't running.
        #[cfg(not(feature = "avatar-3d"))]
        let found = {
            let _ = (user_id, state);
            false
        };

        Ok(CommandResult::Json(serde_json::json!({ "set": found })))
    }

    pub(super) async fn cmd_snapshot_room(&self, _p: &Params<'_>) -> Result<CommandResult, String> {
        #[cfg(feature = "livekit-webrtc")]
        {
            use crate::live::video::capture::VideoFrameCapture;
            use base64::Engine;

            let capture = VideoFrameCapture::instance();
            match capture.snapshot_room().await {
                Some(snap) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&snap.jpeg);
                    Ok(CommandResult::Json(serde_json::json!({
                        "success": true,
                        "base64": b64,
                        "mimeType": "image/jpeg",
                        "width": snap.width,
                        "height": snap.height,
                        "participants": snap.display_name,
                        "hash": snap.hash,
                        "capturedAt": snap.captured_at,
                    })))
                }
                None => Ok(CommandResult::Json(serde_json::json!({
                    "success": false,
                    "error": "No video frames captured yet"
                }))),
            }
        }
        #[cfg(not(feature = "livekit-webrtc"))]
        Ok(CommandResult::Json(serde_json::json!({
            "success": false,
            "error": "Video capture unavailable (compiled without livekit-webrtc)"
        })))
    }

    pub(super) async fn cmd_snapshot_participant(&self, p: &Params<'_>) -> Result<CommandResult, String> {
        #[cfg(feature = "livekit-webrtc")]
        {
            use crate::live::video::capture::VideoFrameCapture;
            use base64::Engine;

            let identity = p.str("identity")?;
            let capture = VideoFrameCapture::instance();
            match capture.snapshot_participant(identity).await {
                Some(snap) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&snap.jpeg);
                    Ok(CommandResult::Json(serde_json::json!({
                        "success": true,
                        "base64": b64,
                        "mimeType": "image/jpeg",
                        "width": snap.width,
                        "height": snap.height,
                        "identity": snap.identity,
                        "displayName": snap.display_name,
                        "hash": snap.hash,
                        "capturedAt": snap.captured_at,
                    })))
                }
                None => Ok(CommandResult::Json(serde_json::json!({
                    "success": false,
                    "error": format!("No video frame for participant '{}'", identity)
                }))),
            }
        }
        #[cfg(not(feature = "livekit-webrtc"))]
        Ok(CommandResult::Json(serde_json::json!({
            "success": false,
            "error": "Video capture unavailable (compiled without livekit-webrtc)"
        })))
    }
}
