//! Stub LiveKitAgentManager — compiled when `livekit-webrtc` feature is disabled.
//!
//! Provides the same public API as the real implementation but returns errors
//! for all operations. This allows the system to compile and run without WebRTC
//! native dependencies (which fail on ARM64 Docker).
//!
//! LiveKit still runs as a separate container for browser-side WebRTC. This stub
//! only disables the Rust-side agent that joins rooms as an AI participant.

use serde::Serialize;
use std::sync::Arc;

/// Stub for LiveKitAgent — no-op implementation.
pub struct LiveKitAgent;

/// Transcription entry (matches real API).
#[derive(Debug, Clone, Serialize)]
pub struct TranscriptionEntry {
    pub call_id: String,
    pub identity: String,
    pub text: String,
    pub timestamp: f64,
    pub is_final: bool,
    pub language: Option<String>,
}

/// Stub LiveKitAgentManager — all operations return errors.
pub struct LiveKitAgentManager {
    url: String,
}

impl LiveKitAgentManager {
    pub fn new() -> Self {
        tracing::warn!("⚠️ LiveKit WebRTC agent disabled (compiled without livekit-webrtc feature)");
        Self {
            url: "ws://localhost:7880".to_string(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn join_as_listener(&self, _call_id: &str) -> Result<(), String> {
        Err("LiveKit WebRTC agent not available (compiled without livekit-webrtc feature)".into())
    }

    pub async fn get_or_create_agent(
        &self,
        _call_id: &str,
        _user_id: &str,
        _display_name: Option<&str>,
    ) -> Result<Arc<LiveKitAgent>, String> {
        Err("LiveKit WebRTC agent not available (compiled without livekit-webrtc feature)".into())
    }

    pub async fn remove_agent(&self, _call_id: &str, _user_id: &str) {}

    pub async fn remove_agents_for_call(&self, _call_id: &str) {}

    pub async fn remove_listener(&self, _call_id: &str) {}

    pub async fn speak_in_call(
        &self,
        _call_id: &str,
        _user_id: &str,
        _text: &str,
        _voice: Option<&str>,
        _adapter: Option<&str>,
        _display_name: Option<&str>,
    ) -> Result<(usize, u64, u32), String> {
        Err("LiveKit WebRTC agent not available (compiled without livekit-webrtc feature)".into())
    }

    pub async fn inject_audio(
        &self,
        _call_id: &str,
        _user_id: &str,
        _samples: Vec<i16>,
    ) -> Result<(), String> {
        Err("LiveKit WebRTC agent not available (compiled without livekit-webrtc feature)".into())
    }

    pub async fn add_ambient_source(
        &self,
        _call_id: &str,
        _name: &str,
    ) -> Result<String, String> {
        Err("LiveKit WebRTC agent not available (compiled without livekit-webrtc feature)".into())
    }

    pub async fn inject_ambient(
        &self,
        _call_id: &str,
        _handle: &str,
        _samples: Vec<i16>,
    ) -> Result<(), String> {
        Err("LiveKit WebRTC agent not available (compiled without livekit-webrtc feature)".into())
    }

    pub async fn remove_ambient_source(
        &self,
        _call_id: &str,
        _handle: &str,
    ) -> Result<(), String> {
        Err("LiveKit WebRTC agent not available (compiled without livekit-webrtc feature)".into())
    }

    pub async fn start_ambient_audio(&self, _call_id: &str) -> Result<(), String> {
        Err("LiveKit WebRTC agent not available (compiled without livekit-webrtc feature)".into())
    }

    pub async fn poll_transcriptions(&self, _call_id: Option<&str>) -> Vec<TranscriptionEntry> {
        Vec::new()
    }
}
