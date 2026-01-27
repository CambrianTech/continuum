//! Hold Music Integration Test
//!
//! Tests that hold music plays when a participant joins a call alone.
//! This is the SIMPLEST test - no TTS, no AI, just verify audio mixing works.

use continuum_core::voice::call_server::CallManager;
use continuum_core::utils::audio::{is_silence, calculate_rms};
use std::time::Duration;

#[tokio::test]
async fn test_hold_music_plays_when_alone() {
    // STEP 1: Create CallManager
    let manager = CallManager::new();

    // STEP 2: Join a call as single participant (false = not AI)
    let (handle, mut audio_rx, _transcription_rx) =
        manager.join_call("test-hold-music", "user-1", "Alice", false).await;

    println!("✓ Participant joined call");

    // STEP 3: Wait for audio loop to tick (audio loops tick every 32ms)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // STEP 4: Receive audio frames and verify hold music (not silence)
    let mut frame_count = 0;
    let mut non_silence_count = 0;
    let max_frames = 10; // Check 10 frames (~320ms of audio)

    while frame_count < max_frames {
        tokio::select! {
            Ok((target_handle, audio)) = audio_rx.recv() => {
                if target_handle == handle {
                    frame_count += 1;

                    if !is_silence(&audio, 50.0) {
                        non_silence_count += 1;
                        println!("✓ Frame {}: Non-silence audio ({} samples, RMS: {:.1})",
                            frame_count, audio.len(), calculate_rms(&audio));
                    } else {
                        println!("  Frame {}: Silence", frame_count);
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                println!("⚠ Timeout waiting for audio frame {}", frame_count + 1);
                break;
            }
        }
    }

    // STEP 5: Verify hold music played (majority of frames should be non-silence)
    println!("\n=== RESULTS ===");
    println!("Total frames: {}", frame_count);
    println!("Non-silence frames: {}", non_silence_count);
    println!("Hold music ratio: {:.1}%", (non_silence_count as f64 / frame_count as f64) * 100.0);

    // Assert that hold music was playing (at least 50% of frames should be non-silence)
    assert!(
        non_silence_count > frame_count / 2,
        "Hold music should play when alone (expected >50% non-silence, got {}%)",
        (non_silence_count * 100) / frame_count
    );

    // STEP 6: Cleanup
    manager.leave_call(&handle).await;
    println!("✓ Test complete - hold music verified");
}
