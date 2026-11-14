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

/// What the resource authority answered a refused lease's wait with. Only the authority decides
/// it; no elapsed time does (CONTINUUM-ENGINE.md, RTOS rule 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoomOutcome {
    /// The authority's published board covers the need: `available` governed bytes for this
    /// consumer. The one outcome a second attempt runs on.
    Published { available: u64 },
    /// The authority's publisher is gone: nothing will ever publish room again. Terminal.
    AuthorityClosed,
}

/// Wait for the authority to publish room (`room` reads it: `Some(available)` once the board
/// covers the need), or to close. Checked before the first wait, so room published between the
/// refusal and this call is seen. While the authority publishes boards that still fall short, or
/// publishes nothing, this stays pending: the caller's own bound (her act) ends it, never a timer
/// here.
async fn await_room<T>(changes: &mut tokio::sync::watch::Receiver<T>, mut room: impl FnMut() -> Option<u64>) -> RoomOutcome {
    loop {
        // Read OUTSIDE any borrow of the board: the authority publishes while holding its
        // governor lock, which `room` takes.
        if let Some(available) = room() {
            return RoomOutcome::Published { available };
        }
        if changes.changed().await.is_err() {
            return RoomOutcome::AuthorityClosed;
        }
    }
}

/// One pool's lease. Refused, it waits for the authority to publish room ([`await_room`]) and tries
/// once more only then; a closed authority is a refusal. A second refusal (another consumer took
/// the room first) reports the availability it measured, never an allocation that would decode to
/// degenerate zeros.
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
        Err(refused_at) => {
            let waited = std::time::Instant::now();
            let outcome = await_room(&mut changes, || {
                let available = daemon.available_for(EMBED_LANE_CONSUMER_ID, pool);
                (available >= bytes).then_some(available)
            })
            .await;
            continuum_base::probe!(
                class = "embed.vram.room_wait",
                pool = ?pool,
                requested = bytes,
                refused_at = refused_at,
                outcome = ?outcome,
                waited_ms = waited.elapsed().as_millis() as u64,
                "a refused embedding lease waited on the authority's own answer"
            );
            match outcome {
                RoomOutcome::Published { .. } => attempt(1)?,
                RoomOutcome::AuthorityClosed => {
                    return Err(format!(
                        "embed: {pool:?} lease refused ({} MiB requested, {} MiB governed-available) and the \
                         resource authority closed while it waited: refusing to embed ungoverned",
                        bytes / (1024 * 1024),
                        refused_at / (1024 * 1024),
                    ));
                }
            }
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

    // what this catches (Codex on #5991): the wait advanced the lease on elapsed time (a timeout
    // whose expiry still ran the second attempt) and read the authority's closure as "published".
    // Closure is a terminal outcome; boards that still fall short, or no board at all, keep the
    // wait pending however long the clock runs; only a board with room answers Published.
    #[tokio::test(start_paused = true)]
    async fn the_room_wait_answers_only_the_authoritys_publication_or_its_closure() {
        let short = || None;

        let (authority, mut changes) = tokio::sync::watch::channel(());
        drop(authority);
        assert_eq!(await_room(&mut changes, short).await, RoomOutcome::AuthorityClosed, "a closed authority is terminal, never room");

        let (authority, mut changes) = tokio::sync::watch::channel(());
        let silent = await_room(&mut changes, short);
        tokio::pin!(silent);
        assert!(futures::poll!(silent.as_mut()).is_pending());
        tokio::time::advance(std::time::Duration::from_secs(3600)).await;
        assert!(futures::poll!(silent.as_mut()).is_pending(), "a silent authority: no elapsed time answers for it");
        authority.send_replace(());
        assert!(futures::poll!(silent.as_mut()).is_pending(), "a board that still falls short keeps it waiting");

        let room = std::sync::atomic::AtomicU64::new(0);
        let reads = || match room.load(std::sync::atomic::Ordering::SeqCst) {
            0 => None,
            available => Some(available),
        };
        let mut changes = authority.subscribe();
        let waiting = await_room(&mut changes, reads);
        tokio::pin!(waiting);
        assert!(futures::poll!(waiting.as_mut()).is_pending());
        room.store(70, std::sync::atomic::Ordering::SeqCst);
        authority.send_replace(());
        assert_eq!(waiting.await, RoomOutcome::Published { available: 70 }, "the publication with room answers, with its measure");
    }
}
