//! What the engine actually is, read from the context it created (never from what was asked): the
//! one owner serving reads for the window, the batch and the KV layout. Each snapshot names the
//! engine incarnation it was read from, so a reader retires a stale observation instead of acting
//! on an engine that is gone.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use continuum_persona_llama::STREAMS;
use llama::{FlashAttn, KvCacheType};

use super::SchedulerConfig;

/// Which engine a snapshot came from: one per scheduler spawn in this process, rising.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Incarnation(u64);

impl Incarnation {
    pub(super) fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

/// The applied geometry of one engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineGeometry {
    pub incarnation: Incarnation,
    /// The window the context holds, across every stream.
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    /// Minds that decode at once: always one (one mind decodes at a time).
    pub decoding: usize,
    /// Windows held beside the decoding one for the next mind, staged as data, never decoding.
    pub staging: usize,
    pub type_k: KvCacheType,
    pub type_v: KvCacheType,
    pub flash_attn: FlashAttn,
}

impl EngineGeometry {
    /// Read from the created context. The KV types and attention are what the context was made
    /// with; llama.cpp reports the window and batches it applied.
    pub(super) fn applied(incarnation: Incarnation, ctx: &llama::Context<'_>, config: &SchedulerConfig) -> Self {
        Self {
            incarnation,
            n_ctx: ctx.n_ctx(),
            n_batch: ctx.n_batch(),
            n_ubatch: ctx.n_ubatch(),
            decoding: 1,
            staging: STREAMS - 1,
            type_k: config.type_k,
            type_v: config.type_v,
            flash_attn: config.flash_attn,
        }
    }
}

/// The engine as serving sees it.
#[derive(Debug, Clone)]
pub enum EngineState {
    /// Spawned; its context is not made yet.
    Starting(Incarnation),
    /// Its context is made: this is what it is.
    Ready(EngineGeometry),
    /// Its context was refused; no turn runs on this incarnation.
    Refused { incarnation: Incarnation, reason: Arc<str> },
}

impl EngineState {
    pub fn incarnation(&self) -> Incarnation {
        match self {
            Self::Starting(incarnation) | Self::Refused { incarnation, .. } => *incarnation,
            Self::Ready(geometry) => geometry.incarnation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two engines in one process sharing an incarnation, so serving cannot tell
    // a snapshot of the engine that is gone from one of the engine that replaced it.
    #[test]
    fn each_engine_spawn_is_a_later_incarnation() {
        let first = Incarnation::next();
        let second = Incarnation::next();
        assert!(second > first);
        assert_eq!(EngineState::Starting(second).incarnation(), second);
    }
}
