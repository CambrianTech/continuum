//! Audio Buffer Pool
//!
//! Server-side cache of synthesized audio indexed by Handle.
//! Audio stays in Rust memory — TypeScript receives only a Handle (UUID)
//! and metadata (duration, sample count). No PCM crosses IPC for handle-based ops.
//!
//! Use cases:
//! - Pre-synthesize phrases, play later by handle
//! - Play same audio multiple times without re-synthesizing
//! - Coordinate multi-phrase sequences from TypeScript without audio round-trips
//!
//! Buffers expire after a configurable TTL (default: 5 minutes).

use super::handle::Handle;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Default time-to-live for audio buffers (5 minutes)
const DEFAULT_TTL: Duration = Duration::from_secs(300);

/// Maximum number of buffers before forced eviction of oldest
const MAX_BUFFERS: usize = 256;

/// Metadata returned to TypeScript when a buffer is created.
/// No audio data — just enough info to coordinate timing.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioHandleInfo {
    pub handle: String,
    pub sample_count: usize,
    pub sample_rate: u32,
    pub duration_ms: u64,
    pub adapter: String,
}

/// A stored audio buffer with metadata and expiration.
struct AudioBuffer {
    samples: Vec<i16>,
    sample_rate: u32,
    duration_ms: u64,
    adapter: String,
    created_at: Instant,
    last_accessed: Instant,
    ttl: Duration,
}

impl AudioBuffer {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// Thread-safe pool of audio buffers, indexed by Handle.
pub struct AudioBufferPool {
    buffers: RwLock<HashMap<Handle, AudioBuffer>>,
}

impl AudioBufferPool {
    pub fn new() -> Self {
        Self {
            buffers: RwLock::new(HashMap::new()),
        }
    }

    /// Store synthesized audio and return a handle + metadata.
    /// Audio stays in Rust memory. Only metadata crosses IPC.
    pub fn store(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
        duration_ms: u64,
        adapter: &str,
    ) -> AudioHandleInfo {
        let handle = Handle::new();
        let sample_count = samples.len();
        let now = Instant::now();

        let buffer = AudioBuffer {
            samples,
            sample_rate,
            duration_ms,
            adapter: adapter.to_string(),
            created_at: now,
            last_accessed: now,
            ttl: DEFAULT_TTL,
        };

        let mut buffers = self.buffers.write();

        // Evict expired buffers
        let before = buffers.len();
        buffers.retain(|_, buf| !buf.is_expired());
        let evicted = before - buffers.len();
        if evicted > 0 {
            info!("AudioBufferPool: Evicted {} expired buffers", evicted);
        }

        // If still over limit, evict oldest by creation time
        if buffers.len() >= MAX_BUFFERS {
            let oldest = buffers
                .iter()
                .min_by_key(|(_, buf)| buf.last_accessed)
                .map(|(h, _)| *h);
            if let Some(oldest_handle) = oldest {
                buffers.remove(&oldest_handle);
                warn!(
                    "AudioBufferPool: Evicted oldest buffer {} (at capacity {})",
                    oldest_handle.short(),
                    MAX_BUFFERS
                );
            }
        }

        buffers.insert(handle, buffer);

        info!(
            "AudioBufferPool: Stored {} ({} samples, {}ms, {}) — {} total",
            handle.short(),
            sample_count,
            duration_ms,
            adapter,
            buffers.len()
        );

        AudioHandleInfo {
            handle: handle.to_string(),
            sample_count,
            sample_rate,
            duration_ms,
            adapter: adapter.to_string(),
        }
    }

    /// Retrieve audio samples by handle (for injection into mixer).
    /// Returns None if handle not found or expired.
    /// Updates last_accessed time on access.
    pub fn get(&self, handle: &Handle) -> Option<Vec<i16>> {
        let mut buffers = self.buffers.write();
        let buffer = buffers.get_mut(handle)?;

        if buffer.is_expired() {
            buffers.remove(handle);
            info!("AudioBufferPool: Handle {} expired on access", handle.short());
            return None;
        }

        buffer.last_accessed = Instant::now();
        Some(buffer.samples.clone())
    }

    /// Get metadata for a handle without cloning audio data.
    pub fn info(&self, handle: &Handle) -> Option<AudioHandleInfo> {
        let buffers = self.buffers.read();
        let buffer = buffers.get(handle)?;

        if buffer.is_expired() {
            return None;
        }

        Some(AudioHandleInfo {
            handle: handle.to_string(),
            sample_count: buffer.samples.len(),
            sample_rate: buffer.sample_rate,
            duration_ms: buffer.duration_ms,
            adapter: buffer.adapter.clone(),
        })
    }

    /// Explicitly discard a buffer. Returns true if it existed.
    pub fn discard(&self, handle: &Handle) -> bool {
        let removed = self.buffers.write().remove(handle).is_some();
        if removed {
            info!("AudioBufferPool: Discarded {}", handle.short());
        }
        removed
    }

    /// Number of active (non-expired) buffers.
    pub fn len(&self) -> usize {
        let buffers = self.buffers.read();
        buffers.values().filter(|b| !b.is_expired()).count()
    }

    /// Returns true if there are no active buffers.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Evict all expired buffers. Returns count evicted.
    pub fn evict_expired(&self) -> usize {
        let mut buffers = self.buffers.write();
        let before = buffers.len();
        buffers.retain(|_, buf| !buf.is_expired());
        before - buffers.len()
    }
}

impl Default for AudioBufferPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_constants::AUDIO_SAMPLE_RATE;

    #[test]
    fn test_store_and_retrieve() {
        let pool = AudioBufferPool::new();
        let samples = vec![100i16, 200, 300, -100, -200];
        let info = pool.store(samples.clone(), AUDIO_SAMPLE_RATE, 100, "kokoro");

        assert_eq!(info.sample_count, 5);
        assert_eq!(info.sample_rate, AUDIO_SAMPLE_RATE);
        assert_eq!(info.duration_ms, 100);
        assert_eq!(info.adapter, "kokoro");

        let handle: Handle = info.handle.parse().expect("Should parse handle");
        let retrieved = pool.get(&handle).expect("Should find buffer");
        assert_eq!(retrieved, samples);
    }

    #[test]
    fn test_discard() {
        let pool = AudioBufferPool::new();
        let info = pool.store(vec![0i16; 100], AUDIO_SAMPLE_RATE, 10, "edge");
        let handle: Handle = info.handle.parse().unwrap();

        assert_eq!(pool.len(), 1);
        assert!(pool.discard(&handle));
        assert_eq!(pool.len(), 0);
        assert!(pool.get(&handle).is_none());
    }

    #[test]
    fn test_nonexistent_handle() {
        let pool = AudioBufferPool::new();
        let fake = Handle::new();
        assert!(pool.get(&fake).is_none());
        assert!(pool.info(&fake).is_none());
        assert!(!pool.discard(&fake));
    }

    #[test]
    fn test_multiple_buffers() {
        let pool = AudioBufferPool::new();
        let info1 = pool.store(vec![1i16; 10], AUDIO_SAMPLE_RATE, 1, "kokoro");
        let info2 = pool.store(vec![2i16; 20], AUDIO_SAMPLE_RATE, 2, "edge");
        let info3 = pool.store(vec![3i16; 30], AUDIO_SAMPLE_RATE, 3, "piper");

        assert_eq!(pool.len(), 3);

        let h1: Handle = info1.handle.parse().unwrap();
        let h2: Handle = info2.handle.parse().unwrap();
        let h3: Handle = info3.handle.parse().unwrap();

        assert_eq!(pool.get(&h1).unwrap(), vec![1i16; 10]);
        assert_eq!(pool.get(&h2).unwrap(), vec![2i16; 20]);
        assert_eq!(pool.get(&h3).unwrap(), vec![3i16; 30]);
    }

    #[test]
    fn test_info_without_cloning_audio() {
        let pool = AudioBufferPool::new();
        let stored = pool.store(vec![0i16; 16000], AUDIO_SAMPLE_RATE, 1000, "kokoro");
        let handle: Handle = stored.handle.parse().unwrap();

        let info = pool.info(&handle).expect("Should have info");
        assert_eq!(info.sample_count, 16000);
        assert_eq!(info.duration_ms, 1000);
        assert_eq!(info.adapter, "kokoro");
    }

    #[test]
    fn test_evict_expired() {
        let pool = AudioBufferPool::new();
        // Store a buffer
        pool.store(vec![0i16; 10], AUDIO_SAMPLE_RATE, 1, "test");
        assert_eq!(pool.len(), 1);

        // Nothing should be expired yet
        let evicted = pool.evict_expired();
        assert_eq!(evicted, 0);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn test_capacity_eviction() {
        let pool = AudioBufferPool::new();

        // Fill to capacity
        for i in 0..MAX_BUFFERS {
            pool.store(vec![i as i16; 10], AUDIO_SAMPLE_RATE, 1, "test");
        }
        assert_eq!(pool.len(), MAX_BUFFERS);

        // One more should evict the oldest
        pool.store(vec![999i16; 10], AUDIO_SAMPLE_RATE, 1, "test");
        assert_eq!(pool.len(), MAX_BUFFERS);
    }
}
