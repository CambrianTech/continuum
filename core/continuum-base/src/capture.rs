//! One capture sink for every structured record of a load-bearing decision
//! (docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md). Nothing watching costs one empty call
//! ([`Noop`]); a watcher takes a bounded [`Ring`] and drains it in [`Interval`]s.
//!
//! An interval knows what it is missing. Every event is stamped on the interval's own monotonic
//! clock, the next interval begins exactly where the last one ended, and an event the ring had no
//! room for is counted, never silently lost. A replay or gate that needs the whole workload takes a
//! [`CompleteInterval`], which only a lossless interval becomes (CONTINUUM-ENGINE 4.10 P5, Codex's
//! review: an incomplete capture cannot pass the live-versus-replay gate).

use std::collections::VecDeque;
use std::num::{NonZeroU64, NonZeroUsize};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Where one kind of record goes.
pub trait CaptureSink<E>: Send + Sync {
    fn record(&self, event: E);
}

/// Nobody is watching.
#[derive(Debug, Default, Clone, Copy)]
pub struct Noop;

impl<E> CaptureSink<E> for Noop {
    fn record(&self, _event: E) {}
}

/// When an event happened: the time since its interval began, on one monotonic clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct At(Duration);

impl At {
    pub fn since_start(self) -> Duration {
        self.0
    }
}

/// One record with its time.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamped<E> {
    pub at: At,
    pub event: E,
}

/// What an interval holds of what happened in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    Complete,
    /// The ring was full for `dropped` events: the interval is a sample, not the workload.
    Lossy { dropped: NonZeroU64 },
}

/// Everything recorded between two drains.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval<E> {
    pub span: Duration,
    pub events: Vec<Stamped<E>>,
    pub completeness: Completeness,
}

/// An interval holding every event that happened in it: the only shape a replay of the workload,
/// or a gate comparing live against replay, accepts.
#[derive(Debug, Clone, PartialEq)]
pub struct CompleteInterval<E> {
    span: Duration,
    events: Vec<Stamped<E>>,
}

impl<E> CompleteInterval<E> {
    pub fn span(&self) -> Duration {
        self.span
    }

    pub fn events(&self) -> &[Stamped<E>] {
        &self.events
    }
}

/// Why an interval cannot stand for the workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Incomplete {
    pub dropped: NonZeroU64,
}

impl std::fmt::Display for Incomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the capture dropped {} events in this interval", self.dropped)
    }
}

impl std::error::Error for Incomplete {}

impl<E> Interval<E> {
    pub fn complete(self) -> Result<CompleteInterval<E>, Incomplete> {
        match self.completeness {
            Completeness::Complete => Ok(CompleteInterval { span: self.span, events: self.events }),
            Completeness::Lossy { dropped } => Err(Incomplete { dropped }),
        }
    }
}

/// A bounded capture its watcher drains. Its bound is the watcher's memory to spend, so the watcher
/// passes it in; a full ring counts what it could not keep.
#[derive(Debug)]
pub struct Ring<E> {
    state: Mutex<RingState<E>>,
}

#[derive(Debug)]
struct RingState<E> {
    began: Instant,
    events: VecDeque<Stamped<E>>,
    capacity: NonZeroUsize,
    dropped: u64,
}

impl<E> Ring<E> {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self { state: Mutex::new(RingState { began: Instant::now(), events: VecDeque::new(), capacity, dropped: 0 }) }
    }

    /// Everything since the last drain; the next interval begins now.
    pub fn drain(&self) -> Interval<E> {
        // a poisoned ring still holds valid records: a panic elsewhere never loses the capture
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        let span = now.duration_since(state.began);
        state.began = now;
        let events = state.events.drain(..).collect();
        let completeness = match NonZeroU64::new(std::mem::take(&mut state.dropped)) {
            None => Completeness::Complete,
            Some(dropped) => Completeness::Lossy { dropped },
        };
        Interval { span, events, completeness }
    }
}

impl<E: Send> CaptureSink<E> for Ring<E> {
    fn record(&self, event: E) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.events.len() >= state.capacity.get() {
            state.dropped += 1;
            return;
        }
        let at = At(Instant::now().duration_since(state.began));
        state.events.push_back(Stamped { at, event });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(capacity: usize) -> Ring<u32> {
        Ring::new(NonZeroUsize::new(capacity).unwrap())
    }

    /// what this catches: a full ring that silently drops would hand the gate a sample posing as
    /// the workload; the loss must be counted and the interval refused as incomplete.
    #[test]
    fn a_full_ring_counts_its_loss_and_the_interval_cannot_gate() {
        let sink = ring(2);
        for n in 0..5 {
            sink.record(n);
        }
        let interval = sink.drain();
        assert_eq!(interval.events.iter().map(|s| s.event).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(interval.completeness, Completeness::Lossy { dropped: NonZeroU64::new(3).unwrap() });
        assert_eq!(interval.complete().unwrap_err().dropped.get(), 3);
    }

    /// what this catches: intervals must tile time with no gap and no carried loss, or a replay
    /// stitched from consecutive drains would invent or hide work.
    #[test]
    fn consecutive_intervals_tile_time_and_each_starts_clean() {
        let sink = ring(1);
        sink.record(1);
        sink.record(2);
        let first = sink.drain();
        sink.record(3);
        let second = sink.drain().complete().unwrap();
        assert!(matches!(first.completeness, Completeness::Lossy { .. }));
        assert_eq!(second.events().len(), 1);
        assert!(second.events()[0].at.since_start() <= second.span());
    }
}
