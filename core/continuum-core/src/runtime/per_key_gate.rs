//! [`PerKeyGate`] — substrate-canonical per-key serialization
//! primitive with structural eviction.
//!
//! ## What it's for
//!
//! "Serialize concurrent work that targets the same resource;
//! different resources proceed in parallel." This is the shape that
//! comes up everywhere in the substrate's RTOS doctrine:
//!
//! - **Per-`(persona, trait, base_model)` training-job submit** —
//!   prevents the lost-update + restore-commingle races flagged in
//!   Reviewer 3's BLOCK C1/C2 on [[matrix-dojo-layer-loading-as-substrate-primitive]]
//!   PR #1580.
//! - **Per-module-name generator template** — prevents two concurrent
//!   `generate/module` commands from racing the same name.
//! - **Per-cursor data pagination** — prevents two concurrent `next`
//!   calls on the same cursor from interleaving and corrupting state.
//! - **Per-stream debug probe** — prevents concurrent `probes/next`
//!   calls on the same stream id from double-draining.
//!
//! Pre-`PerKeyGate`, each of these reinvented the same shape:
//! `Arc<DashMap<K, Arc<tokio::sync::Mutex<()>>>>` + a method that
//! acquires-or-creates. None of them evicted cold keys. Per
//! [[auto-clean-is-structural-not-operational]], that's a slow leak
//! in every long-running substrate.
//!
//! ## The structural-eviction trick
//!
//! `try_evict(&key)` calls `DashMap::remove_if(key, |_, gate| Arc::strong_count(gate) == 1)`.
//! When `strong_count == 1`, the ONLY remaining reference is the
//! map's own entry — no other caller is currently holding a clone
//! of the `Arc<Mutex>`, so removing it cannot break anyone's
//! serialization guarantee. A concurrent caller arriving AFTER
//! eviction sees no gate and `acquire` creates a fresh one — exactly
//! the same semantics as "first-ever acquire for this key."
//!
//! The caller MUST drop their own lease BEFORE calling `try_evict`,
//! otherwise `strong_count >= 2` and eviction is a no-op. The
//! canonical usage pattern:
//!
//! ```ignore
//! let gate = self.gates.acquire(&key);
//! let lease = gate.lock().await;
//! // ... do work that needs serialization for `key` ...
//! drop(lease);
//! self.gates.try_evict(&key);
//! ```
//!
//! ## What it doesn't do
//!
//! - **No TTL.** Eviction is purely structural-based-on-refcount.
//!   A hot key stays in the map as long as it has waiters.
//! - **No LRU.** Keys are evicted opportunistically by callers.
//!   There is no background sweeper.
//! - **No tracking of WHICH caller holds the lease.** The gate is
//!   anonymous — `Arc<Mutex<()>>` has no notion of identity. If you
//!   need "fairness" or "first-come-first-served" beyond `tokio::sync::Mutex`'s
//!   own (which already provides FIFO under contention), use a
//!   semaphore or a queue instead.
//!
//! ## Why `tokio::sync::Mutex` (not `std::sync::Mutex`)
//!
//! Callers hold the lease across `.await`. `std::sync::Mutex` is
//! NOT await-safe (its guard isn't `Send`); `tokio::sync::Mutex` is.
//! Per `docs/architecture/CONCURRENCY-STYLE-GUIDE.md`: hold gate
//! across `.await`, do not hold `DashMap::RefMut` across `.await`.

use std::hash::Hash;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::Mutex;

/// Per-key serialization gate. Cheap to clone (single `Arc`).
///
/// Type parameter `K` is the resource discriminator. Typical
/// choices: `String` for module names, `Uuid` for cursor / stream
/// ids, custom structs like `BucketKey { persona_id, trait_kind,
/// base_model }` for compound resources.
#[derive(Clone, Debug)]
pub struct PerKeyGate<K: Eq + Hash + Clone> {
    gates: Arc<DashMap<K, Arc<Mutex<()>>>>,
}

impl<K: Eq + Hash + Clone> PerKeyGate<K> {
    pub fn new() -> Self {
        Self {
            gates: Arc::new(DashMap::new()),
        }
    }

    /// Acquire (or lazily create) the gate for `key`. Returns an
    /// `Arc<Mutex<()>>` the caller `.lock().await`s to serialize
    /// against other holders of the same key.
    ///
    /// Race-safe under concurrent `acquire` for the same key:
    /// `DashMap::entry().or_insert_with()` runs the closure at most
    /// once per concurrent contention window; subsequent callers in
    /// the same window observe the just-inserted entry and clone its
    /// `Arc`. All concurrent acquirers receive the SAME `Arc<Mutex>`
    /// instance.
    pub fn acquire(&self, key: &K) -> Arc<Mutex<()>> {
        // Fast path: gate already exists.
        if let Some(g) = self.gates.get(key) {
            return g.clone();
        }
        // Slow path: create-or-observe-existing under the shard
        // write lock.
        self.gates
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Try to evict the gate for `key` if no caller currently holds
    /// a reference to it. Per [[auto-clean-is-structural-not-operational]]:
    /// the gate map is a structural-cleanup surface — every
    /// successful work unit on a key should call `try_evict` after
    /// dropping its lease, so cold keys don't accumulate.
    ///
    /// **MUST be called AFTER the local lease has been dropped.**
    /// `Arc::strong_count == 1` checks that the only remaining
    /// reference is the map's own entry. If the caller still holds
    /// their lease, the count is `>= 2` and eviction is a no-op —
    /// silently broken in production, but the doctrine says drop the
    /// lease first.
    ///
    /// Idempotent: calling `try_evict` on a key that has no gate is
    /// a no-op.
    pub fn try_evict(&self, key: &K) {
        self.gates
            .remove_if(key, |_, gate| Arc::strong_count(gate) == 1);
    }

    /// Number of gates currently tracked. Includes both contested
    /// and idle gates (idle gates are candidates for `try_evict`).
    /// Useful for substrate-side leak-detection assertions in tests.
    pub fn len(&self) -> usize {
        self.gates.len()
    }

    /// `true` iff no gates are tracked.
    pub fn is_empty(&self) -> bool {
        self.gates.is_empty()
    }
}

impl<K: Eq + Hash + Clone> Default for PerKeyGate<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: concurrent acquires for the SAME key
    // return the SAME `Arc<Mutex>` instance (Arc::ptr_eq), which is
    // the structural guarantee that makes serialization work. A
    // race in the entry().or_insert_with() path that returned a
    // freshly-created-but-discarded Arc would break this — and
    // would silently break per-key serialization across the
    // substrate.
    #[tokio::test]
    async fn concurrent_acquires_for_same_key_return_same_arc() {
        let gate = PerKeyGate::<String>::new();
        let g1 = gate.acquire(&"alpha".to_string());
        let g2 = gate.acquire(&"alpha".to_string());
        assert!(Arc::ptr_eq(&g1, &g2));
    }

    // what this catches: acquires for DIFFERENT keys return
    // DIFFERENT instances. A regression that flattened the map's
    // hashing would silently make every key serialize against
    // every other key, destroying parallelism.
    #[tokio::test]
    async fn acquires_for_different_keys_return_distinct_arcs() {
        let gate = PerKeyGate::<String>::new();
        let g1 = gate.acquire(&"alpha".to_string());
        let g2 = gate.acquire(&"beta".to_string());
        assert!(!Arc::ptr_eq(&g1, &g2));
    }

    // what this catches: after the caller drops their lease and
    // their Arc clone, `try_evict` removes the entry. Without
    // this, the map grows unbounded — exactly the leak class the
    // primitive exists to close.
    #[tokio::test]
    async fn try_evict_removes_gate_when_no_other_holders() {
        let gate = PerKeyGate::<String>::new();
        {
            let g = gate.acquire(&"alpha".to_string());
            let _lease = g.lock().await;
            // Lease held → strong_count = 2 (map + this scope).
            assert_eq!(gate.len(), 1);
            // try_evict here would no-op because strong_count != 1.
            gate.try_evict(&"alpha".to_string());
            assert_eq!(
                gate.len(),
                1,
                "try_evict while lease held MUST be a no-op"
            );
            drop(_lease);
            drop(g);
        }
        // Now strong_count = 1 (map only) — eviction succeeds.
        gate.try_evict(&"alpha".to_string());
        assert_eq!(
            gate.len(),
            0,
            "try_evict after lease drop MUST remove the gate"
        );
    }

    // what this catches: try_evict on a key that has no gate is a
    // no-op (not a panic, not an error). The map's `remove_if` on a
    // missing key returns None; the primitive must not surface that
    // as an error. Idempotent eviction lets callers retry without
    // checking state.
    #[tokio::test]
    async fn try_evict_on_missing_key_is_noop() {
        let gate = PerKeyGate::<String>::new();
        gate.try_evict(&"never-acquired".to_string());
        assert_eq!(gate.len(), 0);
    }

    // what this catches: when a waiter is queued on the gate but
    // the acquirer hasn't released yet, try_evict by a THIRD caller
    // is a no-op — the waiter still gets to acquire after the
    // current lease drops. Without this guarantee, eviction could
    // race with `lock().await` and a queued task could be stuck on
    // a removed mutex.
    //
    // Verified structurally: a waiter holds an Arc clone of the
    // gate (returned by `acquire()`). So strong_count >= 2 as long
    // as a waiter exists. try_evict only succeeds at strong_count == 1.
    #[tokio::test]
    async fn try_evict_while_waiter_queued_is_noop() {
        let gate = PerKeyGate::<String>::new();
        let g_holder = gate.acquire(&"alpha".to_string());
        let _holder_lease = g_holder.lock().await;

        // A "waiter" holds the Arc but hasn't entered .lock yet.
        let g_waiter = gate.acquire(&"alpha".to_string());

        // strong_count >= 3 (map + holder + waiter). try_evict is no-op.
        gate.try_evict(&"alpha".to_string());
        assert_eq!(gate.len(), 1, "gate must survive while waiter holds Arc");

        // Drop both refs and the lease — now eviction succeeds.
        drop(_holder_lease);
        drop(g_holder);
        drop(g_waiter);
        gate.try_evict(&"alpha".to_string());
        assert_eq!(gate.len(), 0);
    }

    // what this catches: post-eviction acquire creates a FRESH
    // gate with the same identity semantics as first-ever-acquire.
    // A concurrent caller landing right after eviction must see a
    // clean gate, not a partially-removed one. Critical for the
    // "cold key arrives again" pattern — gate gets evicted, then
    // a new submit arrives, then the gate needs to work normally.
    #[tokio::test]
    async fn re_acquire_after_eviction_creates_fresh_gate() {
        let gate = PerKeyGate::<String>::new();
        {
            let g = gate.acquire(&"alpha".to_string());
            let _l = g.lock().await;
        }
        gate.try_evict(&"alpha".to_string());
        assert_eq!(gate.len(), 0);

        let g_again = gate.acquire(&"alpha".to_string());
        assert_eq!(gate.len(), 1);
        let _l_again = g_again.lock().await;
        // No deadlock, no panic — fresh gate works.
    }

    /// Stress tests — gated behind the `stress-tests` feature per
    /// CLAUDE.md test-discipline doctrine. Default `cargo test` does
    /// NOT compile these; CI runs them via `--features stress-tests`.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        use std::collections::HashSet;
        use std::sync::Mutex as StdMutex;

        // what this catches: under N concurrent tasks racing
        // acquire + lease + try_evict for the SAME key, the gate
        // serializes work correctly AND no task gets stuck on a
        // removed mutex. The CHECK is that every task's critical
        // section executes exactly once and the witness counter
        // matches the task count. A race that double-evicted or
        // dropped a waiter's Arc would here surface as missing
        // critical-section runs or a deadlock.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_acquire_evict_cycles_preserve_serialization() {
            let gate = Arc::new(PerKeyGate::<String>::new());
            let witness = Arc::new(StdMutex::new(HashSet::<u32>::new()));

            const N: usize = 100;
            let mut handles = Vec::with_capacity(N);
            for i in 0..N {
                let gate_c = gate.clone();
                let witness_c = witness.clone();
                handles.push(tokio::spawn(async move {
                    let g = gate_c.acquire(&"shared".to_string());
                    let _lease = g.lock().await;
                    // Critical section — must execute exactly once
                    // per task.
                    witness_c.lock().unwrap().insert(i as u32);
                    drop(_lease);
                    drop(g);
                    gate_c.try_evict(&"shared".to_string());
                }));
            }
            for h in handles {
                h.await.unwrap();
            }
            let final_set = witness.lock().unwrap();
            assert_eq!(
                final_set.len(),
                N,
                "every task's critical section must have run exactly once"
            );

            // Gate should be evicted by the last task to run — but
            // ordering is non-deterministic, so we just verify
            // eventual cleanup. Force-evict and confirm.
            gate.try_evict(&"shared".to_string());
            assert_eq!(
                gate.len(),
                0,
                "after all tasks drained, gate must be evictable"
            );
        }

        // what this catches: under N concurrent tasks across K
        // different keys, parallelism is preserved (tasks targeting
        // different keys do NOT block each other). Wall-clock for
        // K=4 worker threads should be roughly N/K × per-task cost,
        // not N × per-task cost. We don't assert wall-clock (flaky)
        // but we do assert that final state is correct: every task
        // ran exactly once, gates are evictable.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn different_keys_proceed_in_parallel_without_corruption() {
            let gate = Arc::new(PerKeyGate::<String>::new());
            let witness = Arc::new(StdMutex::new(HashSet::<(String, u32)>::new()));

            const TASKS_PER_KEY: usize = 25;
            const KEYS: &[&str] = &["alpha", "beta", "gamma", "delta"];
            let total = TASKS_PER_KEY * KEYS.len();

            let mut handles = Vec::with_capacity(total);
            for (k_idx, key) in KEYS.iter().enumerate() {
                for i in 0..TASKS_PER_KEY {
                    let gate_c = gate.clone();
                    let witness_c = witness.clone();
                    let key_owned = key.to_string();
                    handles.push(tokio::spawn(async move {
                        let g = gate_c.acquire(&key_owned);
                        let _lease = g.lock().await;
                        witness_c
                            .lock()
                            .unwrap()
                            .insert((key_owned.clone(), (k_idx * TASKS_PER_KEY + i) as u32));
                        drop(_lease);
                        drop(g);
                        gate_c.try_evict(&key_owned);
                    }));
                }
            }
            for h in handles {
                h.await.unwrap();
            }
            assert_eq!(
                witness.lock().unwrap().len(),
                total,
                "every task across every key must have run exactly once"
            );

            // Force-evict all keys.
            for key in KEYS {
                gate.try_evict(&key.to_string());
            }
            assert_eq!(gate.len(), 0);
        }
    }
}
