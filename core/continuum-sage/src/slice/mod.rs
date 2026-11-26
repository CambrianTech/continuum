//! The engine's slice: which ready mind takes the next step, and when the running one hands over.
//!
//! Minds share one engine by switching at the next step, round robin, never by holding it for a
//! turn. The engine's step is the only serial work in a mind's life, and it is sliced so finely
//! that every mind seems to have the engine to herself (Joel, 2026-11-22: "It's effectively ALL
//! simultaneous ... They don't notice"). A pure state machine like [`crate::Scheduler`]: no locks,
//! no runtime, time passed in, so a validator replays a captured trace in microseconds.
//!
//! This module is the algorithm, isolated; [`lane`] is where minds meet it, by current state;
//! [`tuner`] is the control loop that sets the slice.
//!
//! - **Priority is how often she is chosen** (Joel: "a doubly more priority persona would be chosen
//!   more often or for longer"), as in an RTOS: twice the weight, twice the turns.
//! - **Who gets more is SAGE's need, live.** Each mind's weight is the need its parts publish on
//!   `sage/need`, folded ([`Need::max`]); every change moves her share at once. A rise in urgency
//!   (someone addressed her) takes the next step, cutting the running slice short.
//! - **Fair by stride.** Each mind carries a pass, advanced by the engine time she used over her
//!   weight; the lowest pass among the ready steps next. Nobody starves. A mind that wakes starts
//!   at the ready floor, banking no credit.
//! - **The slice is a control loop optimizing throughput** ([`tuner::Tuner`]), constantly
//!   adjusted from what the engine measured, against one wall: no ready mind waits past
//!   `Policy::wait_bound`. The wall follows the weights (the lightest mind waits for everyone
//!   heavier) and the measured switch; a window in which any mind waited past it backs the loop off.
//! - **Finish first under a switch.** A mind whose predicted remainder (her measured turn length
//!   less the steps taken, at her measured step time) is under one switch keeps the engine.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

use crate::component::{Need, Urgency};

pub mod lane;
pub mod control;
pub mod device;
pub mod room;
pub mod sim;
pub mod trace;
pub mod tuner;

use control::Control;
use tuner::Tuner;

/// What the slice is held to.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// No ready mind waits longer than this for her next step: the wall the loop climbs against.
    pub wait_bound: Duration,
    /// Weight of the newest sample in each measured mean, in (0, 1].
    pub smoothing: f64,
}

/// What the engine does at this step boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next<K> {
    /// The running mind takes the next step.
    Continue,
    /// Bring this mind in (from another, or onto an idle engine) and step her.
    Switch(K),
    /// Nobody is ready.
    Idle,
}

/// A mean that follows its measurements; empty until the first.
#[derive(Debug, Clone, Copy, Default)]
struct Measured(Option<f64>);

impl Measured {
    fn add(&mut self, sample: f64, smoothing: f64) {
        self.0 = Some(match self.0 {
            Some(mean) => mean + smoothing * (sample - mean),
            None => sample,
        });
    }

    /// What it has cost as far as this run has seen, in seconds: its mean, or nothing before the
    /// first measurement. An unmeasured step or switch has taken no engine time yet, and its first
    /// measurement replaces this, so planning never invents a cost it has not seen.
    fn seconds(self) -> f64 {
        self.0.unwrap_or(0.0) // JUSTIFIED: before its first measurement it has taken no engine time this run
    }
}

#[derive(Debug, Default)]
struct Mind {
    need: Need,
    ready: bool,
    /// Engine seconds used over her weight: the lowest ready pass steps next.
    pass: f64,
    /// Seconds per step.
    step: Measured,
    /// Bringing her KV back on the device, the whole copy (0 where a parked mind is a pointer): what a slice before
    /// her must cover for a prefetch to finish under it.
    fetch: Measured,
    /// Steps per turn: what predicts her remainder.
    turn_steps: Measured,
    /// Steps taken in the turn now running.
    steps: u64,
    /// Engine seconds (the slicer's measured clock) when she began waiting for the lane.
    waiting_since: Option<f64>,
}

/// Her share: each urgency level doubles it, and each input waiting adds her weight once more.
fn weight(need: Need) -> f64 {
    let level = match need.urgency {
        Urgency::Content => 1.0,
        Urgency::Want => 2.0,
        Urgency::Urgent => 4.0,
    };
    level * (1 + need.waiting) as f64
}

struct Running<K> {
    mind: K,
    since: Instant,
    /// The engine clock when this slice began: maps a guard's engine time to a wall deadline.
    engine_at: f64,
    /// A ready mind's urgency rose: she steps next, whatever is left of this slice.
    cut: bool,
}

pub struct Slicer<K> {
    policy: Policy,
    minds: HashMap<K, Mind>,
    running: Option<Running<K>>,
    switch: Measured,
    control: Box<dyn Control>,
    /// Engine seconds measured so far (steps and switches): the clock waits are measured on.
    engine: f64,
    /// This window: switches made, and the longest any mind waited.
    window_switches: usize,
    window_worst: f64,
}

impl<K: Copy + Eq + Hash + Ord> Slicer<K> {
    /// A slicer whose slice climbs throughput ([`Tuner`]).
    pub fn new(policy: Policy) -> Self {
        Self::with(policy, Box::new(Tuner::default()))
    }

    /// A slicer under another control law: a baseline, a candidate, a learned one.
    pub fn with(policy: Policy, control: Box<dyn Control>) -> Self {
        Self {
            policy,
            minds: HashMap::new(),
            running: None,
            switch: Measured::default(),
            control,
            engine: 0.0,
            window_switches: 0,
            window_worst: 0.0,
        }
    }

    /// The control law's name, for traces and reports.
    pub fn control(&self) -> &'static str {
        self.control.name()
    }

    /// One recorded input applied, the decision returned for a [`trace::Event::Decide`]: every
    /// input reaches the slicer through here when replayed, so a trace is the whole truth of a run.
    pub fn apply(&mut self, event: &trace::Event<K>, start: Instant) -> Option<Next<K>> {
        use trace::Event;
        match event {
            Event::Need(mind, need) => self.need(*mind, *need),
            Event::Ready(mind) => self.ready(*mind),
            Event::Done(mind) => self.done(mind),
            Event::Stepped(mind, took) => self.stepped(mind, *took),
            Event::Switched(mind, took) => self.switched(mind, *took),
            Event::Fetched(mind, took) => self.fetched(mind, *took),
            Event::Decide(at, _) => return Some(self.next(start + *at)),
        }
        None
    }

    /// The longest any mind waited in the window now open: what the wall is checked against.
    pub fn worst_wait(&self) -> Duration {
        Duration::from_secs_f64(self.window_worst.max(0.0))
    }

    /// Her folded need changed (a `sage/need` event): her share follows at once, and a rise in
    /// urgency while she waits brings her in at the next step.
    pub fn need(&mut self, mind: K, need: Need) {
        let floor = self.floor(&mind);
        let entry = self.minds.entry(mind).or_default();
        let rose = need.urgency > entry.need.urgency;
        entry.need = need;
        let running_is_her = self.running.as_ref().is_some_and(|r| r.mind == mind);
        if rose && entry.ready && !running_is_her {
            if let Some(floor) = floor {
                entry.pass = entry.pass.min(floor);
            }
            if let Some(running) = self.running.as_mut() {
                running.cut = true;
            }
        }
    }

    /// She has a turn to run. She starts at the ready floor: a mind that slept banks no credit.
    pub fn ready(&mut self, mind: K) {
        let floor = self.floor(&mind);
        let engine = self.engine;
        let entry = self.minds.entry(mind).or_default();
        if !entry.ready {
            entry.ready = true;
            entry.steps = 0;
            entry.waiting_since = Some(engine);
            if let Some(floor) = floor {
                entry.pass = entry.pass.max(floor);
            }
        }
    }

    /// Her turn ended: its length teaches the prediction of her next.
    pub fn done(&mut self, mind: &K) {
        let smoothing = self.policy.smoothing;
        if let Some(entry) = self.minds.get_mut(mind) {
            entry.ready = false;
            entry.turn_steps.add(entry.steps as f64, smoothing);
            entry.steps = 0;
            entry.waiting_since = None;
        }
        if self.running.as_ref().is_some_and(|r| &r.mind == mind) {
            self.running = None;
        }
    }

    /// She took one step, measured. Named, so a step reported after a handover is charged to her.
    /// Her first step after waiting ends her wait.
    pub fn stepped(&mut self, mind: &K, took: Duration) {
        let smoothing = self.policy.smoothing;
        let seconds = took.as_secs_f64();
        if let Some(entry) = self.minds.get_mut(mind) {
            if let Some(since) = entry.waiting_since.take() {
                self.window_worst = self.window_worst.max(self.engine - since);
            }
            entry.step.add(seconds, smoothing);
            entry.steps += 1;
            entry.pass += seconds / weight(entry.need);
        }
        self.engine += seconds;
        self.control.stepped(seconds);
    }

    /// Bringing her in took this long, measured; she pays for it. After a full rotation of the
    /// ready minds the loop closes its window and moves the slice.
    pub fn switched(&mut self, mind: &K, took: Duration) {
        let seconds = took.as_secs_f64();
        self.switch.add(seconds, self.policy.smoothing);
        if let Some(entry) = self.minds.get_mut(mind) {
            entry.pass += seconds / weight(entry.need);
        }
        self.engine += seconds;
        self.control.switched(seconds);
        self.window_switches += 1;
        let ready = self.minds.values().filter(|m| m.ready).count();
        if self.window_switches >= ready.max(1) {
            let floor = self.step_of(mind);
            let over = self.window_worst > self.policy.wait_bound.as_secs_f64();
            self.control.close(over, floor, self.wall());
            self.window_switches = 0;
            self.window_worst = 0.0;
        }
    }

    /// Her KV took this long to come back, measured: what the slice before her must cover.
    pub fn fetched(&mut self, mind: &K, took: Duration) {
        if let Some(entry) = self.minds.get_mut(mind) {
            entry.fetch.add(took.as_secs_f64(), self.policy.smoothing);
        }
    }

    /// The fetch of the mind who comes after `running` (the lowest pass among the other ready): a slice shorter than
    /// it never shortens her wait, since she waits for her KV either way, and only adds switches. 0 where nobody is
    /// waiting or her KV is a pointer.
    fn next_fetch(&self, running: &K) -> f64 {
        self.minds
            .iter()
            .filter(|(k, m)| m.ready && *k != running)
            .min_by(|(ka, a), (kb, b)| a.pass.total_cmp(&b.pass).then_with(|| ka.cmp(kb)))
            .map_or(0.0, |(_, m)| m.fetch.seconds()) // map_or: nobody waiting, nothing to cover
    }

    /// The longest slice that keeps every ready mind inside the wait bound. The lightest mind
    /// waits through everyone heavier: `(total weight / lightest - 1)` slices, each with a switch.
    fn wall(&self) -> f64 {
        let weights: Vec<f64> = self.minds.values().filter(|m| m.ready).map(|m| weight(m.need)).collect();
        let lightest = weights.iter().copied().fold(f64::INFINITY, f64::min);
        let total: f64 = weights.iter().sum();
        let ahead = if lightest.is_finite() { (total / lightest - 1.0).max(1.0) } else { 1.0 };
        self.policy.wait_bound.as_secs_f64() / ahead - self.switch.seconds()
    }

    /// When the running slice is spent and the lane must decide again; `None` while nothing can
    /// take the engine from her (nobody else ready, or she finishes under a switch).
    pub fn deadline(&self) -> Option<Instant> {
        let running = self.running.as_ref()?;
        let others = self.minds.iter().filter(|(k, m)| m.ready && **k != running.mind).count();
        if others == 0 {
            return None;
        }
        if running.cut {
            return Some(running.since);
        }
        // The earliest a waiting mind's guard falls due, as a wall instant.
        // An unrepresentable delay is no deadline from the guard, as an unrepresentable slice is none.
        let guard = self.guard(Some(&running.mind)).and_then(|(_, due)| {
            Duration::try_from_secs_f64((due - running.engine_at).max(0.0)).ok().map(|delay| running.since + delay)
        });
        let end = if self.finishes_under_a_switch(&running.mind) {
            None
        } else {
            Duration::try_from_secs_f64(self.slice(&running.mind)).ok().map(|slice| running.since + slice)
        };
        match (guard, end) {
            (Some(g), Some(e)) => Some(g.min(e)),
            (g, e) => g.or(e),
        }
    }

    /// The decision at this step boundary.
    pub fn next(&mut self, now: Instant) -> Next<K> {
        let Some(best) = self.lowest_ready() else {
            self.running = None;
            return Next::Idle;
        };
        let Some(running) = &self.running else {
            // The lane is free (a turn ended): a mind out of slack still comes first.
            let pick = self.pick(best);
            return self.bring_in(pick, now);
        };
        let others = self.minds.iter().filter(|(k, m)| m.ready && **k != running.mind).count();
        if others == 0 {
            return Next::Continue;
        }
        // The bound is a guarantee, not a hope: a waiting mind whose slack is gone runs now, whatever
        // the shares say (stride sets the shares; this guard keeps the bound, as an RTOS deadline).
        if let Some((due, at)) = self.guard(Some(&running.mind)) {
            if at <= self.engine {
                return self.bring_in(due, now);
            }
        }
        if !running.cut {
            if self.finishes_under_a_switch(&running.mind) {
                return Next::Continue;
            }
            if now.saturating_duration_since(running.since).as_secs_f64() < self.slice(&running.mind) {
                return Next::Continue;
            }
        }
        if best == running.mind {
            // Her weight still makes her the lowest pass: a fresh slice, no switch.
            self.running = Some(Running { mind: best, since: now, engine_at: self.engine, cut: false });
            return Next::Continue;
        }
        let pick = self.pick(best);
        self.bring_in(pick, now)
    }

    /// The mind holding the engine now.
    pub fn running(&self) -> Option<&K> {
        self.running.as_ref().map(|r| &r.mind)
    }

    fn bring_in(&mut self, mind: K, now: Instant) -> Next<K> {
        // The mind handing over begins to wait, if she still has decoding to do.
        if let Some(previous) = self.running.take() {
            let engine = self.engine;
            if let Some(entry) = self.minds.get_mut(&previous.mind).filter(|m| m.ready) {
                entry.waiting_since = Some(engine);
            }
        }
        self.running = Some(Running { mind, since: now, engine_at: self.engine, cut: false });
        Next::Switch(mind)
    }

    /// Who comes in at a switch: the stride's choice, unless a waiting mind would miss her bound
    /// during that choice's switch and first step (the next boundary the guard sees), in which case
    /// she does.
    fn pick(&self, best: K) -> K {
        let switch = self.switch.seconds();
        let first_step = self.step_of(&best);
        match self.guard(None) {
            Some((due, at)) if at <= self.engine + switch + first_step => due,
            _ => best,
        }
    }

    /// The waiting mind whose bound falls due first, and the engine time it falls due: the last
    /// moment she can still be brought in and step inside the bound (her wait so far, then one more
    /// step of the running mind, a switch and her own step).
    fn guard(&self, running: Option<&K>) -> Option<(K, f64)> {
        let bound = self.policy.wait_bound.as_secs_f64();
        let switch = self.switch.seconds();
        // An engine with no one running has no step to finish before a switch.
        let running_step = match running {
            Some(mind) => self.step_of(mind),
            None => 0.0,
        };
        let mut due: Vec<(f64, f64, K)> = self
            .minds
            .iter()
            .filter(|(k, m)| m.ready && Some(*k) != running)
            .filter_map(|(k, m)| {
                let since = m.waiting_since?;
                let service = switch + m.step.seconds();
                Some((since + bound - running_step - service, service, *k))
            })
            .collect();
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
        // Served earliest-due first, each mind also waits for the service of every mind due before
        // her: her guard falls that much earlier (EDF feasibility), so two due together both make it.
        // The mind to bring in is the earliest due (EDF order); the instant is the tightest guard of
        // all, which may belong to a mind behind her: bringing her in then is what lets that one
        // make it too.
        let first = due.first().map(|(_, _, mind)| *mind)?;
        let mut ahead = 0.0;
        let mut tightest = f64::INFINITY;
        for (at, service, _) in &due {
            tightest = tightest.min(at - ahead);
            ahead += service;
        }
        Some((first, tightest))
    }

    /// Seconds the running mind keeps the engine: the loop's slice, never past the wall as the
    /// ready set stands now, never under one of her own steps.
    fn slice(&self, mind: &K) -> f64 {
        let step = self.step_of(mind);
        // never under the next mind's fetch: under it the slice saves her no wait and costs the lane a switch (the 5090's
        // day, simulated 2026-11-26: the wall shrank slices below an 8.7 GB copy and the lane spent a seventh of it switching)
        self.control.slice().min(self.wall()).max(step).max(self.next_fetch(mind)).max(0.0)
    }

    /// One mind's step, in seconds: her measured mean, or nothing for a mind not seen stepping yet.
    fn step_of(&self, mind: &K) -> f64 {
        match self.minds.get(mind) {
            Some(seen) => seen.step.seconds(),
            None => 0.0,
        }
    }

    fn finishes_under_a_switch(&self, mind: &K) -> bool {
        let (Some(entry), Some(switch)) = (self.minds.get(mind), self.switch.0) else { return false };
        let (Some(turn), Some(step)) = (entry.turn_steps.0, entry.step.0) else { return false };
        let left = (turn - entry.steps as f64).max(0.0);
        left > 0.0 && left * step < switch
    }

    /// The lowest pass among the ready, ties to the lowest key.
    fn lowest_ready(&self) -> Option<K> {
        self.minds
            .iter()
            .filter(|(_, m)| m.ready)
            .min_by(|(ka, a), (kb, b)| a.pass.total_cmp(&b.pass).then_with(|| ka.cmp(kb)))
            .map(|(k, _)| *k)
    }

    /// The lowest pass among the other ready minds: where a waking or rising mind starts.
    fn floor(&self, mind: &K) -> Option<f64> {
        self.minds
            .iter()
            .filter(|(k, m)| m.ready && *k != mind)
            .map(|(_, m)| m.pass)
            .min_by(f64::total_cmp)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    /// The simulated engine the lane's tests run on: what one step and one switch cost there. Test
    /// inputs of the M5's order, not a claim about any node; the live costs are measured (`Measured`).
    pub(crate) mod fixture {
        use std::time::Duration;

        /// What the simulated engine charges for a step and for a switch.
        pub(crate) struct Costs {
            pub step: Duration,
            pub switch: Duration,
        }

        pub(crate) const ENGINE: Costs = Costs { step: Duration::from_millis(20), switch: Duration::from_millis(26) };
    }

    use super::sim::{simulate, MindLoad, Workload};
    use super::*;
    use crate::slice::device::Unified;
    /// The lane's own tests decode only (their minds' prompts are 0), on the M5's measured prefill.
    const M5: Unified = Unified::M5_AT_64K;
    const CHUNK: std::num::NonZeroU64 = std::num::NonZeroU64::MIN;
    use fixture::ENGINE;


    fn policy() -> Policy {
        Policy { wait_bound: Duration::from_secs(1), smoothing: 0.25 }
    }

    fn need(urgency: Urgency) -> Need {
        Need { urgency, waiting: 0 }
    }

    /// A mind decoding without end from `starts`.
    fn decoding(mind: char, urgency: Urgency, starts: Duration) -> MindLoad<char> {
        MindLoad { mind, need: need(urgency), changes: Vec::new(), step: ENGINE.step, turn: u64::MAX, rest: Duration::ZERO, starts, prompt: 0, kv: 0 }
    }

    // what this catches: one mind holding the engine for a whole turn while equals wait (the decode
    // permit held per generation, 2026-11-22). Equal needs alternate: the engine splits evenly, every
    // mind's next step comes inside the bound, and switching stays near what the wall allows.
    #[test]
    fn equal_minds_take_turns_inside_the_bound() {
        let load = Workload { minds: ['a', 'b', 'c'].map(|m| decoding(m, Urgency::Want, Duration::ZERO)).to_vec(), switch: ENGINE.switch, chunk: CHUNK };
        let (report, _) = simulate(&mut Slicer::new(policy()), &load, &M5, u64::MAX, "baseline", Duration::from_secs(60));
        for mind in ['a', 'b', 'c'] {
            assert!((report.share[&mind] - 1.0 / 3.0).abs() < 0.02, "{mind} got {:.3} of the engine", report.share[&mind]);
        }
        assert!(report.worst().is_none_or(|w| w <= Duration::from_secs(1)), "a mind waited {:?}", report.worst());
        // The best the wall allows: (1 s / 2 others) less one switch, so 474 ms per 26 ms switch, 48.7 steps/s.
        assert!(report.throughput > 0.9 * 50.0, "throughput {:.1} steps/s", report.throughput);
        assert!(report.switches > 50, "they alternated ({} switches), nobody held a turn", report.switches);
    }

    // what this catches: shares fixed at admission instead of following SAGE's events. A mind
    // addressed mid-slice (urgency rises) takes the very next step; an urgent mind gets about twice
    // the engine of a wanting one, and the wanting one still runs.
    #[test]
    fn need_events_move_the_share_and_a_rise_takes_the_next_step() {
        let mut slicer = Slicer::new(policy());
        slicer.need('a', need(Urgency::Want));
        slicer.ready('a');
        slicer.need('b', need(Urgency::Content));
        slicer.ready('b');
        let start = Instant::now();
        assert_eq!(slicer.next(start), Next::Switch('a'));
        slicer.switched(&'a', ENGINE.switch);
        slicer.stepped(&'a', ENGINE.step);
        slicer.need('b', need(Urgency::Urgent));
        assert_eq!(slicer.next(start + ENGINE.step + ENGINE.switch), Next::Switch('b'), "the rise takes the next step, mid-slice");

        // Over a run, the shares follow the needs: content b gets half of want a; once b's need
        // rises a minute in, urgent b gets twice a over the rest.
        let mut b = decoding('b', Urgency::Content, Duration::ZERO);
        b.changes = vec![(Duration::from_secs(60), need(Urgency::Urgent))];
        let load = Workload { minds: vec![decoding('a', Urgency::Want, Duration::ZERO), b], switch: ENGINE.switch, chunk: CHUNK };
        let (before, _) = simulate(&mut Slicer::new(policy()), &load, &M5, u64::MAX, "baseline", Duration::from_secs(60));
        let ratio = before.share[&'a'] / before.share[&'b'];
        assert!((ratio - 2.0).abs() < 0.25, "want got {ratio:.2}x of content");
        let (whole, _) = simulate(&mut Slicer::new(policy()), &load, &M5, u64::MAX, "baseline", Duration::from_secs(180));
        // 60 s at 2:1 for a, then 120 s at 1:2: b's share over the whole run is (1/3 + 2 * 2/3) / 3.
        assert!((whole.share[&'b'] - 5.0 / 9.0).abs() < 0.04, "b's share {:.3} follows the rise", whole.share[&'b']);
        assert!(whole.worst().is_none_or(|w| w <= Duration::from_secs(1)), "through the rise nobody waited past the bound: {:?}", whole.worst());
    }

    // what this catches: a switch spent on a mind about to finish. Her measured turn length says she
    // has fewer steps left than one switch costs, so she keeps the engine past her slice.
    #[test]
    fn a_mind_finishing_under_one_switch_keeps_the_engine() {
        let mut slicer = Slicer::new(policy());
        slicer.need('a', need(Urgency::Want));
        slicer.ready('a');
        let start = Instant::now();
        assert_eq!(slicer.next(start), Next::Switch('a'));
        slicer.switched(&'a', ENGINE.switch);
        for _ in 0..40 {
            slicer.stepped(&'a', ENGINE.step);
        }
        slicer.done(&'a'); // she learns her turns run 40 steps
        slicer.ready('a');
        slicer.need('b', need(Urgency::Want));
        slicer.ready('b');
        let mut now = start + Duration::from_secs(2);
        assert_eq!(slicer.next(now), Next::Switch('a'));
        slicer.switched(&'a', ENGINE.switch);
        for _ in 0..39 {
            slicer.stepped(&'a', ENGINE.step);
        }
        now += Duration::from_secs(5); // far past any slice
        assert_eq!(slicer.next(now), Next::Continue, "one step (20 ms) left is under one switch (26 ms)");
    }

    // what this catches: a mind that slept banking credit and locking the others out on waking.
    // b arrives a minute in; she starts at the ready floor, so a keeps running inside the bound.
    #[test]
    fn a_waking_mind_banks_no_credit() {
        let load = Workload {
            minds: vec![decoding('a', Urgency::Want, Duration::ZERO), decoding('b', Urgency::Want, Duration::from_secs(60))],
            switch: ENGINE.switch,
            chunk: CHUNK,
        };
        let (report, _) = simulate(&mut Slicer::new(policy()), &load, &M5, u64::MAX, "baseline", Duration::from_secs(70));
        assert!(report.worst_wait.get(&'a').is_some_and(|w| *w <= Duration::from_secs(1)), "a waited {:?}", report.worst_wait.get(&'a'));
        assert!(report.share[&'b'] > 0.0);
    }
}
