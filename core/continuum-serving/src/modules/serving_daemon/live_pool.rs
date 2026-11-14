//! The pool the minds' grants share FOLLOWS the host's room while the engine serves: lowered by its
//! share of the host's memory deficit, raised back only into the room the OS says it can give.
//!
//! The M5 (64 GB, Qwen3.6-35B-A3B, four residents), canary d6c0dd892, 2026-11-14 09:52Z: 0.2 GB
//! free and the compressor at 29.3 GB and climbing, while the engine's pool (`launch::engine_pool`,
//! fitted to the host's room once, at launch) still offered the minds every token it was launched
//! with. The prompt cache answered the deficit (`serving_prompt_cache`); the minds' KV, the larger
//! holder, answered nothing, so the minds kept growing into memory the host no longer had.
//!
//! The room has one owner: the memory monitor's poll (`HostDeficit`, from the host's own reading).
//! It publishes the deficit on its snapshot and offers the OS's measured headroom to every holder
//! in turn (`MemoryReporter::take_again`). The pool is one more holder on that path, at
//! [`MemoryPriority::Interactive`] (a mind's working state), so the prompt cache
//! ([`MemoryPriority::Batch`]) gives back first and the minds' pool only what is left owed
//! (`PressureSnapshot::deficit_at`). Taking back runs the other way: the pool registers before the
//! cache, so the minds are offered the headroom first.
//!
//! Never below what the minds need: the floor is every mind's need, and a mind whose act is in
//! flight keeps the grant her act began on (`need_floor`). Never above the engine's pool, and never
//! by relaunching it: the engine's `-c` stays, the grants the minds' bindings follow are what move
//! (`grants.rs`), and a new engine (another incarnation: pid and start time) starts whole from its
//! own launch fit.
//!
//! The grants have ONE publisher (`publish_grants`, run when the pool moves and on serving's tick as the
//! waiting minds' heartbeat). The step never publishes
//! grants itself: it publishes the pool it moved to on a watch, and the grant publisher reads that
//! value, so no second call site can race the first with a stale pool.

use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Weak;

use super::*;
use crate::capacity::window_grant::{Demand, PoolGrants};
use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::planning::MIN_SERVE_CTX;
use crate::system_resources::{
    MemoryBudgetSpec, MemoryPressureMonitor, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
    PressureSnapshot,
};

/// The priority the minds' pool gives back at: after every cache, before what runs in real time.
const PRIORITY: MemoryPriority = MemoryPriority::Interactive;
const NAME: &str = "serving.kv_pool";

/// The pool the grants share, as the host's room has moved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum LivePool {
    /// Nothing owed has lowered it: the engine's own pool stands.
    #[default]
    Launched,
    /// Lowered to `tokens` on the engine incarnation `on`. Any other incarnation is a new engine,
    /// which starts whole from its own launch fit.
    Lowered { tokens: u32, on: EngineIncarnation },
}

/// What one step moves the pool against: the engine serving now and its pool, one token's KV bytes
/// as the footprint measured them, and the least the minds' grants hold now (`need_floor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PoolNow {
    pub(super) engine: EngineIncarnation,
    pub(super) served: NonZeroU32,
    pub(super) kv_per_token: NonZeroU64,
    pub(super) floor: u32,
}

/// One poll of the host's room, as this pool answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RoomChange {
    /// The host owes memory; this pool's share of it (`PressureSnapshot::share_of_deficit`).
    Owed(NonZeroU64),
    /// The OS reads itself normal with this much it can give without re-entering pressure.
    Headroom(NonZeroU64),
}

/// A step's result: the next state and the pool before and after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Followed {
    pub(super) next: LivePool,
    pub(super) before: u32,
    pub(super) after: u32,
}

impl Followed {
    /// The host bytes a raise claims of the headroom (none for a lowering).
    pub(super) fn claimed_bytes(self, kv_per_token: NonZeroU64) -> u64 {
        u64::from(self.after.saturating_sub(self.before)) * kv_per_token.get()
    }
}

impl LivePool {
    /// The most the grants may hold on `engine`'s pool of `served`, before any floor. A lowering
    /// holds only on the incarnation it was made on; an unknown engine is never taken for it.
    pub(super) fn ceiling(self, served: u32, engine: Option<EngineIncarnation>) -> u32 {
        match self {
            Self::Lowered { tokens, on } if engine == Some(on) => tokens.min(served),
            Self::Lowered { .. } | Self::Launched => served,
        }
    }

    /// The pool the grants share: the ceiling, never below `floor`, never above the engine's pool.
    pub(super) fn pool(self, served: u32, floor: u32, engine: Option<EngineIncarnation>) -> u32 {
        self.ceiling(served, engine).max(floor).min(served)
    }

    /// Follow one poll of the room: an owed share lowers the pool by the tokens that hold it (rounded
    /// up), never below the floor; headroom raises it by the tokens that fit in it (rounded down),
    /// never above the engine's pool.
    pub(super) fn follow(self, change: RoomChange, now: PoolNow) -> Followed {
        let served = now.served.get();
        let floor = now.floor.min(served);
        let before = self.pool(served, floor, Some(now.engine));
        let after = match change {
            RoomChange::Owed(share) => {
                let owed = share.get().div_ceil(now.kv_per_token.get());
                before - u64::from(before - floor).min(owed) as u32
            }
            RoomChange::Headroom(room) => {
                let fits = room.get() / now.kv_per_token.get();
                before + u64::from(served - before).min(fits) as u32
            }
        };
        let next = match after >= served {
            true => Self::Launched,
            false => Self::Lowered { tokens: after, on: now.engine },
        };
        Followed { next, before, after }
    }
}

/// The least the minds' grants hold now: each mind's need, and a mind whose act is in flight keeps
/// the grant her act began on when it is larger; never less than one prompt's least window.
pub(super) fn need_floor(demands: &[Demand<uuid::Uuid>], granted: Option<&PoolGrants>) -> u32 {
    let held = |d: &Demand<uuid::Uuid>| match d.in_flight {
        true => granted.and_then(|g| g.grant_for(d.mind)).map_or(d.need.get(), |grant| grant.max(d.need.get())), // map_or: an act in flight with no grant on record holds her need
        false => d.need.get(),
    };
    demands.iter().map(held).fold(0, u32::saturating_add).max(MIN_SERVE_CTX)
}

impl ServingDaemonModule {
    /// What the pool steps against now; `None` while no engine incarnation is known, no engine
    /// reports a pool, no model's KV cost is known, or nothing can be granted.
    fn pool_now(&self) -> Option<PoolNow> {
        let engine = self.server.served_incarnation()?;
        let served = NonZeroU32::new(self.served_pool.load(Ordering::Relaxed))?;
        let kv_per_token = self.kv_per_token()?;
        let floor = self.grant_inputs().ok()?.floor;
        Some(PoolNow { engine, served, kv_per_token, floor })
    }

    /// One token's KV bytes on the serving model, from its footprint.
    fn kv_per_token(&self) -> Option<NonZeroU64> {
        let model = self.serving_tx.borrow().active_model.clone()?;
        let model = (self.model_resolver)(&model)?;
        NonZeroU64::new(footprint_for(&model)?.kv_per_token)
    }

    /// The bytes the grants may fill now, before any floor; `None` while no engine reports a pool.
    fn live_pool_bytes(&self) -> Option<u64> {
        let served = self.served_pool.load(Ordering::Relaxed);
        let engine = self.server.served_incarnation();
        let ceiling = self.live_pool.borrow().ceiling(served, engine);
        NonZeroU32::new(ceiling).zip(self.kv_per_token()).map(|(tokens, kv)| u64::from(tokens.get()) * kv.get())
    }

    /// Move the pool by one poll of the room and publish where it moved to; the grant publisher
    /// reads it (`grants.rs`). Returns the step, `None` when there was nothing to step against. The
    /// monitor's poll is the one writer, so the read and the send cannot interleave with another step.
    fn follow_room(&self, change: RoomChange) -> Option<(Followed, PoolNow)> {
        let now = self.pool_now()?;
        let followed = self.live_pool.borrow().follow(change, now);
        self.live_pool.send_if_modified(|live| {
            let moved = *live != followed.next;
            *live = followed.next;
            moved
        });
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.kv_pool.follow",
            change = ?change,
            engine_pool = now.served.get(),
            floor = now.floor,
            pool_before = followed.before,
            pool_after = followed.after,
            kv_per_token = now.kv_per_token.get(),
            engine = ?now.engine,
            "the minds' pool follows the host's room: lowered by its share of the deficit to no less than their needs, raised only into the OS's measured headroom"
        );
        Some((followed, now))
    }

    /// Register the minds' pool, then the engine's host prompt cache, with the memory monitor: the
    /// one reclaim path for host RAM. The cache gives back first (its priority yields first); the
    /// minds are offered the headroom first. Cache steps run on `runtime`.
    pub fn give_host_memory_back_first(self: &Arc<Self>, monitor: &MemoryPressureMonitor, runtime: tokio::runtime::Handle) {
        let pool = KvPoolShedder { module: Arc::downgrade(self), pressure: monitor.subscribe() };
        monitor.add_reporter(Arc::new(pool));
        self.publish_grants_when_the_pool_moves(&runtime);
        self.register_prompt_cache(monitor, runtime);
    }

    /// The grant publisher subscribes to the pool: a step publishes the minds' grants the moment it
    /// lands, not at serving's next tick (RTOS rule 10). The tick still publishes each evaluation
    /// as the waiting minds' heartbeat (`GrantEvaluation::cadence`); it no longer carries a pool
    /// change. Holds the module weakly: the subscription never keeps a stopped daemon alive.
    pub(super) fn publish_grants_when_the_pool_moves(self: &Arc<Self>, runtime: &tokio::runtime::Handle) {
        let module = Arc::downgrade(self);
        let mut moved = self.live_pool.subscribe();
        runtime.spawn(async move {
            while moved.changed().await.is_ok() {
                let Some(module) = module.upgrade() else { break };
                module.publish_grants();
            }
        });
    }
}

/// The minds' pool as a memory reporter on the monitor's poll.
struct KvPoolShedder {
    module: Weak<ServingDaemonModule>,
    pressure: watch::Receiver<PressureSnapshot>,
}

impl KvPoolShedder {
    fn held_bytes(&self) -> Option<u64> {
        self.module.upgrade()?.live_pool_bytes()
    }
}

impl MemoryReporter for KvPoolShedder {
    fn name(&self) -> &'static str {
        NAME
    }

    fn budget(&self) -> MemoryBudgetSpec {
        let held = self.report().bytes;
        MemoryBudgetSpec { name: NAME.to_string(), priority: PRIORITY, min_bytes: 0, preferred_bytes: held, max_bytes: held }
    }

    fn report(&self) -> ModuleMemoryReport {
        let (bytes, detail) = match self.held_bytes() {
            Some(bytes) => (bytes, "the KV the minds' grants may fill, at the footprint's bytes per token".to_string()),
            None => (0, "no engine reports a pool".to_string()),
        };
        ModuleMemoryReport { name: NAME.to_string(), bytes, detail, can_shed: true }
    }

    fn priority(&self) -> MemoryPriority {
        PRIORITY
    }

    fn can_shed(&self) -> bool {
        true
    }

    /// Lower the pool by its share of the host's deficit. Only the OS's own deficit lowers it, as it
    /// alone lowers the cache: the fixed RAM gates read pressure on a host the OS calls normal.
    fn shed_load(&self, _level: PressureLevel) {
        let Some(module) = self.module.upgrade() else { return };
        let Some(held) = module.live_pool_bytes() else { return };
        let share = {
            let snapshot = self.pressure.borrow();
            match snapshot.deficit_bytes {
                0 => None,
                _ => NonZeroU64::new(snapshot.share_of_deficit(PRIORITY, held)),
            }
        };
        if let Some(share) = share {
            module.follow_room(RoomChange::Owed(share));
        }
    }

    /// Raise the pool by at most the OS's measured headroom; the bytes claimed come off what the
    /// monitor offers the next holder.
    fn take_again(&self, headroom: NonZeroU64) -> u64 {
        let Some(module) = self.module.upgrade() else { return 0 };
        if *module.live_pool.borrow() == LivePool::Launched {
            return 0;
        }
        match module.follow_room(RoomChange::Headroom(headroom)) {
            Some((followed, now)) => followed.claimed_bytes(now.kv_per_token),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::planning::window_allocator::Tokens;
    use crate::system_resources::memory_pressure::{HostDeficit, HostDeficitReading};
    use continuum_host::host_memory::{HostMemoryReading, OsMemoryPressure};

    const GB: u64 = 1_000_000_000;
    /// The M5's measured KV cost, `state/lane-footprint.json`, 2026-11-14 00:57Z.
    const M5_KV_PER_TOKEN: u64 = 15_163;
    /// The M5's free target: `vm.vm_page_free_target` 4,000 pages of 16 KiB (2026-11-13).
    const M5_FREE_TARGET: u64 = 4_000 * 16_384;
    const ENGINE: EngineIncarnation = EngineIncarnation { pid: 4_242, started_s: 1_794_000_000, port: 8_090 };

    fn t(n: u32) -> Tokens {
        Tokens::new(n).expect("test: a nonzero size") // expect: every size here is a positive literal
    }

    fn reading(available: u64, compressed: u64, pressure: OsMemoryPressure) -> HostMemoryReading {
        HostMemoryReading {
            free_bytes: available,
            reclaimable_bytes: 0,
            compressed_bytes: compressed,
            swapped_in_bytes: 0,
            swapped_out_bytes: 0,
            free_target_bytes: M5_FREE_TARGET,
            pressure: Some(pressure),
        }
    }

    fn demand(mind: u128, need: u32, in_flight: bool) -> Demand<uuid::Uuid> {
        Demand { mind: uuid::Uuid::from_u128(mind), need: t(need), target: t(need), wish: t(need), in_flight, settled: true, ceiling: 262_144 }
    }

    // what this catches (the M5, 2026-11-14 09:52Z: 0.2 GB free, compressor 29.3 GB and climbing,
    // and the minds' pool never moved): a pool that keeps its launch size while the host owes
    // memory, one lowered below what the minds need or what an act in flight began on, and one taken
    // back in a jump past the headroom the OS measured. A fake sequence of the host's own readings
    // drives the monitor's real deficit owner; the pool follows every poll.
    #[test]
    fn the_pool_follows_the_room_down_to_the_minds_needs_and_back_only_into_headroom() {
        let kv = NonZeroU64::new(M5_KV_PER_TOKEN).expect("test: a nonzero cost"); // expect: a positive literal
        let served = NonZeroU32::new(480_430).expect("test: the M5's pool after #5989"); // expect: a positive literal
        let (her, other) = (1, 2);
        let granted = PoolGrants { pool_tokens: served.get(), per_sequence_cap: 262_144, by_mind: [(uuid::Uuid::from_u128(her), 140_000)].into(), engine: None, revisions: Default::default() };
        let demands = [demand(her, 100_000, true), demand(other, 60_000, false)];
        let floor = need_floor(&demands, Some(&granted));
        assert_eq!(floor, 140_000 + 60_000, "her act in flight keeps the grant it began on; the other mind keeps her need");
        let now = PoolNow { engine: ENGINE, served, kv_per_token: kv, floor };

        let mut host = HostDeficit::default();
        let mut live = LivePool::default();
        let mut pools = Vec::new();
        // calm with room, pressure building to 09:52Z, held there, then the host recovering
        let polls = [
            reading(20 * GB, 2 * GB, OsMemoryPressure::Normal),
            reading(GB, 12 * GB, OsMemoryPressure::Warn),
            reading(GB / 5, 29_300_000_000, OsMemoryPressure::Critical),
            reading(GB / 5, 29_300_000_000, OsMemoryPressure::Critical),
            reading(2 * GB, 20 * GB, OsMemoryPressure::Normal),
            reading(30 * GB, 3 * GB, OsMemoryPressure::Normal),
        ];
        for poll in &polls {
            let change = match host.observe(Some(poll), 0) {
                HostDeficitReading::Owed(owed) => Some(RoomChange::Owed(owed)),
                verdict => verdict.headroom().map(RoomChange::Headroom),
            };
            let Some(change) = change else {
                pools.push(live.pool(served.get(), floor, Some(ENGINE)));
                continue;
            };
            let followed = live.follow(change, now);
            assert!(followed.after >= floor, "never below what the minds need: {followed:?}");
            assert!(followed.after <= served.get(), "never above the engine's pool: {followed:?}");
            match change {
                RoomChange::Owed(_) => assert!(followed.after <= followed.before, "an owed poll never raises"),
                RoomChange::Headroom(room) => assert!(followed.claimed_bytes(kv) <= room.get(), "a raise claims no more than the headroom"),
            }
            live = followed.next;
            pools.push(followed.after);
        }
        assert_eq!(pools[0], served.get(), "calm with room: the launched pool stands");
        assert!(pools[1] < pools[0], "the host owes memory: the pool falls");
        assert_eq!(pools[3], floor, "held starvation: down to the minds' needs, and no lower");
        assert!(pools[4] > floor && pools[4] < served.get(), "the OS reads normal with ~2 GB: back only by what fits in it ({pools:?})");
        assert_eq!(pools[5], served.get(), "room returned: the whole launched pool, back to Launched");
        assert_eq!(live, LivePool::Launched);
    }

    // what this catches: a lowering carried onto a replacement engine (its pool is its own launch
    // fit, even at the same size), an unknown engine read as the lowered one, a floor above the
    // engine's pool read as a pool larger than the engine, and an empty roster lowering it to nothing.
    #[test]
    fn a_replacement_engine_starts_whole_and_the_pool_never_leaves_the_engines_bounds() {
        let lowered = LivePool::Lowered { tokens: 200_000, on: ENGINE };
        let replacement = EngineIncarnation { pid: ENGINE.pid + 1, started_s: ENGINE.started_s + 60, ..ENGINE };
        assert_eq!(lowered.pool(480_430, 0, Some(ENGINE)), 200_000);
        assert_eq!(lowered.pool(480_430, 0, Some(replacement)), 480_430, "a replacement at the same pool size starts whole");
        assert_eq!(lowered.pool(480_430, 0, None), 480_430, "an unknown engine is never taken for the lowered one");
        assert_eq!(lowered.pool(150_000, 0, Some(ENGINE)), 150_000, "never above the engine's pool");
        assert_eq!(lowered.pool(480_430, 900_000, Some(ENGINE)), 480_430, "a floor past the engine is the engine's pool");
        assert_eq!(need_floor(&[], None), MIN_SERVE_CTX, "no mind measured: one prompt's least window");
    }
}
