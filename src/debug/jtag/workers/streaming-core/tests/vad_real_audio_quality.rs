//! VAD Quality Testing with Real Audio
//!
//! Tests ProductionVAD using:
//! - Real speech (macOS TTS)
//! - Real noise profiles (ffmpeg pink/brown/white noise)
//! - Real phonemes (TTS-generated plosives/fricatives)
//! - Mixed noisy speech at different SNR levels
//!
//! Measures:
//! - Detection accuracy
//! - Confidence scores
//! - Comparison between real vs synthetic audio

use hound::WavReader;
use streaming_core::vad::{
    GroundTruth, ProductionVAD, ProductionVADConfig, SileroRawVAD, TestAudioGenerator,
    VADEvaluator, VoiceActivityDetection, Vowel,
};
use std::path::Path;

const TEST_AUDIO_DIR: &str =
    "/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/workers/streaming-core/test_audio";

/// Load WAV file and return 16kHz mono samples
fn load_wav(filename: &str) -> Result<Vec<i16>, String> {
    let path = Path::new(TEST_AUDIO_DIR).join(filename);
    let mut reader = WavReader::open(&path)
        .map_err(|e| format!("Failed to open {}: {}", filename, e))?;

    let spec = reader.spec();
    if spec.sample_rate != 16000 {
        return Err(format!(
            "{}: Expected 16kHz, got {}Hz",
            filename, spec.sample_rate
        ));
    }

    if spec.channels != 1 {
        return Err(format!(
            "{}: Expected mono, got {} channels",
            filename, spec.channels
        ));
    }

    let samples: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    samples.map_err(|e| format!("Failed to read samples from {}: {}", filename, e))
}

#[tokio::test]
#[ignore] // Requires Silero model and real audio samples
async fn test_real_speech_quality() {
    let mut silero = SileroRawVAD::new();
    silero.initialize().await.expect("Silero init failed");

    println!("\n🎤 Real Speech Quality Assessment\n");
    println!("Testing macOS TTS-generated speech with Silero VAD");
    println!("--------------------------------------------------\n");

    let speech_files = vec![
        "speech_hello.wav",
        "speech_weather.wav",
        "speech_quick.wav",
        "phoneme_plosive.wav",
        "phoneme_fricative.wav",
    ];

    for filename in speech_files {
        let samples = load_wav(filename).unwrap_or_else(|e| {
            println!("⚠️  {}: {}", filename, e);
            vec![]
        });

        if samples.is_empty() {
            continue;
        }

        // Process in 480-sample frames (30ms @ 16kHz)
        let mut confidences = Vec::new();
        let mut speech_frames = 0;
        let total_frames = samples.len() / 480;

        for chunk in samples.chunks(480) {
            if chunk.len() < 480 {
                break;
            }

            let result = silero.detect(chunk).await.expect("Detect failed");
            confidences.push(result.confidence);

            if result.confidence > 0.3 {
                speech_frames += 1;
            }
        }

        let avg_confidence = confidences.iter().sum::<f32>() / confidences.len() as f32;
        let speech_percentage = (speech_frames as f64 / total_frames as f64) * 100.0;

        println!(
            "  {:25} avg_conf: {:.3}, speech: {:3.0}% ({}/{})",
            filename, avg_confidence, speech_percentage, speech_frames, total_frames
        );
    }
}

#[tokio::test]
#[ignore] // Requires Silero model and real audio samples
async fn test_real_noise_rejection() {
    let mut silero = SileroRawVAD::new();
    silero.initialize().await.expect("Silero init failed");

    println!("\n🔇 Real Noise Rejection Assessment\n");
    println!("Testing ffmpeg-generated noise profiles with Silero VAD");
    println!("----------------------------------------------------------\n");

    let noise_files = vec!["noise_pink.wav", "noise_brown.wav", "noise_white.wav"];

    for filename in noise_files {
        let samples = load_wav(filename).unwrap_or_else(|e| {
            println!("⚠️  {}: {}", filename, e);
            vec![]
        });

        if samples.is_empty() {
            continue;
        }

        // Process in 480-sample frames
        let mut confidences = Vec::new();
        let mut false_positives = 0;
        let total_frames = samples.len() / 480;

        for chunk in samples.chunks(480) {
            if chunk.len() < 480 {
                break;
            }

            let result = silero.detect(chunk).await.expect("Detect failed");
            confidences.push(result.confidence);

            if result.confidence > 0.3 {
                false_positives += 1;
            }
        }

        let avg_confidence = confidences.iter().sum::<f32>() / confidences.len() as f32;
        let fpr = (false_positives as f64 / total_frames as f64) * 100.0;

        println!(
            "  {:25} avg_conf: {:.3}, FPR: {:3.1}% ({}/{})",
            filename, avg_confidence, fpr, false_positives, total_frames
        );
    }
}

#[tokio::test]
#[ignore] // Requires Silero model and real audio samples
async fn test_real_noisy_speech_snr() {
    let mut silero = SileroRawVAD::new();
    silero.initialize().await.expect("Silero init failed");

    println!("\n📊 Real Noisy Speech SNR Analysis\n");
    println!("Testing speech mixed with real pink noise at different SNR levels");
    println!("--------------------------------------------------------------------\n");

    let noisy_files = vec![
        ("noisy_speech_snr10.wav", "+10dB"),
        ("noisy_speech_snr0.wav", "  0dB"),
        ("noisy_speech_snr-5.wav", " -5dB"),
    ];

    for (filename, snr_label) in noisy_files {
        let samples = load_wav(filename).unwrap_or_else(|e| {
            println!("⚠️  {}: {}", filename, e);
            vec![]
        });

        if samples.is_empty() {
            continue;
        }

        // Process in 480-sample frames
        let mut confidences = Vec::new();
        let mut speech_frames = 0;
        let total_frames = samples.len() / 480;

        for chunk in samples.chunks(480) {
            if chunk.len() < 480 {
                break;
            }

            let result = silero.detect(chunk).await.expect("Detect failed");
            confidences.push(result.confidence);

            if result.confidence > 0.3 {
                speech_frames += 1;
            }
        }

        let avg_confidence = confidences.iter().sum::<f32>() / confidences.len() as f32;
        let detection_rate = (speech_frames as f64 / total_frames as f64) * 100.0;

        println!(
            "  SNR {}: avg_conf: {:.3}, detected: {:3.0}% ({}/{})",
            snr_label, avg_confidence, detection_rate, speech_frames, total_frames
        );
    }
}

#[tokio::test]
#[ignore] // Requires Silero model and real audio samples
async fn test_real_vs_synthetic_comparison() {
    let mut silero = SileroRawVAD::new();
    silero.initialize().await.expect("Silero init failed");

    let gen = TestAudioGenerator::new(16000);

    println!("\n🔬 Real vs Synthetic Audio Comparison\n");
    println!("Comparing Silero confidence scores on real vs synthetic audio");
    println!("----------------------------------------------------------------\n");

    // Real speech
    let real_speech = load_wav("speech_hello.wav").unwrap();
    let real_plosive = load_wav("phoneme_plosive.wav").unwrap();
    let real_fricative = load_wav("phoneme_fricative.wav").unwrap();

    // Synthetic speech
    let synth_vowel_a = gen.generate_formant_speech(480, Vowel::A);
    let synth_plosive = gen.generate_plosive(480);
    let synth_fricative = gen.generate_fricative(480, 5000.0);

    // Test real speech (first 480 samples)
    let real_speech_frame = &real_speech[..480];
    let result_real = silero.detect(real_speech_frame).await.expect("Detect failed");

    println!("Real Speech (TTS):");
    println!("  Confidence: {:.3}", result_real.confidence);

    // Test synthetic speech
    let result_synth = silero.detect(&synth_vowel_a).await.expect("Detect failed");
    println!("Synthetic Speech (Vowel-A formants):");
    println!("  Confidence: {:.3}", result_synth.confidence);

    println!();

    // Test real plosive
    let real_plosive_frame = &real_plosive[..480];
    let result_real_plo = silero.detect(real_plosive_frame).await.expect("Detect failed");
    println!("Real Plosive (TTS):");
    println!("  Confidence: {:.3}", result_real_plo.confidence);

    // Test synthetic plosive
    let result_synth_plo = silero.detect(&synth_plosive).await.expect("Detect failed");
    println!("Synthetic Plosive (white noise burst):");
    println!("  Confidence: {:.3}", result_synth_plo.confidence);

    println!();

    // Test real fricative
    let real_fric_frame = &real_fricative[..480];
    let result_real_fric = silero.detect(real_fric_frame).await.expect("Detect failed");
    println!("Real Fricative (TTS):");
    println!("  Confidence: {:.3}", result_real_fric.confidence);

    // Test synthetic fricative
    let result_synth_fric = silero
        .detect(&synth_fricative)
        .await
        .expect("Detect failed");
    println!("Synthetic Fricative (filtered noise):");
    println!("  Confidence: {:.3}", result_synth_fric.confidence);

    println!("\n💡 Key Insight:");
    println!("  Real audio should have higher/more consistent confidence scores");
    println!("  This validates why synthetic audio struggles in comprehensive tests");
}
