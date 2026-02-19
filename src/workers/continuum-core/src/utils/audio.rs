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
    if data.len() % 2 != 0 {
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
