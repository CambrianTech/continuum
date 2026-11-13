//! Who may settle a decided credit now: the bounded set of settles in flight per persona, owned
//! by [`super::CreditAcceptanceRegion`] (card a4e8756d).
//!
//! A settle runs as its own task under its own deadline, its QUANTUM: the governor's bound on a
//! region tick, doubled each time that decision's settle was cut. A settle past its quantum is cut
//! when its decision comes round (a hung submit is retried, with backoff), or sooner when another
//! of her owed decisions has no slot (Codex on #5547: eight hung settles must never starve a ninth
//! healthy decision). A slow submit of duration D therefore completes after about log2(D / bound)
//! cuts, where a fixed per-tick cut never let it (the Windows 01:58Z case). A cut task is aborted
//! and its slot stays held (draining) until the task has actually ended, so her settles never
//! exceed the bound even for an instant. Aborting a settle is safe: a replay under the same
//! submission id is accepted at most once (`retry_decided`).
//!
//! Every task is owned: its abort handle lives in its slot, and it is aborted when it is cut,
//! when her persona has left (swept on any persona's pass, [`release_departed`]), or
//! when the region and with it these slots are dropped. Aborts run only after the slots' lock is
//! released, since an aborted task's guard takes that lock to give its slot back.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::task::AbortHandle;
use tokio::time::Instant;
use uuid::Uuid;

/// One decision of one persona.
pub(super) type Key = (Uuid, String);

/// The slots, as the region and every settle's guard share them.
pub(super) type Shared = Arc<parking_lot::Mutex<SettleSlots>>;

struct Running {
    /// Which start of this key the slot belongs to: an old task's guard never frees the slot of
    /// the settle that replaced it.
    generation: u64,
    started: Instant,
    quantum: Duration,
    abort: Option<AbortHandle>,
    /// Aborted to free its slot for a waiting decision: still counted until its task has ended.
    draining: bool,
}

/// What a decision owed an acceptance may do now.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Admit {
    /// Its settle is running within its quantum (or still draining after a cut).
    Running,
    /// Its settle started under this generation.
    Start { generation: u64 },
    /// A settle past its quantum was cut: this decision's own, or another of hers whose slot this
    /// one needs. Its slot frees once its task has ended; the decision starts on a later pass.
    Cut { victim: Key },
    /// Her slots are all held by settles within their quantum, or one is already draining.
    Full,
}

pub(super) struct SettleSlots {
    per_persona: usize,
    base_quantum: Duration,
    running: HashMap<Key, Running>,
    /// How often each still-owed decision's settle was cut: its next quantum's doubling. Dropped once
    /// the decision is settled ([`SettleSlots::settled`]) or her persona leaves.
    cuts: HashMap<Key, u32>,
    next_generation: u64,
    /// Aborts decided under the lock, run once it is released.
    to_abort: Vec<AbortHandle>,
}

impl SettleSlots {
    pub(super) fn shared(per_persona: usize, base_quantum: Duration) -> Shared {
        Arc::new(parking_lot::Mutex::new(Self {
            per_persona,
            base_quantum,
            running: HashMap::new(),
            cuts: HashMap::new(),
            next_generation: 0,
            to_abort: Vec::new(),
        }))
    }

    /// Whether `key`'s settle may start at `now`: its own overdue settle is cut; a free slot of hers
    /// is taken; or, with none free, her most overdue settle is cut to free one.
    fn admit(&mut self, key: &Key, now: Instant) -> Admit {
        if let Some(r) = self.running.get_mut(key) {
            if r.draining || now.duration_since(r.started) <= r.quantum {
                return Admit::Running;
            }
            r.draining = true;
            self.to_abort.extend(r.abort.clone());
            *self.cuts.entry(key.clone()).or_default() += 1;
            return Admit::Cut { victim: key.clone() };
        }
        let persona = key.0;
        if self.running.keys().filter(|k| k.0 == persona).count() >= self.per_persona {
            if self.running.iter().any(|(k, r)| k.0 == persona && r.draining) {
                return Admit::Full; // a slot is already freeing: one abort at a time
            }
            let overdue = self
                .running
                .iter_mut()
                .filter(|(k, r)| k.0 == persona && now.duration_since(r.started) > r.quantum)
                .max_by_key(|(_, r)| now.duration_since(r.started) - r.quantum);
            let Some((victim, r)) = overdue else { return Admit::Full };
            r.draining = true;
            self.to_abort.extend(r.abort.clone());
            let victim = victim.clone();
            *self.cuts.entry(victim.clone()).or_default() += 1;
            return Admit::Cut { victim };
        }
        let generation = self.next_generation;
        self.next_generation += 1;
        let doublings = self.cuts.get(key).copied().unwrap_or(0); // unwrap_or: never cut, the base quantum
        let quantum = self.base_quantum.saturating_mul(1u32.checked_shl(doublings).unwrap_or(u32::MAX)); // unwrap_or: past 31 doublings, the largest quantum
        self.running.insert(key.clone(), Running { generation, started: now, quantum, abort: None, draining: false });
        Admit::Start { generation }
    }

    /// The settle `generation` of `key` ended, however it ended.
    fn finished(&mut self, key: &Key, generation: u64) {
        if self.running.get(key).is_some_and(|r| r.generation == generation) {
            self.running.remove(key);
        }
    }

    /// `key` owes nothing any more: its cut count goes with it.
    pub(super) fn settled(&mut self, key: &Key) {
        self.cuts.remove(key);
    }

    /// Her persona left: every settle of hers is aborted (its slot frees as its task ends) and her
    /// counts forgotten.
    fn release(&mut self, persona: Uuid) {
        for (_, r) in self.running.iter_mut().filter(|(k, _)| k.0 == persona) {
            r.draining = true;
            self.to_abort.extend(r.abort.clone());
        }
        self.cuts.retain(|k, _| k.0 != persona);
    }

    #[cfg(test)]
    pub(super) fn in_flight(&self, persona: Uuid) -> usize {
        self.running.keys().filter(|k| k.0 == persona).count()
    }
}

impl Drop for SettleSlots {
    fn drop(&mut self) {
        for r in self.running.values() {
            if let Some(abort) = &r.abort {
                abort.abort();
            }
        }
    }
}

/// A settle's slot, given up when its task ends however it ends (a panic or an abort included).
/// Weak, so a task never keeps the slots, and with them its own abort, alive.
struct SlotGuard {
    slots: Weak<parking_lot::Mutex<SettleSlots>>,
    key: Key,
    generation: u64,
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if let Some(slots) = self.slots.upgrade() {
            slots.lock().finished(&self.key, self.generation);
        }
    }
}

/// Run what was decided to abort, outside the lock.
fn abort_all(aborts: Vec<AbortHandle>) {
    for abort in aborts {
        abort.abort();
    }
}

/// Admit `key` at `now` and, when it may start, run `settle` as its own owned task.
pub(super) fn try_start(slots: &Shared, key: &Key, now: Instant, settle: impl Future<Output = ()> + Send + 'static) -> Admit {
    let (admit, aborts) = {
        let mut held = slots.lock();
        let admit = held.admit(key, now);
        if let Admit::Start { generation } = admit {
            let guard = SlotGuard { slots: Arc::downgrade(slots), key: key.clone(), generation };
            let task = tokio::spawn(async move {
                let _slot = guard;
                settle.await;
            });
            if let Some(r) = held.running.get_mut(key) {
                r.abort = Some(task.abort_handle());
            }
        }
        (admit, std::mem::take(&mut held.to_abort))
    };
    abort_all(aborts);
    admit
}

/// Abort the settles of every persona `present` no longer answers for: run on any persona's pass,
/// so a persona that left (and is never ticked again) loses hers.
pub(super) fn release_departed(slots: &Shared, present: impl Fn(Uuid) -> bool) {
    let aborts = {
        let mut held = slots.lock();
        let mut personas: Vec<Uuid> = held.running.keys().map(|k| k.0).collect();
        personas.sort_unstable();
        personas.dedup();
        for persona in personas.into_iter().filter(|&p| !present(p)) {
            held.release(persona);
        }
        std::mem::take(&mut held.to_abort)
    };
    abort_all(aborts);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(persona: Uuid, n: usize) -> Key {
        (persona, format!("decision-{n}"))
    }

    /// Let aborted tasks end and their guards run.
    async fn settle_tasks() {
        for _ in 0..4 {
            tokio::task::yield_now().await;
        }
    }

    // what this catches (Codex on #5547 at 5a2a31862): eight settles that hang forever held every
    // slot of hers, so a ninth, healthy decision was never started on any later pass. Under that
    // contention the most overdue settle is aborted; its slot is still counted until its task has
    // ended, then the ninth starts, and her settles never exceed the bound.
    #[tokio::test(start_paused = true)]
    async fn eight_hung_settles_never_starve_a_ninth_healthy_decision() {
        let her = Uuid::new_v4();
        let base = Duration::from_secs(5);
        let slots = SettleSlots::shared(8, base);
        let t0 = Instant::now();
        for n in 0..8 {
            assert!(matches!(try_start(&slots, &key(her, n), t0, std::future::pending()), Admit::Start { .. }));
        }
        assert_eq!(try_start(&slots, &key(her, 0), t0, async {}), Admit::Running, "one settle per decision");
        assert_eq!(try_start(&slots, &key(her, 8), t0 + base, async {}), Admit::Full, "within their quantum the hung keep their slots");
        let Admit::Cut { victim } = try_start(&slots, &key(her, 8), t0 + base * 2, async {}) else {
            panic!("test: a hung settle past its quantum yields to the waiting decision")
        };
        assert_eq!(slots.lock().in_flight(her), 8, "the aborted settle still holds its slot until its task has ended");
        assert_eq!(try_start(&slots, &key(her, 8), t0 + base * 2, async {}), Admit::Full, "one abort at a time");
        settle_tasks().await;
        assert_eq!(slots.lock().in_flight(her), 7, "its task ended, so its slot is free");
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        assert!(matches!(try_start(&slots, &key(her, 8), t0 + base * 2, async move { let _ = done_tx.send(()); }), Admit::Start { .. }));
        assert!(done_rx.await.is_ok(), "the ninth, healthy decision settles");
        settle_tasks().await;
        assert_eq!(slots.lock().in_flight(her), 7, "and frees its slot");
        assert_eq!(try_start(&slots, &victim, t0 + base * 2, async {}), Admit::Start { generation: 9 }, "the cut one runs again when a slot is free");
    }

    // what this catches (card a4e8756d, the Windows 01:58Z cut; Codex on #5547 at 1162e72b2): a
    // fixed per-tick cut never let a slow submit finish, and with no deadline at all a hung submit
    // with nothing waiting behind it was never retried. A settle past its quantum is cut when its
    // decision comes round, each cut doubling the next quantum, so a slow one completes after about
    // log2(D / base) cuts; a cut task's late end never frees its successor's slot.
    #[tokio::test(start_paused = true)]
    async fn an_overdue_settle_is_cut_and_retried_with_a_doubled_quantum() {
        let her = Uuid::new_v4();
        let base = Duration::from_secs(5);
        let slots = SettleSlots::shared(1, base);
        let t0 = Instant::now();
        assert_eq!(try_start(&slots, &key(her, 0), t0, std::future::pending()), Admit::Start { generation: 0 });
        assert_eq!(try_start(&slots, &key(her, 0), t0 + base, async {}), Admit::Running, "within its quantum it runs on");
        let past = t0 + base + Duration::from_millis(1);
        assert_eq!(try_start(&slots, &key(her, 0), past, async {}), Admit::Cut { victim: key(her, 0) }, "a hung settle is cut with nothing waiting behind it");
        assert_eq!(try_start(&slots, &key(her, 0), past, async {}), Admit::Running, "draining until its task has ended");
        assert_eq!(try_start(&slots, &key(her, 1), past, async {}), Admit::Full, "one cut at a time");
        settle_tasks().await;
        assert_eq!(slots.lock().in_flight(her), 0, "its task ended, so its slot is free");
        let t1 = past;
        assert_eq!(try_start(&slots, &key(her, 0), t1, std::future::pending()), Admit::Start { generation: 1 }, "retried under the same decision");
        assert_eq!(try_start(&slots, &key(her, 0), t1 + base * 2, async {}), Admit::Running, "cut once, its quantum doubled");
        assert_eq!(try_start(&slots, &key(her, 0), t1 + base * 2 + Duration::from_millis(1), async {}), Admit::Cut { victim: key(her, 0) });
        settle_tasks().await;
        let t2 = t1 + base * 2 + Duration::from_millis(1);
        assert!(matches!(try_start(&slots, &key(her, 0), t2, std::future::pending()), Admit::Start { generation: 2 }));
        assert_eq!(try_start(&slots, &key(her, 0), t2 + base * 4, async {}), Admit::Running, "cut twice: a settle up to 4x base completes");
        slots.lock().finished(&key(her, 0), 0);
        assert_eq!(slots.lock().in_flight(her), 1, "a cut task's late end never frees its successor's slot");
        slots.lock().settled(&key(her, 0));
        assert!(!slots.lock().cuts.contains_key(&key(her, 0)), "a settled decision forgets its count");
    }

    // what this catches (Codex on #5547): a persona that left and is never ticked again kept her
    // settles running, owned by no one. Any persona's pass sweeps her out; dropping the slots
    // (the region) aborts everything still running.
    #[tokio::test(start_paused = true)]
    async fn a_departed_persona_and_a_dropped_region_abort_their_settles() {
        let (her, other) = (Uuid::new_v4(), Uuid::new_v4());
        let slots = SettleSlots::shared(8, Duration::from_secs(5));
        let t0 = Instant::now();
        let (her_tx, her_rx) = tokio::sync::oneshot::channel::<()>();
        try_start(&slots, &key(her, 0), t0, async move {
            std::future::pending::<()>().await;
            drop(her_tx);
        });
        let (other_tx, other_rx) = tokio::sync::oneshot::channel::<()>();
        try_start(&slots, &key(other, 0), t0, async move {
            std::future::pending::<()>().await;
            drop(other_tx);
        });
        release_departed(&slots, |p| p == other);
        assert!(her_rx.await.is_err(), "her task was aborted (its sender dropped unsent)");
        settle_tasks().await;
        assert_eq!(slots.lock().in_flight(her), 0, "and her slot is free");
        assert_eq!(slots.lock().in_flight(other), 1, "a present persona's settle runs on");
        drop(slots);
        assert!(other_rx.await.is_err(), "dropping the slots aborts the rest");
    }
}
