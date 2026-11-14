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
//! from taking the slice recall is using (#225): it is the lane's working set, its weights plus the
//! contexts it is GRANTED, raised by a grant and lowered by that grant's release (nothing holds what
//! it isn't using). It was raised to every REQUEST with `fetch_max`, so one 13,547 MiB request (the
//! M5, 2026-11-14 03:35Z) reserved 13.5 GiB for the rest of the process, refused or not.

use std::fmt;
use std::sync::Arc;

use parking_lot::Mutex;

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

/// One pool's working set: the weights the lane holds, and the bytes of its contexts granted now.
#[derive(Debug, Clone, Copy)]
struct PoolFloor {
    weights: u64,
    held: u64,
}

impl PoolFloor {
    fn bytes(self) -> u64 {
        self.weights.saturating_add(self.held)
    }
}

/// The lane's floors, one per pool, each restated to the board whenever a grant or a release
/// changes it, under the lock that changed it so the board never sees an older value last.
struct LaneFloors {
    device: Mutex<PoolFloor>,
    host: Mutex<PoolFloor>,
}

/// This process's embed lane floors.
static FLOORS: LaneFloors = LaneFloors::new();

impl LaneFloors {
    const fn new() -> Self {
        Self {
            device: parking_lot::const_mutex(PoolFloor { weights: 0, held: 0 }),
            host: parking_lot::const_mutex(PoolFloor { weights: 0, held: 0 }),
        }
    }

    fn pool(&self, pool: ResourceKind) -> &Mutex<PoolFloor> {
        match pool {
            ResourceKind::Vram => &self.device,
            _ => &self.host,
        }
    }

    /// A grant of `part` in `pool`: the floor rises to the weights plus every context held now.
    fn grant(&self, daemon: &ResourceDaemon, pool: ResourceKind, weights: u64, part: u64) {
        let mut floor = self.pool(pool).lock();
        floor.weights = weights;
        floor.held = floor.held.saturating_add(part);
        Self::restate(daemon, pool, *floor);
    }

    /// The release of a granted `part`: the floor falls by it, to what is still held.
    fn release(&self, daemon: &ResourceDaemon, pool: ResourceKind, part: u64) {
        let mut floor = self.pool(pool).lock();
        floor.held = floor.held.saturating_sub(part);
        Self::restate(daemon, pool, *floor);
    }

    fn restate(daemon: &ResourceDaemon, pool: ResourceKind, floor: PoolFloor) {
        daemon.reserve(EMBED_LANE_CONSUMER_ID, pool, floor.bytes());
    }
}

/// One embedding context's leases. Dropping it releases each grant and lowers the floor by it, so
/// it is held as custody until the context is gone.
pub(crate) struct EmbedLease {
    governed: Option<(Arc<ResourceDaemon>, &'static LaneFloors)>,
    parts: Vec<(ResourceKind, u64, LeaseGuard)>,
}

impl Drop for EmbedLease {
    fn drop(&mut self) {
        let Some((daemon, floors)) = &self.governed else { return };
        for (pool, part, guard) in self.parts.drain(..) {
            drop(guard);
            floors.release(daemon, pool, part);
        }
    }
}

/// Lease each pool's part of one embedding context of `texts` texts, within the lane's `need`.
/// Ungoverned (no lease) on a node with no resource authority: the backend's own allocation is the
/// only gate there.
pub(crate) async fn lease(need: &EmbedNeed, bytes: ContextBytes, texts: usize) -> Result<EmbedLease, EmbedLeaseError> {
    match ResourceDaemon::global() {
        Some(daemon) => lease_on(daemon, &FLOORS, need, bytes, texts).await,
        None => Ok(EmbedLease { governed: None, parts: Vec::new() }),
    }
}

async fn lease_on(
    daemon: Arc<ResourceDaemon>,
    floors: &'static LaneFloors,
    need: &EmbedNeed,
    bytes: ContextBytes,
    texts: usize,
) -> Result<EmbedLease, EmbedLeaseError> {
    if bytes.device > need.context.device || bytes.host > need.context.host {
        return Err(EmbedLeaseError::Oversized { requested: bytes, need: need.context, texts });
    }
    let mut lease = EmbedLease { governed: Some((Arc::clone(&daemon), floors)), parts: Vec::with_capacity(2) };
    for (pool, part, weights) in [
        (ResourceKind::Vram, bytes.device, need.weights.device),
        (ResourceKind::Ram, bytes.host, need.weights.host),
    ] {
        if part == 0 {
            continue;
        }
        // A refusal returns here: the floor is untouched, and the pool already granted is released
        // (and its floor lowered) as `lease` drops.
        let guard = lease_part(&daemon, pool, part, texts).await?;
        floors.grant(&daemon, pool, weights, part);
        lease.parts.push((pool, part, guard));
    }
    Ok(lease)
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
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let serving = LeaseRequest {
            consumer_id: "serving".to_string(),
            kind: ResourceKind::Vram,
            bytes: 60,
            ttl_ms: u64::MAX,
            reclaim_policy: ReclaimPolicy::Pinned,
        };
        let held = daemon.acquire_guarded(&serving).expect("test: serving leases 60 of 100");
        // The authority's first tick publishes now, so the lease's next publication is the release.
        tokio::task::yield_now().await;
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

    // The M5's receipts (canary 958ea3056, 2026-11-14): device 55,662,788,608 B
    // (`serving.vram_ceiling_prior.seeded`), the embed lane's 186,908,672 B context
    // (`embed.vram.refused` at 01:22Z), its 14,205,059,072 B request beside 13,915,160,576 B (03:35Z).
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

    fn floors() -> &'static LaneFloors {
        Box::leak(Box::new(LaneFloors::new())) // test: each test owns its floors, never the process's
    }

    // what this catches (dyno, the M5's 03:35Z receipts): the floor ratcheted by a request. Before,
    // `fetch_max` raised it to the 13,547 MiB request whether or not it was granted, so serving's
    // boot sizing kept 13.5 GiB out for the embedder for the rest of the process. Now a request past
    // the window's context is a typed refusal before any reservation, and a capacity refusal leaves
    // the floor where the last grant put it.
    #[tokio::test(start_paused = true)]
    async fn a_refused_request_leaves_the_floor_unchanged() {
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, DEVICE));
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let floors = floors();
        let need = device_need();
        let held = lease_on(Arc::clone(&daemon), floors, &need, ContextBytes { device: CONTEXT, host: 0 }, 1)
            .await
            .expect("test: a short text's context is granted");
        let floor = embed_claim(&daemon);
        assert_eq!(floor, WEIGHTS + CONTEXT, "the working set: weights plus the granted context");
        let oversized = lease_on(Arc::clone(&daemon), floors, &need, ContextBytes { device: OVERSIZED, host: 0 }, 1).await;
        assert!(matches!(oversized, Err(EmbedLeaseError::Oversized { .. })), "past the window: {:?}", oversized.err());
        assert_eq!(embed_claim(&daemon), floor, "an oversized request never reserves");
        let serving = LeaseRequest {
            consumer_id: "serving".to_string(),
            kind: ResourceKind::Vram,
            bytes: daemon.available_for("serving", ResourceKind::Vram),
            ttl_ms: u64::MAX,
            reclaim_policy: ReclaimPolicy::Pinned,
        };
        let _engine = daemon.acquire_guarded(&serving).expect("test: serving takes the rest of the device");
        let refused = lease_on(Arc::clone(&daemon), floors, &need, ContextBytes { device: WINDOW_CONTEXT, host: 0 }, 1).await;
        assert!(matches!(refused, Err(EmbedLeaseError::Refused { .. })), "no room: {:?}", refused.err());
        assert_eq!(embed_claim(&daemon), floor, "a refusal never raises the floor");
        drop(held);
    }

    // what this catches: a floor that only rises. Releasing a granted context lowers the floor to
    // what is still held (the weights), so the embedder never keeps a context's bytes it isn't using.
    // After the fix the lane's whole claim (weights + its widest window) fits beside the 661,410-token
    // engine (38.3 GB of the device at its measured cost, dyno 14) on the M5's 64 GB.
    #[tokio::test(start_paused = true)]
    async fn a_release_lowers_the_floor() {
        let source = Arc::new(MockCapacitySource::new(ResourceKind::Vram, DEVICE));
        let daemon = ResourceDaemon::start(vec![Arc::clone(&source) as Arc<dyn CapacitySource>], vec![], DaemonConfig::default());
        let floors = floors();
        let need = device_need();
        let wide = lease_on(Arc::clone(&daemon), floors, &need, need.context, 4).await.expect("test: the widest window is granted");
        assert_eq!(embed_claim(&daemon), WEIGHTS + WINDOW_CONTEXT);
        const ENGINE_AT_661_410: u64 = 38_300_000_000;
        assert!(ENGINE_AT_661_410 + embed_claim(&daemon) < DEVICE, "the embedder's whole need fits beside the engine");
        drop(wide);
        assert_eq!(embed_claim(&daemon), WEIGHTS, "released: only the weights it still holds");
        let short = lease_on(Arc::clone(&daemon), floors, &need, ContextBytes { device: CONTEXT, host: 0 }, 1).await.expect("test: granted");
        assert_eq!(embed_claim(&daemon), WEIGHTS + CONTEXT, "a smaller grant sets a smaller floor: both ways");
        drop(short);
        assert_eq!(embed_claim(&daemon), WEIGHTS);
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
