//! Training leases share the resource authority used by live serving.
use crate::resources::{
    LeaseError, LeaseGuard, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind,
};

fn request(consumer: &str, bytes: u64) -> LeaseRequest {
    LeaseRequest {
        consumer_id: consumer.to_owned(),
        kind: ResourceKind::Vram,
        bytes,
        ttl_ms: u64::MAX,
        reclaim_policy: ReclaimPolicy::Pinned,
    }
}

/// Keep prepared native work queued until the authority changes. Dropping this
/// future cancels the wait; no child or lease exists until admission succeeds.
pub(crate) async fn wait_for_training_memory(
    daemon: std::sync::Arc<ResourceDaemon>,
    consumer: &str,
    bytes: u64,
    mut waiting: impl FnMut(u64),
) -> Result<LeaseGuard, String> {
    if bytes == 0 {
        return Err("training memory requirement must be measured before admission".into());
    }
    let mut changes = daemon.subscribe();
    let mut last_available = None;
    loop {
        // Mark before the attempt so a concurrent release cannot be missed.
        changes.borrow_and_update();
        match daemon.acquire_guarded(&request(consumer, bytes)) {
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
