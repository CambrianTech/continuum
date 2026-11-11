//! The dyno's virtual clock and schedule tools: events at virtual milliseconds, advanced to the
//! next one and never slept; seeded schedules drawn from the shared splitmix64 hash
//! (`capacity::grid::churn`); and the shrinker that keeps the shortest failing trace.
#![cfg(test)]

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// Events ordered by virtual time, then by the order they were scheduled (a tie is FIFO, so a
/// replay is deterministic).
pub(crate) struct Clock<E> {
    virtual_ms: u64,
    next_seq: u64,
    due: BinaryHeap<Reverse<(u64, u64)>>,
    events: BTreeMap<u64, E>,
}

impl<E> Clock<E> {
    pub(crate) fn new() -> Self {
        Self { virtual_ms: 0, next_seq: 0, due: BinaryHeap::new(), events: BTreeMap::new() }
    }

    pub(crate) fn virtual_ms(&self) -> u64 {
        self.virtual_ms
    }

    /// Schedule `event` at virtual time `at_ms` (never before now).
    pub(crate) fn at(&mut self, at_ms: u64, event: E) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.due.push(Reverse((at_ms.max(self.virtual_ms), seq)));
        self.events.insert(seq, event);
    }

    /// Schedule `event` `delay_ms` after now.
    pub(crate) fn after(&mut self, delay_ms: u64, event: E) {
        self.at(self.virtual_ms.saturating_add(delay_ms), event);
    }

    /// Advance to the next event and return it; `None` when nothing is scheduled.
    pub(crate) fn next(&mut self) -> Option<E> {
        let Reverse((at_ms, seq)) = self.due.pop()?;
        self.virtual_ms = at_ms;
        self.events.remove(&seq)
    }
}

/// Seeded draws: the same seed is the same schedule on every machine.
pub(crate) struct Draws {
    seed: u64,
    drawn: u64,
}

impl Draws {
    pub(crate) fn new(seed: u64) -> Self {
        Self { seed, drawn: 0 }
    }

    /// A draw in `0..n` (`n > 0`).
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        self.drawn += 1;
        crate::capacity::grid::churn(self.seed.rotate_left(17) ^ self.drawn) % n.max(1)
    }
}

/// The shortest failing schedule reachable from `failing` by deleting steps: chunks first, then
/// single steps, to a fixpoint (ddmin's deletion half). `fails` must hold for `failing`.
pub(crate) fn shrink<S: Clone>(failing: Vec<S>, fails: impl Fn(&[S]) -> bool) -> Vec<S> {
    let mut current = failing;
    let mut chunk = current.len().div_ceil(2).max(1);
    loop {
        let mut removed = false;
        let mut start = 0;
        while start < current.len() {
            let end = (start + chunk).min(current.len());
            let candidate: Vec<S> = current[..start].iter().chain(&current[end..]).cloned().collect();
            if fails(&candidate) {
                current = candidate;
                removed = true;
            } else {
                start = end;
            }
        }
        if !removed {
            if chunk == 1 {
                return current;
            }
            chunk = chunk.div_ceil(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a clock that reorders simultaneous events or sleeps. Two events at the
    // same instant run in the order scheduled, time jumps straight to the next event, and an
    // event scheduled in the past runs now.
    #[test]
    fn the_clock_advances_to_the_next_event_in_schedule_order() {
        let mut clock = Clock::new();
        clock.at(5_000, "tick");
        clock.at(5_000, "turn ends");
        clock.at(42_857, "turn ends again");
        assert_eq!(clock.next(), Some("tick"));
        assert_eq!(clock.virtual_ms(), 5_000);
        assert_eq!(clock.next(), Some("turn ends"));
        clock.at(1, "late");
        assert_eq!(clock.next(), Some("late"));
        assert_eq!(clock.virtual_ms(), 5_000, "a past event runs now; time never moves back");
        assert_eq!(clock.next(), Some("turn ends again"));
        assert_eq!(clock.virtual_ms(), 42_857);
        assert_eq!(clock.next(), None);
    }

    // what this catches: a shrinker that returns a non-failing or non-minimal trace. The failure
    // needs a 3 then a 7 somewhere after it; every other step is noise and is deleted.
    #[test]
    fn shrink_keeps_only_the_steps_the_failure_needs() {
        let fails = |s: &[u32]| s.iter().position(|&x| x == 3).is_some_and(|i| s[i..].contains(&7));
        let failing = vec![1, 3, 4, 4, 9, 2, 7, 3, 1];
        assert!(fails(&failing));
        assert_eq!(shrink(failing, fails), vec![3, 7]);
        let mut a = Draws::new(11);
        let mut b = Draws::new(11);
        assert_eq!((0..8).map(|_| a.below(100)).collect::<Vec<_>>(), (0..8).map(|_| b.below(100)).collect::<Vec<_>>(), "a seed is a schedule");
    }
}
