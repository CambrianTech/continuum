//! A seating's steps, timed in order, so one probe names where its time went (card d1e5675c: on the
//! 5090, Kimi code/owner's Measured waited 255.8 s on its bucket's gate, held by a seating whose
//! decision had finished 169 ms before the tell; read from gaps between probes, it was unattributed).

use std::time::Instant;

/// One step of seating a bucket on the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    /// Her oldest pending row, decoded alone for her policy and name.
    Head,
    /// The competence signature minted from her pending rows.
    Signature,
    /// Which gene she learns, and whether now.
    Decide,
    /// Her next lesson, sized from the rows past her newest generation.
    Size,
    /// The seat's state, built.
    Build,
    /// The seat registered among the trainer's seats (the seats lock waited for, then the insert).
    Register,
    /// The engine told to measure it.
    Tell,
    /// What every seat owes, said.
    Owe,
}

/// Each step's milliseconds, in the order they ran.
pub(super) struct Phases {
    last: Instant,
    ms: Vec<(Phase, u64)>,
}

impl Phases {
    pub(super) fn start() -> Self {
        Self { last: Instant::now(), ms: Vec::new() }
    }

    /// `phase` ended now: its time is what passed since the step before it ended.
    pub(super) fn mark(&mut self, phase: Phase) {
        let now = Instant::now();
        self.ms.push((phase, now.duration_since(self.last).as_millis() as u64)); // as: a step's ms fits a u64
        self.last = now;
    }

    pub(super) fn ms(&self) -> &[(Phase, u64)] {
        &self.ms
    }
}
