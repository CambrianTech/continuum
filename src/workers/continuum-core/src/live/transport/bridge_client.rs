//! Bridge Client — proxies LiveKit operations to livekit-bridge via Unix socket IPC.
//!
//! Drop-in replacement for LiveKitAgentManager. Same public API, but sends commands
//! to the livekit-bridge process over a Unix socket instead of linking webrtc-sys.
//!
//! Bidirectional: sends commands (request/response), receives pushed events
//! (audio frames from human participants for VAD/STT processing).

use continuum_bridge_protocol::{BridgeCommand, BridgeEvent, BridgeResponse};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crate::{clog_error, clog_info, clog_warn};

/// Captured transcription from STT.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TranscriptionEntry {
    pub call_id: String,
    pub speaker_id: String,
    pub speaker_name: String,
    pub text: String,
    pub timestamp_ms: u64,
}

pub type TranscriptionBuffer = Arc<tokio::sync::Mutex<VecDeque<TranscriptionEntry>>>;

/// Re-export for compatibility.
pub use continuum_bridge_protocol::{ParticipantMetadata, ParticipantRole};

/// Pending request — waiting for response from bridge.
struct PendingRequest {
    response: Mutex<Option<BridgeResponse>>,
    signal: Condvar,
}

/// Bridge client — proxies LiveKit operations to livekit-bridge process.
pub struct LiveKitAgentManager {
    /// Shared write access to the bridge socket.
    writer: Mutex<Option<UnixStream>>,
    /// Pending command responses keyed by request_id.
    pending: Arc<Mutex<HashMap<u64, Arc<PendingRequest>>>>,
    /// Bridge socket path.
    bridge_socket_path: String,
    /// LiveKit URL (for logging only — bridge has the actual connection).
    livekit_url: String,
    /// Request ID counter.
    next_request_id: AtomicU64,
    /// Transcription buffer (populated by event handler).
    transcription_buffer: TranscriptionBuffer,
    /// Whether the reader thread is running.
    reader_started: Mutex<bool>,
}

impl LiveKitAgentManager {
    pub fn new() -> Self {
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
            writer: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            bridge_socket_path,
            livekit_url,
            next_request_id: AtomicU64::new(1),
            transcription_buffer: Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
            reader_started: Mutex::new(false),
        }
    }

    pub fn url(&self) -> &str {
        &self.livekit_url
    }

    /// Ensure connected to bridge. Spawns reader thread on first connection.
    fn ensure_connected(&self) -> Result<(), String> {
        let mut writer = self.writer.lock().unwrap();
        if writer.is_some() {
            return Ok(());
        }

        let stream = UnixStream::connect(&self.bridge_socket_path)
            .map_err(|e| format!("Bridge not available at {}: {}", self.bridge_socket_path, e))?;

        clog_info!("🌉 Connected to livekit-bridge at {}", self.bridge_socket_path);

        // Clone for reader thread
        let reader_stream = stream.try_clone()
            .map_err(|e| format!("Failed to clone socket: {}", e))?;

        *writer = Some(stream);

        // Start reader thread (once)
        let mut started = self.reader_started.lock().unwrap();
        if !*started {
            *started = true;
            let pending = self.pending.clone();
            std::thread::spawn(move || {
                reader_loop(reader_stream, pending);
            });
        }

        Ok(())
    }

    /// Send command and wait for response (up to 30s).
    fn send_command(&self, command: BridgeCommand, binary: Option<&[u8]>) -> Result<BridgeResponse, String> {
        self.ensure_connected()?;

        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        // Build envelope
        let mut envelope = serde_json::to_value(&command)
            .map_err(|e| format!("Serialize error: {}", e))?;
        envelope.as_object_mut().unwrap()
            .insert("request_id".to_string(), request_id.into());
        let json_bytes = serde_json::to_vec(&envelope)
            .map_err(|e| format!("Serialize error: {}", e))?;
        let frame = continuum_bridge_protocol::encode_frame(&json_bytes, binary);

        // Register pending request
        let pending_req = Arc::new(PendingRequest {
            response: Mutex::new(None),
            signal: Condvar::new(),
        });
        self.pending.lock().unwrap().insert(request_id, pending_req.clone());

        // Write command
        {
            let mut writer = self.writer.lock().unwrap();
            if let Some(ref mut stream) = *writer {
                if let Err(e) = stream.write_all(&frame) {
                    *writer = None;
                    self.pending.lock().unwrap().remove(&request_id);
                    return Err(format!("Bridge write failed: {}", e));
                }
            } else {
                self.pending.lock().unwrap().remove(&request_id);
                return Err("Not connected".to_string());
            }
        }

        // Wait for response (30s timeout)
        let mut response = pending_req.response.lock().unwrap();
        let timeout = std::time::Duration::from_secs(30);
        let (mut guard, timed_out) = pending_req.signal.wait_timeout_while(
            response,
            timeout,
            |r| r.is_none(),
        ).unwrap();

        if timed_out.timed_out() {
            self.pending.lock().unwrap().remove(&request_id);
            return Err("Bridge command timed out after 30s".to_string());
        }

        guard.take().ok_or_else(|| "No response received".to_string())
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

        // TTS runs HERE in core (uses ort — safe, no webrtc in this process)
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

        // Publish subtitle BEFORE audio
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

        // Bevy animation (stays in core)
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

    pub async fn add_ambient_source(&self, call_id: &str, source_name: &str) -> Result<String, String> {
        let resp = self.send_command(
            BridgeCommand::AddAmbient {
                call_id: call_id.to_string(),
                source_name: source_name.to_string(),
            },
            None,
        )?;
        if resp.success {
            Ok(resp.data.and_then(|d| d.get("handle").and_then(|h| h.as_str().map(|s| s.to_string())))
                .unwrap_or_else(|| format!("ambient-{}", call_id)))
        } else {
            Err(resp.error.unwrap_or_default())
        }
    }

    pub async fn inject_ambient(&self, call_id: &str, handle: &str, samples: Vec<i16>) -> Result<(), String> {
        let pcm_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
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
            let lip_sync_window_ms = 66u32;
            let mouth_weights = calculate_rms_weights(samples, sample_rate, lip_sync_window_ms);

            if sentiment.emotion != crate::live::video::bevy_renderer::Emotion::Neutral {
                bevy_system.set_emotion_by_identity(user_id, sentiment.emotion, sentiment.intensity, 300);
            }
            if sentiment.gesture != crate::live::video::bevy_renderer::Gesture::None {
                bevy_system.set_gesture_by_identity(user_id, sentiment.gesture, 2000);
            }
            bevy_system.play_speech_by_identity(
                user_id,
                SpeechAnimationClip { mouth_weights, interval_ms: lip_sync_window_ms, duration_ms },
            );
        }
    }
}

impl Default for LiveKitAgentManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight handle — actual agent lives in bridge process.
pub struct AgentHandle {
    pub call_id: String,
    pub user_id: String,
    pub audio_track_sid: String,
}

// =============================================================================
// Reader thread — receives responses + pushed events from bridge
// =============================================================================

fn reader_loop(
    mut stream: UnixStream,
    pending: Arc<Mutex<HashMap<u64, Arc<PendingRequest>>>>,
) {
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut data = Vec::new();

    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => {
                clog_warn!("🌉 Bridge disconnected");
                break;
            }
            Ok(n) => n,
            Err(e) => {
                clog_warn!("🌉 Bridge read error: {}", e);
                break;
            }
        };

        data.extend_from_slice(&buf[..n]);

        while data.len() >= 4 {
            let frame_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
            if data.len() < 4 + frame_len {
                break;
            }

            let frame_data = data[4..4 + frame_len].to_vec();
            data.drain(..4 + frame_len);

            let (json_bytes, _binary) = continuum_bridge_protocol::decode_frame(&frame_data);

            // Try as BridgeResponse first (has request_id)
            if let Ok(response) = serde_json::from_slice::<BridgeResponse>(json_bytes) {
                let mut map = pending.lock().unwrap();
                if let Some(req) = map.remove(&response.request_id) {
                    let mut resp = req.response.lock().unwrap();
                    *resp = Some(response);
                    req.signal.notify_one();
                }
                continue;
            }

            // Try as BridgeEvent (pushed from bridge)
            if let Ok(event) = serde_json::from_slice::<BridgeEvent>(json_bytes) {
                handle_bridge_event(event);
            }
        }
    }
}

/// Handle a pushed event from the bridge.
fn handle_bridge_event(event: BridgeEvent) {
    match event {
        BridgeEvent::AudioFrame { call_id, speaker_id, speaker_name, track_sid, sample_count } => {
            // TODO: Feed into VAD/STT pipeline
            // For now, log periodically
            static FRAME_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let count = FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
            if count == 0 || count % 1000 == 0 {
                clog_info!(
                    "🎤 Bridge audio frame #{} from '{}' ({} samples)",
                    count, speaker_name, sample_count
                );
            }
        }
        BridgeEvent::ParticipantJoined { call_id, identity, name } => {
            clog_info!("👤 Bridge: participant joined call {}: {} ({})", &call_id[..8.min(call_id.len())], name, &identity[..8.min(identity.len())]);
        }
        BridgeEvent::ParticipantLeft { call_id, identity } => {
            clog_info!("👤 Bridge: participant left call {}: {}", &call_id[..8.min(call_id.len())], &identity[..8.min(identity.len())]);
        }
        BridgeEvent::ListenerReady { call_id } => {
            clog_info!("🎤 Bridge: STT listener ready for call {}", &call_id[..8.min(call_id.len())]);
        }
        BridgeEvent::RoomDisconnected { call_id, reason } => {
            clog_warn!("🌉 Bridge: room disconnected for call {}: {}", &call_id[..8.min(call_id.len())], reason);
        }
        BridgeEvent::AgentConnected { call_id, user_id, audio_track_sid } => {
            clog_info!("🔊 Bridge: agent connected in call {}: {}", &call_id[..8.min(call_id.len())], &user_id[..8.min(user_id.len())]);
        }
        BridgeEvent::AgentDisconnected { call_id, user_id, reason } => {
            clog_info!("🔊 Bridge: agent disconnected from call {}: {} ({})", &call_id[..8.min(call_id.len())], &user_id[..8.min(user_id.len())], reason);
        }
        _ => {}
    }
}

// =============================================================================
// RMS calculation for mouth weight animation
// =============================================================================

fn calculate_rms_weights(samples: &[i16], sample_rate: u32, window_ms: u32) -> Vec<f32> {
    let window_size = (sample_rate as usize * window_ms as usize) / 1000;
    if window_size == 0 || samples.is_empty() {
        return vec![];
    }
    samples.chunks(window_size).map(|chunk| {
        let sum_sq: f64 = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / chunk.len() as f64).sqrt();
        (rms / 8000.0).min(1.0) as f32
    }).collect()
}
