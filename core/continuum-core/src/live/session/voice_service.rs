//! Voice Service - Service layer for voice operations
//!
//! This layer sits between IPC and the domain logic (orchestrator, TTS, etc.)
//! It handles:
//! - UUID validation and parsing
//! - Lock management
//! - Error handling
//! - Coordination between modules
//!
//! IPC should ONLY call these functions, never touch domain logic directly.

use crate::live::{UtteranceEvent, VoiceOrchestrator, VoiceParticipant};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct VoiceService {
    orchestrator: Arc<Mutex<VoiceOrchestrator>>,
}

impl VoiceService {
    pub fn new() -> Self {
        Self {
            orchestrator: Arc::new(Mutex::new(VoiceOrchestrator::new())),
        }
    }

    /// Sessions the orchestrator currently holds — the read side the live-call
    /// projection folds against `CallManager::live_calls()` (#58). The divergence
    /// between the two IS the defect: a live call with no session here is why a
    /// persona sits in a room, present, while `isInCall()` returns false and her
    /// responses are dropped.
    pub fn registered_sessions(&self) -> Vec<(Uuid, Vec<VoiceParticipant>)> {
        self.orchestrator
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .registered_sessions()
    }

    /// Register a voice session with participants
    pub fn register_session(
        &self,
        session_id: &str,
        room_id: &str,
        participants: Vec<VoiceParticipant>,
    ) -> Result<(), String> {
        let session_uuid =
            Uuid::parse_str(session_id).map_err(|e| format!("Invalid session_id: {e}"))?;

        let room_uuid = Uuid::parse_str(room_id).map_err(|e| format!("Invalid room_id: {e}"))?;

        let orchestrator = self
            .orchestrator
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        orchestrator.register_session(session_uuid, room_uuid, participants);
        Ok(())
    }

    /// Process an utterance and get list of AI responders
    pub fn on_utterance(&self, event: UtteranceEvent) -> Result<Vec<Uuid>, String> {
        let orchestrator = self
            .orchestrator
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        Ok(orchestrator.on_utterance(event))
    }

    /// The AI persona viewers for a live call — who should SEE its video frames.
    /// Parses the `call_id` string (the airc session id) and returns the session's AI
    /// roster; empty for a malformed id, an unknown session, or a poisoned lock (a frame
    /// for a call we don't track simply goes nowhere — never a fabricated viewer).
    pub fn video_viewers(&self, call_id: &str) -> Vec<Uuid> {
        let Ok(session_id) = Uuid::parse_str(call_id) else {
            return Vec::new();
        };
        match self.orchestrator.lock() {
            Ok(orchestrator) => orchestrator.video_viewers(session_id),
            Err(_) => Vec::new(),
        }
    }
}

impl Default for VoiceService {
    fn default() -> Self {
        Self::new()
    }
}
