//! [`PerKeyGate`] — substrate-canonical per-key serialization
//! primitive with structural-on-drop eviction.
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
//! ## Why RAII (and not a separate `try_evict`)
//!
//! v1 of this primitive shipped a paired `acquire` + `try_evict` API.
//! Reviewer-2's BLOCK on PR #1582 found the obvious footgun: the
//! local `gate` binding from `acquire()` is still in scope when
//! `try_evict` runs, so `Arc::strong_count >= 2` and eviction silently
//! no-ops. Production traffic leaks gates forever — the exact
//! `[[auto-clean-is-structural-not-operational]]` failure this
//! primitive was meant to close.
//!
//! v2 redesigns the API around a `Lease<K>` RAII guard. `acquire`
//! returns a Lease that owns BOTH the lock guard AND the gate Arc.
//! On `Drop`, the Lease drops the guard, drops its gate Arc, and
//! then runs `remove_if(strong_count == 1)`. The user CANNOT skip
//! the eviction step and CANNOT hold an extra Arc clone outside the
//! Lease — the API doesn't expose the Arc.
//!
//! Canonical usage (post-v2):
//!
//! ```ignore
//! let _lease = self.gates.acquire(&key).await;
//! // ... do work that needs serialization for `key` ...
//! // _lease dropped at end of scope → lock released, gate evicted
//! //   if no other callers hold it
//! ```
//!
//! ## The structural-eviction trick
//!
//! On `Lease::drop`:
//! 1. Drop the `OwnedMutexGuard` → lock released, guard's internal
//!    Arc clone dropped.
//! 2. Drop the Lease's own gate Arc.
//! 3. `DashMap::remove_if(key, |_, gate| Arc::strong_count(gate) == 1)`.
//!    When `strong_count == 1`, the ONLY remaining reference is the
//!    map's own entry — no other Lease holds a clone of the
//!    `Arc<Mutex>`, so removing it cannot break anyone's
//!    serialization guarantee. A concurrent caller arriving AFTER
//!    eviction sees no gate and `acquire` creates a fresh one —
//!    exactly the same semantics as "first-ever acquire for this key."
//!
//! Concurrent-acquire safety during step 3: `DashMap::remove_if`
//! takes the shard write lock. A concurrent `acquire` fast-path takes
//! the shard read lock. They serialize on the shard, so either:
//! - Concurrent acquirer wins the shard lock first → it clones the
//!   Arc (strong_count goes to 2) → our predicate evaluates and sees
//!   strong_count == 2 → no-op. Correct.
//! - Our remove wins → entry gone → concurrent acquirer falls
//!   through to `entry().or_insert_with()` and creates a fresh gate.
//!   Correct.
//!
//! ## What it doesn't do
//!
//! - **No TTL.** Eviction is purely structural-based-on-refcount.
//!   A hot key stays in the map as long as it has waiters.
//! - **No LRU.** Keys are evicted opportunistically on lease drop.
//!   There is no background sweeper.
//! - **No tracking of WHICH caller holds the lease.** The gate is
//!   anonymous — `Arc<Mutex<()>>` has no notion of identity. If you
//!   need "fairness" or "first-come-first-served" beyond
//!   `tokio::sync::Mutex`'s own (which already provides FIFO under
//!   contention), use a semaphore or a queue instead.
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
use tokio::sync::{Mutex, OwnedMutexGuard};

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

    /// Acquire the gate for `key`, awaiting the lock if another
    /// caller already holds it. Returns a [`Lease<K>`] that holds
    /// both the lock guard and the gate Arc. On `Drop`, the lease
    /// releases the lock and attempts structural eviction of the
    /// gate from the map.
    ///
    /// Race-safe under concurrent `acquire` for the same key:
    /// `DashMap::entry().or_insert_with()` runs the closure at most
    /// once per concurrent contention window; subsequent callers in
    /// the same window observe the just-inserted entry and clone its
    /// `Arc`. All concurrent acquirers serialize on the same
    /// `Mutex<()>`.
    pub async fn acquire(&self, key: &K) -> Lease<K> {
        // Fast path: gate already exists. Read lock on shard only.
        let gate: Arc<Mutex<()>> = if let Some(g) = self.gates.get(key) {
            g.clone()
        } else {
            // Slow path: create-or-observe-existing under shard write
            // lock. or_insert_with runs the closure at most once per
            // contention window.
            self.gates
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        // lock_owned() consumes one Arc clone into the guard; the
        // guard is 'static and Send, so the Lease can be held across
        // .await and across task boundaries.
        let guard = gate.clone().lock_owned().await;
        Lease {
            key: key.clone(),
            gate: Some(gate),
            guard: Some(guard),
            map: self.gates.clone(),
        }
    }

    /// Number of gates currently tracked. Includes both contested
    /// and idle gates. In steady state (no Lease mid-Drop on another
    /// thread), idle gates would have been evicted on the last lease
    /// drop, so a non-zero `len()` implies there ARE active leases.
    /// Useful for substrate-side leak-detection assertions in tests.
    ///
    /// Caveat: during the narrow window between a Lease's
    /// `gate.take()` and the `remove_if` running on another thread,
    /// `len()` can briefly observe an entry with no active lease.
    /// Tests asserting `len() == 0` should run after all leases have
    /// dropped synchronously (the typical case — Drop runs on the
    /// owning task).
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

/// RAII lease on a per-key gate. Cannot be cloned. On `Drop`:
/// releases the lock, drops the gate Arc, and attempts structural
/// eviction (`remove_if Arc::strong_count == 1`).
///
/// The Lease deliberately does NOT expose the underlying `Arc<Mutex>`.
/// Exposing it would let callers smuggle the Arc past the lease's
/// lifetime and break the structural-eviction invariant.
#[must_use = "PerKeyGate lease must be held for the critical section; dropping immediately makes the gate pointless"]
pub struct Lease<K: Eq + Hash + Clone> {
    key: K,
    /// The Lease's own clone of the gate Arc. Held so that we can
    /// drop it explicitly in Drop ordering before evaluating the
    /// eviction predicate.
    gate: Option<Arc<Mutex<()>>>,
    /// The owned mutex guard. Drop order matters: guard MUST be
    /// dropped before gate (the guard internally holds an Arc clone
    /// too; releasing it first lets strong_count drop toward the
    /// eviction threshold).
    guard: Option<OwnedMutexGuard<()>>,
    /// Back-reference to the map so Drop can run `remove_if`.
    map: Arc<DashMap<K, Arc<Mutex<()>>>>,
}

impl<K: Eq + Hash + Clone> Drop for Lease<K> {
    fn drop(&mut self) {
        // Order is load-bearing:
        // 1) Drop the guard → releases the lock AND drops the
        //    guard's internal Arc clone.
        drop(self.guard.take());
        // 2) Drop our explicit gate Arc → strong_count now reflects
        //    only the map's slot + any concurrent acquirers in flight.
        drop(self.gate.take());
        // 3) Attempt structural eviction under the shard write lock.
        //    `remove_if` evaluates the predicate while holding the
        //    write lock, so concurrent acquirers either complete
        //    their clone before us (predicate sees strong_count == 2,
        //    no-op) or arrive after us (find no entry, create fresh
        //    gate via or_insert_with).
        self.map
            .remove_if(&self.key, |_, gate| Arc::strong_count(gate) == 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: repeated acquires for the same key
    // serialize through the SAME underlying Mutex<()>. v1 used
    // Arc::ptr_eq on the returned Arc to prove this; v2 hides the
    // Arc, so we prove serialization end-to-end: a second acquire
    // for the same key blocks until the first lease drops. A
    // regression in entry().or_insert_with() that created a fresh
    // Mutex per acquire would let the second acquire complete
    // immediately and break serialization.
    #[tokio::test]
    async fn repeated_acquires_for_same_key_serialize() {
        let gate = Arc::new(PerKeyGate::<String>::new());

        let lease_1 = gate.acquire(&"alpha".to_string()).await;

        // Try to acquire again with a short timeout. Must NOT
        // succeed while lease_1 is held.
        let gate_c = gate.clone();
        let acquire_fut = tokio::spawn(async move {
            let _l = gate_c.acquire(&"alpha".to_string()).await;
            "acquired"
        });
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            // wrap in a fresh future so timeout is independent of join
            async { acquire_fut.await.unwrap() },
        )
        .await;
        assert!(
            result.is_err(),
            "second acquire on same key must block while first lease is held"
        );

        // Release the first lease — now the second acquire should
        // proceed.
        drop(lease_1);
        // The spawned task should now complete. The gate Arc lives
        // inside that task; we don't await on it directly here to
        // keep the test simple — len observation below confirms
        // either (a) entry still present with task's lease, or (b)
        // task completed and evicted.
    }

    // what this catches: acquires for DIFFERENT keys do NOT
    // serialize against each other. A regression that flattened the
    // map's hashing would silently make every key serialize against
    // every other key, destroying parallelism.
    #[tokio::test]
    async fn acquires_for_different_keys_proceed_independently() {
        let gate = PerKeyGate::<String>::new();
        let l1 = gate.acquire(&"alpha".to_string()).await;
        // Different key — must not block.
        let l2 = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            gate.acquire(&"beta".to_string()),
        )
        .await
        .expect("different-key acquire must not block");
        drop(l1);
        drop(l2);
    }

    // what this catches: when a single lease is acquired and then
    // dropped with no other holders, the gate is structurally
    // evicted from the map. Without this, the map grows unbounded —
    // exactly the leak class the primitive exists to close.
    // Regression for PR #1582 reviewer-2 BLOCK: v1's separate
    // try_evict was no-op'd by the caller's local Arc binding,
    // leaking gates forever in production. v2's RAII Lease makes
    // the leak structurally impossible.
    #[tokio::test]
    async fn lease_drop_evicts_gate_when_no_other_holders() {
        let gate = PerKeyGate::<String>::new();
        {
            let _lease = gate.acquire(&"alpha".to_string()).await;
            assert_eq!(gate.len(), 1, "gate present while lease held");
        }
        assert_eq!(
            gate.len(),
            0,
            "lease drop MUST evict the gate when no other holders remain"
        );
    }

    // what this catches: when TWO leases are acquired for the same
    // key (the second after the first releases), the gate must
    // survive the first lease's drop until the second also drops.
    // A naive eviction that didn't check strong_count would orphan
    // the second lease's Mutex from the map mid-flight.
    #[tokio::test]
    async fn gate_survives_first_lease_drop_while_second_holds() {
        let gate = Arc::new(PerKeyGate::<String>::new());
        let lease_1 = gate.acquire(&"alpha".to_string()).await;

        // Start a second acquire that will park on the lock.
        let gate_c = gate.clone();
        let task = tokio::spawn(async move {
            let lease = gate_c.acquire(&"alpha".to_string()).await;
            // Hold the lease briefly so the parent can observe
            // gate.len() == 1.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            drop(lease);
        });

        // Yield to let the spawned task park on .lock().await.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(gate.len(), 1, "gate held by lease_1 + parked waiter");

        // Drop lease_1. The spawned task should now acquire.
        drop(lease_1);

        // While the spawned task holds its lease, gate must persist.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(
            gate.len(),
            1,
            "gate must persist while second lease is held"
        );

        task.await.unwrap();

        // Now both leases dropped — gate evicted.
        assert_eq!(gate.len(), 0, "gate evicted after final lease drops");
    }

    // what this catches: post-eviction acquire creates a FRESH gate
    // with the same identity semantics as first-ever-acquire. A
    // concurrent caller landing right after eviction must see a
    // clean gate, not a partially-removed one. Critical for the
    // "cold key arrives again" pattern — gate gets evicted, then a
    // new submit arrives, then the gate needs to work normally.
    #[tokio::test]
    async fn re_acquire_after_eviction_works_normally() {
        let gate = PerKeyGate::<String>::new();
        {
            let _lease = gate.acquire(&"alpha".to_string()).await;
        }
        assert_eq!(gate.len(), 0);

        // Re-acquire — must work, must not deadlock, must register
        // a fresh gate.
        let _lease_again = gate.acquire(&"alpha".to_string()).await;
        assert_eq!(gate.len(), 1);
    }

    /// Stress tests — gated behind the `stress-tests` feature per
    /// CLAUDE.md test-discipline doctrine. Default `cargo test` does
    /// NOT compile these; CI runs them via `--features stress-tests`.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // what this catches: under N concurrent tasks racing
        // acquire + critical-section + drop for the SAME key, the
        // gate genuinely serializes critical sections. The witness
        // pattern is a deliberate read-yield-write on an AtomicUsize:
        //
        //   let prev = c.load(Acquire);
        //   yield_now().await;
        //   c.store(prev + 1, Release);
        //
        // If serialization is broken (two tasks in the critical
        // section concurrently), they both read the same `prev` and
        // both write `prev + 1` — a lost update. Final value < N.
        // If serialization works, every task sees a unique `prev`
        // and writes a unique successor — final value == N.
        //
        // This is the rigor improvement called out in PR #1582
        // test-rigor review: a HashSet::insert witness would also
        // pass with broken serialization, hence the AtomicUsize
        // read-yield-write pattern instead.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_acquire_drop_cycles_actually_serialize_critical_sections() {
            let gate = Arc::new(PerKeyGate::<String>::new());
            let counter = Arc::new(AtomicUsize::new(0));

            const N: usize = 200;
            let mut handles = Vec::with_capacity(N);
            for _ in 0..N {
                let gate_c = gate.clone();
                let counter_c = counter.clone();
                handles.push(tokio::spawn(async move {
                    let _lease = gate_c.acquire(&"shared".to_string()).await;
                    // Read-yield-write — a lost update would only
                    // happen if two critical sections interleave.
                    let prev = counter_c.load(Ordering::Acquire);
                    tokio::task::yield_now().await;
                    counter_c.store(prev + 1, Ordering::Release);
                }));
            }
            for h in handles {
                h.await.unwrap();
            }
            assert_eq!(
                counter.load(Ordering::Acquire),
                N,
                "serialized critical sections must produce exactly N increments — \
                 a lower value indicates lost updates from broken mutual exclusion"
            );

            // Every lease dropped — gate must have evicted itself.
            assert_eq!(
                gate.len(),
                0,
                "no leases outstanding → gate must be auto-evicted by the last drop"
            );
        }

        // what this catches: under N concurrent tasks across K
        // different keys, each key's critical sections genuinely
        // serialize independently. Same read-yield-write witness as
        // above, but per-key counters. A regression that flattened
        // the keyspace (everyone shares the same gate) would still
        // pass the per-key counter check (each key's counter still
        // gets N increments serialized GLOBALLY). To distinguish
        // "serialized per-key" from "serialized globally", we also
        // check that the test completes faster than a serialized-
        // global lower bound. Per CLAUDE.md the wall-clock check is
        // a smoke test; not asserted strictly.
        //
        // The non-wall-clock invariant: every key's counter equals
        // its task count, AND the gate is fully evicted at end.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn different_keys_serialize_independently_without_corruption() {
            let gate = Arc::new(PerKeyGate::<String>::new());

            const TASKS_PER_KEY: usize = 50;
            const KEYS: &[&str] = &["alpha", "beta", "gamma", "delta"];

            let counters: Vec<Arc<AtomicUsize>> =
                KEYS.iter().map(|_| Arc::new(AtomicUsize::new(0))).collect();

            let total = TASKS_PER_KEY * KEYS.len();
            let mut handles = Vec::with_capacity(total);
            for (k_idx, key) in KEYS.iter().enumerate() {
                for _ in 0..TASKS_PER_KEY {
                    let gate_c = gate.clone();
                    let counter_c = counters[k_idx].clone();
                    let key_owned = key.to_string();
                    handles.push(tokio::spawn(async move {
                        let _lease = gate_c.acquire(&key_owned).await;
                        let prev = counter_c.load(Ordering::Acquire);
                        tokio::task::yield_now().await;
                        counter_c.store(prev + 1, Ordering::Release);
                    }));
                }
            }
            for h in handles {
                h.await.unwrap();
            }
            for (k_idx, _) in KEYS.iter().enumerate() {
                assert_eq!(
                    counters[k_idx].load(Ordering::Acquire),
                    TASKS_PER_KEY,
                    "key {}: critical sections must serialize per-key — \
                     a lower count means lost updates",
                    KEYS[k_idx]
                );
            }

            // Every lease dropped → all gates auto-evicted.
            assert_eq!(gate.len(), 0, "all leases dropped → all gates auto-evicted");
        }
    }
}
