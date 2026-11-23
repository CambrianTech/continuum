//! Observation every component inherits, switched per component (object model 5.7: "observation is
//! inherited and switchable"). Off, it costs one state read per input. On, the engine measures what
//! passes through it and publishes the numbers as state any reader watches live.

use std::time::Duration;

use crate::dataflow::TopicName;

/// How closely a component is watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Observe {
    /// Not measured (the default).
    #[default]
    Off,
    /// Each handled input is timed, and its telemetry published.
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
