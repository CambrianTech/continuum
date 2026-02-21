//! Shared Audio Utilities for TTS Adapters
//!
//! Single source of truth for audio normalization across all TTS backends.
//! Every adapter outputs audio at its own native sample rate and format —
//! this module normalizes everything to the system standard (16kHz i16 PCM).
//!
//! Analogous to context window normalization for LLMs: each model has different
//! characteristics, but the output interface is uniform.

use super::{SynthesisResult, TTSError};
use crate::audio_constants::AUDIO_SAMPLE_RATE;

/// Convert f32 PCM samples [-1.0, 1.0] to i16 PCM [-32767, 32767].
///
/// Clamps out-of-range values to prevent wrapping artifacts.
pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect()
}

/// Calculate audio duration in milliseconds from sample count and rate.
pub fn duration_ms(num_samples: usize, sample_rate: u32) -> u64 {
    (num_samples as u64 * 1000) / sample_rate as u64
}

/// High-quality resample using rubato FFT (f32 mono).
///
/// Uses `FftFixedInOut` for spectral-domain resampling — significantly
/// better quality than linear interpolation, especially for speech.
/// Handles tail samples via zero-padding with proportional output trimming.
pub fn resample_f32(
    samples: &[f32],
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<f32>, TTSError> {
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
    .map_err(|e| TTSError::SynthesisFailed(format!("Resampler init: {e}")))?;

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
            .map_err(|e| TTSError::SynthesisFailed(format!("Resample chunk: {e}")))?;
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
            .map_err(|e| TTSError::SynthesisFailed(format!("Resample tail: {e}")))?;
        let take = (remaining.len() as f64 * to_rate as f64 / from_rate as f64) as usize;
        output.extend_from_slice(&result[0][..take.min(result[0].len())]);
    }

    Ok(output)
}

/// Normalize f32 audio from any native sample rate to standard SynthesisResult.
///
/// Combines: resample → f32→i16 conversion → duration calculation.
/// This is the canonical path for all TTS adapters that produce f32 PCM.
///
/// # Arguments
/// * `samples` - f32 PCM at the adapter's native sample rate
/// * `native_rate` - the adapter's native sample rate (e.g., 24000 for SNAC/Mimi codecs)
pub fn normalize_audio(
    samples: &[f32],
    native_rate: u32,
) -> Result<SynthesisResult, TTSError> {
    if samples.is_empty() {
        return Err(TTSError::SynthesisFailed(
            "Cannot normalize empty audio".into(),
        ));
    }

    let resampled = resample_f32(samples, native_rate, AUDIO_SAMPLE_RATE)?;
    let i16_samples = f32_to_i16(&resampled);
    let dur = duration_ms(i16_samples.len(), AUDIO_SAMPLE_RATE);

    Ok(SynthesisResult {
        samples: i16_samples,
        sample_rate: AUDIO_SAMPLE_RATE,
        duration_ms: dur,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(duration_ms(8000, 16000), 500);   // 0.5 seconds
        assert_eq!(duration_ms(24000, 24000), 1000);  // 1 second at 24kHz
        assert_eq!(duration_ms(0, 16000), 0);
    }

    #[test]
    fn test_resample_same_rate() {
        let input = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let output = resample_f32(&input, 16000, 16000).unwrap();
        assert_eq!(output, input); // No-op when rates match
    }

    #[test]
    fn test_resample_empty() {
        let output = resample_f32(&[], 24000, 16000).unwrap();
        assert!(output.is_empty());
    }

    #[test]
    fn test_resample_24k_to_16k() {
        // Generate 1 second of 440Hz tone at 24kHz
        let num_samples = 24000;
        let input: Vec<f32> = (0..num_samples)
            .map(|i| (i as f32 / 24000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin())
            .collect();

        let output = resample_f32(&input, 24000, 16000).unwrap();

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
        assert!(max_amp > 0.5, "Resampled signal should retain amplitude, got {max_amp}");
    }

    #[test]
    fn test_normalize_audio_produces_standard_output() {
        // 0.5 seconds of 440Hz at 24kHz
        let input: Vec<f32> = (0..12000)
            .map(|i| (i as f32 / 24000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin())
            .collect();

        let result = normalize_audio(&input, 24000).unwrap();

        assert_eq!(result.sample_rate, AUDIO_SAMPLE_RATE);
        assert!(result.samples.len() > 0);
        assert!(result.duration_ms > 400 && result.duration_ms < 600,
            "Expected ~500ms, got {}ms", result.duration_ms);

        // Should have real audio, not silence
        let max_amp = result.samples.iter().map(|s| s.abs()).max().unwrap_or(0);
        assert!(max_amp > 1000, "Should have audible signal, max amp: {max_amp}");
    }

    #[test]
    fn test_normalize_audio_rejects_empty() {
        let result = normalize_audio(&[], 24000);
        assert!(result.is_err());
    }

    #[test]
    fn test_resample_preserves_silence() {
        let silence = vec![0.0f32; 24000]; // 1 second at 24kHz
        let output = resample_f32(&silence, 24000, 16000).unwrap();
        let max_amp: f32 = output.iter().map(|s| s.abs()).fold(0.0, f32::max);
        assert!(max_amp < 0.001, "Silence should remain silent after resample, max: {max_amp}");
    }
}
