//! IPC server — bidirectional communication with continuum-core.
//!
//! Core → Bridge: commands (join room, speak, etc.) with request_id → response
//! Bridge → Core: events (audio frames, participant changes) pushed asynchronously
//!
//! Both directions share the same Unix socket connection. Messages are
//! length-prefixed frames. Commands have `request_id`; events don't.

use crate::agent::{AgentManager, EventWithPayload};
use continuum_bridge_protocol::{BridgeCommand, BridgeResponse};

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
    let (event_tx, event_rx) = mpsc::unbounded_channel::<EventWithPayload>();
    // MEDIA plane (binary, 2026-09-01): continuous frames (PCM, video) ride
    // their own channel into the SAME writer — identity was bound once via a
    // MediaChannelOpened control event, so each frame is [magic][kind][ch] +
    // payload: no JSON, no strings, no per-frame serialize.
    let (media_tx, media_rx) =
        mpsc::unbounded_channel::<(continuum_bridge_protocol::MediaKind, u16, Vec<u8>)>();
    let manager = Arc::new(AgentManager::new(
        livekit_url.to_string(),
        event_tx,
        media_tx,
    ));

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

                // Spawn event forwarder — pushes bridge events AND binary media
                // frames to core over one socket, two planes. One reused encode
                // buffer: the media hot path allocates nothing once warm.
                let mut event_rx_moved = event_rx;
                let mut media_rx_moved = media_rx;
                let event_handle = handle.clone();
                std::thread::spawn(move || {
                    event_handle.block_on(async move {
                        let mut media_buf: Vec<u8> = Vec::new();
                        loop {
                            tokio::select! {
                                payload = event_rx_moved.recv() => {
                                    let Some(payload) = payload else { break };
                                    let json = match serde_json::to_vec(&payload.event) {
                                        Ok(j) => j,
                                        Err(e) => {
                                            warn!("🌉 Event serialize error: {}", e);
                                            continue;
                                        }
                                    };
                                    let frame = continuum_bridge_protocol::encode_frame(
                                        &json,
                                        payload.binary.as_deref(),
                                    );
                                    let mut w = writer_for_events.lock().unwrap();
                                    if let Err(e) = w.write_all(&frame) {
                                        warn!("🌉 Event write error (core disconnected?): {}", e);
                                        break;
                                    }
                                }
                                media = media_rx_moved.recv() => {
                                    let Some((kind, channel, payload)) = media else { break };
                                    continuum_bridge_protocol::encode_media_frame_into(
                                        &mut media_buf, kind, channel, &payload,
                                    );
                                    let mut w = writer_for_events.lock().unwrap();
                                    if let Err(e) = w.write_all(&media_buf) {
                                        warn!("🌉 Media write error (core disconnected?): {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                    });
                });

                // Handle commands on a DEDICATED std thread — NOT a runtime
                // worker. `dispatch_command` is driven via `rt.block_on(...)`,
                // and `block_on` panics ("Cannot start a runtime from within a
                // runtime") if called from a thread that already has the runtime
                // context entered (i.e. a tokio worker). `run` is an `async fn`
                // awaited by `#[tokio::main]`, so this loop body executes ON a
                // worker; calling `handle_client` inline would panic on the
                // first command. The event forwarder above already sidesteps
                // this with a plain `std::thread` — mirror it here.
                let client = std::thread::spawn(move || {
                    if let Err(e) = handle_client(stream, mgr, handle, writer) {
                        error!("🌉 Client error: {}", e);
                    }
                });
                // Single-client server — one core per bridge. Block until this
                // core disconnects, then stop accepting.
                let _ = client.join();
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
    // Core-assigned OUTBOUND media channels (binary plane): channel →
    // (kind, call_id, user_id), bound once by OpenMediaOut. Media frames then
    // route here with zero JSON and zero identity strings per frame.
    let mut out_channels: std::collections::HashMap<
        u16,
        (continuum_bridge_protocol::MediaKind, String, String),
    > = std::collections::HashMap::new();

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

            // BINARY MEDIA plane first — one byte decides, and the frame is
            // processed as a BORROW of the socket buffer: at 30fps HD RGBA
            // (~3.7MB/frame/persona) every avoidable copy is real CPU
            // (forward-on, no copies — the airc law, 2026-09-01). The only
            // copy on this path is the I420 transform physics requires.
            if let Some((kind, channel, payload)) =
                continuum_bridge_protocol::parse_media_frame(&pending[4..4 + frame_len])
            {
                match out_channels.get(&channel) {
                    Some((_, call_id, user_id)) => match kind {
                        continuum_bridge_protocol::MediaKind::AudioPcm => {
                            let samples: Vec<i16> = payload
                                .chunks_exact(2)
                                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                                .collect();
                            if let Err(e) =
                                rt.block_on(manager.speak(call_id, user_id, samples))
                            {
                                warn!("🌉 media speak failed on ch {}: {}", channel, e);
                            }
                        }
                        continuum_bridge_protocol::MediaKind::VideoRgba => {
                            if let Some((w, h, rgba)) =
                                continuum_bridge_protocol::parse_video_payload(payload)
                            {
                                if let Err(e) = rt.block_on(manager.publish_video_frame(
                                    call_id,
                                    user_id,
                                    rgba,
                                    w as u32,
                                    h as u32,
                                )) {
                                    warn!("🌉 media video failed on ch {}: {}", channel, e);
                                }
                            }
                        }
                        continuum_bridge_protocol::MediaKind::VideoJpeg => {
                            warn!("🌉 VideoJpeg is an inbound-only kind — dropped");
                        }
                    },
                    None => {
                        warn!("🌉 media frame on unbound out-channel {} — dropped", channel)
                    }
                }
                pending.drain(..4 + frame_len);
                continue;
            }

            // JSON control plane (low rate): the owned copy here is fine.
            let frame_data = pending[4..4 + frame_len].to_vec();
            pending.drain(..4 + frame_len);

            let (json_bytes, binary) = continuum_bridge_protocol::decode_frame(&frame_data);

            match serde_json::from_slice::<CommandEnvelope>(json_bytes) {
                Ok(envelope) => {
                    // Channel binding is a CONTROL act, handled here where the
                    // out-channel map lives (once per stream, never per frame).
                    if let BridgeCommand::OpenMediaOut {
                        channel,
                        kind,
                        call_id,
                        user_id,
                    } = envelope.command
                    {
                        info!(
                            "🌉 out-channel {} bound: {:?} for {}/{}",
                            channel,
                            kind,
                            &call_id[..8.min(call_id.len())],
                            &user_id[..8.min(user_id.len())]
                        );
                        out_channels.insert(channel, (kind, call_id, user_id));
                        let response = BridgeResponse {
                            request_id: envelope.request_id,
                            success: true,
                            error: None,
                            data: Some(serde_json::json!({ "opened": true })),
                        };
                        let resp_json = serde_json::to_vec(&response).unwrap_or_default();
                        let resp_frame =
                            continuum_bridge_protocol::encode_frame(&resp_json, None);
                        let mut w = writer.lock().unwrap();
                        if w.write_all(&resp_frame).is_err() {
                            return Ok(());
                        }
                        continue;
                    }
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
        // Bound inline in handle_client (where the out-channel map lives);
        // reaching here means a caller bypassed that path — refuse loudly.
        BridgeCommand::OpenMediaOut { channel, .. } => {
            Err(format!("OpenMediaOut ch {channel} must bind in handle_client"))
        }
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
