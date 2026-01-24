//! VAD Test Audio Generator
//!
//! Generates realistic synthetic audio patterns for VAD accuracy testing.
//! More sophisticated than simple sine waves - includes formants, harmonics,
//! and time-varying characteristics that resemble real speech and noise.

use rand::Rng;
use std::f32::consts::PI;

/// Test audio generator for VAD evaluation
pub struct TestAudioGenerator {
    sample_rate: u32,
}

impl TestAudioGenerator {
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate }
    }

    /// Generate formant-based speech synthesis (more realistic than sine waves)
    ///
    /// Speech has 3-5 formants (resonant frequencies) that give it characteristic timbre.
    /// This creates a vowel-like sound with proper formant structure.
    pub fn generate_formant_speech(&self, duration_samples: usize, vowel: Vowel) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        let (f1, f2, f3) = vowel.formants();
        let fundamental = 150.0; // Typical male voice fundamental frequency

        for i in 0..duration_samples {
            let t = i as f32 / self.sample_rate as f32;

            // Fundamental + harmonics (pitch)
            let mut signal = 0.0f32;
            for harmonic in 1..=10 {
                let freq = fundamental * harmonic as f32;
                let amp = 1.0 / harmonic as f32; // Harmonics decay
                signal += amp * (2.0 * PI * freq * t).sin();
            }

            // Apply formant resonances (amplitude modulation)
            let formant_envelope =
                self.formant_filter(signal, t, f1, 90.0) +
                self.formant_filter(signal, t, f2, 110.0) +
                self.formant_filter(signal, t, f3, 170.0);

            // Add natural variation (shimmer/jitter)
            let variation = 1.0 + rng.gen_range(-0.05..0.05);

            // Amplitude envelope (slight fade in/out)
            let envelope = self.envelope(i, duration_samples);

            let sample = (formant_envelope * variation * envelope * 10000.0).clamp(-32767.0, 32767.0);
            samples[i] = sample as i16;
        }

        samples
    }

    /// Formant filter (simplified bandpass resonance)
    fn formant_filter(&self, signal: f32, t: f32, center_freq: f32, bandwidth: f32) -> f32 {
        let phase = 2.0 * PI * center_freq * t;
        let resonance = phase.sin() * (-bandwidth * t).exp();
        signal * resonance
    }

    /// Amplitude envelope (attack-sustain-release)
    fn envelope(&self, sample_idx: usize, total_samples: usize) -> f32 {
        let pos = sample_idx as f32 / total_samples as f32;

        if pos < 0.05 {
            // Attack (0-5%)
            pos / 0.05
        } else if pos > 0.95 {
            // Release (95-100%)
            (1.0 - pos) / 0.05
        } else {
            // Sustain
            1.0
        }
    }

    /// Generate plosive sounds (P, T, K) - burst of noise
    pub fn generate_plosive(&self, duration_samples: usize) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        for i in 0..duration_samples {
            let envelope = self.envelope(i, duration_samples);
            // White noise burst
            let noise = rng.gen_range(-1.0..1.0);
            samples[i] = (noise * envelope * 15000.0) as i16;
        }

        samples
    }

    /// Generate fricative sounds (S, SH, F) - sustained noise
    pub fn generate_fricative(&self, duration_samples: usize, freq_center: f32) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        for i in 0..duration_samples {
            let t = i as f32 / self.sample_rate as f32;
            let envelope = self.envelope(i, duration_samples);

            // Filtered noise (high-pass characteristics)
            let noise = rng.gen_range(-1.0..1.0);
            let carrier = (2.0 * PI * freq_center * t).sin();

            samples[i] = (noise * carrier * envelope * 12000.0) as i16;
        }

        samples
    }

    /// Generate realistic sentence (sequence of phonemes)
    pub fn generate_sentence(&self, word_count: usize) -> Vec<i16> {
        let mut sentence = Vec::new();
        let mut rng = rand::thread_rng();

        for _ in 0..word_count {
            // Consonant-Vowel-Consonant structure

            // Initial consonant (plosive or fricative)
            if rng.gen_bool(0.5) {
                sentence.extend(self.generate_plosive(320)); // 20ms
            } else {
                sentence.extend(self.generate_fricative(480, 4000.0)); // 30ms
            }

            // Vowel (random formant pattern)
            let vowel = match rng.gen_range(0..5) {
                0 => Vowel::A,
                1 => Vowel::E,
                2 => Vowel::I,
                3 => Vowel::O,
                _ => Vowel::U,
            };
            sentence.extend(self.generate_formant_speech(1600, vowel)); // 100ms

            // Final consonant
            if rng.gen_bool(0.6) {
                sentence.extend(self.generate_fricative(640, 5000.0)); // 40ms
            }

            // Word gap (silence)
            sentence.extend(vec![0i16; 800]); // 50ms
        }

        sentence
    }

    /// Generate TV dialogue simulation (multiple overlapping voices + music)
    pub fn generate_tv_dialogue(&self, duration_samples: usize) -> Vec<i16> {
        let mut samples = vec![0i16; duration_samples];

        // Background music (sustained tones)
        let music = self.generate_music_chord(duration_samples);

        // Main voice (loud)
        let voice1 = self.generate_sentence(3);

        // Background voice (quieter)
        let voice2 = self.generate_sentence(2);

        // Mix all components
        for i in 0..duration_samples {
            let mut mixed = 0i32;

            // Music (40% volume)
            if i < music.len() {
                mixed += (music[i] as i32 * 40) / 100;
            }

            // Voice 1 (70% volume)
            if i < voice1.len() {
                mixed += (voice1[i] as i32 * 70) / 100;
            }

            // Voice 2 (30% volume, delayed)
            let v2_start = duration_samples / 4;
            if i >= v2_start && i - v2_start < voice2.len() {
                mixed += (voice2[i - v2_start] as i32 * 30) / 100;
            }

            samples[i] = mixed.clamp(-32767, 32767) as i16;
        }

        samples
    }

    /// Generate music chord (harmonic series)
    fn generate_music_chord(&self, duration_samples: usize) -> Vec<i16> {
        let mut samples = vec![0i16; duration_samples];

        // C major chord: C (261Hz), E (329Hz), G (392Hz)
        let freqs = [261.0, 329.0, 392.0];

        for i in 0..duration_samples {
            let t = i as f32 / self.sample_rate as f32;
            let mut signal = 0.0f32;

            for &freq in &freqs {
                signal += (2.0 * PI * freq * t).sin();
            }

            samples[i] = (signal / 3.0 * 8000.0) as i16;
        }

        samples
    }

    /// Generate crowd noise (many overlapping voices)
    pub fn generate_crowd(&self, duration_samples: usize, voice_count: usize) -> Vec<i16> {
        let mut samples = vec![0i32; duration_samples];
        let mut rng = rand::thread_rng();

        for _ in 0..voice_count {
            // Random voice with random timing
            let start_offset = rng.gen_range(0..duration_samples / 2);
            let voice = self.generate_sentence(2);

            for (i, &sample) in voice.iter().enumerate() {
                let idx = start_offset + i;
                if idx < duration_samples {
                    samples[idx] += sample as i32 / voice_count as i32;
                }
            }
        }

        samples.iter().map(|&s| s.clamp(-32767, 32767) as i16).collect()
    }

    /// Generate factory floor noise (periodic machinery + random clanks)
    pub fn generate_factory_floor(&self, duration_samples: usize) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        for i in 0..duration_samples {
            let t = i as f32 / self.sample_rate as f32;

            // Base hum (60Hz electrical + 120Hz harmonic)
            let hum =
                (2.0 * PI * 60.0 * t).sin() * 0.3 +
                (2.0 * PI * 120.0 * t).sin() * 0.2;

            // Machinery rumble (low frequency)
            let rumble = (2.0 * PI * 30.0 * t).sin() * 0.4;

            // Random clanks (1% probability per sample)
            let clank = if rng.gen_bool(0.01) {
                rng.gen_range(-0.8..0.8)
            } else {
                0.0
            };

            let signal = hum + rumble + clank;
            samples[i] = (signal * 8000.0).clamp(-32767.0, 32767.0) as i16;
        }

        samples
    }
}

/// Vowel formants (F1, F2, F3 in Hz)
#[derive(Debug, Clone, Copy)]
pub enum Vowel {
    A, // "ah" - open vowel
    E, // "eh" - mid vowel
    I, // "ee" - close front vowel
    O, // "oh" - close back vowel
    U, // "oo" - very close back vowel
}

impl Vowel {
    /// Get formant frequencies (F1, F2, F3)
    fn formants(&self) -> (f32, f32, f32) {
        match self {
            Vowel::A => (730.0, 1090.0, 2440.0),  // "ah"
            Vowel::E => (530.0, 1840.0, 2480.0),  // "eh"
            Vowel::I => (270.0, 2290.0, 3010.0),  // "ee"
            Vowel::O => (570.0, 840.0, 2410.0),   // "oh"
            Vowel::U => (300.0, 870.0, 2240.0),   // "oo"
        }
    }
}

impl Default for TestAudioGenerator {
    fn default() -> Self {
        Self::new(16000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formant_speech_generation() {
        let gen = TestAudioGenerator::new(16000);
        let speech = gen.generate_formant_speech(512, Vowel::A);

        assert_eq!(speech.len(), 512);

        // Check that audio has non-zero content
        let rms: f32 = speech.iter()
            .map(|&s| (s as f32).powi(2))
            .sum::<f32>()
            .sqrt() / speech.len() as f32;

        assert!(rms > 100.0, "Speech should have significant energy");
    }

    #[test]
    fn test_sentence_generation() {
        let gen = TestAudioGenerator::new(16000);
        let sentence = gen.generate_sentence(3); // 3 words

        // Should generate multiple phonemes
        assert!(sentence.len() > 1000, "Sentence should be substantial");
    }

    #[test]
    fn test_tv_dialogue() {
        let gen = TestAudioGenerator::new(16000);
        let tv = gen.generate_tv_dialogue(8000); // 500ms

        assert_eq!(tv.len(), 8000);

        // Should be louder than pure silence
        let max_amplitude = tv.iter().map(|&s| s.abs()).max().unwrap();
        assert!(max_amplitude > 1000);
    }
}
