//! Audio Constants - SINGLE SOURCE OF TRUTH
//!
//! AUTO-GENERATED from shared/audio-constants.json
//! DO NOT EDIT MANUALLY - run: npx tsx generator/generate-audio-constants.ts
//!
//! All audio-related constants MUST be imported from here.
//! DO NOT hardcode sample rates, buffer sizes, etc. anywhere else.

/// Standard sample rate for all audio in the system (Hz)
pub const AUDIO_SAMPLE_RATE: u32 = 16000;

/// Frame size in samples (512 samples = 32ms at 16kHz)
pub const AUDIO_FRAME_SIZE: usize = 512;

/// Frame duration in milliseconds
pub const AUDIO_FRAME_DURATION_MS: u64 = 32;

/// Playback buffer duration in seconds
pub const AUDIO_PLAYBACK_BUFFER_SECONDS: u32 = 2;

/// Audio broadcast channel capacity (number of frames)
pub const AUDIO_CHANNEL_CAPACITY: usize = 2000;

/// Bytes per sample (16-bit PCM = 2 bytes)
pub const BYTES_PER_SAMPLE: usize = 2;

/// WebSocket call server port
pub const CALL_SERVER_PORT: u16 = 50053;
