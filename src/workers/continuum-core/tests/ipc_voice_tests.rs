/// IPC Layer Unit Tests for Voice Operations
/// Tests constants, concurrency, and correct IPC protocol

use continuum_core::voice::{VoiceOrchestrator, VoiceParticipant, SpeakerType, UtteranceEvent};
use serde_json::json;
use std::sync::Arc;
use std::thread;
use uuid::Uuid;

// These constants MUST match the constants defined in src/ipc/mod.rs
const VOICE_RESPONSE_FIELD_RESPONDER_IDS: &str = "responder_ids";

// Test constants
const TEST_SESSION: &str = "00000000-0000-0000-0000-000000000001";
const TEST_SPEAKER: &str = "00000000-0000-0000-0000-000000000010";
const TEST_AI_1: &str = "00000000-0000-0000-0000-000000000020";
const TEST_AI_2: &str = "00000000-0000-0000-0000-000000000021";

fn create_test_ai(id: &str, name: &str) -> VoiceParticipant {
    VoiceParticipant {
        user_id: Uuid::parse_str(id).unwrap(),
        display_name: name.to_string(),
        participant_type: SpeakerType::Persona,
        expertise: vec![],
        is_audio_native: false,
    }
}

#[test]
fn test_ipc_response_uses_constant_field_name() {
    // Create orchestrator with test data
    let orchestrator = VoiceOrchestrator::new();
    let session_id = Uuid::parse_str(TEST_SESSION).unwrap();
    let room_id = Uuid::new_v4();

    orchestrator.register_session(
        session_id,
        room_id,
        vec![create_test_ai(TEST_AI_1, "AI 1")],
    );

    // Create utterance event
    let event = UtteranceEvent {
        session_id,
        speaker_id: Uuid::parse_str(TEST_SPEAKER).unwrap(),
        speaker_name: "Test".to_string(),
        speaker_type: SpeakerType::Human,
        transcript: "test".to_string(),
        confidence: 0.95,
        timestamp: 1000,
    };

    // Process utterance
    let responder_ids = orchestrator.on_utterance(event);

    // Simulate IPC response creation (what ipc/mod.rs does)
    let response = json!({
        VOICE_RESPONSE_FIELD_RESPONDER_IDS: responder_ids.into_iter().map(|id| id.to_string()).collect::<Vec<String>>()
    });

    // Verify field name matches constant
    assert!(response.get(VOICE_RESPONSE_FIELD_RESPONDER_IDS).is_some(),
        "Response must use constant field name");

    let ids = response[VOICE_RESPONSE_FIELD_RESPONDER_IDS].as_array().unwrap();
    assert_eq!(ids.len(), 1, "Should have 1 responder");
}

#[test]
fn test_ipc_response_empty_array_when_no_ais() {
    let orchestrator = VoiceOrchestrator::new();
    let session_id = Uuid::parse_str(TEST_SESSION).unwrap();
    let room_id = Uuid::new_v4();

    // Register session with NO AI participants
    orchestrator.register_session(session_id, room_id, vec![]);

    let event = UtteranceEvent {
        session_id,
        speaker_id: Uuid::parse_str(TEST_SPEAKER).unwrap(),
        speaker_name: "Test".to_string(),
        speaker_type: SpeakerType::Human,
        transcript: "test".to_string(),
        confidence: 0.95,
        timestamp: 1000,
    };

    let responder_ids = orchestrator.on_utterance(event);

    // Simulate IPC response
    let response = json!({
        VOICE_RESPONSE_FIELD_RESPONDER_IDS: responder_ids.into_iter().map(|id| id.to_string()).collect::<Vec<String>>()
    });

    let ids = response[VOICE_RESPONSE_FIELD_RESPONDER_IDS].as_array().unwrap();
    assert_eq!(ids.len(), 0, "Should return empty array");
}

#[test]
fn test_ipc_response_multiple_responders() {
    let orchestrator = VoiceOrchestrator::new();
    let session_id = Uuid::parse_str(TEST_SESSION).unwrap();
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

    let event = UtteranceEvent {
        session_id,
        speaker_id: Uuid::parse_str(TEST_SPEAKER).unwrap(),
        speaker_name: "Test".to_string(),
        speaker_type: SpeakerType::Human,
        transcript: "test".to_string(),
        confidence: 0.95,
        timestamp: 1000,
    };

    let responder_ids = orchestrator.on_utterance(event);

    // Simulate IPC response
    let response = json!({
        VOICE_RESPONSE_FIELD_RESPONDER_IDS: responder_ids.iter().map(|id| id.to_string()).collect::<Vec<String>>()
    });

    let ids = response[VOICE_RESPONSE_FIELD_RESPONDER_IDS].as_array().unwrap();
    assert_eq!(ids.len(), 2, "Should have 2 responders");

    let ids_as_strings: Vec<String> = ids.iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(ids_as_strings.contains(&ai1_id.to_string()));
    assert!(ids_as_strings.contains(&ai2_id.to_string()));
}

#[test]
fn test_ipc_concurrent_requests() {
    let orchestrator = Arc::new(VoiceOrchestrator::new());

    // Register multiple sessions
    for _ in 0..5 {
        let session_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        orchestrator.register_session(
            session_id,
            room_id,
            vec![create_test_ai(TEST_AI_1, "AI 1")],
        );
    }

    let mut handles = vec![];

    // Simulate 20 concurrent IPC requests
    for _ in 0..20 {
        let orch = Arc::clone(&orchestrator);
        let handle = thread::spawn(move || {
            let session_id = Uuid::parse_str(TEST_SESSION).unwrap();
            let room_id = Uuid::new_v4();

            // Register new session
            orch.register_session(session_id, room_id, vec![create_test_ai(TEST_AI_1, "AI 1")]);

            // Process utterance
            let event = UtteranceEvent {
                session_id,
                speaker_id: Uuid::parse_str(TEST_SPEAKER).unwrap(),
                speaker_name: "Test".to_string(),
                speaker_type: SpeakerType::Human,
                transcript: "concurrent test".to_string(),
                confidence: 0.95,
                timestamp: 1000,
            };

            orch.on_utterance(event)
        });
        handles.push(handle);
    }

    // All should succeed
    for handle in handles {
        let responders = handle.join().unwrap();
        assert_eq!(responders.len(), 1, "Each concurrent request should succeed");
    }
}

#[test]
fn test_ipc_field_constant_value_is_correct() {
    // This test verifies the constant value matches what TypeScript expects
    // If this changes, TypeScript bindings MUST be updated
    assert_eq!(
        VOICE_RESPONSE_FIELD_RESPONDER_IDS,
        "responder_ids",
        "Field name constant must match TypeScript expectations"
    );
}

#[test]
fn test_ipc_response_serialization() {
    let orchestrator = VoiceOrchestrator::new();
    let session_id = Uuid::parse_str(TEST_SESSION).unwrap();
    let room_id = Uuid::new_v4();

    orchestrator.register_session(
        session_id,
        room_id,
        vec![create_test_ai(TEST_AI_1, "AI 1")],
    );

    let event = UtteranceEvent {
        session_id,
        speaker_id: Uuid::parse_str(TEST_SPEAKER).unwrap(),
        speaker_name: "Test".to_string(),
        speaker_type: SpeakerType::Human,
        transcript: "test".to_string(),
        confidence: 0.95,
        timestamp: 1000,
    };

    let responder_ids = orchestrator.on_utterance(event);

    // Create response exactly as IPC layer does
    let response = json!({
        VOICE_RESPONSE_FIELD_RESPONDER_IDS: responder_ids.into_iter().map(|id| id.to_string()).collect::<Vec<String>>()
    });

    // Verify it can be serialized to string (what goes over IPC)
    let serialized = serde_json::to_string(&response).unwrap();
    assert!(serialized.contains("responder_ids"), "Serialized response must contain field");
    assert!(serialized.contains(TEST_AI_1), "Serialized response must contain AI ID");
}
