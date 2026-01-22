//! WebSocket Call Server
//!
//! Handles live audio/video calls over WebSocket.
//! Each call has multiple participants, audio is mixed with mix-minus.

use crate::handle::Handle;
use crate::mixer::{AudioMixer, ParticipantStream};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Cursor;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};
use ts_rs::TS;
use once_cell::sync::Lazy;

/// Embedded hold music WAV file (16kHz, mono, 16-bit)
static HOLD_MUSIC_WAV: &[u8] = include_bytes!("../assets/hold-music.wav");

/// Pre-decoded hold music samples (lazy loaded once on first use)
static HOLD_MUSIC_SAMPLES: Lazy<Vec<i16>> = Lazy::new(|| {
    let cursor = Cursor::new(HOLD_MUSIC_WAV);
    match hound::WavReader::new(cursor) {
        Ok(mut reader) => {
            let samples: Vec<i16> = reader
                .samples::<i16>()
                .filter_map(|s| s.ok())
                .collect();
            info!("Loaded hold music: {} samples ({:.1}s at 16kHz)",
                  samples.len(), samples.len() as f32 / 16000.0);
            samples
        }
        Err(e) => {
            error!("Failed to decode hold music WAV: {}", e);
            Vec::new()
        }
    }
});

/// Check if audio samples are effectively silence (RMS below threshold)
fn is_silence(samples: &[i16]) -> bool {
    if samples.is_empty() {
        return true;
    }
    let sum_squares: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
    let rms = (sum_squares / samples.len() as f64).sqrt();
    rms < 50.0  // Very low threshold - basically only true silence
}

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
    },

    /// Leave the call
    Leave,

    /// Audio data (base64 encoded i16 PCM)
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

    /// Mixed audio to play (base64 encoded i16 PCM)
    MixedAudio { data: String },

    /// Error message
    Error { message: String },

    /// Call stats
    Stats {
        participant_count: usize,
        samples_processed: u64,
    },
}

/// Audio stream configuration
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub frame_size: usize,
    pub frame_duration_ms: u64,
}

impl Default for AudioConfig {
    fn default() -> Self {
        // 512 samples at 16kHz = 32ms per frame
        Self {
            sample_rate: 16000,
            frame_size: 512,
            frame_duration_ms: 32,
        }
    }
}

/// A single call instance with server-driven audio clock
pub struct Call {
    pub id: String,
    pub mixer: AudioMixer,
    /// Broadcast channel for sending mixed audio to participants
    pub audio_tx: broadcast::Sender<(Handle, Vec<i16>)>,
    /// Total samples processed (for stats)
    pub samples_processed: u64,
    /// Current position in hold music (sample index)
    hold_music_position: usize,
    /// Audio configuration
    pub config: AudioConfig,
    /// Shutdown signal for the audio loop
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl Call {
    pub fn new(id: String) -> Self {
        let (audio_tx, _) = broadcast::channel(64);
        Self {
            id,
            mixer: AudioMixer::default_voice(),
            audio_tx,
            samples_processed: 0,
            hold_music_position: 0,
            config: AudioConfig::default(),
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
    pub fn push_audio(&mut self, from_handle: &Handle, samples: Vec<i16>) {
        self.mixer.push_audio(from_handle, samples);
    }

    /// Generate one frame of mixed audio for all participants (called by audio loop)
    pub fn tick(&mut self) -> Vec<(Handle, Vec<i16>)> {
        let frame_size = self.config.frame_size;
        self.samples_processed += frame_size as u64;

        let is_alone = self.mixer.participant_count() == 1;
        let mixes = self.mixer.mix_minus_all();

        mixes.into_iter().map(|(handle, mixed_audio)| {
            // If alone, mix in hold tone
            let audio = if is_alone && is_silence(&mixed_audio) {
                self.generate_hold_tone(frame_size)
            } else {
                mixed_audio
            };
            (handle, audio)
        }).collect()
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
}

impl CallManager {
    pub fn new() -> Self {
        Self {
            calls: RwLock::new(HashMap::new()),
            participant_calls: RwLock::new(HashMap::new()),
            audio_loops: RwLock::new(HashMap::new()),
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
            self.start_audio_loop(call_id.to_string(), call.clone()).await;

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
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_millis(frame_duration_ms)
            );

            info!("Audio loop started for call {} ({}ms frames)", call_id_clone, frame_duration_ms);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let mut c = call_clone.write().await;

                        // Only tick if there are participants
                        if c.mixer.participant_count() == 0 {
                            continue;
                        }

                        // Generate mixed audio for all participants
                        let mixes = c.tick();

                        // Broadcast to all
                        for (handle, audio) in mixes {
                            let _ = c.audio_tx.send((handle, audio));
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
    pub async fn join_call(
        &self,
        call_id: &str,
        user_id: &str,
        display_name: &str,
    ) -> (Handle, broadcast::Receiver<(Handle, Vec<i16>)>) {
        let call = self.get_or_create_call(call_id).await;
        let handle = Handle::new();

        // Add participant to call
        {
            let mut call = call.write().await;
            let stream = ParticipantStream::new(handle, user_id.to_string(), display_name.to_string());
            call.mixer.add_participant(stream);
        }

        // Track participant -> call mapping
        {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.insert(handle, call_id.to_string());
        }

        // Subscribe to audio broadcasts
        let rx = {
            let call = call.read().await;
            call.audio_tx.subscribe()
        };

        info!("Participant {} ({}) joined call {}", display_name, handle.short(), call_id);
        (handle, rx)
    }

    /// Leave a call
    pub async fn leave_call(&self, handle: &Handle) {
        let call_id = {
            let mut participant_calls = self.participant_calls.write().await;
            participant_calls.remove(handle)
        };

        if let Some(call_id) = call_id {
            let should_cleanup = {
                let calls = self.calls.read().await;
                if let Some(call) = calls.get(&call_id) {
                    let mut call = call.write().await;
                    if let Some(stream) = call.mixer.remove_participant(handle) {
                        info!("Participant {} ({}) left call {}", stream.display_name, handle.short(), call_id);
                    }
                    // Check if call is now empty
                    call.mixer.participant_count() == 0
                } else {
                    false
                }
            };

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
    pub async fn push_audio(&self, handle: &Handle, samples: Vec<i16>) {
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let mut call = call.write().await;
                call.push_audio(handle, samples);
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
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    manager: Arc<CallManager>,
) {
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
                            Ok(CallMessage::Join { call_id, user_id, display_name }) => {
                                let (handle, mut audio_rx) = manager.join_call(&call_id, &user_id, &display_name).await;
                                participant_handle = Some(handle);

                                // Start audio forwarding task
                                let msg_tx = msg_tx.clone();
                                tokio::spawn(async move {
                                    while let Ok((target_handle, audio)) = audio_rx.recv().await {
                                        // Only send if this is audio meant for us
                                        if target_handle == handle {
                                            let data = base64_encode_i16(&audio);
                                            let msg = CallMessage::MixedAudio { data };
                                            if let Ok(json) = serde_json::to_string(&msg) {
                                                if msg_tx.send(Message::Text(json.into())).await.is_err() {
                                                    break;
                                                }
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
                                if let Some(handle) = &participant_handle {
                                    if let Some(samples) = base64_decode_i16(&data) {
                                        manager.push_audio(handle, samples).await;
                                    }
                                }
                            }
                            Ok(CallMessage::Mute { muted }) => {
                                if let Some(handle) = &participant_handle {
                                    manager.set_mute(handle, muted).await;
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
                        // Binary audio data (raw i16 PCM, little-endian)
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

/// Start the WebSocket call server
pub async fn start_call_server(addr: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr).await?;
    let manager = Arc::new(CallManager::new());

    info!("Call server listening on {}", addr);

    loop {
        let (stream, addr) = listener.accept().await?;
        let manager = manager.clone();
        tokio::spawn(handle_connection(stream, addr, manager));
    }
}

// Helper functions for base64 encoding/decoding i16 audio

fn base64_encode_i16(samples: &[i16]) -> String {
    let bytes: Vec<u8> = samples
        .iter()
        .flat_map(|&s| s.to_le_bytes())
        .collect();
    base64_encode(&bytes)
}

fn base64_decode_i16(data: &str) -> Option<Vec<i16>> {
    let bytes = base64_decode(data)?;
    if bytes.len() % 2 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect(),
    )
}

fn bytes_to_i16(data: &[u8]) -> Vec<i16> {
    if data.len() % 2 != 0 {
        return Vec::new();
    }
    data.chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
}

// Simple base64 encoding (no external dependency)
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(data: &str) -> Option<Vec<u8>> {
    const DECODE: [i8; 128] = [
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,
        -1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,62,-1,-1,-1,63,
        52,53,54,55,56,57,58,59,60,61,-1,-1,-1,-1,-1,-1,
        -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,
        15,16,17,18,19,20,21,22,23,24,25,-1,-1,-1,-1,-1,
        -1,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,
        41,42,43,44,45,46,47,48,49,50,51,-1,-1,-1,-1,-1,
    ];

    let data = data.trim_end_matches('=');
    let mut result = Vec::with_capacity(data.len() * 3 / 4);

    for chunk in data.as_bytes().chunks(4) {
        if chunk.len() < 2 {
            break;
        }

        let b0 = DECODE.get(chunk[0] as usize).copied().unwrap_or(-1);
        let b1 = DECODE.get(chunk[1] as usize).copied().unwrap_or(-1);
        let b2 = chunk.get(2).and_then(|&c| DECODE.get(c as usize).copied()).unwrap_or(0);
        let b3 = chunk.get(3).and_then(|&c| DECODE.get(c as usize).copied()).unwrap_or(0);

        if b0 < 0 || b1 < 0 {
            return None;
        }

        result.push(((b0 << 2) | (b1 >> 4)) as u8);
        if chunk.len() > 2 && b2 >= 0 {
            result.push((((b1 & 0x0f) << 4) | (b2 >> 2)) as u8);
        }
        if chunk.len() > 3 && b3 >= 0 {
            result.push((((b2 & 0x03) << 6) | b3) as u8);
        }
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer::test_utils::*;

    #[test]
    fn test_base64_roundtrip() {
        let samples = generate_sine_wave(440.0, 16000, 320);
        let encoded = base64_encode_i16(&samples);
        let decoded = base64_decode_i16(&encoded).unwrap();
        assert_eq!(samples, decoded);
    }

    #[tokio::test]
    async fn test_call_manager_join_leave() {
        let manager = CallManager::new();

        // Join a call
        let (handle, _rx) = manager.join_call("test-call", "user-1", "Alice").await;

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

        // Two participants join
        let (handle_a, _rx_a) = manager.join_call("test-call", "user-a", "Alice").await;
        let (handle_b, _rx_b) = manager.join_call("test-call", "user-b", "Bob").await;

        // Check count
        let stats = manager.get_stats(&handle_a).await;
        assert_eq!(stats.unwrap().0, 2);

        // Push audio from Alice (buffered, mixed by audio loop)
        let audio = generate_sine_wave(440.0, 16000, 320);
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

        let (handle, _rx) = manager.join_call("test-call", "user-1", "Alice").await;

        // Mute
        manager.set_mute(&handle, true).await;

        // Unmute
        manager.set_mute(&handle, false).await;

        manager.leave_call(&handle).await;
    }
}
