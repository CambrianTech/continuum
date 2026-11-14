//! The embed lane's memory on the resource board: each embedding context's lease, and the lane's
//! standing floor, both in the pool the context resides in and both sized from what it embeds.
//!
//! Where a context resides is the backend's placement, never the machine's kind: with any layer
//! offloaded to a GPU it is device memory, otherwise host memory, even on a node with a dedicated
//! card (Joel, 2026-11-10: "It depends on where it's resident"). On unified memory both kinds are
//! views of one pool, so the charge lands on the same bytes either way.
//!
//! The floor keeps serving's plan from growing into the slice recall needs every turn (#225: the
//! grown window left 604 MiB governed-available and embedding went fully dead). It was a fixed
//! 1,792 MiB VRAM bound for a 2,048-token context, reserved by the resolver, which knows neither
//! the context's residence nor her texts. It is now the largest context this lane has embedded,
//! re-stated at each lease in its own pool: the resolver's probe and calibration passes set it
//! before any turn embeds.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::inference::backends::llamacpp::ContextBytes;
use crate::resources::{LeaseError, LeaseGuard, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind};

/// The embed lane's identity on the resource board: the consumer of its leases and its floors.
pub const EMBED_LANE_CONSUMER_ID: &str = "embed";

/// The largest context part this process's embed lane has leased, per pool: its standing floors.
static DEVICE_FLOOR: AtomicU64 = AtomicU64::new(0);
static HOST_FLOOR: AtomicU64 = AtomicU64::new(0);

/// Lease each pool's part of one embedding context of `texts` texts, after raising that pool's
/// floor to it. Empty on a node with no resource authority (the backend's own allocation is the
/// only gate there). The guards hold the bytes until the context is gone.
pub(crate) async fn lease(bytes: ContextBytes, texts: usize) -> Result<Vec<LeaseGuard>, String> {
    let Some(daemon) = ResourceDaemon::global() else { return Ok(Vec::new()) };
    let mut guards = Vec::with_capacity(2);
    for (pool, part, floor) in [(ResourceKind::Vram, bytes.device, &DEVICE_FLOOR), (ResourceKind::Ram, bytes.host, &HOST_FLOOR)] {
        if part == 0 {
            continue;
        }
        let floor = floor.fetch_max(part, Ordering::Relaxed).max(part);
        daemon.reserve(EMBED_LANE_CONSUMER_ID, pool, floor);
        guards.push(lease_part(&daemon, pool, part, floor, texts).await?);
    }
    Ok(guards)
}

/// One pool's lease, retried once when the authority publishes room for it (bounded by one of its
/// own ticks), then refused with the availability it measured, never an allocation that would
/// decode to degenerate zeros.
async fn lease_part(daemon: &std::sync::Arc<ResourceDaemon>, pool: ResourceKind, bytes: u64, floor: u64, texts: usize) -> Result<LeaseGuard, String> {
    // derived-or-floor: a SIGKILL backstop only; the RAII guard frees the bytes on drop, and an
    // embedding finishes in under a second, so the TTL never decides a live lease.
    const LEASE_TTL_MS: u64 = 60_000;
    let request = LeaseRequest {
        consumer_id: EMBED_LANE_CONSUMER_ID.to_string(),
        kind: pool,
        bytes,
        ttl_ms: LEASE_TTL_MS,
        reclaim_policy: ReclaimPolicy::Pinned,
    };
    // Subscribed BEFORE the first attempt, so a release between its refusal and the wait below is
    // a change the wait still sees.
    let mut changes = daemon.subscribe();
    let attempt = |attempt: u8| match daemon.acquire_guarded(&request) {
        Ok(guard) => {
            continuum_base::probe!(
                class = "embed.vram.leased",
                consumer = EMBED_LANE_CONSUMER_ID,
                pool = ?pool,
                bytes = bytes,
                floor = floor,
                texts = texts,
                retried = (attempt > 0),
                "embedding lane leased its context in the pool it resides in"
            );
            Ok(Ok(guard))
        }
        Err(LeaseError::InsufficientCapacity { available, .. }) => {
            continuum_base::probe!(
                class = "embed.vram.refused",
                pool = ?pool,
                requested = bytes,
                available = available,
                attempt = attempt,
                "embedding lease refused: failing loud, never emitting degenerate zeros"
            );
            Ok(Err(available))
        }
        Err(e) => Err(format!("embed: lease error ({e:?}): refusing to embed ungoverned")),
    };
    let leased = match attempt(0)? {
        Ok(guard) => Ok(guard),
        Err(_) => {
            // Woken by the change that makes room. The authority's own tick bounds it: a transient
            // over-commit is relieved within one, so past it the refusal below stands.
            let room = async {
                while daemon.available_for(EMBED_LANE_CONSUMER_ID, pool) < bytes {
                    if changes.changed().await.is_err() {
                        return; // the authority closed: the second attempt reports it
                    }
                }
            };
            let bound = crate::runtime::daemon::Daemon::cadence(daemon.as_ref());
            // Room published or a tick without it: either way the second attempt decides.
            let published = tokio::time::timeout(bound, room).await.is_ok();
            continuum_base::probe!(
                class = "embed.vram.room_wait",
                pool = ?pool,
                requested = bytes,
                published = published,
                bound_ms = bound.as_millis() as u64,
                "a refused embedding lease waited on the authority for room before its second attempt"
            );
            attempt(1)?
        }
    };
    leased.map_err(|available| {
        format!(
            "embed: {pool:?} lease refused twice: {} MiB requested, {} MiB governed-available. Refusing to \
             allocate an embedding context that would decode to degenerate zeros.",
            bytes / (1024 * 1024),
            available / (1024 * 1024),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{CapacitySource, DaemonConfig, MockCapacitySource};
    use std::sync::Arc;

    // what this catches (timer audit row H17, 2026-11-14): a refused embed lease slept a fixed 2 s
    // ("one governor tick") before its one retry, so every recall that met a transient over-commit
    // paid the sleep even when the room came back at once. On a paused clock, the release that makes
    // room must admit the waiting lease at that very instant.
    #[tokio::test(start_paused = true)]
    async fn a_refused_embed_lease_is_granted_by_the_release_that_makes_room() {
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 100));
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let serving = LeaseRequest {
            consumer_id: "serving".to_string(),
            kind: ResourceKind::Vram,
            bytes: 60,
            ttl_ms: u64::MAX,
            reclaim_policy: ReclaimPolicy::Pinned,
        };
        let held = daemon.acquire_guarded(&serving).expect("test: serving leases 60 of 100");
        let waiting = tokio::spawn({
            let daemon = Arc::clone(&daemon);
            async move { lease_part(&daemon, ResourceKind::Vram, 50, 50, 1).await }
        });
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished(), "50 bytes do not fit in the 40 free");
        let released_at = tokio::time::Instant::now();
        drop(held);
        // The bound only turns a missed wake into a failure instead of a hang.
        let leased = tokio::time::timeout(std::time::Duration::from_secs(60), waiting)
            .await
            .expect("test: the release wakes the lease")
            .expect("test: join");
        assert!(leased.is_ok(), "the room the release made is leased: {:?}", leased.err());
        assert_eq!(released_at.elapsed(), std::time::Duration::ZERO, "granted by the release itself, never a timer");
    }
}
