//! WebSocket Call Server
//!
//! Handles live audio/video calls over WebSocket.
//! Each call has multiple participants, audio is mixed with mix-minus.

use crate::handle::Handle;
use crate::mixer::{AudioMixer, ParticipantStream};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};
use ts_rs::TS;

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

/// A single call instance
pub struct Call {
    pub id: String,
    pub mixer: AudioMixer,
    /// Broadcast channel for sending mixed audio to participants
    pub audio_tx: broadcast::Sender<(Handle, Vec<i16>)>,
    /// Total samples processed (for stats)
    pub samples_processed: u64,
}

impl Call {
    pub fn new(id: String) -> Self {
        let (audio_tx, _) = broadcast::channel(64);
        Self {
            id,
            mixer: AudioMixer::default_voice(),
            audio_tx,
            samples_processed: 0,
        }
    }

    /// Process incoming audio and broadcast mix-minus to all participants
    pub fn process_audio(&mut self, from_handle: &Handle, samples: Vec<i16>) {
        // Update the participant's audio buffer
        self.mixer.push_audio(from_handle, samples);
        self.samples_processed += 320; // Frame size

        // Generate mix-minus for all participants
        let mixes = self.mixer.mix_minus_all();

        // Broadcast to all
        for (handle, mixed_audio) in mixes {
            // Ignore send errors (receiver might have dropped)
            let _ = self.audio_tx.send((handle, mixed_audio));
        }
    }
}

/// Call manager - tracks all active calls
pub struct CallManager {
    calls: RwLock<HashMap<String, Arc<RwLock<Call>>>>,
    /// Map participant handle to call ID
    participant_calls: RwLock<HashMap<Handle, String>>,
}

impl CallManager {
    pub fn new() -> Self {
        Self {
            calls: RwLock::new(HashMap::new()),
            participant_calls: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a call
    pub async fn get_or_create_call(&self, call_id: &str) -> Arc<RwLock<Call>> {
        let mut calls = self.calls.write().await;
        if let Some(call) = calls.get(call_id) {
            call.clone()
        } else {
            let call = Arc::new(RwLock::new(Call::new(call_id.to_string())));
            calls.insert(call_id.to_string(), call.clone());
            call
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
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let mut call = call.write().await;
                if let Some(stream) = call.mixer.remove_participant(handle) {
                    info!("Participant {} ({}) left call {}", stream.display_name, handle.short(), call_id);
                }
            }
        }
    }

    /// Process audio from a participant
    pub async fn process_audio(&self, handle: &Handle, samples: Vec<i16>) {
        let call_id = {
            let participant_calls = self.participant_calls.read().await;
            participant_calls.get(handle).cloned()
        };

        if let Some(call_id) = call_id {
            let calls = self.calls.read().await;
            if let Some(call) = calls.get(&call_id) {
                let mut call = call.write().await;
                call.process_audio(handle, samples);
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
                                        manager.process_audio(handle, samples).await;
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
                            manager.process_audio(handle, samples).await;
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

        // Process audio from Alice
        let audio = generate_sine_wave(440.0, 16000, 320);
        manager.process_audio(&handle_a, audio).await;

        // Check samples processed
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
