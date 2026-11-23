//! Observation every component inherits: every input is measured (its wait in the intake and its
//! time to handle) and published as [`Telemetry`] any reader watches live; the [`Observe`] switch
//! adds only a probe line per input (object model 5.7: "observation is inherited and switchable").

use std::collections::HashMap;
use std::time::Duration;

use crate::dataflow::{InputRevision, TopicName};

/// How closely a component is watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Observe {
    /// Measured and published, no probe line (the default).
    #[default]
    Off,
    /// Also a probe line per handled input.
    Timing,
}

/// The switch, keyed by the component's name: publish `Timing` to watch one part.
pub const OBSERVE: TopicName = TopicName("sage/observe");

/// What the engine measured, keyed by the component's name.
pub const TELEMETRY: TopicName = TopicName("sage/telemetry");

/// One component's numbers since it started, measured for every component on every input (two clock
/// reads) and published as state; switching `Observe` on adds a probe line per input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Telemetry {
    /// Inputs handled.
    pub handled: u64,
    /// How long its last input waited in its intake before it was handled: where this part, or what
    /// it waits behind, is the bottleneck.
    pub last_wait: Duration,
    /// The longest any input waited in its intake.
    pub max_wait: Duration,
    /// How long its last input took to handle.
    pub last_handle: Duration,
    /// The longest any input took: the serial cost this part adds where it is waited on.
    pub max_handle: Duration,
    /// Inputs waiting in its intake after the last one.
    pub waiting: usize,
    /// Frames a coalescing reader skipped before it was handed the next (a part not keeping up).
    pub frames_skipped: u64,
}

/// What accumulates one component's [`Telemetry`]: its numbers so far and, per topic, the last frame
/// it handled (a gap is frames a coalescing reader skipped). Owned by the component's worker.
#[derive(Default)]
pub(super) struct Meter {
    measured: Telemetry,
    last_frames: HashMap<TopicName, u64>,
}

impl Meter {
    /// One handled input: how long it `waited` in the intake, how long it `took`, the frame it came
    /// `from` (none for an act or a timer), and how many inputs are `waiting` behind it.
    pub(super) fn record(&mut self, waited: Duration, took: Duration, from: Option<InputRevision>, waiting: usize) -> Telemetry {
        let skipped = match from.map(|frame| (frame.through, self.last_frames.insert(frame.topic, frame.through))) {
            Some((through, Some(last))) => through.saturating_sub(last + 1),
            // Its first frame on this topic, or no frame at all: nothing to compare.
            _ => 0,
        };
        let m = self.measured;
        self.measured = Telemetry {
            handled: m.handled + 1,
            last_wait: waited,
            max_wait: m.max_wait.max(waited),
            last_handle: took,
            max_handle: m.max_handle.max(took),
            waiting,
            frames_skipped: m.frames_skipped + skipped,
        };
        self.measured
    }

    /// Its numbers so far.
    pub(super) fn measured(&self) -> Telemetry {
        self.measured
    }
}
