//! Training leases share the resource authority used by live serving.
use crate::resources::{LeaseGuard, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind};

/// The caller retains this guard through child exit/reap. This non-expiring, pinned lease cannot be
/// reclaimed while the child runs: the lifetime of the process, not a timer, owns release.
pub(crate) fn acquire_training_memory(consumer: &str, bytes: u64) -> Result<LeaseGuard, String> {
    if bytes == 0 {
        return Err("training memory requirement must be measured before admission".into());
    }
    let daemon =
        ResourceDaemon::global().ok_or("training requires the resource governor to be running")?;
    let guard = daemon
        .acquire_guarded(&LeaseRequest {
            consumer_id: consumer.to_owned(),
            kind: ResourceKind::Vram,
            bytes,
            // No timer may release accounting while the trainer still owns memory.
            ttl_ms: u64::MAX,
            reclaim_policy: ReclaimPolicy::Pinned,
        })
        .map_err(|error| format!("training memory admission refused: {error:?}"))?;
    crate::probe!(
        class = "training.admission",
        consumer = consumer,
        footprint_bytes = bytes,
        "governor granted a training lease"
    );
    Ok(guard)
}
