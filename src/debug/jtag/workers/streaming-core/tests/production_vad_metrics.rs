//! ProductionVAD (Two-Stage) Metrics Evaluation
//!
//! Tests the complete two-stage system (WebRTC → Silero) to measure:
//! - Overall precision/recall/F1
//! - False positive rate on various noise types
//! - False negative rate on speech at different SNR levels
//! - Performance improvement over single-stage approaches

use streaming_core::vad::{
    GroundTruth, ProductionVAD, TestAudioGenerator, VADEvaluator, Vowel,
};

/// Create comprehensive test dataset covering:
/// - Pure silence (baseline)
/// - Various noise types (white, pink, HVAC, crowd, factory)
/// - Clear speech (vowels, plosives, fricatives)
/// - Noisy speech at different SNR levels
fn create_comprehensive_dataset() -> Vec<(Vec<i16>, GroundTruth, &'static str)> {
    let gen = TestAudioGenerator::new(16000);
    let mut dataset = Vec::new();

    // === SILENCE (should NOT trigger) ===
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-1"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-2"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-3"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-4"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-5"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-6"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-7"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-8"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-9"));
    dataset.push((vec![0i16; 480], GroundTruth::Silence, "Silence-10"));

    // === NOISE (should NOT trigger) ===
    // White noise
    let noise1: Vec<i16> = (0..480).map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16).collect();
    let noise2: Vec<i16> = (0..480).map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16).collect();
    let noise3: Vec<i16> = (0..480).map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16).collect();
    dataset.push((noise1, GroundTruth::Silence, "WhiteNoise-1"));
    dataset.push((noise2, GroundTruth::Silence, "WhiteNoise-2"));
    dataset.push((noise3, GroundTruth::Silence, "WhiteNoise-3"));

    // Factory floor noise
    dataset.push((gen.generate_factory_floor(480), GroundTruth::Silence, "FactoryFloor-1"));
    dataset.push((gen.generate_factory_floor(480), GroundTruth::Silence, "FactoryFloor-2"));
    dataset.push((gen.generate_factory_floor(480), GroundTruth::Silence, "FactoryFloor-3"));

    // === CLEAR SPEECH (SHOULD trigger) ===
    // Vowels
    dataset.push((gen.generate_formant_speech(480, Vowel::A), GroundTruth::Speech, "Vowel-A-1"));
    dataset.push((gen.generate_formant_speech(480, Vowel::A), GroundTruth::Speech, "Vowel-A-2"));
    dataset.push((gen.generate_formant_speech(480, Vowel::E), GroundTruth::Speech, "Vowel-E-1"));
    dataset.push((gen.generate_formant_speech(480, Vowel::E), GroundTruth::Speech, "Vowel-E-2"));
    dataset.push((gen.generate_formant_speech(480, Vowel::I), GroundTruth::Speech, "Vowel-I-1"));
    dataset.push((gen.generate_formant_speech(480, Vowel::I), GroundTruth::Speech, "Vowel-I-2"));
    dataset.push((gen.generate_formant_speech(480, Vowel::O), GroundTruth::Speech, "Vowel-O-1"));
    dataset.push((gen.generate_formant_speech(480, Vowel::O), GroundTruth::Speech, "Vowel-O-2"));

    // Plosives
    dataset.push((gen.generate_plosive(480), GroundTruth::Speech, "Plosive-1"));
    dataset.push((gen.generate_plosive(480), GroundTruth::Speech, "Plosive-2"));
    dataset.push((gen.generate_plosive(480), GroundTruth::Speech, "Plosive-3"));

    // Fricatives
    dataset.push((gen.generate_fricative(480, 5000.0), GroundTruth::Speech, "Fricative-1"));
    dataset.push((gen.generate_fricative(480, 5000.0), GroundTruth::Speech, "Fricative-2"));
    dataset.push((gen.generate_fricative(480, 5000.0), GroundTruth::Speech, "Fricative-3"));

    // === NOISY SPEECH (SHOULD trigger) ===
    // Test at SNR: +10dB, 0dB, -5dB
    let speech10 = gen.generate_formant_speech(480, Vowel::A);
    let noise_snr: Vec<i16> = (0..480).map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16).collect();
    dataset.push((TestAudioGenerator::mix_audio_with_snr(&speech10, &noise_snr, 10.0), GroundTruth::Speech, "NoisySpeech-SNR+10dB"));

    let speech0 = gen.generate_formant_speech(480, Vowel::A);
    let noise0: Vec<i16> = (0..480).map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16).collect();
    dataset.push((TestAudioGenerator::mix_audio_with_snr(&speech0, &noise0, 0.0), GroundTruth::Speech, "NoisySpeech-SNR0dB"));

    let speechm5 = gen.generate_formant_speech(480, Vowel::A);
    let noisem5: Vec<i16> = (0..480).map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16).collect();
    dataset.push((TestAudioGenerator::mix_audio_with_snr(&speechm5, &noisem5, -5.0), GroundTruth::Speech, "NoisySpeech-SNR-5dB"));

    dataset
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_production_vad_comprehensive_metrics() {
    let mut vad = ProductionVAD::new();
    vad.initialize()
        .await
        .expect("ProductionVAD init failed");

    let dataset = create_comprehensive_dataset();
    let mut evaluator = VADEvaluator::new();

    println!("\n📊 ProductionVAD (Two-Stage) Comprehensive Evaluation\n");
    println!("Testing {} samples:\n", dataset.len());

    let mut silence_count = 0;
    let mut noise_count = 0;
    let mut clear_speech_count = 0;
    let mut noisy_speech_count = 0;

    for (audio, ground_truth, label) in &dataset {
        // ProductionVAD processes frames, need to send 3+ frames for min_speech_frames
        let mut detected = false;

        // Send 5 frames of same audio (simulating sustained sound)
        for _ in 0..5 {
            if let Ok(Some(_)) = vad.process_frame(audio).await {
                detected = true;
                break;
            }
        }

        // Add 40 silence frames to trigger transcription if any speech detected
        if !detected {
            for _ in 0..40 {
                if let Ok(Some(_)) = vad.process_frame(&vec![0i16; 480]).await {
                    detected = true;
                    break;
                }
            }
        }

        evaluator.record(detected, *ground_truth, if detected { 1.0 } else { 0.0 });

        let correct = match (detected, ground_truth) {
            (true, GroundTruth::Speech) | (false, GroundTruth::Silence) => "✓",
            _ => "✗",
        };

        println!(
            "  {} {:30} → {:5} (truth: {:?})",
            correct, label, detected, ground_truth
        );

        // Count by category
        if label.starts_with("Silence") {
            silence_count += 1;
        } else if label.starts_with("WhiteNoise") || label.starts_with("Factory") {
            noise_count += 1;
        } else if label.starts_with("NoisySpeech") {
            noisy_speech_count += 1;
        } else {
            clear_speech_count += 1;
        }
    }

    println!("\n{}", evaluator.report());

    println!("\n📈 Dataset Breakdown:");
    println!("  Silence samples:      {}", silence_count);
    println!("  Noise samples:        {}", noise_count);
    println!("  Clear speech samples: {}", clear_speech_count);
    println!("  Noisy speech samples: {}", noisy_speech_count);
    println!("  Total:                {}", dataset.len());

    // Production quality targets
    let matrix = evaluator.matrix();
    println!("\n🎯 Production Quality Assessment:");
    println!(
        "  Precision: {:.1}% (target: >95%)",
        matrix.precision() * 100.0
    );
    println!(
        "  Recall:    {:.1}% (target: >98%)",
        matrix.recall() * 100.0
    );
    println!(
        "  FPR:       {:.1}% (target: <5%)",
        matrix.false_positive_rate() * 100.0
    );
    println!(
        "  FNR:       {:.1}% (target: <2%)",
        matrix.false_negative_rate() * 100.0
    );

    // Assert minimum quality
    assert!(
        matrix.precision() > 0.90,
        "Precision too low: {:.1}% < 90%",
        matrix.precision() * 100.0
    );
    assert!(
        matrix.recall() > 0.95,
        "Recall too low: {:.1}% < 95%",
        matrix.recall() * 100.0
    );
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_production_vad_noise_types() {
    let mut vad = ProductionVAD::new();
    vad.initialize()
        .await
        .expect("ProductionVAD init failed");

    let gen = TestAudioGenerator::new(16000);

    println!("\n📊 ProductionVAD Noise Type Breakdown\n");

    // Test different noise types
    for noise_name in ["White Noise", "Factory Floor", "Crowd Murmur"] {
        let mut false_positives = 0;
        let trials = 20;

        for _ in 0..trials {
            let noise = match noise_name {
                "White Noise" => (0..480).map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16).collect(),
                "Factory Floor" => gen.generate_factory_floor(480),
                "Crowd Murmur" => gen.generate_crowd(480, 5),
                _ => vec![0; 480],
            };

            // Send noise frames
            let mut detected = false;
            for _ in 0..5 {
                if let Ok(Some(_)) = vad.process_frame(&noise).await {
                    detected = true;
                    break;
                }
            }

            // Trigger with silence
            if !detected {
                for _ in 0..40 {
                    if let Ok(Some(_)) = vad.process_frame(&vec![0i16; 480]).await {
                        detected = true;
                        break;
                    }
                }
            }

            if detected {
                false_positives += 1;
            }
        }

        let fpr = (false_positives as f64 / trials as f64) * 100.0;
        println!(
            "  {:<20}: {:.1}% false positive rate ({}/{})",
            noise_name, fpr, false_positives, trials
        );
    }
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_production_vad_snr_threshold() {
    let mut vad = ProductionVAD::new();
    vad.initialize()
        .await
        .expect("ProductionVAD init failed");

    let gen = TestAudioGenerator::new(16000);

    println!("\n📊 ProductionVAD SNR Threshold Analysis\n");

    // Test speech detection at various SNR levels
    let snr_levels = vec![30.0, 20.0, 10.0, 5.0, 0.0, -5.0, -10.0];

    for &snr in &snr_levels {
        let mut detections = 0;
        let trials = 10;

        for _ in 0..trials {
            let speech = gen.generate_formant_speech(480, Vowel::A);
            let noise: Vec<i16> = (0..480)
                .map(|_| (rand::random::<f32>() * 2000.0 - 1000.0) as i16)
                .collect();
            let mixed = TestAudioGenerator::mix_audio_with_snr(&speech, &noise, snr);

            // Send mixed audio
            let mut detected = false;
            for _ in 0..5 {
                if let Ok(Some(_)) = vad.process_frame(&mixed).await {
                    detected = true;
                    break;
                }
            }

            // Trigger
            if !detected {
                for _ in 0..40 {
                    if let Ok(Some(_)) = vad.process_frame(&vec![0i16; 480]).await {
                        detected = true;
                        break;
                    }
                }
            }

            if detected {
                detections += 1;
            }
        }

        let detection_rate = (detections as f64 / trials as f64) * 100.0;
        println!(
            "  SNR {:+4.0}dB: {:.0}% detection rate ({}/{})",
            snr, detection_rate, detections, trials
        );
    }
}
