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

use crate::voice::{VoiceOrchestrator, UtteranceEvent, VoiceParticipant};
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

    /// Register a voice session with participants
    pub fn register_session(
        &self,
        session_id: &str,
        room_id: &str,
        participants: Vec<VoiceParticipant>,
    ) -> Result<(), String> {
        let session_uuid = Uuid::parse_str(session_id)
            .map_err(|e| format!("Invalid session_id: {}", e))?;
        
        let room_uuid = Uuid::parse_str(room_id)
            .map_err(|e| format!("Invalid room_id: {}", e))?;

        let orchestrator = self.orchestrator.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        
        orchestrator.register_session(session_uuid, room_uuid, participants);
        Ok(())
    }

    /// Process an utterance and get list of AI responders
    pub fn on_utterance(&self, event: UtteranceEvent) -> Result<Vec<Uuid>, String> {
        let orchestrator = self.orchestrator.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        
        Ok(orchestrator.on_utterance(event))
    }

    /// Check if TTS should be routed to a session
    pub fn should_route_tts(&self, session_id: &str, persona_id: &str) -> Result<bool, String> {
        let session_uuid = Uuid::parse_str(session_id)
            .map_err(|e| format!("Invalid session_id: {}", e))?;
        
        let persona_uuid = Uuid::parse_str(persona_id)
            .map_err(|e| format!("Invalid persona_id: {}", e))?;

        let orchestrator = self.orchestrator.lock()
            .map_err(|e| format!("Lock poisoned: {}", e))?;
        
        Ok(orchestrator.should_route_to_tts(session_uuid, persona_uuid))
    }
}

impl Default for VoiceService {
    fn default() -> Self {
        Self::new()
    }
}
