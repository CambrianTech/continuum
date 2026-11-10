//! Training leases share the resource authority used by live serving: an engine concern
//! (the lease and the serving gate it reads under), so the genome's trainers and the forge
//! both admit through it here.
use crate::inference::serving_lifecycle::LifecycleGate;
use crate::resources::{
    LeaseError, LeaseGuard, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind,
};
use std::time::Duration;

/// A training lease of `bytes` in `kind`: the memory the lane it trains in holds its weights
/// in (a GPU lane's VRAM, a CPU lane's host RAM).
pub fn request(consumer: &str, kind: ResourceKind, bytes: u64) -> LeaseRequest {
    LeaseRequest {
        consumer_id: consumer.to_owned(),
        kind,
        bytes,
        ttl_ms: u64::MAX,
        reclaim_policy: ReclaimPolicy::Pinned,
    }
}

/// While the serving lifecycle refuses (unsettled, or an operation holds the gate),
/// a waiting learner retries on this cadence as well as on governor changes: a relaunch that
/// frees and re-leases can finish with no governor edge, and the gate has no event.
// derived-or-floor: a floor — far below a relaunch's duration, far above a spin.
const SERVING_BUSY_RETRY: Duration = Duration::from_secs(1);

/// Why a training lease is not granted on this attempt, and what to wait for before the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrainingWait {
    /// The serving lifecycle refuses admission (a boot or relaunch window); it has no event.
    Serving(String),
    /// Governed capacity is short: the authority's next change may free it.
    Capacity { needed: u64, available: u64 },
}

impl std::fmt::Display for TrainingWait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serving(refusal) => write!(f, "the serving lifecycle refuses training admission now ({refusal})"),
            Self::Capacity { needed, available } => write!(f, "{available} of {needed} governed bytes are free"),
        }
    }
}

/// One training lease attempt under the serving lifecycle gate, never waiting: a caller holding a
/// mind's serving resources (her act gate, a lane, a slot) tries here, and on a [`TrainingWait`]
/// lets them all go before [`await_training_change`]. Same authority, same gate, no capacity made up.
pub fn try_training_memory(daemon: &std::sync::Arc<ResourceDaemon>, serving: &LifecycleGate, consumer: &str, kind: ResourceKind, bytes: u64) -> Result<Result<LeaseGuard, TrainingWait>, String> {
    if bytes == 0 {
        return Err("training memory requirement must be measured before admission".into());
    }
    let hold = match serving.hold_for_admission() {
        Ok(hold) => hold,
        Err(refusal) => return Ok(Err(TrainingWait::Serving(refusal.name().to_owned()))),
    };
    let attempt = daemon.acquire_guarded(&request(consumer, kind, bytes));
    drop(hold);
    match attempt {
        Ok(guard) => Ok(Ok(guard)),
        Err(LeaseError::InsufficientCapacity { available, .. }) => {
            continuum_base::probe!(class = "training.admission.waiting", consumer = consumer, footprint_bytes = bytes, available_bytes = available,
                "a training lease waits for governed capacity, holding no serving resource");
            Ok(Err(TrainingWait::Capacity { needed: bytes, available }))
        }
        Err(error) => Err(format!("training memory admission failed: {error:?}")),
    }
}

/// Wait, holding nothing, until `wait` may have cleared: the authority's next change for capacity
/// (`changes` subscribed BEFORE the attempt, so a release in between is not missed), or the serving
/// gate's retry cadence, since the lifecycle gate has no event.
pub async fn await_training_change(changes: &mut tokio::sync::watch::Receiver<crate::resources::LeaseBoard>, wait: &TrainingWait) -> Result<(), String> {
    match wait {
        TrainingWait::Capacity { .. } => changes.changed().await.map_err(|_| "training resource authority closed".to_owned()),
        TrainingWait::Serving(_) => {
            tokio::select! {
                changed = changes.changed() => changed.map_err(|_| "training resource authority closed".to_owned()),
                _ = tokio::time::sleep(SERVING_BUSY_RETRY) => Ok(()),
            }
        }
    }
}

/// The caller retains this guard through child exit/reap. This non-expiring, pinned lease cannot be
/// reclaimed while the child runs: the lifetime of the process, not a timer, owns release.
pub fn acquire_training_memory(consumer: &str, kind: ResourceKind, bytes: u64) -> Result<LeaseGuard, String> {
    if bytes == 0 {
        return Err("training memory requirement must be measured before admission".into());
    }
    let daemon =
        ResourceDaemon::global().ok_or("training requires the resource governor to be running")?;
    let guard = daemon
        .acquire_guarded(&request(consumer, kind, bytes))
        .map_err(|error| format!("training memory admission refused: {error:?}"))?;
    continuum_base::probe!(
        class = "training.admission",
        consumer = consumer,
        footprint_bytes = bytes,
        "governor granted a training lease"
    );
    Ok(guard)
}
