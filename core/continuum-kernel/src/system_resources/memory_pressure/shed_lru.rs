//! A rebuildable in-RAM cache that RAM sizes, never a number (Joel, 2026-10-29; Cormac on the
//! pattern): entries are kept in recency order and, when the memory pressure monitor sheds, the
//! least recently used are dropped until the cache gives back its share of the host's overrun past
//! its priority's gate. Everything in it can be rebuilt (airc, a recompute), so its floor is zero.
//!
//! One owner for the pattern: every in-process cache of rebuildable values registers one of these
//! with the monitor instead of carrying its own entry cap. The overrun at a priority is shared
//! across every holder at that priority in proportion to what each holds, so one pressure event
//! frees it once (Cormac on #5697).

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use tokio::sync::watch;

use super::{
    MemoryBudgetSpec, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
    PressureSnapshot,
};

impl PressureSnapshot {
    /// Bytes the host is over `priority`'s gate (used RAM past `gate x total`): what every consumer
    /// at that priority gives back between them when they shed. Zero at or under the gate.
    pub fn over_gate(&self, priority: MemoryPriority) -> u64 {
        let used = self.total_bytes.saturating_sub(self.available_bytes);
        let gate = (self.total_bytes as f64 * priority.pressure_gate()) as u64;
        used.saturating_sub(gate)
    }

    /// The part of `priority`'s overrun a consumer holding `held` bytes gives back: its share in
    /// proportion to what it holds among every registered holder at that priority, so one pressure
    /// event frees the overrun once, not once per consumer.
    pub fn share_of_overrun(&self, priority: MemoryPriority, held: u64) -> u64 {
        let total = held_at(priority).max(held);
        if total == 0 {
            return 0;
        }
        (u128::from(self.over_gate(priority)) * u128::from(held) / u128::from(total)) as u64
    }
}

/// A consumer that holds RAM at a priority and sheds a share of that priority's overrun.
pub trait Holder: Send + Sync {
    fn priority(&self) -> MemoryPriority;
    fn held_bytes(&self) -> u64;
}

/// Every shedding consumer, held weakly (a dropped one leaves the group): the population an
/// overrun is apportioned across.
fn holders() -> &'static Mutex<Vec<Weak<dyn Holder>>> {
    static HOLDERS: std::sync::OnceLock<Mutex<Vec<Weak<dyn Holder>>>> = std::sync::OnceLock::new();
    HOLDERS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Join the group an overrun is shared across.
pub fn register_holder(holder: Weak<dyn Holder>) {
    let mut holders = holders().lock();
    holders.retain(|held| held.strong_count() > 0);
    holders.push(holder);
}

/// Bytes every live holder at `priority` holds now.
fn held_at(priority: MemoryPriority) -> u64 {
    holders()
        .lock()
        .iter()
        .filter_map(Weak::upgrade)
        .filter(|holder| holder.priority() == priority)
        .map(|holder| holder.held_bytes())
        .sum()
}

/// How many bytes of RAM a cached value holds (what shedding it gives back).
pub trait ByteSized {
    fn byte_size(&self) -> u64;
}

struct Entry<V> {
    value: Arc<V>,
    used: u64,
}

struct State<K, V> {
    entries: HashMap<K, Entry<V>>,
    /// Recency order: the tick an entry was last used, to its key.
    by_use: BTreeMap<u64, K>,
    tick: u64,
}

impl<K: Clone + Eq + Hash, V> State<K, V> {
    fn touch(&mut self, key: &K) -> Option<Arc<V>> {
        self.tick += 1;
        let tick = self.tick;
        let entry = self.entries.get_mut(key)?;
        self.by_use.remove(&entry.used);
        entry.used = tick;
        self.by_use.insert(tick, key.clone());
        Some(Arc::clone(&entry.value))
    }

    fn insert(&mut self, key: K, value: Arc<V>) {
        self.tick += 1;
        let used = self.tick;
        self.by_use.insert(used, key.clone());
        self.entries.insert(key, Entry { value, used });
    }
}

/// A cache of rebuildable values that the memory monitor sheds, least recently used first.
pub struct ShedLru<K, V> {
    name: &'static str,
    priority: MemoryPriority,
    state: Mutex<State<K, V>>,
    /// The monitor's published snapshot, attached when the cache registers with it.
    pressure: Mutex<Option<watch::Receiver<PressureSnapshot>>>,
}

impl<K: Clone + Eq + Hash + Send + Sync, V: ByteSized + Send + Sync> ShedLru<K, V> {
    /// A cache named `name` at `priority`. It sheds once registered with the monitor
    /// ([`Self::register`]); until then it only holds.
    pub fn new(name: &'static str, priority: MemoryPriority) -> Self {
        Self {
            name,
            priority,
            state: Mutex::new(State {
                entries: HashMap::new(),
                by_use: BTreeMap::new(),
                tick: 0,
            }),
            pressure: Mutex::new(None),
        }
    }

    /// Answer to `monitor` from now on: it reads this cache's bytes and asks it to shed its share.
    pub fn register(self: &Arc<Self>, monitor: &super::MemoryPressureMonitor)
    where
        K: 'static,
        V: 'static,
    {
        *self.pressure.lock() = Some(monitor.subscribe());
        let holder: Arc<dyn Holder> = Arc::clone(self) as Arc<dyn Holder>;
        register_holder(Arc::downgrade(&holder));
        monitor.add_reporter(Arc::clone(self) as Arc<dyn MemoryReporter>);
    }

    /// The cached value for `key` (marking it used), or `None`.
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        self.state.lock().touch(key)
    }

    /// The cached value for `key`, or `make()`'s, cached. A value already present wins, so every
    /// caller shares one. `make` runs under the cache's lock: keep it cheap, and compute an
    /// expensive value outside and use [`Self::insert_if_absent`].
    pub fn get_or_insert_with(&self, key: K, make: impl FnOnce() -> V) -> Arc<V> {
        let mut state = self.state.lock();
        if let Some(value) = state.touch(&key) {
            return value;
        }
        let value = Arc::new(make());
        state.insert(key, Arc::clone(&value));
        value
    }

    /// Cache `value` under `key` unless one is already there, which wins (so every caller shares
    /// one): for a value computed outside the lock, where two racing computations keep the first.
    pub fn insert_if_absent(&self, key: K, value: V) -> Arc<V> {
        let mut state = self.state.lock();
        if let Some(existing) = state.touch(&key) {
            return existing;
        }
        let value = Arc::new(value);
        state.insert(key, Arc::clone(&value));
        value
    }

    pub fn len(&self) -> usize {
        self.state.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bytes held now (summed at read time, so values that grow after insert are counted).
    pub fn bytes(&self) -> u64 {
        self.state
            .lock()
            .entries
            .values()
            .map(|entry| entry.value.byte_size())
            .sum()
    }

    /// Drop least recently used entries until at least `want` bytes are given back (or nothing
    /// sheddable is left). An entry a caller still holds is skipped: dropping the cache's copy would
    /// free nothing, and counting it would drain the cache while the RAM stays pinned elsewhere.
    /// Returns the bytes actually given back.
    pub fn shed(&self, want: u64) -> u64 {
        let mut state = self.state.lock();
        let mut freed = 0;
        let mut dropped = Vec::new();
        for (tick, key) in &state.by_use {
            if freed >= want {
                break;
            }
            if let Some(entry) = state.entries.get(key) {
                if Arc::strong_count(&entry.value) == 1 {
                    freed += entry.value.byte_size();
                    dropped.push((*tick, key.clone()));
                }
            }
        }
        for (tick, key) in dropped {
            state.by_use.remove(&tick);
            state.entries.remove(&key);
        }
        freed
    }
}

impl<K: Clone + Eq + Hash + Send + Sync, V: ByteSized + Send + Sync> Holder for ShedLru<K, V> {
    fn priority(&self) -> MemoryPriority {
        self.priority
    }

    fn held_bytes(&self) -> u64 {
        self.bytes()
    }
}

impl<K: Clone + Eq + Hash + Send + Sync, V: ByteSized + Send + Sync> MemoryReporter
    for ShedLru<K, V>
{
    fn name(&self) -> &'static str {
        self.name
    }

    fn budget(&self) -> MemoryBudgetSpec {
        let held = self.bytes();
        MemoryBudgetSpec {
            name: self.name.to_string(),
            priority: self.priority,
            min_bytes: 0,
            preferred_bytes: held,
            max_bytes: held,
        }
    }

    fn report(&self) -> ModuleMemoryReport {
        ModuleMemoryReport {
            name: self.name.to_string(),
            bytes: self.bytes(),
            detail: format!("{} rebuildable entries", self.len()),
            can_shed: true,
        }
    }

    fn can_shed(&self) -> bool {
        true
    }

    /// Give back this cache's share of the overrun past its priority's gate, least recently used first.
    fn shed_load(&self, _level: PressureLevel) {
        let held = self.bytes();
        let Some(share) = self
            .pressure
            .lock()
            .as_ref()
            .map(|pressure| pressure.borrow().share_of_overrun(self.priority, held))
        else {
            return;
        };
        if share > 0 {
            let freed = self.shed(share);
            continuum_base::probe!(
                class = "memory.shed_lru.shed",
                cache = self.name,
                share = share,
                freed = freed,
                "a rebuildable cache gave back its share of RAM past its priority's gate"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Bytes(u64);
    impl ByteSized for Bytes {
        fn byte_size(&self) -> u64 {
            self.0
        }
    }

    fn cache() -> ShedLru<u32, Bytes> {
        ShedLru::new("test", MemoryPriority::Background)
    }

    // what this catches: a cache sized by a count instead of RAM, or shedding the wrong entries:
    // it gives back the requested bytes least-recently-used first, keeps a value it was just asked
    // for, and shares one value per key.
    #[test]
    fn it_sheds_least_recently_used_first_by_bytes() {
        let lru = cache();
        for key in 0..4u32 {
            lru.get_or_insert_with(key, || Bytes(10));
        }
        assert_eq!((lru.len(), lru.bytes()), (4, 40));
        assert!(lru.get(&0).is_some(), "0 is now the most recently used");
        assert_eq!(
            lru.shed(15),
            20,
            "two oldest (1, 2) give back at least 15 bytes"
        );
        assert!(lru.get(&1).is_none() && lru.get(&2).is_none());
        assert!(
            lru.get(&0).is_some() && lru.get(&3).is_some(),
            "the recently used stay"
        );
        let first = lru.insert_if_absent(7, Bytes(1));
        let again = lru.insert_if_absent(7, Bytes(99));
        assert!(
            Arc::ptr_eq(&first, &again),
            "one shared value per key; the first insert wins"
        );
        drop((first, again));
        assert_eq!(lru.shed(u64::MAX), 21, "shedding everything empties it");
        assert!(lru.is_empty());
    }

    // what this catches: an entry a caller still holds counted as freed: the cache would drain
    // itself while the RAM stays pinned elsewhere. A held entry is skipped.
    #[test]
    fn a_held_entry_is_never_counted_as_freed() {
        let lru = cache();
        let held = lru.get_or_insert_with(1, || Bytes(10));
        lru.get_or_insert_with(2, || Bytes(10));
        assert_eq!(lru.shed(u64::MAX), 10, "only the unheld entry frees");
        assert!(lru.get(&1).is_some(), "the held one stays cached");
        drop(held);
    }

    struct Fixed(MemoryPriority, u64);
    impl Holder for Fixed {
        fn priority(&self) -> MemoryPriority {
            self.0
        }
        fn held_bytes(&self) -> u64 {
            self.1
        }
    }

    // what this catches: every consumer at a priority shedding the WHOLE overrun, so N caches free N
    // times what the host needed and rebuildable entries thrash: the shares of one overrun across a
    // priority's holders add up to the overrun, in proportion to what each holds.
    #[test]
    fn one_overrun_is_shared_across_a_priority_not_repeated() {
        let priority = MemoryPriority::Batch; // a priority no other test in this crate registers
        let a: Arc<dyn Holder> = Arc::new(Fixed(priority, 300));
        let b: Arc<dyn Holder> = Arc::new(Fixed(priority, 100));
        register_holder(Arc::downgrade(&a));
        register_holder(Arc::downgrade(&b));
        let snapshot = PressureSnapshot {
            total_bytes: 1000,
            available_bytes: 300,
            ..PressureSnapshot::default()
        };
        assert_eq!(
            snapshot.over_gate(priority),
            200,
            "used 700 of 1000 is 200 past Batch's 0.50 gate"
        );
        let (share_a, share_b) = (
            snapshot.share_of_overrun(priority, 300),
            snapshot.share_of_overrun(priority, 100),
        );
        assert_eq!(
            (share_a, share_b),
            (150, 50),
            "in proportion to what each holds"
        );
        assert_eq!(
            share_a + share_b,
            200,
            "the shares add up to the overrun once"
        );
    }
}
