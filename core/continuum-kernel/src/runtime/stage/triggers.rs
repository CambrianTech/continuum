//! When a stage runs. Triggers compose: a stage declares every condition that wakes it.

use std::time::Duration;

use super::topic::TopicName;

/// The conditions that run a stage, together: an input topic moved, a task it dispatched
/// completed, and the never-comatose cadence floor (a floor, never the main trigger).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Triggers {
    pub inputs: Vec<TopicName>,
    pub completions: bool,
    pub floor: Option<Duration>,
}

impl Triggers {
    /// Whether any declared condition is met.
    pub fn fires(&self, moved: &[TopicName], completed: bool, since_last_run: Duration) -> bool {
        moved.iter().any(|topic| self.inputs.contains(topic))
            || (self.completions && completed)
            || self.floor.is_some_and(|floor| since_last_run >= floor)
    }
}
