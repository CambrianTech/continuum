use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtteranceEvent {
    pub session_id: Uuid,
    pub speaker_id: Uuid,
    pub speaker_name: String,
    pub speaker_type: SpeakerType,
    pub transcript: String,
    pub confidence: f32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeakerType {
    Human,
    Persona,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceParticipant {
    pub user_id: Uuid,
    pub display_name: String,
    pub participant_type: SpeakerType,
    pub expertise: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ConversationContext {
    pub session_id: Uuid,
    pub room_id: Uuid,
    pub recent_utterances: Vec<UtteranceEvent>,
    pub last_responder_id: Option<Uuid>,
    pub turn_count: u32,
}

impl ConversationContext {
    pub fn new(session_id: Uuid, room_id: Uuid) -> Self {
        Self {
            session_id,
            room_id,
            recent_utterances: Vec::new(),
            last_responder_id: None,
            turn_count: 0,
        }
    }

    pub fn add_utterance(&mut self, event: UtteranceEvent) {
        self.recent_utterances.push(event);
        if self.recent_utterances.len() > 20 {
            self.recent_utterances.remove(0);
        }
        self.turn_count += 1;
    }
}
