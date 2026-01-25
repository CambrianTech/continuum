//! Hold Music Integration Test
//!
//! Tests that hold music plays when a participant joins a call alone.
//! This is the SIMPLEST test - no TTS, no AI, just verify audio mixing works.

use continuum_core::voice::call_server::CallManager;
use continuum_core::voice::orchestrator::VoiceOrchestrator;
use std::sync::Arc;
use std::time::Duration;

/// Check if audio samples are effectively silence (RMS below threshold)
fn is_silence(samples: &[i16]) -> bool {
    if samples.is_empty() {
        return true;
    }
    let sum_squares: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
    let rms = (sum_squares / samples.len() as f64).sqrt();
    rms < 50.0 // Very low threshold - basically only true silence
}

#[tokio::test]
async fn test_hold_music_plays_when_alone() {
    // STEP 1: Create CallManager with orchestrator
    let orchestrator = Arc::new(VoiceOrchestrator::new());
    let manager = CallManager::new(orchestrator);

    // STEP 2: Join a call as single participant
    let (handle, mut audio_rx, _transcription_rx) =
        manager.join_call("test-hold-music", "user-1", "Alice").await;

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

                    if !is_silence(&audio) {
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

/// Calculate RMS (root mean square) of audio samples
fn calculate_rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
    (sum_squares / samples.len() as f64).sqrt()
}
