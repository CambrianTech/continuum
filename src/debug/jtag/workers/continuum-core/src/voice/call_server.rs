//! WebSocket Call Server
//!
//! Handles live audio/video calls over WebSocket.
//! Each call has multiple participants, audio is mixed with mix-minus.

use crate::audio_constants::AUDIO_SAMPLE_RATE;
use crate::voice::audio_router::{AudioRouter, RoutedParticipant};
use crate::voice::capabilities::ModelCapabilityRegistry;
use crate::voice::handle::Handle;
use crate::voice::mixer::{AudioMixer, ParticipantStream};
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
    /// Broadcast channel for sending mixed audio to participants
    pub audio_tx: broadcast::Sender<(Handle, Vec<i16>)>,
    /// Broadcast channel for sending transcriptions to participants
    pub transcription_tx: broadcast::Sender<TranscriptionEvent>,
    /// Total samples processed (for stats)
    pub samples_processed: u64,
    /// Current position in hold music (sample index)
    hold_music_position: usize,
    /// Audio configuration
    pub config: AudioConfig,
    /// Shutdown signal for the audio loop
    shutdown_tx: Option<mpsc::Sender<()>>,
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

        Self {
            id,
            mixer: AudioMixer::default_voice(),
            audio_tx,
            transcription_tx,
            samples_processed: 0,
            hold_music_position: 0,
            config,
            shutdown_tx: None,
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

    /// Generate one frame of mixed audio for all participants (called by audio loop)
    pub fn tick(&mut self) -> Vec<(Handle, Vec<i16>)> {
        let frame_size = self.config.frame_size;
        self.samples_processed += frame_size as u64;

        let is_alone = self.mixer.participant_count() == 1;
        let mixes = self.mixer.mix_minus_all();

        mixes
            .into_iter()
            .map(|(handle, mixed_audio)| {
                // If alone, mix in hold tone
                let audio = if is_alone && is_silence(&mixed_audio, 50.0) {
                    self.generate_hold_tone(frame_size)
                } else {
                    mixed_audio
                };
                (handle, audio)
            })
            .collect()
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
                        let (mixes, audio_tx) = {
                            let mut c = call_clone.write().await;

                            // Only tick if there are participants
                            if c.mixer.participant_count() == 0 {
                                continue;
                            }

                            // Generate mixed audio for all participants
                            let mixes = c.tick();
                            let audio_tx = c.audio_tx.clone();

                            (mixes, audio_tx)
                        };  // <-- Write lock released here, before broadcasting

                        // Broadcast to all participants WITHOUT holding write lock
                        // This prevents incoming audio from being blocked by slow/lagging receivers
                        for (handle, audio) in mixes {
                            if audio_tx.send((handle, audio)).is_err() {
                                // Log broadcast failures (lagging receivers)
                                // This is expected when a participant can't keep up
                                // Note: With 2000-frame buffer (~40s), this should be extremely rare
                                warn!("Audio broadcast to {} failed (receiver too slow, dropped frame)", handle.short());
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
    ) -> (
        Handle,
        broadcast::Receiver<(Handle, Vec<i16>)>,
        broadcast::Receiver<TranscriptionEvent>,
    ) {
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
                // Fallback to non-VAD participant (won't get transcriptions)
            }
        }

        // Track participant -> call mapping
        {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.insert(handle, call_id.to_string());
        }

        // Subscribe to audio and transcription broadcasts
        let (audio_rx, transcription_rx) = {
            let call = call.read().await;
            (call.audio_tx.subscribe(), call.transcription_tx.subscribe())
        };

        info!(
            "Participant {} ({}) joined call {}",
            display_name,
            handle.short(),
            call_id
        );
        (handle, audio_rx, transcription_rx)
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
    ) -> (
        Handle,
        broadcast::Receiver<(Handle, Vec<i16>)>,
        broadcast::Receiver<TranscriptionEvent>,
    ) {
        // AI participants always get server-side buffering
        let (handle, audio_rx, transcription_rx) =
            self.join_call(call_id, user_id, display_name, true).await;

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

        (handle, audio_rx, transcription_rx)
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
                .ok_or_else(|| format!("Call '{}' not found", call_id))?;
            let call = call.read().await;
            let handle = call.mixer.find_handle_by_user_id(user_id)
                .ok_or_else(|| format!("User '{}' not in call '{}'", user_id, call_id))?;
            let display_name = call.mixer.get_participant(&handle)
                .map(|p| p.display_name.clone())
                .unwrap_or_else(|| user_id.to_string());
            (handle, display_name)
        };

        // Step 2: Synthesize (blocking TTS, creates own runtime)
        let synthesis = tts_service::synthesize_speech_sync(text, voice, adapter)
            .map_err(|e| format!("TTS failed: {}", e))?;

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
                .ok_or_else(|| format!("Call '{}' not found", call_id))?;
            let call = call.read().await;
            let handle = call.mixer.find_handle_by_user_id(user_id)
                .ok_or_else(|| format!("User '{}' not in call '{}'", user_id, call_id))?;
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
                                let (handle, mut audio_rx, mut transcription_rx) = manager.join_call(&call_id, &user_id, &display_name, is_ai).await;
                                participant_handle = Some(handle);

                                // Start audio forwarding task - BINARY WebSocket frames (not JSON+base64)
                                // This eliminates base64 encoding overhead (~33%) for real-time audio
                                let msg_tx_audio = msg_tx.clone();
                                tokio::spawn(async move {
                                    while let Ok((target_handle, audio)) = audio_rx.recv().await {
                                        // Only send if this is audio meant for us
                                        if target_handle == handle {
                                            // Send raw i16 PCM as binary WebSocket frame (little-endian)
                                            // NO JSON, NO base64 - direct bytes transfer
                                            let bytes: Vec<u8> = audio
                                                .iter()
                                                .flat_map(|&s| s.to_le_bytes())
                                                .collect();
                                            if msg_tx_audio.send(Message::Binary(bytes)).await.is_err() {
                                                break;
                                            }
                                        }
                                    }
                                });

                                // Start transcription forwarding task
                                let msg_tx_transcription = msg_tx.clone();
                                let ws_display_name = display_name.clone();
                                tokio::spawn(async move {
                                    while let Ok(event) = transcription_rx.recv().await {
                                        info!("[STEP 7] 🌐 WebSocket sending transcription to {}: \"{}\"",
                                            ws_display_name, event.text.chars().take(TEXT_PREVIEW_LENGTH).collect::<String>());
                                        // Send transcription to all participants
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
                            Ok(_) => {
                                // Ignore other message types from client
                            }
                            Err(e) => {
                                warn!("Failed to parse message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        // Binary audio data (raw i16 PCM, little-endian)
                        // Skip processing if muted at connection level
                        if is_muted {
                            continue;
                        }
                        if let Some(handle) = &participant_handle {
                            let samples = bytes_to_i16(&data);
                            manager.push_audio(handle, samples).await;
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
        let (handle, _rx, _transcription_rx) =
            manager.join_call("test-call", "user-1", "Alice", false).await;

        // Check stats
        let stats = manager.get_stats(&handle).await;
        assert!(stats.is_some());
        let (count, _) = stats.unwrap();
        assert_eq!(count, 1);

        // Leave call
        manager.leave_call(&handle).await;

        // Stats should be gone
        let stats = manager.get_stats(&handle).await;
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_call_manager_multi_participant() {
        let manager = CallManager::new();

        // Two participants join (humans)
        let (handle_a, _rx_a, _transcription_rx_a) =
            manager.join_call("test-call", "user-a", "Alice", false).await;
        let (handle_b, _rx_b, _transcription_rx_b) =
            manager.join_call("test-call", "user-b", "Bob", false).await;

        // Check count
        let stats = manager.get_stats(&handle_a).await;
        assert_eq!(stats.unwrap().0, 2);

        // Push audio from Alice (buffered, mixed by audio loop)
        let audio = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        manager.push_audio(&handle_a, audio).await;

        // Give audio loop time to tick
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check samples processed (audio loop should have ticked)
        let stats = manager.get_stats(&handle_a).await;
        assert!(stats.unwrap().1 > 0);

        // Leave
        manager.leave_call(&handle_a).await;
        manager.leave_call(&handle_b).await;
    }

    #[tokio::test]
    async fn test_mute() {
        let manager = CallManager::new();

        let (handle, _rx, _transcription_rx) =
            manager.join_call("test-call", "user-1", "Alice", false).await;

        // Mute
        manager.set_mute(&handle, true).await;

        // Unmute
        manager.set_mute(&handle, false).await;

        manager.leave_call(&handle).await;
    }
}
