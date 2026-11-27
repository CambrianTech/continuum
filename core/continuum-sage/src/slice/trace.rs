//! A run of the lane as data: every input the slicer was given and every decision it made, in
//! order, timed from the run's start. A trace is the whole truth of a run, so:
//!
//! - **replayed** into a fresh slicer under the same control law, it must reproduce every
//!   decision exactly ([`replay`]): the regression validator for any change to the slicer;
//! - **fitted** into a workload ([`super::sim::Workload::fit`]), it drives the simulator, where any
//!   control law can be compared on the load a live node actually carried. (A different law makes
//!   different decisions, so the recorded steps after its first divergence never happened under
//!   it: comparing laws goes through the fitted workload, never the raw trace.)

use std::hash::Hash;
use std::time::{Duration, Instant};

use crate::component::Need;

use super::{Next, Slicer};

/// One thing the slicer was told, or decided.
#[derive(Debug, Clone, PartialEq)]
pub enum Event<K> {
    Need(K, Need),
    Ready(K),
    Done(K),
    Stepped(K, Duration),
    Switched(K, Duration),
    /// Her KV came back on the device in this long, measured: the whole copy, whether or not a slice hid it.
    Fetched(K, Duration),
    /// It decided at this offset from the run's start, and what.
    Decide(Duration, Next<K>),
}

/// A run, in order.
#[derive(Debug, Clone, Default)]
pub struct Trace<K> {
    /// The control law that ran it.
    pub control: &'static str,
    pub events: Vec<Event<K>>,
}

/// Where a replay first decided differently from the trace.
#[derive(Debug, Clone, PartialEq)]
pub struct Diverged<K> {
    pub at: usize,
    pub recorded: Next<K>,
    pub replayed: Next<K>,
}

/// Feed `trace` to `slicer` and check every decision against the recorded one.
pub fn replay<K: Copy + Eq + Hash + Ord>(trace: &Trace<K>, slicer: &mut Slicer<K>) -> Result<usize, Diverged<K>> {
    let start = Instant::now();
    let mut decisions = 0;
    for (at, event) in trace.events.iter().enumerate() {
        if let (Some(replayed), Event::Decide(_, recorded)) = (slicer.apply(event, start), event) {
            if &replayed != recorded {
                return Err(Diverged { at, recorded: *recorded, replayed });
            }
            decisions += 1;
        }
    }
    Ok(decisions)
}
