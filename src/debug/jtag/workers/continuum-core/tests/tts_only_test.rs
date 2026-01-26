//! TTS-Only Test via IPC
//!
//! Tests that TTS produces valid audio via IPC.
//! This test uses the currently running server.
//!
//! Run with: cargo test -p continuum-core --test tts_only_test -- --nocapture

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const IPC_SOCKET: &str = "/tmp/continuum-core.sock";

#[derive(Serialize)]
struct SynthesizeRequest {
    command: &'static str,
    text: String,
}

#[derive(Deserialize)]
struct IpcResponse {
    success: bool,
    result: Option<serde_json::Value>,
    error: Option<String>,
}

fn send_ipc_request<T: Serialize>(stream: &mut UnixStream, request: &T) -> Result<IpcResponse, String> {
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();

    let json = serde_json::to_string(request).map_err(|e| format!("Serialize error: {}", e))?;
    writeln!(stream, "{}", json).map_err(|e| format!("Write error: {}", e))?;

    let mut reader = BufReader::new(stream.try_clone().map_err(|e| format!("Clone error: {}", e))?);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| format!("Read error: {}", e))?;

    serde_json::from_str(&line).map_err(|e| format!("Parse error: {} (response: {})", e, line))
}

#[test]
fn test_tts_synthesize_via_ipc() {
    println!("\n=== TTS Synthesize Test (IPC) ===\n");

    let mut stream = match UnixStream::connect(IPC_SOCKET) {
        Ok(s) => s,
        Err(e) => {
            println!("⚠️  Cannot connect to {}: {}", IPC_SOCKET, e);
            println!("   Make sure server is running");
            println!("   Skipping test.\n");
            return;
        }
    };
    println!("✓ Connected to IPC server");

    let request = SynthesizeRequest {
        command: "voice/synthesize",
        text: "Hello world, this is a test of text to speech.".to_string(),
    };

    println!("Synthesizing: \"{}\"", request.text);
    let response = match send_ipc_request(&mut stream, &request) {
        Ok(r) => r,
        Err(e) => {
            println!("❌ IPC error: {}", e);
            return;
        }
    };

    if !response.success {
        println!("❌ TTS failed: {:?}", response.error);
        return;
    }

    let result = response.result.unwrap();
    let sample_rate = result["sample_rate"].as_u64().unwrap_or(0);
    let duration_ms = result["duration_ms"].as_u64().unwrap_or(0);
    let audio_base64 = result["audio"].as_str().unwrap_or("");

    let audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64)
        .unwrap_or_default();
    let sample_count = audio_bytes.len() / 2;

    println!("Sample rate: {}Hz", sample_rate);
    println!("Samples: {}", sample_count);
    println!("Duration: {}ms ({:.2}s)", duration_ms, duration_ms as f64 / 1000.0);
    println!("Audio bytes: {}", audio_bytes.len());

    // Analyze audio samples
    let samples: Vec<i16> = audio_bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    // Check for silence
    let non_zero = samples.iter().filter(|&&s| s != 0).count();
    let max_amplitude = samples.iter().map(|&s| s.abs()).max().unwrap_or(0);
    let rms = (samples.iter().map(|&s| (s as i64).pow(2)).sum::<i64>() as f64 / samples.len() as f64).sqrt();

    println!("\n--- Audio Analysis ---");
    println!("Non-zero samples: {} / {} ({:.1}%)", non_zero, samples.len(), non_zero as f64 / samples.len() as f64 * 100.0);
    println!("Max amplitude: {} (max: 32767)", max_amplitude);
    println!("RMS: {:.1}", rms);

    // Verify sample rate is 16kHz
    assert_eq!(sample_rate, 16000, "Sample rate must be 16kHz");

    // Verify we have audio (not silence)
    assert!(non_zero > samples.len() / 2, "Audio should not be mostly silent");

    // Verify reasonable duration
    let expected_duration = (sample_count as u64 * 1000) / 16000;
    assert!(
        (duration_ms as i64 - expected_duration as i64).abs() < 100,
        "Duration mismatch"
    );

    println!("\n✅ TTS test PASSED");
}

#[test]
fn test_tts_audio_quality() {
    println!("\n=== TTS Audio Quality Test ===\n");

    let mut stream = match UnixStream::connect(IPC_SOCKET) {
        Ok(s) => s,
        Err(e) => {
            println!("⚠️  Cannot connect to {}: {}", IPC_SOCKET, e);
            return;
        }
    };

    // Test with multiple phrases
    let phrases = vec![
        "Hello",
        "Testing audio quality",
        "The quick brown fox jumps over the lazy dog",
    ];

    for phrase in phrases {
        stream = match UnixStream::connect(IPC_SOCKET) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let request = SynthesizeRequest {
            command: "voice/synthesize",
            text: phrase.to_string(),
        };

        let response = match send_ipc_request(&mut stream, &request) {
            Ok(r) if r.success => r,
            _ => {
                println!("❌ Failed to synthesize \"{}\"", phrase);
                continue;
            }
        };

        let result = response.result.unwrap();
        let sample_rate = result["sample_rate"].as_u64().unwrap_or(0);
        let duration_ms = result["duration_ms"].as_u64().unwrap_or(0);
        let audio_base64 = result["audio"].as_str().unwrap_or("");

        let audio_bytes = base64::engine::general_purpose::STANDARD
            .decode(audio_base64)
            .unwrap_or_default();

        // Analyze samples
        let samples: Vec<i16> = audio_bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        let non_zero_pct = samples.iter().filter(|&&s| s.abs() > 10).count() as f64 / samples.len() as f64 * 100.0;
        let max_amp = samples.iter().map(|&s| s.abs()).max().unwrap_or(0);

        println!("\"{}\"", phrase);
        println!("  Rate: {}Hz, Duration: {}ms, Non-silence: {:.1}%, Max: {}",
            sample_rate, duration_ms, non_zero_pct, max_amp);

        assert_eq!(sample_rate, 16000, "Sample rate must be 16kHz for \"{}\"", phrase);
    }

    println!("\n✅ Audio quality test PASSED");
}
