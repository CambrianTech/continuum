//! Bridge Client — proxies LiveKit operations to livekit-bridge via Unix socket IPC.
//!
//! Drop-in replacement for LiveKitAgentManager. Same public API, but instead of
//! linking webrtc-sys and managing LiveKit rooms directly, sends commands to the
//! livekit-bridge process over a Unix socket.
//!
//! This eliminates the ort/webrtc-sys protobuf conflict: continuum-core has ort
//! (for VAD, TTS, embeddings) and the bridge has webrtc-sys (for LiveKit rooms).
//! They never share an address space.
//!
//! Audio pipeline:
//!   Core does TTS → PCM → bridge publishes to LiveKit
//!   Bridge receives human audio → PCM → core does VAD/STT

use continuum_bridge_protocol::{BridgeCommand, BridgeResponse, SAMPLE_RATE};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::{clog_error, clog_info, clog_warn};

/// Captured transcription (same structure as original livekit_agent.rs).
#[derive(Debug, Clone, serde::Serialize)]
pub struct TranscriptionEntry {
    pub call_id: String,
    pub speaker_id: String,
    pub speaker_name: String,
    pub text: String,
    pub timestamp_ms: u64,
}

pub type TranscriptionBuffer = Arc<tokio::sync::Mutex<VecDeque<TranscriptionEntry>>>;

const MAX_TRANSCRIPTION_BUFFER: usize = 100;

/// Participant metadata — re-exported from protocol crate for compatibility.
pub use continuum_bridge_protocol::{ParticipantMetadata, ParticipantRole};

/// Bridge client — proxies LiveKit operations to livekit-bridge process.
/// Thread-safe: all state behind Arc<Mutex>.
pub struct LiveKitAgentManager {
    /// Connection to the bridge process. None if not connected.
    connection: Mutex<Option<BridgeConnection>>,
    /// Path to the bridge's Unix socket.
    bridge_socket_path: String,
    /// LiveKit URL (for logging/diagnostics only — bridge has the actual connection).
    livekit_url: String,
    /// Request ID counter.
    next_request_id: AtomicU64,
    /// Transcription buffer (populated by audio frame handler from bridge).
    transcription_buffer: TranscriptionBuffer,
}

struct BridgeConnection {
    stream: UnixStream,
}

impl LiveKitAgentManager {
    /// Create a new bridge client. Does NOT connect immediately — lazy connection
    /// on first command, with automatic reconnection on failure.
    pub fn new() -> Self {
        // Bridge socket path: same directory as core's socket, predictable name.
        let socket_dir = std::env::var("CONTINUUM_SOCKET_DIR")
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|h| h.join(".continuum/sockets").to_string_lossy().to_string())
                    .unwrap_or_else(|| "/tmp".to_string())
            });
        let bridge_socket_path = format!("{}/livekit-bridge.sock", socket_dir);

        let livekit_url = std::env::var("LIVEKIT_URL")
            .unwrap_or_else(|_| "ws://localhost:7880".to_string());

        Self {
            connection: Mutex::new(None),
            bridge_socket_path,
            livekit_url,
            next_request_id: AtomicU64::new(1),
            transcription_buffer: Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
        }
    }

    pub fn url(&self) -> &str {
        &self.livekit_url
    }

    /// Send a command to the bridge and wait for response.
    fn send_command(&self, command: BridgeCommand, binary: Option<&[u8]>) -> Result<BridgeResponse, String> {
        let mut conn = self.connection.lock().unwrap();

        // Lazy connect
        if conn.is_none() {
            match UnixStream::connect(&self.bridge_socket_path) {
                Ok(stream) => {
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();
                    clog_info!("🌉 Connected to livekit-bridge at {}", self.bridge_socket_path);
                    *conn = Some(BridgeConnection { stream });
                }
                Err(e) => {
                    return Err(format!("Bridge not available at {}: {}", self.bridge_socket_path, e));
                }
            }
        }

        let bc = conn.as_mut().unwrap();
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        // Build envelope: { request_id, ...command fields }
        let mut envelope = serde_json::to_value(&command)
            .map_err(|e| format!("Serialize error: {}", e))?;
        envelope.as_object_mut().unwrap().insert("request_id".to_string(), request_id.into());

        let json_bytes = serde_json::to_vec(&envelope)
            .map_err(|e| format!("Serialize error: {}", e))?;

        let frame = continuum_bridge_protocol::encode_frame(&json_bytes, binary);

        // Send
        if let Err(e) = bc.stream.write_all(&frame) {
            *conn = None; // Drop broken connection
            return Err(format!("Bridge write failed: {}", e));
        }

        // Read response
        let mut len_buf = [0u8; 4];
        if let Err(e) = bc.stream.read_exact(&mut len_buf) {
            *conn = None;
            return Err(format!("Bridge read failed: {}", e));
        }
        let resp_len = u32::from_le_bytes(len_buf) as usize;

        let mut resp_buf = vec![0u8; resp_len];
        if let Err(e) = bc.stream.read_exact(&mut resp_buf) {
            *conn = None;
            return Err(format!("Bridge read failed: {}", e));
        }

        let (json_bytes, _binary) = continuum_bridge_protocol::decode_frame(&resp_buf);
        serde_json::from_slice::<BridgeResponse>(json_bytes)
            .map_err(|e| format!("Bridge response parse error: {}", e))
    }

    // =========================================================================
    // Public API — matches original LiveKitAgentManager interface
    // =========================================================================

    pub async fn join_as_listener(&self, call_id: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::StartListener { call_id: call_id.to_string() },
            None,
        )?;
        if resp.success {
            clog_info!("🎤 STT listener started via bridge for {}", &call_id[..8.min(call_id.len())]);
            Ok(())
        } else {
            Err(resp.error.unwrap_or_else(|| "Bridge error".to_string()))
        }
    }

    pub async fn get_or_create_agent(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<AgentHandle, String> {
        let resp = self.send_command(
            BridgeCommand::JoinRoom {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                display_name: display_name.unwrap_or(user_id).to_string(),
            },
            None,
        )?;
        if resp.success {
            let sid = resp.data
                .and_then(|d| d.get("audio_track_sid").and_then(|s| s.as_str().map(|s| s.to_string())))
                .unwrap_or_default();
            Ok(AgentHandle {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                audio_track_sid: sid,
            })
        } else {
            Err(resp.error.unwrap_or_else(|| "Bridge error".to_string()))
        }
    }

    pub async fn remove_agent(&self, call_id: &str, user_id: &str) {
        let _ = self.send_command(
            BridgeCommand::LeaveRoom {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
            },
            None,
        );
    }

    pub async fn remove_agents_for_call(&self, call_id: &str) {
        let _ = self.send_command(
            BridgeCommand::LeaveAllAgents { call_id: call_id.to_string() },
            None,
        );
    }

    pub async fn remove_listener(&self, call_id: &str) {
        let _ = self.send_command(
            BridgeCommand::StopListener { call_id: call_id.to_string() },
            None,
        );
    }

    /// Synthesize TTS locally (ort — safe, no webrtc in this process), then
    /// send the resulting PCM to the bridge for LiveKit publishing.
    pub async fn speak_in_call(
        &self,
        call_id: &str,
        user_id: &str,
        text: &str,
        voice: Option<&str>,
        adapter: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<(usize, u64, u32), String> {
        use crate::live::audio::tts_service;
        use crate::live::avatar::gender::gender_from_identity;
        use crate::live::avatar::types::AvatarGender;

        // Ensure agent exists in bridge
        let _ = self.get_or_create_agent(call_id, user_id, display_name).await?;

        // TTS runs HERE in core (uses ort — safe, no webrtc conflict)
        let gender = gender_from_identity(user_id);
        let gender_str = match gender {
            AvatarGender::Male => "male",
            AvatarGender::Female => "female",
        };

        let synthesis = tts_service::synthesize_speech_async(text, voice, adapter, Some(gender_str))
            .await
            .map_err(|e| format!("TTS synthesis failed: {}", e))?;

        let num_samples = synthesis.samples.len();
        let duration_ms = synthesis.duration_ms;
        let sample_rate = synthesis.sample_rate;

        // Publish subtitle BEFORE audio (same ordering as original)
        let _ = self.send_command(
            BridgeCommand::PublishTranscription {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                text: text.to_string(),
                track_sid: String::new(),
                r#final: true,
            },
            None,
        );

        // Send Bevy animation commands (still in core — Bevy stays in core)
        self.trigger_speech_animation(user_id, text, &synthesis.samples, sample_rate, duration_ms);

        // Send PCM audio to bridge for LiveKit publishing
        let pcm_bytes: Vec<u8> = synthesis.samples.iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        self.send_command(
            BridgeCommand::Speak {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                sample_count: num_samples as u32,
            },
            Some(&pcm_bytes),
        )?;

        Ok((num_samples, duration_ms, sample_rate))
    }

    pub async fn inject_audio(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let pcm_bytes: Vec<u8> = samples.iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        let resp = self.send_command(
            BridgeCommand::InjectAudio {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                sample_count: samples.len() as u32,
            },
            Some(&pcm_bytes),
        )?;
        if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default()) }
    }

    pub async fn add_ambient_source(
        &self,
        call_id: &str,
        source_name: &str,
    ) -> Result<String, String> {
        let resp = self.send_command(
            BridgeCommand::AddAmbient {
                call_id: call_id.to_string(),
                source_name: source_name.to_string(),
            },
            None,
        )?;
        if resp.success {
            let handle = resp.data
                .and_then(|d| d.get("handle").and_then(|h| h.as_str().map(|s| s.to_string())))
                .unwrap_or_else(|| format!("ambient-{}", call_id));
            Ok(handle)
        } else {
            Err(resp.error.unwrap_or_default())
        }
    }

    pub async fn inject_ambient(
        &self,
        call_id: &str,
        handle: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let pcm_bytes: Vec<u8> = samples.iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        let resp = self.send_command(
            BridgeCommand::InjectAmbient {
                call_id: call_id.to_string(),
                handle: handle.to_string(),
                sample_count: samples.len() as u32,
            },
            Some(&pcm_bytes),
        )?;
        if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default()) }
    }

    pub async fn remove_ambient_source(&self, call_id: &str, handle: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::RemoveAmbient {
                call_id: call_id.to_string(),
                handle: handle.to_string(),
            },
            None,
        )?;
        if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default()) }
    }

    pub async fn start_ambient_audio(&self, call_id: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::AddAmbient {
                call_id: call_id.to_string(),
                source_name: "rain".to_string(),
            },
            None,
        )?;
        if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default()) }
    }

    pub async fn poll_transcriptions(&self, call_id: Option<&str>) -> Vec<TranscriptionEntry> {
        let mut buf = self.transcription_buffer.lock().await;
        if let Some(cid) = call_id {
            let (matching, remaining): (VecDeque<_>, VecDeque<_>) =
                buf.drain(..).partition(|e| e.call_id == cid);
            *buf = remaining;
            matching.into_iter().collect()
        } else {
            buf.drain(..).collect()
        }
    }

    // =========================================================================
    // Bevy animation (stays in core — core owns the renderer)
    // =========================================================================

    fn trigger_speech_animation(
        &self,
        user_id: &str,
        text: &str,
        samples: &[i16],
        sample_rate: u32,
        duration_ms: u64,
    ) {
        if let Some(bevy_system) = crate::live::video::bevy_renderer::try_get() {
            use crate::live::video::bevy_renderer::SpeechAnimationClip;
            use crate::live::session::sentiment::extract_sentiment;

            let sentiment = extract_sentiment(text);

            // Mouth weights for lip sync
            let lip_sync_window_ms = 66u32;
            let mouth_weights = calculate_rms_weights(samples, sample_rate, lip_sync_window_ms);

            // Emotion
            if sentiment.emotion != crate::live::video::bevy_renderer::Emotion::Neutral {
                bevy_system.set_emotion_by_identity(
                    user_id,
                    sentiment.emotion,
                    sentiment.intensity,
                    300,
                );
            }

            // Gesture
            if sentiment.gesture != crate::live::video::bevy_renderer::Gesture::None {
                bevy_system.set_gesture_by_identity(user_id, sentiment.gesture, 2000);
            }

            // Speech animation
            bevy_system.play_speech_by_identity(
                user_id,
                SpeechAnimationClip {
                    mouth_weights,
                    interval_ms: lip_sync_window_ms,
                    duration_ms,
                },
            );
        }
    }
}

impl Default for LiveKitAgentManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight handle returned by get_or_create_agent.
/// The actual agent lives in the bridge process — this is just metadata.
pub struct AgentHandle {
    pub call_id: String,
    pub user_id: String,
    pub audio_track_sid: String,
}

// =============================================================================
// RMS calculation for mouth weight animation (copied from livekit_agent.rs)
// =============================================================================

fn calculate_rms_weights(samples: &[i16], sample_rate: u32, window_ms: u32) -> Vec<f32> {
    let window_size = (sample_rate as usize * window_ms as usize) / 1000;
    if window_size == 0 || samples.is_empty() {
        return vec![];
    }

    let mut weights = Vec::new();
    for chunk in samples.chunks(window_size) {
        let sum_sq: f64 = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / chunk.len() as f64).sqrt();
        // Normalize to 0.0-1.0 range (32768 is max i16 amplitude)
        let normalized = (rms / 8000.0).min(1.0) as f32;
        weights.push(normalized);
    }
    weights
}
