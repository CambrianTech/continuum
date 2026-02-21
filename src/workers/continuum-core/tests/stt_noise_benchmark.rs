//! STT Noise Robustness Benchmark
//!
//! Tests each available STT adapter against synthetic TTS speech mixed with
//! various background noise types at different SNR levels. Produces a matrix
//! showing word accuracy for each (adapter × noise × SNR) combination.
//!
//! Flow:
//! 1. Synthesize a known phrase via TTS (IPC)
//! 2. Generate each noise type locally via TestAudioGenerator
//! 3. Mix speech + noise at various SNR levels
//! 4. Transcribe mixed audio with each STT adapter (voice/transcribe-with-adapter)
//! 5. Measure word overlap and print results matrix
//!
//! REQUIREMENTS: Server must be running (npm start)
//! Run with: cargo test -p continuum-core --release --test stt_noise_benchmark -- --nocapture

mod common;

use base64::Engine;
use common::{ipc_connect, ipc_request, IpcResult};
use continuum_core::voice::vad::{NoiseType, TestAudioGenerator};
use serde::Serialize;

const TEST_PHRASE: &str = "The quick brown fox jumps over the lazy dog";

const SNR_LEVELS: &[f32] = &[20.0, 10.0, 5.0, 0.0, -5.0];

// ============================================================================
// Request Types
// ============================================================================

#[derive(Serialize)]
struct SynthesizeRequest {
    command: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    adapter: Option<String>,
}

#[derive(Serialize)]
struct TranscribeWithAdapterRequest {
    command: &'static str,
    audio: String,
    language: Option<String>,
    adapter: String,
}

#[derive(Serialize)]
struct SttListRequest {
    command: &'static str,
}

// ============================================================================
// Helpers
// ============================================================================

fn word_overlap(expected: &str, actual: &str) -> f32 {
    let normalize = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { ' ' })
            .collect::<String>()
            .split_whitespace()
            .map(|w| w.to_string())
            .collect()
    };

    let expected_words = normalize(expected);
    let actual_words: std::collections::HashSet<String> =
        normalize(actual).into_iter().collect();

    if expected_words.is_empty() {
        return if actual_words.is_empty() { 1.0 } else { 0.0 };
    }

    let matches = expected_words.iter().filter(|w| actual_words.contains(*w)).count();
    matches as f32 / expected_words.len() as f32
}

fn samples_to_base64(samples: &[i16]) -> String {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(&bytes)
}

/// Pad or truncate noise to match signal length
fn match_length(noise: &[i16], target_len: usize) -> Vec<i16> {
    if noise.len() >= target_len {
        noise[..target_len].to_vec()
    } else {
        // Loop noise to fill target length
        let mut result = Vec::with_capacity(target_len);
        while result.len() < target_len {
            let remaining = target_len - result.len();
            let take = remaining.min(noise.len());
            result.extend_from_slice(&noise[..take]);
        }
        result
    }
}

// ============================================================================
// Benchmark
// ============================================================================

struct BenchmarkResult {
    noise_label: String,
    snr_db: f32,
    adapter: String,
    word_accuracy: f32,
    transcription: String,
}

#[test]
fn benchmark_stt_under_noise() {
    println!("\n{}", "=".repeat(70));
    println!("=== STT Noise Robustness Benchmark ===");
    println!("{}\n", "=".repeat(70));

    // Step 1: Connect and list available STT adapters
    let mut stream = match ipc_connect() {
        Some(s) => s,
        None => return,
    };

    let list_req = SttListRequest { command: "voice/stt-list" };
    let list_result = match ipc_request(&mut stream, &list_req) {
        Ok(r) => r,
        Err(e) => {
            println!("Failed to list STT adapters: {e}");
            return;
        }
    };

    let list_resp = list_result.into_json();
    if !list_resp.success {
        println!("voice/stt-list failed: {:?}", list_resp.error);
        return;
    }

    let result = list_resp.result.unwrap();
    let adapters: Vec<String> = result["adapters"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|a| a["name"].as_str().unwrap_or("").to_string())
        .filter(|name| !name.is_empty() && name != "stub")
        .collect();

    println!("Available STT adapters: {:?}", adapters);
    println!("Active: {}\n", result["active"].as_str().unwrap_or("none"));

    if adapters.is_empty() {
        println!("No STT adapters available (excluding stub). Skipping benchmark.");
        return;
    }

    // Step 2: Synthesize test phrase via TTS
    println!("Synthesizing: \"{TEST_PHRASE}\"");
    let mut stream = match ipc_connect() {
        Some(s) => s,
        None => return,
    };

    let synth_req = SynthesizeRequest {
        command: "voice/synthesize",
        text: TEST_PHRASE.to_string(),
        adapter: Some("kokoro".to_string()),
    };

    let synth_result = match ipc_request(&mut stream, &synth_req) {
        Ok(r) => r,
        Err(e) => {
            println!("TTS synthesis failed: {e}");
            return;
        }
    };

    let (header, pcm_bytes) = match synth_result {
        IpcResult::Binary { header, data } => (header, data),
        IpcResult::Json(resp) => {
            println!("TTS returned JSON instead of binary: {:?}", resp.error);
            return;
        }
    };

    if !header.success {
        println!("TTS failed: {:?}", header.error);
        return;
    }

    let speech_samples: Vec<i16> = pcm_bytes
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();

    let meta = header.result.unwrap();
    println!(
        "  TTS: {} samples, {}ms, {}Hz\n",
        meta["num_samples"], meta["duration_ms"], meta["sample_rate"]
    );

    // Step 3: Build noise types
    let noise_types: Vec<(&str, NoiseType)> = vec![
        ("crowd-5", NoiseType::Crowd(5)),
        ("factory", NoiseType::FactoryFloor),
        ("gunfire", NoiseType::Gunfire(3.0)),
        ("explosion", NoiseType::Explosion),
        ("siren", NoiseType::Siren),
        ("music", NoiseType::Music),
        ("wind", NoiseType::Wind),
        ("rain", NoiseType::Rain),
    ];

    let gen = TestAudioGenerator::default();
    let speech_len = speech_samples.len();

    // Step 4: Generate all noise samples
    let noise_samples: Vec<(&str, Vec<i16>)> = noise_types
        .iter()
        .map(|(label, nt)| {
            let raw = gen.generate_noise(nt, speech_len);
            let matched = match_length(&raw, speech_len);
            (*label, matched)
        })
        .collect();

    // Step 5: Run benchmark — for each adapter × noise × SNR
    let mut results: Vec<BenchmarkResult> = Vec::new();

    // First: clean speech (no noise) as baseline
    for adapter in &adapters {
        println!("Testing adapter '{}' with clean speech...", adapter);
        let audio_b64 = samples_to_base64(&speech_samples);

        let mut stream = match ipc_connect() {
            Some(s) => s,
            None => continue,
        };

        let req = TranscribeWithAdapterRequest {
            command: "voice/transcribe-with-adapter",
            audio: audio_b64,
            language: Some("en".to_string()),
            adapter: adapter.clone(),
        };

        match ipc_request(&mut stream, &req) {
            Ok(r) => {
                let resp = r.into_json();
                if resp.success {
                    let result = resp.result.unwrap();
                    let text = result["text"].as_str().unwrap_or("").to_string();
                    let accuracy = word_overlap(TEST_PHRASE, &text);
                    println!("  clean/∞dB: {:.0}% — \"{}\"", accuracy * 100.0, text);
                    results.push(BenchmarkResult {
                        noise_label: "clean".to_string(),
                        snr_db: f32::INFINITY,
                        adapter: adapter.clone(),
                        word_accuracy: accuracy,
                        transcription: text,
                    });
                } else {
                    println!("  {} failed: {:?}", adapter, resp.error);
                }
            }
            Err(e) => println!("  IPC error: {e}"),
        }
    }

    // Then: noisy conditions
    for (noise_label, noise) in &noise_samples {
        for &snr_db in SNR_LEVELS {
            let mixed = TestAudioGenerator::mix_audio_with_snr(&speech_samples, noise, snr_db);
            let audio_b64 = samples_to_base64(&mixed);

            for adapter in &adapters {
                let mut stream = match ipc_connect() {
                    Some(s) => s,
                    None => continue,
                };

                let req = TranscribeWithAdapterRequest {
                    command: "voice/transcribe-with-adapter",
                    audio: audio_b64.clone(),
                    language: Some("en".to_string()),
                    adapter: adapter.clone(),
                };

                match ipc_request(&mut stream, &req) {
                    Ok(r) => {
                        let resp = r.into_json();
                        if resp.success {
                            let result = resp.result.unwrap();
                            let text = result["text"].as_str().unwrap_or("").to_string();
                            let accuracy = word_overlap(TEST_PHRASE, &text);
                            results.push(BenchmarkResult {
                                noise_label: noise_label.to_string(),
                                snr_db,
                                adapter: adapter.clone(),
                                word_accuracy: accuracy,
                                transcription: text,
                            });
                        } else {
                            println!(
                                "  {}/{}/{}dB failed: {:?}",
                                adapter, noise_label, snr_db, resp.error
                            );
                        }
                    }
                    Err(e) => {
                        println!("  IPC error for {}/{}/{}dB: {}", adapter, noise_label, snr_db, e);
                    }
                }
            }
        }
        println!("  Completed noise type: {}", noise_label);
    }

    // Step 6: Print results matrix
    println!("\n{}", "=".repeat(90));
    println!("RESULTS MATRIX — Word Accuracy (%)");
    println!("{}", "=".repeat(90));

    // Header row
    print!("{:<12} {:>6}", "Noise", "SNR");
    for adapter in &adapters {
        print!(" | {:>12}", adapter);
    }
    println!();
    println!("{}", "-".repeat(90));

    // Clean row
    print!("{:<12} {:>6}", "clean", "∞");
    for adapter in &adapters {
        let r = results.iter().find(|r| r.noise_label == "clean" && r.adapter == *adapter);
        match r {
            Some(r) => print!(" | {:>11.0}%", r.word_accuracy * 100.0),
            None => print!(" |         N/A"),
        }
    }
    println!();

    // Noisy rows
    for (noise_label, _) in &noise_samples {
        for &snr_db in SNR_LEVELS {
            print!("{:<12} {:>5.0}dB", noise_label, snr_db);
            for adapter in &adapters {
                let r = results.iter().find(|r| {
                    r.noise_label == *noise_label
                        && r.adapter == *adapter
                        && (r.snr_db - snr_db).abs() < 0.1
                });
                match r {
                    Some(r) => print!(" | {:>11.0}%", r.word_accuracy * 100.0),
                    None => print!(" |         N/A"),
                }
            }
            println!();
        }
    }

    println!("{}", "=".repeat(90));
    println!(
        "\nTotal benchmark points: {} ({} adapters × {} noise types × {} SNR levels + {} clean baselines)",
        results.len(),
        adapters.len(),
        noise_samples.len(),
        SNR_LEVELS.len(),
        adapters.len()
    );

    // Print any failed transcriptions at very high SNR (20dB) — these indicate adapter issues
    let high_snr_failures: Vec<&BenchmarkResult> = results
        .iter()
        .filter(|r| r.snr_db >= 20.0 && r.word_accuracy < 0.5)
        .collect();

    if !high_snr_failures.is_empty() {
        println!("\nWARNING: Low accuracy at high SNR (possible adapter issues):");
        for r in &high_snr_failures {
            println!(
                "  {} / {} / {}dB: {:.0}% — \"{}\"",
                r.adapter,
                r.noise_label,
                r.snr_db,
                r.word_accuracy * 100.0,
                r.transcription
            );
        }
    }
}
