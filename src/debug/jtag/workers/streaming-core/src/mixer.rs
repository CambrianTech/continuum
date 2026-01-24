//! Audio Mixer
//!
//! Multi-participant audio mixing with mix-minus support.
//! Each participant hears everyone except themselves.

use crate::handle::Handle;
use crate::vad::{VADFactory, VoiceActivityDetection};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// Audio test utilities for generating synthetic audio
pub mod test_utils {
    use std::f32::consts::PI;

    /// Generate a sine wave at the given frequency
    pub fn generate_sine_wave(frequency: f32, sample_rate: u32, num_samples: usize) -> Vec<i16> {
        (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                let sample = (2.0 * PI * frequency * t).sin();
                // Convert to i16 range (-32768 to 32767)
                (sample * 32767.0) as i16
            })
            .collect()
    }

    /// Generate silence
    pub fn generate_silence(num_samples: usize) -> Vec<i16> {
        vec![0i16; num_samples]
    }

    /// Generate white noise
    pub fn generate_noise(num_samples: usize) -> Vec<i16> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..num_samples)
            .map(|_| rng.gen_range(-16384i16..16384i16))
            .collect()
    }

    /// Generate a click/impulse
    pub fn generate_click(num_samples: usize, click_position: usize) -> Vec<i16> {
        let mut samples = vec![0i16; num_samples];
        if click_position < num_samples {
            samples[click_position] = 32767;
        }
        samples
    }

    /// Calculate RMS (root mean square) of audio samples
    pub fn calculate_rms(samples: &[i16]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_squares: f64 = samples.iter().map(|&s| (s as f64).powi(2)).sum();
        (sum_squares / samples.len() as f64).sqrt() as f32
    }

    /// Check if audio is mostly silence (RMS below threshold)
    pub fn is_silence(samples: &[i16], threshold: f32) -> bool {
        calculate_rms(samples) < threshold
    }

    /// Detect dominant frequency using zero-crossing rate (simple method)
    pub fn detect_frequency_approx(samples: &[i16], sample_rate: u32) -> f32 {
        if samples.len() < 2 {
            return 0.0;
        }

        let mut zero_crossings = 0;
        for i in 1..samples.len() {
            if (samples[i - 1] >= 0 && samples[i] < 0) || (samples[i - 1] < 0 && samples[i] >= 0) {
                zero_crossings += 1;
            }
        }

        // Frequency = zero_crossings / (2 * duration)
        let duration = samples.len() as f32 / sample_rate as f32;
        zero_crossings as f32 / (2.0 * duration)
    }
}

/// Standard frame size (20ms at 16kHz = 320 samples)
pub const FRAME_SIZE: usize = 320;

/// Max speech buffer for transcription (30s at 16kHz) - pre-allocated once
const MAX_SPEECH_SAMPLES: usize = 16000 * 30;

/// Streaming transcription window (3s) - triggers periodic transcription during speech
const STREAMING_WINDOW_SAMPLES: usize = 16000 * 3;

/// Minimum speech samples before transcription (0.5s at 16kHz)
/// Keep this LOW for responsiveness - Whisper adapter will pad if needed
const MIN_SPEECH_SAMPLES: usize = 8000; // 0.5s at 16kHz

/// Silence frames needed to declare speech has ended
/// Research-based threshold: 500-1500ms is industry standard
/// 22 frames * 32ms/frame = 704ms of silence (balanced for natural pauses)
/// This threshold already provides hangover behavior - keeps speech active
/// during natural volume variations before declaring end-of-speech
const SILENCE_THRESHOLD_FRAMES: u32 = 22;

/// Participant audio stream - zero allocations on hot path
pub struct ParticipantStream {
    pub handle: Handle,
    pub user_id: String,
    pub display_name: String,
    /// Current audio frame - FIXED SIZE array, no allocation
    audio_frame: [i16; FRAME_SIZE],
    /// Valid samples in audio_frame (may be < FRAME_SIZE for partial frames)
    frame_len: usize,
    /// Is this participant currently muted?
    pub muted: bool,
    /// Is this an AI participant (no transcription needed - we have their text)?
    pub is_ai: bool,

    // === Voice Activity Detection ===
    /// VAD algorithm (Silero ML or RMS fallback)
    vad: Arc<Box<dyn VoiceActivityDetection>>,

    // === Transcription state (streaming, not batch) ===
    /// Pre-allocated speech ring buffer (fixed capacity, never grows)
    speech_ring: Vec<i16>,
    /// Write position in ring buffer
    speech_write_pos: usize,
    /// How many samples accumulated since last transcription emit
    samples_since_emit: usize,
    /// Consecutive silence frames for end-of-speech
    silence_frames: u32,
    /// Is currently speaking?
    is_speaking: bool,
    /// Min speech to transcribe (0.5s)
    min_speech_samples: usize,
    /// Silence frames to end speech (320ms)
    silence_threshold_frames: u32,
}

/// Result of pushing audio - indicates if speech ended and transcription is ready
#[derive(Debug)]
pub struct PushAudioResult {
    /// Speech ended and buffer is ready for transcription
    pub speech_ended: bool,
    /// Accumulated speech samples (only populated if speech_ended is true)
    pub speech_samples: Option<Vec<i16>>,
}

impl ParticipantStream {
    pub fn new(handle: Handle, user_id: String, display_name: String) -> Self {
        // Pre-allocate speech buffer ONCE at construction - never grows
        let speech_ring = vec![0; MAX_SPEECH_SAMPLES];

        // Create VAD instance (Silero if available, RMS fallback)
        let vad = Arc::new(VADFactory::default());

        Self {
            handle,
            user_id,
            display_name,
            audio_frame: [0i16; FRAME_SIZE],
            frame_len: 0,
            muted: false,
            is_ai: false,
            vad,
            speech_ring,
            speech_write_pos: 0,
            samples_since_emit: 0,
            silence_frames: 0,
            is_speaking: false,
            min_speech_samples: MIN_SPEECH_SAMPLES,
            silence_threshold_frames: SILENCE_THRESHOLD_FRAMES,
        }
    }

    pub fn new_ai(handle: Handle, user_id: String, display_name: String) -> Self {
        // AI participants don't need speech buffer (no transcription)
        // Still need VAD instance (unused but keeps struct consistent)
        let vad = Arc::new(VADFactory::default());

        Self {
            handle,
            user_id,
            display_name,
            audio_frame: [0i16; FRAME_SIZE],
            frame_len: 0,
            muted: false,
            is_ai: true,
            vad,
            speech_ring: Vec::new(), // Empty - AI doesn't need transcription
            speech_write_pos: 0,
            samples_since_emit: 0,
            silence_frames: 0,
            is_speaking: false,
            min_speech_samples: MIN_SPEECH_SAMPLES,
            silence_threshold_frames: SILENCE_THRESHOLD_FRAMES,
        }
    }

    /// Update audio frame with new samples - ZERO ALLOCATION on hot path
    /// Returns PushAudioResult indicating if transcription should run
    ///
    /// Streaming behavior:
    /// - During speech: emits every STREAMING_WINDOW_SAMPLES for partial transcription
    /// - On silence: emits final transcription with is_final=true
    pub fn push_audio(&mut self, samples: Vec<i16>) -> PushAudioResult {
        // [STEP 1] Audio frame received
        // Copy into fixed-size frame (no allocation, just memcpy)
        let copy_len = samples.len().min(FRAME_SIZE);
        self.audio_frame[..copy_len].copy_from_slice(&samples[..copy_len]);
        self.frame_len = copy_len;

        // Skip VAD for AI participants (we already have their text from TTS)
        if self.is_ai || self.muted {
            return PushAudioResult {
                speech_ended: false,
                speech_samples: None,
            };
        }

        // VAD: Use modular VAD system (Silero ML or RMS fallback)
        // NOTE: This blocks briefly (~1ms for Silero, <0.1ms for RMS)
        // Running on audio thread is OK because VAD is designed for real-time
        let vad_result = futures::executor::block_on(self.vad.detect(&samples));

        let is_silence = match vad_result {
            Ok(result) => !result.is_speech, // VAD says no speech = silence
            Err(e) => {
                debug!("VAD error (falling back to RMS): {:?}", e);
                test_utils::is_silence(&samples, 500.0) // Fallback to old RMS
            }
        };

        if is_silence {
            self.silence_frames += 1;

            // If we were speaking and hit silence threshold, speech has ended
            if self.is_speaking && self.silence_frames >= self.silence_threshold_frames {
                self.is_speaking = false;
                info!(
                    "[STEP 3] 🔇 VAD: Speech ENDED for {} ({}ms of speech)",
                    self.display_name,
                    self.samples_since_emit * 1000 / 16000
                );

                // Emit final transcription if we have enough speech
                if self.samples_since_emit >= self.min_speech_samples {
                    let speech = self.extract_speech_buffer();
                    info!(
                        "[STEP 4] 📤 Emitting FINAL transcription ({} samples) for {}",
                        speech.len(),
                        self.display_name
                    );
                    self.samples_since_emit = 0;
                    return PushAudioResult {
                        speech_ended: true,
                        speech_samples: Some(speech),
                    };
                } else {
                    debug!(
                        "[STEP 3] ⏭️ Speech too short ({}ms), discarding",
                        self.samples_since_emit * 1000 / 16000
                    );
                    self.samples_since_emit = 0;
                }
            }
        } else {
            // Speech detected
            if !self.is_speaking {
                info!("[STEP 3] 🎤 VAD: Speech STARTED for {}", self.display_name);
            }
            self.silence_frames = 0;
            self.is_speaking = true;

            // [STEP 2] Write to pre-allocated ring buffer (no allocation)
            self.write_to_ring(&samples);

            // STREAMING: Emit partial transcription every 3 seconds during speech
            if self.samples_since_emit >= STREAMING_WINDOW_SAMPLES {
                let speech = self.extract_speech_buffer();
                info!(
                    "[STEP 4] 📤 Emitting STREAMING transcription ({} samples, 3s chunk) for {}",
                    speech.len(),
                    self.display_name
                );
                self.samples_since_emit = 0;
                return PushAudioResult {
                    speech_ended: false, // Not final - speech continues
                    speech_samples: Some(speech),
                };
            }
        }

        PushAudioResult {
            speech_ended: false,
            speech_samples: None,
        }
    }

    /// Write samples to pre-allocated ring buffer - ZERO ALLOCATION
    fn write_to_ring(&mut self, samples: &[i16]) {
        if self.speech_ring.is_empty() {
            return; // AI participant has no buffer
        }

        for &sample in samples {
            self.speech_ring[self.speech_write_pos] = sample;
            self.speech_write_pos = (self.speech_write_pos + 1) % MAX_SPEECH_SAMPLES;
        }
        self.samples_since_emit += samples.len();
    }

    /// Extract accumulated speech from ring buffer
    /// Returns a new Vec (allocation happens here, but this is off the hot path)
    fn extract_speech_buffer(&mut self) -> Vec<i16> {
        if self.samples_since_emit == 0 {
            return Vec::new();
        }

        // Calculate read position (write_pos - samples_since_emit, wrapped)
        let read_start = if self.speech_write_pos >= self.samples_since_emit {
            self.speech_write_pos - self.samples_since_emit
        } else {
            MAX_SPEECH_SAMPLES - (self.samples_since_emit - self.speech_write_pos)
        };

        // Extract samples
        let mut result = Vec::with_capacity(self.samples_since_emit);
        for i in 0..self.samples_since_emit {
            let idx = (read_start + i) % MAX_SPEECH_SAMPLES;
            result.push(self.speech_ring[idx]);
        }

        result
    }

    /// Get audio samples (returns silence if muted)
    pub fn get_audio(&self) -> &[i16] {
        if self.muted || self.frame_len == 0 {
            &[]
        } else {
            &self.audio_frame[..self.frame_len]
        }
    }

    /// Check if currently speaking (for UI indicators)
    pub fn is_currently_speaking(&self) -> bool {
        self.is_speaking
    }
}

/// Result of pushing audio to mixer - includes participant info if transcription ready
#[derive(Debug)]
pub struct MixerPushResult {
    pub success: bool,
    pub speech_ended: bool,
    pub user_id: Option<String>,
    pub display_name: Option<String>,
    pub speech_samples: Option<Vec<i16>>,
}

/// Audio Mixer for multi-participant calls
pub struct AudioMixer {
    /// All participants in the call
    participants: HashMap<Handle, ParticipantStream>,
    /// Sample rate for mixing
    sample_rate: u32,
    /// Frame size in samples (e.g., 320 for 20ms at 16kHz)
    frame_size: usize,
}

impl AudioMixer {
    /// Create a new mixer
    pub fn new(sample_rate: u32, frame_size: usize) -> Self {
        Self {
            participants: HashMap::new(),
            sample_rate,
            frame_size,
        }
    }

    /// Create mixer with default settings (16kHz, 20ms frames)
    pub fn default_voice() -> Self {
        Self::new(16000, 320) // 16kHz, 20ms = 320 samples
    }

    /// Add a participant
    pub fn add_participant(&mut self, stream: ParticipantStream) {
        self.participants.insert(stream.handle, stream);
    }

    /// Remove a participant
    pub fn remove_participant(&mut self, handle: &Handle) -> Option<ParticipantStream> {
        self.participants.remove(handle)
    }

    /// Get participant by handle
    pub fn get_participant(&self, handle: &Handle) -> Option<&ParticipantStream> {
        self.participants.get(handle)
    }

    /// Get mutable participant by handle
    pub fn get_participant_mut(&mut self, handle: &Handle) -> Option<&mut ParticipantStream> {
        self.participants.get_mut(handle)
    }

    /// Update audio for a participant
    /// Returns MixerPushResult with transcription data if speech ended
    pub fn push_audio(&mut self, handle: &Handle, samples: Vec<i16>) -> MixerPushResult {
        if let Some(participant) = self.participants.get_mut(handle) {
            let result = participant.push_audio(samples);

            if result.speech_ended {
                MixerPushResult {
                    success: true,
                    speech_ended: true,
                    user_id: Some(participant.user_id.clone()),
                    display_name: Some(participant.display_name.clone()),
                    speech_samples: result.speech_samples,
                }
            } else {
                MixerPushResult {
                    success: true,
                    speech_ended: false,
                    user_id: None,
                    display_name: None,
                    speech_samples: None,
                }
            }
        } else {
            MixerPushResult {
                success: false,
                speech_ended: false,
                user_id: None,
                display_name: None,
                speech_samples: None,
            }
        }
    }

    /// Get number of participants
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Mix all participants (sum all streams)
    pub fn mix_all(&self) -> Vec<i16> {
        let mut mixed = vec![0i32; self.frame_size];

        for participant in self.participants.values() {
            let audio = participant.get_audio();
            for (i, &sample) in audio.iter().enumerate() {
                if i < self.frame_size {
                    mixed[i] += sample as i32;
                }
            }
        }

        // Convert back to i16 with clamping
        Self::clamp_to_i16(&mixed)
    }

    /// Mix-minus: mix all participants EXCEPT the one with the given handle
    ///
    /// This is the standard approach for conference calls - each participant
    /// hears everyone except themselves to prevent feedback.
    pub fn mix_minus(&self, exclude_handle: &Handle) -> Vec<i16> {
        let mut mixed = vec![0i32; self.frame_size];

        for (handle, participant) in &self.participants {
            if handle == exclude_handle {
                continue; // Skip the excluded participant
            }

            let audio = participant.get_audio();
            for (i, &sample) in audio.iter().enumerate() {
                if i < self.frame_size {
                    mixed[i] += sample as i32;
                }
            }
        }

        // Convert back to i16 with clamping
        Self::clamp_to_i16(&mixed)
    }

    /// Generate mix-minus for all participants
    /// Returns a map of handle -> mixed audio (what that participant should hear)
    pub fn mix_minus_all(&self) -> HashMap<Handle, Vec<i16>> {
        self.participants
            .keys()
            .map(|handle| (*handle, self.mix_minus(handle)))
            .collect()
    }

    /// Clamp i32 samples to i16 range
    fn clamp_to_i16(samples: &[i32]) -> Vec<i16> {
        samples
            .iter()
            .map(|&s| s.clamp(-32768, 32767) as i16)
            .collect()
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get frame size
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_utils::*;

    #[test]
    fn test_generate_sine_wave() {
        let samples = generate_sine_wave(440.0, 16000, 320);
        assert_eq!(samples.len(), 320);

        // Should not be silence
        assert!(!is_silence(&samples, 100.0));

        // Frequency should be approximately 440Hz
        let detected = detect_frequency_approx(&samples, 16000);
        assert!((detected - 440.0).abs() < 50.0, "Detected: {detected}");
    }

    #[test]
    fn test_generate_silence() {
        let samples = generate_silence(320);
        assert_eq!(samples.len(), 320);
        assert!(is_silence(&samples, 1.0));
    }

    #[test]
    fn test_mixer_add_remove() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());

        mixer.add_participant(stream_a);
        assert_eq!(mixer.participant_count(), 1);

        mixer.remove_participant(&handle_a);
        assert_eq!(mixer.participant_count(), 0);
    }

    #[test]
    fn test_mix_all() {
        let mut mixer = AudioMixer::default_voice();

        // Add two participants with different tones
        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        // Alice plays 440Hz, Bob plays 880Hz
        stream_a.push_audio(generate_sine_wave(440.0, 16000, 320));
        stream_b.push_audio(generate_sine_wave(880.0, 16000, 320));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        // Mix should contain both frequencies (not silence)
        let mixed = mixer.mix_all();
        assert_eq!(mixed.len(), 320);
        assert!(!is_silence(&mixed, 100.0));
    }

    #[test]
    fn test_mix_minus() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();
        let handle_c = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());
        let mut stream_c = ParticipantStream::new(handle_c, "user-c".into(), "Charlie".into());

        // Each plays a different frequency
        stream_a.push_audio(generate_sine_wave(440.0, 16000, 320));
        stream_b.push_audio(generate_sine_wave(880.0, 16000, 320));
        stream_c.push_audio(generate_sine_wave(1320.0, 16000, 320));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);
        mixer.add_participant(stream_c);

        // Alice should hear Bob + Charlie (not herself)
        let mix_for_a = mixer.mix_minus(&handle_a);
        assert!(!is_silence(&mix_for_a, 100.0));

        // Bob should hear Alice + Charlie (not himself)
        let mix_for_b = mixer.mix_minus(&handle_b);
        assert!(!is_silence(&mix_for_b, 100.0));

        // Mix-minus for each should be different from mix_all
        let mix_all = mixer.mix_all();
        assert_ne!(mix_for_a, mix_all);
        assert_ne!(mix_for_b, mix_all);
    }

    #[test]
    fn test_mix_minus_two_participants() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        let audio_a = generate_sine_wave(440.0, 16000, 320);
        let audio_b = generate_sine_wave(880.0, 16000, 320);

        stream_a.push_audio(audio_a.clone());
        stream_b.push_audio(audio_b.clone());

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        // Alice hears only Bob
        let mix_for_a = mixer.mix_minus(&handle_a);
        assert_eq!(mix_for_a, audio_b, "Alice should hear exactly Bob's audio");

        // Bob hears only Alice
        let mix_for_b = mixer.mix_minus(&handle_b);
        assert_eq!(mix_for_b, audio_a, "Bob should hear exactly Alice's audio");
    }

    #[test]
    fn test_muted_participant() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.push_audio(generate_sine_wave(440.0, 16000, 320));
        stream_b.push_audio(generate_sine_wave(880.0, 16000, 320));
        stream_a.muted = true; // Alice is muted

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        // Mix-minus for Bob should be silence (Alice is muted)
        let mix_for_b = mixer.mix_minus(&handle_b);
        assert!(
            is_silence(&mix_for_b, 10.0),
            "Bob should hear silence (Alice muted)"
        );

        // Mix-minus for Alice should be Bob's audio
        let mix_for_a = mixer.mix_minus(&handle_a);
        assert!(!is_silence(&mix_for_a, 100.0), "Alice should hear Bob");
    }

    #[test]
    fn test_ai_participant() {
        let mut mixer = AudioMixer::default_voice();

        let handle_human = Handle::new();
        let handle_ai = Handle::new();

        let mut stream_human =
            ParticipantStream::new(handle_human, "user-human".into(), "Joel".into());
        let mut stream_ai =
            ParticipantStream::new_ai(handle_ai, "ai-helper".into(), "Helper AI".into());

        assert!(!stream_human.is_ai);
        assert!(stream_ai.is_ai);

        // Human speaks
        stream_human.push_audio(generate_sine_wave(440.0, 16000, 320));

        // AI injects TTS audio
        stream_ai.push_audio(generate_sine_wave(220.0, 16000, 320));

        mixer.add_participant(stream_human);
        mixer.add_participant(stream_ai);

        // Both should be in the mix
        let mix_all = mixer.mix_all();
        assert!(!is_silence(&mix_all, 100.0));

        // Human hears AI
        let mix_for_human = mixer.mix_minus(&handle_human);
        assert!(!is_silence(&mix_for_human, 100.0));

        // AI "hears" human (for STT purposes)
        let mix_for_ai = mixer.mix_minus(&handle_ai);
        assert!(!is_silence(&mix_for_ai, 100.0));
    }

    #[test]
    fn test_mix_minus_all() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.push_audio(generate_sine_wave(440.0, 16000, 320));
        stream_b.push_audio(generate_sine_wave(880.0, 16000, 320));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        let all_mixes = mixer.mix_minus_all();
        assert_eq!(all_mixes.len(), 2);
        assert!(all_mixes.contains_key(&handle_a));
        assert!(all_mixes.contains_key(&handle_b));
    }

    #[test]
    fn test_clipping_prevention() {
        let mut mixer = AudioMixer::default_voice();

        // Add many loud participants
        for i in 0..10 {
            let handle = Handle::new();
            let mut stream =
                ParticipantStream::new(handle, format!("user-{i}"), format!("User {i}"));
            // Max amplitude sine wave
            stream.push_audio(generate_sine_wave(440.0 + (i as f32 * 100.0), 16000, 320));
            mixer.add_participant(stream);
        }

        // Mix should not overflow - if we get here without panic, clamping worked
        let mixed = mixer.mix_all();
        assert_eq!(mixed.len(), 320);
        // Values are already i16 so they're in valid range by type constraints
        // The real test is that clamp_to_i16 prevents overflow during mixing
    }
}
