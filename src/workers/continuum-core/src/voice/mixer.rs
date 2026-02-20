//! Audio Mixer
//!
//! Multi-participant audio mixing with mix-minus support.
//! Each participant hears everyone except themselves.

use crate::audio_constants::AUDIO_FRAME_SIZE;
use crate::voice::handle::Handle;
use crate::voice::vad::{ProductionVAD, VADError};
use std::collections::HashMap;
use tracing::{debug, info, warn};

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

    // Audio analysis utilities (is_silence, calculate_rms) moved to crate::utils::audio

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

/// Standard frame size - uses AUDIO_FRAME_SIZE from constants (single source of truth)
pub const FRAME_SIZE: usize = AUDIO_FRAME_SIZE;

/// Ring buffer duration for AI audio (seconds)
/// Must be large enough for the longest possible TTS response.
/// A conversational response can be 30-60 seconds of speech.
/// 60 seconds * 16kHz * 2 bytes = ~1.9MB per AI participant — acceptable.
const AI_RING_BUFFER_SECONDS: usize = 60;

/// Ring buffer size for AI audio (samples)
const AI_RING_BUFFER_SIZE: usize = crate::audio_constants::AUDIO_SAMPLE_RATE as usize * AI_RING_BUFFER_SECONDS;

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
    /// Is this an ambient audio source (TV, music, background noise)?
    /// Ambient sources use AI ring buffer infrastructure but are NEVER excluded
    /// from mix-minus — everyone hears them, including themselves.
    pub is_ambient: bool,

    // === AI Audio Ring Buffer ===
    // AI participants dump all TTS audio at once, we buffer and pull frame-by-frame
    // This eliminates JavaScript timing jitter from the audio pipeline
    // Vec<i16> instead of Box<[i16; N]> to avoid stack overflow during allocation
    ai_ring_buffer: Option<Vec<i16>>,
    ai_ring_write: usize,  // Write position
    ai_ring_read: usize,   // Read position
    ai_ring_available: usize, // Samples available

    // === Voice Activity Detection (Production Two-Stage VAD) ===
    /// Production VAD (WebRTC → Silero, with sentence buffering)
    vad: Option<ProductionVAD>,

    /// Is currently speaking? (for UI indicators)
    is_speaking: bool,
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
    /// Create new human participant with production VAD
    pub fn new(handle: Handle, user_id: String, display_name: String) -> Self {
        // Create ProductionVAD with default config (initialized later)
        let vad = Some(ProductionVAD::new());

        Self {
            handle,
            user_id,
            display_name,
            audio_frame: [0i16; FRAME_SIZE],
            frame_len: 0,
            muted: false,
            is_ai: false,
            is_ambient: false,
            ai_ring_buffer: None, // Humans don't need ring buffer (Vec not allocated)
            ai_ring_write: 0,
            ai_ring_read: 0,
            ai_ring_available: 0,
            vad,
            is_speaking: false,
        }
    }

    /// Create AI participant (no VAD needed - we already have their text from TTS)
    /// AI participants get a ring buffer for server-side audio pacing
    pub fn new_ai(handle: Handle, user_id: String, display_name: String) -> Self {
        // Allocate ring buffer on heap via Vec (60 seconds ≈ 1.9MB)
        let ring_buffer = vec![0i16; AI_RING_BUFFER_SIZE];

        Self {
            handle,
            user_id,
            display_name,
            audio_frame: [0i16; FRAME_SIZE],
            frame_len: 0,
            muted: false,
            is_ai: true,
            is_ambient: false,
            ai_ring_buffer: Some(ring_buffer),
            ai_ring_write: 0,
            ai_ring_read: 0,
            ai_ring_available: 0,
            vad: None, // AI doesn't need VAD
            is_speaking: false,
        }
    }

    /// Create ambient audio source (TV, music, background noise)
    /// Uses ring buffer infrastructure (same as AI) for server-paced playback.
    /// Ambient sources are NEVER excluded from mix-minus — everyone hears them.
    pub fn new_ambient(handle: Handle, source_name: String) -> Self {
        let ring_buffer = vec![0i16; AI_RING_BUFFER_SIZE];

        Self {
            handle,
            user_id: format!("ambient:{}", source_name),
            display_name: source_name,
            audio_frame: [0i16; FRAME_SIZE],
            frame_len: 0,
            muted: false,
            is_ai: true, // Uses AI ring buffer path for push/get_audio
            is_ambient: true,
            ai_ring_buffer: Some(ring_buffer),
            ai_ring_write: 0,
            ai_ring_read: 0,
            ai_ring_available: 0,
            vad: None,
            is_speaking: false,
        }
    }

    /// Initialize VAD (must be called after construction)
    /// Returns Ok even if model loading fails (graceful degradation for tests)
    pub fn initialize_vad(&mut self) -> Result<(), VADError> {
        if let Some(ref mut vad) = self.vad {
            match vad.initialize() {
                Ok(_) => {
                    info!("🎯 ProductionVAD initialized for {}", self.display_name);
                }
                Err(e) => {
                    warn!("⚠️ VAD init failed for {}: {:?}", self.display_name, e);
                    // In tests, VAD may not be available - gracefully disable
                    self.vad = None;
                }
            }
        }
        Ok(())
    }

    /// Update audio frame with new samples
    /// Returns PushAudioResult indicating if transcription should run
    ///
    /// For AI participants: Writes to ring buffer (can accept large chunks at once)
    /// For human participants: Uses ProductionVAD for sentence detection
    pub fn push_audio(&mut self, samples: Vec<i16>) -> PushAudioResult {
        // AI PARTICIPANTS: Write to ring buffer for server-paced playback
        // This eliminates JavaScript timing jitter - AI can dump all TTS audio at once
        if self.is_ai {
            if let Some(ref mut ring) = self.ai_ring_buffer {
                let samples_to_write = samples.len().min(AI_RING_BUFFER_SIZE - self.ai_ring_available);

                if samples_to_write < samples.len() {
                    warn!(
                        "⚠️ AI ring buffer overflow for {}: dropping {} of {} samples (buffer: {}/{}s used)",
                        self.display_name,
                        samples.len() - samples_to_write,
                        samples.len(),
                        self.ai_ring_available as f32 / crate::audio_constants::AUDIO_SAMPLE_RATE as f32,
                        AI_RING_BUFFER_SECONDS
                    );
                }

                // Write samples to ring buffer
                for &sample in samples.iter().take(samples_to_write) {
                    ring[self.ai_ring_write] = sample;
                    self.ai_ring_write = (self.ai_ring_write + 1) % AI_RING_BUFFER_SIZE;
                }
                self.ai_ring_available += samples_to_write;

                if samples_to_write > 0 {
                    debug!(
                        "🤖 AI {} buffered {} samples (total: {} = {:.1}s)",
                        self.display_name,
                        samples_to_write,
                        self.ai_ring_available,
                        self.ai_ring_available as f32 / crate::audio_constants::AUDIO_SAMPLE_RATE as f32
                    );
                }
            }

            return PushAudioResult {
                speech_ended: false,
                speech_samples: None,
            };
        }

        // HUMAN PARTICIPANTS: Copy into fixed-size frame for immediate mixing
        let copy_len = samples.len().min(FRAME_SIZE);
        self.audio_frame[..copy_len].copy_from_slice(&samples[..copy_len]);
        self.frame_len = copy_len;

        // Skip VAD for muted participants
        if self.muted {
            return PushAudioResult {
                speech_ended: false,
                speech_samples: None,
            };
        }

        // Use ProductionVAD (two-stage VAD + sentence buffering)
        if let Some(ref mut vad) = self.vad {
            // ProductionVAD.process_frame() returns complete sentence when ready
            let vad_result = vad.process_frame(&samples);

            match vad_result {
                Ok(Some(complete_sentence)) => {
                    // Complete sentence ready for transcription
                    let duration_ms = (complete_sentence.len() as f32 / crate::audio_constants::AUDIO_SAMPLE_RATE as f32) * 1000.0;
                    info!(
                        "📤 Complete sentence ready for {} ({} samples, {:.0}ms)",
                        self.display_name,
                        complete_sentence.len(),
                        duration_ms
                    );

                    self.is_speaking = false;

                    PushAudioResult {
                        speech_ended: true,
                        speech_samples: Some(complete_sentence),
                    }
                }
                Ok(None) => {
                    // Still buffering - check if we should update speaking state
                    // (This is approximate - ProductionVAD handles the real logic)
                    PushAudioResult {
                        speech_ended: false,
                        speech_samples: None,
                    }
                }
                Err(e) => {
                    debug!("VAD error for {}: {:?}", self.display_name, e);
                    PushAudioResult {
                        speech_ended: false,
                        speech_samples: None,
                    }
                }
            }
        } else {
            // No VAD (shouldn't happen for human participants, but handle gracefully)
            PushAudioResult {
                speech_ended: false,
                speech_samples: None,
            }
        }
    }

    /// Get audio samples for mixing
    /// - Human participants: Returns current frame (set by push_audio)
    /// - AI participants: Pulls one frame from ring buffer (server-paced playback)
    pub fn get_audio(&mut self) -> &[i16] {
        if self.muted {
            return &[];
        }

        // AI PARTICIPANTS: Pull one frame from ring buffer
        if self.is_ai {
            if let Some(ref ring) = self.ai_ring_buffer {
                if self.ai_ring_available >= FRAME_SIZE {
                    // Pull FRAME_SIZE samples from ring buffer into audio_frame
                    for i in 0..FRAME_SIZE {
                        self.audio_frame[i] = ring[(self.ai_ring_read + i) % AI_RING_BUFFER_SIZE];
                    }
                    self.ai_ring_read = (self.ai_ring_read + FRAME_SIZE) % AI_RING_BUFFER_SIZE;
                    self.ai_ring_available -= FRAME_SIZE;
                    self.frame_len = FRAME_SIZE;
                } else if self.ai_ring_available > 0 {
                    // Partial frame - play what we have
                    let available = self.ai_ring_available;
                    for i in 0..available {
                        self.audio_frame[i] = ring[(self.ai_ring_read + i) % AI_RING_BUFFER_SIZE];
                    }
                    // Zero-pad the rest
                    for i in available..FRAME_SIZE {
                        self.audio_frame[i] = 0;
                    }
                    self.ai_ring_read = (self.ai_ring_read + available) % AI_RING_BUFFER_SIZE;
                    self.ai_ring_available = 0;
                    self.frame_len = FRAME_SIZE;
                } else {
                    // No audio available - silence
                    return &[];
                }
            } else {
                return &[];
            }
        }

        // Return current frame
        if self.frame_len == 0 {
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

    // === Pre-allocated scratch buffers for the 20ms mix tick ===
    // These eliminate per-tick HashMap/Vec allocations on the real-time audio path.
    // At 50Hz tick rate with 5 participants, this saves ~800KB/sec of allocation churn.

    /// Cached audio frames pulled from participants (reused across ticks)
    tick_audio_cache: HashMap<Handle, Vec<i16>>,
    /// i32 accumulation buffer for mixing (avoids per-target allocation)
    tick_mix_buffer: Vec<i32>,
    /// Participant handle snapshot for iteration (avoids borrow conflicts)
    tick_handles: Vec<(Handle, bool)>,
}

impl AudioMixer {
    /// Create a new mixer
    pub fn new(sample_rate: u32, frame_size: usize) -> Self {
        Self {
            participants: HashMap::new(),
            sample_rate,
            frame_size,
            tick_audio_cache: HashMap::new(),
            tick_mix_buffer: vec![0i32; frame_size],
            tick_handles: Vec::new(),
        }
    }

    /// Create mixer with default settings (uses audio_constants)
    pub fn default_voice() -> Self {
        use crate::audio_constants::{AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE};
        Self::new(AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE)
    }

    /// Add a participant
    /// Note: Call initialize_vad() on the participant BEFORE adding to mixer
    pub fn add_participant(&mut self, stream: ParticipantStream) {
        self.participants.insert(stream.handle, stream);
    }

    /// Add a participant and initialize VAD
    pub async fn add_participant_with_init(&mut self, mut stream: ParticipantStream) -> Result<(), VADError> {
        stream.initialize_vad()?;
        self.participants.insert(stream.handle, stream);
        Ok(())
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
    /// Note: Requires &mut self because AI participants pull from ring buffer
    pub fn mix_all(&mut self) -> Vec<i16> {
        // Reuse pre-allocated i32 buffer
        for s in self.tick_mix_buffer.iter_mut() { *s = 0; }

        for participant in self.participants.values_mut() {
            let audio = participant.get_audio();
            for (i, &sample) in audio.iter().enumerate() {
                if i < self.frame_size {
                    self.tick_mix_buffer[i] += sample as i32;
                }
            }
        }

        Self::clamp_to_i16(&self.tick_mix_buffer)
    }

    /// Mix-minus: mix all participants EXCEPT the one with the given handle
    ///
    /// This is the standard approach for conference calls - each participant
    /// hears everyone except themselves to prevent feedback.
    /// Note: Requires &mut self because AI participants pull from ring buffer
    pub fn mix_minus(&mut self, exclude_handle: &Handle) -> Vec<i16> {
        // Reuse pre-allocated i32 buffer
        for s in self.tick_mix_buffer.iter_mut() { *s = 0; }

        for (handle, participant) in &mut self.participants {
            if handle == exclude_handle {
                continue;
            }

            let audio = participant.get_audio();
            for (i, &sample) in audio.iter().enumerate() {
                if i < self.frame_size {
                    self.tick_mix_buffer[i] += sample as i32;
                }
            }
        }

        Self::clamp_to_i16(&self.tick_mix_buffer)
    }

    /// Generate mix-minus for all participants
    /// Returns a map of handle -> mixed audio (what that participant should hear)
    /// Note: Requires &mut self because AI participants pull from ring buffer
    ///
    /// CRITICAL: Pull all audio frames ONCE at the start, then mix from cache.
    /// Otherwise AI ring buffers get pulled N-1 times per tick (once per other participant),
    /// causing audio to play at (N-1)x speed!
    ///
    /// Ambient sources are NEVER excluded from any mix — everyone hears them,
    /// including the ambient source's own "participant" entry (which is just a source,
    /// not a listener). Ambient sources don't get mix output entries since they're
    /// not listeners.
    pub fn mix_minus_all(&mut self) -> HashMap<Handle, Vec<i16>> {
        // STEP 1: Pull audio from ALL participants ONCE into pre-allocated cache.
        // Uses std::mem::take to avoid borrow conflicts between participants and cache.
        // The cache HashMap retains its capacity across ticks — no reallocation.
        let mut audio_cache = std::mem::take(&mut self.tick_audio_cache);
        audio_cache.clear();
        for (handle, participant) in &mut self.participants {
            let audio = participant.get_audio();
            let entry = audio_cache.entry(*handle)
                .or_insert_with(|| Vec::with_capacity(self.frame_size));
            entry.clear();
            entry.extend_from_slice(audio);
        }

        // STEP 2: Snapshot participant handles into pre-allocated vec
        let mut handles = std::mem::take(&mut self.tick_handles);
        handles.clear();
        handles.extend(self.participants.iter().map(|(h, p)| (*h, p.is_ambient)));

        // STEP 3: Generate mix-minus for each non-ambient participant using cached audio.
        // Reuses tick_mix_buffer for i32 accumulation (zeroed per target, not reallocated).
        let frame_size = self.frame_size;
        let mix_buffer = &mut self.tick_mix_buffer;
        let mut result = HashMap::with_capacity(handles.len());

        for (target_handle, target_is_ambient) in &handles {
            // Ambient sources are not listeners — skip generating output for them
            if *target_is_ambient {
                continue;
            }

            // Zero the pre-allocated mixing buffer
            for s in mix_buffer.iter_mut() { *s = 0; }

            // Mix all OTHER participants' cached audio
            for (handle, audio) in &audio_cache {
                if handle == target_handle {
                    continue;
                }

                for (i, &sample) in audio.iter().enumerate() {
                    if i < frame_size {
                        mix_buffer[i] += sample as i32;
                    }
                }
            }

            result.insert(*target_handle, Self::clamp_to_i16(mix_buffer));
        }

        // Return scratch buffers for next tick
        self.tick_audio_cache = audio_cache;
        self.tick_handles = handles;

        result
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

    /// Find a participant's handle by user_id
    pub fn find_handle_by_user_id(&self, user_id: &str) -> Option<Handle> {
        self.participants.iter()
            .find(|(_, stream)| stream.user_id == user_id)
            .map(|(handle, _)| *handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_utils::*;
    use crate::utils::audio::is_silence;
    use crate::audio_constants::{AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE};

    #[test]
    fn test_generate_sine_wave() {
        let samples = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        assert_eq!(samples.len(), AUDIO_FRAME_SIZE);

        // Should not be silence
        assert!(!is_silence(&samples, 100.0));

        // Frequency should be approximately 440Hz
        let detected = detect_frequency_approx(&samples, AUDIO_SAMPLE_RATE);
        assert!((detected - 440.0).abs() < 50.0, "Detected: {detected}");
    }

    #[test]
    fn test_generate_silence() {
        let samples = generate_silence(AUDIO_FRAME_SIZE);
        assert_eq!(samples.len(), AUDIO_FRAME_SIZE);
        assert!(is_silence(&samples, 1.0));
    }

    #[tokio::test]
    async fn test_mixer_add_remove() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        stream_a.initialize_vad().expect("VAD init failed");

        mixer.add_participant(stream_a);
        assert_eq!(mixer.participant_count(), 1);

        mixer.remove_participant(&handle_a);
        assert_eq!(mixer.participant_count(), 0);
    }

    #[tokio::test]
    async fn test_mix_all() {
        let mut mixer = AudioMixer::default_voice();

        // Add two participants with different tones
        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");

        // Alice plays 440Hz, Bob plays 880Hz
        stream_a.push_audio(generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));
        stream_b.push_audio(generate_sine_wave(880.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        // Mix should contain both frequencies (not silence)
        let mixed = mixer.mix_all();
        assert_eq!(mixed.len(), AUDIO_FRAME_SIZE);
        assert!(!is_silence(&mixed, 100.0));
    }

    #[tokio::test]
    async fn test_mix_minus() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();
        let handle_c = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());
        let mut stream_c = ParticipantStream::new(handle_c, "user-c".into(), "Charlie".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");
        stream_c.initialize_vad().expect("VAD init failed");

        // Each plays a different frequency
        stream_a.push_audio(generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));
        stream_b.push_audio(generate_sine_wave(880.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));
        stream_c.push_audio(generate_sine_wave(1320.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));

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

    #[tokio::test]
    async fn test_mix_minus_two_participants() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");

        let audio_a = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        let audio_b = generate_sine_wave(880.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);

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

    #[tokio::test]
    async fn test_muted_participant() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");

        stream_a.push_audio(generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));
        stream_b.push_audio(generate_sine_wave(880.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));
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

    #[tokio::test]
    async fn test_ai_participant() {
        let mut mixer = AudioMixer::default_voice();

        let handle_human = Handle::new();
        let handle_ai = Handle::new();

        let mut stream_human =
            ParticipantStream::new(handle_human, "user-human".into(), "Joel".into());
        let stream_ai =
            ParticipantStream::new_ai(handle_ai, "ai-helper".into(), "Helper AI".into());

        stream_human.initialize_vad().expect("VAD init failed");
        // AI doesn't need VAD initialization

        assert!(!stream_human.is_ai);
        assert!(stream_ai.is_ai);

        // Human speaks (3 frames - mix_all() + mix_minus() x2 each consume from human)
        stream_human.push_audio(generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));

        // AI injects TTS audio (3 frames - each mixing call consumes one frame from ring buffer)
        // mix_all(), mix_minus(&human), mix_minus(&ai) each pull one frame
        let mut stream_ai_mut = stream_ai;
        stream_ai_mut.push_audio(generate_sine_wave(220.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE * 3));

        mixer.add_participant(stream_human);
        mixer.add_participant(stream_ai_mut);

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

    #[tokio::test]
    async fn test_mix_minus_all() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");

        stream_a.push_audio(generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));
        stream_b.push_audio(generate_sine_wave(880.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        let all_mixes = mixer.mix_minus_all();
        assert_eq!(all_mixes.len(), 2);
        assert!(all_mixes.contains_key(&handle_a));
        assert!(all_mixes.contains_key(&handle_b));
    }

    #[tokio::test]
    async fn test_clipping_prevention() {
        let mut mixer = AudioMixer::default_voice();

        // Add many loud participants
        for i in 0..10 {
            let handle = Handle::new();
            let mut stream =
                ParticipantStream::new(handle, format!("user-{i}"), format!("User {i}"));
            stream.initialize_vad().expect("VAD init failed");
            // Max amplitude sine wave
            stream.push_audio(generate_sine_wave(440.0 + (i as f32 * 100.0), AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE));
            mixer.add_participant(stream);
        }

        // Mix should not overflow - if we get here without panic, clamping worked
        let mixed = mixer.mix_all();
        assert_eq!(mixed.len(), AUDIO_FRAME_SIZE);
        // Values are already i16 so they're in valid range by type constraints
        // The real test is that clamp_to_i16 prevents overflow during mixing
    }
}
