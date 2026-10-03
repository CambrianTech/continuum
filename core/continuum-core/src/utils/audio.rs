//! Audio utility functions
//!
//! Centralized audio processing utilities used across voice modules.
//! These handle common operations like:
//! - Sample format conversion (i16 <-> f32, bytes <-> samples)
//! - Base64 encoding/decoding for audio transport
//! - Resampling between sample rates

use base64::{engine::general_purpose::STANDARD, Engine};

/// Convert raw bytes to i16 audio samples (little-endian)
///
/// Returns empty vec if byte count is not even (i16 requires 2 bytes)
pub fn bytes_to_i16(data: &[u8]) -> Vec<i16> {
    if !data.len().is_multiple_of(2) {
        return Vec::new();
    }
    data.chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
}

/// Decode base64 string to i16 audio samples
///
/// The encoded data should be little-endian i16 samples.
/// Returns None if base64 is invalid or byte count is odd.
pub fn base64_decode_i16(data: &str) -> Option<Vec<i16>> {
    let bytes = STANDARD.decode(data).ok()?;
    if bytes.len() % 2 != 0 {
        return None;
    }
    Some(bytes_to_i16(&bytes))
}

/// Encode i16 audio samples to base64 string
///
/// Samples are encoded as little-endian bytes.
pub fn base64_encode_i16(samples: &[i16]) -> String {
    let bytes: Vec<u8> = samples.iter().flat_map(|&s| s.to_le_bytes()).collect();
    STANDARD.encode(&bytes)
}

/// Convert i16 PCM samples to f32 (-1.0 to 1.0)
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// Convert f32 samples (-1.0 to 1.0) to i16 PCM
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect()
}

/// Resample audio from any rate to any target rate
///
/// Uses high-quality FFT-based resampling via rubato crate.
/// Returns original samples if rates match or on error.
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    use rubato::Resampler;

    let params = rubato::FftFixedInOut::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        samples.len().min(1024),
        1, // mono
    );

    match params {
        Ok(mut resampler) => {
            let input = vec![samples.to_vec()];
            match resampler.process(&input, None) {
                Ok(output) => output.into_iter().next().unwrap_or_default(),
                Err(e) => {
                    tracing::error!("Resample failed: {}", e);
                    samples.to_vec()
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to create resampler: {}", e);
            samples.to_vec()
        }
    }
}

/// Stateful mono PCM rate conversion for an explicitly negotiated stream.
/// Retains filter history across packets and flushes the filter tail only once.
/// Unlike the batch convenience function, errors never return unconverted audio.
pub struct StreamingPcmResampler {
    filter: rubato::FftFixedInOut<f32>,
    pending: Vec<f32>,
    block: usize,
    skip: usize,
    input_samples: u64,
    output_samples: u64,
    from_rate: u32,
    to_rate: u32,
    finished: bool,
}

impl StreamingPcmResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Result<Self, String> {
        use rubato::Resampler;
        if from_rate == 0 || to_rate == 0 {
            return Err("PCM stream sample rates must be positive".into());
        }
        let filter = rubato::FftFixedInOut::new(
            from_rate as usize, to_rate as usize, (from_rate as usize / 100).max(1), 1,
        ).map_err(|e| format!("PCM stream resampler: {e}"))?;
        let block = filter.input_frames_next();
        let skip = filter.output_delay();
        Ok(Self { filter, pending: Vec::with_capacity(block), block, skip,
            input_samples: 0, output_samples: 0, from_rate, to_rate, finished: false })
    }

    fn process_block(&mut self) -> Result<Vec<i16>, String> {
        use rubato::Resampler;
        let result = self.filter.process(&[&self.pending], None)
            .map_err(|e| format!("PCM stream conversion: {e}"))?;
        self.pending.clear();
        let skip = self.skip.min(result[0].len());
        self.skip -= skip;
        Ok(f32_to_i16(&result[0][skip..]))
    }

    pub fn push(&mut self, samples: &[i16]) -> Result<Vec<i16>, String> {
        if self.finished { return Err("PCM stream is already finished".into()); }
        self.input_samples += samples.len() as u64;
        let mut output = Vec::new();
        let mut remaining = samples;
        while !remaining.is_empty() {
            let take = remaining.len().min(self.block - self.pending.len());
            self.pending.extend(remaining[..take].iter().map(|&s| s as f32 / 32768.0));
            remaining = &remaining[take..];
            if self.pending.len() == self.block { output.extend(self.process_block()?); }
        }
        self.output_samples += output.len() as u64;
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<i16>, String> {
        if self.finished { return Err("PCM stream is already finished".into()); }
        self.finished = true;
        let expected = self.input_samples * self.to_rate as u64 / self.from_rate as u64;
        let mut output = Vec::new();
        while self.output_samples < expected {
            self.pending.resize(self.block, 0.0);
            let block = self.process_block()?;
            let take = block.len().min((expected - self.output_samples) as usize);
            output.extend_from_slice(&block[..take]);
            self.output_samples += take as u64;
        }
        self.pending.clear();
        Ok(output)
    }
}

/// Resample audio to standard sample rate (common for speech models like Whisper)
pub fn resample_to_16k(samples: &[f32], from_rate: u32) -> Vec<f32> {
    use crate::audio_constants::AUDIO_SAMPLE_RATE;
    resample(samples, from_rate, AUDIO_SAMPLE_RATE)
}

/// Calculate RMS (root mean square) of audio samples
pub fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
    (sum_squares / samples.len() as f64).sqrt() as f32
}

/// Check if audio samples are effectively silence (RMS below threshold)
pub fn is_silence(samples: &[i16], threshold: f32) -> bool {
    calculate_rms(samples) < threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: packet boundaries must not reset filter phase, lose
    // the final audio tail, or change the native utterance's duration.
    #[test]
    fn streaming_pcm_is_packet_boundary_invariant() {
        let samples: Vec<i16> = (0..24013).map(|i| ((i as f32 * 0.11).sin() * 12000.0) as i16).collect();
        let mut whole = StreamingPcmResampler::new(24000, 16000).unwrap();
        let mut expected = whole.push(&samples).unwrap();
        expected.extend(whole.finish().unwrap());
        let mut stream = StreamingPcmResampler::new(24000, 16000).unwrap();
        let mut actual = Vec::new();
        for packet in samples.chunks(137) { actual.extend(stream.push(packet).unwrap()); }
        assert!(!actual.is_empty(), "playback must start before terminal completion");
        actual.extend(stream.finish().unwrap());
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), samples.len() * 16000 / 24000);
        assert!(stream.push(&[1]).is_err());
        assert!(stream.finish().is_err());
        assert!(StreamingPcmResampler::new(0, 16000).is_err());
    }

    #[test]
    fn test_bytes_to_i16_roundtrip() {
        let original: Vec<i16> = vec![0, 1000, -1000, i16::MAX, i16::MIN];
        let bytes: Vec<u8> = original.iter().flat_map(|&s| s.to_le_bytes()).collect();
        let decoded = bytes_to_i16(&bytes);
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_bytes_to_i16_odd_length() {
        let bytes = vec![0u8, 1, 2]; // 3 bytes - invalid
        let decoded = bytes_to_i16(&bytes);
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_base64_roundtrip() {
        let samples: Vec<i16> = vec![0, 1000, -1000, 32767, -32768];
        let encoded = base64_encode_i16(&samples);
        let decoded = base64_decode_i16(&encoded).unwrap();
        assert_eq!(samples, decoded);
    }

    #[test]
    fn test_base64_invalid() {
        assert!(base64_decode_i16("not valid base64!!!").is_none());
    }
}
