//! TTS Timing Benchmark
//!
//! Measures TTS synthesis time for different adapters and text lengths.
//! Outputs structured timing data for iteration and optimization.
//!
//! Run with: cargo test -p continuum-core --release --test tts_timing_benchmark -- --nocapture

mod common;

use common::{ipc_connect_with_timeout, ipc_request, IpcResult};
use serde::Serialize;
use std::time::{Duration, Instant};

/// Benchmark configuration
const TEST_PHRASES: &[(&str, &str)] = &[
    ("short", "Hello"),
    ("medium", "Hello, this is a test of text to speech synthesis."),
    ("long", "The quick brown fox jumps over the lazy dog. This is a longer sentence to test how TTS performance scales with text length. We want to understand the relationship between input text length and synthesis time."),
    ("very_long", "Hello and welcome to this comprehensive test of our text to speech system. We are measuring the time it takes to synthesize various lengths of text using different TTS adapters. This includes both local models like Piper and Kokoro, as well as potential cloud-based solutions. The goal is to optimize our voice pipeline to achieve sub-second latency for typical conversational responses. Real-time voice communication requires fast synthesis to maintain natural conversation flow."),
];

#[derive(Serialize)]
struct SynthesizeRequest {
    command: &'static str,
    text: String,
}

/// Timing result for a single synthesis
#[derive(Debug, Clone)]
struct TimingResult {
    adapter: String,
    phrase_name: String,
    text_chars: usize,
    synthesis_ms: u128,
    audio_duration_ms: u64,
    sample_count: usize,
    real_time_factor: f64,
}

impl TimingResult {
    fn to_csv_row(&self) -> String {
        format!("{},{},{},{},{},{},{:.3}",
            self.adapter,
            self.phrase_name,
            self.text_chars,
            self.synthesis_ms,
            self.audio_duration_ms,
            self.sample_count,
            self.real_time_factor,
        )
    }
}

/// Run timing benchmark via IPC to running server
fn benchmark_via_ipc(text: &str) -> Result<(Duration, u64, usize), String> {
    let mut stream = ipc_connect_with_timeout(Duration::from_secs(120))
        .ok_or_else(|| "Cannot connect to IPC server".to_string())?;

    let request = SynthesizeRequest {
        command: "voice/synthesize",
        text: text.to_string(),
    };

    // Time the full round-trip
    let start = Instant::now();
    let result = ipc_request(&mut stream, &request)?;
    let elapsed = start.elapsed();

    // Extract metadata from binary response
    let (header, pcm_bytes) = match result {
        IpcResult::Binary { header, data } => (header, data),
        IpcResult::Json(resp) => {
            if !resp.success {
                return Err(format!("TTS failed: {:?}", resp.error));
            }
            return Err("Expected binary response, got JSON-only".into());
        }
    };

    if !header.success {
        return Err(format!("TTS failed: {:?}", header.error));
    }

    let meta = header.result.ok_or("No result")?;
    let duration_ms = meta["duration_ms"].as_u64().unwrap_or(0);
    let sample_count = pcm_bytes.len() / 2; // i16 = 2 bytes

    Ok((elapsed, duration_ms, sample_count))
}

#[test]
fn benchmark_tts_timing() {
    println!("\n{}", "=".repeat(80));
    println!("TTS TIMING BENCHMARK");
    println!("{}\n", "=".repeat(80));

    // Check if server is running
    if !common::server_is_running() {
        println!("Server not running. Start with: npm start");
        println!("Skipping benchmark.");
        return;
    }

    println!("CSV Header: adapter,phrase,chars,synthesis_ms,audio_ms,samples,rtf");
    println!();

    let mut results: Vec<TimingResult> = Vec::new();

    for (phrase_name, text) in TEST_PHRASES {
        println!("Testing '{}' ({} chars)...", phrase_name, text.len());

        // Run multiple iterations for stability
        let iterations = 3;
        let mut timings: Vec<Duration> = Vec::new();
        let mut audio_duration_ms = 0u64;
        let mut sample_count = 0usize;

        for i in 0..iterations {
            match benchmark_via_ipc(text) {
                Ok((elapsed, duration, samples)) => {
                    timings.push(elapsed);
                    audio_duration_ms = duration;
                    sample_count = samples;
                    println!("  Run {}: {}ms", i + 1, elapsed.as_millis());
                }
                Err(e) => {
                    println!("  Run {}: FAILED - {}", i + 1, e);
                }
            }

            // Brief pause between iterations
            std::thread::sleep(Duration::from_millis(100));
        }

        if timings.is_empty() {
            println!("  All runs failed, skipping");
            continue;
        }

        // Calculate statistics
        let avg_ms: u128 = timings.iter().map(|d| d.as_millis()).sum::<u128>() / timings.len() as u128;
        let min_ms = timings.iter().map(|d| d.as_millis()).min().unwrap_or(0);
        let max_ms = timings.iter().map(|d| d.as_millis()).max().unwrap_or(0);

        let real_time_factor = if audio_duration_ms > 0 {
            avg_ms as f64 / audio_duration_ms as f64
        } else {
            0.0
        };

        let result = TimingResult {
            adapter: "piper".to_string(),
            phrase_name: phrase_name.to_string(),
            text_chars: text.len(),
            synthesis_ms: avg_ms,
            audio_duration_ms,
            sample_count,
            real_time_factor,
        };

        println!("  Avg: {avg_ms}ms | Min: {min_ms}ms | Max: {max_ms}ms | Audio: {audio_duration_ms}ms | RTF: {real_time_factor:.2}x");

        results.push(result);
    }

    // Print summary table
    println!("\n{}", "=".repeat(80));
    println!("SUMMARY");
    println!("{}", "=".repeat(80));
    println!();
    println!("{:<12} {:<8} {:<12} {:<12} {:<10} {:<12}",
        "Phrase", "Chars", "Synth(ms)", "Audio(ms)", "RTF", "Status");
    println!("{}", "-".repeat(70));

    for r in &results {
        let status = if r.real_time_factor < 1.0 {
            "REAL-TIME"
        } else if r.real_time_factor < 2.0 {
            "OK"
        } else if r.real_time_factor < 5.0 {
            "SLOW"
        } else {
            "VERY SLOW"
        };

        println!("{:<12} {:<8} {:<12} {:<12} {:<10.2} {:<12}",
            r.phrase_name, r.text_chars, r.synthesis_ms, r.audio_duration_ms, r.real_time_factor, status);
    }

    // Print CSV for easy export
    println!("\n{}", "=".repeat(80));
    println!("CSV OUTPUT (for analysis)");
    println!("{}", "=".repeat(80));
    println!("adapter,phrase,chars,synthesis_ms,audio_ms,samples,rtf");
    for r in &results {
        println!("{}", r.to_csv_row());
    }

    // Performance targets
    println!("\n{}", "=".repeat(80));
    println!("PERFORMANCE TARGETS");
    println!("{}", "=".repeat(80));
    println!("Target: RTF < 1.0 (faster than real-time)");
    println!("Acceptable: RTF < 2.0 (2x slower than real-time)");
    println!("Slow: RTF > 2.0 (needs optimization)");
    println!();

    // Check if we met targets
    let slow_results: Vec<_> = results.iter().filter(|r| r.real_time_factor > 2.0).collect();
    if !slow_results.is_empty() {
        println!("SLOW ITEMS REQUIRING OPTIMIZATION:");
        for r in &slow_results {
            println!("  - {} ({} chars): {:.1}x slower than real-time", r.phrase_name, r.text_chars, r.real_time_factor);
        }
    } else {
        println!("All items within acceptable performance range!");
    }

    println!("\n{}", "=".repeat(80));
}

#[test]
fn benchmark_tts_scaling() {
    println!("\n{}", "=".repeat(80));
    println!("TTS SCALING TEST (chars vs time)");
    println!("{}\n", "=".repeat(80));

    if !common::server_is_running() {
        println!("Server not running. Skipping.");
        return;
    }

    // Test with progressively longer texts
    let base_sentence = "Hello world. ";
    let lengths = [1, 2, 4, 8, 16];

    println!("{:<10} {:<8} {:<12} {:<12} {:<10}",
        "Reps", "Chars", "Synth(ms)", "Audio(ms)", "ms/char");
    println!("{}", "-".repeat(56));

    for reps in lengths {
        let text = base_sentence.repeat(reps);
        let chars = text.len();

        match benchmark_via_ipc(&text) {
            Ok((elapsed, audio_ms, _samples)) => {
                let synth_ms = elapsed.as_millis();
                let ms_per_char = synth_ms as f64 / chars as f64;

                println!("{reps:<10} {chars:<8} {synth_ms:<12} {audio_ms:<12} {ms_per_char:<10.2}");
            }
            Err(e) => {
                println!("{reps:<10} {chars:<8} FAILED: {e}");
            }
        }

        std::thread::sleep(Duration::from_millis(200));
    }

    println!("\nLook for linear vs superlinear scaling.");
    println!("Linear = O(n): ms/char stays constant");
    println!("Superlinear = O(n^2): ms/char increases with length");
}
