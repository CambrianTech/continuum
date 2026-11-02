//! A rebuildable in-RAM cache that RAM sizes, never a number (Joel, 2026-10-29; Cormac on the
//! pattern): entries are kept in recency order and, when the memory pressure monitor sheds, the
//! least recently used are dropped until the cache gives back exactly the host's overrun past its
//! priority's gate. Everything in it can be rebuilt (airc, a recompute), so its floor is zero.
//!
//! One owner for the pattern: every in-process cache of rebuildable values registers one of these
//! with the monitor instead of carrying its own entry cap.

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::watch;

use super::{
    MemoryBudgetSpec, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
    PressureSnapshot,
};

impl PressureSnapshot {
    /// Bytes the host is over `priority`'s gate (used RAM past `gate x total`): what a consumer at
    /// that priority gives back when it sheds. Zero at or under the gate.
    pub fn over_gate(&self, priority: MemoryPriority) -> u64 {
        let used = self.total_bytes.saturating_sub(self.available_bytes);
        let gate = (self.total_bytes as f64 * priority.pressure_gate()) as u64;
        used.saturating_sub(gate)
    }
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

    /// Answer to `monitor` from now on: it reads this cache's bytes and asks it to shed.
    pub fn register(self: &Arc<Self>, monitor: &super::MemoryPressureMonitor)
    where
        K: 'static,
        V: 'static,
    {
        *self.pressure.lock() = Some(monitor.subscribe());
        monitor.add_reporter(Arc::clone(self) as Arc<dyn MemoryReporter>);
    }

    /// The cached value for `key` (marking it used), or `None`.
    pub fn get(&self, key: &K) -> Option<Arc<V>> {
        self.state.lock().touch(key)
    }

    /// The cached value for `key`, or `make()`'s, cached. A value already present wins, so every
    /// caller shares one.
    pub fn get_or_insert_with(&self, key: K, make: impl FnOnce() -> V) -> Arc<V> {
        let mut state = self.state.lock();
        if let Some(value) = state.touch(&key) {
            return value;
        }
        state.tick += 1;
        let used = state.tick;
        let value = Arc::new(make());
        state.by_use.insert(used, key.clone());
        state.entries.insert(
            key,
            Entry {
                value: Arc::clone(&value),
                used,
            },
        );
        value
    }

    pub fn len(&self) -> usize {
        self.state.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bytes held now: what shedding everything would give back.
    pub fn bytes(&self) -> u64 {
        self.state
            .lock()
            .entries
            .values()
            .map(|entry| entry.value.byte_size())
            .sum()
    }

    /// Drop least recently used entries until at least `want` bytes are given back (or nothing is
    /// left). Returns the bytes given back.
    pub fn shed(&self, want: u64) -> u64 {
        let mut state = self.state.lock();
        let mut freed = 0;
        while freed < want {
            let Some((_, key)) = state.by_use.pop_first() else {
                break;
            };
            if let Some(entry) = state.entries.remove(&key) {
                freed += entry.value.byte_size();
            }
        }
        freed
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

    /// Give back the host's overrun past this priority's gate, least recently used first.
    fn shed_load(&self, _level: PressureLevel) {
        let Some(overrun) = self
            .pressure
            .lock()
            .as_ref()
            .map(|pressure| pressure.borrow().over_gate(self.priority))
        else {
            return;
        };
        if overrun > 0 {
            let freed = self.shed(overrun);
            continuum_base::probe!(
                class = "memory.shed_lru.shed",
                cache = self.name,
                overrun = overrun,
                freed = freed,
                "a rebuildable cache gave back RAM past its priority's gate"
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
    // it holds what it is given, gives back the requested bytes least-recently-used first, keeps
    // a value it was just asked for, and shares one value per key.
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
        let first = lru.get_or_insert_with(7, || Bytes(1));
        let again = lru.get_or_insert_with(7, || Bytes(99));
        assert!(Arc::ptr_eq(&first, &again), "one shared value per key");
        assert_eq!(lru.shed(u64::MAX), 21, "shedding everything empties it");
        assert!(lru.is_empty());
    }
}
