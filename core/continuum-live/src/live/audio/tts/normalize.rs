//! Every TTS adapter's output, normalized to the system standard (16 kHz i16 PCM).
//!
//! Each adapter synthesizes at its own native rate and format; this is the one path from
//! that to a `SynthesisResult`, the way context-window normalization is for LLMs. The PCM
//! operations it composes are `live::audio::pcm`'s.

use super::{SynthesisResult, TTSError};
use continuum_base::audio_constants::AUDIO_SAMPLE_RATE;
use crate::live::audio::pcm;

/// Normalize f32 audio from any native sample rate to standard SynthesisResult.
///
/// Combines: resample → f32→i16 conversion → duration calculation.
/// This is the canonical path for all TTS adapters that produce f32 PCM.
///
/// # Arguments
/// * `samples` - f32 PCM at the adapter's native sample rate
/// * `native_rate` - the adapter's native sample rate (e.g., 24000 for SNAC/Mimi codecs)
pub fn normalize_audio(samples: &[f32], native_rate: u32) -> Result<SynthesisResult, TTSError> {
    if samples.is_empty() {
        return Err(TTSError::SynthesisFailed(
            "Cannot normalize empty audio".into(),
        ));
    }

    let resampled = pcm::resample(samples, native_rate, AUDIO_SAMPLE_RATE).map_err(TTSError::SynthesisFailed)?;
    let i16_samples = pcm::f32_to_i16(&resampled);
    let dur = pcm::duration_ms(i16_samples.len(), AUDIO_SAMPLE_RATE);

    Ok(SynthesisResult {
        samples: i16_samples,
        sample_rate: AUDIO_SAMPLE_RATE,
        duration_ms: dur,
        voice_name: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_audio_produces_standard_output() {
        // 0.5 seconds of 440Hz at 24kHz
        let input: Vec<f32> = (0..12000)
            .map(|i| (i as f32 / 24000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin())
            .collect();

        let result = normalize_audio(&input, 24000).unwrap();

        assert_eq!(result.sample_rate, AUDIO_SAMPLE_RATE);
        assert!(result.samples.len() > 0);
        assert!(
            result.duration_ms > 400 && result.duration_ms < 600,
            "Expected ~500ms, got {}ms",
            result.duration_ms
        );

        // Should have real audio, not silence
        let max_amp = result.samples.iter().map(|s| s.abs()).max().unwrap_or(0);
        assert!(
            max_amp > 1000,
            "Should have audible signal, max amp: {max_amp}"
        );
    }

    #[test]
    fn test_normalize_audio_rejects_empty() {
        let result = normalize_audio(&[], 24000);
        assert!(result.is_err());
    }
}
