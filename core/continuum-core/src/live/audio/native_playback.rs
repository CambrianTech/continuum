//! Incremental native PCM decoding at the playback boundary. No model selection,
//! speech synthesis, task, or second bus: the existing stream owner feeds packets.
use crate::ai::stream_sinks::MediaChunk;
use crate::utils::audio::{bytes_to_i16, StreamingPcmResampler};
use std::sync::{Arc, atomic::{AtomicU8, Ordering}};

/// Owned by the live consumer future. Dropping/aborting that future invalidates
/// queued playback without spawning an asynchronous cleanup task. Successful
/// completion permits the remaining queue to drain.
#[must_use = "hold the playback lease until completion or interruption"]
pub struct NativePlaybackLease(pub(super) Arc<AtomicU8>);

impl Drop for NativePlaybackLease {
    fn drop(&mut self) {
        let _ = self.0.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
    }
}

pub struct NativePcmPlayback {
    converter: StreamingPcmResampler,
    sequence: u64,
    source_samples: u64,
}

impl NativePcmPlayback {
    /// Explicitly selects the supported native wire and the existing mixer rate.
    pub fn new(mime: &str) -> Result<Self, String> {
        if mime != crate::inference::native_output::PCM_MIME {
            return Err(format!("Unsupported native playback format: {mime}"));
        }
        Ok(Self {
            converter: StreamingPcmResampler::new(24_000, crate::audio_constants::AUDIO_SAMPLE_RATE)?,
            sequence: 0,
            source_samples: 0,
        })
    }

    pub fn push(&mut self, packet: &MediaChunk) -> Result<Vec<i16>, String> {
        if packet.mime_type != crate::inference::native_output::PCM_MIME
            || packet.sequence != self.sequence
            || packet.presentation_time_us != self.source_samples * 1_000_000 / 24_000
        {
            return Err("Native playback format or media continuity changed".into());
        }
        if packet.data.is_empty() || packet.data.len() % 2 != 0
            || packet.data.len() > crate::ai::stream_sinks::MAX_GENERATION_CHUNK_BYTES
        {
            return Err("Native playback requires a bounded complete PCM packet".into());
        }
        let samples = bytes_to_i16(&packet.data);
        let output = self.converter.push(&samples)?;
        self.source_samples += samples.len() as u64;
        self.sequence += 1;
        Ok(output)
    }

    /// The owner calls this only after successful model completion, never merely
    /// because a channel disconnected. Failure/cancellation discards this state.
    pub fn finish(&mut self) -> Result<Vec<i16>, String> {
        if self.source_samples == 0 { return Err("Native playback received no audio".into()); }
        self.converter.finish()
    }
}
