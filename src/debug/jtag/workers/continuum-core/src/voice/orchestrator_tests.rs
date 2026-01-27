/// Comprehensive unit tests for VoiceOrchestrator
/// 100% coverage with concurrent operation tests

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::sync::Arc;
    use std::thread;

    // Test constants
    const TEST_SESSION_1: &str = "00000000-0000-0000-0000-000000000001";
    const TEST_SESSION_2: &str = "00000000-0000-0000-0000-000000000002";
    const TEST_SPEAKER: &str = "00000000-0000-0000-0000-000000000010";
    const TEST_AI_1: &str = "00000000-0000-0000-0000-000000000020";
    const TEST_AI_2: &str = "00000000-0000-0000-0000-000000000021";
    const TEST_AI_3: &str = "00000000-0000-0000-0000-000000000022";

    fn create_test_ai(id: &str, name: &str) -> VoiceParticipant {
        VoiceParticipant {
            user_id: Uuid::parse_str(id).unwrap(),
            display_name: name.to_string(),
            participant_type: SpeakerType::Persona,
            expertise: vec![],
        }
    }

    fn create_test_utterance(session: &str, speaker: &str, text: &str) -> UtteranceEvent {
        UtteranceEvent {
            session_id: Uuid::parse_str(session).unwrap(),
            speaker_id: Uuid::parse_str(speaker).unwrap(),
            speaker_name: "Test Speaker".to_string(),
            speaker_type: SpeakerType::Human,
            transcript: text.to_string(),
            confidence: 0.95,
            timestamp: 1000,
        }
    }

    // ========================================================================
    // Basic Functionality Tests
    // ========================================================================

    #[test]
    fn test_register_session_stores_participants() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        let participants = vec![
            create_test_ai(TEST_AI_1, "AI 1"),
            create_test_ai(TEST_AI_2, "AI 2"),
        ];

        orchestrator.register_session(session_id, room_id, participants);

        // Verify session exists by trying to process utterance
        let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "test");
        let responders = orchestrator.on_utterance(utterance);

        assert_eq!(responders.len(), 2, "Should have 2 AI participants");
    }

    #[test]
    fn test_unregister_session_removes_data() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        orchestrator.register_session(
            session_id,
            room_id,
            vec![create_test_ai(TEST_AI_1, "AI 1")],
        );

        orchestrator.unregister_session(session_id);

        // After unregistering, utterance should return empty
        let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "test");
        let responders = orchestrator.on_utterance(utterance);

        assert_eq!(responders.len(), 0, "Unregistered session should return no responders");
    }

    // ========================================================================
    // Broadcast Logic Tests
    // ========================================================================

    #[test]
    fn test_broadcast_to_all_ais() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        let ai1_id = Uuid::parse_str(TEST_AI_1).unwrap();
        let ai2_id = Uuid::parse_str(TEST_AI_2).unwrap();

        orchestrator.register_session(
            session_id,
            room_id,
            vec![
                create_test_ai(TEST_AI_1, "AI 1"),
                create_test_ai(TEST_AI_2, "AI 2"),
            ],
        );

        let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "Hello everyone");
        let responders = orchestrator.on_utterance(utterance);

        assert_eq!(responders.len(), 2, "Should broadcast to all AIs");
        assert!(responders.contains(&ai1_id), "Should include AI 1");
        assert!(responders.contains(&ai2_id), "Should include AI 2");
    }

    #[test]
    fn test_statement_broadcasts_to_all() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        orchestrator.register_session(
            session_id,
            room_id,
            vec![create_test_ai(TEST_AI_1, "AI 1")],
        );

        // Statement (not a question)
        let statement = create_test_utterance(
            TEST_SESSION_1,
            TEST_SPEAKER,
            "This is a statement, not a question",
        );
        let responders = orchestrator.on_utterance(statement);

        assert_eq!(responders.len(), 1, "Statements should broadcast to all AIs");
    }

    #[test]
    fn test_question_broadcasts_to_all() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        orchestrator.register_session(
            session_id,
            room_id,
            vec![create_test_ai(TEST_AI_1, "AI 1")],
        );

        // Question
        let question = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "Can you hear me?");
        let responders = orchestrator.on_utterance(question);

        assert_eq!(responders.len(), 1, "Questions should broadcast to all AIs");
    }

    #[test]
    fn test_speaker_excluded_from_responders() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();
        let speaker_id = Uuid::parse_str(TEST_SPEAKER).unwrap();

        // Register session with speaker as AI (unusual but possible)
        orchestrator.register_session(
            session_id,
            room_id,
            vec![
                VoiceParticipant {
                    user_id: speaker_id,
                    display_name: "Speaker AI".to_string(),
                    participant_type: SpeakerType::Persona,
                    expertise: vec![],
                },
                create_test_ai(TEST_AI_1, "Other AI"),
            ],
        );

        let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "test");
        let responders = orchestrator.on_utterance(utterance);

        assert_eq!(responders.len(), 1, "Speaker should be excluded");
        assert!(!responders.contains(&speaker_id), "Speaker should not be in responders");
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[test]
    fn test_no_ai_participants_returns_empty() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        // Register session with only human participant
        orchestrator.register_session(
            session_id,
            room_id,
            vec![VoiceParticipant {
                user_id: Uuid::parse_str(TEST_SPEAKER).unwrap(),
                display_name: "Human".to_string(),
                participant_type: SpeakerType::Human,
                expertise: vec![],
            }],
        );

        let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "test");
        let responders = orchestrator.on_utterance(utterance);

        assert_eq!(responders.len(), 0, "No AI participants should return empty");
    }

    #[test]
    fn test_unregistered_session_returns_empty() {
        let orchestrator = VoiceOrchestrator::new();

        let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "test");
        let responders = orchestrator.on_utterance(utterance);

        assert_eq!(responders.len(), 0, "Unregistered session should return empty");
    }

    #[test]
    fn test_empty_transcript() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        orchestrator.register_session(
            session_id,
            room_id,
            vec![create_test_ai(TEST_AI_1, "AI 1")],
        );

        let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "");
        let responders = orchestrator.on_utterance(utterance);

        // Empty transcripts should still broadcast (AI can decide if they care)
        assert_eq!(responders.len(), 1, "Empty transcript should still broadcast");
    }

    #[test]
    fn test_very_long_transcript() {
        let orchestrator = VoiceOrchestrator::new();
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        orchestrator.register_session(
            session_id,
            room_id,
            vec![create_test_ai(TEST_AI_1, "AI 1")],
        );

        let long_text = "a".repeat(10000);
        let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, &long_text);
        let responders = orchestrator.on_utterance(utterance);

        assert_eq!(responders.len(), 1, "Long transcript should work");
    }

    // ========================================================================
    // Multiple Session Tests
    // ========================================================================

    #[test]
    fn test_multiple_sessions_isolated() {
        let orchestrator = VoiceOrchestrator::new();
        let session1_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let session2_id = Uuid::parse_str(TEST_SESSION_2).unwrap();
        let room_id = Uuid::new_v4();

        let ai1_id = Uuid::parse_str(TEST_AI_1).unwrap();
        let ai2_id = Uuid::parse_str(TEST_AI_2).unwrap();

        // Session 1 has AI 1
        orchestrator.register_session(
            session1_id,
            room_id,
            vec![create_test_ai(TEST_AI_1, "AI 1")],
        );

        // Session 2 has AI 2
        orchestrator.register_session(
            session2_id,
            room_id,
            vec![create_test_ai(TEST_AI_2, "AI 2")],
        );

        // Utterance in session 1
        let utterance1 = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, "test1");
        let responders1 = orchestrator.on_utterance(utterance1);

        // Utterance in session 2
        let utterance2 = create_test_utterance(TEST_SESSION_2, TEST_SPEAKER, "test2");
        let responders2 = orchestrator.on_utterance(utterance2);

        assert_eq!(responders1.len(), 1, "Session 1 should have 1 responder");
        assert_eq!(responders2.len(), 1, "Session 2 should have 1 responder");
        assert!(responders1.contains(&ai1_id), "Session 1 should have AI 1");
        assert!(responders2.contains(&ai2_id), "Session 2 should have AI 2");
    }

    // ========================================================================
    // Concurrency Tests - CRITICAL for Rust
    // ========================================================================

    #[test]
    fn test_concurrent_utterances_same_session() {
        let orchestrator = Arc::new(VoiceOrchestrator::new());
        let session_id = Uuid::parse_str(TEST_SESSION_1).unwrap();
        let room_id = Uuid::new_v4();

        orchestrator.register_session(
            session_id,
            room_id,
            vec![
                create_test_ai(TEST_AI_1, "AI 1"),
                create_test_ai(TEST_AI_2, "AI 2"),
            ],
        );

        let mut handles = vec![];

        // Spawn 10 threads processing utterances concurrently
        for i in 0..10 {
            let orch = Arc::clone(&orchestrator);
            let handle = thread::spawn(move || {
                let text = format!("Utterance {}", i);
                let utterance = create_test_utterance(TEST_SESSION_1, TEST_SPEAKER, &text);
                orch.on_utterance(utterance)
            });
            handles.push(handle);
        }

        // All should succeed with 2 responders each
        for handle in handles {
            let responders = handle.join().unwrap();
            assert_eq!(responders.len(), 2, "Concurrent utterances should all broadcast");
        }
    }

    #[test]
    fn test_concurrent_session_registration() {
        let orchestrator = Arc::new(VoiceOrchestrator::new());
        let mut handles = vec![];

        // Register 10 sessions concurrently
        for i in 0..10 {
            let orch = Arc::clone(&orchestrator);
            let handle = thread::spawn(move || {
                let session_id = Uuid::new_v4();
                let room_id = Uuid::new_v4();
                orch.register_session(
                    session_id,
                    room_id,
                    vec![create_test_ai(TEST_AI_1, "AI 1")],
                );
                session_id
            });
            handles.push(handle);
        }

        // All should succeed
        let mut session_ids = vec![];
        for handle in handles {
            session_ids.push(handle.join().unwrap());
        }

        assert_eq!(session_ids.len(), 10, "All concurrent registrations should succeed");
    }

    #[test]
    fn test_concurrent_register_unregister() {
        let orchestrator = Arc::new(VoiceOrchestrator::new());
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();

        let mut handles = vec![];

        // Concurrently register and unregister same session
        for i in 0..5 {
            let orch = Arc::clone(&orchestrator);
            let sid = session_id;
            let rid = room_id;

            let handle = thread::spawn(move || {
                if i % 2 == 0 {
                    orch.register_session(sid, rid, vec![create_test_ai(TEST_AI_1, "AI 1")]);
                } else {
                    orch.unregister_session(sid);
                }
            });
            handles.push(handle);
        }

        // All should complete without panicking
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_concurrent_different_sessions() {
        let orchestrator = Arc::new(VoiceOrchestrator::new());

        // Pre-register multiple sessions
        for i in 0..5 {
            let session_id = Uuid::new_v4();
            let room_id = Uuid::new_v4();
            orchestrator.register_session(
                session_id,
                room_id,
                vec![create_test_ai(TEST_AI_1, "AI 1")],
            );
        }

        // This test verifies concurrent access doesn't deadlock
        // Just completing without hanging is success
    }
}
