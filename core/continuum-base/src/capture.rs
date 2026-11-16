//! One capture sink for every structured record of a load-bearing decision
//! (docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md). Nothing watching costs one empty call
//! ([`Noop`]); a watcher takes a bounded [`Ring`] and drains it in [`Interval`]s.
//!
//! A capture knows what it is and what it is missing. Each [`Ring`] is one capture with its own
//! identity; every event carries the time it HAPPENED on the producer's monotonic clock (never when
//! the sink received it); consecutive intervals are indexed from zero and tile the capture's clock
//! with no gap; events from concurrent producers are placed in the order they happened; and an
//! event the ring had no room for, or one that happened inside an interval already drained, is
//! counted, never silently kept or lost. A replay or gate that needs the whole workload takes a
//! [`CompleteInterval`], which only an interval with nothing missing becomes (CONTINUUM-ENGINE 4.10
//! P5; Codex's reviews of #6052).

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Where one kind of record goes, with the moment it happened on the producer's monotonic clock.
pub trait CaptureSink<E>: Send + Sync {
    fn record(&self, happened: Instant, event: E);
    /// `count` records the producer lost before they reached this sink (its own full buffer, a gap
    /// in its sequence): the interval they fell in is incomplete.
    fn missed(&self, count: u64);
}

/// Nobody is watching.
#[derive(Debug, Default, Clone, Copy)]
pub struct Noop;

impl<E> CaptureSink<E> for Noop {
    fn record(&self, _happened: Instant, _event: E) {}
    fn missed(&self, _count: u64) {}
}

/// One capture: every interval drained from one ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaptureId(uuid::Uuid);

impl CaptureId {
    pub fn mint() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

/// When an event happened: the time since its capture began, on one monotonic clock shared by every
/// interval the capture drains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct At(Duration);

impl At {
    /// The capture's own start.
    pub const ORIGIN: At = At(Duration::ZERO);

    pub fn since_origin(self) -> Duration {
        self.0
    }

    /// For a fixture or a replayed clock.
    pub fn from_origin(elapsed: Duration) -> Self {
        Self(elapsed)
    }
}

/// One record with its time.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamped<E> {
    pub at: At,
    pub event: E,
}

/// What an interval is missing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Missing {
    /// Events the ring had no room for.
    pub dropped: u64,
    /// Events that happened inside an interval already drained (or before the capture began): they
    /// arrived too late to be placed, so they are not kept.
    pub misordered: u64,
}

impl Missing {
    pub fn nothing(self) -> bool {
        self.dropped == 0 && self.misordered == 0
    }
}

/// Everything recorded between two drains: the `index`-th interval of `capture`, covering
/// `from..to` on the capture's clock, and what it is missing.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval<E> {
    pub capture: CaptureId,
    pub index: u64,
    pub from: At,
    pub to: At,
    pub events: Vec<Stamped<E>>,
    pub missing: Missing,
}

/// An interval holding every event that happened in it, in the order they happened: the only shape
/// a replay of the workload, or a gate comparing live against replay, accepts. A fixture recorded
/// earlier is an [`Interval`] too, and becomes one only through [`Interval::complete`].
#[derive(Debug, Clone, PartialEq)]
pub struct CompleteInterval<E> {
    capture: CaptureId,
    index: u64,
    from: At,
    to: At,
    events: Vec<Stamped<E>>,
}

impl<E> CompleteInterval<E> {
    pub fn capture(&self) -> CaptureId {
        self.capture
    }

    pub fn index(&self) -> u64 {
        self.index
    }

    pub fn from(&self) -> At {
        self.from
    }

    pub fn to(&self) -> At {
        self.to
    }

    pub fn events(&self) -> &[Stamped<E>] {
        &self.events
    }
}

/// Why an interval cannot stand for the workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incomplete {
    /// The ring dropped or rejected events.
    Missing { index: u64, missing: Missing },
    /// Its events are not in the order they happened, or fall outside its span.
    Unordered { index: u64, at: At },
}

impl std::fmt::Display for Incomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Incomplete::Missing { index, missing } => {
                write!(f, "interval {index} dropped {} and rejected {} misordered events", missing.dropped, missing.misordered)
            }
            Incomplete::Unordered { index, at } => write!(f, "interval {index} holds an event out of order at {at:?}"),
        }
    }
}

impl std::error::Error for Incomplete {}

impl<E> Interval<E> {
    /// The interval as the workload, or what disqualifies it: anything missing, or events not in
    /// the order they happened within its span (a recorded fixture is checked like a drain).
    pub fn complete(self) -> Result<CompleteInterval<E>, Incomplete> {
        if !self.missing.nothing() {
            return Err(Incomplete::Missing { index: self.index, missing: self.missing });
        }
        let mut last = self.from;
        for stamped in &self.events {
            if stamped.at < last || stamped.at > self.to {
                return Err(Incomplete::Unordered { index: self.index, at: stamped.at });
            }
            last = stamped.at;
        }
        Ok(CompleteInterval { capture: self.capture, index: self.index, from: self.from, to: self.to, events: self.events })
    }
}

/// A bounded capture its watcher drains. Its bound is the watcher's memory to spend, so the watcher
/// passes it in; a full ring counts what it could not keep.
#[derive(Debug)]
pub struct Ring<E> {
    capture: CaptureId,
    origin: Instant,
    state: Mutex<RingState<E>>,
}

#[derive(Debug)]
struct RingState<E> {
    index: u64,
    from: At,
    last: At,
    events: VecDeque<Stamped<E>>,
    capacity: NonZeroUsize,
    missing: Missing,
}

impl<E> Ring<E> {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capture: CaptureId::mint(),
            origin: Instant::now(),
            state: Mutex::new(RingState { index: 0, from: At::ORIGIN, last: At::ORIGIN, events: VecDeque::new(), capacity, missing: Missing::default() }),
        }
    }

    pub fn capture(&self) -> CaptureId {
        self.capture
    }

    /// Everything since the last drain; the next interval begins exactly where this one ends.
    pub fn drain(&self) -> Interval<E> {
        let to = At(Instant::now().saturating_duration_since(self.origin));
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner); // poison holds valid records: a panic elsewhere never loses the capture
        let to = to.max(state.last);
        let from = std::mem::replace(&mut state.from, to);
        let index = state.index;
        state.index += 1;
        let events = state.events.drain(..).collect();
        let missing = std::mem::take(&mut state.missing);
        Interval { capture: self.capture, index, from, to, events, missing }
    }
}

impl<E: Send> CaptureSink<E> for Ring<E> {
    fn record(&self, happened: Instant, event: E) {
        let before_origin = happened < self.origin;
        let at = At(happened.saturating_duration_since(self.origin));
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner); // poison holds valid records: a panic elsewhere never loses the capture
        if before_origin || at < state.from {
            state.missing.misordered += 1;
            return;
        }
        if state.events.len() >= state.capacity.get() {
            state.missing.dropped += 1;
            return;
        }
        // placed where it happened: a producer reporting a moment after another keeps its order
        let place = state.events.partition_point(|stamped| stamped.at <= at);
        state.events.insert(place, Stamped { at, event });
        state.last = state.last.max(at);
    }

    fn missed(&self, count: u64) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner); // poison holds valid records: a panic elsewhere never loses the capture
        state.missing.dropped = state.missing.dropped.saturating_add(count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(capacity: usize) -> Ring<u32> {
        Ring::new(NonZeroUsize::new(capacity).expect("test: a literal capacity above zero"))
    }

    /// what this catches: a full ring that silently drops would hand the gate a sample posing as
    /// the workload; the loss must be counted and the interval refused as incomplete.
    #[test]
    fn a_full_ring_counts_its_loss_and_the_interval_cannot_gate() {
        let sink = ring(2);
        for n in 0..5 {
            sink.record(Instant::now(), n);
        }
        let interval = sink.drain();
        assert_eq!(interval.events.iter().map(|s| s.event).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(interval.missing, Missing { dropped: 3, misordered: 0 });
        assert!(matches!(interval.complete(), Err(Incomplete::Missing { .. })));
    }

    /// what this catches (Codex on #6052): a sink stamping receive time lets a late report of an
    /// earlier happening reorder the workload. Within an interval the event is placed where it
    /// happened; once its interval is drained it is too late, and is counted so the gate refuses.
    #[test]
    fn a_late_report_is_placed_where_it_happened_or_counted_once_drained() {
        let sink = ring(8);
        let earlier = Instant::now();
        sink.record(earlier + Duration::from_millis(5), 1);
        sink.record(earlier, 2);
        let first = sink.drain();
        assert_eq!(first.events.iter().map(|s| s.event).collect::<Vec<_>>(), vec![2, 1], "placed by when it happened");
        sink.record(earlier, 3);
        let second = sink.drain();
        assert_eq!(second.missing.misordered, 1, "its interval was already drained");
        assert!(first.complete().is_ok() && second.complete().is_err());
    }

    /// what this catches: a producer's own loss (its full buffer) never reaching the sink would let
    /// that interval gate; reported loss voids it like a drop here.
    #[test]
    fn loss_reported_by_the_producer_voids_the_interval() {
        let sink = ring(8);
        sink.record(Instant::now(), 1);
        sink.missed(2);
        let interval = sink.drain();
        assert_eq!(interval.missing.dropped, 2);
        assert!(interval.complete().is_err());
    }

    /// what this catches: intervals must tile time from the capture's origin with no gap and no
    /// carried loss, or a replay stitched from consecutive drains would invent or hide work.
    #[test]
    fn consecutive_intervals_tile_the_capture_from_its_origin() {
        let sink = ring(1);
        sink.record(Instant::now(), 1);
        sink.record(Instant::now(), 2);
        let first = sink.drain();
        sink.record(Instant::now(), 3);
        let second = sink.drain().complete().expect("test: the second interval lost nothing");
        assert_eq!((first.index, first.from), (0, At::ORIGIN));
        assert!(first.missing.dropped > 0);
        assert_eq!((second.index(), second.from(), second.capture()), (1, first.to, sink.capture()));
    }
}
