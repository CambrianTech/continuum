//! IPC server — bidirectional communication with continuum-core.
//!
//! Core → Bridge: commands (join room, speak, etc.) with request_id → response
//! Bridge → Core: events (audio frames, participant changes) pushed asynchronously
//!
//! Both directions share the same Unix socket connection. Messages are
//! length-prefixed frames. Commands have `request_id`; events don't.

use crate::agent::AgentManager;
use continuum_bridge_protocol::{BridgeCommand, BridgeEvent, BridgeResponse};

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Run the bridge IPC server.
pub async fn run(socket_path: &str, livekit_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    info!("🌉 Bridge IPC listening on {}", socket_path);

    // Event channel — AgentManager sends events here, we forward to core
    let (event_tx, event_rx) = mpsc::unbounded_channel::<BridgeEvent>();
    let manager = Arc::new(AgentManager::new(livekit_url.to_string(), event_tx));

    let rt = tokio::runtime::Handle::current();

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let mgr = manager.clone();
                let handle = rt.clone();
                info!("🌉 Core connected");

                // Clone stream for the event writer (OS-level dup)
                let write_stream = match stream.try_clone() {
                    Ok(s) => s,
                    Err(e) => {
                        error!("🌉 Failed to clone socket: {}", e);
                        continue;
                    }
                };

                // Share write access between command handler and event forwarder
                let writer = Arc::new(std::sync::Mutex::new(write_stream));
                let writer_for_events = writer.clone();

                // Spawn event forwarder — pushes bridge events to core
                let mut event_rx_moved = {
                    // We can only have one receiver, so take it on first connection.
                    // Subsequent connections won't get events (single-client design).
                    // TODO: For multi-client, use broadcast channel.
                    event_rx
                };
                let event_handle = handle.clone();
                std::thread::spawn(move || {
                    event_handle.block_on(async move {
                        while let Some(event) = event_rx_moved.recv().await {
                            let json = match serde_json::to_vec(&event) {
                                Ok(j) => j,
                                Err(e) => {
                                    warn!("🌉 Event serialize error: {}", e);
                                    continue;
                                }
                            };

                            // For AudioFrame events, include binary PCM payload
                            // (TODO: optimize — currently audio samples are in the JSON)
                            let frame = continuum_bridge_protocol::encode_frame(&json, None);

                            let mut w = writer_for_events.lock().unwrap();
                            if let Err(e) = w.write_all(&frame) {
                                warn!("🌉 Event write error (core disconnected?): {}", e);
                                break;
                            }
                        }
                    });
                });

                // Handle commands on this thread
                if let Err(e) = handle_client(stream, mgr, handle, writer) {
                    error!("🌉 Client error: {}", e);
                }

                // Only handle one core connection (first one wins).
                // Bridge is a single-client server — one core per bridge.
                break;
            }
            Err(e) => {
                error!("🌉 Accept error: {}", e);
            }
        }
    }

    Ok(())
}

/// Handle commands from core — read, dispatch, respond.
fn handle_client(
    mut read_stream: UnixStream,
    manager: Arc<AgentManager>,
    rt: tokio::runtime::Handle,
    writer: Arc<std::sync::Mutex<UnixStream>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = vec![0u8; 4 * 1024 * 1024]; // 4MB read buffer (video frames can be large)
    let mut pending = Vec::new();

    loop {
        let n = match read_stream.read(&mut buf) {
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
                break;
            }

            let frame_data = pending[4..4 + frame_len].to_vec();
            pending.drain(..4 + frame_len);

            let (json_bytes, binary) = continuum_bridge_protocol::decode_frame(&frame_data);

            match serde_json::from_slice::<CommandEnvelope>(json_bytes) {
                Ok(envelope) => {
                    let response = rt.block_on(dispatch_command(
                        &manager,
                        envelope.request_id,
                        envelope.command,
                        binary,
                    ));

                    let resp_json = serde_json::to_vec(&response).unwrap_or_default();
                    let resp_frame = continuum_bridge_protocol::encode_frame(&resp_json, None);

                    let mut w = writer.lock().unwrap();
                    if let Err(e) = w.write_all(&resp_frame) {
                        warn!("🌉 Response write error: {}", e);
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

#[derive(serde::Deserialize)]
struct CommandEnvelope {
    request_id: u64,
    #[serde(flatten)]
    command: BridgeCommand,
}

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
        BridgeCommand::SetCognitiveState { .. } => Ok(serde_json::json!({ "ack": true })),
        BridgeCommand::AddAmbient { .. } |
        BridgeCommand::InjectAmbient { .. } |
        BridgeCommand::RemoveAmbient { .. } => Ok(serde_json::json!({ "ack": true })),
        BridgeCommand::SnapshotRoom | BridgeCommand::SnapshotParticipant { .. } => {
            Ok(serde_json::json!({ "snapshot": "not_implemented" }))
        }
    };

    match result {
        Ok(data) => BridgeResponse { request_id, success: true, error: None, data: Some(data) },
        Err(e) => BridgeResponse { request_id, success: false, error: Some(e), data: None },
    }
}

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
