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
    /// Governed capacity in `kind` is short for `consumer`: resumable once that much is free.
    Capacity { consumer: String, kind: ResourceKind, needed: u64, available: u64 },
}

impl std::fmt::Display for TrainingWait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serving(refusal) => write!(f, "the serving lifecycle refuses training admission now ({refusal})"),
            Self::Capacity { kind, needed, available, .. } => write!(f, "{available} of {needed} governed {kind:?} bytes are free"),
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
            Ok(Err(TrainingWait::Capacity { consumer: consumer.to_owned(), kind, needed: bytes, available }))
        }
        Err(error) => Err(format!("training memory admission failed: {error:?}")),
    }
}

/// Wait, holding nothing, until `wait` may have cleared. For capacity: until the authority's governed
/// free bytes in that pool cover the need, the very test a lease is refused by, read at each of its
/// changes (`changes` subscribed BEFORE the attempt, so a release in between is not missed). The
/// board changes with every lease serving takes, so waking on any change re-ran a learner's whole
/// preparation several times a second against a need that could not fit (BigMama 2026-11-10: 150
/// probes in 28 s for 16.8 GB needed against 3.03 GB free). For the serving gate, which has no event:
/// its retry cadence.
pub async fn await_training_change(daemon: &std::sync::Arc<ResourceDaemon>, changes: &mut tokio::sync::watch::Receiver<crate::resources::LeaseBoard>, wait: &TrainingWait) -> Result<(), String> {
    match wait {
        TrainingWait::Capacity { consumer, kind, needed, .. } => loop {
            if daemon.available_for(consumer, *kind) >= *needed {
                return Ok(());
            }
            changes.changed().await.map_err(|_| "training resource authority closed".to_owned())?;
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{CapacitySource, DaemonConfig, MockCapacitySource};
    use std::sync::Arc;

    // what this catches (card b9dc9130; BigMama 2026-11-10, 150 probes in 28 s): a learner's capacity
    // wait woke on every board change. Serving's own leases change the board constantly, so the
    // learner re-ran its whole preparation against a need that could not fit. Changes that leave too
    // little free must not wake it; the change that frees enough must.
    #[tokio::test]
    async fn a_capacity_wait_sleeps_through_changes_that_cannot_cover_it_and_wakes_on_one_that_does() {
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 100));
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let mut changes = daemon.subscribe();
        changes.borrow_and_update();
        let serving = LifecycleGate::unowned(true);
        let wait = match try_training_memory(&daemon, &serving, "learning:her", ResourceKind::Vram, 1_000).expect("test: one attempt") {
            Err(wait) => wait,
            Ok(_) => panic!("test: 1,000 bytes cannot fit in 100"),
        };
        let waiting = await_training_change(&daemon, &mut changes, &wait);
        tokio::pin!(waiting);
        for _ in 0..5 {
            let lease = daemon.acquire_guarded(&request("serving", ResourceKind::Vram, 10)).expect("test: serving leases a little");
            drop(lease);
            tokio::task::yield_now().await;
            assert!(futures::poll!(&mut waiting).is_pending(), "a board change that leaves too little free must not wake her learner");
        }
        source.set_ceiling(10_000);
        tokio::time::timeout(Duration::from_secs(10), waiting)
            .await
            .expect("test: the change that frees enough wakes it")
            .expect("test: the authority is open");
    }
}
