//! VAD Testing with Real(istic) Background Noise Samples
//!
//! Tests all VAD implementations against 10 different realistic background noises:
//! 1. White Noise (TV static)
//! 2. Pink Noise (rain, natural ambiance)
//! 3. Brown Noise (traffic rumble, ocean)
//! 4. HVAC / Air Conditioning
//! 5. Computer Fan
//! 6. Fluorescent Light Buzz
//! 7. Office Ambiance
//! 8. Crowd Murmur
//! 9. Traffic / Road Noise
//! 10. Restaurant / Cafe
//!
//! Each noise is mixed with synthetic speech at various SNR levels.

use streaming_core::vad::{
    GroundTruth, RmsThresholdVAD, SileroRawVAD, TestAudioGenerator, VADEvaluator,
    VoiceActivityDetection, Vowel, WebRtcVAD, wav_loader,
};

/// Background noise descriptors
struct BackgroundNoise {
    filename: &'static str,
    description: &'static str,
}

const BACKGROUND_NOISES: [BackgroundNoise; 10] = [
    BackgroundNoise {
        filename: "01_white_noise",
        description: "White Noise (TV static)",
    },
    BackgroundNoise {
        filename: "02_pink_noise",
        description: "Pink Noise (rain/nature)",
    },
    BackgroundNoise {
        filename: "03_brown_noise",
        description: "Brown Noise (traffic/ocean)",
    },
    BackgroundNoise {
        filename: "04_hvac_hum",
        description: "HVAC / Air Conditioning",
    },
    BackgroundNoise {
        filename: "05_fan_noise",
        description: "Computer Fan",
    },
    BackgroundNoise {
        filename: "06_fluorescent_buzz",
        description: "Fluorescent Light Buzz",
    },
    BackgroundNoise {
        filename: "07_office_ambiance",
        description: "Office Ambiance",
    },
    BackgroundNoise {
        filename: "08_crowd_murmur",
        description: "Crowd Murmur",
    },
    BackgroundNoise {
        filename: "09_traffic_road",
        description: "Traffic / Road Noise",
    },
    BackgroundNoise {
        filename: "10_restaurant_cafe",
        description: "Restaurant / Cafe",
    },
];

/// Create test dataset: speech + 10 realistic background noises at various SNR
fn create_realistic_noise_dataset() -> Vec<(Vec<i16>, GroundTruth, String, f32)> {
    let gen = TestAudioGenerator::new(16000);

    // Generate base speech samples
    let speech_a = gen.generate_formant_speech(240, Vowel::A);
    let speech_i = gen.generate_formant_speech(240, Vowel::I);
    let plosive = gen.generate_plosive(240);

    let mut dataset = vec![];

    // SNR levels: +10dB (clean), +5dB (moderate), 0dB (equal), -5dB (very noisy)
    let snr_levels = vec![10.0, 5.0, 0.0, -5.0];

    // Test each background noise
    for bg_noise in &BACKGROUND_NOISES {
        match wav_loader::load_background_noise(bg_noise.filename) {
            Ok(noise_full) => {
                // Get 240 samples from the noise
                let noise = wav_loader::get_chunk(&noise_full, 1000, 240);

                // Test at each SNR level
                for &snr in &snr_levels {
                    // Mix speech with this background noise
                    dataset.push((
                        TestAudioGenerator::mix_audio_with_snr(&speech_a, &noise, snr),
                        GroundTruth::Speech,
                        format!("{} + Speech/A", bg_noise.description),
                        snr,
                    ));

                    dataset.push((
                        TestAudioGenerator::mix_audio_with_snr(&speech_i, &noise, snr),
                        GroundTruth::Speech,
                        format!("{} + Speech/I", bg_noise.description),
                        snr,
                    ));

                    dataset.push((
                        TestAudioGenerator::mix_audio_with_snr(&plosive, &noise, snr),
                        GroundTruth::Speech,
                        format!("{} + Plosive", bg_noise.description),
                        snr,
                    ));
                }

                // Test pure noise (should be rejected as silence)
                dataset.push((
                    noise.clone(),
                    GroundTruth::Silence,
                    format!("Pure {}", bg_noise.description),
                    f32::NAN,
                ));
            }
            Err(e) => {
                eprintln!("⚠️  Failed to load {}: {}", bg_noise.filename, e);
                eprintln!("   Run: ./scripts/generate_10_noises.sh");
            }
        }
    }

    dataset
}

#[tokio::test]
async fn test_rms_realistic_bg_noise() {
    let vad = RmsThresholdVAD::new();
    vad.initialize().await.expect("RMS init failed");

    let dataset = create_realistic_noise_dataset();
    if dataset.is_empty() {
        println!("⚠️  No background noise files loaded");
        println!("   Run: ./scripts/generate_10_noises.sh");
        return;
    }

    let mut evaluator = VADEvaluator::new();

    println!("\n📊 RMS VAD with 10 Realistic Background Noises:\n");
    println!("  SNR   Sample                                      → Result  Truth    Correct");
    println!("  ──────────────────────────────────────────────────────────────────────────────");

    for (audio, ground_truth, label, snr) in dataset {
        let result = vad.detect(&audio).await.expect("Detection failed");
        evaluator.record(result.is_speech, ground_truth, result.confidence);

        let correct = match (result.is_speech, ground_truth) {
            (true, GroundTruth::Speech) | (false, GroundTruth::Silence) => "✓",
            _ => "✗",
        };

        let snr_str = if snr.is_nan() {
            "  N/A".to_string()
        } else {
            format!("{:+5.0}dB", snr)
        };

        println!(
            "  {} {:45} → {:5}  {:?}  {}",
            snr_str,
            label.chars().take(45).collect::<String>(),
            result.is_speech,
            ground_truth,
            correct
        );
    }

    println!("\n{}", evaluator.report());

    let matrix = evaluator.matrix();
    println!("\n🔑 Key Metrics:");
    println!("   Specificity (noise rejection): {:.1}%", matrix.specificity() * 100.0);
    println!("   False Positive Rate: {:.1}%", matrix.false_positive_rate() * 100.0);
}

#[tokio::test]
async fn test_webrtc_realistic_bg_noise() {
    let vad = WebRtcVAD::new();
    vad.initialize().await.expect("WebRTC init failed");

    let dataset = create_realistic_noise_dataset();
    if dataset.is_empty() {
        println!("⚠️  No background noise files loaded");
        println!("   Run: ./scripts/generate_10_noises.sh");
        return;
    }

    let mut evaluator = VADEvaluator::new();

    println!("\n📊 WebRTC VAD with 10 Realistic Background Noises:\n");
    println!("  SNR   Sample                                      → Result  Truth    Correct");
    println!("  ──────────────────────────────────────────────────────────────────────────────");

    for (audio, ground_truth, label, snr) in dataset {
        let result = vad.detect(&audio).await.expect("Detection failed");
        evaluator.record(result.is_speech, ground_truth, result.confidence);

        let correct = match (result.is_speech, ground_truth) {
            (true, GroundTruth::Speech) | (false, GroundTruth::Silence) => "✓",
            _ => "✗",
        };

        let snr_str = if snr.is_nan() {
            "  N/A".to_string()
        } else {
            format!("{:+5.0}dB", snr)
        };

        println!(
            "  {} {:45} → {:5}  {:?}  {}",
            snr_str,
            label.chars().take(45).collect::<String>(),
            result.is_speech,
            ground_truth,
            correct
        );
    }

    println!("\n{}", evaluator.report());

    let matrix = evaluator.matrix();
    println!("\n🔑 Key Metrics:");
    println!("   Specificity (noise rejection): {:.1}%", matrix.specificity() * 100.0);
    println!("   False Positive Rate: {:.1}%", matrix.false_positive_rate() * 100.0);
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_silero_realistic_bg_noise() {
    let vad = SileroRawVAD::new();
    vad.initialize().await.expect("Silero init failed");

    let dataset = create_realistic_noise_dataset();
    if dataset.is_empty() {
        println!("⚠️  No background noise files loaded");
        println!("   Run: ./scripts/generate_10_noises.sh");
        return;
    }

    let mut evaluator = VADEvaluator::new();

    println!("\n📊 Silero VAD with 10 Realistic Background Noises:\n");
    println!("  SNR   Sample                                      → Result  Truth    Correct");
    println!("  ──────────────────────────────────────────────────────────────────────────────");

    for (audio, ground_truth, label, snr) in dataset {
        let result = vad.detect(&audio).await.expect("Detection failed");
        evaluator.record(result.is_speech, ground_truth, result.confidence);

        let correct = match (result.is_speech, ground_truth) {
            (true, GroundTruth::Speech) | (false, GroundTruth::Silence) => "✓",
            _ => "✗",
        };

        let snr_str = if snr.is_nan() {
            "  N/A".to_string()
        } else {
            format!("{:+5.0}dB", snr)
        };

        println!(
            "  {} {:45} → {:5}  {:?}  {}",
            snr_str,
            label.chars().take(45).collect::<String>(),
            result.is_speech,
            ground_truth,
            correct
        );
    }

    println!("\n{}", evaluator.report());

    let matrix = evaluator.matrix();
    println!("\n🔑 Key Metrics:");
    println!("   Specificity (noise rejection): {:.1}%", matrix.specificity() * 100.0);
    println!("   False Positive Rate: {:.1}%", matrix.false_positive_rate() * 100.0);
    println!("\n💡 Silero's performance on 10 different background noises:");
    println!("   Can it maintain 100% specificity across all noise types?");
}

#[tokio::test]
async fn test_bg_noise_summary() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║      Background Noise Robustness Test Summary          ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    println!("10 Realistic Background Noises:\n");
    for (i, noise) in BACKGROUND_NOISES.iter().enumerate() {
        println!("  {}. {}", i + 1, noise.description);
    }

    println!("\nTest Methodology:");
    println!("  - Mix formant-synthesized speech with each noise");
    println!("  - 4 SNR levels: +10dB, +5dB, 0dB, -5dB");
    println!("  - 3 speech types per noise (vowel A, I, plosive)");
    println!("  - Plus pure noise (should be rejected)");
    println!("  - Total: ~130 test samples\n");

    println!("Expected Results:\n");

    println!("RMS Threshold:");
    println!("  ❌ Cannot reject ANY background noise");
    println!("  ❌ High false positive rate on all 10 noises\n");

    println!("WebRTC (earshot):");
    println!("  ❓ May reject some pure noises");
    println!("  ❌ Likely high FPR on mixed speech+noise\n");

    println!("Silero Raw:");
    println!("  ✅ Should maintain 100% noise rejection");
    println!("  ✅ 0% false positives across all 10 noise types");
    println!("  ❌ Will reject synthetic speech (not trained on it)\n");

    println!("Run tests:");
    println!("  cargo test --release test_rms_realistic_bg_noise -- --nocapture");
    println!("  cargo test --release test_webrtc_realistic_bg_noise -- --nocapture");
    println!("  cargo test --release test_silero_realistic_bg_noise -- --ignored --nocapture");
}
