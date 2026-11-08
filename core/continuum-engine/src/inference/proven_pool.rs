//! The largest KV pool each model ran HEALTHY at on this node (controller card 8a983e31), kept
//! across core restarts through the one saved-state contract (`state/proven-pools.json`).
//!
//! A relaunch from a gap reading (an engine that just exited reads 0 bytes) floors its pool here
//! instead of at what the gap leaves (#5835). Before, the record was a map in the serving daemon's
//! memory keyed by model alone: a core restart onto a dead engine found it empty (Fable on #5835),
//! and a pool proven under one engine build or KV layout stood for another (#5838 pattern 3: a
//! last-good reading counts only under the engine and config it was taken on).
//!
//! The record holds under a [`PoolContract`]: the engine build that will serve and the resolved
//! KV plan (cache type, flash attention, backend), read from their typed owners, never from a
//! command line. A changed contract is a new world: the saved record loads `Diverged`, and the
//! node proves its pools again from nothing (growing toward a pool proven elsewhere is an upward
//! trial, the controller's to schedule, never an adoption).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use continuum_saved_state::{OwnedFile, Rebuildable, Retention, SavedState, Singleton, Store};

/// What a proven pool holds under: the facts that change what a token of KV costs or whether the
/// engine that serves is the one that ran it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PoolContract {
    /// The engine build (`<commit>:<backend>` stamp of its slot); `None` outside the slots.
    pub engine: Option<String>,
    /// The resolved KV cache type (`f16`, `q8_0`, …).
    pub cache_type: String,
    pub flash_attn: bool,
    /// The backend the KV plan was decided against.
    pub backend: crate::inference::kv_cache_plan::ServingBackend,
}

impl PoolContract {
    /// The contract the NEXT launch serves under: the current engine slot's build and the KV plan
    /// as resolved now.
    pub fn live() -> Self {
        let kv = crate::inference::kv_cache_plan::resolve();
        PoolContract {
            engine: next_engine_build(),
            cache_type: kv.cache_type,
            flash_attn: kv.flash_attn,
            backend: kv.backend,
        }
    }

    /// The contract the RUNNING engine serves under: the live lane's engine build (an engine
    /// installed since its launch is not the one that proved this pool) and the KV plan.
    pub fn running() -> Self {
        PoolContract { engine: crate::inference::engine_slots::live_lane_build(), ..Self::live() }
    }
}

/// The build of the engine the NEXT launch runs: the current slot's stamp (`engine_slots`).
/// `None` when the slots are unreadable or none is current.
fn next_engine_build() -> Option<String> {
    use crate::inference::engine_slots::{current_slot, root, slot_stamp};
    let root = root(&crate::paths::continuum_home().ok()?).ok()?;
    slot_stamp(&root, current_slot(&root)?)
}

/// Every model's proven pool under one contract.
#[derive(Debug, Clone, PartialEq)]
struct Proven {
    contract: PoolContract,
    pools: BTreeMap<String, u64>,
}

impl Proven {
    /// The largest pool `model` ran at under `contract`; `None` under any other or before any run.
    fn of(&self, model: &str, contract: &PoolContract) -> Option<u64> {
        (&self.contract == contract).then(|| self.pools.get(model).copied()).flatten()
    }

    /// Record `pool` for `model` under `contract`, raising its proof; a new contract starts empty.
    /// `true` when the record changed.
    fn raise(&mut self, model: &str, pool: u64, contract: PoolContract) -> bool {
        if self.contract != contract {
            *self = Proven { contract, pools: BTreeMap::new() };
        }
        let held = self.pools.entry(model.to_string()).or_insert(0);
        let raised = pool > *held;
        *held = (*held).max(pool);
        raised
    }
}

struct ProvenPools;
impl SavedState for ProvenPools {
    const KIND: &'static str = "proven-pools";
    const FORMAT: u32 = 1;
    const RETENTION: Retention = Retention::PerKey { bound: "one file per node, one record per model it has served" };
    type Key = Singleton;
    type Contract = PoolContract;
    type Value = BTreeMap<String, u64>;
    type Medium = OwnedFile;
}
/// A proof is re-measured by the next healthy run: a node with none starts from nothing.
impl Rebuildable for ProvenPools {}

static STORE: LazyLock<Store<ProvenPools>> = LazyLock::new(crate::inference::node_state::store);

static PROVEN: LazyLock<parking_lot::Mutex<Proven>> = LazyLock::new(|| {
    let contract = PoolContract::live();
    // The trunk probes every verdict (`state.load.<outcome>`); anything but a match under this
    // contract is a node with nothing proven yet.
    let pools = STORE.load_rebuildable(&Singleton, &contract);
    parking_lot::Mutex::new(Proven { contract, pools })
});

/// The largest pool `model` ran at under the contract the next launch serves on; `None` when none.
pub fn proven(model: &str) -> Option<u64> {
    PROVEN.lock().of(model, &PoolContract::live())
}

/// `model` served healthy on a pool of `pool` tokens under `contract` (the running engine's):
/// raised and saved when it is larger than the proof.
pub fn observe(model: &str, pool: u64, contract: PoolContract) {
    let saved = {
        let mut proven = PROVEN.lock();
        proven.raise(model, pool, contract).then(|| proven.clone())
    };
    if let Some(proven) = saved {
        // Written outside the lock; a failed save is the trunk's to probe and retry on the next raise.
        STORE.save_rebuildable(&Singleton, &proven.contract, &proven.pools);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(engine: &str, cache_type: &str) -> PoolContract {
        PoolContract { engine: Some(engine.into()), cache_type: cache_type.into(), flash_attn: false, backend: crate::inference::kv_cache_plan::ServingBackend::Cpu }
    }

    // what this catches: a core restart forgetting the pool a model ran at (Fable on #5835: a
    // restart onto a dead engine floored the relaunch at the gap reading), and any other file read
    // as proven: every outcome of the saved-state contract for this kind, a changed engine build
    // or KV type loading as Diverged.
    #[test]
    fn a_proven_pool_survives_a_restart_and_no_other_contract_inherits_it() {
        let Ok(dir) = tempfile::tempdir() else { panic!("a temp dir") };
        let pools = BTreeMap::from([("m".to_string(), 152_064)]);
        continuum_saved_state::round_trip::every_outcome::<ProvenPools>(
            dir.path(),
            Singleton,
            contract("abc:cpu", "f16"),
            Some(contract("def:cpu", "f16")),
            pools,
        );
    }

    // what this catches: a pool proven under one engine build or KV layout standing for another
    // (#5838 pattern 3), and a proof that ever moves down within one contract.
    #[test]
    fn a_proof_holds_only_under_its_contract_and_only_rises() {
        let old = contract("abc:cpu", "f16");
        let mut proven = Proven { contract: old.clone(), pools: BTreeMap::new() };
        assert!(proven.raise("m", 100_000, old.clone()));
        assert!(!proven.raise("m", 60_000, old.clone()), "a smaller healthy pool never lowers the proof");
        assert_eq!(proven.of("m", &old), Some(100_000));
        assert_eq!(proven.of("other", &old), None);
        let other_backend = PoolContract { backend: crate::inference::kv_cache_plan::ServingBackend::Metal, ..old.clone() };
        for changed in [contract("def:cpu", "f16"), contract("abc:cpu", "q8_0"), other_backend] {
            assert_eq!(proven.of("m", &changed), None, "{changed:?} inherits nothing");
        }
        let new = contract("def:cpu", "f16");
        assert!(proven.raise("m", 50_000, new.clone()));
        assert_eq!((proven.of("m", &new), proven.of("m", &old)), (Some(50_000), None), "a new contract starts empty");
    }
}
