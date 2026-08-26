//! KV serving slots as the third implementer of THE one paging engine.
//!
//! **The law this module exists under (Joel, 2026-08-26):** *"we made a paging
//! system for reuse across many things … instead of 10 different codes doing
//! compression or paging, one exists and all trait implementers know HOW this
//! is done for their concern."* MoE experts and the LoRA genome already page
//! through [`PagedResourcePool`]; a llama-server slot — one warm conversation's
//! KV — is the same shape: a scarce residency under contention with a
//! measurable cost of loss. So this module is a thin CONCERN adapter over the
//! shared engine, never a second lease/eviction implementation
//! ([[one-paging-engine-many-trait-implementers]], KV-CACHE-ECONOMY §6: "KV
//! slot residency implements the same shape rather than inventing a rival").
//!
//! **And the key law:** *"we do NOT use strings for keys, we use roomId … the
//! moment you start prepending strings to uuids is the moment it falls
//! apart."* The unit of warmth is the ACTIVITY — a persona's conversation in a
//! room (rooms are 1:1 with activities) — so the key is the typed
//! [`ActivityKey`] struct of UUIDs. Its predecessor, a
//! `format!("{persona}@{room}")` string, lived half a day; the
//! `no_string_composite_id_keys_in_serving` ratchet keeps it dead.
//!
//! What the engine gives this concern for free: single-flight assignment,
//! pin-during-use (mid-turn eviction immunity, wired when the request path
//! pins), hit/miss/eviction stats, `ResourcePool` for the `PressureBroker`,
//! and an [`EvictionPriority`] seam where the PRICED policy
//! (`cost_of_loss = tail_tokens / prefill_rate`, plan slice B5) drops in
//! without new machinery. What this concern supplies: the typed key, the
//! slot-index free list (RAII — an evicted activity's index returns on drop),
//! and the per-server directory.

use std::sync::Arc;

use parking_lot::Mutex;
use uuid::Uuid;

use crate::paging::pool::{lru_priority, PagedResourcePool, PoolConfig};

/// The warm-KV identity: one persona's conversation in one room — an ACTIVITY.
/// Typed UUIDs, non-nil by construction; the map keys on this struct itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActivityKey {
    pub persona: Uuid,
    pub room: Uuid,
}

impl ActivityKey {
    /// Both halves must be real ids. A nil room cannot become a key — the old
    /// persona-only collapse (N concurrent activities thrashing one slot) and
    /// the nil-room phantom key were the two halves of the KV-reuse-0% bug.
    pub fn new(persona: Uuid, room: Uuid) -> Option<Self> {
        if persona.is_nil() || room.is_nil() {
            return None;
        }
        Some(Self { persona, room })
    }
}

/// One leased slot index. The free list is threaded in so that WHENEVER the
/// pool drops this entry — explicit evict, pressure evict, priced evict (B5) —
/// the index returns to circulation by RAII, with no bookkeeping at any call
/// site. (The pool clones values out to readers; the index returns when the
/// LAST clone drops, i.e. after any in-flight reader is done with it.)
#[derive(Debug)]
pub struct KvSlotLease {
    slot: u32,
    free: Arc<Mutex<Vec<u32>>>,
}

impl Drop for KvSlotLease {
    fn drop(&mut self) {
        self.free.lock().push(self.slot);
    }
}

/// The per-server slot pool: N leasable slot indices, activities resident in
/// the shared paging engine.
pub struct KvSlotPool {
    pool: PagedResourcePool<ActivityKey, Arc<KvSlotLease>>,
    free: Arc<Mutex<Vec<u32>>>,
    n_slots: u32,
}

impl KvSlotPool {
    pub fn new(server_root: &str, n_slots: u32) -> Self {
        // Low indices lease first (pop from the back), so occupancy is
        // deterministic and the highest index stays last-touched — which is the
        // index slice B2 reserves as the scratch slot.
        let free: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new((0..n_slots).rev().collect()));
        let pool = PagedResourcePool::new(PoolConfig {
            name: format!("kv-slots {server_root}"),
            // Count-based pool (sizer 1). Capacity is set ABOVE the real slot
            // count on purpose: the free list is the true allocator, so the
            // engine's own over-capacity auto-evict (which drains to 75% —
            // right for byte tiers, thrash for a 4-count pool) can never fire;
            // eviction happens only through the explicit make-room path below.
            max_bytes: (n_slots as u64) + 1,
            sizer: Arc::new(|_| 1),
            // LRU now; the PRICED policy (tail_tokens / prefill_rate — a 7.6k
            // head must never evict a 36k tail, KV-CACHE-ECONOMY §2) replaces
            // this closure in slice B5 through the same seam.
            eviction_priority: lru_priority(),
        });
        Self {
            pool,
            free,
            n_slots,
        }
    }

    pub fn n_slots(&self) -> u32 {
        self.n_slots
    }

    /// Lease the slot for `key`: warm reuse if the activity is resident, a free
    /// index otherwise, evicting the engine-chosen (LRU, later priced) resident
    /// when full. `None` only if the pool cannot free an index (all pinned).
    pub async fn lease(&self, key: ActivityKey) -> Option<u32> {
        // Warm path — also refreshes recency in the engine.
        if let Some(lease) = self.pool.get(&key) {
            return Some(lease.slot);
        }
        // Two rounds: (try allocate) → (make room, try again). load_or_share is
        // single-flight per key, so concurrent same-activity requests share one
        // assignment; distinct activities racing for the last index resolve by
        // one of them evicting (mild over-eviction under a stampede is idle-
        // activity warmth lost, never correctness).
        for round in 0..2 {
            if self.free.lock().is_empty() && round > 0 {
                return None; // eviction freed nothing — everything pinned
            }
            if self.free.lock().is_empty() {
                let evicted = self.pool.evict_at_least(1);
                crate::probe!(
                    class = "inference.slot_affinity.evicted",
                    evicted_count = evicted,
                    "all slots held — engine evicted the least-valuable activity; its \
                     warm prefix is forfeit and its next turn re-prefills (or restores \
                     from the server's prompt cache, measured ~0.1s at 20k)",
                );
                if evicted == 0 {
                    return None;
                }
            }
            let free = Arc::clone(&self.free);
            let res = self
                .pool
                .load_or_share(key, move |_| async move {
                    let slot = free
                        .lock()
                        .pop()
                        .ok_or_else(|| "no free slot index".to_string())?;
                    Ok(Arc::new(KvSlotLease { slot, free }))
                })
                .await;
            match res {
                Ok(lease) => {
                    crate::probe!(
                        class = "inference.slot_affinity.pinned",
                        persona = %key.persona,
                        room = %key.room,
                        slot = lease.slot as u64,
                        "activity pinned to a llama-server slot — its prefix warms HERE",
                    );
                    return Some(lease.slot);
                }
                Err(_) => continue, // lost the race for the last index — make room
            }
        }
        None
    }
}

/// Per-server directory: which roots have a pool, and which are latched
/// unsupported (no /props surface / single slot). The transport-side probe
/// (the adapter owns the HTTP client) discovers; this directory owns state.
pub struct SlotDirectory {
    pools: dashmap::DashMap<String, Option<Arc<KvSlotPool>>>,
}

impl SlotDirectory {
    /// `None` entry == latched Unsupported for this server root.
    pub fn latch_unsupported(&self, root: &str) {
        self.pools.insert(root.to_string(), None);
    }

    /// `Some(Some(pool))` = pool ready; `Some(None)` = latched unsupported;
    /// `None` = never probed (caller should probe /props).
    pub fn get(&self, root: &str) -> Option<Option<Arc<KvSlotPool>>> {
        self.pools.get(root).map(|e| e.clone())
    }

    /// Install (or return the already-installed) pool for a probed server.
    /// First writer wins — the probe race resolves to ONE pool per root.
    pub fn ensure_pool(&self, root: &str, n_slots: u32) -> Arc<KvSlotPool> {
        let entry = self
            .pools
            .entry(root.to_string())
            .or_insert_with(|| Some(Arc::new(KvSlotPool::new(root, n_slots))));
        match entry.value() {
            Some(pool) => Arc::clone(pool),
            None => {
                // A racing latch_unsupported won — honor it by returning a
                // zero-slot pool that never leases (callers treat None lease
                // as unpinned). Practically unreachable: latch and ensure are
                // driven by the same probe outcome.
                Arc::new(KvSlotPool::new(root, 0))
            }
        }
    }
}

/// The one process-wide directory (same scope as the serving resource itself:
/// every adapter instance talking to one server shares one assignment).
pub fn directory() -> &'static SlotDirectory {
    static DIR: std::sync::OnceLock<SlotDirectory> = std::sync::OnceLock::new();
    DIR.get_or_init(|| SlotDirectory {
        pools: dashmap::DashMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(p: u128, r: u128) -> ActivityKey {
        ActivityKey::new(Uuid::from_u128(p), Uuid::from_u128(r)).expect("non-nil test ids")
    }

    // what this catches: the 2026-08-26 KV-reuse-0% bug, both halves. One persona
    // running N concurrent activities (a 4-instance benchmark dispatch, each its
    // own room) must lease N DISTINCT slots — under the old persona-keyed lease
    // all four returned slot 0 and thrashed it (cached:0 every turn). And a nil
    // room must be unrepresentable as a key — it previously became the live
    // string key "persona@000…0".
    #[tokio::test]
    async fn one_persona_many_rooms_leases_distinct_slots() {
        let pool = KvSlotPool::new("test", 4);
        let mut slots = Vec::new();
        for r in 1..=4u128 {
            slots.push(pool.lease(key(7, r)).await.expect("free slot"));
        }
        slots.sort_unstable();
        assert_eq!(slots, vec![0, 1, 2, 3], "four activities hold four distinct slots");
        // Warm re-entry: the same activity gets ITS slot back.
        assert_eq!(pool.lease(key(7, 1)).await, Some(0), "room 1 kept its warm slot");
        // Nil is unrepresentable.
        assert!(ActivityKey::new(Uuid::from_u128(7), Uuid::nil()).is_none());
        assert!(ActivityKey::new(Uuid::nil(), Uuid::from_u128(1)).is_none());
    }

    // what this catches: eviction leaking slot indices. When a 5th activity
    // arrives on a full 4-slot pool, the engine evicts the LRU resident and the
    // freed INDEX must return through the lease's Drop — a leak here would
    // shrink the pool one eviction at a time until nobody can lease.
    #[tokio::test]
    async fn eviction_recycles_the_slot_index() {
        let pool = KvSlotPool::new("test", 2);
        let a = pool.lease(key(1, 1)).await.expect("a");
        let _b = pool.lease(key(1, 2)).await.expect("b");
        // Refresh a so b is the LRU.
        let _ = pool.lease(key(1, 1)).await;
        let c = pool.lease(key(1, 3)).await.expect("c evicts LRU and reuses its index");
        assert_ne!(c, a, "c must not steal the warm slot that was just refreshed");
        // And the evicted activity can come back (cold) on whatever frees next.
        let _ = pool.lease(key(1, 2)).await.expect("evicted activity re-leases");
    }

    // what this catches: the directory latch — an unsupported server must never
    // hand out a pool, and the first probe outcome wins for the process life.
    #[test]
    fn directory_latch_and_ensure() {
        let dir = SlotDirectory {
            pools: dashmap::DashMap::new(),
        };
        assert!(dir.get("s1").is_none(), "unprobed root is unknown");
        dir.latch_unsupported("s1");
        assert!(matches!(dir.get("s1"), Some(None)), "latched unsupported");
        let p = dir.ensure_pool("s2", 4);
        assert_eq!(p.n_slots(), 4);
        assert!(matches!(dir.get("s2"), Some(Some(_))));
    }
}
