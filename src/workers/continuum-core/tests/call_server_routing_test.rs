//! Call Server Routing Integration Test
//!
//! TDD: Test that call_server properly routes audio based on model capabilities.
//!
//! Scenario:
//! - Human joins call
//! - GPT-4o (audio-native) joins call
//! - Claude (text-only) joins call
//! - Human speaks → GPT-4o gets audio, Claude gets transcription
//! - Claude responds via TTS → GPT-4o should hear it

use continuum_core::voice::call_server::CallManager;

/// Test: Join participants with model info, verify routing setup
#[tokio::test]
async fn test_call_manager_tracks_model_capabilities() {
    let manager = CallManager::new();
    let call_id = "test-call-1";

    // Human joins
    let human_join = manager
        .join_call(call_id, "user-1", "Joel", false)
        .await;

    // GPT-4o joins (audio-native)
    let gpt_join = manager
        .join_call_with_model(call_id, "ai-gpt", "GPT-4o", "gpt-4o-realtime")
        .await;

    // Claude joins (text-only)
    let claude_join = manager
        .join_call_with_model(call_id, "ai-claude", "Claude", "claude-3-sonnet")
        .await;

    // Verify participants are tracked
    // (This test documents the expected API - implementation follows)

    // Cleanup
    manager.leave_call(&human_join.handle).await;
    manager.leave_call(&gpt_join.handle).await;
    manager.leave_call(&claude_join.handle).await;
}

/// Test: Audio routes to audio-capable participants only
#[tokio::test]
async fn test_audio_routes_to_capable_participants() {
    let manager = CallManager::new();
    let call_id = "test-call-2";

    // Human joins
    let human_join = manager
        .join_call(call_id, "user-1", "Joel", false)
        .await;

    // GPT-4o joins (should receive audio)
    let gpt_join = manager
        .join_call_with_model(call_id, "ai-gpt", "GPT-4o", "gpt-4o-realtime")
        .await;

    // Claude joins (should NOT receive raw audio, only transcription)
    let claude_join = manager
        .join_call_with_model(call_id, "ai-claude", "Claude", "claude-3-sonnet")
        .await;

    // Human speaks - push some audio
    let test_audio = vec![100i16; 512]; // One frame
    manager.push_audio(&human_join.handle, test_audio).await;

    // Wait briefly for audio loop to process
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // GPT-4o should receive mixed audio (it can hear)
    // Human should receive mixed audio (everyone hears mixed)
    // Claude should receive transcription when speech completes

    // Cleanup
    manager.leave_call(&human_join.handle).await;
    manager.leave_call(&gpt_join.handle).await;
    manager.leave_call(&claude_join.handle).await;
}

/// Test: TTS from text models routes to audio-native models
#[tokio::test]
async fn test_tts_routes_to_audio_native_models() {
    let manager = CallManager::new();
    let call_id = "test-call-3";

    // GPT-4o joins (should hear Claude's TTS)
    let gpt_join = manager
        .join_call_with_model(call_id, "ai-gpt", "GPT-4o", "gpt-4o-realtime")
        .await;

    // Claude joins
    let claude_join = manager
        .join_call_with_model(call_id, "ai-claude", "Claude", "claude-3-sonnet")
        .await;

    // Claude speaks via TTS - inject TTS audio
    let tts_audio = vec![50i16; 16000]; // 1 second
    manager.inject_tts_audio(
        call_id,
        &claude_join.handle,
        "Claude",
        "Hello from Claude!",
        tts_audio,
    ).await;

    // GPT-4o should receive this TTS audio in its mix
    // (because it can hear audio)

    // Cleanup
    manager.leave_call(&gpt_join.handle).await;
    manager.leave_call(&claude_join.handle).await;
}
