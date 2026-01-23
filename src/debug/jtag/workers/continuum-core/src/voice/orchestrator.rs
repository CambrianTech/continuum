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

    pub fn on_utterance(&self, event: UtteranceEvent) -> Option<Uuid> {
        println!("🎙️ VoiceOrchestrator: Utterance from {}: \"{}...\"",
                 event.speaker_name, &event.transcript[..event.transcript.len().min(50)]);

        // Get context
        let mut contexts = self.session_contexts.lock().unwrap();
        let context = match contexts.get_mut(&event.session_id) {
            Some(ctx) => ctx,
            None => {
                println!("🎙️ VoiceOrchestrator: No context for session {}", &event.session_id.to_string()[..8]);
                return None;
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
                return None;
            }
        };

        // Get AI participants (excluding speaker)
        let ai_participants: Vec<&VoiceParticipant> = session_participants
            .iter()
            .filter(|p| matches!(p.participant_type, SpeakerType::Persona) && p.user_id != event.speaker_id)
            .collect();

        if ai_participants.is_empty() {
            println!("🎙️ VoiceOrchestrator: No AI participants to respond");
            return None;
        }

        // Turn arbitration
        let responder = self.select_responder(&event, &ai_participants, context)?;

        println!("🎙️ VoiceOrchestrator: {} selected to respond via voice", responder.display_name);

        // Track voice responder
        self.voice_responders.lock().unwrap().insert(event.session_id, responder.user_id);
        context.last_responder_id = Some(responder.user_id);

        Some(responder.user_id)
    }

    fn select_responder<'a>(
        &self,
        event: &UtteranceEvent,
        candidates: &[&'a VoiceParticipant],
        context: &ConversationContext,
    ) -> Option<&'a VoiceParticipant> {
        // 1. Check for direct addressing
        if let Some(responder) = self.check_direct_address(event, candidates) {
            println!("🎙️ Arbiter: Selected {} (directed)", responder.display_name);
            return Some(responder);
        }

        // 2. Check for topic relevance
        if let Some(responder) = self.check_relevance(event, candidates) {
            println!("🎙️ Arbiter: Selected {} (relevance)", responder.display_name);
            return Some(responder);
        }

        // 3. Round-robin for questions only
        if self.is_question(&event.transcript) {
            if let Some(responder) = self.round_robin(candidates, context) {
                println!("🎙️ Arbiter: Selected {} (round-robin for question)", responder.display_name);
                return Some(responder);
            }
        }

        // 4. No responder for statements
        println!("🎙️ Arbiter: No responder selected (statement, not question)");
        None
    }

    fn check_direct_address<'a>(&self, event: &UtteranceEvent, candidates: &[&'a VoiceParticipant]) -> Option<&'a VoiceParticipant> {
        let text_lower = event.transcript.to_lowercase();

        for candidate in candidates {
            let name_lower = candidate.display_name.to_lowercase();
            let name_hyphen = name_lower.replace(" ", "-");

            if text_lower.contains(&name_lower)
                || text_lower.contains(&name_hyphen)
                || text_lower.contains(&format!("@{name_lower}"))
                || text_lower.contains(&format!("@{name_hyphen}"))
            {
                return Some(candidate);
            }
        }

        None
    }

    fn check_relevance<'a>(&self, event: &UtteranceEvent, candidates: &[&'a VoiceParticipant]) -> Option<&'a VoiceParticipant> {
        let text_lower = event.transcript.to_lowercase();

        let mut scored: Vec<(&VoiceParticipant, f32)> = candidates
            .iter()
            .map(|candidate| {
                let mut score = 0.0;
                for keyword in &candidate.expertise {
                    if text_lower.contains(&keyword.to_lowercase()) {
                        score += 0.3;
                    }
                }
                (*candidate, score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        if !scored.is_empty() && scored[0].1 > 0.2 {
            return Some(scored[0].0);
        }

        None
    }

    fn round_robin<'a>(&self, candidates: &[&'a VoiceParticipant], context: &ConversationContext) -> Option<&'a VoiceParticipant> {
        if candidates.is_empty() {
            return None;
        }

        if let Some(last_id) = context.last_responder_id {
            if let Some(last_index) = candidates.iter().position(|c| c.user_id == last_id) {
                let next_index = (last_index + 1) % candidates.len();
                return Some(candidates[next_index]);
            }
        }

        Some(candidates[0])
    }

    fn is_question(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        text.contains('?')
            || lower.starts_with("what")
            || lower.starts_with("how")
            || lower.starts_with("why")
            || lower.starts_with("can")
            || lower.starts_with("could")
            || lower.starts_with("should")
            || lower.starts_with("would")
    }

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
mod tests {
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
    fn test_turn_arbitration_question() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let speaker_id = Uuid::new_v4();
        let ai_id = Uuid::new_v4();

        let participant = VoiceParticipant {
            user_id: ai_id,
            display_name: "Helper AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
        };

        orchestrator.register_session(session_id, room_id, vec![participant]);

        let event = UtteranceEvent {
            session_id,
            speaker_id,
            speaker_name: "Joel".to_string(),
            speaker_type: SpeakerType::Human,
            transcript: "Can anyone hear me?".to_string(),
            confidence: 0.95,
            timestamp: 1000,
        };

        let responder = orchestrator.on_utterance(event);
        assert_eq!(responder, Some(ai_id));
    }
}
