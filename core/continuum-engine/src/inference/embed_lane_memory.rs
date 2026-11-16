//! The embed lane's memory on the resource board: each embedding context's lease, and the lane's
//! standing floor, both in the pool the context resides in and both bounded by what the lane needs.
//!
//! Where a context resides is the backend's placement, never the machine's kind: with any layer
//! offloaded to a GPU it is device memory, otherwise host memory, even on a node with a dedicated
//! card (Joel, 2026-11-10: "It depends on where it's resident"). On unified memory both kinds are
//! views of one pool, so the charge lands on the same bytes either way.
//!
//! The lane's need ([`EmbedNeed`]) is its weights and the context its window allows: a request
//! larger than that is a typed refusal before anything is reserved. The floor keeps serving's plan
//! from taking the slice recall uses every turn (#225): it is [`EmbedResidency`], the whole need,
//! reserved while the embedder is loaded and cleared when it is unloaded, never moved by a lease.
//!
//! Two floors came before it. One was raised to every REQUEST with `fetch_max`, so a 13,547 MiB
//! request (the M5, 2026-11-14 03:35Z) reserved 13.5 GiB for the rest of the process (#5996 ended
//! it). The next fell to the weights between embeds, and the engine's pool took the gap: on the M5
//! (canary 7f875ccfd, -c 551865, 2026-11-14 07:52Z) the embed lane's 186,908,672 and 373,817,344 B
//! leases were refused at 0 available 13 times in 24 minutes, 11 recalls lost.

use std::fmt;
use std::sync::Arc;

use crate::inference::backends::llamacpp::ContextBytes;
use crate::resources::{LeaseError, LeaseGuard, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind};

/// The embed lane's identity on the resource board: the consumer of its leases and its floors.
pub const EMBED_LANE_CONSUMER_ID: &str = "embed";

/// What the embed lane holds while it embeds, per pool: its loaded weights, and the context its
/// window allows (`backends::llamacpp::EmbedWindow`), the largest any request can allocate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmbedNeed {
    pub weights: ContextBytes,
    pub context: ContextBytes,
}

/// Why an embedding context was not leased. Each one fails loud: never an allocation that would
/// decode to degenerate zeros.
#[derive(Debug)]
pub(crate) enum EmbedLeaseError {
    /// The request is larger than the context the lane's window allows: a caller that planned past
    /// the window. Refused before anything is reserved.
    Oversized { requested: ContextBytes, need: ContextBytes, texts: usize },
    /// The authority had no room for it, and its next published board still had none.
    Refused { pool: ResourceKind, requested: u64, available: u64 },
    /// The authority had no room for it, and closed while it waited: nothing will publish room.
    AuthorityClosed { pool: ResourceKind, requested: u64, available: u64 },
    /// The authority answered with something other than capacity.
    Ungoverned(LeaseError),
}

impl fmt::Display for EmbedLeaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const MIB: u64 = 1024 * 1024;
        match self {
            Self::Oversized { requested, need, texts } => write!(
                f,
                "embed: a {texts}-text request needs a {} MiB device / {} MiB host context, past the lane's window \
                 ({} / {} MiB): plan its texts in windows",
                requested.device / MIB,
                requested.host / MIB,
                need.device / MIB,
                need.host / MIB,
            ),
            Self::Refused { pool, requested, available } => write!(
                f,
                "embed: {pool:?} lease refused twice: {} MiB requested, {} MiB governed-available. Refusing to \
                 allocate an embedding context that would decode to degenerate zeros.",
                requested / MIB,
                available / MIB,
            ),
            Self::AuthorityClosed { pool, requested, available } => write!(
                f,
                "embed: {pool:?} lease refused ({} MiB requested, {} MiB governed-available) and the resource \
                 authority closed while it waited: refusing to embed ungoverned",
                requested / MIB,
                available / MIB,
            ),
            Self::Ungoverned(e) => write!(f, "embed: lease error ({e:?}): refusing to embed ungoverned"),
        }
    }
}

/// The embed lane's resident claim: its whole need (weights plus one window context) reserved in
/// each pool for as long as the embedder is loaded, so no other claim (the engine's pool, #5993's
/// `room_beside_others`) can take the room its next embed uses. The backend holds it; dropping it,
/// when the embedder unloads, clears the floor. Leases are granted inside it and never move it.
pub(crate) struct EmbedResidency {
    need: EmbedNeed,
    daemon: Option<Arc<ResourceDaemon>>,
}

impl EmbedResidency {
    /// Claim `need` on this process's resource authority; ungoverned (no claim, no lease) on a node
    /// with none, where the backend's own allocation is the only gate.
    pub(crate) fn claim(need: EmbedNeed) -> Self {
        Self::claim_on(ResourceDaemon::global(), need)
    }

    fn claim_on(daemon: Option<Arc<ResourceDaemon>>, need: EmbedNeed) -> Self {
        let residency = Self { need, daemon };
        residency.restate(|floor| floor);
        residency
    }

    /// Each pool's floor, after `of` (the need's bytes, or zero at unload), restated to the board.
    fn restate(&self, of: impl Fn(u64) -> u64) {
        let Some(daemon) = &self.daemon else { return };
        let ContextBytes { device, host } = self.need.weights;
        for (pool, bytes) in [
            (ResourceKind::Vram, device.saturating_add(self.need.context.device)),
            (ResourceKind::Ram, host.saturating_add(self.need.context.host)),
        ] {
            daemon.reserve(EMBED_LANE_CONSUMER_ID, pool, of(bytes));
        }
    }

    /// Lease each pool's part of one embedding context of `texts` texts, inside the resident claim.
    pub(crate) async fn lease(&self, bytes: ContextBytes, texts: usize) -> Result<EmbedLease, EmbedLeaseError> {
        if bytes.device > self.need.context.device || bytes.host > self.need.context.host {
            return Err(EmbedLeaseError::Oversized { requested: bytes, need: self.need.context, texts });
        }
        let Some(daemon) = &self.daemon else { return Ok(EmbedLease { _guards: Vec::new() }) };
        let mut guards = Vec::with_capacity(2);
        for (pool, part) in [(ResourceKind::Vram, bytes.device), (ResourceKind::Ram, bytes.host)] {
            if part == 0 {
                continue;
            }
            // A refusal returns here; a pool already granted is released as `guards` drops.
            guards.push(lease_part(daemon, pool, part, texts).await?);
        }
        Ok(EmbedLease { _guards: guards })
    }
}

impl Drop for EmbedResidency {
    fn drop(&mut self) {
        self.restate(|_| 0);
    }
}

/// One embedding context's leases, held as custody until the context is gone.
pub(crate) struct EmbedLease {
    _guards: Vec<LeaseGuard>,
}

/// What the resource authority answered a refused lease's wait with. Only the authority decides
/// it; no elapsed time does (CONTINUUM-ENGINE.md, RTOS rule 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoomOutcome {
    /// The authority's board covers the need: `available` governed bytes for this consumer. The
    /// one outcome a second attempt runs on.
    Published { available: u64 },
    /// The authority's next published board still falls short: its answer is no. Terminal.
    StillShort { available: u64 },
    /// The authority's publisher is gone: nothing will ever publish room again. Terminal.
    AuthorityClosed,
}

/// The authority's answer to a refused lease: its current board if that already covers the need
/// (room published between the refusal and this call), else its NEXT publication, read for room
/// (`room` measures it: `available`, and whether it covers), or its closure. While the authority
/// publishes nothing this stays pending: the caller's own bound (her act) ends it, never a timer.
async fn await_room<T>(changes: &mut tokio::sync::watch::Receiver<T>, mut room: impl FnMut() -> (u64, bool)) -> RoomOutcome {
    // Read OUTSIDE any borrow of the board: the authority publishes while holding its governor
    // lock, which `room` takes.
    if let (available, true) = room() {
        return RoomOutcome::Published { available };
    }
    if changes.changed().await.is_err() {
        return RoomOutcome::AuthorityClosed;
    }
    match room() {
        (available, true) => RoomOutcome::Published { available },
        (available, false) => RoomOutcome::StillShort { available },
    }
}

/// One pool's lease. Refused, it waits for the authority's answer ([`await_room`]) and tries once
/// more only on published room; a still-short board or a closed authority is a typed refusal.
async fn lease_part(daemon: &Arc<ResourceDaemon>, pool: ResourceKind, bytes: u64, texts: usize) -> Result<LeaseGuard, EmbedLeaseError> {
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
    // Seen as of now: the authority hands out a clone of its stored receiver, which would count
    // every board since its start as new, and the wait would answer from a stale one.
    drop(changes.borrow_and_update());
    let attempt = |attempt: u8| match daemon.acquire_guarded(&request) {
        Ok(guard) => {
            continuum_base::probe!(
                class = "embed.vram.leased",
                consumer = EMBED_LANE_CONSUMER_ID,
                pool = ?pool,
                bytes = bytes,
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
        Err(e) => Err(EmbedLeaseError::Ungoverned(e)),
    };
    let refused_at = match attempt(0)? {
        Ok(guard) => return Ok(guard),
        Err(available) => available,
    };
    let waited = std::time::Instant::now();
    let outcome = await_room(&mut changes, || {
        let available = daemon.available_for(EMBED_LANE_CONSUMER_ID, pool);
        (available, available >= bytes)
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
        RoomOutcome::Published { .. } => attempt(1)?.map_err(|available| EmbedLeaseError::Refused { pool, requested: bytes, available }),
        RoomOutcome::StillShort { available } => Err(EmbedLeaseError::Refused { pool, requested: bytes, available }),
        RoomOutcome::AuthorityClosed => Err(EmbedLeaseError::AuthorityClosed { pool, requested: bytes, available: refused_at }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{CapacitySource, DaemonConfig, MockCapacitySource};

    // what this catches (timer audit row H17, 2026-11-14): a refused embed lease slept a fixed 2 s
    // ("one governor tick") before its one retry, so every recall that met a transient over-commit
    // paid the sleep even when the room came back at once. On a paused clock, the release that makes
    // room must admit the waiting lease at that very instant.
    #[tokio::test(start_paused = true)]
    async fn a_refused_embed_lease_is_granted_by_the_release_that_makes_room() {
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 100));
        // A day between the authority's ticks, and its first tick published before anything is leased:
        // the waiting lease's next publication is then the release, never a tick (whose answer, still
        // short, is a refusal by design).
        let config = DaemonConfig { tick_interval: std::time::Duration::from_secs(24 * 3600), ..DaemonConfig::default() };
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], config);
        let mut board = daemon.subscribe();
        drop(board.borrow_and_update());
        board.changed().await.expect("test: the authority's tick publishes");
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
            async move { lease_part(&daemon, ResourceKind::Vram, 50, 1).await }
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

    // The M5's receipts (canary 958ea3056 and 7f875ccfd, 2026-11-14): device 55,662,788,608 B
    // (`serving.vram_ceiling_prior.seeded`), the embed lane's 186,908,672 B context
    // (`embed.vram.refused`), its 14,205,059,072 B request beside 13,915,160,576 B (03:35Z).
    const DEVICE: u64 = 55_662_788_608;
    const CONTEXT: u64 = 186_908_672;
    const OVERSIZED: u64 = 14_205_059_072;
    const WEIGHTS: u64 = 639_150_592; // Qwen3-Embedding-0.6B Q8_0, ~0.6 GiB
    const WINDOW_CONTEXT: u64 = 1_495_269_376; // what a 2,048-token window allocates on the device

    fn device_need() -> EmbedNeed {
        EmbedNeed { weights: ContextBytes { device: WEIGHTS, host: 0 }, context: ContextBytes { device: WINDOW_CONTEXT, host: 0 } }
    }

    /// The embed lane's claim as serving's boot sizing sees it (`room_beside_others`).
    fn embed_claim(daemon: &ResourceDaemon) -> u64 {
        DEVICE - daemon.room_beside_others("serving", ResourceKind::Vram)
    }

    /// The engine sized into everything the board lets it have.
    fn engine_fills(daemon: &Arc<ResourceDaemon>) -> LeaseGuard {
        let serving = LeaseRequest {
            consumer_id: "serving".to_string(),
            kind: ResourceKind::Vram,
            bytes: daemon.available_for("serving", ResourceKind::Vram),
            ttl_ms: u64::MAX,
            reclaim_policy: ReclaimPolicy::Pinned,
        };
        daemon.acquire_guarded(&serving).expect("test: serving takes every byte it may")
    }

    // what this catches (the M5, canary 7f875ccfd, 2026-11-14 07:52-08:16Z, engine -c 551865): a
    // floor that fell to the weights between embeds. The engine's pool took the idle context's room
    // and the next 186,908,672 B lease was refused at 0 available (13 refusals, 11 recalls lost). The
    // resident claim holds the whole need through the idle gap: with the engine sized to fill the
    // room, an embed after the gap is granted, up to the widest window, and the claim never moves.
    #[tokio::test(start_paused = true)]
    async fn an_embed_after_an_idle_gap_is_granted_beside_an_engine_that_fills_the_room() {
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, DEVICE));
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let residency = EmbedResidency::claim_on(Some(Arc::clone(&daemon)), device_need());
        assert_eq!(embed_claim(&daemon), WEIGHTS + WINDOW_CONTEXT, "resident: weights plus one window context");
        let _engine = engine_fills(&daemon);
        let first = residency.lease(ContextBytes { device: CONTEXT, host: 0 }, 1).await.expect("test: the first embed");
        drop(first); // the idle gap
        assert_eq!(embed_claim(&daemon), WEIGHTS + WINDOW_CONTEXT, "the idle gap keeps the whole need");
        let after = residency.lease(ContextBytes { device: CONTEXT, host: 0 }, 1).await;
        assert!(after.is_ok(), "the embed after the gap is granted: {:?}", after.err());
        drop(after);
        let widest = residency.lease(device_need().context, 4).await;
        assert!(widest.is_ok(), "the widest window fits its own claim: {:?}", widest.err());
        const ENGINE_AT_661_410: u64 = 38_300_000_000; // dyno 14
        assert!(ENGINE_AT_661_410 + embed_claim(&daemon) < DEVICE, "the whole need fits beside the engine");
    }

    // what this catches (dyno, the M5's 03:35Z receipts): a floor moved by a request. A request past
    // the window's context is a typed refusal before any reservation; a capacity refusal (an engine
    // sized before the embedder loaded) leaves the claim where it was.
    #[tokio::test(start_paused = true)]
    async fn a_refused_request_leaves_the_floor_unchanged() {
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, DEVICE));
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let _engine = engine_fills(&daemon);
        let residency = EmbedResidency::claim_on(Some(Arc::clone(&daemon)), device_need());
        let floor = embed_claim(&daemon);
        assert_eq!(floor, WEIGHTS + WINDOW_CONTEXT);
        let oversized = residency.lease(ContextBytes { device: OVERSIZED, host: 0 }, 1).await;
        assert!(matches!(oversized, Err(EmbedLeaseError::Oversized { .. })), "past the window: {:?}", oversized.err());
        assert_eq!(embed_claim(&daemon), floor, "an oversized request never reserves");
        let refused = residency.lease(ContextBytes { device: CONTEXT, host: 0 }, 1).await;
        assert!(refused.is_err(), "the engine took the device before the embedder loaded");
        assert_eq!(embed_claim(&daemon), floor, "a refusal never moves the claim");
    }

    // what this catches: a claim held past the embedder's life (nothing holds what it isn't using).
    // Unloading the embedder clears its floor; a release of a lease does not.
    #[tokio::test(start_paused = true)]
    async fn unloading_the_embedder_clears_its_floor() {
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, DEVICE));
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let residency = EmbedResidency::claim_on(Some(Arc::clone(&daemon)), device_need());
        drop(residency.lease(ContextBytes { device: CONTEXT, host: 0 }, 1).await.expect("test: granted"));
        assert_eq!(embed_claim(&daemon), WEIGHTS + WINDOW_CONTEXT, "a release does not lower it");
        drop(residency);
        assert_eq!(embed_claim(&daemon), 0, "unloaded: no claim");
    }

    // what this catches (Codex on #5991): the wait advanced the lease on elapsed time (a timeout
    // whose expiry still ran the second attempt) and read the authority's closure as "published".
    // Closure is terminal and never room; a silent authority keeps the wait pending however long
    // the clock runs; the next publication answers, with room or without it.
    #[tokio::test(start_paused = true)]
    async fn the_room_wait_answers_only_the_authoritys_publication_or_its_closure() {
        let short = || (40, false);

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
        assert_eq!(silent.await, RoomOutcome::StillShort { available: 40 }, "its next board, still short, is its answer");

        let room = std::sync::atomic::AtomicU64::new(40);
        let reads = || {
            let available = room.load(std::sync::atomic::Ordering::SeqCst);
            (available, available >= 50)
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
