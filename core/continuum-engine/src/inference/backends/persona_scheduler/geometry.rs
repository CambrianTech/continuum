//! What the engine actually is, read from the context it created (never from what was asked): the
//! one owner serving reads for the window, the batch and the sequence layout. Each snapshot names
//! the engine incarnation it was read from, so a reader retires a stale observation instead of
//! acting on an engine that is gone, and a driver that exits (or panics) says so.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use llama::KvCacheType;

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

/// Settings the context was ASKED for that llama.cpp does not report back: the KV types are
/// requests. Never read as applied capability. (Flash attention is not a request: the scheduler
/// always enables it, and a context that cannot have it is refused, never made.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requested {
    pub type_k: KvCacheType,
    pub type_v: KvCacheType,
}

/// The applied geometry of one engine, as llama.cpp reports it from the created context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineGeometry {
    pub incarnation: Incarnation,
    /// The window the context holds, across every sequence.
    pub n_ctx: u32,
    /// The window ONE mind may use: the whole pool (what serving publishes as her usable context).
    pub n_ctx_seq: u32,
    /// Sequences the context holds: one seat for each mind held in place in its pool.
    pub n_seq_max: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    /// Minds that decode at once: always one (one mind decodes at a time).
    pub decoding: u32,
    pub requested: Requested,
}

impl EngineGeometry {
    /// Read from the created context; only what it cannot report comes from the request.
    pub(super) fn applied(incarnation: Incarnation, ctx: &llama::Context<'_>, config: &SchedulerConfig) -> Self {
        let n_seq_max = ctx.n_seq_max();
        Self {
            incarnation,
            n_ctx: ctx.n_ctx(),
            n_ctx_seq: ctx.n_ctx_seq(),
            n_seq_max,
            n_batch: ctx.n_batch(),
            n_ubatch: ctx.n_ubatch(),
            decoding: 1,
            requested: Requested { type_k: config.type_k, type_v: config.type_v },
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
    /// Its driver has exited (its last handle dropped, or a panic unwound it): no turn runs on this
    /// incarnation, and nothing it published before is current.
    Stopped(Incarnation),
}

impl EngineState {
    pub fn incarnation(&self) -> Incarnation {
        match self {
            Self::Starting(incarnation) | Self::Refused { incarnation, .. } | Self::Stopped(incarnation) => *incarnation,
            Self::Ready(geometry) => geometry.incarnation,
        }
    }

    /// The geometry, only while the engine is serving.
    pub fn ready(&self) -> Option<&EngineGeometry> {
        match self {
            Self::Ready(geometry) => Some(geometry),
            Self::Starting(_) | Self::Refused { .. } | Self::Stopped(_) => None,
        }
    }
}

/// Owned by the driver thread: when it exits by any path, including a panic unwinding it, a
/// `Starting` or `Ready` it left behind becomes `Stopped`. A `Refused` keeps its reason.
pub(super) struct StopsOnExit<'s> {
    pub(super) state: &'s tokio::sync::watch::Sender<EngineState>,
    pub(super) incarnation: Incarnation,
}

impl Drop for StopsOnExit<'_> {
    fn drop(&mut self) {
        let incarnation = self.incarnation;
        self.state.send_if_modified(|state| match state {
            EngineState::Starting(_) | EngineState::Ready(_) => {
                *state = EngineState::Stopped(incarnation);
                true
            }
            EngineState::Refused { .. } | EngineState::Stopped(_) => false,
        });
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

    // what this catches (Codex on #6150): a driver that exited or panicked leaving its last Ready
    // in the watch, so serving grants turns to an engine that is gone; and a Refused reason
    // overwritten on the way out.
    #[test]
    fn an_exiting_driver_leaves_stopped_never_a_stale_ready_and_keeps_a_refusal() {
        let incarnation = Incarnation::next();
        let (state, seen) = tokio::sync::watch::channel(EngineState::Starting(incarnation));
        drop(StopsOnExit { state: &state, incarnation });
        assert!(matches!(*seen.borrow(), EngineState::Stopped(i) if i == incarnation));
        assert!(seen.borrow().ready().is_none());

        let reason: Arc<str> = "the context was refused".into();
        state.send_replace(EngineState::Refused { incarnation, reason });
        drop(StopsOnExit { state: &state, incarnation });
        assert!(matches!(&*seen.borrow(), EngineState::Refused { reason, .. } if &**reason == "the context was refused"));

        // A panic unwinding the driver drops the guard the same way.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            state.send_replace(EngineState::Starting(incarnation));
            let _stops = StopsOnExit { state: &state, incarnation };
            panic!("the driver failed");
        }));
        assert!(panicked.is_err());
        assert!(matches!(*seen.borrow(), EngineState::Stopped(_)));
    }
}
