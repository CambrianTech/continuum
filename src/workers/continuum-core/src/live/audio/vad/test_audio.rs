//! VAD Test Audio Generator
//!
//! Generates realistic synthetic audio patterns for VAD accuracy testing.
//! More sophisticated than simple sine waves - includes formants, harmonics,
//! and time-varying characteristics that resemble real speech and noise.

use rand::Rng;
use serde_json;
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

        for (i, sample_out) in samples.iter_mut().enumerate() {
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
            *sample_out = sample as i16;
        }

        samples
    }

    /// Formant filter (simplified bandpass resonance)
    fn formant_filter(&self, signal: f32, t: f32, center_freq: f32, _bandwidth: f32) -> f32 {
        let phase = 2.0 * PI * center_freq * t;
        let resonance = phase.sin();
        signal * resonance * 0.3 // Reduced amplitude to prevent clipping
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

        for (i, sample_out) in samples.iter_mut().enumerate() {
            let envelope = self.envelope(i, duration_samples);
            // White noise burst
            let noise = rng.gen_range(-1.0..1.0);
            *sample_out = (noise * envelope * 15000.0) as i16;
        }

        samples
    }

    /// Generate fricative sounds (S, SH, F) - sustained noise
    pub fn generate_fricative(&self, duration_samples: usize, freq_center: f32) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        for (i, sample_out) in samples.iter_mut().enumerate() {
            let t = i as f32 / self.sample_rate as f32;
            let envelope = self.envelope(i, duration_samples);

            // Filtered noise (high-pass characteristics)
            let noise = rng.gen_range(-1.0..1.0);
            let carrier = (2.0 * PI * freq_center * t).sin();

            *sample_out = (noise * carrier * envelope * 12000.0) as i16;
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

        for (i, sample_out) in samples.iter_mut().enumerate() {
            let t = i as f32 / self.sample_rate as f32;
            let mut signal = 0.0f32;

            for &freq in &freqs {
                signal += (2.0 * PI * freq * t).sin();
            }

            *sample_out = (signal / 3.0 * 8000.0) as i16;
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

    /// Generate gunfire noise — sharp transient bursts with exponential decay
    ///
    /// Each "shot" is a 2ms white noise burst followed by 100ms exponential decay.
    /// Timing between shots varies ±30% for realism.
    pub fn generate_gunfire(&self, duration_samples: usize, shots_per_second: f32) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        let avg_interval = self.sample_rate as f32 / shots_per_second;
        let burst_len = (self.sample_rate as f32 * 0.002) as usize; // 2ms burst
        let decay_len = (self.sample_rate as f32 * 0.100) as usize; // 100ms decay

        let mut pos = rng.gen_range(0..avg_interval as usize);
        while pos < duration_samples {
            // White noise burst
            for i in 0..burst_len {
                let idx = pos + i;
                if idx < duration_samples {
                    let noise = rng.gen_range(-1.0f32..1.0);
                    samples[idx] = (samples[idx] as f32 + noise * 28000.0)
                        .clamp(-32767.0, 32767.0) as i16;
                }
            }
            // Exponential decay tail
            for i in 0..decay_len {
                let idx = pos + burst_len + i;
                if idx < duration_samples {
                    let noise = rng.gen_range(-1.0f32..1.0);
                    let decay = (-5.0 * i as f32 / decay_len as f32).exp();
                    samples[idx] = (samples[idx] as f32 + noise * decay * 15000.0)
                        .clamp(-32767.0, 32767.0) as i16;
                }
            }

            // Next shot with ±30% jitter
            let jitter = rng.gen_range(0.7..1.3);
            pos += (avg_interval * jitter) as usize;
        }

        samples
    }

    /// Generate explosion — low-frequency boom with debris scatter
    ///
    /// Brown noise sweep from 150Hz→20Hz over 500ms + random high-freq clicks,
    /// all with exponential decay envelope.
    pub fn generate_explosion(&self, duration_samples: usize) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];
        let sweep_len = (self.sample_rate as f32 * 0.5).min(duration_samples as f32) as usize;

        for (i, sample_out) in samples.iter_mut().enumerate() {
            let t = i as f32 / self.sample_rate as f32;
            let pos = i as f32 / duration_samples as f32;

            // Global exponential decay envelope
            let envelope = (-3.0 * pos).exp();

            // Brown noise sweep: frequency decreases from 150Hz to 20Hz
            let sweep_progress = (i as f32 / sweep_len as f32).min(1.0);
            let freq = 150.0 - 130.0 * sweep_progress; // 150 → 20 Hz
            let boom = (2.0 * PI * freq * t).sin();

            // Sub-bass rumble
            let rumble = (2.0 * PI * 25.0 * t).sin() * 0.5;

            // Debris: random high-frequency clicks (3% probability, decaying)
            let debris = if rng.gen_bool((0.03 * envelope as f64).min(0.03)) {
                rng.gen_range(-0.6f32..0.6)
            } else {
                0.0
            };

            let signal = (boom * 0.7 + rumble + debris) * envelope;
            *sample_out = (signal * 25000.0).clamp(-32767.0, 32767.0) as i16;
        }

        samples
    }

    /// Generate emergency siren — alternating two-tone sweep
    ///
    /// Sine sweep between 600Hz↔1200Hz with a 2-second cycle.
    pub fn generate_siren(&self, duration_samples: usize) -> Vec<i16> {
        let mut samples = vec![0i16; duration_samples];
        let cycle_samples = self.sample_rate as f32 * 2.0; // 2s full cycle

        let mut phase = 0.0f32;
        for (i, sample_out) in samples.iter_mut().enumerate() {
            // Triangle LFO between 0 and 1 (2s period)
            let cycle_pos = (i as f32 % cycle_samples) / cycle_samples;
            let lfo = if cycle_pos < 0.5 {
                cycle_pos * 2.0
            } else {
                2.0 - cycle_pos * 2.0
            };

            // Sweep frequency between 600Hz and 1200Hz
            let freq = 600.0 + 600.0 * lfo;
            phase += 2.0 * PI * freq / self.sample_rate as f32;
            if phase > 2.0 * PI { phase -= 2.0 * PI; }

            let signal = phase.sin() * 0.7;
            *sample_out = (signal * 20000.0).clamp(-32767.0, 32767.0) as i16;
        }

        samples
    }

    /// Generate background music — chord progression with triangle waves
    ///
    /// C-Am-F-G loop, each chord 1 second, triangle wave timbre.
    pub fn generate_music(&self, duration_samples: usize) -> Vec<i16> {
        let mut samples = vec![0i16; duration_samples];
        let chord_len = self.sample_rate as usize; // 1 second per chord

        // C-Am-F-G progression (frequencies in Hz)
        let chords: &[&[f32]] = &[
            &[261.6, 329.6, 392.0],  // C major
            &[220.0, 261.6, 329.6],  // A minor
            &[349.2, 440.0, 523.3],  // F major
            &[392.0, 493.9, 587.3],  // G major
        ];

        for (i, sample_out) in samples.iter_mut().enumerate() {
            let chord_idx = (i / chord_len) % chords.len();
            let chord = chords[chord_idx];

            let mut signal = 0.0f32;
            for &freq in chord {
                // Triangle wave: more mellow than sine, musical character
                let phase = (i as f32 * freq / self.sample_rate as f32) % 1.0;
                let tri = if phase < 0.5 {
                    4.0 * phase - 1.0
                } else {
                    3.0 - 4.0 * phase
                };
                signal += tri;
            }

            signal /= chord.len() as f32;
            *sample_out = (signal * 0.3 * 15000.0).clamp(-32767.0, 32767.0) as i16;
        }

        samples
    }

    /// Generate wind noise — filtered noise with slow LFO modulation
    ///
    /// Pink noise modulated by a slow 0.2Hz sine LFO, bandpass ~200-2000Hz effect
    /// achieved via spectral shaping.
    pub fn generate_wind(&self, duration_samples: usize) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        // Simple pink noise approximation using cascaded first-order filters
        let mut b0 = 0.0f32;
        let mut b1 = 0.0f32;
        let mut b2 = 0.0f32;

        for (i, sample_out) in samples.iter_mut().enumerate() {
            let t = i as f32 / self.sample_rate as f32;
            let white = rng.gen_range(-1.0f32..1.0);

            // Paul Kellet's pink noise filter (3-tap approximation)
            b0 = 0.99765 * b0 + white * 0.0990460;
            b1 = 0.96300 * b1 + white * 0.2965164;
            b2 = 0.57000 * b2 + white * 1.0526913;
            let pink = (b0 + b1 + b2 + white * 0.1848) * 0.15;

            // Slow LFO modulation (0.2Hz)
            let lfo = ((2.0 * PI * 0.2 * t).sin() * 0.5 + 0.5) * 0.6 + 0.4;

            // Simple bandpass effect: attenuate DC and very high frequencies
            // by mixing pink (no DC) with the LFO
            let signal = pink * lfo;
            *sample_out = (signal * 12000.0).clamp(-32767.0, 32767.0) as i16;
        }

        samples
    }

    /// Generate rain noise — continuous patter with occasional loud drops
    ///
    /// Pink noise base (0.2 amplitude) + random click bursts
    /// (2% probability, 1-3ms duration) for individual drop impacts.
    pub fn generate_rain(&self, duration_samples: usize) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        // Pink noise state
        let mut b0 = 0.0f32;
        let mut b1 = 0.0f32;
        let mut b2 = 0.0f32;

        let mut drop_remaining = 0usize;

        for (i, sample_out) in samples.iter_mut().enumerate() {
            let _ = i; // suppress unused warning
            let white = rng.gen_range(-1.0f32..1.0);

            // Pink noise base (continuous patter)
            b0 = 0.99765 * b0 + white * 0.0990460;
            b1 = 0.96300 * b1 + white * 0.2965164;
            b2 = 0.57000 * b2 + white * 1.0526913;
            let pink = (b0 + b1 + b2 + white * 0.1848) * 0.15;
            let base = pink * 0.2;

            // Individual raindrop impacts (2% chance of starting a new drop)
            if drop_remaining == 0 && rng.gen_bool(0.02) {
                // 1-3ms drop duration
                let drop_ms = rng.gen_range(1.0..3.0);
                drop_remaining = (self.sample_rate as f32 * drop_ms / 1000.0) as usize;
            }

            let drop = if drop_remaining > 0 {
                drop_remaining -= 1;
                let decay = drop_remaining as f32 / (self.sample_rate as f32 * 0.003);
                rng.gen_range(-0.5f32..0.5) * decay
            } else {
                0.0
            };

            let signal = base + drop;
            *sample_out = (signal * 12000.0).clamp(-32767.0, 32767.0) as i16;
        }

        samples
    }

    /// Generate factory floor noise (periodic machinery + random clanks)
    pub fn generate_factory_floor(&self, duration_samples: usize) -> Vec<i16> {
        let mut rng = rand::thread_rng();
        let mut samples = vec![0i16; duration_samples];

        for (i, sample_out) in samples.iter_mut().enumerate() {
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
            *sample_out = (signal * 8000.0).clamp(-32767.0, 32767.0) as i16;
        }

        samples
    }

    /// Mix two audio signals together with SNR (Signal-to-Noise Ratio) control
    ///
    /// SNR is in decibels (dB):
    /// - 0 dB = equal volume
    /// - +10 dB = signal is 10dB louder than noise
    /// - -10 dB = noise is 10dB louder than signal
    ///
    /// # Arguments
    /// * `signal` - The primary audio (speech)
    /// * `noise` - The background noise
    /// * `snr_db` - Signal-to-noise ratio in decibels
    ///
    /// # Returns
    /// Mixed audio with specified SNR
    pub fn mix_audio_with_snr(signal: &[i16], noise: &[i16], snr_db: f32) -> Vec<i16> {
        assert_eq!(signal.len(), noise.len(), "Signal and noise must be same length");

        // Convert SNR from dB to linear ratio
        // SNR_linear = 10^(SNR_dB / 20)
        let snr_linear = 10_f32.powf(snr_db / 20.0);

        // Calculate RMS (Root Mean Square) of both signals
        let signal_rms = Self::calculate_rms(signal);
        let noise_rms = Self::calculate_rms(noise);

        // Calculate noise scaling factor to achieve desired SNR
        // SNR = signal_rms / (noise_rms * scale)
        // scale = signal_rms / (noise_rms * SNR_linear)
        let noise_scale = if noise_rms > 0.0 {
            signal_rms / (noise_rms * snr_linear)
        } else {
            0.0
        };

        // Mix signals
        signal
            .iter()
            .zip(noise.iter())
            .map(|(&s, &n)| {
                let mixed = s as f32 + (n as f32 * noise_scale);
                mixed.clamp(-32767.0, 32767.0) as i16
            })
            .collect()
    }

    /// Calculate RMS (Root Mean Square) of audio signal
    pub fn calculate_rms(samples: &[i16]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }

        let sum_squares: f64 = samples
            .iter()
            .map(|&s| (s as f64) * (s as f64))
            .sum();

        ((sum_squares / samples.len() as f64).sqrt()) as f32
    }

    /// Generate noise by type — convenience dispatcher
    pub fn generate_noise(&self, noise_type: &NoiseType, duration_samples: usize) -> Vec<i16> {
        match noise_type {
            NoiseType::Crowd(count) => self.generate_crowd(duration_samples, *count),
            NoiseType::FactoryFloor => self.generate_factory_floor(duration_samples),
            NoiseType::Gunfire(shots_per_sec) => self.generate_gunfire(duration_samples, *shots_per_sec),
            NoiseType::Explosion => self.generate_explosion(duration_samples),
            NoiseType::Siren => self.generate_siren(duration_samples),
            NoiseType::Music => self.generate_music(duration_samples),
            NoiseType::Wind => self.generate_wind(duration_samples),
            NoiseType::Rain => self.generate_rain(duration_samples),
            NoiseType::TvDialogue => self.generate_tv_dialogue(duration_samples),
        }
    }
}

/// Background noise types for testing STT robustness
#[derive(Debug, Clone)]
pub enum NoiseType {
    /// Multiple overlapping voices (voice_count)
    Crowd(usize),
    /// Machinery hum + random clanks
    FactoryFloor,
    /// Sharp transient gunshot bursts (shots_per_second)
    Gunfire(f32),
    /// Low-frequency boom + debris scatter
    Explosion,
    /// Alternating two-tone emergency siren
    Siren,
    /// Background chord progression (C-Am-F-G)
    Music,
    /// Filtered noise with slow LFO modulation
    Wind,
    /// Continuous patter + raindrop impacts
    Rain,
    /// Multiple voices + background music
    TvDialogue,
}

impl NoiseType {
    /// Human-readable label for benchmark output
    pub fn label(&self) -> &'static str {
        match self {
            NoiseType::Crowd(_) => "crowd",
            NoiseType::FactoryFloor => "factory",
            NoiseType::Gunfire(_) => "gunfire",
            NoiseType::Explosion => "explosion",
            NoiseType::Siren => "siren",
            NoiseType::Music => "music",
            NoiseType::Wind => "wind",
            NoiseType::Rain => "rain",
            NoiseType::TvDialogue => "tv_dialogue",
        }
    }

    /// Parse from string name (for IPC commands)
    pub fn from_name(name: &str, params: Option<&serde_json::Value>) -> Result<Self, String> {
        match name {
            "crowd" => {
                let count = params
                    .and_then(|p| p.get("voice_count"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as usize;
                Ok(NoiseType::Crowd(count))
            }
            "factory" | "factory_floor" => Ok(NoiseType::FactoryFloor),
            "gunfire" => {
                let sps = params
                    .and_then(|p| p.get("shots_per_second"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(3.0) as f32;
                Ok(NoiseType::Gunfire(sps))
            }
            "explosion" => Ok(NoiseType::Explosion),
            "siren" => Ok(NoiseType::Siren),
            "music" => Ok(NoiseType::Music),
            "wind" => Ok(NoiseType::Wind),
            "rain" => Ok(NoiseType::Rain),
            "tv_dialogue" | "tv" => Ok(NoiseType::TvDialogue),
            _ => Err(format!(
                "Unknown noise type: '{}'. Supported: crowd, factory, gunfire, explosion, siren, music, wind, rain, tv_dialogue",
                name
            )),
        }
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
        use crate::audio_constants::AUDIO_SAMPLE_RATE;
        Self::new(AUDIO_SAMPLE_RATE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_constants::{AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE};

    #[test]
    fn test_formant_speech_generation() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let speech = gen.generate_formant_speech(AUDIO_FRAME_SIZE, Vowel::A);

        assert_eq!(speech.len(), AUDIO_FRAME_SIZE);

        // Check that audio has non-zero content
        let rms: f32 = speech.iter()
            .map(|&s| (s as f32).powi(2))
            .sum::<f32>()
            .sqrt() / speech.len() as f32;

        assert!(rms > 100.0, "Speech should have significant energy");
    }

    #[test]
    fn test_sentence_generation() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let sentence = gen.generate_sentence(3); // 3 words

        // Should generate multiple phonemes
        assert!(sentence.len() > 1000, "Sentence should be substantial");
    }

    #[test]
    fn test_tv_dialogue() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        // Generate 500ms of TV dialogue (sample_rate / 2 samples)
        let duration_samples = AUDIO_SAMPLE_RATE as usize / 2;
        let tv = gen.generate_tv_dialogue(duration_samples);

        assert_eq!(tv.len(), duration_samples);

        // Should be louder than pure silence
        let max_amplitude = tv.iter().map(|&s| s.abs()).max().unwrap();
        assert!(max_amplitude > 1000);
    }

    #[test]
    fn test_gunfire_generation() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let duration = AUDIO_SAMPLE_RATE as usize; // 1 second
        let gunfire = gen.generate_gunfire(duration, 3.0);

        assert_eq!(gunfire.len(), duration);
        let rms = TestAudioGenerator::calculate_rms(&gunfire);
        assert!(rms > 100.0, "Gunfire should have significant energy, got {}", rms);

        // Gunfire should be transient — lots of silence between shots
        let silent = gunfire.iter().filter(|&&s| s.abs() < 100).count();
        assert!(silent > duration / 4, "Gunfire should have quiet gaps between shots");
    }

    #[test]
    fn test_explosion_generation() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let duration = AUDIO_SAMPLE_RATE as usize; // 1 second
        let explosion = gen.generate_explosion(duration);

        assert_eq!(explosion.len(), duration);
        let rms = TestAudioGenerator::calculate_rms(&explosion);
        assert!(rms > 100.0, "Explosion should have significant energy, got {}", rms);

        // Explosion should decay — first quarter should be louder than last quarter
        let first_q = &explosion[..duration / 4];
        let last_q = &explosion[3 * duration / 4..];
        let first_rms = TestAudioGenerator::calculate_rms(first_q);
        let last_rms = TestAudioGenerator::calculate_rms(last_q);
        assert!(first_rms > last_rms * 2.0,
            "Explosion should decay: first_rms={}, last_rms={}", first_rms, last_rms);
    }

    #[test]
    fn test_siren_generation() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let duration = AUDIO_SAMPLE_RATE as usize * 2; // 2 seconds (full cycle)
        let siren = gen.generate_siren(duration);

        assert_eq!(siren.len(), duration);
        let rms = TestAudioGenerator::calculate_rms(&siren);
        assert!(rms > 5000.0, "Siren should be a strong signal, got {}", rms);

        // Siren should have consistent energy throughout (no decay)
        let first_half = &siren[..duration / 2];
        let second_half = &siren[duration / 2..];
        let ratio = TestAudioGenerator::calculate_rms(first_half)
            / TestAudioGenerator::calculate_rms(second_half);
        assert!((ratio - 1.0).abs() < 0.3, "Siren should be steady, ratio={}", ratio);
    }

    #[test]
    fn test_music_generation() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let duration = AUDIO_SAMPLE_RATE as usize * 4; // 4 seconds (full chord cycle)
        let music = gen.generate_music(duration);

        assert_eq!(music.len(), duration);
        let rms = TestAudioGenerator::calculate_rms(&music);
        assert!(rms > 500.0, "Music should have audible energy, got {}", rms);
    }

    #[test]
    fn test_wind_generation() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let duration = AUDIO_SAMPLE_RATE as usize; // 1 second
        let wind = gen.generate_wind(duration);

        assert_eq!(wind.len(), duration);
        let rms = TestAudioGenerator::calculate_rms(&wind);
        assert!(rms > 100.0, "Wind should have energy, got {}", rms);
    }

    #[test]
    fn test_rain_generation() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let duration = AUDIO_SAMPLE_RATE as usize; // 1 second
        let rain = gen.generate_rain(duration);

        assert_eq!(rain.len(), duration);
        let rms = TestAudioGenerator::calculate_rms(&rain);
        assert!(rms > 50.0, "Rain should have energy, got {}", rms);
    }

    #[test]
    fn test_noise_type_dispatcher() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);
        let duration = AUDIO_SAMPLE_RATE as usize / 2; // 500ms

        let types = vec![
            NoiseType::Crowd(5),
            NoiseType::FactoryFloor,
            NoiseType::Gunfire(3.0),
            NoiseType::Explosion,
            NoiseType::Siren,
            NoiseType::Music,
            NoiseType::Wind,
            NoiseType::Rain,
            NoiseType::TvDialogue,
        ];

        for nt in &types {
            let audio = gen.generate_noise(nt, duration);
            assert_eq!(audio.len(), duration, "NoiseType::{:?} wrong length", nt);
            let rms = TestAudioGenerator::calculate_rms(&audio);
            assert!(rms > 10.0, "NoiseType::{:?} should produce non-zero audio, rms={}", nt, rms);
        }
    }

    #[test]
    fn test_noise_type_from_name() {
        assert!(NoiseType::from_name("crowd", None).is_ok());
        assert!(NoiseType::from_name("factory", None).is_ok());
        assert!(NoiseType::from_name("gunfire", None).is_ok());
        assert!(NoiseType::from_name("explosion", None).is_ok());
        assert!(NoiseType::from_name("siren", None).is_ok());
        assert!(NoiseType::from_name("music", None).is_ok());
        assert!(NoiseType::from_name("wind", None).is_ok());
        assert!(NoiseType::from_name("rain", None).is_ok());
        assert!(NoiseType::from_name("tv_dialogue", None).is_ok());
        assert!(NoiseType::from_name("invalid", None).is_err());
    }

    #[test]
    fn test_audio_mixing() {
        let gen = TestAudioGenerator::new(AUDIO_SAMPLE_RATE);

        // Generate signal and noise (same length)
        let signal = gen.generate_formant_speech(240, Vowel::A);
        let noise = gen.generate_factory_floor(240);

        // Mix at different SNR levels
        let mixed_high_snr = TestAudioGenerator::mix_audio_with_snr(&signal, &noise, 20.0); // Signal 20dB louder
        let mixed_equal = TestAudioGenerator::mix_audio_with_snr(&signal, &noise, 0.0);     // Equal volume
        let mixed_low_snr = TestAudioGenerator::mix_audio_with_snr(&signal, &noise, -10.0); // Noise 10dB louder

        assert_eq!(mixed_high_snr.len(), 240);
        assert_eq!(mixed_equal.len(), 240);
        assert_eq!(mixed_low_snr.len(), 240);

        // High SNR should be dominated by signal
        let signal_rms = TestAudioGenerator::calculate_rms(&signal);
        let mixed_high_rms = TestAudioGenerator::calculate_rms(&mixed_high_snr);
        assert!((mixed_high_rms - signal_rms).abs() / signal_rms < 0.2); // Within 20%
    }
}
