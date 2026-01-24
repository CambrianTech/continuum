//! Real Speech Validation Tests
//!
//! Validates ProductionVAD with actual human speech samples.
//! Falls back to synthetic speech if real samples unavailable.
//!
//! ## Setup
//!
//! Download real speech samples first:
//! ```bash
//! cd streaming-core
//! ./scripts/download_speech_samples_simple.sh
//! ```
//!
//! ## Run Tests
//!
//! ```bash
//! cargo test --test vad_real_speech_validation -- --ignored
//! ```

use streaming_core::vad::{ProductionVAD, ProductionVADConfig, TestAudioGenerator, Vowel};
use streaming_core::vad::wav_loader;
use std::path::Path;

/// Check if real speech samples are available
fn has_real_speech_samples() -> bool {
    let audio_dir = Path::new("test_audio/real_speech");
    if !audio_dir.exists() {
        return false;
    }

    // Check for at least one WAV file
    std::fs::read_dir(audio_dir)
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .find(|e| e.path().extension().and_then(|s| s.to_str()) == Some("wav"))
        })
        .is_some()
}

/// Load all available WAV files from test_audio/real_speech/
fn load_real_speech_samples() -> Vec<(String, Vec<i16>)> {
    let audio_dir = Path::new("test_audio/real_speech");
    let mut samples = Vec::new();

    if !audio_dir.exists() {
        return samples;
    }

    if let Ok(entries) = std::fs::read_dir(audio_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("wav") {
                match wav_loader::load_wav_file(&path) {
                    Ok(audio) => {
                        let filename = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        samples.push((filename, audio));
                    }
                    Err(e) => {
                        println!("⚠️  Failed to load {}: {:?}", path.display(), e);
                    }
                }
            }
        }
    }

    samples
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_production_vad_real_speech_detection() {
    println!("\n📊 ProductionVAD: Real Speech Validation\n");

    let mut vad = ProductionVAD::new();
    vad.initialize().await.expect("ProductionVAD init failed");

    // Try to load real speech samples
    let real_samples = load_real_speech_samples();

    if real_samples.is_empty() {
        println!("⚠️  No real speech samples found");
        println!("   Run: ./scripts/download_speech_samples_simple.sh");
        println!("   Falling back to synthetic speech...\n");

        // Use synthetic speech as fallback
        let gen = TestAudioGenerator::new(16000);
        let test_samples = vec![
            ("synthetic_hello", gen.generate_formant_speech(8000, Vowel::A)),
            ("synthetic_world", gen.generate_formant_speech(8000, Vowel::O)),
        ];

        for (name, samples) in &test_samples {
            println!("Testing synthetic: {}", name);
            test_speech_sample(&mut vad, samples, name).await;
        }
    } else {
        println!("✅ Found {} real speech samples\n", real_samples.len());

        for (filename, samples) in &real_samples {
            println!("Testing real speech: {}", filename);
            test_speech_sample(&mut vad, samples, filename).await;
        }
    }
}

/// Test a single speech sample
async fn test_speech_sample(vad: &mut ProductionVAD, samples: &[i16], name: &str) {
    let duration_ms = (samples.len() as f32 / 16000.0) * 1000.0;
    println!("  Duration: {:.0}ms ({} samples)", duration_ms, samples.len());

    // Process in 512-sample chunks (32ms @ 16kHz)
    let frame_size = 512;
    let mut frame_count = 0;
    let mut speech_detected = false;

    for chunk in samples.chunks(frame_size) {
        if chunk.len() < frame_size {
            // Pad last chunk to frame_size
            let mut padded = chunk.to_vec();
            padded.resize(frame_size, 0);

            if let Ok(Some(_complete)) = vad.process_frame(&padded).await {
                speech_detected = true;
                break;
            }
        } else {
            if let Ok(Some(_complete)) = vad.process_frame(chunk).await {
                speech_detected = true;
                break;
            }
        }

        frame_count += 1;
    }

    // Add silence to trigger final transcription
    if !speech_detected {
        for _ in 0..40 {
            if let Ok(Some(complete)) = vad.process_frame(&vec![0; frame_size]).await {
                let detected_duration = (complete.len() as f32 / 16000.0) * 1000.0;
                println!(
                    "  ✓ Speech detected! Buffered {:.0}ms for transcription",
                    detected_duration
                );
                speech_detected = true;
                break;
            }
        }
    }

    if !speech_detected {
        println!("  ⚠️  No speech detected (possible issue or very quiet sample)");
    }

    println!();
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_production_vad_noise_vs_speech_discrimination() {
    println!("\n📊 ProductionVAD: Noise vs Speech Discrimination\n");

    let mut vad = ProductionVAD::new();
    vad.initialize().await.expect("ProductionVAD init failed");

    // Test 1: Pure silence (should NOT detect speech)
    println!("Test 1: Pure Silence");
    let silence = vec![0i16; 8000]; // 0.5s of silence
    let mut false_positive = false;

    for chunk in silence.chunks(512) {
        if let Ok(Some(_)) = vad.process_frame(chunk).await {
            false_positive = true;
            break;
        }
    }

    // Add silence frames to check final result
    for _ in 0..40 {
        if let Ok(Some(_)) = vad.process_frame(&vec![0; 512]).await {
            false_positive = true;
            break;
        }
    }

    if false_positive {
        println!("  ❌ FALSE POSITIVE: Detected speech in pure silence");
    } else {
        println!("  ✅ Correctly ignored silence");
    }

    // Test 2: White noise (should NOT detect speech)
    println!("\nTest 2: White Noise");
    let noise: Vec<i16> = (0..8000)
        .map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16)
        .collect();

    false_positive = false;
    for chunk in noise.chunks(512) {
        if let Ok(Some(_)) = vad.process_frame(chunk).await {
            false_positive = true;
            break;
        }
    }

    for _ in 0..40 {
        if let Ok(Some(_)) = vad.process_frame(&vec![0; 512]).await {
            false_positive = true;
            break;
        }
    }

    if false_positive {
        println!("  ⚠️  Detected speech in noise (may happen with voice-like noise)");
    } else {
        println!("  ✅ Correctly ignored noise");
    }

    // Test 3: Real/synthetic speech (SHOULD detect speech)
    println!("\nTest 3: Speech Detection");

    let real_samples = load_real_speech_samples();
    let test_audio = if real_samples.is_empty() {
        println!("  Using synthetic speech (no real samples available)");
        let gen = TestAudioGenerator::new(16000);
        gen.generate_formant_speech(8000, Vowel::A)
    } else {
        println!("  Using real speech sample");
        real_samples[0].1.clone()
    };

    let mut speech_detected = false;
    for chunk in test_audio.chunks(512) {
        if chunk.len() < 512 {
            let mut padded = chunk.to_vec();
            padded.resize(512, 0);
            if let Ok(Some(_)) = vad.process_frame(&padded).await {
                speech_detected = true;
                break;
            }
        } else {
            if let Ok(Some(_)) = vad.process_frame(chunk).await {
                speech_detected = true;
                break;
            }
        }
    }

    for _ in 0..40 {
        if let Ok(Some(_)) = vad.process_frame(&vec![0; 512]).await {
            speech_detected = true;
            break;
        }
    }

    if speech_detected {
        println!("  ✅ Correctly detected speech");
    } else {
        println!("  ❌ FAILED to detect speech");
    }

    println!();
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_production_vad_sentence_completeness() {
    println!("\n📊 ProductionVAD: Sentence Completeness\n");

    let config = ProductionVADConfig {
        silero_threshold: 0.3,
        silence_threshold_frames: 40, // 1.28s
        ..Default::default()
    };

    let mut vad = ProductionVAD::with_config(config);
    vad.initialize().await.expect("ProductionVAD init failed");

    // Load real speech or use synthetic
    let real_samples = load_real_speech_samples();

    if real_samples.is_empty() {
        println!("⚠️  No real speech samples (using synthetic)");
        println!("   Run: ./scripts/download_speech_samples_simple.sh\n");

        // Simulate sentence with pauses: "Hello" (pause) "how are you"
        let gen = TestAudioGenerator::new(16000);
        let frames = vec![
            gen.generate_formant_speech(512, Vowel::A),   // "Hello"
            vec![0; 512],                                  // pause
            gen.generate_formant_speech(512, Vowel::O),   // "how"
            vec![0; 512],                                  // pause
            gen.generate_formant_speech(512, Vowel::A),   // "are"
            gen.generate_formant_speech(512, Vowel::U),   // "you"
        ];

        let mut fragment_count = 0;
        for (i, frame) in frames.iter().enumerate() {
            if let Ok(Some(complete)) = vad.process_frame(frame).await {
                fragment_count += 1;
                println!(
                    "  Fragment #{} after frame {} ({} samples)",
                    fragment_count,
                    i,
                    complete.len()
                );
            }
        }

        // Trigger final transcription
        for i in 0..40 {
            if let Ok(Some(complete)) = vad.process_frame(&vec![0; 512]).await {
                let duration = (complete.len() as f32 / 16000.0) * 1000.0;
                println!("  ✓ Complete sentence after {} silence frames", i + 1);
                println!("    Duration: {:.0}ms", duration);
                fragment_count += 1;
                break;
            }
        }

        if fragment_count <= 2 {
            println!("\n  ✅ Good sentence buffering ({} fragment(s))", fragment_count);
        } else {
            println!("\n  ⚠️  Too many fragments ({})", fragment_count);
        }
    } else {
        println!("✅ Using real speech sample\n");

        // Process entire sample
        let samples = &real_samples[0].1;
        let mut transcriptions = 0;

        for chunk in samples.chunks(512) {
            if chunk.len() < 512 {
                let mut padded = chunk.to_vec();
                padded.resize(512, 0);
                if let Ok(Some(complete)) = vad.process_frame(&padded).await {
                    transcriptions += 1;
                    let duration = (complete.len() as f32 / 16000.0) * 1000.0;
                    println!("  Transcription #{}: {:.0}ms", transcriptions, duration);
                }
            } else {
                if let Ok(Some(complete)) = vad.process_frame(chunk).await {
                    transcriptions += 1;
                    let duration = (complete.len() as f32 / 16000.0) * 1000.0;
                    println!("  Transcription #{}: {:.0}ms", transcriptions, duration);
                }
            }
        }

        // Final silence
        for _ in 0..40 {
            if let Ok(Some(complete)) = vad.process_frame(&vec![0; 512]).await {
                transcriptions += 1;
                let duration = (complete.len() as f32 / 16000.0) * 1000.0;
                println!("  Final transcription: {:.0}ms", duration);
                break;
            }
        }

        println!("\n  Total transcriptions: {}", transcriptions);
        println!("  (Fewer is better - means less fragmentation)");
    }

    println!();
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_production_vad_configuration_impact() {
    println!("\n📊 ProductionVAD: Configuration Impact Analysis\n");

    // Test different configurations
    let configs = vec![
        (
            "Conservative (default)",
            ProductionVADConfig {
                silero_threshold: 0.5,
                silence_threshold_frames: 22,
                ..Default::default()
            },
        ),
        (
            "Balanced (recommended)",
            ProductionVADConfig {
                silero_threshold: 0.3,
                silence_threshold_frames: 40,
                ..Default::default()
            },
        ),
        (
            "Aggressive (high recall)",
            ProductionVADConfig {
                silero_threshold: 0.2,
                silence_threshold_frames: 50,
                ..Default::default()
            },
        ),
    ];

    // Load test audio
    let real_samples = load_real_speech_samples();
    let test_audio = if real_samples.is_empty() {
        let gen = TestAudioGenerator::new(16000);
        gen.generate_formant_speech(8000, Vowel::A)
    } else {
        real_samples[0].1.clone()
    };

    for (name, config) in configs {
        println!("Configuration: {}", name);
        println!(
            "  Threshold: {:.2}, Silence: {} frames ({:.2}s)",
            config.silero_threshold,
            config.silence_threshold_frames,
            config.silence_threshold_frames as f32 * 0.032
        );

        let mut vad = ProductionVAD::with_config(config);
        vad.initialize().await.expect("VAD init failed");

        // Process audio
        let mut detected = false;
        for chunk in test_audio.chunks(512) {
            if chunk.len() < 512 {
                let mut padded = chunk.to_vec();
                padded.resize(512, 0);
                if let Ok(Some(_)) = vad.process_frame(&padded).await {
                    detected = true;
                    break;
                }
            } else {
                if let Ok(Some(_)) = vad.process_frame(chunk).await {
                    detected = true;
                    break;
                }
            }
        }

        // Trigger with silence
        if !detected {
            for _ in 0..50 {
                if let Ok(Some(_)) = vad.process_frame(&vec![0; 512]).await {
                    detected = true;
                    break;
                }
            }
        }

        println!("  Result: {}", if detected { "✅ Detected" } else { "❌ Missed" });
        println!();
    }
}
