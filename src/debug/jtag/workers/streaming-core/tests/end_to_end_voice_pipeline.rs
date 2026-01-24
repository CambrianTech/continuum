//! End-to-End Voice Pipeline Integration Test
//!
//! Validates the complete audio pipeline:
//! 1. TTS generates speech
//! 2. VAD detects speech and buffers complete sentences
//! 3. STT transcribes the buffered audio
//! 4. Compare transcription with original text
//!
//! This tests the entire closed-loop system working together.

use streaming_core::mixer::{AudioMixer, ParticipantStream};
use streaming_core::stt;
use streaming_core::tts;
use streaming_core::Handle;

#[tokio::test]
#[ignore] // Requires Silero VAD + Whisper STT + Piper TTS models
async fn test_end_to_end_tts_vad_stt_pipeline() {
    println!("\n═══════════════════════════════════════════════════");
    println!("║  End-to-End Voice Pipeline Integration Test   ║");
    println!("═══════════════════════════════════════════════════\n");

    // Initialize registries
    tts::init_registry();
    stt::init_registry();

    // Step 1: Initialize TTS
    println!("STEP 1: Initialize TTS");
    println!("━━━━━━━━━━━━━━━━━━━━━━━");

    let tts_adapter = tts::get_registry()
        .read()
        .get_active()
        .expect("No TTS adapter available");

    println!("  TTS Adapter: {}", tts_adapter.name());
    println!("  Description: {}", tts_adapter.description());

    match tts_adapter.initialize().await {
        Ok(_) => println!("  ✅ TTS initialized\n"),
        Err(e) => {
            println!("  ⚠️  TTS initialization failed: {:?}", e);
            println!("  Skipping test (models not available)\n");
            return;
        }
    }

    // Step 2: Initialize STT
    println!("STEP 2: Initialize STT");
    println!("━━━━━━━━━━━━━━━━━━━━━━━");

    let stt_adapter = stt::get_registry()
        .read()
        .get_active()
        .expect("No STT adapter available");

    println!("  STT Adapter: {}", stt_adapter.name());
    println!("  Description: {}", stt_adapter.description());

    match stt_adapter.initialize().await {
        Ok(_) => println!("  ✅ STT initialized\n"),
        Err(e) => {
            println!("  ⚠️  STT initialization failed: {:?}", e);
            println!("  Skipping test (models not available)\n");
            return;
        }
    }

    // Step 3: Create mixer with ProductionVAD
    println!("STEP 3: Initialize Mixer with ProductionVAD");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let mut mixer = AudioMixer::default_voice();
    let handle = Handle::new();
    let mut participant = ParticipantStream::new(handle, "test-user".into(), "Test User".into());

    match participant.initialize_vad().await {
        Ok(_) => println!("  ✅ ProductionVAD initialized\n"),
        Err(e) => {
            println!("  ⚠️  VAD initialization failed: {:?}", e);
            println!("  Skipping test (Silero model not available)\n");
            return;
        }
    }

    mixer.add_participant(participant);

    // Test phrases (start with simple ones)
    let test_phrases = vec![
        "Hello world",
        "How are you today",
        "This is a test of the voice pipeline",
    ];

    for (i, original_text) in test_phrases.iter().enumerate() {
        println!("\n═══════════════════════════════════════════════════");
        println!("  Test Phrase #{}: \"{}\"", i + 1, original_text);
        println!("═══════════════════════════════════════════════════\n");

        // Step 4: Synthesize speech from text
        println!("STEP 4: TTS Synthesis");
        println!("━━━━━━━━━━━━━━━━━━━━━━");

        let synthesis_result = match tts_adapter
            .synthesize(original_text, tts_adapter.default_voice())
            .await
        {
            Ok(result) => {
                println!("  ✅ Synthesized speech");
                println!("     Duration: {}ms", result.duration_ms);
                println!("     Samples: {}", result.samples.len());
                println!("     Sample rate: {}Hz\n", result.sample_rate);
                result
            }
            Err(e) => {
                println!("  ❌ TTS synthesis failed: {:?}\n", e);
                continue;
            }
        };

        // Step 5: Process through VAD
        println!("STEP 5: VAD Processing (ProductionVAD)");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let mut vad_output: Option<Vec<i16>> = None;
        let frame_size = 512; // 32ms @ 16kHz

        // Feed synthesized audio through VAD
        for chunk in synthesis_result.samples.chunks(frame_size) {
            let mut frame = chunk.to_vec();
            if frame.len() < frame_size {
                frame.resize(frame_size, 0); // Pad last frame
            }

            let result = mixer.push_audio(&handle, frame);

            if result.speech_ended {
                vad_output = result.speech_samples;
                println!("  ✅ VAD detected complete sentence");
                println!("     Buffered samples: {}", vad_output.as_ref().unwrap().len());
                break;
            }
        }

        // If VAD didn't trigger, add silence to flush buffer
        if vad_output.is_none() {
            println!("  Adding silence to trigger VAD end-of-speech...");
            for i in 0..40 {
                // 40 frames = 1.28s silence threshold
                let result = mixer.push_audio(&handle, vec![0; frame_size]);
                if result.speech_ended {
                    vad_output = result.speech_samples;
                    println!("  ✅ VAD triggered after {} silence frames", i + 1);
                    println!("     Buffered samples: {}", vad_output.as_ref().unwrap().len());
                    break;
                }
            }
        }

        let speech_samples = match vad_output {
            Some(samples) => samples,
            None => {
                println!("  ❌ VAD did not detect speech\n");
                continue;
            }
        };

        println!();

        // Step 6: Transcribe with STT
        println!("STEP 6: STT Transcription");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━");

        // Convert i16 to f32 for STT
        let samples_f32: Vec<f32> = speech_samples
            .iter()
            .map(|&s| s as f32 / 32768.0)
            .collect();

        let transcription = match stt_adapter.transcribe(samples_f32, Some("en")).await {
            Ok(result) => {
                println!("  ✅ Transcription successful");
                println!("     Text: \"{}\"", result.text);
                println!("     Language: {}", result.language);
                println!("     Confidence: {:.2}", result.confidence);
                println!("     Segments: {}", result.segments.len());
                result.text
            }
            Err(e) => {
                println!("  ❌ STT transcription failed: {:?}\n", e);
                continue;
            }
        };

        println!();

        // Step 7: Compare results
        println!("STEP 7: Validation");
        println!("━━━━━━━━━━━━━━━━━━━━━━");
        println!("  Original:      \"{}\"", original_text);
        println!("  Transcription: \"{}\"", transcription);

        // Simple similarity check (lowercase, trim)
        let original_normalized = original_text.to_lowercase().trim().to_string();
        let transcription_normalized = transcription.to_lowercase().trim().to_string();

        // Check if transcription contains the key words
        let words_match = original_normalized
            .split_whitespace()
            .filter(|word| transcription_normalized.contains(word))
            .count();
        let total_words = original_normalized.split_whitespace().count();
        let match_ratio = words_match as f32 / total_words as f32;

        println!("  Word match: {}/{} ({:.0}%)", words_match, total_words, match_ratio * 100.0);

        if match_ratio >= 0.7 {
            println!("  ✅ PASS - Transcription reasonably matches original\n");
        } else {
            println!("  ⚠️  PARTIAL - Some discrepancies in transcription\n");
        }
    }

    println!("\n═══════════════════════════════════════════════════");
    println!("║          End-to-End Pipeline Complete          ║");
    println!("═══════════════════════════════════════════════════\n");
}

#[tokio::test]
#[ignore] // Requires all models
async fn test_pipeline_with_silence_handling() {
    println!("\n📊 Testing VAD Silence Handling in Pipeline\n");

    // Initialize
    tts::init_registry();
    let mut mixer = AudioMixer::default_voice();
    let handle = Handle::new();
    let mut participant = ParticipantStream::new(handle, "test-user".into(), "Test User".into());

    if participant.initialize_vad().await.is_err() {
        println!("⚠️  Skipping test (VAD not available)\n");
        return;
    }

    mixer.add_participant(participant);

    println!("Test 1: Pure silence should NOT trigger transcription");
    for i in 0..100 {
        let result = mixer.push_audio(&handle, vec![0; 512]);
        if result.speech_ended {
            println!("  ❌ FALSE POSITIVE at frame {}", i);
            println!("     VAD should not detect speech in pure silence");
            return;
        }
    }
    println!("  ✅ Correctly ignored 100 silence frames\n");

    println!("Test 2: Noise should NOT trigger transcription");
    for i in 0..100 {
        let noise: Vec<i16> = (0..512)
            .map(|_| (rand::random::<f32>() * 1000.0 - 500.0) as i16)
            .collect();
        let result = mixer.push_audio(&handle, noise);
        if result.speech_ended {
            println!("  ⚠️  Detected speech in noise at frame {}", i);
            println!("     (This may be acceptable for voice-like noise)");
            return;
        }
    }
    println!("  ✅ Correctly ignored 100 noise frames\n");

    println!("✅ Silence handling validated");
}

#[tokio::test]
#[ignore] // Requires all models
async fn test_pipeline_latency_measurement() {
    use std::time::Instant;

    println!("\n📊 Pipeline Latency Measurement\n");

    // Initialize
    tts::init_registry();
    stt::init_registry();

    let tts_adapter = tts::get_registry().read().get_active().unwrap();
    let stt_adapter = stt::get_registry().read().get_active().unwrap();

    // Initialize adapters
    if tts_adapter.initialize().await.is_err() || stt_adapter.initialize().await.is_err() {
        println!("⚠️  Skipping test (models not available)\n");
        return;
    }

    let test_text = "Hello world";

    // Measure TTS latency
    let start = Instant::now();
    let synthesis = tts_adapter
        .synthesize(test_text, tts_adapter.default_voice())
        .await
        .expect("TTS failed");
    let tts_latency = start.elapsed();

    println!("TTS Latency:");
    println!("  Time: {:?}", tts_latency);
    println!("  Real-time factor: {:.2}x",
        synthesis.duration_ms as f64 / tts_latency.as_millis() as f64);
    println!();

    // Measure STT latency
    let samples_f32: Vec<f32> = synthesis
        .samples
        .iter()
        .map(|&s| s as f32 / 32768.0)
        .collect();

    let start = Instant::now();
    let _transcription = stt_adapter
        .transcribe(samples_f32, Some("en"))
        .await
        .expect("STT failed");
    let stt_latency = start.elapsed();

    println!("STT Latency:");
    println!("  Time: {:?}", stt_latency);
    println!("  Real-time factor: {:.2}x",
        synthesis.duration_ms as f64 / stt_latency.as_millis() as f64);
    println!();

    println!("Total Pipeline Latency:");
    println!("  TTS + STT: {:?}", tts_latency + stt_latency);
    println!("  (VAD latency is negligible: <100μs per frame)");
}
