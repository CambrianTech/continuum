use super::types::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct VoiceOrchestrator {
    session_participants: Arc<Mutex<HashMap<Uuid, Vec<VoiceParticipant>>>>,
    session_contexts: Arc<Mutex<HashMap<Uuid, ConversationContext>>>,
    voice_responders: Arc<Mutex<HashMap<Uuid, Uuid>>>, // sessionId -> personaId
}

impl Default for VoiceOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceOrchestrator {
    pub fn new() -> Self {
        Self {
            session_participants: Arc::new(Mutex::new(HashMap::new())),
            session_contexts: Arc::new(Mutex::new(HashMap::new())),
            voice_responders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register_session(&self, session_id: Uuid, room_id: Uuid, participants: Vec<VoiceParticipant>) {
        {
            let mut sessions = self.session_participants.lock().unwrap();
            sessions.insert(session_id, participants.clone());
        }
        {
            let mut contexts = self.session_contexts.lock().unwrap();
            contexts.insert(session_id, ConversationContext::new(session_id, room_id));
        }
        println!("🎙️ VoiceOrchestrator: Registered session {} with {} participants",
                 &session_id.to_string()[..8], participants.len());
    }

    pub fn unregister_session(&self, session_id: Uuid) {
        self.session_participants.lock().unwrap().remove(&session_id);
        self.session_contexts.lock().unwrap().remove(&session_id);
        self.voice_responders.lock().unwrap().remove(&session_id);
        println!("🎙️ VoiceOrchestrator: Unregistered session {}", &session_id.to_string()[..8]);
    }

    /// Process utterance and return ALL AI participant IDs (broadcast model)
    /// Each AI will decide if they want to respond via their own logic
    pub fn on_utterance(&self, event: UtteranceEvent) -> Vec<Uuid> {
        println!("🎙️ VoiceOrchestrator: Utterance from {}: \"{}...\"",
                 event.speaker_name, crate::voice::tts::truncate_str(&event.transcript, 50));

        // Get context
        let mut contexts = self.session_contexts.lock().unwrap();
        let context = match contexts.get_mut(&event.session_id) {
            Some(ctx) => ctx,
            None => {
                println!("🎙️ VoiceOrchestrator: No context for session {}", crate::voice::tts::truncate_str(&event.session_id.to_string(), 8));
                return Vec::new();
            }
        };

        // Update context
        context.add_utterance(event.clone());

        // Get participants
        let participants = self.session_participants.lock().unwrap();
        let session_participants = match participants.get(&event.session_id) {
            Some(p) => p,
            None => {
                println!("🎙️ VoiceOrchestrator: No participants for session {}", &event.session_id.to_string()[..8]);
                return Vec::new();
            }
        };

        // Get AI participants (excluding speaker)
        let ai_participants: Vec<&VoiceParticipant> = session_participants
            .iter()
            .filter(|p| matches!(p.participant_type, SpeakerType::Persona) && p.user_id != event.speaker_id)
            .collect();

        if ai_participants.is_empty() {
            println!("🎙️ VoiceOrchestrator: No AI participants to respond");
            return Vec::new();
        }

        // NO ARBITER - broadcast to ALL AI participants, let THEM decide if they want to respond
        // Their PersonaUser.shouldRespond() logic handles engagement decisions
        println!("🎙️ VoiceOrchestrator: Broadcasting to {} AIs (no filtering)", ai_participants.len());

        ai_participants.iter().map(|p| p.user_id).collect()
    }

    // Arbiter methods removed - no filtering, broadcast to all AIs

    pub fn should_route_to_tts(&self, session_id: Uuid, persona_id: Uuid) -> bool {
        self.voice_responders
            .lock()
            .unwrap()
            .get(&session_id)
            .map(|expected| *expected == persona_id)
            .unwrap_or(false)
    }

    pub fn clear_voice_responder(&self, session_id: Uuid) {
        self.voice_responders.lock().unwrap().remove(&session_id);
    }
}

#[cfg(test)]
#[path = "orchestrator_tests.rs"]
mod orchestrator_tests;

#[cfg(test)]
mod old_tests {
    use super::*;

    #[test]
    fn test_register_session() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let participant = VoiceParticipant {
            user_id: Uuid::new_v4(),
            display_name: "Test AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec!["coding".to_string()],
        };

        orchestrator.register_session(session_id, room_id, vec![participant]);

        let participants = orchestrator.session_participants.lock().unwrap();
        assert!(participants.contains_key(&session_id));
    }

    #[test]
    fn test_broadcast_to_all_ais() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let speaker_id = Uuid::new_v4();
        let ai1_id = Uuid::new_v4();
        let ai2_id = Uuid::new_v4();

        let participant1 = VoiceParticipant {
            user_id: ai1_id,
            display_name: "Helper AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
        };

        let participant2 = VoiceParticipant {
            user_id: ai2_id,
            display_name: "Teacher AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
        };

        orchestrator.register_session(session_id, room_id, vec![participant1, participant2]);

        // Test with a statement (should broadcast to ALL, not filter)
        let event = UtteranceEvent {
            session_id,
            speaker_id,
            speaker_name: "Joel".to_string(),
            speaker_type: SpeakerType::Human,
            transcript: "This is a statement, not a question".to_string(),
            confidence: 0.95,
            timestamp: 1000,
        };

        let responders = orchestrator.on_utterance(event);
        assert_eq!(responders.len(), 2); // Both AIs should receive
        assert!(responders.contains(&ai1_id));
        assert!(responders.contains(&ai2_id));
    }
}
