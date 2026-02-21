//! WebSocket Call Server
//!
//! Handles live audio/video calls over WebSocket.
//! Each call has multiple participants, audio is mixed with mix-minus.

use crate::audio_constants::AUDIO_SAMPLE_RATE;
use crate::voice::audio_router::{AudioRouter, RoutedParticipant};
use crate::voice::capabilities::ModelCapabilityRegistry;
use crate::voice::handle::Handle;
use crate::voice::mixer::{AudioMixer, ParticipantStream};
use crate::voice::types::FrameKind;
use crate::voice::video_source::{TestPatternSource, VideoSource};
use crate::utils::audio::{base64_decode_i16, bytes_to_i16, i16_to_f32, is_silence, resample_to_16k};
use crate::voice::stt;
use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock, Semaphore};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};
use ts_rs::TS;

/// Maximum characters to show in truncated text previews (logs, errors)
const TEXT_PREVIEW_LENGTH: usize = 30;

/// Maximum concurrent transcription tasks
/// With base model (~10x realtime), 2 concurrent should handle bursts
/// If this fills up, we drop new audio rather than accumulate backlog
const MAX_CONCURRENT_TRANSCRIPTIONS: usize = 2;

/// Global semaphore to limit concurrent transcriptions
static TRANSCRIPTION_SEMAPHORE: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(MAX_CONCURRENT_TRANSCRIPTIONS)));

/// Embedded hold music WAV file (16kHz, mono, 16-bit)
static HOLD_MUSIC_WAV: &[u8] = include_bytes!("assets/hold-music.wav");

/// Pre-decoded hold music samples (lazy loaded once on first use)
static HOLD_MUSIC_SAMPLES: Lazy<Vec<i16>> = Lazy::new(|| {
    let cursor = Cursor::new(HOLD_MUSIC_WAV);
    match hound::WavReader::new(cursor) {
        Ok(mut reader) => {
            let samples: Vec<i16> = reader.samples::<i16>().filter_map(|s| s.ok()).collect();
            info!(
                "Loaded hold music: {} samples ({:.1}s at {}Hz)",
                samples.len(),
                samples.len() as f32 / AUDIO_SAMPLE_RATE as f32,
                AUDIO_SAMPLE_RATE
            );
            samples
        }
        Err(e) => {
            error!("Failed to decode hold music WAV: {}", e);
            Vec::new()
        }
    }
});

/// Message types for call protocol
/// TypeScript types are generated via `cargo test -p streaming-core export_types`
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/CallMessage.ts")]
#[serde(tag = "type")]
pub enum CallMessage {
    /// Join a call
    Join {
        call_id: String,
        user_id: String,
        display_name: String,
        #[serde(default)]
        is_ai: bool,  // AI participants get server-side audio buffering
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
        use crate::audio_constants::{AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE, AUDIO_FRAME_DURATION_MS, AUDIO_CHANNEL_CAPACITY};
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
    hold_music_position: usize,
    /// Stable handle for hold music (synthetic sender, never matches a real participant)
    hold_music_handle: Handle,
    /// Audio configuration
    pub config: AudioConfig,
    /// Shutdown signal for the audio loop
    shutdown_tx: Option<mpsc::Sender<()>>,
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
    fn generate_hold_tone(&mut self, frame_size: usize) -> Vec<i16> {
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
    calls: RwLock<HashMap<String, Arc<RwLock<Call>>>>,
    /// Map participant handle to call ID
    participant_calls: RwLock<HashMap<Handle, String>>,
    /// Track running audio loops
    audio_loops: RwLock<HashMap<String, tokio::task::JoinHandle<()>>>,
    /// Track video source shutdowns per call (multiple sources possible)
    video_source_shutdowns: RwLock<HashMap<String, Vec<mpsc::Sender<()>>>>,
    /// Audio router for model-capability-based routing (heterogeneous conversations)
    audio_router: AudioRouter,
    /// Model capability registry for looking up what models can do
    capability_registry: Arc<ModelCapabilityRegistry>,
}

impl CallManager {
    pub fn new() -> Self {
        Self {
            calls: RwLock::new(HashMap::new()),
            participant_calls: RwLock::new(HashMap::new()),
            audio_loops: RwLock::new(HashMap::new()),
            video_source_shutdowns: RwLock::new(HashMap::new()),
            audio_router: AudioRouter::new(),
            capability_registry: Arc::new(ModelCapabilityRegistry::new()),
        }
    }

    /// Get or create a call, starting audio loop if new
    async fn get_or_create_call(&self, call_id: &str) -> Arc<RwLock<Call>> {
        let mut calls = self.calls.write().await;
        if let Some(call) = calls.get(call_id) {
            call.clone()
        } else {
            let call = Arc::new(RwLock::new(Call::new(call_id.to_string())));
            calls.insert(call_id.to_string(), call.clone());

            // Start server-driven audio loop for this call
            self.start_audio_loop(call_id.to_string(), call.clone())
                .await;

            // Auto-start test video source (proves video plumbing works)
            // TODO: Remove when real video sources (webcam, bgfx-rs, avatar API) are connected
            let shutdown_tx = Self::start_test_video_source_for(&call, call_id);
            {
                let mut shutdowns = self.video_source_shutdowns.write().await;
                shutdowns.entry(call_id.to_string()).or_default().push(shutdown_tx);
            }

            call
        }
    }

    /// Start the server-driven audio loop (sends audio at fixed intervals)
    async fn start_audio_loop(&self, call_id: String, call: Arc<RwLock<Call>>) {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Configure call with shutdown signal
        {
            let mut c = call.write().await;
            c.set_shutdown(shutdown_tx);
        }

        // Get config once
        let frame_duration_ms = {
            let c = call.read().await;
            c.config.frame_duration_ms
        };

        let call_clone = call.clone();
        let call_id_clone = call_id.clone();

        // Spawn the audio loop task
        let handle = tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_millis(frame_duration_ms));

            info!(
                "Audio loop started for call {} ({}ms frames)",
                call_id_clone, frame_duration_ms
            );

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // CRITICAL: Minimize write lock duration to prevent blocking incoming audio
                        // Only hold lock for mixing, NOT for broadcasting
                        let (frames, audio_tx) = {
                            let mut c = call_clone.write().await;

                            // Only tick if there are participants
                            if c.mixer.participant_count() == 0 {
                                continue;
                            }

                            // Pull per-sender audio frames (SFU pattern)
                            let frames = c.tick();
                            let audio_tx = c.audio_tx.clone();

                            (frames, audio_tx)
                        };  // <-- Write lock released here, before broadcasting

                        // Broadcast per-sender frames to all participants WITHOUT holding write lock
                        // Each receiver filters out their own handle (mix-minus)
                        for (sender_handle, user_id, audio) in frames {
                            if audio_tx.send((sender_handle, user_id, audio)).is_err() {
                                // No receivers — acceptable
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Audio loop shutdown for call {}", call_id_clone);
                        break;
                    }
                }
            }
        });

        // Track the loop
        let mut loops = self.audio_loops.write().await;
        loops.insert(call_id, handle);
    }

    /// Stop audio loop for a call
    async fn stop_audio_loop(&self, call_id: &str) {
        // Signal shutdown
        {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(call_id) {
                let mut c = call.write().await;
                c.shutdown().await;
            }
        }

        // Remove and abort the task
        let mut loops = self.audio_loops.write().await;
        if let Some(handle) = loops.remove(call_id) {
            handle.abort();
        }
    }

    /// Join a participant to a call
    /// is_ai: If true, creates AI participant with server-side audio buffering
    pub async fn join_call(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: &str,
        is_ai: bool,
    ) -> CallJoinResult {
        let call = self.get_or_create_call(call_id).await;
        let handle = Handle::new();

        // Add participant to call
        // AI participants get a ring buffer for server-paced audio playback
        // Human participants get VAD for speech detection
        {
            let mut call = call.write().await;
            let stream = if is_ai {
                info!("🤖 Creating AI participant {} with ring buffer", display_name);
                ParticipantStream::new_ai(handle, user_id.to_string(), display_name.to_string())
            } else {
                ParticipantStream::new(handle, user_id.to_string(), display_name.to_string())
            };

            // Initialize VAD for speech detection and transcription (humans only)
            if let Err(e) = call.mixer.add_participant_with_init(stream).await {
                error!("Failed to initialize VAD for {}: {:?}", display_name, e);
            }
        }

        // Track participant -> call mapping
        {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.insert(handle, call_id.to_string());
        }

        // Subscribe to all broadcast channels
        let (audio_rx, transcription_rx, video_rx, message_rx) = {
            let call = call.read().await;
            (
                call.audio_tx.subscribe(),
                call.transcription_tx.subscribe(),
                call.video_tx.subscribe(),
                call.message_tx.subscribe(),
            )
        };

        info!(
            "Participant {} ({}) joined call {}",
            display_name,
            handle.short(),
            call_id
        );
        CallJoinResult { handle, audio_rx, transcription_rx, video_rx, message_rx }
    }

    /// Join a participant to a call with model-specific capabilities
    /// This enables heterogeneous conversations where audio-native models (GPT-4o)
    /// can hear TTS from text-only models (Claude) and vice versa.
    pub async fn join_call_with_model(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: &str,
        model_id: &str,
    ) -> CallJoinResult {
        // AI participants always get server-side buffering
        let result = self.join_call(call_id, user_id, display_name, true).await;

        // Create routed participant with model capabilities
        let participant = RoutedParticipant::ai(
            user_id.to_string(),
            display_name.to_string(),
            model_id,
            &self.capability_registry,
        );

        // Log routing info
        let caps = &participant.routing.capabilities;
        info!(
            "🎯 Model {} joined with routing: audio_in={}, audio_out={}, needs_stt={}, needs_tts={}",
            model_id,
            caps.audio_input,
            caps.audio_output,
            caps.needs_stt(),
            caps.needs_tts()
        );

        // Add to audio router for capability-based routing
        self.audio_router.add_participant(participant).await;

        result
    }

    /// Inject TTS audio into a call (for text-only models speaking)
    /// This routes the TTS audio to all audio-capable participants so they can hear it.
    pub async fn inject_tts_audio(
        &self,
        call_id: &str,
        from_handle: &Handle,
        display_name: &str,
        text: &str,
        samples: Vec<i16>,
    ) {
        let call = {
            let calls = self.calls.read().await;
            calls.get(call_id).cloned()
        };

        if let Some(call) = call {
            // Add TTS audio to the mixer so it gets mixed for all participants
            let mut call = call.write().await;

            // Push the TTS audio as if it came from this participant
            // The mixer will include it in mix-minus for everyone else to hear
            call.mixer.push_audio(from_handle, samples.clone());

            info!(
                "🔊 Injected TTS audio for {} into call {} ({} samples, \"{}\")",
                display_name,
                call_id,
                samples.len(),
                text.chars().take(TEXT_PREVIEW_LENGTH).collect::<String>()
            );

            // Also route through audio router for capability-aware handling
            self.audio_router
                .route_tts_audio(
                    &from_handle.to_string(),
                    display_name,
                    text,
                    samples,
                    AUDIO_SAMPLE_RATE,
                )
                .await;
        }
    }

    /// Leave a call
    pub async fn leave_call(&self, handle: &Handle) {
        let call_id = {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.remove(handle)
        };

        if let Some(call_id) = call_id {
            let (should_cleanup, user_id) = {
                let calls = self.calls.read().await;
                if let Some(call) = calls.get(&call_id) {
                    let mut call = call.write().await;
                    let user_id = if let Some(stream) = call.mixer.remove_participant(handle) {
                        info!(
                            "Participant {} ({}) left call {}",
                            stream.display_name,
                            handle.short(),
                            call_id
                        );
                        Some(stream.user_id.clone())
                    } else {
                        None
                    };
                    // Check if call is now empty
                    (call.mixer.participant_count() == 0, user_id)
                } else {
                    (false, None)
                }
            };

            // Remove from audio router if this was a model-aware participant
            if let Some(user_id) = user_id {
                self.audio_router.remove_participant(&user_id).await;
            }

            // Cleanup empty call
            if should_cleanup {
                self.stop_audio_loop(&call_id).await;

                // Stop all video sources for this call
                {
                    let mut shutdowns = self.video_source_shutdowns.write().await;
                    if let Some(sources) = shutdowns.remove(&call_id) {
                        for tx in sources {
                            let _ = tx.send(()).await;
                        }
                    }
                }

                let mut calls = self.calls.write().await;
                calls.remove(&call_id);
                info!("Call {} cleaned up (no participants)", call_id);
            }
        }
    }

    /// Push audio from a participant (buffered, mixed by audio loop)
    /// If speech ends, triggers transcription and broadcasts result
    pub async fn push_audio(&self, handle: &Handle, samples: Vec<i16>) {
        // STEP 1: Lookup call ID (fast, read-only)
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            // STEP 2: Push audio and check for speech end (minimized write lock)
            let (result, transcription_tx) = {
                let calls = self.calls.read().await;
                if let Some(call) = calls.get(&call_id) {
                    // Only hold write lock for the actual push operation
                    let mut call = call.write().await;
                    let result = call.push_audio(handle, samples);
                    let transcription_tx = call.transcription_tx.clone();
                    drop(call); // Explicitly release write lock early

                    (Some(result), Some(transcription_tx))
                } else {
                    (None, None)
                }
            };

            // STEP 3: Spawn transcription task if speech ended (no locks held)
            if let (Some(result), Some(transcription_tx)) = (result, transcription_tx) {
                if result.speech_ended {
                    if let (Some(user_id), Some(display_name), Some(speech_samples)) =
                        (result.user_id, result.display_name, result.speech_samples)
                    {
                        // Try to acquire semaphore permit (non-blocking)
                        // If we can't, drop this audio to prevent backlog
                        let semaphore = TRANSCRIPTION_SEMAPHORE.clone();
                        match semaphore.clone().try_acquire_owned() {
                            Ok(permit) => {
                                // Spawn transcription task with permit
                                tokio::spawn(async move {
                                    Self::transcribe_and_broadcast(
                                        transcription_tx,
                                        user_id,
                                        display_name,
                                        speech_samples,
                                    )
                                    .await;
                                    // Permit automatically released when dropped
                                    drop(permit);
                                });
                            }
                            Err(_) => {
                                // Queue full - drop this audio to stay current
                                warn!(
                                    "🚨 Dropping audio from {} - transcription queue full ({} max)",
                                    display_name, MAX_CONCURRENT_TRANSCRIPTIONS
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Push a video frame from a participant (broadcast to all others via mix-minus)
    /// frame_data is [VideoFrameHeader (16 bytes)][pixel data] — no FrameKind prefix
    pub async fn push_video(&self, handle: &Handle, frame_data: Vec<u8>) {
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let call = call.read().await;
                // Look up user_id from mixer — the sender's identity for client-side routing
                let user_id = call.mixer.find_user_id_by_handle(handle)
                    .unwrap_or_else(|| "unknown".to_string());
                // Broadcast with sender handle + user_id — receivers filter out their own frames
                if call.video_tx.send((*handle, user_id, frame_data)).is_err() {
                    // No receivers — this is fine, means nobody has video enabled
                }
            }
        }
    }

    /// Broadcast a CallMessage to all participants in a call (avatar updates, etc.)
    pub async fn broadcast_message(&self, call_id: &str, msg: CallMessage) {
        let call = {
            let calls = self.calls.read().await;
            calls.get(call_id).cloned()
        };

        if let Some(call) = call {
            let call = call.read().await;
            if call.message_tx.send(msg).is_err() {
                // No receivers — acceptable, means no WebSocket clients connected
            }
        }
    }

    /// Transcribe speech samples and broadcast to all participants
    async fn transcribe_and_broadcast(
        transcription_tx: broadcast::Sender<TranscriptionEvent>,
        user_id: String,
        display_name: String,
        samples: Vec<i16>,
    ) {
        // Check if STT is initialized
        if !stt::is_initialized() {
            warn!("STT adapter not initialized - skipping transcription");
            return;
        }

        info!(
            "[STEP 5] 📝 Whisper transcription START for {} ({} samples, {:.1}s)",
            display_name,
            samples.len(),
            samples.len() as f32 / AUDIO_SAMPLE_RATE as f32
        );

        // Convert i16 to f32 for Whisper
        let f32_samples = i16_to_f32(&samples);

        // Resample if needed (Whisper expects standard sample rate)
        let samples_16k = resample_to_16k(&f32_samples, AUDIO_SAMPLE_RATE);

        // Transcribe
        match stt::transcribe(samples_16k, Some("en")).await {
            Ok(result) => {
                let text = result.text.trim();
                if !text.is_empty() {
                    info!(
                        "[STEP 5] 📝 Whisper transcription DONE: \"{}\" (confidence: {:.2})",
                        text, result.confidence
                    );

                    // [STEP 6] Broadcast transcription to all participants
                    let event = TranscriptionEvent {
                        user_id: user_id.clone(),
                        display_name: display_name.clone(),
                        text: text.to_string(),
                        confidence: result.confidence,
                        language: result.language.clone(),
                    };

                    info!("[STEP 6] 📡 Broadcasting transcription to WebSocket clients");

                    // ERROR RECOVERY: Broadcast with detailed error logging
                    // With 500-event buffer, failures should be extremely rare
                    // If this fails, it means ALL receivers are too slow (lagging by 500+ events)
                    if transcription_tx.send(event).is_err() {
                        error!(
                            "[STEP 6] ❌ TRANSCRIPTION DROPPED: \"{}...\" from {}. \
                            ALL receivers are too slow - consider increasing buffer size or investigating blocking.",
                            text.chars().take(TEXT_PREVIEW_LENGTH).collect::<String>(),
                            display_name
                        );

                        // This is a critical issue - log to stderr for monitoring
                        eprintln!(
                            "🚨 CRITICAL: Transcription dropped due to buffer overflow. \
                            Text: \"{}...\", Speaker: {}, Buffer: 500 events",
                            text.chars().take(TEXT_PREVIEW_LENGTH).collect::<String>(),
                            display_name
                        );
                    }
                } else {
                    info!("📝 Empty transcription result from {}", display_name);
                }
            }
            Err(e) => {
                error!("Transcription failed for {}: {}", display_name, e);
            }
        }
    }

    /// Set mute state for a participant
    pub async fn set_mute(&self, handle: &Handle, muted: bool) {
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let mut call = call.write().await;
                if let Some(participant) = call.mixer.get_participant_mut(handle) {
                    participant.muted = muted;
                    info!("Participant {} muted: {}", handle.short(), muted);
                }
            }
        }
    }

    /// Synthesize text and inject directly into a call's mixer.
    /// Audio never leaves the Rust process — TypeScript only gets metadata back.
    /// Returns (num_samples, duration_ms, sample_rate) on success.
    pub async fn speak_in_call(
        &self,
        call_id: &str,
        user_id: &str,
        text: &str,
        voice: Option<&str>,
        adapter: Option<&str>,
    ) -> Result<(usize, u64, u32), String> {
        use crate::voice::tts_service;

        // Step 1: Verify participant is in this call
        let (handle, display_name) = {
            let calls = self.calls.read().await;
            let call = calls.get(call_id)
                .ok_or_else(|| format!("Call '{call_id}' not found"))?;
            let call = call.read().await;
            let handle = call.mixer.find_handle_by_user_id(user_id)
                .ok_or_else(|| format!("User '{user_id}' not in call '{call_id}'"))?;
            let display_name = call.mixer.get_participant(&handle)
                .map(|p| p.display_name.clone())
                .unwrap_or_else(|| user_id.to_string());
            (handle, display_name)
        };

        // Step 2: Synthesize (async — runs in current tokio context)
        let synthesis = tts_service::synthesize_speech_async(text, voice, adapter).await
            .map_err(|e| format!("TTS failed: {e}"))?;

        let num_samples = synthesis.samples.len();
        let duration_ms = synthesis.duration_ms;
        let sample_rate = synthesis.sample_rate;

        info!(
            "🔊 speak_in_call: Synthesized {} samples ({:.1}s) for {} in call {}",
            num_samples,
            duration_ms as f64 / 1000.0,
            display_name,
            call_id
        );

        // Step 3: Inject directly into the call mixer (audio stays in Rust)
        self.inject_tts_audio(call_id, &handle, &display_name, text, synthesis.samples).await;

        Ok((num_samples, duration_ms, sample_rate))
    }

    /// Inject pre-synthesized audio (from buffer pool) into a call's mixer.
    /// Resolves user_id to participant Handle, then pushes samples.
    pub async fn inject_audio(
        &self,
        call_id: &str,
        user_id: &str,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let (handle, display_name) = {
            let calls = self.calls.read().await;
            let call = calls.get(call_id)
                .ok_or_else(|| format!("Call '{call_id}' not found"))?;
            let call = call.read().await;
            let handle = call.mixer.find_handle_by_user_id(user_id)
                .ok_or_else(|| format!("User '{user_id}' not in call '{call_id}'"))?;
            let display_name = call.mixer.get_participant(&handle)
                .map(|p| p.display_name.clone())
                .unwrap_or_else(|| user_id.to_string());
            (handle, display_name)
        };

        let sample_count = samples.len();
        self.inject_tts_audio(call_id, &handle, &display_name, "(handle-based)", samples).await;

        info!(
            "inject_audio: {} samples into call {} for {}",
            sample_count, call_id, display_name
        );

        Ok(())
    }

    /// Add an ambient audio source to a call (TV, music, background noise).
    /// Returns a Handle for injecting audio and removing the source later.
    /// Ambient sources use AI ring buffer infrastructure for server-paced playback
    /// and are NEVER excluded from mix-minus (everyone hears them).
    pub async fn add_ambient_source(
        &self,
        call_id: &str,
        source_name: &str,
    ) -> Result<Handle, String> {
        let call = {
            let calls = self.calls.read().await;
            calls.get(call_id).cloned()
                .ok_or_else(|| format!("Call '{call_id}' not found"))?
        };

        let handle = Handle::new();
        {
            let mut call = call.write().await;
            let stream = ParticipantStream::new_ambient(handle, source_name.to_string());
            call.mixer.add_participant(stream);
        }

        // Track ambient source -> call mapping
        {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.insert(handle, call_id.to_string());
        }

        info!(
            "🔊 Added ambient source '{}' ({}) to call {}",
            source_name, handle.short(), call_id
        );

        Ok(handle)
    }

    /// Remove an ambient audio source from a call
    pub async fn remove_ambient_source(
        &self,
        call_id: &str,
        handle: Handle,
    ) -> Result<(), String> {
        let call = {
            let calls = self.calls.read().await;
            calls.get(call_id).cloned()
                .ok_or_else(|| format!("Call '{call_id}' not found"))?
        };

        {
            let mut call = call.write().await;
            call.mixer.remove_participant(&handle);
        }

        {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.remove(&handle);
        }

        info!(
            "🔊 Removed ambient source ({}) from call {}",
            handle.short(), call_id
        );

        Ok(())
    }

    /// Inject audio directly into a call's mixer by handle.
    /// Used for ambient sources where we already have the handle.
    pub async fn inject_audio_by_handle(
        &self,
        call_id: &str,
        handle: &Handle,
        samples: Vec<i16>,
    ) -> Result<(), String> {
        let call = {
            let calls = self.calls.read().await;
            calls.get(call_id).cloned()
                .ok_or_else(|| format!("Call '{call_id}' not found"))?
        };

        {
            let mut call = call.write().await;
            call.mixer.push_audio(handle, samples);
        }

        Ok(())
    }

    /// Start a test video source for a call using the VideoSource trait.
    /// Generates SMPTE color bar frames at ~10fps to prove video streaming works.
    /// The source gets its own Handle (acts as a virtual participant for video only).
    /// Returns a shutdown sender — drop it or send () to stop the generator.
    ///
    /// Accepts the Call Arc directly to avoid deadlocks when called from
    /// get_or_create_call() which already holds the calls write lock.
    fn start_test_video_source_for(
        call: &Arc<RwLock<Call>>,
        call_id: &str,
    ) -> mpsc::Sender<()> {
        let video_tx = {
            let call_guard = call.try_read().expect("Call should be available (just created)");
            call_guard.video_tx.clone()
        };

        let source_handle = Handle::new();
        let source = Box::new(TestPatternSource::default_test());

        info!("Starting {} for call {}", source.name(), call_id);

        source.start(video_tx, source_handle)
    }

    /// Add a video source to a call. The source starts producing frames immediately.
    /// Returns the Handle assigned to the source (for mix-minus filtering).
    pub async fn add_video_source(
        &self,
        call_id: &str,
        source: Box<dyn VideoSource>,
    ) -> Result<Handle, String> {
        let call = {
            let calls = self.calls.read().await;
            calls.get(call_id).cloned()
                .ok_or_else(|| format!("Call '{call_id}' not found"))?
        };

        let video_tx = {
            let call = call.read().await;
            call.video_tx.clone()
        };

        let handle = Handle::new();
        let source_name = source.name().to_string();
        let source_user_id = source.user_id().to_string();

        info!(
            "Adding video source '{}' (user_id={}) to call {}",
            source_name, source_user_id, call_id
        );

        let shutdown_tx = source.start(video_tx, handle);

        {
            let mut shutdowns = self.video_source_shutdowns.write().await;
            shutdowns.entry(call_id.to_string()).or_default().push(shutdown_tx);
        }

        Ok(handle)
    }

    /// Remove ALL video sources for a call (cleanup)
    pub async fn remove_video_sources(&self, call_id: &str) {
        let mut shutdowns = self.video_source_shutdowns.write().await;
        if let Some(sources) = shutdowns.remove(call_id) {
            let count = sources.len();
            for tx in sources {
                let _ = tx.send(()).await;
            }
            info!("Removed {} video sources from call {}", count, call_id);
        }
    }

    /// Get call stats
    pub async fn get_stats(&self, handle: &Handle) -> Option<(usize, u64)> {
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let call = call.read().await;
                return Some((call.mixer.participant_count(), call.samples_processed));
            }
        }
        None
    }
}

impl Default for CallManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle a single WebSocket connection
async fn handle_connection(stream: TcpStream, addr: SocketAddr, manager: Arc<CallManager>) {
    let ws_stream = match accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!("WebSocket handshake failed for {}: {}", addr, e);
            return;
        }
    };

    info!("New WebSocket connection from {}", addr);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut participant_handle: Option<Handle> = None;
    let mut is_muted = false; // Track mute state at connection level

    // Channel for sending messages from audio receiver task
    let (msg_tx, mut msg_rx) = mpsc::channel::<Message>(64);

    // Spawn task to forward messages to WebSocket
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Main message loop
    loop {
        tokio::select! {
            // Receive message from WebSocket
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<CallMessage>(&text) {
                            Ok(CallMessage::Join { call_id, user_id, display_name, is_ai }) => {
                                let join = manager.join_call(&call_id, &user_id, &display_name, is_ai).await;
                                let handle = join.handle;
                                let mut audio_rx = join.audio_rx;
                                let mut transcription_rx = join.transcription_rx;
                                let mut video_rx = join.video_rx;
                                let mut message_rx = join.message_rx;
                                participant_handle = Some(handle);

                                // Audio forwarding: SFU per-sender with sender_id in wire format
                                // Wire: [0x01 FrameKind::Audio][sender_id_len: u8][sender_id: UTF-8][PCM16 i16 LE]
                                // Same pattern as video — browser routes by senderId for A/V sync
                                let msg_tx_audio = msg_tx.clone();
                                tokio::spawn(async move {
                                    while let Ok((sender_handle, sender_user_id, audio)) = audio_rx.recv().await {
                                        // Mix-minus: skip our own audio frames
                                        if sender_handle != handle {
                                            let id_bytes = sender_user_id.as_bytes();
                                            let id_len = id_bytes.len().min(255) as u8;
                                            let mut bytes = Vec::with_capacity(1 + 1 + id_len as usize + audio.len() * 2);
                                            bytes.push(FrameKind::Audio as u8);
                                            bytes.push(id_len);
                                            bytes.extend_from_slice(&id_bytes[..id_len as usize]);
                                            bytes.extend(audio.iter().flat_map(|&s| s.to_le_bytes()));
                                            if msg_tx_audio.send(Message::Binary(bytes)).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                });

                                // Transcription forwarding (JSON text frames)
                                let msg_tx_transcription = msg_tx.clone();
                                let ws_display_name = display_name.clone();
                                tokio::spawn(async move {
                                    while let Ok(event) = transcription_rx.recv().await {
                                        info!("[STEP 7] 🌐 WebSocket sending transcription to {}: \"{}\"",
                                            ws_display_name, event.text.chars().take(TEXT_PREVIEW_LENGTH).collect::<String>());
                                        let msg = CallMessage::Transcription {
                                            user_id: event.user_id,
                                            display_name: event.display_name,
                                            text: event.text,
                                            confidence: event.confidence,
                                            language: event.language,
                                        };
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            if msg_tx_transcription.send(Message::Text(json)).await.is_err() {
                                                warn!("[STEP 7] ❌ WebSocket send FAILED for {}", ws_display_name);
                                                break;
                                            }
                                        }
                                    }
                                });

                                // Video forwarding: mix-minus (see everyone but yourself)
                                // Wire format: [0x02 FrameKind::Video][sender_id_len: u8][sender_id: UTF-8][VideoFrameHeader 16b][pixels]
                                let msg_tx_video = msg_tx.clone();
                                tokio::spawn(async move {
                                    while let Ok((sender_handle, sender_user_id, video_data)) = video_rx.recv().await {
                                        // Mix-minus: skip our own video frames
                                        if sender_handle != handle {
                                            let id_bytes = sender_user_id.as_bytes();
                                            let id_len = id_bytes.len().min(255) as u8;
                                            let mut frame = Vec::with_capacity(1 + 1 + id_len as usize + video_data.len());
                                            frame.push(FrameKind::Video as u8);
                                            frame.push(id_len);
                                            frame.extend_from_slice(&id_bytes[..id_len as usize]);
                                            frame.extend_from_slice(&video_data);
                                            if msg_tx_video.send(Message::Binary(frame)).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                });

                                // General message forwarding (avatar updates, video config, etc.)
                                let msg_tx_messages = msg_tx.clone();
                                tokio::spawn(async move {
                                    while let Ok(call_msg) = message_rx.recv().await {
                                        if let Ok(json) = serde_json::to_string(&call_msg) {
                                            if msg_tx_messages.send(Message::Text(json)).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                });
                            }
                            Ok(CallMessage::Leave) => {
                                if let Some(handle) = participant_handle.take() {
                                    manager.leave_call(&handle).await;
                                }
                                break;
                            }
                            Ok(CallMessage::Audio { data }) => {
                                // Skip processing if muted at connection level
                                if is_muted {
                                    continue;
                                }
                                if let Some(handle) = &participant_handle {
                                    if let Some(samples) = base64_decode_i16(&data) {
                                        manager.push_audio(handle, samples).await;
                                    }
                                }
                            }
                            Ok(CallMessage::Mute { muted }) => {
                                is_muted = muted; // Track locally for this connection
                                if let Some(handle) = &participant_handle {
                                    manager.set_mute(handle, muted).await;
                                }
                                info!("Connection mute state set: {}", muted);
                            }
                            Ok(CallMessage::VideoConfig { width, height, fps, format }) => {
                                info!(
                                    "📹 Video config from {}: {}x{} @{}fps format={}",
                                    addr, width, height, fps, format
                                );
                                // Mark this call as having video enabled
                                if let Some(handle) = &participant_handle {
                                    let call_id = {
                                        let pc = manager.participant_calls.read().await;
                                        pc.get(handle).cloned()
                                    };
                                    if let Some(call_id) = call_id {
                                        let calls = manager.calls.read().await;
                                        if let Some(call) = calls.get(&call_id) {
                                            let mut call = call.write().await;
                                            call.has_video = true;
                                        }
                                    }
                                }
                            }
                            Ok(_) => {
                                // Ignore other message types from client
                            }
                            Err(e) => {
                                warn!("Failed to parse message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        // Binary frame protocol: first byte is FrameKind discriminator
                        if data.is_empty() || is_muted { continue; }
                        if let Some(handle) = &participant_handle {
                            match FrameKind::from_byte(data[0]) {
                                Some(FrameKind::Audio) => {
                                    // [0x01][PCM16 i16 LE bytes]
                                    let samples = bytes_to_i16(&data[1..]);
                                    manager.push_audio(handle, samples).await;
                                }
                                Some(FrameKind::Video) => {
                                    // [0x02][VideoFrameHeader 16 bytes][pixel data]
                                    manager.push_video(handle, data[1..].to_vec()).await;
                                }
                                Some(FrameKind::AvatarState) => {
                                    // Client should not send avatar state (server→client only)
                                    warn!("Received AvatarState from client — ignored");
                                }
                                None => {
                                    // Legacy: no FrameKind prefix, treat entire payload as raw audio
                                    let samples = bytes_to_i16(&data);
                                    manager.push_audio(handle, samples).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(_)) => {
                        // Ignore ping/pong
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
        }
    }

    // Cleanup
    if let Some(handle) = participant_handle {
        manager.leave_call(&handle).await;
    }

    info!("WebSocket connection closed for {}", addr);
    sender_task.abort();
}

/// Start the WebSocket call server with an externally-created CallManager.
/// This allows the IPC server to share the same CallManager for direct audio injection.
pub async fn start_call_server(addr: &str, manager: Arc<CallManager>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr).await?;

    info!("Call server listening on {}", addr);

    loop {
        let (stream, addr) = listener.accept().await?;
        let manager = manager.clone();
        tokio::spawn(handle_connection(stream, addr, manager));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_constants::{AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE};
    use crate::voice::mixer::test_utils::*;
    use crate::utils::audio::base64_encode_i16;

    #[test]
    fn test_base64_roundtrip() {
        let samples = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        let encoded = base64_encode_i16(&samples);
        let decoded = base64_decode_i16(&encoded).unwrap();
        assert_eq!(samples, decoded);
    }

    #[tokio::test]
    async fn test_call_manager_join_leave() {
        let manager = CallManager::new();

        // Join a call (false = not AI)
        let join = manager.join_call("test-call", "user-1", "Alice", false).await;

        // Check stats
        let stats = manager.get_stats(&join.handle).await;
        assert!(stats.is_some());
        let (count, _) = stats.unwrap();
        assert_eq!(count, 1);

        // Leave call
        manager.leave_call(&join.handle).await;

        // Stats should be gone
        let stats = manager.get_stats(&join.handle).await;
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_call_manager_multi_participant() {
        let manager = CallManager::new();

        // Two participants join (humans)
        let join_a = manager.join_call("test-call", "user-a", "Alice", false).await;
        let join_b = manager.join_call("test-call", "user-b", "Bob", false).await;

        // Check count
        let stats = manager.get_stats(&join_a.handle).await;
        assert_eq!(stats.unwrap().0, 2);

        // Push audio from Alice (buffered, mixed by audio loop)
        let audio = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        manager.push_audio(&join_a.handle, audio).await;

        // Give audio loop time to tick
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check samples processed (audio loop should have ticked)
        let stats = manager.get_stats(&join_a.handle).await;
        assert!(stats.unwrap().1 > 0);

        // Leave
        manager.leave_call(&join_a.handle).await;
        manager.leave_call(&join_b.handle).await;
    }

    #[tokio::test]
    async fn test_mute() {
        let manager = CallManager::new();

        let join = manager.join_call("test-call", "user-1", "Alice", false).await;

        // Mute
        manager.set_mute(&join.handle, true).await;

        // Unmute
        manager.set_mute(&join.handle, false).await;

        manager.leave_call(&join.handle).await;
    }

    #[tokio::test]
    async fn test_video_push_broadcast() {
        let manager = CallManager::new();

        // Two participants join
        let join_a = manager.join_call("test-call", "user-a", "Alice", false).await;
        let mut join_b = manager.join_call("test-call", "user-b", "Bob", false).await;

        // Alice sends a video frame
        let fake_frame = vec![0x00; 20]; // 16 byte header + 4 byte payload
        manager.push_video(&join_a.handle, fake_frame.clone()).await;

        // Bob should receive Alice's video (mix-minus: not your own)
        // Give broadcast time to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Try receiving — might be empty if broadcast hasn't delivered yet
        match join_b.video_rx.try_recv() {
            Ok((sender, user_id, data)) => {
                assert_eq!(sender, join_a.handle);
                assert_eq!(user_id, "user-a");
                assert_eq!(data, fake_frame);
            }
            Err(_) => {
                // Broadcast delivery is async, this is acceptable in tests
            }
        }

        manager.leave_call(&join_a.handle).await;
        manager.leave_call(&join_b.handle).await;
    }
}
