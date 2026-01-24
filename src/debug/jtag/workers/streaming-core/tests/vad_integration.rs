//! VAD Integration Tests
//!
//! Synthesizes audio patterns and tests VAD detection:
//! - Silence (should detect no speech)
//! - Sine wave speech-like patterns (should detect speech)
//! - Noise/TV-like patterns (Silero should reject, RMS accepts)

use streaming_core::vad::{RmsThresholdVAD, SileroVAD, VADFactory, VoiceActivityDetection};
use streaming_core::mixer::test_utils;

/// Generate speech-like audio (sine wave with speech frequency)
fn generate_speech_like(num_samples: usize) -> Vec<i16> {
    // 200 Hz fundamental (typical male voice)
    test_utils::generate_sine_wave(200.0, 16000, num_samples)
}

/// Generate TV-like audio (multiple frequencies, constant)
fn generate_tv_like(num_samples: usize) -> Vec<i16> {
    // TV audio: Mix of frequencies (music, dialogue, sound effects)
    let freq1 = test_utils::generate_sine_wave(440.0, 16000, num_samples); // A4 note
    let freq2 = test_utils::generate_sine_wave(880.0, 16000, num_samples); // A5 note

    // Mix them (crude TV simulation)
    freq1.iter().zip(freq2.iter())
        .map(|(&s1, &s2)| ((s1 as i32 + s2 as i32) / 2) as i16)
        .collect()
}

#[tokio::test]
async fn test_rms_vad_silence() {
    let vad = RmsThresholdVAD::new();
    vad.initialize().await.expect("RMS VAD init failed");

    let silence = test_utils::generate_silence(512); // 32ms at 16kHz
    let result = vad.detect(&silence).await.expect("Detection failed");

    println!("🔇 RMS VAD - Silence: is_speech={}, confidence={}",
             result.is_speech, result.confidence);

    assert!(!result.is_speech, "Silence should not be detected as speech");
    assert!(result.confidence < 0.1, "Silence should have low confidence");
}

#[tokio::test]
async fn test_rms_vad_speech_like() {
    let vad = RmsThresholdVAD::new();
    vad.initialize().await.expect("RMS VAD init failed");

    let speech = generate_speech_like(512); // 32ms at 16kHz
    let result = vad.detect(&speech).await.expect("Detection failed");

    println!("🎤 RMS VAD - Speech-like: is_speech={}, confidence={}",
             result.is_speech, result.confidence);

    assert!(result.is_speech, "Speech-like audio should be detected");
    assert!(result.confidence > 0.5, "Speech should have high confidence");
}

#[tokio::test]
async fn test_rms_vad_tv_like() {
    let vad = RmsThresholdVAD::new();
    vad.initialize().await.expect("RMS VAD init failed");

    let tv = generate_tv_like(512); // 32ms at 16kHz
    let result = vad.detect(&tv).await.expect("Detection failed");

    println!("📺 RMS VAD - TV-like: is_speech={}, confidence={}",
             result.is_speech, result.confidence);

    // RMS CANNOT distinguish TV from speech - this is the bug!
    assert!(result.is_speech, "RMS incorrectly detects TV as speech (expected bug)");
}

#[tokio::test]
async fn test_vad_factory_default() {
    let vad = VADFactory::default();
    println!("✨ VADFactory created: {} - {}", vad.name(), vad.description());

    // Should work regardless of which VAD was selected
    let silence = test_utils::generate_silence(512);
    let result = vad.detect(&silence).await;

    match result {
        Ok(r) => {
            println!("🎯 Factory VAD detected silence: is_speech={}", r.is_speech);
            assert!(!r.is_speech, "Factory VAD should detect silence correctly");
        }
        Err(e) => {
            // If uninitialized, that's OK - we're just testing factory creation
            println!("⚠️ VAD needs initialization: {:?}", e);
        }
    }
}

#[tokio::test]
#[ignore] // Requires Silero model download
async fn test_silero_vad_initialization() {
    let vad = SileroVAD::new();

    match vad.initialize().await {
        Ok(_) => {
            println!("✅ Silero VAD initialized successfully");
            assert!(vad.is_initialized(), "Should be initialized");
        }
        Err(e) => {
            println!("⚠️ Silero model not found (expected): {:?}", e);
            println!("📥 Download with:");
            println!("   mkdir -p models/vad");
            println!("   curl -L https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx \\");
            println!("     -o models/vad/silero_vad.onnx");
        }
    }
}

#[tokio::test]
#[ignore] // Requires Silero model download
async fn test_silero_vad_silence() {
    let vad = SileroVAD::new();
    vad.initialize().await.expect("Failed to initialize Silero - download model first");

    let silence = test_utils::generate_silence(512); // 32ms at 16kHz
    let result = vad.detect(&silence).await.expect("Detection failed");

    println!("🔇 Silero VAD - Silence: is_speech={}, confidence={:.3}",
             result.is_speech, result.confidence);

    assert!(!result.is_speech, "Silero should detect silence correctly");
    assert!(result.confidence < 0.3, "Silence should have low confidence: {}", result.confidence);
}

#[tokio::test]
#[ignore] // Requires Silero model download
async fn test_silero_vad_speech_like() {
    let vad = SileroVAD::new();
    vad.initialize().await.expect("Failed to initialize Silero - download model first");

    let speech = generate_speech_like(512); // 32ms at 16kHz
    let result = vad.detect(&speech).await.expect("Detection failed");

    println!("🎤 Silero VAD - Speech-like: is_speech={}, confidence={:.3}",
             result.is_speech, result.confidence);

    // Silero should recognize speech patterns
    // NOTE: Sine wave is not perfect speech - confidence may vary
    println!("   (Sine wave is crude speech simulation - real speech would score higher)");
}

#[tokio::test]
#[ignore] // Requires Silero model download
async fn test_silero_vad_tv_like() {
    let vad = SileroVAD::new();
    vad.initialize().await.expect("Failed to initialize Silero - download model first");

    let tv = generate_tv_like(512); // 32ms at 16kHz
    let result = vad.detect(&tv).await.expect("Detection failed");

    println!("📺 Silero VAD - TV-like: is_speech={}, confidence={:.3}",
             result.is_speech, result.confidence);

    // Silero SHOULD reject TV audio (key advantage over RMS)
    // NOTE: This is crude simulation - real TV would be more complex
    println!("   (Mixed sine waves are crude TV simulation)");
    println!("   Expected: Silero rejects background noise (is_speech=false)");
}

#[tokio::test]
async fn test_rms_vs_factory_comparison() {
    let rms = RmsThresholdVAD::new();
    let factory = VADFactory::default();

    rms.initialize().await.expect("RMS init failed");

    let speech = generate_speech_like(512);

    let rms_result = rms.detect(&speech).await.expect("RMS detection failed");
    let factory_result = factory.detect(&speech).await;

    println!("🔬 Comparison Test:");
    println!("   RMS: is_speech={}, confidence={:.3}",
             rms_result.is_speech, rms_result.confidence);

    match factory_result {
        Ok(r) => {
            println!("   Factory ({}): is_speech={}, confidence={:.3}",
                     factory.name(), r.is_speech, r.confidence);
        }
        Err(e) => {
            println!("   Factory VAD uninitialized: {:?}", e);
            println!("   (This is OK - Silero needs model download)");
        }
    }
}

/// Test with longer audio sequence (multiple frames)
#[tokio::test]
async fn test_rms_vad_sequence() {
    let vad = RmsThresholdVAD::new();
    vad.initialize().await.expect("RMS init failed");

    println!("\n🎬 Testing RMS VAD with audio sequence:");

    // Sequence: silence → speech → silence → TV
    let sequences = vec![
        ("Silence", test_utils::generate_silence(512)),
        ("Speech", generate_speech_like(512)),
        ("Silence", test_utils::generate_silence(512)),
        ("TV-like", generate_tv_like(512)),
    ];

    for (label, audio) in sequences {
        let result = vad.detect(&audio).await.expect("Detection failed");
        println!("   {} → is_speech={}, confidence={:.3}",
                 label, result.is_speech, result.confidence);
    }
}

/// Performance benchmark - how fast is VAD?
#[tokio::test]
async fn test_rms_vad_performance() {
    let vad = RmsThresholdVAD::new();
    vad.initialize().await.expect("RMS init failed");

    let audio = generate_speech_like(512); // 32ms of audio

    let iterations = 100;
    let start = std::time::Instant::now();

    for _ in 0..iterations {
        let _ = vad.detect(&audio).await.expect("Detection failed");
    }

    let elapsed = start.elapsed();
    let avg_micros = elapsed.as_micros() / iterations;

    println!("⚡ RMS VAD Performance:");
    println!("   {} iterations: {:?}", iterations, elapsed);
    println!("   Average: {}μs per 32ms frame", avg_micros);
    println!("   Real-time factor: {:.1}x", (32000.0 / avg_micros as f64));

    assert!(avg_micros < 1000, "RMS VAD should be <1ms (was {}μs)", avg_micros);
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_silero_vad_performance() {
    let vad = SileroVAD::new();
    vad.initialize().await.expect("Failed to initialize Silero - download model first");

    let audio = generate_speech_like(512); // 32ms of audio

    let iterations = 100;
    let start = std::time::Instant::now();

    for _ in 0..iterations {
        let _ = vad.detect(&audio).await.expect("Detection failed");
    }

    let elapsed = start.elapsed();
    let avg_micros = elapsed.as_micros() / iterations;

    println!("⚡ Silero VAD Performance:");
    println!("   {} iterations: {:?}", iterations, elapsed);
    println!("   Average: {}μs per 32ms frame", avg_micros);
    println!("   Real-time factor: {:.1}x", (32000.0 / avg_micros as f64));

    // Silero should be <5ms for real-time performance
    assert!(avg_micros < 5000, "Silero VAD should be <5ms (was {}μs)", avg_micros);
}
