//! Example: Benchmarking VAD Quality
//!
//! Shows how to use the benchmarking framework to measure VAD quality:
//! - Load real audio samples
//! - Run VAD on test dataset
//! - Compute metrics (accuracy, precision, recall, FPR, latency)
//! - Generate markdown report
//! - Export JSON for tracking over time

use hound::WavReader;
use streaming_core::benchmark::BenchmarkSuite;
use streaming_core::vad::{
    ProductionVAD, ProductionVADConfig, SileroRawVAD, VoiceActivityDetection,
};
use std::collections::HashMap;
use std::path::Path;

const TEST_AUDIO_DIR: &str =
    "/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/workers/streaming-core/test_audio";

/// Load WAV file
fn load_wav(filename: &str) -> Result<Vec<i16>, String> {
    let path = Path::new(TEST_AUDIO_DIR).join(filename);
    let mut reader = WavReader::open(&path)
        .map_err(|e| format!("Failed to open {}: {}", filename, e))?;

    let samples: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    samples.map_err(|e| format!("Failed to read samples: {}", e))
}

#[tokio::test]
#[ignore] // Requires Silero model and real audio samples
async fn test_benchmark_vad_with_real_audio() {
    let mut silero = SileroRawVAD::new();
    silero.initialize().await.expect("Silero init failed");

    let mut suite = BenchmarkSuite::new("VAD Quality Assessment");

    println!("\n📊 Running VAD Benchmark on Real Audio\n");

    // Test speech samples (should detect as speech)
    let speech_files = vec!["speech_hello.wav", "speech_weather.wav", "speech_quick.wav"];

    for filename in speech_files {
        let samples = match load_wav(filename) {
            Ok(s) => s,
            Err(e) => {
                println!("⚠️  Skipping {}: {}", filename, e);
                continue;
            }
        };

        // Process in 480-sample frames
        for (chunk_idx, chunk) in samples.chunks(480).enumerate() {
            if chunk.len() < 480 {
                break;
            }

            // Benchmark this frame
            let test_id = format!("{}-frame{}", filename, chunk_idx);
            let chunk_vec = chunk.to_vec();

            let start = std::time::Instant::now();
            let result = silero.detect(&chunk_vec).await.expect("Detect failed");
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

            let is_speech = result.confidence > 0.3;

            let mut custom_metrics = HashMap::new();
            custom_metrics.insert("confidence".into(), result.confidence as f64);

            suite.add_result(streaming_core::benchmark::BenchmarkResult {
                test_id,
                ground_truth: serde_json::json!("speech"),
                prediction: serde_json::json!(if is_speech { "speech" } else { "silence" }),
                is_correct: Some(is_speech), // Ground truth is speech
                confidence: Some(result.confidence),
                latency_ms,
                custom_metrics,
            });
        }
    }

    // Test noise samples (should NOT detect as speech)
    let noise_files = vec!["noise_pink.wav", "noise_brown.wav", "noise_white.wav"];

    for filename in noise_files {
        let samples = match load_wav(filename) {
            Ok(s) => s,
            Err(e) => {
                println!("⚠️  Skipping {}: {}", filename, e);
                continue;
            }
        };

        // Process first 10 frames only (noise is long)
        for (chunk_idx, chunk) in samples.chunks(480).take(10).enumerate() {
            if chunk.len() < 480 {
                break;
            }

            let test_id = format!("{}-frame{}", filename, chunk_idx);
            let chunk_vec = chunk.to_vec();

            let start = std::time::Instant::now();
            let result = silero.detect(&chunk_vec).await.expect("Detect failed");
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

            let is_speech = result.confidence > 0.3;

            let mut custom_metrics = HashMap::new();
            custom_metrics.insert("confidence".into(), result.confidence as f64);

            suite.add_result(streaming_core::benchmark::BenchmarkResult {
                test_id,
                ground_truth: serde_json::json!("silence"),
                prediction: serde_json::json!(if is_speech { "speech" } else { "silence" }),
                is_correct: Some(!is_speech), // Ground truth is silence
                confidence: Some(result.confidence),
                latency_ms,
                custom_metrics,
            });
        }
    }

    // Generate report
    println!("{}", suite.report());

    // Export JSON for tracking
    let json = suite.to_json().expect("JSON export failed");
    println!("\n📄 Exported {} results to JSON\n", suite.compute_stats().total);

    // Save to file
    std::fs::write(
        "/tmp/vad_benchmark_results.json",
        json,
    )
    .expect("Failed to write JSON");

    println!("✅ Results saved to /tmp/vad_benchmark_results.json");

    // Assert quality
    let stats = suite.compute_stats();
    assert!(
        stats.accuracy.unwrap_or(0.0) > 0.8,
        "VAD accuracy should be >80%"
    );
}

#[tokio::test]
#[ignore] // Requires Silero model
async fn test_benchmark_compare_vad_configs() {
    println!("\n🔬 Comparing VAD Configurations\n");

    // Test configs
    let configs = vec![
        (
            "Threshold 0.3 (default)",
            ProductionVADConfig {
                silero_threshold: 0.3,
                use_two_stage: true,
                ..Default::default()
            },
        ),
        (
            "Threshold 0.2 (sensitive)",
            ProductionVADConfig {
                silero_threshold: 0.2,
                use_two_stage: true,
                ..Default::default()
            },
        ),
        (
            "Single-stage (Silero only)",
            ProductionVADConfig {
                silero_threshold: 0.3,
                use_two_stage: false,
                ..Default::default()
            },
        ),
    ];

    for (name, config) in configs {
        let mut vad = ProductionVAD::with_config(config);
        vad.initialize().await.expect("Init failed");

        let mut suite = BenchmarkSuite::new(name);

        // Load test audio
        let samples = match load_wav("speech_hello.wav") {
            Ok(s) => s,
            Err(_) => {
                println!("⚠️  Test audio not found, skipping");
                return;
            }
        };

        // Test first 10 frames
        for chunk in samples.chunks(480).take(10) {
            if chunk.len() < 480 {
                break;
            }

            let chunk_vec = chunk.to_vec();
            let start = std::time::Instant::now();
            let _result = vad.process_frame(&chunk_vec).await;
            let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

            suite.add_result(streaming_core::benchmark::BenchmarkResult {
                test_id: "speech_frame".into(),
                ground_truth: serde_json::json!("speech"),
                prediction: serde_json::json!("speech"),
                is_correct: Some(true),
                confidence: None,
                latency_ms,
                custom_metrics: HashMap::new(),
            });
        }

        let stats = suite.compute_stats();
        println!("{}:", name);
        println!("  Avg latency: {:.2}ms", stats.latency.mean_ms);
        println!("  P95 latency: {:.2}ms\n", stats.latency.p95_ms);
    }

    println!("💡 Use this to compare different VAD configurations");
}
