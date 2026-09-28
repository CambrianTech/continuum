//! Training leases share the resource authority used by live serving.
use crate::modules::serving_daemon::LifecycleGate;
use crate::resources::{
    LeaseError, LeaseGuard, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind,
};
use std::time::Duration;

pub(crate) fn request(consumer: &str, bytes: u64) -> LeaseRequest {
    LeaseRequest {
        consumer_id: consumer.to_owned(),
        kind: ResourceKind::Vram,
        bytes,
        ttl_ms: u64::MAX,
        reclaim_policy: ReclaimPolicy::Pinned,
    }
}

/// While the serving lifecycle refuses (unsettled, or an operation holds the gate),
/// admission retries on this cadence as well as on governor changes: a relaunch that
/// frees and re-leases can finish with no governor edge, and the gate has no event.
// derived-or-floor: a floor — far below a relaunch's duration, far above a spin.
const SERVING_BUSY_RETRY: Duration = Duration::from_secs(1);

/// Keep prepared native work queued until the authority changes. Dropping this
/// future cancels the wait; no child or lease exists until admission succeeds.
///
/// Each attempt reads and leases UNDER the serving lifecycle gate (card aae8af55): a
/// relaunch or placement move frees the engine's memory while it holds that gate, so a
/// free-memory read inside its window is refused instead of admitting a trainer nobody
/// called; and nothing is admitted before serving's first plan on this core, when the
/// authority may still read a card with no physical reading as wholly free. The gate is released as soon as the attempt decides; the granted lease is
/// what keeps serving out of the trainer's memory for the run.
pub(crate) async fn wait_for_training_memory(
    daemon: std::sync::Arc<ResourceDaemon>,
    serving: &LifecycleGate,
    consumer: &str,
    bytes: u64,
    waiting: impl FnMut(u64),
) -> Result<LeaseGuard, String> {
    wait_for_training_memory_bound(daemon, serving, consumer, bytes, waiting, || Ok(())).await
}

/// [`wait_for_training_memory`], plus `bind`: run once the lease is granted and BEFORE the
/// serving gate is released, so whatever it establishes is in place before any relaunch can
/// take the gate (SHARED-RESIDENT-LIFECYCLE.md step 1: "establish the residency record inside
/// the existing admission hold, after capacity admission and before dropping that hold").
/// In-engine training binds its run to the engine incarnation here. A `bind` error refuses
/// the admission: the lease is released and the error returned, so a lane replaced while the
/// job waited is never trained on.
pub(crate) async fn wait_for_training_memory_bound(
    daemon: std::sync::Arc<ResourceDaemon>,
    serving: &LifecycleGate,
    consumer: &str,
    bytes: u64,
    mut waiting: impl FnMut(u64),
    bind: impl FnOnce() -> Result<(), String>,
) -> Result<LeaseGuard, String> {
    let mut bind = Some(bind);
    if bytes == 0 {
        return Err("training memory requirement must be measured before admission".into());
    }
    let mut changes = daemon.subscribe();
    let mut last_available = None;
    let mut serving_refusal_said = None;
    let mut retry = tokio::time::interval_at(
        tokio::time::Instant::now() + SERVING_BUSY_RETRY,
        SERVING_BUSY_RETRY,
    );
    retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        // Mark before the attempt so a concurrent release cannot be missed.
        changes.borrow_and_update();
        let hold = match serving.hold_for_admission() {
            Ok(hold) => hold,
            Err(refusal) => {
                if serving_refusal_said != Some(refusal) {
                    crate::probe!(
                        class = "training.admission.serving_busy",
                        consumer = consumer,
                        footprint_bytes = bytes,
                        refusal = refusal.name(),
                        "prepared training waits on the serving lifecycle: free memory now is \
                         a boot or relaunch window, not capacity"
                    );
                    serving_refusal_said = Some(refusal);
                }
                tokio::select! {
                    changed = changes.changed() => {
                        changed.map_err(|_| "training resource authority closed".to_owned())?
                    }
                    _ = retry.tick() => {}
                }
                continue;
            }
        };
        serving_refusal_said = None;
        let attempt = daemon.acquire_guarded(&request(consumer, bytes));
        // bound while the gate is still held; a failed bind drops the lease with the error
        let attempt = match attempt {
            Ok(guard) => match bind.take().map_or(Ok(()), |b| b()) {
                Ok(()) => Ok(guard),
                Err(why) => {
                    drop(guard);
                    drop(hold);
                    return Err(why);
                }
            },
            Err(e) => Err(e),
        };
        drop(hold);
        match attempt {
            Ok(guard) => return Ok(guard),
            Err(LeaseError::InsufficientCapacity { available, .. }) => {
                if last_available != Some(available) {
                    crate::probe!(
                        class = "training.admission.waiting",
                        consumer = consumer,
                        footprint_bytes = bytes,
                        available_bytes = available,
                        "prepared training waits for governed capacity"
                    );
                    waiting(available);
                    last_available = Some(available);
                }
            }
            Err(error) => return Err(format!("training memory admission failed: {error:?}")),
        }
        changes
            .changed()
            .await
            .map_err(|_| "training resource authority closed".to_owned())?;
    }
}

/// The caller retains this guard through child exit/reap. This non-expiring, pinned lease cannot be
/// reclaimed while the child runs: the lifetime of the process, not a timer, owns release.
pub(crate) fn acquire_training_memory(consumer: &str, bytes: u64) -> Result<LeaseGuard, String> {
    if bytes == 0 {
        return Err("training memory requirement must be measured before admission".into());
    }
    let daemon =
        ResourceDaemon::global().ok_or("training requires the resource governor to be running")?;
    let guard = daemon
        .acquire_guarded(&request(consumer, bytes))
        .map_err(|error| format!("training memory admission refused: {error:?}"))?;
    crate::probe!(
        class = "training.admission",
        consumer = consumer,
        footprint_bytes = bytes,
        "governor granted a training lease"
    );
    Ok(guard)
}
