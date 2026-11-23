//! Where an owner's time goes, readable from anywhere: a [`Profiler`] holds its parts' telemetry
//! readers (never their lifetimes), so a command, a view or the lane reads every part's measured
//! waits and handle times without holding or waiting on any part.

use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;

use super::Telemetry;
use crate::dataflow::Reader;

/// One part's measured numbers, by name.
struct Gauge {
    name: &'static str,
    telemetry: Reader<Telemetry>,
}

/// An owner's parts' numbers, read-only and cheap to clone; an owner's later parts appear in it,
/// and its parts leave it when the owner drops.
#[derive(Clone, Default)]
pub struct Profiler {
    gauges: Arc<RwLock<Vec<Gauge>>>,
}

impl Profiler {
    pub(super) fn add(&self, name: &'static str, telemetry: Reader<Telemetry>) {
        self.gauges.write().push(Gauge { name, telemetry });
    }

    pub(super) fn clear(&self) {
        self.gauges.write().clear();
    }

    /// Every part's numbers now, the longest-waiting first. A part that has handled nothing yet
    /// reads as zero.
    pub fn read(&self) -> Vec<(&'static str, Telemetry)> {
        let mut profile: Vec<_> = self
            .gauges
            .read()
            .iter()
            .map(|gauge| (gauge.name, gauge.telemetry.latest().map(|snapshot| *snapshot.value).unwrap_or_default())) // JUSTIFIED: nothing handled yet is zero time, not a missing fact
            .collect();
        profile.sort_by(|a, b| b.1.max_wait.cmp(&a.1.max_wait).then(b.1.max_handle.cmp(&a.1.max_handle)));
        profile
    }
}

/// The node's owner's profiler, beside the node's bus: set once, by the node's owner at boot.
static NODE: OnceLock<Profiler> = OnceLock::new();

pub(super) fn register_node(profiler: &Profiler) {
    if NODE.set(profiler.clone()).is_err() {
        continuum_base::probe!(class = "sage.owner.second_node_owner", "a second node owner was built; the first one's profile stays the node's");
    }
}

/// The node's parts' numbers, the longest-waiting first; `None` before the node's owner is built.
pub fn node_profile() -> Option<Vec<(&'static str, Telemetry)>> {
    NODE.get().map(Profiler::read)
}
