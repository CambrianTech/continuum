//! The cache lever on the host's deficit path (card 0f631c46): asked first, by its share, and
//! raised within the host's measured headroom when the OS reads normal. The fake engine is shared with the serving dyno.

#![cfg(test)]

use super::*;
use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::llama_server::{LlamaServerError, PromptCacheLimit, PromptCacheRefusal};
use crate::system_resources::memory_pressure::HeldAtPriority;
use async_trait::async_trait;
use std::num::NonZeroU64;

pub(crate) const MIB: u64 = 1 << 20;
/// The M5's launch grant on 2026-11-13 23:40Z: `--cache-ram 16525`.
pub(crate) const M5_GRANT_MIB: u32 = 16_525;

/// An engine's cache route modelled as the fork's: the cache holds `held` bytes (at most its
/// limit, often far less); a new limit evicts what is above it and answers what it holds and
/// freed. Records each limit asked; `gate`, when set, holds every call until a permit is added.
pub(crate) struct FakeEngineCache {
    asked: std::sync::Mutex<Vec<u64>>,
    held: std::sync::Mutex<u64>,
    refusal: Option<PromptCacheRefusal>,
    gate: Option<Arc<tokio::sync::Semaphore>>,
}

impl FakeEngineCache {
    pub(crate) fn holding(held: u64) -> Arc<Self> {
        Arc::new(Self { asked: std::sync::Mutex::new(Vec::new()), held: std::sync::Mutex::new(held), refusal: None, gate: None })
    }
    pub(crate) fn full(grant_mib: u32) -> Arc<Self> {
        Self::holding(u64::from(grant_mib) * MIB)
    }
    fn refusing(refusal: PromptCacheRefusal) -> Arc<Self> {
        Arc::new(Self { asked: std::sync::Mutex::new(Vec::new()), held: std::sync::Mutex::new(0), refusal: Some(refusal), gate: None })
    }
    pub(crate) fn asked(&self) -> Vec<u64> {
        self.asked.lock().unwrap().clone() // JUSTIFIED unwrap: a test mutex
    }
}

#[async_trait]
impl PromptCacheControl for FakeEngineCache {
    fn engine(&self) -> Option<EngineIncarnation> {
        None
    }
    async fn set_limit(&self, limit_mib: NonZeroU64) -> Result<PromptCacheLimit, LlamaServerError> {
        if let Some(gate) = &self.gate {
            gate.acquire().await.unwrap().forget(); // JUSTIFIED unwrap: the test never closes the gate
        }
        self.asked.lock().unwrap().push(limit_mib.get()); // JUSTIFIED unwrap: a test mutex
        if let Some(refusal) = &self.refusal {
            return Err(LlamaServerError::PromptCacheRefused(refusal.clone()));
        }
        let mut held = self.held.lock().unwrap(); // JUSTIFIED unwrap: a test mutex
        let freed = held.saturating_sub(limit_mib.get() * MIB);
        *held -= freed;
        Ok(PromptCacheLimit { held_bytes: *held, freed_bytes: freed })
    }
}

/// A ready engine launched with a host prompt cache of `grant_mib`.
pub(crate) fn serving(grant_mib: u32) -> ServingSnapshot {
    ServingSnapshot {
        loading_model: None,
        held_for: None,
        ready_verified_at_ms: None,
        active_model: Some("qwen3.6-35b-a3b".to_string()),
        ready: true,
        base_url: "http://localhost:0/v1".into(),
        adapters: Vec::new(),
        served_context_window: 262_144,
        lanes: 5,
        host_prompt_cache_mib: grant_mib,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
    }
}

/// The poll the monitor would publish: `deficit` host bytes, the cache's reported bytes at Batch
/// beside `others` at higher priorities, and RAM "used" `used` of `total`.
pub(crate) fn poll(shedder: &PromptCacheShedder, deficit: u64, others: &[HeldAtPriority], (used, total): (u64, u64)) -> PressureSnapshot {
    let mut held_by_priority = vec![HeldAtPriority { priority: PRIORITY, bytes: shedder.report().bytes }];
    held_by_priority.extend_from_slice(others);
    PressureSnapshot { total_bytes: total, available_bytes: total - used, deficit_bytes: deficit, held_by_priority, ..PressureSnapshot::default() }
}

/// A shedder over `cache`, with the senders that play the daemon and the monitor.
pub(crate) fn rig(cache: Arc<FakeEngineCache>, grant_mib: u32) -> (PromptCacheShedder, watch::Sender<ServingSnapshot>, watch::Sender<PressureSnapshot>) {
    let (serving_tx, serving_rx) = watch::channel(serving(grant_mib));
    let (pressure_tx, pressure_rx) = watch::channel(PressureSnapshot::default());
    let shedder = PromptCacheShedder::new(cache, serving_rx, pressure_rx, tokio::runtime::Handle::current());
    (shedder, serving_tx, pressure_tx)
}

async fn run(step: Option<JoinHandle<()>>) {
    step.expect("test: a step was started").await.expect("test: the step ran to the end");
}

const GIB: u64 = 1 << 30;
const QUIET_RAM: (u64, u64) = (0, 64 * GIB);

// what this catches: the cache waiting behind every other holder, or shedding the whole deficit
// instead of its share. A 4 GiB deficit lowers the full 16,525 MiB cache by 4 GiB in one call,
// and the cache at Batch covers the deficit so a mind's results (Interactive) owe nothing.
#[tokio::test]
async fn the_host_deficit_lowers_the_cache_first_by_its_share() {
    let cache = FakeEngineCache::full(M5_GRANT_MIB);
    let (shedder, _serving, pressure) = rig(cache.clone(), M5_GRANT_MIB);
    let minds = [HeldAtPriority { priority: MemoryPriority::Interactive, bytes: 2 * GIB }];
    let snapshot = poll(&shedder, 4 * GIB, &minds, QUIET_RAM);
    assert_eq!(snapshot.deficit_at(MemoryPriority::Interactive), 0, "the cache yields first: nothing left for the minds");
    pressure.send_replace(snapshot);
    run(shedder.shed_now()).await;
    assert_eq!(cache.asked(), vec![u64::from(M5_GRANT_MIB) - 4_096]);
    assert_eq!(shedder.report().bytes, (u64::from(M5_GRANT_MIB) - 4_096) * MIB, "it reports what the engine said it holds");
}

// what this catches: the fixed-fraction RAM gate lowering the cache on a host the OS calls normal,
// fighting the take-again half; and a step with no engine to lower. No OS deficit (RAM 94% "used")
// or no ready engine: the engine is never asked.
#[tokio::test]
async fn only_the_os_deficit_lowers_the_cache() {
    let cache = FakeEngineCache::full(M5_GRANT_MIB);
    let (shedder, serving_tx, pressure) = rig(cache.clone(), M5_GRANT_MIB);
    let snapshot = poll(&shedder, 0, &[], (60 * GIB, 64 * GIB));
    assert!(snapshot.deficit_at(PRIORITY) > 0, "test: the RAM gate alone reads an overrun at Batch");
    pressure.send_replace(snapshot);
    assert!(shedder.shed_now().is_none());
    pressure.send_replace(poll(&shedder, 4 * GIB, &[], QUIET_RAM));
    serving_tx.send_modify(|s| s.ready = false);
    assert!(shedder.shed_now().is_none(), "no engine serving: nothing to lower");
    assert!(cache.asked().is_empty());
}

// what this catches: the cache staying low after the host recovered (the take-again half missing),
// a raise handed to a cache serving never lowered, and a raise past the measured headroom. With
// 1 GiB of room the cache lowered by 4 GiB is raised by 1 GiB, claiming exactly that; a second
// offer of the same room raises nothing, since the 1 GiB it was given is not filled yet.
#[tokio::test]
async fn the_cache_is_raised_by_the_measured_headroom_only_after_it_was_lowered() {
    let room = NonZeroU64::new(GIB).expect("test: nonzero");
    let cache = FakeEngineCache::full(M5_GRANT_MIB);
    let (shedder, _serving, pressure) = rig(cache.clone(), M5_GRANT_MIB);
    assert!(shedder.take_again_now(room).is_none(), "never lowered: nothing to take back");
    pressure.send_replace(poll(&shedder, 4 * GIB, &[], QUIET_RAM));
    run(shedder.shed_now()).await;
    let (step, claimed) = shedder.take_again_now(room).expect("test: a raise within the room");
    step.await.expect("test: the raise ran");
    assert_eq!(claimed, GIB);
    assert_eq!(cache.asked(), vec![u64::from(M5_GRANT_MIB) - 4_096, u64::from(M5_GRANT_MIB) - 3_072]);
    assert!(shedder.take_again_now(room).is_none(), "the room it was given is not filled yet: nothing more");
    let wide = NonZeroU64::new(64 * GIB).expect("test: nonzero");
    let (step, _) = shedder.take_again_now(wide).expect("test: room past the unfilled part");
    step.await.expect("test: the raise ran");
    assert_eq!(cache.asked().last(), Some(&u64::from(M5_GRANT_MIB)), "never past the launch grant");
}

// what this catches: two steps racing on the engine (a lowering and a raise, or two lowerings from
// back-to-back polls), each planned from a limit the other is moving. One step at a time; a poll
// that finds one in flight leaves it, and the next poll can start again.
#[tokio::test]
async fn one_step_on_the_engine_at_a_time() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let cache = Arc::new(FakeEngineCache {
        asked: std::sync::Mutex::new(Vec::new()),
        held: std::sync::Mutex::new(u64::from(M5_GRANT_MIB) * MIB),
        refusal: None,
        gate: Some(gate.clone()),
    });
    let (shedder, _serving, pressure) = rig(cache.clone(), M5_GRANT_MIB);
    pressure.send_replace(poll(&shedder, GIB, &[], QUIET_RAM));
    let first = shedder.shed_now();
    assert!(first.is_some());
    assert!(shedder.shed_now().is_none(), "a step is in flight");
    assert!(shedder.take_again_now(NonZeroU64::new(64 * GIB).expect("test: nonzero")).is_none(), "no raise beside a lowering");
    gate.add_permits(1);
    run(first).await;
    gate.add_permits(1);
    run(shedder.shed_now()).await;
    assert_eq!(cache.asked().len(), 2, "the next poll steps again");
}

// what this catches: a limit far above what the cache holds walking down by the share per poll
// while nothing is freed (limit 16,525 MiB holding 2 GiB, a 1 GiB share: lowering the limit to
// 15,501 MiB frees 0). One step learns what the cache holds from the first reply and sets a second
// limit from it; the next poll plans from what it holds.
#[tokio::test]
async fn a_cache_holding_less_than_its_limit_frees_its_share_in_one_step() {
    let cache = FakeEngineCache::holding(2 * GIB);
    let (shedder, _serving, pressure) = rig(cache.clone(), M5_GRANT_MIB);
    pressure.send_replace(poll(&shedder, GIB, &[], QUIET_RAM));
    run(shedder.shed_now()).await;
    assert_eq!(cache.asked(), vec![u64::from(M5_GRANT_MIB) - 1_024, 1_024], "two calls, never more");
    pressure.send_replace(poll(&shedder, GIB, &[], QUIET_RAM));
    run(shedder.shed_now()).await;
    assert_eq!(cache.asked(), vec![u64::from(M5_GRANT_MIB) - 1_024, 1_024, 1], "planned from the 1 GiB it holds, down to the route's floor");
}

// what this catches: a spent or missing lever wedging the shed path. An engine older than fork
// 9269444f6 refuses (no route), and a later poll asks again; a cache at the route's floor (1 MiB)
// or of unknown size (grant 0, the snapshot's one absence) is never asked.
#[tokio::test]
async fn a_refusal_or_a_spent_cache_is_said_and_never_wedges() {
    let refusing = FakeEngineCache::refusing(PromptCacheRefusal::RouteMissing);
    let (shedder, _serving, pressure) = rig(refusing.clone(), M5_GRANT_MIB);
    pressure.send_replace(poll(&shedder, GIB, &[], QUIET_RAM));
    run(shedder.shed_now()).await;
    run(shedder.shed_now()).await;
    assert_eq!(refusing.asked().len(), 2, "each poll asks again");
    for grant in [1, 0] {
        let cache = FakeEngineCache::full(M5_GRANT_MIB);
        let (shedder, _serving, pressure) = rig(cache.clone(), grant);
        pressure.send_replace(poll(&shedder, GIB, &[], QUIET_RAM));
        if let Some(step) = shedder.shed_now() {
            run(Some(step)).await;
        }
        assert!(cache.asked().is_empty(), "grant {grant}: nothing to lower");
    }
}

// what this catches: a lowering RAISING the cache. Only a different engine (both identities
// known) starts again from its launch grant; an engine the lever cannot tell apart keeps the
// lower of what serving set and its grant.
#[test]
fn only_a_different_engine_starts_again_from_its_launch_grant() {
    use super::lever::{current_limit, LastSet};
    let engine = |pid| Some(EngineIncarnation { pid, started_s: 100, port: 8080 });
    let set = |engine, mib| Some(LastSet { engine, limit_mib: NonZeroU64::new(mib).unwrap(), held_bytes: 0 }); // JUSTIFIED unwrap: test literals are nonzero
    let grant = NonZeroU64::new(9_369).unwrap(); // JUSTIFIED unwrap: a nonzero test literal
    let limit = |last, now| current_limit(last, now, grant).limit_mib.get();
    assert_eq!(limit(set(engine(7), 4_000), engine(7)), 4_000, "the same engine");
    assert_eq!(limit(set(engine(7), 4_000), engine(8)), 9_369, "a relaunched engine");
    assert_eq!(current_limit(set(engine(7), 4_000), engine(8), grant).held_bytes, None, "its contents unknown");
    assert_eq!(limit(set(engine(7), 4_000), None), 4_000, "an unknown engine is never raised");
    assert_eq!(limit(set(engine(7), 12_000), engine(7)), 9_369, "never above the grant");
    assert_eq!(limit(None, engine(7)), 9_369, "nothing set: the launch grant");
}
