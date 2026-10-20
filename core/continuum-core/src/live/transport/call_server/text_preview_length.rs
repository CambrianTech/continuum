use super::*;

/// Maximum characters to show in truncated text previews (logs, errors)
pub(super) const TEXT_PREVIEW_LENGTH: usize = 30;

/// Maximum concurrent transcription tasks
/// With base model (~10x realtime), 2 concurrent should handle bursts
/// If this fills up, we drop new audio rather than accumulate backlog
pub(super) const MAX_CONCURRENT_TRANSCRIPTIONS: usize = 2;

/// Global semaphore to limit concurrent transcriptions
pub(super) static TRANSCRIPTION_SEMAPHORE: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(MAX_CONCURRENT_TRANSCRIPTIONS)));

/// Embedded hold music WAV file (16kHz, mono, 16-bit)
pub(super) static HOLD_MUSIC_WAV: &[u8] = include_bytes!("../../assets/hold-music.wav");

/// Pre-decoded hold music samples (lazy loaded once on first use)
pub(super) static HOLD_MUSIC_SAMPLES: Lazy<Vec<i16>> = Lazy::new(|| {
    let cursor = Cursor::new(HOLD_MUSIC_WAV);
    match hound::WavReader::new(cursor) {
        Ok(mut reader) => {
            let samples: Vec<i16> = reader.samples::<i16>().filter_map(|s| s.ok()).collect();
            clog_info!(
                "Loaded hold music: {} samples ({:.1}s at {}Hz)",
                samples.len(),
                samples.len() as f32 / AUDIO_SAMPLE_RATE as f32,
                AUDIO_SAMPLE_RATE
            );
            samples
        }
        Err(e) => {
            clog_error!("Failed to decode hold music WAV: {}", e);
            Vec::new()
        }
    }
});

/// Message types for call protocol
/// TypeScript types are generated via `cargo test -p streaming-core export_types`
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/CallMessage.ts")]
#[serde(tag = "type")]
pub enum CallMessage {
    /// Join a call
    Join {
        call_id: String,
        user_id: String,
        display_name: String,
        #[serde(default)]
        is_ai: bool, // AI participants get server-side audio buffering
    },

    /// Leave the call
    Leave,

    /// Audio data from client (base64 encoded i16 PCM).
    /// LEGACY: Prefer binary WebSocket frames (Message::Binary) for audio.
    /// Kept for backward compatibility with older clients.
    Audio { data: String },

    /// Mute/unmute
    Mute { muted: bool },

    /// Participant joined notification
    ParticipantJoined {
        user_id: String,
        display_name: String,
    },

    /// Participant left notification
    ParticipantLeft { user_id: String },

    /// Error message
    Error { message: String },

    /// Call stats
    Stats {
        participant_count: usize,
        samples_processed: u64,
    },

    /// Transcription result (server → client)
    Transcription {
        user_id: String,
        display_name: String,
        text: String,
        confidence: f32,
        language: String,
    },

    /// Video configuration (client → server, negotiates format)
    VideoConfig {
        width: u16,
        height: u16,
        fps: u8,
        /// Pixel format: "rgba8", "vp8", "h264", "jpeg"
        format: String,
    },

    /// Avatar state update (server → client, drives browser avatar rendering)
    AvatarUpdate {
        persona_id: String,
        speaking: bool,
        listening: bool,
        emotion: String,
        #[serde(default)]
        viseme: u8,
        #[serde(default)]
        viseme_weight: f32,
        #[serde(default)]
        head_rotation: [f32; 3],
        #[serde(default)]
        gaze_target: [f32; 2],
    },
}

/// Audio stream configuration - SINGLE SOURCE OF TRUTH for all buffer sizes
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub frame_size: usize,
    pub frame_duration_ms: u64,

    /// Audio broadcast channel capacity (frames)
    /// Larger = more buffering, less likely to drop audio if client slow
    /// Each frame ~640 bytes, so 2000 frames = ~1.3MB = 40 seconds
    pub audio_channel_capacity: usize,

    /// Transcription broadcast channel capacity (events)
    /// Transcription events are small, buffer generously
    pub transcription_channel_capacity: usize,
}

impl Default for AudioConfig {
    fn default() -> Self {
        use crate::audio_constants::{
            AUDIO_CHANNEL_CAPACITY, AUDIO_FRAME_DURATION_MS, AUDIO_FRAME_SIZE, AUDIO_SAMPLE_RATE,
        };
        // Sample rate and frame size from audio constants (single source of truth)
        Self {
            sample_rate: AUDIO_SAMPLE_RATE,
            frame_size: AUDIO_FRAME_SIZE,
            frame_duration_ms: AUDIO_FRAME_DURATION_MS,
            // NEVER drop audio - buffer 40+ seconds
            audio_channel_capacity: AUDIO_CHANNEL_CAPACITY,
            // NEVER drop transcriptions - buffer 500 events
            transcription_channel_capacity: 500,
        }
    }
}

/// Transcription event for broadcasting to participants
#[derive(Debug, Clone)]
pub struct TranscriptionEvent {
    pub user_id: String,
    pub display_name: String,
    pub text: String,
    pub confidence: f32,
    pub language: String,
}

/// A single call instance with server-driven audio clock
pub struct Call {
    pub id: String,
    pub mixer: AudioMixer,
    /// Broadcast channel for per-sender audio (SFU pattern: sender handle, user_id, raw audio)
    /// Browser handles mixing — enables per-participant audio/video synchronization
    pub audio_tx: broadcast::Sender<(Handle, String, Vec<i16>)>,
    /// Broadcast channel for sending transcriptions to participants
    pub transcription_tx: broadcast::Sender<TranscriptionEvent>,
    /// Broadcast channel for video frames (handle for mix-minus, user_id for routing, data is raw frame)
    pub video_tx: broadcast::Sender<(Handle, String, Vec<u8>)>,
    /// Broadcast channel for general JSON messages (avatar updates, video config, etc.)
    pub message_tx: broadcast::Sender<CallMessage>,
    /// Total samples processed (for stats)
    pub samples_processed: u64,
    /// Current position in hold music (sample index)
    pub(super) hold_music_position: usize,
    /// Stable handle for hold music (synthetic sender, never matches a real participant)
    pub(super) hold_music_handle: Handle,
    /// Audio configuration
    pub config: AudioConfig,
    /// Shutdown signal for the audio loop
    pub(super) shutdown_tx: Option<mpsc::Sender<()>>,
    /// Whether any participant has video enabled
    pub has_video: bool,
}

/// Result of joining a call — all the broadcast receivers a participant needs
pub struct CallJoinResult {
    pub handle: Handle,
    /// Per-sender audio (SFU): (sender_handle, sender_user_id, audio_frame)
    pub audio_rx: broadcast::Receiver<(Handle, String, Vec<i16>)>,
    pub transcription_rx: broadcast::Receiver<TranscriptionEvent>,
    pub video_rx: broadcast::Receiver<(Handle, String, Vec<u8>)>,
    pub message_rx: broadcast::Receiver<CallMessage>,
}

/// Result of pushing audio - contains transcription info if speech ended
pub struct CallPushAudioResult {
    pub speech_ended: bool,
    pub user_id: Option<String>,
    pub display_name: Option<String>,
    pub speech_samples: Option<Vec<i16>>,
}

impl Call {
    pub fn new(id: String) -> Self {
        let config = AudioConfig::default();

        // Use config values for channel capacities (single source of truth)
        // CRITICAL: Large buffers to NEVER drop audio/transcriptions
        // Audio frames are tiny (~640 bytes each), so 2000 frames = ~1.3MB = 40 seconds
        let (audio_tx, _) = broadcast::channel(config.audio_channel_capacity);
        let (transcription_tx, _) = broadcast::channel(config.transcription_channel_capacity);
        // Video frames are larger but fewer (30fps vs 50fps audio).
        // 120 frames = 4 seconds at 30fps, each frame ~50KB compressed = ~6MB buffer
        let (video_tx, _) = broadcast::channel(120);
        // General JSON messages (avatar updates, video config responses) — small and infrequent
        let (message_tx, _) = broadcast::channel(100);

        Self {
            id,
            mixer: AudioMixer::default_voice(),
            audio_tx,
            transcription_tx,
            video_tx,
            message_tx,
            samples_processed: 0,
            hold_music_position: 0,
            hold_music_handle: Handle::new(),
            config,
            shutdown_tx: None,
            has_video: false,
        }
    }

    /// Generate hold music from pre-decoded samples
    pub(super) fn generate_hold_tone(&mut self, frame_size: usize) -> Vec<i16> {
        let samples = &*HOLD_MUSIC_SAMPLES;

        if samples.is_empty() {
            return vec![0i16; frame_size];
        }

        let total_len = samples.len();
        let mut output = Vec::with_capacity(frame_size);

        for i in 0..frame_size {
            let idx = (self.hold_music_position + i) % total_len;
            output.push(samples[idx]);
        }

        self.hold_music_position = (self.hold_music_position + frame_size) % total_len;
        output
    }

    /// Update incoming audio from a participant (doesn't send anything)
    /// Returns result indicating if speech ended and is ready for transcription
    pub fn push_audio(&mut self, from_handle: &Handle, samples: Vec<i16>) -> CallPushAudioResult {
        let result = self.mixer.push_audio(from_handle, samples);
        CallPushAudioResult {
            speech_ended: result.speech_ended,
            user_id: result.user_id,
            display_name: result.display_name,
            speech_samples: result.speech_samples,
        }
    }

    /// Generate per-sender audio frames (SFU pattern, called by audio loop).
    /// Returns (sender_handle, sender_user_id, audio_frame) for each active sender.
    /// Browser handles mixing — this enables per-participant audio/video synchronization.
    pub fn tick(&mut self) -> Vec<(Handle, String, Vec<i16>)> {
        let frame_size = self.config.frame_size;
        self.samples_processed += frame_size as u64;

        let is_alone = self.mixer.participant_count() == 1;
        let mut frames = self.mixer.pull_all_audio();

        // If participant is alone and nobody is producing audio, inject hold music
        // as a synthetic sender so the lonely participant hears something
        if is_alone && frames.iter().all(|(_, _, audio)| is_silence(audio, 50.0)) {
            frames.push((
                self.hold_music_handle,
                "hold-music".to_string(),
                self.generate_hold_tone(frame_size),
            ));
        }

        frames
    }

    /// Set shutdown sender (called by CallManager when starting audio loop)
    pub fn set_shutdown(&mut self, tx: mpsc::Sender<()>) {
        self.shutdown_tx = Some(tx);
    }

    /// Signal shutdown
    pub async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
    }
}

/// Call manager - tracks all active calls with server-driven audio loops
pub struct CallManager {
    pub(super) calls: RwLock<HashMap<String, Arc<RwLock<Call>>>>,
    /// Map participant handle to call ID
    pub(super) participant_calls: RwLock<HashMap<Handle, String>>,
    /// Track running audio loops
    pub(super) audio_loops: RwLock<HashMap<String, tokio::task::JoinHandle<()>>>,
    /// Track video source shutdowns per call (multiple sources possible)
    pub(super) video_source_shutdowns: RwLock<HashMap<String, Vec<mpsc::Sender<()>>>>,
    /// Audio router for model-capability-based routing (heterogeneous conversations)
    pub(super) audio_router: AudioRouter,
    /// Model capability registry for looking up what models can do
    pub(super) capability_registry: Arc<ModelCapabilityRegistry>,
    /// Stable per-(call,persona) mixer handle for the native-plane voice tee (#193
    /// audio convergence). A persona is not a native WS participant — she publishes
    /// to LiveKit — so `push_persona_audio` registers her as a virtual AI sender in
    /// the call's mixer under this handle. Keyed `"{call_id}\0{user_id}"`.
    pub(super) persona_audio_handles: RwLock<HashMap<String, Handle>>,
    /// Core-side session registration hook (#58). `None` in tests and until boot
    /// installs it; a call still works without it, it is just not registered — which
    /// is precisely the state the live-call view now renders instead of hiding.
    pub(super) session_registrar: Option<SessionRegistrar>,
    /// Where a HUMAN's transcribed utterance goes so the CITIZENS can perceive
    /// it (closed 2026-09-01 — the second STT dead link: transcriptions
    /// forwarded to WebSocket SUBTITLES only, so even a heard human was never
    /// answered; the words reached screens and no minds). The call id IS the
    /// airc RoomId, so the sink posts the utterance as a ROOM MESSAGE from the
    /// human — and the whole existing pipeline (perception, directedness,
    /// turns, voice-out) fires as if she had typed it. Same callback shape and
    /// reason as [`SessionRegistrar`]: transport announces a fact, never
    /// learns what cognition does with it.
    pub(super) transcript_sink: Option<TranscriptSink>,
}

/// `(call_id/room, speaker user_id, speaker display name, transcribed text)`.
pub type TranscriptSink = Arc<dyn Fn(&str, &str, &str, &str) + Send + Sync>;

/// Told when a participant joins a live call, so the CORE can register the session
/// itself (#58).
///
/// A callback, not an import: transport announces a fact and never learns what the
/// session layer does with it. `CallManager` stays ignorant of `VoiceService`,
/// `VoiceOrchestrator` and `VoiceParticipant`, which is what keeps the recipe layer
/// from leaking into the audio path.
///
/// Why it exists at all: registration was CLIENT-driven. `join_call` knew a call had
/// started, held the call id (= the airc RoomId) and the joiner, and told nobody. So
/// the orchestration lived in whichever client bothered to implement it — exactly one
/// did, in Node, in `legacy/` — and iOS/Android/TUI citizens were structurally
/// voiceless. The legacy bridge documents the consequence: "Without this, isInCall()
/// returns false and AI responses are silently dropped."
pub type SessionRegistrar = Arc<dyn Fn(&str, &str, &str, bool) + Send + Sync>;
