//! TTS-Only Test via IPC
//!
//! Tests that TTS produces valid audio via IPC.
//! This test uses the currently running server.
//!
//! Run with: cargo test -p continuum-core --release --test tts_only_test -- --nocapture

mod common;

use common::{ipc_connect, ipc_request, IpcResult};
use serde::Serialize;

#[derive(Serialize)]
struct SynthesizeRequest {
    command: &'static str,
    text: String,
}

#[test]
fn test_tts_synthesize_via_ipc() {
    println!("\n=== TTS Synthesize Test (IPC) ===\n");

    let mut stream = match ipc_connect() {
        Some(s) => s,
        None => return,
    };
    println!("Connected to IPC server");

    let request = SynthesizeRequest {
        command: "voice/synthesize",
        text: "Hello world, this is a test of text to speech.".to_string(),
    };

    println!("Synthesizing: \"{}\"", request.text);
    let result = match ipc_request(&mut stream, &request) {
        Ok(r) => r,
        Err(e) => {
            println!("IPC error: {e}");
            return;
        }
    };

    // voice/synthesize returns Binary frame: JSON header + raw PCM
    let (header, pcm_bytes) = match result {
        IpcResult::Binary { header, data } => (header, data),
        IpcResult::Json(resp) => {
            if !resp.success {
                println!("TTS failed: {:?}", resp.error);
            } else {
                println!("Expected binary response, got JSON-only");
            }
            return;
        }
    };

    if !header.success {
        println!("TTS failed: {:?}", header.error);
        return;
    }

    let meta = header.result.unwrap();
    let sample_rate = meta["sample_rate"].as_u64().unwrap_or(0);
    let num_samples = meta["num_samples"].as_u64().unwrap_or(0);
    let duration_ms = meta["duration_ms"].as_u64().unwrap_or(0);

    println!("Sample rate: {sample_rate}Hz");
    println!("Samples: {} (header), {} (from PCM bytes)", num_samples, pcm_bytes.len() / 2);
    println!("Duration: {}ms ({:.2}s)", duration_ms, duration_ms as f64 / 1000.0);
    println!("PCM bytes: {}", pcm_bytes.len());

    // Decode PCM samples from raw binary
    let samples: Vec<i16> = pcm_bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    // Audio analysis
    let non_zero = samples.iter().filter(|&&s| s != 0).count();
    let max_amplitude = samples.iter().map(|&s| s.abs()).max().unwrap_or(0);
    let rms = (samples.iter().map(|&s| (s as i64).pow(2)).sum::<i64>() as f64 / samples.len().max(1) as f64).sqrt();

    println!("\n--- Audio Analysis ---");
    println!("Non-zero samples: {} / {} ({:.1}%)", non_zero, samples.len(), non_zero as f64 / samples.len().max(1) as f64 * 100.0);
    println!("Max amplitude: {max_amplitude} (max: 32767)");
    println!("RMS: {rms:.1}");

    // Verify sample rate is 16kHz
    assert_eq!(sample_rate, 16000, "Sample rate must be 16kHz");

    // Verify we have audio (not silence)
    assert!(non_zero > samples.len() / 2, "Audio should not be mostly silent");

    // Verify PCM byte count matches header
    assert_eq!(
        pcm_bytes.len(),
        num_samples as usize * 2,
        "PCM bytes should be 2 * num_samples"
    );

    // Verify reasonable duration
    let expected_duration = (num_samples * 1000) / 16000;
    assert!(
        (duration_ms as i64 - expected_duration as i64).abs() < 100,
        "Duration mismatch"
    );

    println!("\nTTS test PASSED");
}

#[test]
fn test_tts_audio_quality() {
    println!("\n=== TTS Audio Quality Test ===\n");

    let phrases = vec![
        "Hello",
        "Testing audio quality",
        "The quick brown fox jumps over the lazy dog",
    ];

    for phrase in phrases {
        let mut stream = match ipc_connect() {
            Some(s) => s,
            None => return,
        };

        let request = SynthesizeRequest {
            command: "voice/synthesize",
            text: phrase.to_string(),
        };

        let result = match ipc_request(&mut stream, &request) {
            Ok(r) => r,
            Err(e) => {
                println!("\"{phrase}\" - IPC error: {e}");
                continue;
            }
        };

        let (header, pcm_bytes) = match result {
            IpcResult::Binary { header, data } if header.success => (header, data),
            IpcResult::Binary { header, .. } => {
                println!("\"{}\" - TTS failed: {:?}", phrase, header.error);
                continue;
            }
            IpcResult::Json(resp) => {
                println!("\"{}\" - Failed: {:?}", phrase, resp.error);
                continue;
            }
        };

        let meta = header.result.unwrap();
        let sample_rate = meta["sample_rate"].as_u64().unwrap_or(0);
        let duration_ms = meta["duration_ms"].as_u64().unwrap_or(0);

        // Decode PCM samples
        let samples: Vec<i16> = pcm_bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        let non_zero_pct = samples.iter().filter(|&&s| s.abs() > 10).count() as f64 / samples.len().max(1) as f64 * 100.0;
        let max_amp = samples.iter().map(|&s| s.abs()).max().unwrap_or(0);

        println!("\"{phrase}\"");
        println!("  Rate: {sample_rate}Hz, Duration: {duration_ms}ms, Non-silence: {non_zero_pct:.1}%, Max: {max_amp}");

        assert_eq!(sample_rate, 16000, "Sample rate must be 16kHz for \"{phrase}\"");
    }

    println!("\nAudio quality test PASSED");
}
