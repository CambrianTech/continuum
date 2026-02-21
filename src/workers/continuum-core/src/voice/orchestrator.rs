use super::types::*;
use crate::clog_info;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct VoiceOrchestrator {
    session_participants: Arc<Mutex<HashMap<Uuid, Vec<VoiceParticipant>>>>,
    session_contexts: Arc<Mutex<HashMap<Uuid, ConversationContext>>>,
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
        }
    }

    pub fn register_session(&self, session_id: Uuid, room_id: Uuid, participants: Vec<VoiceParticipant>) {
        {
            let mut sessions = self.session_participants.lock().unwrap_or_else(|e| e.into_inner());
            sessions.insert(session_id, participants.clone());
        }
        {
            let mut contexts = self.session_contexts.lock().unwrap_or_else(|e| e.into_inner());
            contexts.insert(session_id, ConversationContext::new(session_id, room_id));
        }
        let audio_native_count = participants.iter().filter(|p| p.is_audio_native).count();
        let text_ai_count = participants.iter().filter(|p| matches!(p.participant_type, SpeakerType::Persona) && !p.is_audio_native).count();
        clog_info!("Registered session {} with {} participants ({} text-based AI, {} audio-native)",
                 &session_id.to_string()[..8], participants.len(), text_ai_count, audio_native_count);
    }

    pub fn unregister_session(&self, session_id: Uuid) {
        self.session_participants.lock().unwrap_or_else(|e| e.into_inner()).remove(&session_id);
        self.session_contexts.lock().unwrap_or_else(|e| e.into_inner()).remove(&session_id);
        clog_info!("Unregistered session {}", &session_id.to_string()[..8]);
    }

    /// Process utterance and return ALL AI participant IDs (broadcast model)
    /// Each AI will decide if they want to respond via their own logic
    pub fn on_utterance(&self, event: UtteranceEvent) -> Vec<Uuid> {
        clog_info!("Utterance from {}: \"{}...\"",
                 event.speaker_name, crate::voice::tts::truncate_str(&event.transcript, 50));

        // Get context
        let mut contexts = self.session_contexts.lock().unwrap_or_else(|e| e.into_inner());
        let context = match contexts.get_mut(&event.session_id) {
            Some(ctx) => ctx,
            None => {
                clog_info!("No context for session {}", crate::voice::tts::truncate_str(&event.session_id.to_string(), 8));
                return Vec::new();
            }
        };

        // Update context
        context.add_utterance(event.clone());

        // Get participants
        let participants = self.session_participants.lock().unwrap_or_else(|e| e.into_inner());
        let session_participants = match participants.get(&event.session_id) {
            Some(p) => p,
            None => {
                clog_info!("No participants for session {}", &event.session_id.to_string()[..8]);
                return Vec::new();
            }
        };

        // Get TEXT-BASED AI participants (excluding speaker AND audio-native AIs)
        // Audio-native AIs (Gemini Live, Qwen3-Omni, GPT-4o Realtime) hear raw audio
        // through the mixer's mix-minus stream — sending them transcriptions too would
        // cause them to respond twice (once to audio, once to text).
        let ai_participants: Vec<&VoiceParticipant> = session_participants
            .iter()
            .filter(|p| {
                matches!(p.participant_type, SpeakerType::Persona)
                    && p.user_id != event.speaker_id
                    && !p.is_audio_native
            })
            .collect();

        if ai_participants.is_empty() {
            clog_info!("No text-based AI participants to respond (audio-native AIs hear via mixer)");
            return Vec::new();
        }

        // Broadcast to text-based AI participants only
        clog_info!("Broadcasting to {} text-based AIs (audio-native excluded)", ai_participants.len());

        ai_participants.iter().map(|p| p.user_id).collect()
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
            is_audio_native: false,
        };

        orchestrator.register_session(session_id, room_id, vec![participant]);

        let participants = orchestrator.session_participants.lock().unwrap_or_else(|e| e.into_inner());
        assert!(participants.contains_key(&session_id));
    }

    #[test]
    fn test_broadcast_to_text_based_ais_only() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let speaker_id = Uuid::new_v4();
        let text_ai_id = Uuid::new_v4();
        let audio_native_ai_id = Uuid::new_v4();

        let text_ai = VoiceParticipant {
            user_id: text_ai_id,
            display_name: "Helper AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: false,
        };

        let audio_native_ai = VoiceParticipant {
            user_id: audio_native_ai_id,
            display_name: "Gemini AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: true,
        };

        orchestrator.register_session(session_id, room_id, vec![text_ai, audio_native_ai]);

        let event = UtteranceEvent {
            session_id,
            speaker_id,
            speaker_name: "Joel".to_string(),
            speaker_type: SpeakerType::Human,
            transcript: "This is a statement".to_string(),
            confidence: 0.95,
            timestamp: 1000,
        };

        let responders = orchestrator.on_utterance(event);
        // Only text-based AI should receive transcription
        assert_eq!(responders.len(), 1);
        assert!(responders.contains(&text_ai_id));
        // Audio-native AI excluded (hears via mixer stream)
        assert!(!responders.contains(&audio_native_ai_id));
    }

    #[test]
    fn test_broadcast_to_all_text_ais() {
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
            is_audio_native: false,
        };

        let participant2 = VoiceParticipant {
            user_id: ai2_id,
            display_name: "Teacher AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: false,
        };

        orchestrator.register_session(session_id, room_id, vec![participant1, participant2]);

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
        assert_eq!(responders.len(), 2);
        assert!(responders.contains(&ai1_id));
        assert!(responders.contains(&ai2_id));
    }
}
