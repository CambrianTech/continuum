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
//!   - Role set via ParticipantMetadata (stt_listener) — browser filters by metadata
//!
//! Audio format: 16kHz mono i16 PCM — matches our TTS output and LiveKit's AudioFrame.data (Cow<[i16]>).

use crate::audio_constants::{AUDIO_SAMPLE_RATE, LIVEKIT_DEV_KEY, LIVEKIT_DEV_SECRET, LIVEKIT_PORT};
use crate::secrets::get_secret;

use livekit::prelude::*;
use livekit::options::{TrackPublishOptions, VideoEncoding};
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit_api::access_token::{AccessToken, VideoGrants};

use std::borrow::Cow;
use std::collections::HashMap;
use tokio::sync::{mpsc, Mutex};
use std::collections::VecDeque;
use std::sync::Arc;
use crate::{clog_info, clog_warn, clog_error};

// =============================================================================
// Participant metadata — typed role classification instead of string prefixes.
// Serialized as JSON in the LiveKit JWT token's metadata field.
// Browser reads via participant.metadata (livekit-client JS SDK).
// Must match src/shared/LiveKitTypes.ts enum values exactly.
// =============================================================================

/// LiveKit participant role — determines audio routing and UI visibility.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    /// Human user with microphone/camera
    Human,
    /// AI persona agent — publishes TTS audio, visible in participant grid
    AiPersona,
    /// STT listener — subscribe-only for VAD/STT, invisible in UI
    SttListener,
    /// Ambient audio source (rain, hold music) — invisible in UI
    AmbientAudio,
}

/// Metadata attached to every LiveKit participant via JWT token.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParticipantMetadata {
    pub role: ParticipantRole,
}

impl ParticipantMetadata {
    pub fn new(role: ParticipantRole) -> Self {
        Self { role }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

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

/// STT listener identity prefix (for descriptive LiveKit room identities only).
/// Role classification uses ParticipantMetadata, NOT these prefixes.
const STT_IDENTITY_PREFIX: &str = "stt-";
/// Ambient audio identity prefix (descriptive only).
const AMBIENT_IDENTITY_PREFIX: &str = "ambient-";

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

/// Lazily-initialized video publishing state.
/// Holds the NativeVideoSource, created on first frame to avoid streaming
/// uninitialized buffer data before the renderer is ready.
struct VideoPublishState {
    source: Option<NativeVideoSource>,
}

impl VideoPublishState {
    fn new() -> Self {
        Self { source: None }
    }
}

/// Server-side LiveKit participant that bridges our AI pipeline to WebRTC.
pub struct LiveKitAgent {
    room: Room,
    /// Primary audio source for TTS output
    audio_source: NativeAudioSource,
    /// SID of the published audio track — reserved for future native transcription
    /// with word-by-word timing (start_time/end_time per segment).
    _audio_track_sid: String,
    /// Video source for avatar rendering — lazily created on first publish_video_frame() call.
    /// Not created at connect() time to avoid publishing uninitialized garbled frames.
    video_state: Mutex<VideoPublishState>,
    /// Additional audio sources for ambient/background audio (hold music, etc.)
    ambient_sources: Arc<Mutex<HashMap<String, NativeAudioSource>>>,
    /// Agent events channel — kept alive to prevent receiver from closing.
    /// Sends happen on clones (event_tx_clone in room event handler).
    _event_tx: mpsc::UnboundedSender<AgentEvent>,
    /// Identity of this agent in the LiveKit room
    identity: String,
    /// Display name (persona name) for transcription attribution
    display_name: String,
    /// TTS voice name — set on first speak, used for gender-matched avatar selection.
    /// First-speak-wins: once set, the avatar doesn't change mid-call.
    voice_name: std::sync::Mutex<Option<String>>,
}

impl LiveKitAgent {
    /// Connect to a LiveKit room as a server-side participant.
    ///
    /// Publishes an audio track immediately (for TTS output).
    /// Video track is deferred — created lazily on first publish_video_frame() call
    /// to avoid streaming uninitialized buffer data (garbled frames).
    ///
    /// Returns the agent and an event receiver for incoming audio/participant events.
    pub async fn connect(
        livekit_url: &str,
        call_id: &str,
        persona_id: &str,
        persona_name: &str,
    ) -> Result<(Self, mpsc::UnboundedReceiver<AgentEvent>), String> {
        // Generate access token with metadata for role classification
        let metadata = ParticipantMetadata::new(ParticipantRole::AiPersona);
        let api_key = get_secret("LIVEKIT_API_KEY").unwrap_or(LIVEKIT_DEV_KEY);
        let api_secret = get_secret("LIVEKIT_API_SECRET").unwrap_or(LIVEKIT_DEV_SECRET);
        let token = AccessToken::with_api_key(api_key, api_secret)
            .with_identity(persona_id)
            .with_name(persona_name)
            .with_metadata(&metadata.to_json())
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

        clog_info!(
            "🔊 LiveKitAgent '{}' connected to room '{}' (role=ai_persona)",
            persona_name, call_id
        );

        // Create audio source for TTS output (16kHz mono, 1s buffer)
        let audio_source = NativeAudioSource::new(
            AudioSourceOptions::default(),
            AUDIO_SAMPLE_RATE,
            1, // mono
            1000, // 1 second queue
        );

        // Publish TTS audio track immediately — capture SID for transcription sync
        let audio_track = LocalAudioTrack::create_audio_track(
            &format!("{}-voice", persona_id),
            RtcAudioSource::Native(audio_source.clone()),
        );
        let audio_publication = room.local_participant()
            .publish_track(
                LocalTrack::Audio(audio_track),
                TrackPublishOptions {
                    source: TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("Failed to publish audio track: {}", e))?;

        let audio_track_sid: String = audio_publication.sid().into();
        clog_info!("🔊 Audio track published with SID: {}", audio_track_sid);

        // Video source is NOT created here — deferred to first publish_video_frame().
        // Publishing an empty NativeVideoSource streams uninitialized buffer memory
        // which renders as garbled patterns in the browser (looks like broken vsync).
        // The browser shows avatar circles for participants without video tracks.

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let event_tx_clone = event_tx.clone();
        let identity = persona_id.to_string();
        let identity_for_events = identity.clone();

        // Spawn event handler task — routes room events to the agent's event channel.
        // Uses participant metadata for role classification, not identity string matching.
        tokio::spawn(async move {
            // Hysteresis state: track current tier and last downgrade request.
            // Upgrades (larger) are immediate; downgrades require 2s of stability.
            use crate::voice::avatar::ResolutionTier;
            let mut current_tier: ResolutionTier = ResolutionTier::Large;
            let mut pending_downgrade: Option<(ResolutionTier, tokio::time::Instant)> = None;
            const DOWNGRADE_HOLDOFF: tokio::time::Duration = tokio::time::Duration::from_secs(2);

            while let Some(event) = room_events.recv().await {
                match event {
                    RoomEvent::DataReceived { payload, topic, .. } => {
                        if topic.as_deref() == Some("tile_resolution") {
                            // Parse JSON: { "persona-uuid": TileResolution, ... }
                            // TileResolution is the ts-rs shared type (voice/types.rs)
                            if let Ok(text) = std::str::from_utf8(&payload) {
                                if let Ok(data) = serde_json::from_str::<HashMap<String, crate::voice::types::TileResolution>>(text) {
                                    if let Some(dims) = data.get(&identity_for_events) {
                                        let requested_tier = ResolutionTier::from_tile_width(dims.w);

                                        if requested_tier == current_tier {
                                            pending_downgrade = None;
                                            continue;
                                        }

                                        let requested_pixels = {
                                            let (rw, rh) = requested_tier.dimensions();
                                            rw * rh
                                        };
                                        let current_pixels = {
                                            let (cw, ch) = current_tier.dimensions();
                                            cw * ch
                                        };

                                        if requested_pixels >= current_pixels {
                                            // Upgrade: apply immediately
                                            let (rw, rh) = requested_tier.dimensions();
                                            if let Some(bevy_system) = crate::voice::bevy_renderer::try_get() {
                                                bevy_system.resize_by_identity(&identity_for_events, rw, rh);
                                            }
                                            current_tier = requested_tier;
                                            pending_downgrade = None;
                                            clog_info!("🎨 Tier upgrade for '{}': {:?} ({}x{})",
                                                &identity_for_events[..8.min(identity_for_events.len())],
                                                requested_tier, rw, rh);
                                        } else {
                                            // Downgrade: require 2s hold-down
                                            match &pending_downgrade {
                                                Some((pending_tier, since)) if *pending_tier == requested_tier => {
                                                    if since.elapsed() >= DOWNGRADE_HOLDOFF {
                                                        let (rw, rh) = requested_tier.dimensions();
                                                        if let Some(bevy_system) = crate::voice::bevy_renderer::try_get() {
                                                            bevy_system.resize_by_identity(&identity_for_events, rw, rh);
                                                        }
                                                        current_tier = requested_tier;
                                                        pending_downgrade = None;
                                                        clog_info!("🎨 Tier downgrade for '{}': {:?} ({}x{})",
                                                            &identity_for_events[..8.min(identity_for_events.len())],
                                                            requested_tier, rw, rh);
                                                    }
                                                }
                                                _ => {
                                                    pending_downgrade = Some((requested_tier, tokio::time::Instant::now()));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    RoomEvent::TrackSubscribed {
                        track,
                        publication: _,
                        participant,
                    } => {
                        let speaker_id = participant.identity().to_string();
                        let meta = ParticipantMetadata::from_json(&participant.metadata());

                        // Only process audio from human participants
                        let is_human = meta.as_ref()
                            .map(|m| m.role == ParticipantRole::Human)
                            .unwrap_or(true); // Unknown = probably human
                        if !is_human {
                            continue;
                        }

                        clog_info!("🎤 Agent subscribed to track from '{}'", speaker_id);

                        if let RemoteTrack::Audio(audio_track) = track {
                            let tx = event_tx_clone.clone();
                            let sid = speaker_id.clone();
                            tokio::spawn(async move {
                                process_audio_stream_with_vad(audio_track, sid, tx).await;
                            });
                        }
                    }
                    RoomEvent::ParticipantConnected(participant) => {
                        let meta = ParticipantMetadata::from_json(&participant.metadata());
                        let is_visible = meta.as_ref()
                            .map(|m| m.role == ParticipantRole::Human || m.role == ParticipantRole::AiPersona)
                            .unwrap_or(true);
                        if !is_visible { continue; }

                        let name = participant.name().to_string();
                        let id = participant.identity().to_string();
                        clog_info!("👤 Participant joined: {} ({})", name, id);
                        let _ = event_tx_clone.send(AgentEvent::ParticipantJoined {
                            identity: id,
                            name,
                        });
                    }
                    RoomEvent::ParticipantDisconnected(participant) => {
                        let meta = ParticipantMetadata::from_json(&participant.metadata());
                        let is_visible = meta.as_ref()
                            .map(|m| m.role == ParticipantRole::Human || m.role == ParticipantRole::AiPersona)
                            .unwrap_or(true);
                        if !is_visible { continue; }

                        let id = participant.identity().to_string();
                        clog_info!("👤 Participant left: {}", id);
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
            _audio_track_sid: audio_track_sid,
            video_state: Mutex::new(VideoPublishState::new()),
            ambient_sources: Arc::new(Mutex::new(HashMap::new())),
            _event_tx: event_tx,
            identity,
            display_name: persona_name.to_string(),
            voice_name: std::sync::Mutex::new(None),

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
    /// Accepts RGBA8 pixel data. Creates and publishes the video track lazily on first call.
    ///
    /// The video track is NOT created at connect() time — it's published on the first
    /// `publish_video_frame()` call so the browser only shows a video tile when there's
    /// real content to display.
    pub async fn publish_video_frame(&self, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
        let mut state = self.video_state.lock().await;

        // Lazily create and publish video source on first frame
        if state.source.is_none() {
            let video_source = NativeVideoSource::new(
                VideoResolution { width, height },
                false, // is_screencast
            );
            let video_track = LocalVideoTrack::create_video_track(
                &format!("{}-avatar", self.identity),
                RtcVideoSource::Native(video_source.clone()),
            );
            self.room
                .local_participant()
                .publish_track(
                    LocalTrack::Video(video_track),
                    TrackPublishOptions {
                        source: TrackSource::Camera,
                        // Avatar video: server-rendered VGA, one consumer per track.
                        // Disable simulcast (no need for quality layers) and set
                        // explicit bitrate to prevent WebRTC adaptive compression blur.
                        simulcast: false,
                        video_encoding: Some(VideoEncoding {
                            max_bitrate: 800_000, // 800 kbps — VGA@15fps avatar with lip sync
                            max_framerate: 15.0,
                        }),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| format!("Failed to publish video track: {}", e))?;

            clog_info!("📹 Video track published for '{}' ({}x{})", self.identity, width, height);
            state.source = Some(video_source);
        }

        // Convert RGBA to I420 and capture frame.
        // I420Buffer wraps C++ UniquePtr — not reusable across VideoFrame submissions.
        // Phase 2 (FramePublisher) will eliminate this per-agent overhead entirely.
        let source = state.source.as_ref().unwrap();
        let mut buffer = I420Buffer::new(width, height);
        rgba_to_i420_into(rgba, &mut buffer, width, height);
        let frame = VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: 0, // auto-timestamp to current time
            buffer,
        };
        source.capture_frame(&frame);
        Ok(())
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

        clog_info!("🎵 Added ambient source '{}' (handle: {})", name, &handle[..8]);
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
            clog_info!("🎵 Removed ambient source (handle: {})", &handle[..8.min(handle.len())]);
            Ok(())
        } else {
            Err(format!("Ambient source not found: {}", &handle[..8.min(handle.len())]))
        }
    }

    /// Publish a transcription for this agent's speech.
    ///
    /// Uses data channel (topic="transcription") which is proven reliable in the browser.
    /// Native LiveKit transcription API has cross-participant resolution issues in the
    /// browser SDK — the `participant` param in `TranscriptionReceived` can be null,
    /// causing the handler to silently drop it.
    ///
    /// Called BEFORE feeding audio frames so the subtitle arrives at the browser
    /// at the same time as (or slightly before) the first audio frames.
    ///
    /// Future: Switch to native transcription with word-by-word segments
    /// (start_time/end_time) for progressive reveal synced with audio playback.
    pub async fn publish_transcription(
        &self,
        text: &str,
        _speaker_id: &str,
        _is_final: bool,
    ) -> Result<(), String> {
        publish_data_channel_transcription(&self.room, text, &self.identity, &self.display_name).await
    }

    /// Disconnect from the room.
    pub async fn disconnect(self) {
        clog_info!("🔊 LiveKitAgent '{}' disconnecting", self.identity);
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

    /// Set the TTS voice name for gender-matched avatar selection.
    /// First-speak-wins: only sets if not already set (avatar doesn't change mid-call).
    pub fn set_voice(&self, voice: &str) {
        let mut guard = self.voice_name.lock().unwrap();
        if guard.is_none() {
            *guard = Some(voice.to_string());
        }
    }

    /// Get the current voice name, if set.
    pub fn voice_name(&self) -> Option<String> {
        self.voice_name.lock().unwrap().clone()
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Generate a LiveKit JWT token for a given identity and room.
/// Includes participant metadata for role classification.
fn generate_token(
    identity: &str,
    name: &str,
    room: &str,
    can_publish: bool,
    metadata: &ParticipantMetadata,
) -> Result<String, String> {
    let api_key = get_secret("LIVEKIT_API_KEY").unwrap_or(LIVEKIT_DEV_KEY);
    let api_secret = get_secret("LIVEKIT_API_SECRET").unwrap_or(LIVEKIT_DEV_SECRET);
    AccessToken::with_api_key(api_key, api_secret)
        .with_identity(identity)
        .with_name(name)
        .with_metadata(&metadata.to_json())
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

/// Publish a transcription via data channel.
///
/// The STT listener transcribes human speech but doesn't own the human's audio track.
/// Cross-participant native transcriptions have resolution issues on the browser,
/// so human transcriptions use the reliable data channel instead.
async fn publish_data_channel_transcription(
    room: &Room,
    text: &str,
    speaker_id: &str,
    speaker_name: &str,
) -> Result<(), String> {
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

    clog_info!("📝 Published data channel transcription: \"{}\" (speaker={})",
        &text[..40.min(text.len())], &speaker_id[..8.min(speaker_id.len())]);
    Ok(())
}

// =============================================================================
// Audio amplitude analysis — RMS per window for mouth weight animation
// =============================================================================

/// Calculate RMS amplitude per window from PCM i16 samples.
/// Returns a Vec of normalized weights (0.0 to 1.0) for each window.
///
/// Used for amplitude-responsive lip sync — each weight is sent to the Bevy
/// renderer timed to match audio playback, replacing the default sine oscillation.
fn calculate_rms_weights(samples: &[i16], sample_rate: u32, window_ms: u32) -> Vec<f32> {
    let window_samples = (sample_rate as usize * window_ms as usize) / 1000;
    if window_samples == 0 || samples.is_empty() {
        return Vec::new();
    }

    let mut weights = Vec::new();
    let mut max_rms: f64 = 0.0;

    // First pass: calculate raw RMS per window
    let mut raw_rms = Vec::new();
    for chunk in samples.chunks(window_samples) {
        let sum_sq: f64 = chunk.iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();
        let rms = (sum_sq / chunk.len() as f64).sqrt();
        if rms > max_rms {
            max_rms = rms;
        }
        raw_rms.push(rms);
    }

    // Second pass: normalize to 0.0-1.0 using the max RMS as reference
    if max_rms > 0.0 {
        for rms in raw_rms {
            weights.push((rms / max_rms) as f32);
        }
    }

    weights
}

// =============================================================================
// Video avatar rendering — procedural colored circle with persona initial
// =============================================================================

// RGBA → I420 conversion moved to frame_publisher.rs (single source of truth).
// Used by both CpuI420Publisher and the direct publish_video_frame path below.
use crate::voice::avatar::frame_publisher::rgba_to_i420_into;

/// Start a background video frame loop for an agent.
///
/// Uses `avatar::spawn_renderer_loop()` to create a BevyChannelRenderer (3D VRM)
/// and `frame_publisher::create_publisher()` to select the best frame publishing
/// strategy for the current platform.
///
/// The video loop is both renderer-agnostic AND publisher-agnostic:
///   Renderer (Bevy/procedural/...) → RgbaFrame → channel → FramePublisher → LiveKit
///
/// New renderers plug in via AvatarRenderer trait (render_loop.rs).
/// New publishers plug in via FramePublisher trait (frame_publisher.rs).
///
/// spawn_renderer_loop() acquires std::sync::Mutex locks (slot pool, active renderers)
/// that can block for up to 5s waiting for slots. Wrapped in spawn_blocking to avoid
/// starving the tokio runtime.
fn start_video_loop(agent: Arc<LiveKitAgent>) {
    use crate::voice::avatar::{AvatarConfig, select_dynamic_avatar, avatar_model_path, create_publisher};

    let agent_clone = agent.clone();
    tokio::spawn(async move {
        let agent = agent_clone;
        let voice = agent.voice_name();
        let avatar = select_dynamic_avatar(&agent.identity, voice.as_deref());
        let vrm_path = avatar_model_path(&avatar.filename);
        let vrm_model_path = if vrm_path.exists() {
            Some(vrm_path.to_string_lossy().to_string())
        } else {
            clog_warn!("📹 VRM model not found for '{}': {}", agent.identity, vrm_path.display());
            None
        };

        // Skip video loop if no VRM model — only the human user should lack a model
        if vrm_model_path.is_none() {
            clog_info!("📹 No VRM model for '{}', skipping video loop", agent.identity);
            return;
        }

        let width = 640u32;
        let height = 480u32;

        let config = AvatarConfig {
            identity: agent.identity.clone(),
            display_name: agent.display_name.clone(),
            width,
            height,
            fps: 15.0,
            vrm_model_path,
            ..Default::default()
        };

        // spawn_renderer_loop acquires std::sync::Mutex locks (slot pool, active renderers)
        // which can block up to 5s waiting for slot availability. Run off the tokio runtime.
        let renderer_result = tokio::task::spawn_blocking(move || {
            crate::voice::avatar::spawn_renderer_loop(config)
        }).await;

        let (frame_rx, _interval_nanos) = match renderer_result {
            Ok(Some(result)) => result,
            Ok(None) => {
                clog_warn!("📹 No renderer available for '{}', skipping video", agent.identity);
                return;
            }
            Err(e) => {
                clog_warn!("📹 spawn_renderer_loop panicked for '{}': {}", agent.identity, e);
                return;
            }
        };

        // Lazily create video source on first frame (avoid garbled uninitialized frames).
        // The FramePublisher handles format conversion; we just need the NativeVideoSource.
        let video_source = {
            let mut state = agent.video_state.lock().await;
            if state.source.is_none() {
                let vs = NativeVideoSource::new(
                    VideoResolution { width, height },
                    false,
                );
                let video_track = LocalVideoTrack::create_video_track(
                    &format!("{}-avatar", agent.identity),
                    RtcVideoSource::Native(vs.clone()),
                );
                if let Err(e) = agent.room
                    .local_participant()
                    .publish_track(
                        LocalTrack::Video(video_track),
                        TrackPublishOptions {
                            source: TrackSource::Camera,
                            simulcast: false,
                            video_encoding: Some(VideoEncoding {
                                max_bitrate: 800_000,
                                max_framerate: 15.0,
                            }),
                            ..Default::default()
                        },
                    )
                    .await
                {
                    clog_warn!("📹 Failed to publish video track for '{}': {}", agent.identity, e);
                    return;
                }
                clog_info!("📹 Video track published for '{}' ({}x{})", agent.identity, width, height);
                state.source = Some(vs);
            }
            state.source.clone().unwrap()
        };

        // Create platform-appropriate frame publisher.
        // Currently: CpuI420Publisher (all platforms).
        // Phase 3: NativeBufferPublisher (macOS) selected automatically.
        let mut publisher = create_publisher(frame_rx, width, height);
        clog_info!("📹 Video loop started for '{}' → model '{}' ({}) [publisher={}]",
            agent.identity, avatar.name, avatar.filename, publisher.name());

        let frame_interval = std::time::Duration::from_nanos((1_000_000_000.0 / 15.0) as u64);
        loop {
            // try_publish is non-blocking (try_recv + ~1ms I420 conversion).
            // Short enough for a tokio worker — no need for spawn_blocking.
            match publisher.try_publish(&video_source) {
                Ok(true) => {} // Frame published
                Ok(false) => {} // No frame ready
                Err(_) => {
                    // Channel closed — renderer thread exited
                    clog_info!("📹 Video publisher channel closed for '{}'", agent.identity);
                    break;
                }
            }

            tokio::time::sleep(frame_interval).await;
        }
    });
}

/// Process incoming audio from a remote participant's track using ProductionVAD.
/// Detects sentence boundaries and emits Utterance events for STT processing.
///
/// LiveKit sends 10ms frames (160 samples at 16kHz), but earshot WebRTC VAD
/// requires minimum 240 samples. We accumulate to 480-sample (30ms) chunks.
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
        clog_error!("🎤 Failed to initialize VAD for '{}': {}", speaker_id, e);
        return;
    }

    clog_info!("🎤 VAD initialized for audio stream from '{}'", speaker_id);

    // Frame accumulation: LiveKit sends 10ms (160 samples) frames,
    // but earshot WebRTC VAD requires >=240 samples per call.
    // Accumulate to 480 samples (30ms) before feeding to VAD.
    const VAD_FRAME_SIZE: usize = 480;
    let mut accum_buf: Vec<i16> = Vec::with_capacity(VAD_FRAME_SIZE);

    while let Some(frame) = audio_stream.next().await {
        let samples: &[i16] = frame.data.as_ref();

        accum_buf.extend_from_slice(samples);
        if accum_buf.len() < VAD_FRAME_SIZE {
            continue;
        }

        // Drain accumulated buffer in VAD_FRAME_SIZE chunks
        while accum_buf.len() >= VAD_FRAME_SIZE {
            let vad_frame: Vec<i16> = accum_buf.drain(..VAD_FRAME_SIZE).collect();

            match vad.process_frame(&vad_frame) {
                Ok(Some(sentence_samples)) => {
                    // Complete sentence detected by VAD — emit for STT
                    clog_info!(
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
                    clog_warn!("🎤 VAD error for '{}': {}", speaker_id, e);
                }
            }
        }
    }

    clog_info!("🎤 Audio stream ended for '{}'", speaker_id);
}

// =============================================================================
// STT Listener — subscribe-only agent for VAD → STT → transcription
// =============================================================================

/// Spawn an STT listener agent for a call.
///
/// Joins the LiveKit room with stt_listener role metadata.
/// Subscribes only to human audio tracks (checks participant metadata).
/// Runs VAD → STT and publishes native transcriptions (synced with audio track).
///
/// Browser filters out STT participants via metadata role, not identity prefix.
async fn spawn_stt_listener(
    livekit_url: &str,
    call_id: &str,
    transcription_buffer: TranscriptionBuffer,
) -> Result<Arc<Room>, String> {
    let listener_id = format!("{}{}", STT_IDENTITY_PREFIX, &call_id[..8.min(call_id.len())]);
    let metadata = ParticipantMetadata::new(ParticipantRole::SttListener);
    let token = generate_token(&listener_id, "STT", call_id, true, &metadata)?;

    let (room, mut room_events) = Room::connect(livekit_url, &token, RoomOptions::default())
        .await
        .map_err(|e| format!("Failed to connect STT listener: {}", e))?;

    clog_info!("🎤 STT listener connected to room '{}' as '{}' (role=stt_listener)", call_id, listener_id);

    let room = Arc::new(room);
    let room_for_events = room.clone();
    let call_id_owned = call_id.to_string();

    // Spawn room event handler — subscribes to audio tracks and processes them.
    // Uses participant metadata to determine role. Only transcribes human audio.
    tokio::spawn(async move {
        while let Some(event) = room_events.recv().await {
            match event {
                RoomEvent::TrackSubscribed {
                    track,
                    publication,
                    participant,
                } => {
                    let speaker_id = participant.identity().to_string();
                    let speaker_name = participant.name().to_string();
                    let meta = ParticipantMetadata::from_json(&participant.metadata());

                    // Only transcribe audio from human participants.
                    // AI persona TTS, STT listeners, and ambient audio are skipped.
                    // Without this, AI TTS gets transcribed → infinite echo loop.
                    let is_human = meta.as_ref()
                        .map(|m| m.role == ParticipantRole::Human)
                        .unwrap_or(true); // Unknown metadata = probably human
                    if !is_human {
                        clog_info!("🎤 STT: Skipping non-human track from '{}' (role={:?})",
                            speaker_id, meta.as_ref().map(|m| &m.role));
                        continue;
                    }

                    if let RemoteTrack::Audio(audio_track) = track {
                        // Capture the remote track SID for native transcription sync
                        let track_sid: String = publication.sid().into();
                        clog_info!("🎤 STT listener: subscribed to audio from '{}' ({}) track={}",
                            speaker_name, speaker_id, &track_sid[..8.min(track_sid.len())]);

                        let room_ref = room_for_events.clone();
                        let cid = call_id_owned.clone();
                        let tbuf = transcription_buffer.clone();
                        tokio::spawn(async move {
                            listen_and_transcribe(
                                audio_track,
                                speaker_id,
                                speaker_name,
                                track_sid,
                                room_ref,
                                cid,
                                tbuf,
                            ).await;
                        });
                    }
                }
                RoomEvent::Disconnected { reason } => {
                    clog_info!("🎤 STT listener disconnected from '{}': {:?}",
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
/// `track_sid` is the remote audio track's SID — used for native transcription
/// sync so subtitles align with audio playback in the browser.
async fn listen_and_transcribe(
    audio_track: RemoteAudioTrack,
    speaker_id: String,
    speaker_name: String,
    track_sid: String,
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
        clog_error!("🎤 STT: Failed to init VAD for '{}': {}", speaker_name, e);
        return;
    }

    clog_info!("🎤 STT: VAD initialized, listening to '{}'", speaker_name);

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
            clog_info!("🎤 STT: Frame #{} from '{}' — {} samples, max_amp={}, sr={}",
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
                    clog_info!(
                        "🎤 STT: Sentence from '{}' ({} samples, {:.1}s)",
                        speaker_name, sample_count, duration_s
                    );

                    // Acquire semaphore (non-blocking — drop if at capacity)
                    let permit = match semaphore.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => {
                            clog_warn!("🎤 STT: Dropping utterance from '{}' — transcription queue full",
                                speaker_name);
                            continue;
                        }
                    };

                    let sid = speaker_id.clone();
                    let sname = speaker_name.clone();
                    let _tsid = track_sid.clone();
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
                                clog_info!("📝 STT: {} said: \"{}{}\"",
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

                                // 1. Publish transcription via data channel (human STT)
                                // Uses data channel because the STT listener doesn't own
                                // the human's audio track — native transcription has
                                // cross-participant resolution issues on the browser.
                                if let Err(e) = publish_data_channel_transcription(
                                    &room_ref, text, &sid, &sname,
                                ).await {
                                    clog_warn!("📝 STT: Failed to publish transcription: {}", e);
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
                                        clog_info!("📝 STT: AI routing result: {}", result);
                                    }
                                    Err(e) => {
                                        clog_warn!("📝 STT: Failed to route transcription to AI: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                clog_warn!("📝 STT: Transcription failed for '{}': {}", sname, e);
                            }
                        }
                    });
                }
                Ok(None) => {} // Still buffering
                Err(e) => {
                    clog_warn!("🎤 STT: VAD error for '{}': {}", speaker_name, e);
                }
            }
        } // end inner while (accum_buf drain)
    } // end outer while (audio_stream)

    clog_info!("🎤 STT: Audio stream ended for '{}'", speaker_name);
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
                clog_info!("🎤 STT listener already active for call {}", &call_id[..8.min(call_id.len())]);
                return Ok(());
            }
        }

        let room = spawn_stt_listener(&self.livekit_url, call_id, self.transcription_buffer.clone()).await?;
        self.listeners.write().await.insert(call_id.to_string(), room);

        clog_info!("🎤 STT listener registered for call {}", &call_id[..8.min(call_id.len())]);
        Ok(())
    }

    /// Get or create an agent for a persona in a call.
    /// If the agent doesn't exist yet, connects to LiveKit and starts the video loop.
    ///
    /// Called both on room join (to show 3D avatars immediately) and on speak
    /// (to ensure agent exists before feeding TTS audio).
    ///
    /// `display_name` is shown in the browser participant grid. If None, falls back
    /// to user_id (UUID) — callers with access to the persona name should pass it.
    ///
    /// The agent's identity is the persona's user_id directly — no prefix mangling.
    /// Role classification uses JWT metadata (ParticipantRole::AiPersona), which
    /// the STT listener and browser both check to determine behavior.
    pub async fn get_or_create_agent(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Arc<LiveKitAgent>, String> {
        let key = (call_id.to_string(), user_id.to_string());

        // Fast path: agent already exists
        {
            let agents = self.agents.read().await;
            if let Some(agent) = agents.get(&key) {
                return Ok(agent.clone());
            }
        }

        // Acquire per-identity creation lock to prevent TOCTOU race.
        // Without this, 3 concurrent callers can all pass the fast path check,
        // then all call connect(), creating 3 agents and 3 video loops
        // that burn 3 Bevy render slots for the same identity.
        let creation_lock = {
            static CREATION_LOCKS: std::sync::Mutex<Option<std::collections::HashMap<
                (String, String), Arc<tokio::sync::Mutex<()>>
            >>> = std::sync::Mutex::new(None);
            let mut locks = CREATION_LOCKS.lock().unwrap();
            let map = locks.get_or_insert_with(std::collections::HashMap::new);
            map.entry(key.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = creation_lock.lock().await;

        // Re-check after acquiring lock — another task may have created the agent
        {
            let agents = self.agents.read().await;
            if let Some(agent) = agents.get(&key) {
                return Ok(agent.clone());
            }
        }

        // Create new agent with ai_persona role in metadata
        let name = display_name.unwrap_or(user_id);
        let (agent, _event_rx) = LiveKitAgent::connect(
            &self.livekit_url,
            call_id,
            user_id,  // Identity = persona's userId (unique UUID, no prefix needed)
            name,     // Display name shown in browser
        ).await?;

        let agent = Arc::new(agent);
        self.agents.write().await.insert(key, agent.clone());

        // Speaking agents don't process their own event_rx — the STT listener
        // handles all incoming audio processing centrally (one per call).

        // Start video loop immediately — the avatar should appear as soon as
        // the persona connects, not wait for first speech. Voice name isn't
        // available yet, so avatar selection uses deterministic hash (same persona
        // always gets the same model).
        start_video_loop(agent.clone());

        Ok(agent)
    }

    /// Synthesize TTS and inject into a call (replaces CallManager::speak_in_call).
    ///
    /// Publishes the subtitle BEFORE feeding audio frames so the browser receives
    /// the transcription at the same time as (or slightly before) the first audio
    /// frames arrive via WebRTC. Without this ordering, audio plays first and
    /// subtitles appear late because the data channel is instant but audio has
    /// WebRTC buffering/encoding latency.
    pub async fn speak_in_call(
        &self,
        call_id: &str,
        user_id: &str,
        text: &str,
        voice: Option<&str>,
        adapter: Option<&str>,
    ) -> Result<(usize, u64, u32), String> {
        use crate::voice::tts_service;
        use crate::voice::avatar::gender::gender_from_identity;
        use crate::voice::avatar::types::AvatarGender;

        // Derive gender from user_id — same function avatar selection uses.
        // This is the single source of truth: identity → gender → avatar + voice.
        let gender = gender_from_identity(user_id);
        let gender_str = match gender {
            AvatarGender::Male => "male",
            AvatarGender::Female => "female",
        };

        let synthesis = tts_service::synthesize_speech_async(
            text,
            voice,
            adapter,
            Some(gender_str),
        ).await.map_err(|e| format!("TTS synthesis failed: {}", e))?;

        let num_samples = synthesis.samples.len();
        let duration_ms = synthesis.duration_ms;
        let sample_rate = synthesis.sample_rate;

        let agent = self.get_or_create_agent(call_id, user_id, None).await?;

        // Set resolved voice name for gender-matched avatar selection (first-speak-wins).
        // Use the resolved name from TTS (e.g., "af_bella") not the input voice param
        // (which is typically a persona UUID that doesn't encode gender).
        if let Some(ref resolved) = synthesis.voice_name {
            agent.set_voice(resolved);
        } else if let Some(v) = voice {
            agent.set_voice(v);
        }

        // Signal avatar to start speaking animation (mouth movement, head nod)
        if let Some(bevy_system) = crate::voice::bevy_renderer::try_get() {
            bevy_system.set_speaking_by_identity(user_id, true);
        }

        // Calculate per-chunk RMS amplitude for mouth weight animation.
        // 66ms windows (~15fps) for smooth lip sync matching the render framerate.
        let mouth_weights = calculate_rms_weights(&synthesis.samples, sample_rate, 66);

        // Publish subtitle FIRST — native transcription linked to the audio track SID.
        // This ensures the browser receives the subtitle at the same time as audio starts,
        // rather than after all audio frames are queued (which caused audio-ahead-of-subtitles).
        if let Err(e) = agent.publish_transcription(text, user_id, true).await {
            clog_warn!("🤖 Failed to publish AI subtitle for {}: {}", &user_id[..8.min(user_id.len())], e);
        }

        // THEN feed audio frames to LiveKit
        agent.speak(synthesis.samples).await?;

        // Schedule amplitude-based mouth weights timed to audio playback.
        // Each weight corresponds to a 66ms window (~15fps) of the audio.
        let uid_for_mouth = user_id.to_string();
        if !mouth_weights.is_empty() {
            tokio::spawn(async move {
                for weight in mouth_weights {
                    if let Some(bevy_system) = crate::voice::bevy_renderer::try_get() {
                        bevy_system.set_mouth_weight_by_identity(&uid_for_mouth, weight);
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(66)).await;
                }
                // Reset mouth weight to 0 after audio finishes
                if let Some(bevy_system) = crate::voice::bevy_renderer::try_get() {
                    bevy_system.set_mouth_weight_by_identity(&uid_for_mouth, 0.0);
                }
            });
        }

        // Schedule speaking-off after audio duration elapses.
        // speak() feeds frames synchronously, so audio is queued by now.
        // Add a small buffer (200ms) for LiveKit's audio pipeline latency.
        let uid = user_id.to_string();
        let stop_delay = std::time::Duration::from_millis(duration_ms + 200);
        tokio::spawn(async move {
            tokio::time::sleep(stop_delay).await;
            if let Some(bevy_system) = crate::voice::bevy_renderer::try_get() {
                bevy_system.set_speaking_by_identity(&uid, false);
            }
        });

        Ok((num_samples, duration_ms, sample_rate))
    }

    /// Inject raw audio samples into a call (replaces CallManager::inject_audio).
    pub async fn inject_audio(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let agent = self.get_or_create_agent(call_id, user_id, None).await?;
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
        let agent = self.get_or_create_agent(call_id, &agent_id, None).await?;
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
                clog_warn!("🎵 Ambient audio error for call {}: {}", &cid[..8.min(cid.len())], e);
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

    let identity = format!("{}{}", AMBIENT_IDENTITY_PREFIX, &call_id[..8.min(call_id.len())]);
    let metadata = ParticipantMetadata::new(ParticipantRole::AmbientAudio);
    let token = generate_token(&identity, "Background Audio", call_id, true, &metadata)?;

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

    clog_info!("🎵 Ambient audio started for call {} (rain)", &call_id[..8.min(call_id.len())]);

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
                clog_info!("🎵 Ambient audio stopping — room empty for call {}", &call_id[..8.min(call_id.len())]);
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
                clog_warn!("🎵 Ambient frame error: {}", e);
                break;
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(chunk_duration_ms)).await;
    }

    room.close().await.ok();
    clog_info!("🎵 Ambient audio stopped for call {}", &call_id[..8.min(call_id.len())]);
    Ok(())
}
