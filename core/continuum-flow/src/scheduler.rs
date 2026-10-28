//! The scheduler's decisions as a pure state machine: no locks, no runtime, time passed in. The
//! dispatcher (`flow.rs`) turns events into these calls; a validator replays them in microseconds.

use crate::Qos;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::time::{Duration, Instant};

/// One unit of work's identity, unique per scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ticket(pub u64);

/// What the dispatcher must do after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Start this waiting ticket: capacity is free and it is the best eligible.
    Run(Ticket),
    /// Ask this running ticket to yield at its next safe point: a higher class is waiting and
    /// nothing is free. Asked once per ticket.
    Yield(Ticket),
}

/// How long work of each class waits before it is treated as one class higher (never above
/// `Work`, so aged work never preempts and never passes a directed reply). Measured per
/// deployment; indexed by `Qos as usize`.
#[derive(Debug, Clone, Copy)]
pub struct Aging(pub [Duration; 4]);

struct Waiting<K> {
    owner: K,
    qos: Qos,
    since: Instant,
}

struct Running<K> {
    owner: K,
    qos: Qos,
    yield_asked: bool,
}

/// Why a waiting ticket left without running. The flow, not each caller, keeps queues short:
/// a long queue gives every request everyone's latency and answers stale questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dropped {
    /// The same owner submitted newer work: a mind's fresher thought replaces her waiting one.
    Superseded,
    /// A utility or background queue passed its bound: the oldest waiting is shed.
    Shed,
}

pub struct Scheduler<K> {
    capacity: usize,
    aging: Aging,
    waiting: HashMap<Ticket, Waiting<K>>,
    running: HashMap<Ticket, Running<K>>,
    busy_owners: HashSet<K>,
}

impl<K: Clone + Eq + Hash> Scheduler<K> {
    pub fn new(capacity: usize, aging: Aging) -> Self {
        Self { capacity, aging, waiting: HashMap::new(), running: HashMap::new(), busy_owners: HashSet::new() }
    }

    /// Queue `ticket`, returning the tickets that leave the queue because of it. An owner keeps
    /// at most one waiting ticket (the newer supersedes the older, keeping the higher class);
    /// utility and background keep at most `capacity` waiting each, shedding the oldest.
    /// Interactive and work are bounded by the owners themselves and never shed.
    pub fn submit(&mut self, ticket: Ticket, owner: K, qos: Qos, now: Instant) -> Vec<(Ticket, Dropped)> {
        let mut dropped = Vec::new();
        // The newer work keeps the owner's place in line and the higher of the two classes.
        let (mut qos, mut since) = (qos, now);
        let older: Vec<Ticket> = self.waiting.iter().filter(|(_, w)| w.owner == owner).map(|(t, _)| *t).collect();
        for t in older {
            if let Some(w) = self.waiting.remove(&t) {
                qos = qos.max(w.qos);
                since = since.min(w.since);
                dropped.push((t, Dropped::Superseded));
            }
        }
        self.waiting.insert(ticket, Waiting { owner, qos, since });
        if qos <= Qos::Utility {
            let bound = self.capacity.max(1);
            let mut same: Vec<(Instant, Ticket)> =
                self.waiting.iter().filter(|(_, w)| w.qos == qos).map(|(t, w)| (w.since, *t)).collect();
            if same.len() > bound {
                same.sort();
                let excess = same.len() - bound;
                for (_, t) in same.into_iter().take(excess) {
                    self.waiting.remove(&t);
                    dropped.push((t, Dropped::Shed));
                }
            }
        }
        dropped
    }

    /// A running ticket finished, or a waiting one was cancelled. Unknown tickets are ignored:
    /// a release can race the decision that would have started it.
    pub fn release(&mut self, ticket: Ticket) {
        self.waiting.remove(&ticket);
        if let Some(done) = self.running.remove(&ticket) {
            self.busy_owners.remove(&done.owner);
        }
    }

    /// Capacity follows the resource (an engine's slots after a relaunch). Shrinking never
    /// interrupts running work; it only stops new starts until enough finish.
    pub fn resize(&mut self, capacity: usize) {
        self.capacity = capacity;
    }

    /// The class a waiting ticket competes as now: its own, raised one class per aging step,
    /// never above `Work`.
    fn effective(&self, w: &Waiting<K>, now: Instant) -> u32 {
        let step = self.aging.0[w.qos as usize];
        let raised = if step.is_zero() { 0 } else { (now.saturating_duration_since(w.since).as_nanos() / step.as_nanos()) as u32 };
        let ceiling = Qos::Work.rank().max(w.qos.rank());
        (w.qos.rank() + raised).min(ceiling)
    }

    /// The best waiting ticket whose owner has nothing running (each owner is a serial queue):
    /// highest effective class, then oldest, then lowest ticket.
    fn best_eligible(&self, now: Instant) -> Option<Ticket> {
        self.waiting
            .iter()
            .filter(|(_, w)| !self.busy_owners.contains(&w.owner))
            .max_by(|(ta, a), (tb, b)| {
                self.effective(a, now)
                    .cmp(&self.effective(b, now))
                    .then(b.since.cmp(&a.since))
                    .then(tb.cmp(ta))
            })
            .map(|(t, _)| *t)
    }

    /// Everything to do now: start the best eligible work while capacity is free, then, if a
    /// higher class still waits, ask the lowest running class to yield.
    pub fn decide(&mut self, now: Instant) -> Vec<Decision> {
        let mut out = Vec::new();
        while self.running.len() < self.capacity {
            let Some(ticket) = self.best_eligible(now) else { break };
            let Some(w) = self.waiting.remove(&ticket) else { break };
            self.busy_owners.insert(w.owner.clone());
            self.running.insert(ticket, Running { owner: w.owner, qos: w.qos, yield_asked: false });
            out.push(Decision::Run(ticket));
        }
        if let Some(best) = self.best_eligible(now) {
            let wanted = self.waiting.get(&best).map(|w| w.qos);
            let victim = self
                .running
                .iter()
                .filter(|(_, r)| !r.yield_asked && wanted.is_some_and(|q| q > r.qos))
                .min_by_key(|(t, r)| (r.qos, std::cmp::Reverse(**t)))
                .map(|(t, _)| *t);
            if let Some(t) = victim {
                if let Some(r) = self.running.get_mut(&t) {
                    r.yield_asked = true;
                }
                out.push(Decision::Yield(t));
            }
        }
        out
    }

    /// When the next waiting ticket rises a class: the dispatcher's only timer, so it sleeps
    /// until an event or this instant and never polls.
    pub fn next_wakeup(&self, now: Instant) -> Option<Instant> {
        self.waiting
            .values()
            .filter(|w| self.effective(w, now) < Qos::Work.rank().max(w.qos.rank()))
            .filter_map(|w| {
                let step = self.aging.0[w.qos as usize];
                if step.is_zero() {
                    return None;
                }
                let steps = (now.saturating_duration_since(w.since).as_nanos() / step.as_nanos()) as u32 + 1;
                Some(w.since + step * steps)
            })
            .min()
    }

    pub fn running(&self) -> usize {
        self.running.len()
    }

    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }
}
