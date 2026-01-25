//! Adaptive VAD Configuration
//!
//! Automatically adjusts VAD thresholds based on:
//! - Background noise level
//! - Recent false positive/negative rate
//! - Speech pattern characteristics
//!
//! This solves real-world problems:
//! - Factory floor (loud) vs quiet office (same threshold doesn't work)
//! - User moves from quiet room to noisy environment
//! - Background noise changes over time

use super::{VADError, VADResult, VoiceActivityDetection};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Noise level estimation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseLevel {
    Quiet,      // Library, bedroom at night
    Moderate,   // Office, home
    Loud,       // Cafe, street
    VeryLoud,   // Factory floor, construction site
}

/// Adaptive VAD configuration that learns from environment
#[derive(Debug, Clone)]
pub struct AdaptiveConfig {
    /// Current Silero threshold (adapts over time)
    pub silero_threshold: f32,

    /// Estimated noise level
    pub noise_level: NoiseLevel,

    /// Recent background noise RMS
    pub background_rms: f32,

    /// Recent false positive rate (0.0-1.0)
    pub false_positive_rate: f32,

    /// Recent false negative rate (0.0-1.0)
    pub false_negative_rate: f32,

    /// Last adaptation time
    pub last_adapted: Instant,

    /// Adaptation interval
    pub adaptation_interval: Duration,
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        Self {
            silero_threshold: 0.3,  // Start conservative
            noise_level: NoiseLevel::Moderate,
            background_rms: 0.0,
            false_positive_rate: 0.0,
            false_negative_rate: 0.0,
            last_adapted: Instant::now(),
            adaptation_interval: Duration::from_secs(10),
        }
    }
}

impl AdaptiveConfig {
    /// Update threshold based on noise level
    pub fn update_for_noise_level(&mut self, level: NoiseLevel) {
        self.noise_level = level;

        // Lower threshold in noisier environments to catch speech
        self.silero_threshold = match level {
            NoiseLevel::Quiet => 0.4,      // Can be more selective
            NoiseLevel::Moderate => 0.3,   // Standard
            NoiseLevel::Loud => 0.25,      // Lower to catch speech in noise
            NoiseLevel::VeryLoud => 0.2,   // Very low threshold
        };
    }

    /// Adapt based on recent performance
    pub fn adapt_from_metrics(&mut self, false_positives: usize, false_negatives: usize, total: usize) {
        if total == 0 || Instant::now() - self.last_adapted < self.adaptation_interval {
            return;
        }

        self.false_positive_rate = false_positives as f32 / total as f32;
        self.false_negative_rate = false_negatives as f32 / total as f32;

        // Too many false positives (transcribing noise) - raise threshold
        if self.false_positive_rate > 0.1 {  // >10% FP rate
            self.silero_threshold = (self.silero_threshold + 0.05).min(0.6);
        }

        // Too many false negatives (missing speech) - lower threshold
        if self.false_negative_rate > 0.1 {  // >10% FN rate
            self.silero_threshold = (self.silero_threshold - 0.05).max(0.15);
        }

        self.last_adapted = Instant::now();
    }

    /// Estimate noise level from recent audio
    pub fn estimate_noise_level(recent_silence_rms: &[f32]) -> NoiseLevel {
        if recent_silence_rms.is_empty() {
            return NoiseLevel::Moderate;
        }

        // Average RMS during silence frames
        let avg_rms: f32 = recent_silence_rms.iter().sum::<f32>() / recent_silence_rms.len() as f32;

        match avg_rms {
            x if x < 100.0 => NoiseLevel::Quiet,
            x if x < 500.0 => NoiseLevel::Moderate,
            x if x < 2000.0 => NoiseLevel::Loud,
            _ => NoiseLevel::VeryLoud,
        }
    }
}

/// Adaptive VAD wrapper
///
/// Wraps any VAD implementation and automatically adjusts thresholds
/// based on environment noise and performance metrics.
pub struct AdaptiveVAD<V: VoiceActivityDetection> {
    /// Underlying VAD implementation
    vad: V,

    /// Adaptive configuration
    config: AdaptiveConfig,

    /// Recent silence RMS values (for noise estimation)
    silence_rms_history: VecDeque<f32>,

    /// Recent detection results (for FP/FN tracking)
    recent_results: VecDeque<(bool, f32)>, // (is_speech, confidence)

    /// Maximum history size
    max_history: usize,
}

impl<V: VoiceActivityDetection> AdaptiveVAD<V> {
    /// Create adaptive VAD with default config
    pub fn new(vad: V) -> Self {
        Self {
            vad,
            config: AdaptiveConfig::default(),
            silence_rms_history: VecDeque::new(),
            recent_results: VecDeque::new(),
            max_history: 100,
        }
    }

    /// Create with custom configuration
    pub fn with_config(vad: V, config: AdaptiveConfig) -> Self {
        Self {
            vad,
            config,
            silence_rms_history: VecDeque::new(),
            recent_results: VecDeque::new(),
            max_history: 100,
        }
    }

    /// Get current threshold
    pub fn current_threshold(&self) -> f32 {
        self.config.silero_threshold
    }

    /// Get estimated noise level
    pub fn noise_level(&self) -> NoiseLevel {
        self.config.noise_level
    }

    /// Calculate RMS of audio frame
    fn calculate_rms(samples: &[i16]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }

        let sum_squares: f64 = samples
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();

        ((sum_squares / samples.len() as f64).sqrt()) as f32
    }

    /// Process frame and adapt thresholds
    pub async fn detect_adaptive(&mut self, samples: &[i16]) -> Result<VADResult, VADError> {
        // Get raw VAD result
        let result = self.vad.detect(samples).await?;

        // Track result
        self.recent_results.push_back((result.is_speech, result.confidence));
        if self.recent_results.len() > self.max_history {
            self.recent_results.pop_front();
        }

        // Update noise estimation if silence
        if !result.is_speech {
            let rms = Self::calculate_rms(samples);
            self.silence_rms_history.push_back(rms);
            if self.silence_rms_history.len() > self.max_history {
                self.silence_rms_history.pop_front();
            }

            // Re-estimate noise level periodically
            if self.silence_rms_history.len() >= 50 {
                let noise_samples: Vec<f32> = self.silence_rms_history.iter().copied().collect();
                let estimated_level = AdaptiveConfig::estimate_noise_level(&noise_samples);

                if estimated_level != self.config.noise_level {
                    self.config.update_for_noise_level(estimated_level);
                }
            }
        }

        // Apply adaptive threshold
        let is_speech_adaptive = result.confidence > self.config.silero_threshold;

        Ok(VADResult {
            is_speech: is_speech_adaptive,
            confidence: result.confidence,
        })
    }

    /// Report user feedback (for improving adaptation)
    ///
    /// Call this when user corrects VAD mistakes:
    /// - false_positive: VAD detected speech but it was noise
    /// - false_negative: VAD missed speech
    pub fn report_user_feedback(&mut self, false_positive: bool, false_negative: bool) {
        if false_positive {
            // Too many false positives - raise threshold
            self.config.silero_threshold = (self.config.silero_threshold + 0.02).min(0.6);
        }

        if false_negative {
            // Missed speech - lower threshold
            self.config.silero_threshold = (self.config.silero_threshold - 0.02).max(0.15);
        }
    }

    /// Get current configuration
    pub fn config(&self) -> &AdaptiveConfig {
        &self.config
    }

    /// Get underlying VAD
    pub fn inner(&self) -> &V {
        &self.vad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_level_estimation() {
        // Quiet environment
        let quiet_rms = vec![50.0, 60.0, 55.0, 45.0];
        assert_eq!(
            AdaptiveConfig::estimate_noise_level(&quiet_rms),
            NoiseLevel::Quiet
        );

        // Moderate noise
        let moderate_rms = vec![200.0, 300.0, 250.0];
        assert_eq!(
            AdaptiveConfig::estimate_noise_level(&moderate_rms),
            NoiseLevel::Moderate
        );

        // Loud environment
        let loud_rms = vec![1000.0, 1200.0, 1100.0];
        assert_eq!(
            AdaptiveConfig::estimate_noise_level(&loud_rms),
            NoiseLevel::Loud
        );

        // Very loud
        let very_loud_rms = vec![3000.0, 3500.0, 3200.0];
        assert_eq!(
            AdaptiveConfig::estimate_noise_level(&very_loud_rms),
            NoiseLevel::VeryLoud
        );
    }

    #[test]
    fn test_threshold_adaptation() {
        let mut config = AdaptiveConfig::default();

        // Initial threshold
        assert_eq!(config.silero_threshold, 0.3);

        // Move to loud environment - threshold should decrease
        config.update_for_noise_level(NoiseLevel::Loud);
        assert_eq!(config.silero_threshold, 0.25);

        // Move to very loud - threshold decreases more
        config.update_for_noise_level(NoiseLevel::VeryLoud);
        assert_eq!(config.silero_threshold, 0.2);

        // Move to quiet - threshold increases
        config.update_for_noise_level(NoiseLevel::Quiet);
        assert_eq!(config.silero_threshold, 0.4);
    }

    #[test]
    fn test_performance_based_adaptation() {
        let mut config = AdaptiveConfig::default();
        let initial_threshold = config.silero_threshold;

        // Set last_adapted to past to bypass adaptation_interval check
        config.last_adapted = Instant::now() - Duration::from_secs(11);

        // High false positive rate - should raise threshold
        config.adapt_from_metrics(15, 0, 100); // 15% FP rate
        assert!(config.silero_threshold > initial_threshold);

        // Reset and set last_adapted again
        config.silero_threshold = 0.3;
        config.last_adapted = Instant::now() - Duration::from_secs(11);

        // High false negative rate - should lower threshold
        config.adapt_from_metrics(0, 15, 100); // 15% FN rate
        assert!(config.silero_threshold < 0.3);
    }
}
