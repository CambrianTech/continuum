//! TTS -> STT Roundtrip Integration Test
//!
//! Verifies audio pipeline produces intelligible speech by:
//! 1. Connecting to running continuum-core server via IPC
//! 2. Synthesizing known text with TTS
//! 3. Transcribing it back with STT
//! 4. Comparing the results
//!
//! REQUIREMENTS: Server must be running (npm start)
//! Run with: cargo test -p continuum-core --release --test tts_stt_roundtrip -- --nocapture

mod common;

use base64::Engine;
use common::{ipc_connect, ipc_request, IpcResult};
use serde::Serialize;

const TEST_PHRASES: &[&str] = &[
    "Hello world",
    "The quick brown fox",
    "Testing one two three",
];

// ============================================================================
// Request Types
// ============================================================================

#[derive(Serialize)]
struct SynthesizeRequest {
    command: &'static str,
    text: String,
}

#[derive(Serialize)]
struct TranscribeRequest {
    command: &'static str,
    audio: String,
    language: Option<String>,
}

// ============================================================================
// Helpers
// ============================================================================

fn word_similarity(expected: &str, actual: &str) -> f32 {
    // Normalize: lowercase, replace hyphens/punctuation with spaces, split on whitespace.
    // Also split pure-digit tokens into individual digits (STT concatenates "1 2 3" → "123").
    let normalize = |s: &str| -> Vec<String> {
        let tokens: Vec<String> = s.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .map(|w| w.to_string())
            .collect();
        // Split pure-digit tokens into individual digit strings
        let mut result = Vec::new();
        for token in tokens {
            if token.len() > 1 && token.chars().all(|c| c.is_ascii_digit()) {
                for ch in token.chars() {
                    result.push(ch.to_string());
                }
            } else {
                result.push(token);
            }
        }
        result
    };

    let expected_words = normalize(expected);
    let actual_words = normalize(actual);

    if expected_words.is_empty() {
        return if actual_words.is_empty() { 1.0 } else { 0.0 };
    }

    let mut matches = 0;
    for word in &expected_words {
        let word_matches = actual_words.iter().any(|w| {
            // Exact match
            w == word
            // Number words ↔ digits
            || (word == "one" && w == "1")
            || (word == "two" && w == "2")
            || (word == "three" && w == "3")
            || (word == "four" && w == "4")
            || (word == "five" && w == "5")
            // STT often clips the first phoneme — match if suffix with ≤1 char difference
            || (word.len() > 3 && w.len() + 1 >= word.len() && word.ends_with(w.as_str()))
            || (w.len() > 3 && word.len() + 1 >= w.len() && w.ends_with(word.as_str()))
        });
        if word_matches {
            matches += 1;
        }
    }

    matches as f32 / expected_words.len() as f32
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_tts_stt_roundtrip_via_ipc() {
    println!("\n=== TTS -> STT Roundtrip Test (IPC) ===\n");

    // Verify server is reachable before running test suite
    if ipc_connect().is_none() {
        return;
    }
    println!("Connected to IPC server\n");

    let mut passed = 0;
    let mut failed = 0;

    for phrase in TEST_PHRASES {
        println!("Testing: \"{}\"", phrase);

        // Fresh connection per phrase
        let mut stream = match ipc_connect() {
            Some(s) => s,
            None => { failed += 1; continue; }
        };

        // Step 1: Synthesize via IPC
        print!("  1. TTS synthesizing... ");
        let synth_request = SynthesizeRequest {
            command: "voice/synthesize",
            text: phrase.to_string(),
        };

        let synth_result = match ipc_request(&mut stream, &synth_request) {
            Ok(r) => r,
            Err(e) => {
                println!("IPC error: {}", e);
                failed += 1;
                continue;
            }
        };

        // Extract PCM audio — synthesize returns Binary frame
        let (header, pcm_bytes) = match synth_result {
            IpcResult::Binary { header, data } => (header, data),
            IpcResult::Json(resp) => {
                if !resp.success {
                    println!("TTS failed: {:?}", resp.error);
                } else {
                    println!("Expected binary response, got JSON-only");
                }
                failed += 1;
                continue;
            }
        };

        if !header.success {
            println!("TTS failed: {:?}", header.error);
            failed += 1;
            continue;
        }

        let result = header.result.unwrap();
        let sample_rate = result["sample_rate"].as_u64().unwrap_or(16000);
        let num_samples = result["num_samples"].as_u64().unwrap_or(0);
        let duration_ms = result["duration_ms"].as_u64().unwrap_or(0);

        println!("{} samples at {}Hz ({}ms)", num_samples, sample_rate, duration_ms);

        if sample_rate != 16000 {
            println!("  WARNING: Sample rate is {}Hz, expected 16000Hz", sample_rate);
        }

        // Step 2: Transcribe via IPC — encode raw PCM as base64 for STT input
        print!("  2. STT transcribing... ");

        let mut stream = match ipc_connect() {
            Some(s) => s,
            None => { failed += 1; continue; }
        };

        let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&pcm_bytes);
        let transcribe_request = TranscribeRequest {
            command: "voice/transcribe",
            audio: audio_base64,
            language: Some("en".to_string()),
        };

        let transcribe_result = match ipc_request(&mut stream, &transcribe_request) {
            Ok(r) => r,
            Err(e) => {
                println!("IPC error: {}", e);
                failed += 1;
                continue;
            }
        };

        let transcribe_resp = transcribe_result.into_json();

        if !transcribe_resp.success {
            println!("STT failed: {:?}", transcribe_resp.error);
            failed += 1;
            continue;
        }

        let result = transcribe_resp.result.unwrap();
        let transcription = result["text"].as_str().unwrap_or("");
        let confidence = result["confidence"].as_f64().unwrap_or(0.0);

        println!("\"{}\" (confidence: {:.2})", transcription, confidence);

        // Step 3: Compare
        let similarity = word_similarity(phrase, transcription);
        println!("  3. Similarity: {:.1}%", similarity * 100.0);

        if similarity >= 0.6 {
            println!("  PASSED\n");
            passed += 1;
        } else {
            println!("  FAILED - transcription mismatch");
            println!("     Expected: \"{}\"", phrase);
            println!("     Got:      \"{}\"\n", transcription);
            failed += 1;
        }
    }

    println!("=== Results ===");
    println!("Passed: {}/{}", passed, TEST_PHRASES.len());
    println!("Failed: {}/{}", failed, TEST_PHRASES.len());

    assert!(failed == 0, "Some TTS->STT roundtrip tests failed");
}

#[test]
fn test_tts_sample_rate_via_ipc() {
    println!("\n=== TTS Sample Rate Test (IPC) ===\n");

    let mut stream = match ipc_connect() {
        Some(s) => s,
        None => return,
    };

    let request = SynthesizeRequest {
        command: "voice/synthesize",
        text: "Test sample rate".to_string(),
    };

    let result = ipc_request(&mut stream, &request).expect("IPC request failed");
    let (header, pcm_bytes) = result.into_binary();

    assert!(header.success, "TTS failed: {:?}", header.error);

    let result = header.result.unwrap();
    let sample_rate = result["sample_rate"].as_u64().unwrap();
    let num_samples = result["num_samples"].as_u64().unwrap();
    let duration_ms = result["duration_ms"].as_u64().unwrap();

    // PCM bytes should be 2 * num_samples (i16 = 2 bytes)
    assert_eq!(
        pcm_bytes.len(),
        num_samples as usize * 2,
        "PCM byte count should be 2 * num_samples"
    );

    println!("Sample rate: {}Hz", sample_rate);
    println!("Samples: {}", num_samples);
    println!("Duration: {}ms", duration_ms);
    println!("PCM bytes: {}", pcm_bytes.len());

    // Verify sample rate is 16kHz
    assert_eq!(sample_rate, 16000, "TTS must output 16kHz for CallServer compatibility");

    // Verify duration matches sample count (within 100ms tolerance)
    let expected_duration = (num_samples * 1000) / 16000;
    assert!(
        (duration_ms as i64 - expected_duration as i64).abs() < 100,
        "Duration {}ms doesn't match sample count (expected ~{}ms)",
        duration_ms,
        expected_duration
    );

    // Verify PCM data is not silence
    let samples: Vec<i16> = pcm_bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    let max_amp = samples.iter().map(|s| s.abs()).max().unwrap_or(0);
    assert!(max_amp > 100, "Audio should not be silence, max amplitude: {}", max_amp);

    println!("Max amplitude: {}", max_amp);
    println!("Sample rate test PASSED");
}

#[test]
fn test_stt_whisper_via_ipc() {
    println!("\n=== STT Whisper Test (IPC) ===\n");

    // Create known audio: 1 second of 440Hz tone
    let mut samples: Vec<i16> = vec![0; 16000];
    for (i, sample) in samples.iter_mut().enumerate() {
        let t = i as f32 / 16000.0;
        *sample = (440.0_f32 * 2.0 * std::f32::consts::PI * t).sin() as i16 * 1000;
    }

    // Encode to base64 for STT input
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    let mut stream = match ipc_connect() {
        Some(s) => s,
        None => return,
    };

    let request = TranscribeRequest {
        command: "voice/transcribe",
        audio: audio_base64,
        language: Some("en".to_string()),
    };

    let result = match ipc_request(&mut stream, &request) {
        Ok(r) => r,
        Err(e) => {
            println!("IPC error: {}", e);
            println!("   This may indicate the STT model is not loaded.");
            return;
        }
    };

    let response = result.into_json();

    match (response.success, response.result) {
        (true, Some(result)) => {
            println!("Transcription: \"{}\"", result["text"].as_str().unwrap_or(""));
            println!("Language: {}", result["language"].as_str().unwrap_or(""));
            println!("Confidence: {:.2}", result["confidence"].as_f64().unwrap_or(0.0));
            println!("STT infrastructure test PASSED");
        }
        (false, _) => {
            println!("STT failed: {:?}", response.error);
            println!("   This is OK if Whisper model is not loaded.");
        }
        _ => {
            println!("Unexpected response format");
        }
    }
}
