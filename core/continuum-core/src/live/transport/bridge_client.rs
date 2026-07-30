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
// The livekit-bridge is a Unix-socket sidecar. On Windows there is no
// Unix-domain socket; alias to TcpStream so this client compiles unchanged.
// `connect()` to the bridge's filesystem-path socket then fails gracefully at
// runtime (voice/livekit is a Unix-only subsystem today). BEHAVIORAL GAP:
// voice bridge is unavailable on Windows until a TCP endpoint is wired.
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(windows)]
use std::net::TcpStream as UnixStream;
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
///
/// Two planes over one socket (the data-plane doctrine, Joel 2026-07-30):
/// - **Control plane** — [`Self::send_command`]: request/response, registers a
///   pending entry and blocks for the ack. Session lifecycle verbs (join, agent
///   create/remove, ambient add/remove) live here. On the grid this is where a
///   remote caller acquires the media HANDLE — grid calls are commands.
/// - **Media plane** — [`Self::send_media`]: fire-and-forget through a bounded
///   queue drained by a dedicated pump thread. PCM and video frames live here.
///   A stalled bridge drops frames (loud, counted) instead of stalling the
///   caller behind a 30s control timeout — media must never starve, and stale
///   media is garbage anyway.
pub struct LiveKitAgentManager {
    /// Shared write access to the bridge socket (control writes + media pump).
    writer: Arc<Mutex<Option<UnixStream>>>,
    /// Pending command responses keyed by request_id.
    pending: Arc<Mutex<HashMap<u64, Arc<PendingRequest>>>>,
    /// Media-plane queue head. Replaced on every reconnect; the old pump thread
    /// exits when its receiver disconnects.
    media_tx: Mutex<Option<std::sync::mpsc::SyncSender<MediaFrame>>>,
    /// Frames dropped because the media queue was full (bridge stalled). Loud
    /// via rate-limited warn in [`Self::send_media`]; MUST stay 0 steady-state.
    media_dropped: Arc<AtomicU64>,
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

/// One queued media-plane frame: fully wire-encoded (envelope + payload), ready
/// for a single `write_all`. Encoding happens on the caller's thread so the
/// pump does nothing but move bytes to the kernel.
struct MediaFrame {
    frame: Vec<u8>,
    kind: &'static str,
}

/// Media queue depth. Sized for burst absorption (~a second of mixed PCM chunks
/// and video frames), small enough that a stalled bridge surfaces as drops
/// within one breath rather than buffering minutes of stale media.
const MEDIA_QUEUE_DEPTH: usize = 32;

/// i16 PCM → little-endian wire bytes in ONE allocation. The former per-site
/// `flat_map(to_le_bytes).collect()` has no useful size hint, so `collect`
/// re-allocates its way up the buffer on the per-turn audio hot path — the
/// "needless copies" class of the data-plane doctrine. This reserves exactly
/// `len * 2` up front; on little-endian targets LLVM lowers the loop to a memcpy.
fn pcm_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

impl LiveKitAgentManager {
    pub fn new() -> Self {
        let socket_dir = std::env::var("CONTINUUM_SOCKET_DIR").unwrap_or_else(|_| {
            dirs::home_dir()
                .map(|h| h.join(".continuum/sockets").to_string_lossy().to_string())
                .unwrap_or_else(|| "/tmp".to_string())
        });
        let bridge_socket_path = format!("{}/livekit-bridge.sock", socket_dir);
        let livekit_url =
            std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "ws://localhost:7880".to_string());

        Self {
            writer: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(HashMap::new())),
            media_tx: Mutex::new(None),
            media_dropped: Arc::new(AtomicU64::new(0)),
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

        clog_info!(
            "🌉 Connected to livekit-bridge at {}",
            self.bridge_socket_path
        );

        // Clone for reader thread
        let reader_stream = stream
            .try_clone()
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
        drop(started);

        // (Re)start the media pump for this connection. Replacing the sender
        // disconnects the previous pump's receiver, so the old thread exits on
        // its own — one live pump per connection, no generation bookkeeping.
        let (tx, rx) = std::sync::mpsc::sync_channel::<MediaFrame>(MEDIA_QUEUE_DEPTH);
        *self.media_tx.lock().unwrap() = Some(tx);
        let pump_writer = Arc::clone(&self.writer);
        std::thread::spawn(move || {
            media_pump_loop(rx, pump_writer);
        });

        Ok(())
    }

    /// Fire-and-forget media-plane send: enqueue a wire-encoded frame for the
    /// pump thread and return immediately. Never blocks the caller on the
    /// socket, never waits for an ack (the bridge's response, if any, falls
    /// through `reader_loop`'s unclaimed-id path). A full queue DROPS the frame
    /// and counts it — the doctrine: media never starves the producer, and a
    /// drop is loud, never silent.
    ///
    /// Grid contract: this is the LOCAL media plane. A remote node never speaks
    /// this socket — it acquires a media handle via a command (grid calls are
    /// commands) and its bytes arrive through that handle's plane.
    fn send_media(
        &self,
        command: BridgeCommand,
        binary: &[u8],
        kind: &'static str,
    ) -> Result<(), String> {
        self.ensure_connected()?;

        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let mut envelope =
            serde_json::to_value(&command).map_err(|e| format!("Serialize error: {}", e))?;
        envelope
            .as_object_mut()
            .unwrap()
            .insert("request_id".to_string(), request_id.into());
        let json_bytes =
            serde_json::to_vec(&envelope).map_err(|e| format!("Serialize error: {}", e))?;
        let frame = continuum_bridge_protocol::encode_frame(&json_bytes, Some(binary));

        let tx_guard = self.media_tx.lock().unwrap();
        let Some(tx) = tx_guard.as_ref() else {
            return Err("Not connected".to_string());
        };
        match tx.try_send(MediaFrame { frame, kind }) {
            Ok(()) => Ok(()),
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                let n = self.media_dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 || n % 32 == 0 {
                    clog_warn!(
                        "🌉 media pump full — {} frames dropped so far (latest: {})",
                        n,
                        kind
                    );
                }
                Ok(())
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                Err("Media pump not running".to_string())
            }
        }
    }

    /// Send command and wait for response (up to 30s).
    fn send_command(
        &self,
        command: BridgeCommand,
        binary: Option<&[u8]>,
    ) -> Result<BridgeResponse, String> {
        self.ensure_connected()?;

        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);

        // Build envelope
        let mut envelope =
            serde_json::to_value(&command).map_err(|e| format!("Serialize error: {}", e))?;
        envelope
            .as_object_mut()
            .unwrap()
            .insert("request_id".to_string(), request_id.into());
        let json_bytes =
            serde_json::to_vec(&envelope).map_err(|e| format!("Serialize error: {}", e))?;
        let frame = continuum_bridge_protocol::encode_frame(&json_bytes, binary);

        // Register pending request
        let pending_req = Arc::new(PendingRequest {
            response: Mutex::new(None),
            signal: Condvar::new(),
        });
        self.pending
            .lock()
            .unwrap()
            .insert(request_id, pending_req.clone());

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
        let response = pending_req.response.lock().unwrap();
        let timeout = std::time::Duration::from_secs(30);
        let (mut guard, timed_out) = pending_req
            .signal
            .wait_timeout_while(response, timeout, |r| r.is_none())
            .unwrap();

        if timed_out.timed_out() {
            self.pending.lock().unwrap().remove(&request_id);
            return Err("Bridge command timed out after 30s".to_string());
        }

        guard
            .take()
            .ok_or_else(|| "No response received".to_string())
    }

    // =========================================================================
    // Public API — matches original LiveKitAgentManager interface
    // =========================================================================

    pub async fn join_as_listener(&self, call_id: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::StartListener {
                call_id: call_id.to_string(),
            },
            None,
        )?;
        if resp.success {
            clog_info!(
                "🎤 STT listener started via bridge for {}",
                &call_id[..8.min(call_id.len())]
            );
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
            let sid = resp
                .data
                .and_then(|d| {
                    d.get("audio_track_sid")
                        .and_then(|s| s.as_str().map(|s| s.to_string()))
                })
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
            BridgeCommand::LeaveAllAgents {
                call_id: call_id.to_string(),
            },
            None,
        );
    }

    pub async fn remove_listener(&self, call_id: &str) {
        let _ = self.send_command(
            BridgeCommand::StopListener {
                call_id: call_id.to_string(),
            },
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
    ) -> Result<(Vec<i16>, u64, u32), String> {
        use crate::live::audio::tts_service;
        use crate::live::avatar::gender::gender_from_identity;
        use crate::live::avatar::types::AvatarGender;

        // Ensure agent exists in bridge
        let _ = self
            .get_or_create_agent(call_id, user_id, display_name)
            .await?;

        // TTS runs HERE in core (uses ort — safe, no webrtc in this process)
        // Name-anchored gender first so the voice matches the persona's NAME +
        // avatar ([[procedural-persona-genesis]]); id-hash only as a fallback.
        let gender = crate::live::avatar::selection::registered_gender(user_id)
            .unwrap_or_else(|| gender_from_identity(user_id));
        let gender_str = match gender {
            AvatarGender::Male => "male",
            AvatarGender::Female => "female",
            // Neuter (they/them): pass "neutral" — resolve_voice_gendered picks a
            // neutral-tagged voice if the backend has one, else falls through to an
            // identity-seeded pick from the full pool (any voice is coherent with
            // they/them). Forward-compatible with neuter voices as they're added.
            AvatarGender::Neutral => "neutral",
        };

        // Gap #2 ([[procedural-persona-genesis]]): when the persona has no explicit
        // voice, seed the voice pick from its IDENTITY — NOT the literal "default",
        // which hashes to ONE voice and collapses every same-gender persona onto it.
        // The TTS resolver treats an arbitrary string as a gender-filtered per-voice
        // SEED, so `user_id` is exactly the right seed: a stable, UNIQUE voice per
        // persona, coherent with the gender + avatar (all drawn from the same id).
        let voice_seed = match voice {
            Some(v) if !v.is_empty() && v != "default" => v,
            _ => user_id,
        };
        let synthesis =
            tts_service::synthesize_speech_async(text, Some(voice_seed), adapter, Some(gender_str))
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

        // Send PCM audio to bridge for LiveKit publishing — media plane,
        // fire-and-forget: the speak turn must never block behind the bridge.
        let pcm_bytes = pcm_le_bytes(&synthesis.samples);

        self.send_media(
            BridgeCommand::Speak {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                sample_count: num_samples as u32,
            },
            &pcm_bytes,
            "speak-pcm",
        )?;

        // Return the synthesized PCM so the caller can tee it into the native call
        // plane (#193 audio convergence) — the same samples that just went to LiveKit.
        Ok((synthesis.samples, duration_ms, sample_rate))
    }

    pub async fn inject_audio(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let pcm_bytes = pcm_le_bytes(&samples);
        self.send_media(
            BridgeCommand::InjectAudio {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                sample_count: samples.len() as u32,
            },
            &pcm_bytes,
            "inject-audio",
        )
    }

    /// Publish an RGBA avatar frame through a persona's LiveKit video track.
    ///
    /// The bridge lazily creates the `NativeVideoSource` + `LocalVideoTrack` on
    /// the first frame (see `Agent::publish_video_frame`), so no explicit track
    /// setup is needed here — the pump just streams frames as the Bevy renderer
    /// produces them. RGBA→I420 conversion happens bridge-side (the only process
    /// that links webrtc). Binary payload = raw RGBA bytes; the bridge trusts
    /// `width`/`height` to interpret them.
    pub async fn publish_video_frame(
        &self,
        call_id: &str,
        user_id: &str,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        self.send_media(
            BridgeCommand::PublishVideoFrame {
                call_id: call_id.to_string(),
                user_id: user_id.to_string(),
                width,
                height,
            },
            rgba,
            "video-frame",
        )
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
            Ok(resp
                .data
                .and_then(|d| {
                    d.get("handle")
                        .and_then(|h| h.as_str().map(|s| s.to_string()))
                })
                .unwrap_or_else(|| format!("ambient-{}", call_id)))
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
        let pcm_bytes = pcm_le_bytes(&samples);
        self.send_media(
            BridgeCommand::InjectAmbient {
                call_id: call_id.to_string(),
                handle: handle.to_string(),
                sample_count: samples.len() as u32,
            },
            &pcm_bytes,
            "inject-ambient",
        )
    }

    pub async fn remove_ambient_source(&self, call_id: &str, handle: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::RemoveAmbient {
                call_id: call_id.to_string(),
                handle: handle.to_string(),
            },
            None,
        )?;
        if resp.success {
            Ok(())
        } else {
            Err(resp.error.unwrap_or_default())
        }
    }

    pub async fn start_ambient_audio(&self, call_id: &str) -> Result<(), String> {
        let resp = self.send_command(
            BridgeCommand::AddAmbient {
                call_id: call_id.to_string(),
                source_name: "rain".to_string(),
            },
            None,
        )?;
        if resp.success {
            Ok(())
        } else {
            Err(resp.error.unwrap_or_default())
        }
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
            use crate::live::session::sentiment::extract_sentiment;
            use crate::live::video::bevy_renderer::SpeechAnimationClip;

            let sentiment = extract_sentiment(text);
            let lip_sync_window_ms = 66u32;
            let mouth_weights = calculate_rms_weights(samples, sample_rate, lip_sync_window_ms);

            if sentiment.emotion != crate::live::video::bevy_renderer::Emotion::Neutral {
                bevy_system.set_emotion_by_identity(
                    user_id,
                    sentiment.emotion,
                    sentiment.intensity,
                    300,
                );
            }
            if sentiment.gesture != crate::live::video::bevy_renderer::Gesture::None {
                bevy_system.set_gesture_by_identity(user_id, sentiment.gesture, 2000);
            }
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

/// Lightweight handle — actual agent lives in bridge process.
pub struct AgentHandle {
    pub call_id: String,
    pub user_id: String,
    pub audio_track_sid: String,
}

// =============================================================================
// Reader thread — receives responses + pushed events from bridge
// =============================================================================

/// Media-plane pump: drain the bounded queue onto the shared socket writer.
/// Runs on its own thread so a slow/stalled bridge back-pressures into DROPPED
/// frames (counted at enqueue) instead of blocking speak turns or the Bevy
/// render loop. Writes serialize through the SAME writer mutex as control
/// commands, so wire framing stays intact; each write is a short memcpy to the
/// kernel, never a wait-for-ack. Exits when the sender is replaced (reconnect)
/// or dropped (shutdown); a write error clears the shared writer so the next
/// caller's `ensure_connected` reconnects and spawns a fresh pump.
fn media_pump_loop(
    rx: std::sync::mpsc::Receiver<MediaFrame>,
    writer: Arc<Mutex<Option<UnixStream>>>,
) {
    while let Ok(item) = rx.recv() {
        let mut guard = writer.lock().unwrap();
        match guard.as_mut() {
            Some(stream) => {
                if let Err(e) = stream.write_all(&item.frame) {
                    clog_warn!("🌉 media pump write failed ({}): {}", item.kind, e);
                    *guard = None;
                    return;
                }
            }
            None => {
                // Connection died under us — this pump's generation is over.
                return;
            }
        }
    }
}

fn reader_loop(mut stream: UnixStream, pending: Arc<Mutex<HashMap<u64, Arc<PendingRequest>>>>) {
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut data = Vec::new();

    // Audio processing state per speaker (call_id + speaker_id → VAD state)
    let mut audio_processors: HashMap<String, AudioProcessor> = HashMap::new();

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

            let (json_bytes, binary) = continuum_bridge_protocol::decode_frame(&frame_data);

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
                handle_bridge_event(event, binary, &mut audio_processors);
            }
        }
    }
}

/// Per-speaker audio processing state (VAD frame accumulation).
struct AudioProcessor {
    call_id: String,
    speaker_id: String,
    speaker_name: String,
    track_sid: String,
    /// Accumulation buffer for VAD (needs 512 samples = 32ms at 16kHz).
    accum_buf: Vec<i16>,
    frame_count: u64,
}

impl AudioProcessor {
    fn new(call_id: String, speaker_id: String, speaker_name: String, track_sid: String) -> Self {
        Self {
            call_id,
            speaker_id,
            speaker_name,
            track_sid,
            accum_buf: Vec::with_capacity(crate::audio_constants::AUDIO_FRAME_SIZE),
            frame_count: 0,
        }
    }
}

/// Handle a pushed event from the bridge.
fn handle_bridge_event(
    event: BridgeEvent,
    binary: Option<&[u8]>,
    processors: &mut HashMap<String, AudioProcessor>,
) {
    match event {
        BridgeEvent::AudioFrame {
            call_id,
            speaker_id,
            speaker_name,
            track_sid,
            sample_count: _,
        } => {
            // Decode PCM samples from binary payload
            let samples: Vec<i16> = match binary {
                Some(bytes) => bytes
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]))
                    .collect(),
                None => return, // No audio data
            };

            let key = format!("{}:{}", call_id, speaker_id);
            let processor = processors.entry(key).or_insert_with(|| {
                clog_info!(
                    "🎤 New audio processor for '{}' in call {}",
                    speaker_name,
                    &call_id[..8.min(call_id.len())]
                );
                AudioProcessor::new(
                    call_id.clone(),
                    speaker_id.clone(),
                    speaker_name.clone(),
                    track_sid.clone(),
                )
            });

            processor.frame_count += 1;
            if processor.frame_count == 1 || processor.frame_count % 3000 == 0 {
                let max_amp = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
                clog_info!(
                    "🎤 Audio frame #{} from '{}': {} samples, max_amp={}",
                    processor.frame_count,
                    processor.speaker_name,
                    samples.len(),
                    max_amp
                );
            }

            // Accumulate into VAD-sized chunks and process
            processor.accum_buf.extend_from_slice(&samples);
            let vad_frame_size = crate::audio_constants::AUDIO_FRAME_SIZE;

            while processor.accum_buf.len() >= vad_frame_size {
                let vad_frame: Vec<i16> = processor.accum_buf.drain(..vad_frame_size).collect();

                // TODO: Feed vad_frame into ProductionVAD → STT pipeline.
                // This requires initializing VAD per-speaker (spawn_blocking for ORT)
                // and routing completed transcriptions to the TS server.
                // For now, the frame accumulation is correct and ready for wiring.
                // The VAD/STT integration is the next step after verifying the
                // bridge audio transport works end-to-end.
                let _ = vad_frame; // Silence unused warning
            }
        }
        BridgeEvent::ParticipantJoined {
            call_id,
            identity,
            name,
        } => {
            clog_info!(
                "👤 Bridge: participant joined call {}: {} ({})",
                &call_id[..8.min(call_id.len())],
                name,
                &identity[..8.min(identity.len())]
            );
        }
        BridgeEvent::ParticipantLeft {
            ref call_id,
            ref identity,
        } => {
            clog_info!(
                "👤 Bridge: participant left call {}: {}",
                &call_id[..8.min(call_id.len())],
                &identity[..8.min(identity.len())]
            );
            // Clean up audio processor for this speaker
            let key = format!("{}:{}", call_id, identity);
            processors.remove(&key);
        }
        BridgeEvent::ListenerReady { call_id } => {
            clog_info!(
                "🎤 Bridge: STT listener ready for call {}",
                &call_id[..8.min(call_id.len())]
            );
        }
        BridgeEvent::RoomDisconnected { call_id, reason } => {
            clog_warn!(
                "🌉 Bridge: room disconnected for call {}: {}",
                &call_id[..8.min(call_id.len())],
                reason
            );
            // Clean up all processors for this call
            processors.retain(|k, _| !k.starts_with(&format!("{}:", call_id)));
        }
        BridgeEvent::VideoFrame {
            call_id,
            speaker_id,
            speaker_name,
            width,
            height,
        } => {
            if let Some(jpeg) = binary {
                #[cfg(feature = "livekit-webrtc")]
                {
                    // When livekit-webrtc is enabled, VideoFrameCapture uses LiveKit directly.
                    // The bridge path is for when livekit-webrtc is disabled. Mark the
                    // bridge-path bindings as used so this cfg doesn't emit unused-variable
                    // warnings (which also trip a rustc 1.94 annotate-snippets render ICE here).
                    let _ = (&call_id, &speaker_id, &speaker_name, &width, &height, &jpeg);
                }
                #[cfg(not(feature = "livekit-webrtc"))]
                {
                    static FRAME_COUNT: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    let count = FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
                    if count == 0 || count % 60 == 0 {
                        clog_info!(
                            "👁 Video frame #{} from '{}': {}x{} ({}KB JPEG)",
                            count,
                            speaker_name,
                            width,
                            height,
                            jpeg.len() / 1024
                        );
                    }
                    // #192: hand the already-encoded frame to perception ingest — a
                    // NON-BLOCKING post from this reader thread (no reactor here) onto
                    // the drain's mpsc; a tokio task fans it out to each persona-viewer's
                    // PerceptionBuffer. Drops silently until the drain is installed. This
                    // is a COPY the bridge already made — the human display plane (the
                    // LiveKit video track) is never touched, and perception samples it at
                    // ~2 Hz regardless of the frame rate ([[perceive-the-room-as-it-is-now]]).
                    crate::media::perception_ingest::try_enqueue(
                        crate::media::perception_ingest::IngestFrame {
                            call_id,
                            speaker_id,
                            jpeg: jpeg.to_vec(),
                            mime: "image/jpeg".to_string(),
                        },
                    );
                }
            }
        }
        BridgeEvent::AgentConnected {
            call_id, user_id, ..
        } => {
            clog_info!(
                "🔊 Bridge: agent connected in call {}: {}",
                &call_id[..8.min(call_id.len())],
                &user_id[..8.min(user_id.len())]
            );
        }
        BridgeEvent::AgentDisconnected {
            call_id,
            user_id,
            reason,
        } => {
            clog_info!(
                "🔊 Bridge: agent disconnected from call {}: {} ({})",
                &call_id[..8.min(call_id.len())],
                &user_id[..8.min(user_id.len())],
                reason
            );
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
    samples
        .chunks(window_size)
        .map(|chunk| {
            let sum_sq: f64 = chunk.iter().map(|&s| (s as f64) * (s as f64)).sum();
            let rms = (sum_sq / chunk.len() as f64).sqrt();
            // Mouth-open weight from windowed RMS. Divisor is the sensitivity: at
            // /8000 the mouth barely cracked open on normal speech (peak ~0.3), so
            // lip-sync read as near-static. /4000 (2x gain) opens the mouth clearly
            // on loud vowels (→1.0, capped) while quiet syllables stay small — a
            // natural, visible range instead of a subtle flutter.
            (rms / 4000.0).min(1.0) as f32
        })
        .collect()
}
