//! PCM samples: the conversions, resampling and level checks every voice path in
//! `live::audio` shares.
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

/// High-quality resample using rubato FFT (f32 mono).
///
/// Uses `FftFixedInOut` for spectral-domain resampling — significantly
/// better quality than linear interpolation, especially for speech.
/// Handles tail samples via zero-padding with proportional output trimming.
///
/// It replaced a resampler here that made ONE `process` call, sized for at most 1024
/// frames, over the whole input. rubato 0.15 rejects only a SHORT buffer
/// (`validate_buffers`) and reads `input_frames_next()` frames of a longer one, so
/// everything past the first chunk was silently dropped, and an init error handed
/// back the input as if resampled. Its one caller asked 16 kHz to 16 kHz, which
/// returned before rubato, so the bug never ran.
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, String> {
    if from_rate == to_rate || samples.is_empty() {
        return Ok(samples.to_vec());
    }

    use rubato::{FftFixedInOut, Resampler};

    let chunk_size = 1024;
    let mut resampler = FftFixedInOut::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        chunk_size,
        1, // mono
    )
    .map_err(|e| format!("Resampler init: {e}"))?;

    let mut output = Vec::with_capacity(
        (samples.len() as f64 * to_rate as f64 / from_rate as f64) as usize + chunk_size,
    );

    let input_frames = resampler.input_frames_next();
    let mut pos = 0;

    // Process full chunks
    while pos + input_frames <= samples.len() {
        let chunk = &samples[pos..pos + input_frames];
        let result = resampler
            .process(&[chunk], None)
            .map_err(|e| format!("Resample chunk: {e}"))?;
        output.extend_from_slice(&result[0]);
        pos += input_frames;
    }

    // Handle remaining samples — zero-pad, process, take proportional output
    if pos < samples.len() {
        let remaining = &samples[pos..];
        let mut padded = vec![0.0f32; input_frames];
        padded[..remaining.len()].copy_from_slice(remaining);
        let result = resampler
            .process(&[&padded], None)
            .map_err(|e| format!("Resample tail: {e}"))?;
        let take = (remaining.len() as f64 * to_rate as f64 / from_rate as f64) as usize;
        output.extend_from_slice(&result[0][..take.min(result[0].len())]);
    }

    Ok(output)
}

/// Calculate audio duration in milliseconds from sample count and rate.
pub fn duration_ms(num_samples: usize, sample_rate: u32) -> u64 {
    (num_samples as u64 * 1000) / sample_rate as u64
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

    #[test]
    fn test_f32_to_i16_range() {
        let input = vec![-1.0, -0.5, 0.0, 0.5, 1.0];
        let output = f32_to_i16(&input);
        assert_eq!(output[0], -32767);
        assert_eq!(output[2], 0);
        assert_eq!(output[4], 32767);
    }

    #[test]
    fn test_f32_to_i16_clamps() {
        // Values outside [-1, 1] should be clamped, not wrap
        let input = vec![-2.0, 2.0, 5.0, -5.0];
        let output = f32_to_i16(&input);
        assert_eq!(output[0], -32767);
        assert_eq!(output[1], 32767);
        assert_eq!(output[2], 32767);
        assert_eq!(output[3], -32767);
    }

    #[test]
    fn test_duration_ms() {
        assert_eq!(duration_ms(16000, 16000), 1000); // 1 second
        assert_eq!(duration_ms(8000, 16000), 500); // 0.5 seconds
        assert_eq!(duration_ms(24000, 24000), 1000); // 1 second at 24kHz
        assert_eq!(duration_ms(0, 16000), 0);
    }

    #[test]
    fn test_resample_same_rate() {
        let input = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let output = resample(&input, 16000, 16000).unwrap();
        assert_eq!(output, input); // No-op when rates match
    }

    #[test]
    fn test_resample_empty() {
        let output = resample(&[], 24000, 16000).unwrap();
        assert!(output.is_empty());
    }

    #[test]
    fn test_resample_24k_to_16k() {
        // Generate 1 second of 440Hz tone at 24kHz
        let num_samples = 24000;
        let input: Vec<f32> = (0..num_samples)
            .map(|i| (i as f32 / 24000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin())
            .collect();

        let output = resample(&input, 24000, 16000).unwrap();

        // Output should be ~16000 samples (1 second at 16kHz)
        let expected_len = 16000;
        let tolerance = 100; // rubato chunk boundaries may vary slightly
        assert!(
            (output.len() as i64 - expected_len as i64).unsigned_abs() < tolerance,
            "Expected ~{expected_len} samples, got {}",
            output.len()
        );

        // Output should still have signal (not all zeros)
        let max_amp: f32 = output.iter().map(|s| s.abs()).fold(0.0, f32::max);
        assert!(
            max_amp > 0.5,
            "Resampled signal should retain amplitude, got {max_amp}"
        );
    }

    #[test]
    fn test_resample_preserves_silence() {
        let silence = vec![0.0f32; 24000]; // 1 second at 24kHz
        let output = resample(&silence, 24000, 16000).unwrap();
        let max_amp: f32 = output.iter().map(|s| s.abs()).fold(0.0, f32::max);
        assert!(
            max_amp < 0.001,
            "Silence should remain silent after resample, max: {max_amp}"
        );
    }
}
