//! Continuous Transcription Module
//!
//! Implements streaming transcription with sliding window buffer and partial results.
//! Based on CONTINUOUS-TRANSCRIPTION-ARCHITECTURE.md spec.

mod sliding_buffer;

pub use sliding_buffer::SlidingAudioBuffer;
