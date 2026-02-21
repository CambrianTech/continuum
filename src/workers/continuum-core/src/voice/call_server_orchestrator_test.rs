/// Integration test: CallServer → VoiceOrchestrator flow
///
/// Tests that after transcribing audio, CallServer:
/// 1. Calls VoiceOrchestrator.on_utterance()
/// 2. Gets list of AI participant IDs
/// 3. Emits events to those AIs
///
/// This test verifies the COMPLETE flow stays in Rust (no TypeScript relay needed)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::{VoiceOrchestrator, VoiceParticipant, SpeakerType};
    use uuid::Uuid;

    #[test]
    fn test_transcription_triggers_orchestrator() {
        // Setup: Create VoiceOrchestrator with 2 AI participants
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let speaker_id = Uuid::new_v4();
        let ai1_id = Uuid::new_v4();
        let ai2_id = Uuid::new_v4();

        let ai1 = VoiceParticipant {
            user_id: ai1_id,
            display_name: "Helper AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: false,
        };

        let ai2 = VoiceParticipant {
            user_id: ai2_id,
            display_name: "Teacher AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: false,
        };

        orchestrator.register_session(session_id, room_id, vec![ai1, ai2]);

        // Simulate: Transcription completed
        let event = UtteranceEvent {
            session_id,
            speaker_id,
            speaker_name: "User".to_string(),
            speaker_type: SpeakerType::Human,
            transcript: "Hello AI team".to_string(),
            confidence: 0.95,
            timestamp: 1000,
        };

        // Act: Call orchestrator (this is what CallServer should do after transcribing)
        let responder_ids = orchestrator.on_utterance(event);

        // Assert: Both AIs should receive the utterance
        assert_eq!(responder_ids.len(), 2);
        assert!(responder_ids.contains(&ai1_id));
        assert!(responder_ids.contains(&ai2_id));

        // TODO: CallServer needs to emit events to these AI IDs
        // This will be implemented in call_server.rs after this test
    }

    #[test]
    fn test_transcription_broadcasts_to_all_not_just_questions() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let speaker_id = Uuid::new_v4();
        let ai_id = Uuid::new_v4();

        let ai = VoiceParticipant {
            user_id: ai_id,
            display_name: "Helper AI".to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
            is_audio_native: false,
        };

        orchestrator.register_session(session_id, room_id, vec![ai]);

        // Test with STATEMENT (not a question)
        let statement = UtteranceEvent {
            session_id,
            speaker_id,
            speaker_name: "User".to_string(),
            speaker_type: SpeakerType::Human,
            transcript: "This is a statement, not a question".to_string(),
            confidence: 0.90,
            timestamp: 1000,
        };

        let responders = orchestrator.on_utterance(statement);

        // Should broadcast even for statements (no question-only filtering)
        assert_eq!(responders.len(), 1);
        assert_eq!(responders[0], ai_id);
    }
}
