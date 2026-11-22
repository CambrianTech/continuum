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
pub mod tuner;

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
#[derive(Debug, Clone, PartialEq, Eq)]
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
}

#[derive(Debug, Default)]
struct Mind {
    need: Need,
    ready: bool,
    /// Engine seconds used over her weight: the lowest ready pass steps next.
    pass: f64,
    /// Seconds per step.
    step: Measured,
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
    tuner: Tuner,
    /// Engine seconds measured so far (steps and switches): the clock waits are measured on.
    engine: f64,
    /// This window: switches made, and the longest any mind waited.
    window_switches: usize,
    window_worst: f64,
}

impl<K: Clone + Eq + Hash + Ord> Slicer<K> {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            minds: HashMap::new(),
            running: None,
            switch: Measured::default(),
            tuner: Tuner::default(),
            engine: 0.0,
            window_switches: 0,
            window_worst: 0.0,
        }
    }

    /// The longest any mind waited in the window now open: what the wall is checked against.
    pub fn worst_wait(&self) -> Duration {
        Duration::from_secs_f64(self.window_worst.max(0.0))
    }

    /// Her folded need changed (a `sage/need` event): her share follows at once, and a rise in
    /// urgency while she waits brings her in at the next step.
    pub fn need(&mut self, mind: K, need: Need) {
        let floor = self.floor(&mind);
        let entry = self.minds.entry(mind.clone()).or_default();
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
        self.tuner.stepped(seconds);
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
        self.tuner.switched(seconds);
        self.window_switches += 1;
        let ready = self.minds.values().filter(|m| m.ready).count();
        if self.window_switches >= ready.max(1) {
            let floor = self.minds.get(mind).and_then(|m| m.step.0).unwrap_or(0.0);
            let over = self.window_worst > self.policy.wait_bound.as_secs_f64();
            self.tuner.close(over, floor, self.wall());
            self.window_switches = 0;
            self.window_worst = 0.0;
        }
    }

    /// The longest slice that keeps every ready mind inside the wait bound. The lightest mind
    /// waits through everyone heavier: `(total weight / lightest - 1)` slices, each with a switch.
    fn wall(&self) -> f64 {
        let weights: Vec<f64> = self.minds.values().filter(|m| m.ready).map(|m| weight(m.need)).collect();
        let lightest = weights.iter().copied().fold(f64::INFINITY, f64::min);
        let total: f64 = weights.iter().sum();
        let ahead = if lightest.is_finite() { (total / lightest - 1.0).max(1.0) } else { 1.0 };
        self.policy.wait_bound.as_secs_f64() / ahead - self.switch.0.unwrap_or(0.0)
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
        let guard = self.guard(&running.mind).map(|(_, due)| {
            running.since + Duration::try_from_secs_f64((due - running.engine_at).max(0.0)).unwrap_or_default()
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
            return self.bring_in(best, now);
        };
        let others = self.minds.iter().filter(|(k, m)| m.ready && **k != running.mind).count();
        if others == 0 {
            return Next::Continue;
        }
        // The bound is a guarantee, not a hope: a waiting mind whose slack is gone runs now, whatever
        // the shares say (stride sets the shares; this guard keeps the bound, as an RTOS deadline).
        if let Some((due, at)) = self.guard(&running.mind) {
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
        self.bring_in(best, now)
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
        self.running = Some(Running { mind: mind.clone(), since: now, engine_at: self.engine, cut: false });
        Next::Switch(mind)
    }

    /// The waiting mind whose bound falls due first, and the engine time it falls due: the last
    /// moment she can still be brought in and step inside the bound (her wait so far, then one more
    /// step of the running mind, a switch and her own step).
    fn guard(&self, running: &K) -> Option<(K, f64)> {
        let bound = self.policy.wait_bound.as_secs_f64();
        let switch = self.switch.0.unwrap_or(0.0);
        let running_step = self.minds.get(running).and_then(|m| m.step.0).unwrap_or(0.0);
        self.minds
            .iter()
            .filter(|(k, m)| m.ready && *k != running)
            .filter_map(|(k, m)| {
                let since = m.waiting_since?;
                let margin = running_step + switch + m.step.0.unwrap_or(0.0);
                Some((k.clone(), since + bound - margin))
            })
            .min_by(|(ka, a), (kb, b)| a.total_cmp(b).then_with(|| ka.cmp(kb)))
    }

    /// Seconds the running mind keeps the engine: the loop's slice, never past the wall as the
    /// ready set stands now, never under one of her own steps.
    fn slice(&self, mind: &K) -> f64 {
        let step = self.minds.get(mind).and_then(|m| m.step.0).unwrap_or(0.0);
        self.tuner.slice().min(self.wall()).max(step).max(0.0)
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
            .map(|(k, _)| k.clone())
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
mod tests {
    use super::*;

    const STEP: Duration = Duration::from_millis(20);
    const SWITCH: Duration = Duration::from_millis(26);

    fn policy() -> Policy {
        Policy { wait_bound: Duration::from_secs(1), smoothing: 0.25 }
    }

    fn need(urgency: Urgency) -> Need {
        Need { urgency, waiting: 0 }
    }

    /// Runs the engine for `steps` step boundaries: each step takes STEP, each switch SWITCH.
    /// Returns the engine seconds each mind stepped, the switches, and the longest any ready mind
    /// went between its steps.
    struct Run {
        stepped: HashMap<char, f64>,
        switches: usize,
        longest_wait: HashMap<char, Duration>,
    }

    fn run(slicer: &mut Slicer<char>, start: Instant, steps: usize, mut event: impl FnMut(usize, &mut Slicer<char>)) -> Run {
        let mut now = start;
        let mut out = Run { stepped: HashMap::new(), switches: 0, longest_wait: HashMap::new() };
        let mut last: HashMap<char, Instant> = HashMap::new();
        for i in 0..steps {
            event(i, slicer);
            let had = slicer.running().is_some();
            match slicer.next(now) {
                Next::Idle => break,
                Next::Switch(mind) => {
                    if had {
                        out.switches += 1;
                    }
                    now += SWITCH;
                    slicer.switched(&mind, SWITCH);
                }
                Next::Continue => {}
            }
            let Some(&mind) = slicer.running() else { break };
            if let Some(prev) = last.get(&mind) {
                let gap = now.saturating_duration_since(*prev);
                let worst = out.longest_wait.entry(mind).or_default();
                *worst = (*worst).max(gap);
            }
            now += STEP;
            slicer.stepped(&mind, STEP);
            last.insert(mind, now);
            *out.stepped.entry(mind).or_default() += STEP.as_secs_f64();
        }
        out
    }

    // what this catches: one mind holding the engine for a whole turn while equals wait (the decode
    // permit held per generation, 2026-11-22). Equal needs alternate at the measured slice: switches
    // stay near the switch share, every mind's next step comes within the wait target, and the
    // engine time splits evenly.
    #[test]
    fn equal_minds_take_turns_at_the_measured_slice() {
        let mut slicer = Slicer::new(policy());
        for mind in ['a', 'b', 'c'] {
            slicer.need(mind, need(Urgency::Want));
            slicer.ready(mind);
        }
        let run = run(&mut slicer, Instant::now(), 3_000, |_, _| {});
        let total: f64 = run.stepped.values().sum();
        for mind in ['a', 'b', 'c'] {
            let share = run.stepped[&mind] / total;
            assert!((share - 1.0 / 3.0).abs() < 0.02, "{mind} got {share:.3} of the engine");
            assert!(run.longest_wait[&mind] <= Duration::from_secs(1) + STEP, "{mind} waited {:?}", run.longest_wait[&mind]);
        }
        let switching = run.switches as f64 * SWITCH.as_secs_f64();
        // The best the wall allows: a slice of (1 s / 2 others) less one switch, so 474 ms per 26 ms switch.
        assert!(switching / (switching + total) <= 0.10, "switching took {:.3} of the engine; the wall allows 0.052", switching / (switching + total));
        assert!(run.switches > 50, "they alternated ({} switches), nobody held a turn", run.switches);
    }

    // what this catches: shares fixed at admission instead of following SAGE's events. A mind
    // addressed mid-slice (urgency rises) takes the very next step; afterwards the higher need gets
    // about twice the engine of the lower one, and the lower one still runs.
    #[test]
    fn need_events_move_the_share_and_a_rise_takes_the_next_step() {
        let mut slicer = Slicer::new(policy());
        slicer.need('a', need(Urgency::Want));
        slicer.ready('a');
        slicer.need('b', need(Urgency::Content));
        slicer.ready('b');
        let start = Instant::now();
        assert_eq!(slicer.next(start), Next::Switch('a'));
        slicer.switched(&'a', SWITCH);
        slicer.stepped(&'a', STEP);
        slicer.need('b', need(Urgency::Urgent));
        assert_eq!(slicer.next(start + STEP + SWITCH), Next::Switch('b'), "the rise takes the next step, mid-slice");

        let run = run(&mut slicer, start + Duration::from_secs(1), 6_000, |_, _| {});
        let ratio = run.stepped[&'b'] / run.stepped[&'a'];
        assert!((ratio - 2.0).abs() < 0.2, "urgent got {ratio:.2}x of want's engine");
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
        slicer.switched(&'a', SWITCH);
        for _ in 0..40 {
            slicer.stepped(&'a', STEP);
        }
        slicer.done(&'a'); // she learns her turns run 40 steps
        slicer.ready('a');
        slicer.need('b', need(Urgency::Want));
        slicer.ready('b');
        let mut now = start + Duration::from_secs(2);
        assert_eq!(slicer.next(now), Next::Switch('a'));
        slicer.switched(&'a', SWITCH);
        for _ in 0..39 {
            slicer.stepped(&'a', STEP);
        }
        now += Duration::from_secs(5); // far past any slice
        assert_eq!(slicer.next(now), Next::Continue, "one step (20 ms) left is under one switch (26 ms)");
    }

    // what this catches: a mind that slept banking credit and locking the others out on waking.
    // She starts at the ready floor, so the engine splits evenly from her return.
    #[test]
    fn a_waking_mind_banks_no_credit() {
        let mut slicer = Slicer::new(policy());
        slicer.need('a', need(Urgency::Want));
        slicer.ready('a');
        slicer.need('b', need(Urgency::Want));
        let start = Instant::now();
        run(&mut slicer, start, 500, |_, _| {});
        slicer.ready('b');
        let run = run(&mut slicer, start + Duration::from_secs(60), 100, |_, _| {});
        assert!(run.stepped.get(&'a').copied().unwrap_or(0.0) > 0.0, "a still runs after b wakes");
        assert!(run.longest_wait[&'a'] <= Duration::from_secs(1) + STEP);
    }
}
