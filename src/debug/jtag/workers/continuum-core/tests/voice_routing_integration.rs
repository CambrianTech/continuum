//! Voice Routing Integration Tests
//!
//! Tests the full voice pipeline routing:
//! - Audio routing based on model capabilities
//! - Transcription delivery to text-only models
//! - TTS audio delivery to audio-native models
//!
//! TDD: Write tests first, then implement the integration.

use continuum_core::voice::{
    AudioEvent, AudioRouter,
    ModelCapabilityRegistry, RoutedParticipant,
};

/// Test: Human speaks, both audio and text models receive appropriately
#[tokio::test]
async fn test_human_speech_routes_to_all_models() {
    let router = AudioRouter::new();
    let registry = ModelCapabilityRegistry::new();

    // Add participants
    router.add_participant(RoutedParticipant::human(
        "human-1".into(),
        "Joel".into(),
    )).await;

    router.add_participant(RoutedParticipant::ai(
        "ai-gpt4o".into(),
        "GPT-4o".into(),
        "gpt-4o-realtime",
        &registry,
    )).await;

    router.add_participant(RoutedParticipant::ai(
        "ai-claude".into(),
        "Claude".into(),
        "claude-3-sonnet",
        &registry,
    )).await;

    // Subscribe to events
    let mut event_rx = router.subscribe();

    // Human speaks - route audio
    let test_audio = vec![0.1f32; 16000]; // 1 second of audio
    router.route_audio("human-1", test_audio.clone(), 16000).await;

    // Should receive RawAudio event (for GPT-4o)
    let event = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        event_rx.recv()
    ).await;

    assert!(event.is_ok(), "Should receive audio event");
    match event.unwrap().unwrap() {
        AudioEvent::RawAudio { from_user_id, samples, .. } => {
            assert_eq!(from_user_id, "human-1");
            assert_eq!(samples.len(), 16000);
        }
        _ => panic!("Expected RawAudio event"),
    }

    // Route transcription for text-only models
    router.route_transcription(
        "human-1",
        "Joel",
        "Hello, can you hear me?",
        true,
    ).await;

    let event = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        event_rx.recv()
    ).await;

    assert!(event.is_ok(), "Should receive transcription event");
    match event.unwrap().unwrap() {
        AudioEvent::Transcription { from_user_id, text, is_final, .. } => {
            assert_eq!(from_user_id, "human-1");
            assert_eq!(text, "Hello, can you hear me?");
            assert!(is_final);
        }
        _ => panic!("Expected Transcription event"),
    }
}

/// Test: Text model (Claude) speaks via TTS, audio models should hear it
#[tokio::test]
async fn test_text_model_tts_routes_to_audio_models() {
    let router = AudioRouter::new();
    let registry = ModelCapabilityRegistry::new();

    // Add GPT-4o (can hear) and Claude (speaks via TTS)
    router.add_participant(RoutedParticipant::ai(
        "ai-gpt4o".into(),
        "GPT-4o".into(),
        "gpt-4o-realtime",
        &registry,
    )).await;

    router.add_participant(RoutedParticipant::ai(
        "ai-claude".into(),
        "Claude".into(),
        "claude-3-sonnet",
        &registry,
    )).await;

    let mut event_rx = router.subscribe();

    // Claude speaks via TTS
    let tts_samples = vec![0i16; 24000]; // 1.5 seconds at 16kHz
    router.route_tts_audio(
        "ai-claude",
        "Claude",
        "I can help you with that!",
        tts_samples.clone(),
        16000,
    ).await;

    // GPT-4o should receive TTSAudio event
    let event = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        event_rx.recv()
    ).await;

    assert!(event.is_ok(), "GPT-4o should receive TTS audio event");
    match event.unwrap().unwrap() {
        AudioEvent::TTSAudio { from_user_id, from_display_name, text, samples, .. } => {
            assert_eq!(from_user_id, "ai-claude");
            assert_eq!(from_display_name, "Claude");
            assert_eq!(text, "I can help you with that!");
            assert_eq!(samples.len(), 24000);
        }
        _ => panic!("Expected TTSAudio event"),
    }
}

/// Test: Audio model (GPT-4o) speaks, needs transcription for text models
#[tokio::test]
async fn test_audio_model_speech_transcribed_for_text_models() {
    let router = AudioRouter::new();
    let registry = ModelCapabilityRegistry::new();

    // Add GPT-4o (speaks natively) and Claude (needs transcription)
    router.add_participant(RoutedParticipant::ai(
        "ai-gpt4o".into(),
        "GPT-4o".into(),
        "gpt-4o-realtime",
        &registry,
    )).await;

    router.add_participant(RoutedParticipant::ai(
        "ai-claude".into(),
        "Claude".into(),
        "claude-3-sonnet",
        &registry,
    )).await;

    let mut event_rx = router.subscribe();

    // GPT-4o speaks native audio
    let native_audio = vec![0.5f32; 32000]; // 2 seconds
    router.route_native_audio_response(
        "ai-gpt4o",
        "GPT-4o",
        native_audio.clone(),
        16000,
    ).await;

    // Should receive NativeAudioResponse event
    let event = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        event_rx.recv()
    ).await;

    assert!(event.is_ok());
    match event.unwrap().unwrap() {
        AudioEvent::NativeAudioResponse { from_user_id, samples, .. } => {
            assert_eq!(from_user_id, "ai-gpt4o");
            assert_eq!(samples.len(), 32000);
            // Note: Caller is responsible for running STT and routing transcription
        }
        _ => panic!("Expected NativeAudioResponse event"),
    }
}

/// Test: Capability detection for various models
#[test]
fn test_model_capability_detection() {
    let registry = ModelCapabilityRegistry::new();

    // Audio-native models
    assert!(registry.get("gpt-4o").is_audio_native());
    assert!(registry.get("gpt-4o-realtime-preview").is_audio_native());
    assert!(registry.get("gemini-2.0-flash").is_audio_native());

    // Audio input only (can hear but text output)
    let gemini_15 = registry.get("gemini-1.5-pro");
    assert!(gemini_15.audio_input);
    assert!(!gemini_15.audio_output);

    // Text-only models
    assert!(registry.get("claude-3-sonnet").needs_stt());
    assert!(registry.get("claude-3-sonnet").needs_tts());
    assert!(registry.get("llama3").needs_stt());
    assert!(registry.get("mistral").needs_tts());

    // Unknown model defaults to text-only (safe)
    let unknown = registry.get("some-future-model");
    assert!(unknown.needs_stt());
    assert!(unknown.needs_tts());
}

/// Test: Routing decisions are correct for mixed conversation
#[tokio::test]
async fn test_mixed_conversation_routing() {
    let router = AudioRouter::new();
    let registry = ModelCapabilityRegistry::new();

    // Human + 3 AIs with different capabilities
    router.add_participant(RoutedParticipant::human(
        "human".into(), "User".into()
    )).await;

    router.add_participant(RoutedParticipant::ai(
        "gpt4o".into(), "GPT-4o".into(), "gpt-4o-realtime", &registry
    )).await;

    router.add_participant(RoutedParticipant::ai(
        "gemini".into(), "Gemini".into(), "gemini-1.5-pro", &registry
    )).await;

    router.add_participant(RoutedParticipant::ai(
        "claude".into(), "Claude".into(), "claude-3-sonnet", &registry
    )).await;

    // Check who needs what
    let audio_receivers = router.get_participants_needing_audio().await;
    let text_receivers = router.get_participants_needing_transcription().await;

    // Human, GPT-4o, and Gemini 1.5 can hear audio
    assert!(audio_receivers.contains(&"human".to_string()));
    assert!(audio_receivers.contains(&"gpt4o".to_string()));
    assert!(audio_receivers.contains(&"gemini".to_string()));

    // Claude needs transcription
    assert!(text_receivers.contains(&"claude".to_string()));
    // Human doesn't need transcription (they hear directly)
    assert!(!text_receivers.contains(&"human".to_string()));
}

/// Test: Routing summary for debugging
#[tokio::test]
async fn test_routing_summary() {
    let router = AudioRouter::new();
    let registry = ModelCapabilityRegistry::new();

    router.add_participant(RoutedParticipant::human(
        "h1".into(), "Alice".into()
    )).await;
    router.add_participant(RoutedParticipant::ai(
        "a1".into(), "GPT".into(), "gpt-4o", &registry
    )).await;
    router.add_participant(RoutedParticipant::ai(
        "a2".into(), "Claude".into(), "claude-3-sonnet", &registry
    )).await;

    let summary = router.get_routing_summary().await;

    assert!(summary.contains("Alice"));
    assert!(summary.contains("GPT"));
    assert!(summary.contains("Claude"));
    assert!(summary.contains("input=audio")); // Human and GPT
    assert!(summary.contains("input=text"));  // Claude
    assert!(summary.contains("output=TTS"));  // Claude
}
