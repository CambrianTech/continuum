//! VAD Testing with Speech + Background Noise
//!
//! Tests all VAD implementations on realistic scenarios:
//! - Speech + white noise (poor microphone quality)
//! - Speech + factory floor (user's specific use case)
//! - Speech + TV background
//!
//! Uses SNR (Signal-to-Noise Ratio) to control background noise level.

use streaming_core::mixer::test_utils;
use streaming_core::vad::{
    GroundTruth, RmsThresholdVAD, SileroRawVAD, TestAudioGenerator, VADEvaluator,
    VoiceActivityDetection, Vowel, WebRtcVAD,
};

/// Create realistic test dataset: speech + background noise at various SNR levels
fn create_noisy_speech_dataset() -> Vec<(Vec<i16>, GroundTruth, &'static str, f32)> {
    let gen = TestAudioGenerator::new(16000);

    // Generate base speech samples
    let speech_a = gen.generate_formant_speech(240, Vowel::A);
    let speech_e = gen.generate_formant_speech(240, Vowel::E);
    let speech_i = gen.generate_formant_speech(240, Vowel::I);
    let plosive = gen.generate_plosive(240);
    let fricative = gen.generate_fricative(240, 5000.0);

    // Generate background noises
    let white_noise = test_utils::generate_noise(240);
    let factory = gen.generate_factory_floor(240);
    let tv = gen.generate_tv_dialogue(240);

    let mut dataset = vec![];

    // SNR levels to test:
    // +20 dB = Very clean (speech 100x louder than noise)
    // +10 dB = Clean (speech 10x louder)
    // +5 dB = Moderate noise
    // 0 dB = Equal volume (challenging)
    // -5 dB = Noisy (noise louder)
    let snr_levels = vec![20.0, 10.0, 5.0, 0.0, -5.0];

    for &snr in &snr_levels {
        // Speech + White Noise (poor microphone)
        dataset.push((
            TestAudioGenerator::mix_audio_with_snr(&speech_a, &white_noise, snr),
            GroundTruth::Speech,
            "Speech/A + WhiteNoise",
            snr,
        ));

        dataset.push((
            TestAudioGenerator::mix_audio_with_snr(&speech_e, &white_noise, snr),
            GroundTruth::Speech,
            "Speech/E + WhiteNoise",
            snr,
        ));

        // Speech + Factory Floor (user's use case)
        dataset.push((
            TestAudioGenerator::mix_audio_with_snr(&speech_i, &factory, snr),
            GroundTruth::Speech,
            "Speech/I + Factory",
            snr,
        ));

        dataset.push((
            TestAudioGenerator::mix_audio_with_snr(&plosive, &factory, snr),
            GroundTruth::Speech,
            "Plosive + Factory",
            snr,
        ));

        // Speech + TV Background
        dataset.push((
            TestAudioGenerator::mix_audio_with_snr(&fricative, &tv, snr),
            GroundTruth::Speech,
            "Fricative + TV",
            snr,
        ));
    }

    // Pure noise samples (should always be rejected)
    dataset.push((
        white_noise.clone(),
        GroundTruth::Silence,
        "Pure WhiteNoise",
        f32::NAN,
    ));
    dataset.push((factory.clone(), GroundTruth::Silence, "Pure Factory", f32::NAN));
    dataset.push((tv, GroundTruth::Silence, "Pure TV", f32::NAN));

    // Pure silence
    dataset.push((
        test_utils::generate_silence(240),
        GroundTruth::Silence,
        "Pure Silence",
        f32::NAN,
    ));

    dataset
}

#[tokio::test]
async fn test_rms_noisy_speech() {
    let vad = RmsThresholdVAD::new();
    vad.initialize().await.expect("RMS init failed");

    let dataset = create_noisy_speech_dataset();
    let mut evaluator = VADEvaluator::new();

    println!("\n📊 RMS VAD with Background Noise:\n");
    println!("  SNR   Sample                        → Result (conf)  Truth      Correct");
    println!("  ─────────────────────────────────────────────────────────────────────────");

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
            "  {} {:30} → {:5} ({:.3})  {:?}  {}",
            snr_str,
            label,
            result.is_speech,
            result.confidence,
            ground_truth,
            correct
        );
    }

    println!("\n{}", evaluator.report());
}

#[tokio::test]
async fn test_webrtc_noisy_speech() {
    let vad = WebRtcVAD::new();
    vad.initialize().await.expect("WebRTC init failed");

    let dataset = create_noisy_speech_dataset();
    let mut evaluator = VADEvaluator::new();

    println!("\n📊 WebRTC VAD with Background Noise:\n");
    println!("  SNR   Sample                        → Result (conf)  Truth      Correct");
    println!("  ─────────────────────────────────────────────────────────────────────────");

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
            "  {} {:30} → {:5} ({:.3})  {:?}  {}",
            snr_str,
            label,
            result.is_speech,
            result.confidence,
            ground_truth,
            correct
        );
    }

    println!("\n{}", evaluator.report());

    let matrix = evaluator.matrix();
    println!("\n💡 Key Insight: How does WebRTC handle noisy speech?");
    println!("   Specificity (noise rejection): {:.1}%", matrix.specificity() * 100.0);
    println!("   Recall (speech detection): {:.1}%", matrix.recall() * 100.0);
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_silero_noisy_speech() {
    let vad = SileroRawVAD::new();
    vad.initialize().await.expect("Silero init failed");

    let dataset = create_noisy_speech_dataset();
    let mut evaluator = VADEvaluator::new();

    println!("\n📊 Silero VAD with Background Noise:\n");
    println!("  SNR   Sample                        → Result (conf)  Truth      Correct");
    println!("  ─────────────────────────────────────────────────────────────────────────");

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
            "  {} {:30} → {:5} ({:.3})  {:?}  {}",
            snr_str,
            label,
            result.is_speech,
            result.confidence,
            ground_truth,
            correct
        );
    }

    println!("\n{}", evaluator.report());

    let matrix = evaluator.matrix();
    println!("\n💡 Key Insight: Silero's robustness to background noise");
    println!("   Specificity (noise rejection): {:.1}%", matrix.specificity() * 100.0);
    println!("   Recall (speech detection): {:.1}%", matrix.recall() * 100.0);
    println!("   Can Silero detect speech in factory floor noise at 0dB SNR?");
}

#[tokio::test]
async fn test_snr_threshold_analysis() {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║         SNR Threshold Analysis for VAD Systems          ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    println!("Signal-to-Noise Ratio (SNR) Levels:\n");
    println!("  +20 dB = Very clean (speech 100x louder than noise)");
    println!("  +10 dB = Clean (speech 10x louder) - typical office");
    println!("   +5 dB = Moderate noise - busy cafe");
    println!("    0 dB = Equal volume - factory floor, construction");
    println!("   -5 dB = Noisy (noise louder) - very loud environment\n");

    println!("Expected Performance:\n");

    println!("RMS Threshold:");
    println!("  ❌ Cannot distinguish speech from background noise");
    println!("  ❌ Will trigger on mixed signal regardless of SNR");
    println!("  📊 Expected: Poor performance at all SNR levels\n");

    println!("WebRTC (earshot):");
    println!("  ❓ Rule-based frequency analysis");
    println!("  ⚡ May work better at high SNR (>10dB)");
    println!("  ❌ Likely struggles at low SNR (<5dB)");
    println!("  📊 Expected: Degrades as SNR decreases\n");

    println!("Silero Raw:");
    println!("  ✅ ML-trained on 6000+ hours with noise");
    println!("  ✅ Should handle moderate SNR (5-10dB)");
    println!("  ❓ Performance at 0dB SNR (equal volume)?");
    println!("  📊 Expected: Best performance, but still synthetic speech\n");

    println!("⚠️  IMPORTANT:");
    println!("  These tests use FORMANT SYNTHESIS for speech.");
    println!("  Silero may still reject synthetic speech even with noise.");
    println!("  Real human speech would show better results.\n");

    println!("Run tests with: cargo test --release test_*_noisy_speech -- --nocapture");
}
