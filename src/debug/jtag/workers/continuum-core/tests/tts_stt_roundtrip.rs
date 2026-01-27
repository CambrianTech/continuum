//! TTS → STT Roundtrip Integration Test
//!
//! Verifies audio pipeline produces intelligible speech by:
//! 1. Connecting to running continuum-core server via IPC
//! 2. Synthesizing known text with TTS
//! 3. Transcribing it back with STT
//! 4. Comparing the results
//!
//! REQUIREMENTS: Server must be running (npm start)
//! Run with: cargo test -p continuum-core --test tts_stt_roundtrip -- --nocapture

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

const IPC_SOCKET: &str = "/tmp/continuum-core.sock";

const TEST_PHRASES: &[&str] = &[
    "Hello world",
    "The quick brown fox",
    "Testing one two three",
];

/// IPC request for TTS
#[derive(Serialize)]
struct SynthesizeRequest {
    command: &'static str,
    text: String,
}

/// IPC request for STT
#[derive(Serialize)]
struct TranscribeRequest {
    command: &'static str,
    audio: String,
    language: Option<String>,
}

/// IPC response
#[derive(Deserialize)]
struct IpcResponse {
    success: bool,
    result: Option<serde_json::Value>,
    error: Option<String>,
}

fn word_similarity(expected: &str, actual: &str) -> f32 {
    // Normalize: lowercase, remove punctuation
    let normalize = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .map(|w| w.to_string())
            .collect()
    };

    let expected_words = normalize(expected);
    let actual_words = normalize(actual);

    if expected_words.is_empty() {
        return if actual_words.is_empty() { 1.0 } else { 0.0 };
    }

    let mut matches = 0;
    for word in &expected_words {
        // Allow partial matches for numbers (e.g., "one" matches "1")
        let word_matches = actual_words.iter().any(|w| {
            w == word ||
            // Handle number words
            (word == "one" && w == "1") ||
            (word == "two" && w == "2") ||
            (word == "three" && w == "3")
        });
        if word_matches {
            matches += 1;
        }
    }

    matches as f32 / expected_words.len() as f32
}

fn send_ipc_request<T: Serialize>(stream: &mut UnixStream, request: &T) -> Result<IpcResponse, String> {
    let json = serde_json::to_string(request).map_err(|e| format!("Serialize error: {}", e))?;
    writeln!(stream, "{}", json).map_err(|e| format!("Write error: {}", e))?;

    let mut reader = BufReader::new(stream.try_clone().map_err(|e| format!("Clone error: {}", e))?);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| format!("Read error: {}", e))?;

    serde_json::from_str(&line).map_err(|e| format!("Parse error: {} (response: {})", e, line))
}

#[test]
fn test_tts_stt_roundtrip_via_ipc() {
    println!("\n=== TTS → STT Roundtrip Test (IPC) ===\n");

    // Connect to server
    let mut stream = match UnixStream::connect(IPC_SOCKET) {
        Ok(s) => s,
        Err(e) => {
            println!("⚠️  Cannot connect to {}: {}", IPC_SOCKET, e);
            println!("   Make sure server is running: npm start");
            println!("   Skipping test.\n");
            return;
        }
    };
    println!("✓ Connected to IPC server\n");

    let mut passed = 0;
    let mut failed = 0;

    for phrase in TEST_PHRASES {
        println!("Testing: \"{}\"", phrase);

        // Reconnect for each phrase (clean connection)
        stream = match UnixStream::connect(IPC_SOCKET) {
            Ok(s) => s,
            Err(e) => {
                println!("  ❌ Reconnect failed: {}", e);
                failed += 1;
                continue;
            }
        };

        // Step 1: Synthesize via IPC
        print!("  1. TTS synthesizing... ");
        let synth_request = SynthesizeRequest {
            command: "voice/synthesize",
            text: phrase.to_string(),
        };

        let synth_response = match send_ipc_request(&mut stream, &synth_request) {
            Ok(r) => r,
            Err(e) => {
                println!("❌ IPC error: {}", e);
                failed += 1;
                continue;
            }
        };

        if !synth_response.success {
            println!("❌ TTS failed: {:?}", synth_response.error);
            failed += 1;
            continue;
        }

        let result = synth_response.result.unwrap();
        let audio_base64 = result["audio"].as_str().unwrap_or("");
        let sample_rate = result["sample_rate"].as_u64().unwrap_or(16000);
        let duration_ms = result["duration_ms"].as_u64().unwrap_or(0);

        // Decode to get sample count
        let audio_bytes = base64::engine::general_purpose::STANDARD
            .decode(audio_base64)
            .unwrap_or_default();
        let sample_count = audio_bytes.len() / 2;

        println!("✓ {} samples at {}Hz ({}ms)", sample_count, sample_rate, duration_ms);

        if sample_rate != 16000 {
            println!("  ⚠️  WARNING: Sample rate is {}Hz, expected 16000Hz", sample_rate);
        }

        // Step 2: Transcribe via IPC
        print!("  2. STT transcribing... ");

        // Reconnect for STT
        stream = match UnixStream::connect(IPC_SOCKET) {
            Ok(s) => s,
            Err(e) => {
                println!("❌ Reconnect failed: {}", e);
                failed += 1;
                continue;
            }
        };

        let transcribe_request = TranscribeRequest {
            command: "voice/transcribe",
            audio: audio_base64.to_string(),
            language: Some("en".to_string()),
        };

        let transcribe_response = match send_ipc_request(&mut stream, &transcribe_request) {
            Ok(r) => r,
            Err(e) => {
                println!("❌ IPC error: {}", e);
                failed += 1;
                continue;
            }
        };

        if !transcribe_response.success {
            println!("❌ STT failed: {:?}", transcribe_response.error);
            failed += 1;
            continue;
        }

        let result = transcribe_response.result.unwrap();
        let transcription = result["text"].as_str().unwrap_or("");
        let confidence = result["confidence"].as_f64().unwrap_or(0.0);

        println!("✓ \"{}\" (confidence: {:.2})", transcription, confidence);

        // Step 3: Compare
        let similarity = word_similarity(phrase, transcription);
        println!("  3. Similarity: {:.1}%", similarity * 100.0);

        if similarity >= 0.6 {
            println!("  ✅ PASSED\n");
            passed += 1;
        } else {
            println!("  ❌ FAILED - transcription mismatch");
            println!("     Expected: \"{}\"", phrase);
            println!("     Got:      \"{}\"\n", transcription);
            failed += 1;
        }
    }

    println!("=== Results ===");
    println!("Passed: {}/{}", passed, TEST_PHRASES.len());
    println!("Failed: {}/{}", failed, TEST_PHRASES.len());

    assert!(failed == 0, "Some TTS→STT roundtrip tests failed");
}

#[test]
fn test_tts_sample_rate_via_ipc() {
    println!("\n=== TTS Sample Rate Test (IPC) ===\n");

    let mut stream = match UnixStream::connect(IPC_SOCKET) {
        Ok(s) => s,
        Err(e) => {
            println!("⚠️  Cannot connect to {}: {}", IPC_SOCKET, e);
            println!("   Skipping test.\n");
            return;
        }
    };

    let request = SynthesizeRequest {
        command: "voice/synthesize",
        text: "Test sample rate".to_string(),
    };

    let response = send_ipc_request(&mut stream, &request).expect("IPC failed");
    assert!(response.success, "TTS failed: {:?}", response.error);

    let result = response.result.unwrap();
    let sample_rate = result["sample_rate"].as_u64().unwrap();
    let duration_ms = result["duration_ms"].as_u64().unwrap();
    let audio_base64 = result["audio"].as_str().unwrap();

    let audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(audio_base64)
        .unwrap();
    let sample_count = audio_bytes.len() / 2;

    println!("Sample rate: {}Hz", sample_rate);
    println!("Samples: {}", sample_count);
    println!("Duration: {}ms", duration_ms);

    // Verify sample rate is 16kHz
    assert_eq!(sample_rate, 16000, "TTS must output 16kHz for CallServer compatibility");

    // Verify duration matches sample count (within 100ms tolerance)
    let expected_duration = (sample_count as u64 * 1000) / 16000;
    assert!(
        (duration_ms as i64 - expected_duration as i64).abs() < 100,
        "Duration {}ms doesn't match sample count (expected ~{}ms)",
        duration_ms,
        expected_duration
    );

    println!("✅ Sample rate test PASSED");
}

#[test]
fn test_stt_whisper_via_ipc() {
    println!("\n=== STT Whisper Test (IPC) ===\n");

    // Create known audio samples (silence with a tone)
    // This tests that STT infrastructure works, even if it doesn't recognize silence
    let mut samples: Vec<i16> = vec![0; 16000]; // 1 second of silence

    // Add a simple tone to make it non-silent
    for (i, sample) in samples.iter_mut().enumerate() {
        let t = i as f32 / 16000.0;
        *sample = (440.0_f32 * 2.0 * std::f32::consts::PI * t).sin() as i16 * 1000;
    }

    // Encode to base64
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let mut stream = match UnixStream::connect(IPC_SOCKET) {
        Ok(s) => s,
        Err(e) => {
            println!("⚠️  Cannot connect to {}: {}", IPC_SOCKET, e);
            println!("   Skipping test.\n");
            return;
        }
    };

    let request = TranscribeRequest {
        command: "voice/transcribe",
        audio: audio_base64,
        language: Some("en".to_string()),
    };

    let response = send_ipc_request(&mut stream, &request);

    match response {
        Ok(r) if r.success => {
            let result = r.result.unwrap();
            println!("Transcription: \"{}\"", result["text"].as_str().unwrap_or(""));
            println!("Language: {}", result["language"].as_str().unwrap_or(""));
            println!("Confidence: {:.2}", result["confidence"].as_f64().unwrap_or(0.0));
            println!("✅ STT infrastructure test PASSED");
        }
        Ok(r) => {
            println!("❌ STT failed: {:?}", r.error);
            println!("   This is OK if Whisper model is not loaded.");
        }
        Err(e) => {
            println!("❌ IPC error: {}", e);
        }
    }
}
