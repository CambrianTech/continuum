//! LiveKitAgent — Server-side participant bridging TTS/STT/VAD into LiveKit rooms.
//!
//! Replaces the custom WebSocket call_server + mixer with LiveKit's WebRTC SFU.
//! Each AI persona in a call gets one LiveKitAgent that:
//!   - Publishes TTS audio → NativeAudioSource → LiveKit → browsers
//!   - Publishes avatar video → NativeVideoSource → LiveKit → browsers
//!   - Publishes ambient audio (hold music, etc.) via separate tracks
//!   - Broadcasts transcriptions via LiveKit's native transcription API
//!
//! STT Listener agent (one per call):
//!   - Subscribes to human audio tracks → NativeAudioStream → VAD → STT
//!   - Publishes transcription segments via LiveKit's native API
//!   - Uses __stt__ identity prefix (filtered out in browser UI)
//!
//! Audio format: 16kHz mono i16 PCM — matches our TTS output and LiveKit's AudioFrame.data (Cow<[i16]>).

use crate::audio_constants::{AUDIO_SAMPLE_RATE, LIVEKIT_DEV_KEY, LIVEKIT_DEV_SECRET, LIVEKIT_PORT};
use crate::secrets::get_secret;

use livekit::prelude::*;
use livekit::options::TrackPublishOptions;
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit_api::access_token::{AccessToken, VideoGrants};

use std::borrow::Cow;
use std::collections::HashMap;
use tokio::sync::{mpsc, Mutex};
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::{info, warn, error};

/// A captured transcription from the STT listener, available for test polling.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TranscriptionEntry {
    pub call_id: String,
    pub speaker_id: String,
    pub speaker_name: String,
    pub text: String,
    pub timestamp_ms: u64,
}

/// Shared buffer for storing transcriptions from STT listeners.
/// Tests poll this via `voice/poll-transcriptions`.
pub type TranscriptionBuffer = Arc<Mutex<VecDeque<TranscriptionEntry>>>;

const MAX_TRANSCRIPTION_BUFFER: usize = 100;

/// Audio samples per 10ms at 16kHz — LiveKit processes in 10ms chunks
const SAMPLES_PER_10MS: u32 = (AUDIO_SAMPLE_RATE / 100) as u32;

/// Identity prefix for STT listener agents — filtered out in browser UI
pub const STT_LISTENER_PREFIX: &str = "__stt__";

/// Identity prefix for AI persona agents — STT listener skips these to prevent echo loops
pub const AI_AGENT_PREFIX: &str = "__ai__";

/// Events emitted by the agent for the VoiceModule to handle
#[derive(Debug)]
pub enum AgentEvent {
    /// Full utterance detected (VAD sentence boundary)
    Utterance {
        speaker_id: String,
        samples: Vec<i16>,
    },
    /// Participant joined the room
    ParticipantJoined {
        identity: String,
        name: String,
    },
    /// Participant left the room
    ParticipantLeft {
        identity: String,
    },
}

/// Server-side LiveKit participant that bridges our AI pipeline to WebRTC.
pub struct LiveKitAgent {
    room: Room,
    /// Primary audio source for TTS output
    audio_source: NativeAudioSource,
    /// Video source for avatar rendering
    video_source: NativeVideoSource,
    /// Additional audio sources for ambient/background audio (hold music, etc.)
    ambient_sources: Arc<Mutex<HashMap<String, NativeAudioSource>>>,
    /// Agent events channel
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    /// Identity of this agent in the LiveKit room
    identity: String,
}

impl LiveKitAgent {
    /// Connect to a LiveKit room as a server-side participant (publishes audio + video tracks).
    ///
    /// Returns the agent and an event receiver for incoming audio/participant events.
    pub async fn connect(
        livekit_url: &str,
        call_id: &str,
        persona_id: &str,
        persona_name: &str,
    ) -> Result<(Self, mpsc::UnboundedReceiver<AgentEvent>), String> {
        // Generate access token using secrets (config.env) with dev fallbacks
        let api_key = get_secret("LIVEKIT_API_KEY").unwrap_or(LIVEKIT_DEV_KEY);
        let api_secret = get_secret("LIVEKIT_API_SECRET").unwrap_or(LIVEKIT_DEV_SECRET);
        let token = AccessToken::with_api_key(api_key, api_secret)
            .with_identity(persona_id)
            .with_name(persona_name)
            .with_grants(VideoGrants {
                room_join: true,
                room: call_id.to_string(),
                can_publish: true,
                can_subscribe: true,
                can_publish_data: true,
                ..Default::default()
            })
            .to_jwt()
            .map_err(|e| format!("Failed to generate LiveKit token: {}", e))?;

        // Connect to room
        let (room, mut room_events) = Room::connect(livekit_url, &token, RoomOptions::default())
            .await
            .map_err(|e| format!("Failed to connect to LiveKit room: {}", e))?;

        info!(
            "🔊 LiveKitAgent '{}' connected to room '{}'",
            persona_name, call_id
        );

        // Create audio source for TTS output (16kHz mono, 1s buffer)
        let audio_source = NativeAudioSource::new(
            AudioSourceOptions::default(),
            AUDIO_SAMPLE_RATE,
            1, // mono
            1000, // 1 second queue
        );

        // Publish TTS audio track
        let audio_track = LocalAudioTrack::create_audio_track(
            &format!("{}-voice", persona_id),
            RtcAudioSource::Native(audio_source.clone()),
        );
        room.local_participant()
            .publish_track(
                LocalTrack::Audio(audio_track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("Failed to publish audio track: {}", e))?;

        // Create video source for avatar (640x480 default, not a screencast)
        let video_source = NativeVideoSource::new(
            VideoResolution {
                width: 640,
                height: 480,
            },
            false, // is_screencast
        );

        // Publish avatar video track
        let video_track = LocalVideoTrack::create_video_track(
            &format!("{}-avatar", persona_id),
            RtcVideoSource::Native(video_source.clone()),
        );
        room.local_participant()
            .publish_track(
                LocalTrack::Video(video_track),
                TrackPublishOptions {
                    source: TrackSource::Camera,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("Failed to publish video track: {}", e))?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let event_tx_clone = event_tx.clone();
        let identity = persona_id.to_string();

        // Spawn event handler task — routes room events to the agent's event channel
        tokio::spawn(async move {
            while let Some(event) = room_events.recv().await {
                match event {
                    RoomEvent::TrackSubscribed {
                        track,
                        publication: _,
                        participant,
                    } => {
                        let speaker_id = participant.identity().to_string();
                        // Skip tracks from other system agents
                        if speaker_id.starts_with(STT_LISTENER_PREFIX) {
                            continue;
                        }
                        info!("🎤 Agent subscribed to track from '{}'", speaker_id);

                        if let RemoteTrack::Audio(audio_track) = track {
                            let tx = event_tx_clone.clone();
                            let sid = speaker_id.clone();
                            tokio::spawn(async move {
                                process_audio_stream_with_vad(audio_track, sid, tx).await;
                            });
                        }
                    }
                    RoomEvent::ParticipantConnected(participant) => {
                        let name = participant.name().to_string();
                        let id = participant.identity().to_string();
                        if id.starts_with(STT_LISTENER_PREFIX) {
                            continue;
                        }
                        info!("👤 Participant joined: {} ({})", name, id);
                        let _ = event_tx_clone.send(AgentEvent::ParticipantJoined {
                            identity: id,
                            name,
                        });
                    }
                    RoomEvent::ParticipantDisconnected(participant) => {
                        let id = participant.identity().to_string();
                        if id.starts_with(STT_LISTENER_PREFIX) {
                            continue;
                        }
                        info!("👤 Participant left: {}", id);
                        let _ = event_tx_clone.send(AgentEvent::ParticipantLeft {
                            identity: id,
                        });
                    }
                    _ => {}
                }
            }
        });

        let agent = Self {
            room,
            audio_source,
            video_source,
            ambient_sources: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
            identity,
        };

        Ok((agent, event_rx))
    }

    /// Feed TTS-synthesized PCM audio to the LiveKit room.
    /// Accepts our standard format: Vec<i16> at 16kHz mono.
    /// Splits into 10ms chunks for LiveKit's AudioFrame.
    pub async fn speak(&self, samples: Vec<i16>) -> Result<(), String> {
        let chunk_size = SAMPLES_PER_10MS as usize;

        for chunk in samples.chunks(chunk_size) {
            let frame = AudioFrame {
                data: Cow::Borrowed(chunk),
                sample_rate: AUDIO_SAMPLE_RATE,
                num_channels: 1,
                samples_per_channel: chunk.len() as u32,
            };

            self.audio_source
                .capture_frame(&frame)
                .await
                .map_err(|e| format!("Failed to capture audio frame: {}", e))?;
        }

        Ok(())
    }

    /// Inject raw PCM i16 audio samples into the call (for audio-native model output).
    pub async fn inject_audio(&self, samples: Vec<i16>) -> Result<(), String> {
        self.speak(samples).await
    }

    /// Publish a video frame to the avatar track.
    /// Accepts RGBA8 pixel data and converts to I420 for LiveKit.
    pub fn publish_video_frame(&self, _rgba: &[u8], width: u32, height: u32) {
        // TODO: Convert RGBA to I420 VideoFrame and call video_source.capture_frame()
        // For now this is a placeholder — avatar rendering will be wired in later.
        let _ = (width, height);
    }

    /// Add a named ambient audio source (hold music, background noise, etc.)
    pub async fn add_ambient_source(&self, name: &str) -> Result<String, String> {
        let source = NativeAudioSource::new(
            AudioSourceOptions::default(),
            AUDIO_SAMPLE_RATE,
            1,
            1000,
        );

        // Publish as a separate audio track
        let track = LocalAudioTrack::create_audio_track(
            &format!("{}-ambient-{}", self.identity, name),
            RtcAudioSource::Native(source.clone()),
        );
        self.room
            .local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                TrackPublishOptions {
                    source: TrackSource::Unknown,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("Failed to publish ambient track: {}", e))?;

        let handle = uuid::Uuid::new_v4().to_string();
        self.ambient_sources
            .lock()
            .await
            .insert(handle.clone(), source);

        info!("🎵 Added ambient source '{}' (handle: {})", name, &handle[..8]);
        Ok(handle)
    }

    /// Inject audio into an ambient source by handle.
    pub async fn inject_ambient(&self, handle: &str, samples: Vec<i16>) -> Result<(), String> {
        let sources = self.ambient_sources.lock().await;
        let source = sources
            .get(handle)
            .ok_or_else(|| format!("Ambient source not found: {}", &handle[..8.min(handle.len())]))?;

        let chunk_size = SAMPLES_PER_10MS as usize;
        for chunk in samples.chunks(chunk_size) {
            let frame = AudioFrame {
                data: Cow::Borrowed(chunk),
                sample_rate: AUDIO_SAMPLE_RATE,
                num_channels: 1,
                samples_per_channel: chunk.len() as u32,
            };
            source
                .capture_frame(&frame)
                .await
                .map_err(|e| format!("Failed to capture ambient frame: {}", e))?;
        }

        Ok(())
    }

    /// Remove an ambient audio source.
    pub async fn remove_ambient_source(&self, handle: &str) -> Result<(), String> {
        let mut sources = self.ambient_sources.lock().await;
        if sources.remove(handle).is_some() {
            // Track will be unpublished when the source is dropped
            info!("🎵 Removed ambient source (handle: {})", &handle[..8.min(handle.len())]);
            Ok(())
        } else {
            Err(format!("Ambient source not found: {}", &handle[..8.min(handle.len())]))
        }
    }

    /// Publish a transcription segment to all room participants via LiveKit's native transcription API.
    pub async fn publish_transcription(
        &self,
        text: &str,
        speaker_id: &str,
        is_final: bool,
    ) -> Result<(), String> {
        publish_transcription_to_room(&self.room, text, speaker_id, speaker_id, is_final).await
    }

    /// Disconnect from the room.
    pub async fn disconnect(self) {
        info!("🔊 LiveKitAgent '{}' disconnecting", self.identity);
        let _ = self.room.close().await;
    }

    /// Get the room name this agent is connected to.
    pub fn room_name(&self) -> String {
        self.room.name().to_string()
    }

    /// Get this agent's identity in the room.
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Generate a LiveKit JWT token for a given identity and room.
fn generate_token(
    identity: &str,
    name: &str,
    room: &str,
    can_publish: bool,
) -> Result<String, String> {
    let api_key = get_secret("LIVEKIT_API_KEY").unwrap_or(LIVEKIT_DEV_KEY);
    let api_secret = get_secret("LIVEKIT_API_SECRET").unwrap_or(LIVEKIT_DEV_SECRET);
    AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_name(name)
        .with_grants(VideoGrants {
            room_join: true,
            room: room.to_string(),
            can_publish,
            can_subscribe: true,
            can_publish_data: true,
            ..Default::default()
        })
        .to_jwt()
        .map_err(|e| format!("Failed to generate LiveKit token: {}", e))
}

/// Publish a transcription to a LiveKit room.
async fn publish_transcription_to_room(
    room: &Room,
    text: &str,
    speaker_id: &str,
    speaker_name: &str,
    _is_final: bool,
) -> Result<(), String> {
    // Send transcription via data channel (topic: "transcription")
    // The STT listener doesn't publish audio tracks, so LiveKit's native
    // transcription API requires a track SID we don't have.
    // Data channel is reliable and guaranteed to reach all participants.
    let payload = serde_json::json!({
        "speaker_id": speaker_id,
        "speaker_name": speaker_name,
        "text": text,
        "language": "en",
        "final": true,
    });
    let bytes = payload.to_string().into_bytes();

    room.local_participant()
        .publish_data(DataPacket {
            payload: bytes.into(),
            topic: Some("transcription".to_string()),
            reliable: true,
            ..Default::default()
        })
        .await
        .map_err(|e| format!("Failed to publish transcription data: {}", e))?;

    info!("📝 Published transcription to room: \"{}\" (speaker={})",
        &text[..40.min(text.len())], &speaker_id[..8.min(speaker_id.len())]);
    Ok(())
}

/// Process incoming audio from a remote participant's track using ProductionVAD.
/// Detects sentence boundaries and emits Utterance events for STT processing.
async fn process_audio_stream_with_vad(
    audio_track: RemoteAudioTrack,
    speaker_id: String,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
) {
    use livekit::webrtc::audio_stream::native::NativeAudioStream;
    use tokio_stream::StreamExt;
    use crate::voice::vad::ProductionVAD;

    let mut audio_stream = NativeAudioStream::new(
        audio_track.rtc_track(),
        AUDIO_SAMPLE_RATE as i32,
        1, // mono
    );

    // Initialize ProductionVAD for proper sentence detection
    let mut vad = ProductionVAD::new();
    if let Err(e) = vad.initialize() {
        error!("🎤 Failed to initialize VAD for '{}': {}", speaker_id, e);
        return;
    }

    info!("🎤 VAD initialized for audio stream from '{}'", speaker_id);

    while let Some(frame) = audio_stream.next().await {
        let samples: &[i16] = frame.data.as_ref();

        match vad.process_frame(samples) {
            Ok(Some(sentence_samples)) => {
                // Complete sentence detected by VAD — emit for STT
                info!(
                    "🎤 Sentence detected from '{}' ({} samples, {:.1}s)",
                    speaker_id,
                    sentence_samples.len(),
                    sentence_samples.len() as f64 / AUDIO_SAMPLE_RATE as f64,
                );
                let _ = event_tx.send(AgentEvent::Utterance {
                    speaker_id: speaker_id.clone(),
                    samples: sentence_samples,
                });
            }
            Ok(None) => {} // Still buffering — VAD hasn't detected sentence end yet
            Err(e) => {
                warn!("🎤 VAD error for '{}': {}", speaker_id, e);
            }
        }
    }

    info!("🎤 Audio stream ended for '{}'", speaker_id);
}

// =============================================================================
// STT Listener — subscribe-only agent for VAD → STT → transcription
// =============================================================================

/// Spawn an STT listener agent for a call.
///
/// This agent joins the LiveKit room with `__stt__` prefix identity,
/// subscribes to all human audio tracks, runs VAD → STT, and publishes
/// transcriptions via LiveKit's native transcription API.
///
/// The browser filters out `__stt__` participants from the UI grid.
async fn spawn_stt_listener(
    livekit_url: &str,
    call_id: &str,
    transcription_buffer: TranscriptionBuffer,
) -> Result<Arc<Room>, String> {
    let listener_id = format!("{}{}", STT_LISTENER_PREFIX, &call_id[..8.min(call_id.len())]);
    let token = generate_token(&listener_id, "STT", call_id, true)?;

    let (room, mut room_events) = Room::connect(livekit_url, &token, RoomOptions::default())
        .await
        .map_err(|e| format!("Failed to connect STT listener: {}", e))?;

    info!("🎤 STT listener connected to room '{}' as '{}'", call_id, listener_id);

    let room = Arc::new(room);
    let room_for_events = room.clone();
    let call_id_owned = call_id.to_string();

    // Spawn room event handler — subscribes to audio tracks and processes them
    tokio::spawn(async move {
        while let Some(event) = room_events.recv().await {
            match event {
                RoomEvent::TrackSubscribed {
                    track,
                    publication: _,
                    participant,
                } => {
                    let speaker_id = participant.identity().to_string();
                    let speaker_name = participant.name().to_string();

                    // Skip tracks from system/AI agents (STT listeners, ambient sources, AI persona agents)
                    // Without this, AI TTS audio gets transcribed back → infinite echo loop
                    if speaker_id.starts_with(STT_LISTENER_PREFIX)
                        || speaker_id.starts_with("ambient-")
                        || speaker_id.starts_with(AI_AGENT_PREFIX) {
                        continue;
                    }

                    if let RemoteTrack::Audio(audio_track) = track {
                        info!("🎤 STT listener: subscribed to audio from '{}' ({})",
                            speaker_name, speaker_id);

                        let room_ref = room_for_events.clone();
                        let cid = call_id_owned.clone();
                        let tbuf = transcription_buffer.clone();
                        tokio::spawn(async move {
                            listen_and_transcribe(
                                audio_track,
                                speaker_id,
                                speaker_name,
                                room_ref,
                                cid,
                                tbuf,
                            ).await;
                        });
                    }
                }
                RoomEvent::Disconnected { reason } => {
                    info!("🎤 STT listener disconnected from '{}': {:?}",
                        call_id_owned, reason);
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(room)
}

/// Process a single audio track: VAD → STT → publish transcription → notify AI.
///
/// Runs in its own tokio task. One instance per human participant per call.
async fn listen_and_transcribe(
    audio_track: RemoteAudioTrack,
    speaker_id: String,
    speaker_name: String,
    room: Arc<Room>,
    call_id: String,
    transcription_buffer: TranscriptionBuffer,
) {
    use livekit::webrtc::audio_stream::native::NativeAudioStream;
    use tokio_stream::StreamExt;
    use crate::voice::vad::ProductionVAD;
    use crate::voice::stt_service;

    let mut audio_stream = NativeAudioStream::new(
        audio_track.rtc_track(),
        AUDIO_SAMPLE_RATE as i32,
        1, // mono
    );

    // Initialize ProductionVAD — two-stage (WebRTC fast filter → Silero confirmation)
    let mut vad = ProductionVAD::new();
    if let Err(e) = vad.initialize() {
        error!("🎤 STT: Failed to init VAD for '{}': {}", speaker_name, e);
        return;
    }

    info!("🎤 STT: VAD initialized, listening to '{}'", speaker_name);

    // Transcription semaphore: max 2 concurrent STT operations per speaker
    let semaphore = Arc::new(tokio::sync::Semaphore::new(2));
    let mut frame_count: u64 = 0;

    // Frame accumulation buffer: LiveKit sends 10ms (160 samples) frames,
    // but our VAD (earshot WebRTC) requires 240-sample minimum chunks.
    // Accumulate to 480 samples (30ms) before feeding to VAD.
    const VAD_FRAME_SIZE: usize = 480;
    let mut accum_buf: Vec<i16> = Vec::with_capacity(VAD_FRAME_SIZE);

    while let Some(frame) = audio_stream.next().await {
        let samples: &[i16] = frame.data.as_ref();
        frame_count += 1;

        // Log first frame + every 3000th frame
        if frame_count == 1 || frame_count % 3000 == 0 {
            let max_amp = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
            info!("🎤 STT: Frame #{} from '{}' — {} samples, max_amp={}, sr={}",
                frame_count, speaker_name, samples.len(), max_amp, frame.sample_rate);
        }

        // Accumulate until we have a full VAD frame
        accum_buf.extend_from_slice(samples);
        if accum_buf.len() < VAD_FRAME_SIZE {
            continue;
        }

        // Drain accumulated buffer in VAD_FRAME_SIZE chunks
        while accum_buf.len() >= VAD_FRAME_SIZE {
            let vad_frame: Vec<i16> = accum_buf.drain(..VAD_FRAME_SIZE).collect();

            match vad.process_frame(&vad_frame) {
                Ok(Some(sentence_samples)) => {
                    let sample_count = sentence_samples.len();
                    let duration_s = sample_count as f64 / AUDIO_SAMPLE_RATE as f64;
                    info!(
                        "🎤 STT: Sentence from '{}' ({} samples, {:.1}s)",
                        speaker_name, sample_count, duration_s
                    );

                    // Acquire semaphore (non-blocking — drop if at capacity)
                    let permit = match semaphore.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            warn!("🎤 STT: Dropping utterance from '{}' — transcription queue full",
                                speaker_name);
                            continue;
                        }
                    };

                    let sid = speaker_id.clone();
                    let sname = speaker_name.clone();
                    let room_ref = room.clone();
                    let cid = call_id.clone();
                    let tbuf = transcription_buffer.clone();

                    tokio::spawn(async move {
                        let _permit = permit; // Hold until done

                        match stt_service::transcribe_speech_async(&sentence_samples, Some("en")).await {
                            Ok(transcript) => {
                                let text = transcript.text.trim();
                                if text.is_empty() {
                                    return;
                                }

                                let display_len = 60.min(text.len());
                                info!("📝 STT: {} said: \"{}{}\"",
                                    sname,
                                    &text[..display_len],
                                    if text.len() > 60 { "..." } else { "" },
                                );

                                // Store in transcription buffer (for test polling via voice/poll-transcriptions)
                                {
                                    let mut buf = tbuf.lock().await;
                                    buf.push_back(TranscriptionEntry {
                                        call_id: cid.clone(),
                                        speaker_id: sid.clone(),
                                        speaker_name: sname.clone(),
                                        text: text.to_string(),
                                        timestamp_ms: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_millis() as u64,
                                    });
                                    while buf.len() > MAX_TRANSCRIPTION_BUFFER {
                                        buf.pop_front();
                                    }
                                }

                                // 1. Publish to LiveKit room (browser subtitles)
                                if let Err(e) = publish_transcription_to_room(
                                    &room_ref, text, &sid, &sname, true,
                                ).await {
                                    warn!("📝 STT: Failed to publish transcription: {}", e);
                                }

                                // 2. Notify TS server for AI response routing
                                //    Calls collaboration/live/transcription → VoiceOrchestrator → PersonaUser inbox
                                let ts_params = serde_json::json!({
                                    "callSessionId": cid,
                                    "speakerId": sid,
                                    "speakerName": sname,
                                    "transcript": text,
                                    "confidence": 1.0,
                                    "language": "en",
                                    "timestamp": std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as u64,
                                });
                                match crate::runtime::command_executor::execute_ts_json(
                                    "collaboration/live/transcription", ts_params
                                ).await {
                                    Ok(result) => {
                                        info!("📝 STT: AI routing result: {}", result);
                                    }
                                    Err(e) => {
                                        warn!("📝 STT: Failed to route transcription to AI: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("📝 STT: Transcription failed for '{}': {}", sname, e);
                            }
                        }
                    });
                }
                Ok(None) => {} // Still buffering
                Err(e) => {
                    warn!("🎤 STT: VAD error for '{}': {}", speaker_name, e);
                }
            }
        } // end inner while (accum_buf drain)
    } // end outer while (audio_stream)

    info!("🎤 STT: Audio stream ended for '{}'", speaker_name);
}

// =============================================================================
// LiveKitAgentManager — manages agents across calls (replaces CallManager role)
// =============================================================================

use tokio::sync::RwLock;

/// Key for agent lookup: (call_id, user_id)
type AgentKey = (String, String);

/// Manages LiveKitAgents across all active calls.
/// Drop-in replacement for CallManager's role in VoiceModule.
pub struct LiveKitAgentManager {
    /// Active agents keyed by (call_id, user_id)
    agents: Arc<RwLock<HashMap<AgentKey, Arc<LiveKitAgent>>>>,
    /// Active STT listener rooms keyed by call_id
    listeners: Arc<RwLock<HashMap<String, Arc<Room>>>>,
    /// LiveKit server URL
    livekit_url: String,
    /// Transcription buffer from STT listeners (polled by tests via voice/poll-transcriptions)
    transcription_buffer: TranscriptionBuffer,
}

impl LiveKitAgentManager {
    /// Create manager, resolving LiveKit URL from secrets (config.env) with dev fallback.
    pub fn new() -> Self {
        let default_url = format!("ws://127.0.0.1:{}", LIVEKIT_PORT);
        let livekit_url = get_secret("LIVEKIT_URL")
            .map(|s| s.to_string())
            .unwrap_or(default_url);

        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            listeners: Arc::new(RwLock::new(HashMap::new())),
            livekit_url,
            transcription_buffer: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Get the LiveKit server URL this manager is configured for.
    pub fn url(&self) -> &str {
        &self.livekit_url
    }

    /// Join a call as an STT listener. Subscribes to all audio tracks,
    /// runs VAD → STT, and publishes transcriptions via LiveKit.
    ///
    /// Called when `voice/register-session` fires (human joins a call).
    /// Idempotent — no-op if already listening on this call.
    pub async fn join_as_listener(&self, call_id: &str) -> Result<(), String> {
        // Check if already listening
        {
            let listeners = self.listeners.read().await;
            if listeners.contains_key(call_id) {
                info!("🎤 STT listener already active for call {}", &call_id[..8.min(call_id.len())]);
                return Ok(());
            }
        }

        let room = spawn_stt_listener(&self.livekit_url, call_id, self.transcription_buffer.clone()).await?;
        self.listeners.write().await.insert(call_id.to_string(), room);

        info!("🎤 STT listener registered for call {}", &call_id[..8.min(call_id.len())]);
        Ok(())
    }

    /// Get or create an agent for a persona in a call.
    /// If the agent doesn't exist yet, connects to LiveKit.
    async fn get_or_create_agent(
        &self,
        call_id: &str,
        user_id: &str,
    ) -> Result<Arc<LiveKitAgent>, String> {
        let key = (call_id.to_string(), user_id.to_string());

        // Fast path: agent already exists
        {
            let agents = self.agents.read().await;
            if let Some(agent) = agents.get(&key) {
                return Ok(agent.clone());
            }
        }

        // Slow path: create new agent
        // Use __ai__ prefix so STT listener skips this participant (prevents echo loop)
        let ai_identity = format!("{}{}", AI_AGENT_PREFIX, user_id);
        let (agent, _event_rx) = LiveKitAgent::connect(
            &self.livekit_url,
            call_id,
            &ai_identity,
            user_id, // display name = user_id for now (caller should provide better name)
        ).await?;

        let agent = Arc::new(agent);
        self.agents.write().await.insert(key, agent.clone());

        // Speaking agents don't process their own event_rx — the STT listener
        // handles all incoming audio processing centrally (one per call).

        Ok(agent)
    }

    /// Synthesize TTS and inject into a call (replaces CallManager::speak_in_call).
    pub async fn speak_in_call(
        &self,
        call_id: &str,
        user_id: &str,
        text: &str,
        voice: Option<&str>,
        adapter: Option<&str>,
    ) -> Result<(usize, u64, u32), String> {
        use crate::voice::tts_service;

        let synthesis = tts_service::synthesize_speech_async(
            text,
            voice,
            adapter,
        ).await.map_err(|e| format!("TTS synthesis failed: {}", e))?;

        let num_samples = synthesis.samples.len();
        let duration_ms = synthesis.duration_ms;
        let sample_rate = synthesis.sample_rate;

        let agent = self.get_or_create_agent(call_id, user_id).await?;
        agent.speak(synthesis.samples).await?;

        // Publish AI response text as subtitle (same data channel as human transcriptions)
        if let Err(e) = agent.publish_transcription(text, user_id, true).await {
            warn!("🤖 Failed to publish AI subtitle for {}: {}", &user_id[..8.min(user_id.len())], e);
        }

        Ok((num_samples, duration_ms, sample_rate))
    }

    /// Inject raw audio samples into a call (replaces CallManager::inject_audio).
    pub async fn inject_audio(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let agent = self.get_or_create_agent(call_id, user_id).await?;
        agent.inject_audio(samples).await
    }

    /// Add an ambient audio source to a call (replaces CallManager::add_ambient_source).
    pub async fn add_ambient_source(
        &self,
        call_id: &str,
        source_name: &str,
    ) -> Result<String, String> {
        // Use a default agent identity for ambient sources
        let agent_id = format!("ambient-{}", call_id);
        let agent = self.get_or_create_agent(call_id, &agent_id).await?;
        agent.add_ambient_source(source_name).await
    }

    /// Inject audio into an ambient source (replaces CallManager::inject_audio_by_handle).
    pub async fn inject_ambient(
        &self,
        call_id: &str,
        handle: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        // Search all agents in this call for the ambient handle
        let agents = self.agents.read().await;
        for ((cid, _), agent) in agents.iter() {
            if cid == call_id {
                // Try to inject — if the handle isn't on this agent, it'll error
                match agent.inject_ambient(handle, samples.clone()).await {
                    Ok(()) => return Ok(()),
                    Err(_) => continue,
                }
            }
        }
        Err(format!("Ambient handle not found in call {}", call_id))
    }

    /// Poll and drain the transcription buffer (for tests).
    /// Returns all transcriptions since the last poll, optionally filtered by call_id.
    pub async fn poll_transcriptions(&self, call_id: Option<&str>) -> Vec<TranscriptionEntry> {
        let mut buf = self.transcription_buffer.lock().await;
        if let Some(cid) = call_id {
            let (matching, remaining): (VecDeque<_>, VecDeque<_>) = buf.drain(..)
                .partition(|e| e.call_id == cid);
            *buf = remaining;
            matching.into_iter().collect()
        } else {
            buf.drain(..).collect()
        }
    }

    /// Remove an ambient audio source (replaces CallManager::remove_ambient_source).
    pub async fn remove_ambient_source(
        &self,
        call_id: &str,
        handle: &str,
    ) -> Result<(), String> {
        let agents = self.agents.read().await;
        for ((cid, _), agent) in agents.iter() {
            if cid == call_id {
                match agent.remove_ambient_source(handle).await {
                    Ok(()) => return Ok(()),
                    Err(_) => continue,
                }
            }
        }
        Err(format!("Ambient handle not found in call {}", call_id))
    }

    /// Start continuous ambient background audio for a call.
    /// Spawns a tokio task that generates rain noise and publishes it
    /// through a dedicated LiveKit agent. Runs until no other agents remain.
    pub async fn start_ambient_audio(&self, call_id: &str) -> Result<(), String> {
        let cid = call_id.to_string();
        let agents_ref = self.agents.clone();
        let url = self.livekit_url.clone();

        tokio::spawn(async move {
            if let Err(e) = run_ambient_audio_loop(&agents_ref, &url, &cid).await {
                warn!("🎵 Ambient audio error for call {}: {}", &cid[..8.min(cid.len())], e);
            }
        });

        Ok(())
    }
}

/// Generate and publish continuous ambient rain audio into a LiveKit room.
/// Runs until no other participants remain in the room.
async fn run_ambient_audio_loop(
    _agents: &Arc<RwLock<HashMap<AgentKey, Arc<LiveKitAgent>>>>,
    livekit_url: &str,
    call_id: &str,
) -> Result<(), String> {
    use crate::voice::vad::test_audio::TestAudioGenerator;

    let identity = format!("ambient-bg-{}", &call_id[..8.min(call_id.len())]);
    let token = generate_token(&identity, "Background Audio", call_id, true)?;

    let (room, _events) = Room::connect(livekit_url, &token, RoomOptions::default())
        .await
        .map_err(|e| format!("Failed to connect ambient agent: {}", e))?;

    // Create and publish audio source
    let source = NativeAudioSource::new(
        AudioSourceOptions::default(),
        AUDIO_SAMPLE_RATE,
        1,
        1000, // 1 second queue
    );
    let track = LocalAudioTrack::create_audio_track(
        "ambient-rain",
        RtcAudioSource::Native(source.clone()),
    );
    room.local_participant()
        .publish_track(LocalTrack::Audio(track), TrackPublishOptions::default())
        .await
        .map_err(|e| format!("Failed to publish ambient track: {}", e))?;

    info!("🎵 Ambient audio started for call {} (rain)", &call_id[..8.min(call_id.len())]);

    // Wait for other participants (human, STT listener) to join the room
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE as u32);
    let chunk_duration_ms: u64 = 100; // 100ms chunks
    let chunk_samples = (AUDIO_SAMPLE_RATE as usize * chunk_duration_ms as usize) / 1000;

    // Background volume: -20dB (0.1x amplitude)
    let volume_scale: f32 = 0.1;

    // Track consecutive empty checks — stop only after sustained absence
    let mut empty_checks: u32 = 0;

    loop {
        // Check if call is still active via the LiveKit room's own participant list.
        // This is authoritative — includes human users, STT listeners, any server agents.
        let participant_count = room.remote_participants().len();
        if participant_count == 0 {
            empty_checks += 1;
            // Wait for 5 consecutive empty checks (5 seconds) before stopping
            if empty_checks >= 5 {
                info!("🎵 Ambient audio stopping — room empty for call {}", &call_id[..8.min(call_id.len())]);
                break;
            }
        } else {
            empty_checks = 0;
        }

        // Generate rain noise
        let mut samples = gen.generate_rain(chunk_samples);
        for s in samples.iter_mut() {
            *s = (*s as f32 * volume_scale) as i16;
        }

        // Publish in 10ms frames
        let frame_size = SAMPLES_PER_10MS as usize;
        for frame_chunk in samples.chunks(frame_size) {
            let frame = AudioFrame {
                data: Cow::Borrowed(frame_chunk),
                sample_rate: AUDIO_SAMPLE_RATE as u32,
                num_channels: 1,
                samples_per_channel: frame_chunk.len() as u32,
            };
            if let Err(e) = source.capture_frame(&frame).await {
                warn!("🎵 Ambient frame error: {}", e);
                break;
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(chunk_duration_ms)).await;
    }

    room.close().await.ok();
    info!("🎵 Ambient audio stopped for call {}", &call_id[..8.min(call_id.len())]);
    Ok(())
}
