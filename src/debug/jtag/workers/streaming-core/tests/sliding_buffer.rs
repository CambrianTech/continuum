//! Tests for SlidingAudioBuffer - TDD Phase 1
//!
//! Tests written FIRST before implementation to drive the design.

use streaming_core::continuous::SlidingAudioBuffer;

/// Constants matching CONTINUOUS-TRANSCRIPTION-ARCHITECTURE.md spec
const SAMPLE_RATE: usize = 16000; // 16kHz
const TRANSCRIPTION_INTERVAL_SAMPLES: usize = 24000; // 1.5s at 16kHz
const CONTEXT_OVERLAP_SAMPLES: usize = 8000; // 0.5s overlap

#[test]
fn test_sliding_buffer_push_accumulates_samples() {
    let mut buffer = SlidingAudioBuffer::new(TRANSCRIPTION_INTERVAL_SAMPLES, CONTEXT_OVERLAP_SAMPLES);

    // Push 1000 samples
    let samples: Vec<f32> = vec![0.5; 1000];
    buffer.push(&samples);

    // Should have accumulated 1000 samples
    assert_eq!(buffer.total_samples(), 1000);
    assert!(!buffer.ready_for_transcription());
}

#[test]
fn test_sliding_buffer_ready_for_transcription_timing() {
    let mut buffer = SlidingAudioBuffer::new(TRANSCRIPTION_INTERVAL_SAMPLES, CONTEXT_OVERLAP_SAMPLES);

    // Not ready initially
    assert!(!buffer.ready_for_transcription());

    // Push samples up to threshold (24000 samples = 1.5s)
    for _ in 0..24 {
        let samples: Vec<f32> = vec![0.5; 1000];
        buffer.push(&samples);
    }

    // Should be ready now
    assert_eq!(buffer.total_samples(), 24000);
    assert!(buffer.ready_for_transcription());
}

#[test]
fn test_sliding_buffer_extract_with_overlap() {
    let mut buffer = SlidingAudioBuffer::new(TRANSCRIPTION_INTERVAL_SAMPLES, CONTEXT_OVERLAP_SAMPLES);

    // Push 30000 samples (more than threshold)
    for i in 0..30 {
        // Create unique pattern for each chunk so we can verify extraction
        let samples: Vec<f32> = vec![i as f32; 1000];
        buffer.push(&samples);
    }

    // Extract chunk
    let chunk = buffer.extract_chunk();

    // Should include full interval + overlap from previous
    // First extraction has no previous, so just interval samples
    assert_eq!(chunk.len(), TRANSCRIPTION_INTERVAL_SAMPLES);

    // After extraction, should still have overlap samples for next chunk
    assert_eq!(buffer.samples_since_last_extract(), 30000 - TRANSCRIPTION_INTERVAL_SAMPLES);
}

#[test]
fn test_sliding_buffer_context_overlap_preserved() {
    let mut buffer = SlidingAudioBuffer::new(TRANSCRIPTION_INTERVAL_SAMPLES, CONTEXT_OVERLAP_SAMPLES);

    // Push first batch (30000 samples)
    for i in 0..30 {
        let samples: Vec<f32> = vec![i as f32 / 100.0; 1000];
        buffer.push(&samples);
    }

    // Extract first chunk
    let first_chunk = buffer.extract_chunk();
    assert_eq!(first_chunk.len(), TRANSCRIPTION_INTERVAL_SAMPLES);

    // Push another batch
    for i in 30..60 {
        let samples: Vec<f32> = vec![i as f32 / 100.0; 1000];
        buffer.push(&samples);
    }

    // Extract second chunk - should include overlap from first
    let second_chunk = buffer.extract_chunk();

    // Second chunk should start with overlap from end of first chunk
    // We can verify by checking the sample values
    let overlap_start = &first_chunk[first_chunk.len() - CONTEXT_OVERLAP_SAMPLES..];
    let second_chunk_start = &second_chunk[..CONTEXT_OVERLAP_SAMPLES];

    assert_eq!(overlap_start.len(), CONTEXT_OVERLAP_SAMPLES);
    assert_eq!(second_chunk_start.len(), CONTEXT_OVERLAP_SAMPLES);

    // Values should match (overlap preserved)
    for i in 0..CONTEXT_OVERLAP_SAMPLES {
        assert!((overlap_start[i] - second_chunk_start[i]).abs() < 0.001);
    }
}

#[test]
fn test_sliding_buffer_wrap_around() {
    // Create buffer with smaller capacity for easier testing
    let capacity = 40000;
    let interval = 24000;
    let overlap = 8000;

    let mut buffer = SlidingAudioBuffer::with_capacity(capacity, interval, overlap);

    // Fill buffer to capacity
    for i in 0..(capacity / 1000) {
        let samples: Vec<f32> = vec![i as f32; 1000];
        buffer.push(&samples);
    }

    // Extract (wraps write position)
    let _ = buffer.extract_chunk();

    // Push more samples (should wrap around in ring buffer)
    for i in 0..30 {
        let samples: Vec<f32> = vec![(40 + i) as f32; 1000];
        buffer.push(&samples);
    }

    // Should still work correctly after wrap
    assert!(buffer.ready_for_transcription());

    let chunk = buffer.extract_chunk();
    assert_eq!(chunk.len(), interval);
}

#[test]
fn test_sliding_buffer_multiple_extractions() {
    let mut buffer = SlidingAudioBuffer::new(TRANSCRIPTION_INTERVAL_SAMPLES, CONTEXT_OVERLAP_SAMPLES);

    // Simulate 10 seconds of audio (160000 samples)
    for i in 0..160 {
        let samples: Vec<f32> = vec![i as f32 / 100.0; 1000];
        buffer.push(&samples);
    }

    let mut extraction_count = 0;
    while buffer.ready_for_transcription() {
        let chunk = buffer.extract_chunk();
        assert_eq!(chunk.len(), TRANSCRIPTION_INTERVAL_SAMPLES);
        extraction_count += 1;
        println!("Extraction {}: chunk_len={}, samples_since_last={}",
                 extraction_count, chunk.len(), buffer.samples_since_last_extract());

        // Break to avoid infinite loop in case of bug
        if extraction_count > 20 {
            panic!("Too many extractions - infinite loop detected");
        }
    }

    // Should have extracted multiple chunks
    // 10s of audio: first at 1.5s, then every 1.0s (interval - overlap)
    // So: 1.5s, 2.5s, 3.5s, 4.5s, 5.5s, 6.5s, 7.5s, 8.5s, 9.5s = 9 transcriptions
    println!("Total extractions: {}", extraction_count);
    assert!(extraction_count >= 8 && extraction_count <= 10,
            "Expected 8-10 extractions, got {}", extraction_count);
}

#[test]
fn test_sliding_buffer_empty_buffer() {
    let buffer = SlidingAudioBuffer::new(TRANSCRIPTION_INTERVAL_SAMPLES, CONTEXT_OVERLAP_SAMPLES);

    // Empty buffer not ready
    assert_eq!(buffer.total_samples(), 0);
    assert!(!buffer.ready_for_transcription());
}

#[test]
fn test_sliding_buffer_partial_samples() {
    let mut buffer = SlidingAudioBuffer::new(TRANSCRIPTION_INTERVAL_SAMPLES, CONTEXT_OVERLAP_SAMPLES);

    // Push only half the required samples
    for _ in 0..12 {
        let samples: Vec<f32> = vec![0.5; 1000];
        buffer.push(&samples);
    }

    // Should have 12000 samples but not ready (needs 24000)
    assert_eq!(buffer.total_samples(), 12000);
    assert!(!buffer.ready_for_transcription());
}
