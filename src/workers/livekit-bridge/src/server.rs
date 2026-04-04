//! IPC server — accepts commands from continuum-core over Unix socket.
//!
//! Protocol: length-prefixed JSON frames with optional binary payload.
//! Same wire format as livekit-protocol::encode_frame/decode_frame.

use crate::agent::AgentManager;
use continuum_bridge_protocol::{BridgeCommand, BridgeEvent, BridgeResponse};

use std::os::unix::net::{UnixListener, UnixStream};
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

/// Run the bridge IPC server.
pub async fn run(socket_path: &str, livekit_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Remove stale socket
    let _ = std::fs::remove_file(socket_path);

    let listener = UnixListener::bind(socket_path)?;
    info!("🌉 Bridge IPC listening on {}", socket_path);

    // Event channel — bridge events (audio frames, participant changes) sent to core
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<BridgeEvent>();

    let manager = Arc::new(AgentManager::new(livekit_url.to_string(), event_tx));

    // Accept connections from core
    let rt = tokio::runtime::Handle::current();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let mgr = manager.clone();
                let handle = rt.clone();
                info!("🌉 Core connected");

                // Spawn client handler
                std::thread::spawn(move || {
                    if let Err(e) = handle_client(stream, mgr, handle) {
                        error!("🌉 Client error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("🌉 Accept error: {}", e);
            }
        }
    }

    Ok(())
}

/// Handle a single core connection — read commands, dispatch to AgentManager.
fn handle_client(
    mut stream: UnixStream,
    manager: Arc<AgentManager>,
    rt: tokio::runtime::Handle,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = vec![0u8; 1024 * 1024]; // 1MB read buffer
    let mut pending = Vec::new();

    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => {
                info!("🌉 Core disconnected");
                return Ok(());
            }
            Ok(n) => n,
            Err(e) => {
                warn!("🌉 Read error: {}", e);
                return Err(e.into());
            }
        };

        pending.extend_from_slice(&buf[..n]);

        // Process complete frames
        while pending.len() >= 4 {
            let frame_len = u32::from_le_bytes(pending[0..4].try_into().unwrap()) as usize;
            if pending.len() < 4 + frame_len {
                break; // Need more data
            }

            let frame_data = pending[4..4 + frame_len].to_vec();
            pending.drain(..4 + frame_len);

            let (json_bytes, binary) = continuum_bridge_protocol::decode_frame(&frame_data);

            // Parse command
            match serde_json::from_slice::<CommandEnvelope>(json_bytes) {
                Ok(envelope) => {
                    let response = rt.block_on(dispatch_command(
                        &manager,
                        envelope.request_id,
                        envelope.command,
                        binary,
                    ));

                    // Send response
                    let resp_json = serde_json::to_vec(&response).unwrap_or_default();
                    let resp_frame = continuum_bridge_protocol::encode_frame(&resp_json, None);
                    if let Err(e) = stream.write_all(&resp_frame) {
                        warn!("🌉 Write error: {}", e);
                        return Err(e.into());
                    }
                }
                Err(e) => {
                    warn!("🌉 Invalid command JSON: {}", e);
                }
            }
        }
    }
}

/// Command envelope wrapping request_id + command.
#[derive(serde::Deserialize)]
struct CommandEnvelope {
    request_id: u64,
    #[serde(flatten)]
    command: BridgeCommand,
}

/// Dispatch a command to the AgentManager.
async fn dispatch_command(
    manager: &AgentManager,
    request_id: u64,
    command: BridgeCommand,
    binary: Option<&[u8]>,
) -> BridgeResponse {
    let result = match command {
        BridgeCommand::JoinRoom { call_id, user_id, display_name } => {
            match manager.join_room(&call_id, &user_id, &display_name).await {
                Ok(sid) => Ok(serde_json::json!({ "audio_track_sid": sid })),
                Err(e) => Err(e),
            }
        }

        BridgeCommand::LeaveRoom { call_id, user_id } => {
            manager.leave_room(&call_id, &user_id).await;
            Ok(serde_json::json!({ "left": true }))
        }

        BridgeCommand::LeaveAllAgents { call_id } => {
            manager.leave_all(&call_id).await;
            Ok(serde_json::json!({ "left_all": true }))
        }

        BridgeCommand::StartListener { call_id } => {
            manager.start_listener(&call_id).await
                .map(|_| serde_json::json!({ "listening": true }))
        }

        BridgeCommand::StopListener { call_id } => {
            manager.stop_listener(&call_id).await;
            Ok(serde_json::json!({ "stopped": true }))
        }

        BridgeCommand::Speak { call_id, user_id, sample_count } => {
            let samples = decode_i16_samples(binary, sample_count);
            manager.speak(&call_id, &user_id, samples).await
                .map(|_| serde_json::json!({ "spoken": true }))
        }

        BridgeCommand::InjectAudio { call_id, user_id, sample_count } => {
            let samples = decode_i16_samples(binary, sample_count);
            manager.speak(&call_id, &user_id, samples).await
                .map(|_| serde_json::json!({ "injected": true }))
        }

        BridgeCommand::PublishVideoFrame { call_id, user_id, width, height } => {
            if let Some(rgba) = binary {
                manager.publish_video_frame(&call_id, &user_id, rgba, width, height).await
                    .map(|_| serde_json::json!({ "published": true }))
            } else {
                Err("Missing RGBA binary payload".to_string())
            }
        }

        BridgeCommand::PublishTranscription { call_id, user_id, text, .. } => {
            manager.publish_transcription(&call_id, &user_id, &text).await
                .map(|_| serde_json::json!({ "published": true }))
        }

        BridgeCommand::SetCognitiveState { .. } => {
            // Cognitive state is a core concern — bridge just acknowledges
            Ok(serde_json::json!({ "ack": true }))
        }

        BridgeCommand::AddAmbient { .. } |
        BridgeCommand::InjectAmbient { .. } |
        BridgeCommand::RemoveAmbient { .. } => {
            // TODO: Ambient audio management
            Ok(serde_json::json!({ "ack": true }))
        }

        BridgeCommand::SnapshotRoom | BridgeCommand::SnapshotParticipant { .. } => {
            // TODO: Diagnostics
            Ok(serde_json::json!({ "snapshot": "not_implemented" }))
        }
    };

    match result {
        Ok(data) => BridgeResponse {
            request_id,
            success: true,
            error: None,
            data: Some(data),
        },
        Err(e) => BridgeResponse {
            request_id,
            success: false,
            error: Some(e),
            data: None,
        },
    }
}

/// Decode i16 PCM samples from binary payload (little-endian).
fn decode_i16_samples(binary: Option<&[u8]>, expected_count: u32) -> Vec<i16> {
    match binary {
        Some(bytes) => {
            let mut samples = Vec::with_capacity(expected_count as usize);
            for chunk in bytes.chunks_exact(2) {
                samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
            }
            samples
        }
        None => Vec::new(),
    }
}
