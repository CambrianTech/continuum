//! Sliding Audio Buffer
//!
//! Ring buffer with sliding window for continuous transcription.
//! Includes context overlap to prevent word boundary errors.

/// Sliding audio buffer for continuous transcription
///
/// Design:
/// - Ring buffer (fixed capacity, preallocated)
/// - Sliding window extraction (every TRANSCRIPTION_INTERVAL_SAMPLES)
/// - Context overlap (preserves CONTEXT_OVERLAP_SAMPLES from previous chunk)
/// - Zero-copy where possible
pub struct SlidingAudioBuffer {
    /// Ring buffer for audio samples (preallocated, fixed size)
    buffer: Vec<f32>,

    /// Capacity of ring buffer
    capacity: usize,

    /// Write position in ring buffer
    write_pos: usize,

    /// Total samples written (for timestamp tracking, wraps at usize::MAX)
    total_samples: usize,

    /// Last position we transcribed from
    last_transcribed_pos: usize,

    /// Transcription interval - trigger transcription every N samples
    transcription_interval: usize,

    /// Context overlap - include N samples from previous chunk
    context_overlap: usize,
}

impl SlidingAudioBuffer {
    /// Create new sliding buffer with default capacity
    ///
    /// # Arguments
    /// * `transcription_interval` - Samples between transcriptions (e.g., 24000 for 1.5s at 16kHz)
    /// * `context_overlap` - Samples to overlap from previous chunk (e.g., 8000 for 0.5s)
    pub fn new(transcription_interval: usize, context_overlap: usize) -> Self {
        // Default capacity: 10 intervals worth of samples
        let capacity = transcription_interval * 10;
        Self::with_capacity(capacity, transcription_interval, context_overlap)
    }

    /// Create new sliding buffer with specific capacity
    pub fn with_capacity(
        capacity: usize,
        transcription_interval: usize,
        context_overlap: usize,
    ) -> Self {
        assert!(capacity >= transcription_interval + context_overlap);
        assert!(context_overlap < transcription_interval);

        Self {
            buffer: vec![0.0; capacity],
            capacity,
            write_pos: 0,
            total_samples: 0,
            last_transcribed_pos: 0,
            transcription_interval,
            context_overlap,
        }
    }

    /// Add new audio samples to the buffer
    ///
    /// Samples are written to the ring buffer. If buffer is full, oldest samples are overwritten.
    pub fn push(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.buffer[self.write_pos] = sample;
            self.write_pos = (self.write_pos + 1) % self.capacity;
            self.total_samples = self.total_samples.wrapping_add(1);
        }
    }

    /// Check if buffer is ready for transcription
    ///
    /// Ready when we have accumulated TRANSCRIPTION_INTERVAL samples since last extraction
    pub fn ready_for_transcription(&self) -> bool {
        self.samples_since_last_extract() >= self.transcription_interval
    }

    /// Get number of samples accumulated since last extraction
    pub fn samples_since_last_extract(&self) -> usize {
        self.total_samples.wrapping_sub(self.last_transcribed_pos)
    }

    /// Get total samples written to buffer
    pub fn total_samples(&self) -> usize {
        self.total_samples
    }

    /// Extract next chunk for transcription
    ///
    /// Returns a chunk containing:
    /// - TRANSCRIPTION_INTERVAL samples total
    /// - First CONTEXT_OVERLAP samples are from previous chunk (if available)
    /// - Remaining samples are new
    ///
    /// Example with interval=24000, overlap=8000:
    /// - Chunk 1: [0...24000] (24000 samples, no overlap)
    /// - Chunk 2: [16000...40000] (24000 samples, first 8000 are overlap from chunk 1)
    /// - Chunk 3: [32000...56000] (24000 samples, first 8000 are overlap from chunk 2)
    ///
    /// This advances the last_transcribed_pos marker by (interval - overlap) each time.
    pub fn extract_chunk(&mut self) -> Vec<f32> {
        assert!(self.ready_for_transcription(), "Buffer not ready for extraction");

        // Always extract transcription_interval samples
        let chunk_size = self.transcription_interval;

        // Calculate start position in ring buffer
        let has_previous = self.last_transcribed_pos > 0;
        let start_offset = if has_previous {
            // Start from (last_transcribed_pos - overlap) to include context
            self.last_transcribed_pos.wrapping_sub(self.context_overlap)
        } else {
            // First extraction starts at 0
            0
        };

        // Extract samples (handling ring buffer wrap)
        let mut chunk = Vec::with_capacity(chunk_size);

        for i in 0..chunk_size {
            let pos = start_offset.wrapping_add(i) % self.capacity;
            chunk.push(self.buffer[pos]);
        }

        // Advance last_transcribed_pos by (interval - overlap)
        // This makes the next chunk overlap with current chunk
        let advance_by = if has_previous {
            self.transcription_interval - self.context_overlap
        } else {
            // First extraction: advance by full interval
            self.transcription_interval
        };

        self.last_transcribed_pos = self.last_transcribed_pos.wrapping_add(advance_by);

        chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let buffer = SlidingAudioBuffer::new(24000, 8000);
        assert_eq!(buffer.total_samples(), 0);
        assert!(!buffer.ready_for_transcription());
    }

    #[test]
    fn test_with_capacity() {
        let buffer = SlidingAudioBuffer::with_capacity(100000, 24000, 8000);
        assert_eq!(buffer.capacity, 100000);
    }

    #[test]
    #[should_panic]
    fn test_invalid_capacity() {
        // Capacity too small for interval + overlap
        SlidingAudioBuffer::with_capacity(1000, 24000, 8000);
    }

    #[test]
    #[should_panic]
    fn test_invalid_overlap() {
        // Overlap >= interval
        SlidingAudioBuffer::with_capacity(100000, 24000, 24000);
    }
}
