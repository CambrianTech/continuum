//! The lane in simulation: any control law against any workload, in milliseconds, on any machine.
//!
//! A workload is the minds sharing the lane: each with her need (and when it changes), her step
//! time, how many steps a turn of hers decodes, and how long she spends off the lane between turns
//! (tools, perception, thought: her own workers, never waiting on the lane). Write one by hand, or
//! fit one from a live trace ([`Workload::fit`]). [`run`] drives a [`Slicer`] through it, records
//! the run as a [`Trace`] (every input through [`Slicer::apply`], so the trace replays exactly),
//! and reports what a live read reports: throughput, each mind's worst wait and share.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

use crate::component::Need;

use super::trace::{Event, Trace};
use super::{Next, Slicer};

/// One mind's load on the lane.
#[derive(Debug, Clone)]
pub struct MindLoad<K> {
    pub mind: K,
    pub need: Need,
    /// Her need changing at these offsets from the start.
    pub changes: Vec<(Duration, Need)>,
    /// One decode step of hers.
    pub step: Duration,
    /// Steps per turn.
    pub turn: u64,
    /// Off the lane between turns.
    pub rest: Duration,
    /// When her first turn arrives.
    pub starts: Duration,
}

/// The minds sharing one lane, and what a switch costs.
#[derive(Debug, Clone)]
pub struct Workload<K> {
    pub minds: Vec<MindLoad<K>>,
    pub switch: Duration,
}

/// What a run measured.
#[derive(Debug, Clone)]
pub struct Report<K> {
    pub control: &'static str,
    pub steps: u64,
    pub switches: u64,
    /// Steps per second of lane time (stepping and switching; idle excluded).
    pub throughput: f64,
    /// Each mind's longest wait: from her turn arriving, or her last step, to her next step.
    pub worst_wait: HashMap<K, Duration>,
    /// Each mind's share of the lane's stepping.
    pub share: HashMap<K, f64>,
}

impl<K> Report<K> {
    /// The longest any mind waited; `None` when nobody waited.
    pub fn worst(&self) -> Option<Duration> {
        self.worst_wait.values().copied().max()
    }
}

struct State {
    /// Steps left in her turn; 0 while she is off the lane.
    left: u64,
    resume: Duration,
    next_change: usize,
    /// When she was last served or her turn arrived, while she has steps left.
    waiting_since: Option<Duration>,
}

fn feed<K: Copy + Eq + Hash + Ord>(slicer: &mut Slicer<K>, trace: &mut Trace<K>, start: Instant, event: Event<K>) -> Option<Next<K>> {
    let decided = slicer.apply(&event, start);
    let event = match (event, &decided) {
        (Event::Decide(at, _), Some(next)) => Event::Decide(at, *next),
        (event, _) => event,
    };
    trace.events.push(event);
    decided
}

/// Drive `slicer` through `load` for `length` of simulated time.
pub fn run<K: Copy + Eq + Hash + Ord>(slicer: &mut Slicer<K>, load: &Workload<K>, length: Duration) -> (Report<K>, Trace<K>) {
    let start = Instant::now();
    let mut trace = Trace { control: slicer.control(), events: Vec::new() };
    let mut states: Vec<State> =
        load.minds.iter().map(|m| State { left: 0, resume: m.starts, next_change: 0, waiting_since: None }).collect();
    let mut report = Report {
        control: slicer.control(),
        steps: 0,
        switches: 0,
        throughput: 0.0,
        worst_wait: HashMap::new(),
        share: HashMap::new(),
    };
    for m in &load.minds {
        feed(slicer, &mut trace, start, Event::Need(m.mind, m.need));
    }
    let (mut now, mut busy) = (Duration::ZERO, Duration::ZERO);
    let mut stepped: HashMap<K, Duration> = HashMap::new();
    while now < length {
        for (m, s) in load.minds.iter().zip(states.iter_mut()) {
            while let Some((_, need)) = m.changes.get(s.next_change).copied().filter(|(at, _)| *at <= now) {
                feed(slicer, &mut trace, start, Event::Need(m.mind, need));
                s.next_change += 1;
            }
            if s.left == 0 && s.resume <= now {
                s.left = m.turn;
                s.waiting_since = Some(now);
                feed(slicer, &mut trace, start, Event::Ready(m.mind));
            }
        }
        let had = slicer.running().is_some();
        let held = match feed(slicer, &mut trace, start, Event::Decide(now, Next::Idle)) {
            Some(Next::Switch(mind)) => {
                if had {
                    report.switches += 1;
                }
                now += load.switch;
                busy += load.switch;
                feed(slicer, &mut trace, start, Event::Switched(mind, load.switch));
                mind
            }
            Some(Next::Continue) => match slicer.running() {
                Some(mind) => *mind,
                None => break,
            },
            _ => {
                // Nobody has decoding to do: the lane idles until the next turn arrives.
                match states.iter().filter(|s| s.left == 0).map(|s| s.resume).filter(|r| *r > now).min() {
                    Some(resume) => {
                        now = resume;
                        continue;
                    }
                    None => break,
                }
            }
        };
        let Some(i) = load.minds.iter().position(|m| m.mind == held) else { break };
        let (m, s) = (&load.minds[i], &mut states[i]);
        if let Some(since) = s.waiting_since {
            let worst = report.worst_wait.entry(held).or_default();
            *worst = (*worst).max(now.saturating_sub(since));
        }
        now += m.step;
        busy += m.step;
        report.steps += 1;
        *stepped.entry(held).or_default() += m.step;
        feed(slicer, &mut trace, start, Event::Stepped(held, m.step));
        s.left = s.left.saturating_sub(1);
        if s.left == 0 {
            s.waiting_since = None;
            s.resume = now + m.rest;
            feed(slicer, &mut trace, start, Event::Done(held));
        } else {
            s.waiting_since = Some(now);
        }
    }
    report.throughput = if busy.is_zero() { 0.0 } else { report.steps as f64 / busy.as_secs_f64() };
    let total: Duration = stepped.values().sum();
    report.share = stepped.into_iter().map(|(k, d)| (k, d.as_secs_f64() / total.as_secs_f64().max(f64::MIN_POSITIVE))).collect();
    (report, trace)
}

impl<K: Copy + Eq + Hash + Ord> Workload<K> {
    /// The load a trace carried, as means per mind: her step, her turn length, her time off the
    /// lane, her first need and its changes; the lane's switch. Drives the simulator on what a
    /// live node actually saw.
    pub fn fit(trace: &Trace<K>) -> Self {
        #[derive(Default)]
        struct Seen {
            need: Option<Need>,
            changes: Vec<(Duration, Need)>,
            step: (Duration, u32),
            turns: (u64, u32),
            in_turn: u64,
            rest: (Duration, u32),
            done_at: Option<Duration>,
            first_ready: Option<Duration>,
        }
        /// Her record, in first-seen order.
        fn of<'a, K: Copy + Eq + Hash>(seen: &'a mut HashMap<K, Seen>, order: &mut Vec<K>, k: &K) -> &'a mut Seen {
            if !seen.contains_key(k) {
                order.push(*k);
            }
            seen.entry(*k).or_default()
        }
        let mut order: Vec<K> = Vec::new();
        let mut seen: HashMap<K, Seen> = HashMap::new();
        let (mut now, mut switch) = (Duration::ZERO, (Duration::ZERO, 0u32));
        for event in &trace.events {
            match event {
                Event::Decide(at, _) => now = *at,
                Event::Switched(_, took) => switch = (switch.0 + *took, switch.1 + 1),
                Event::Need(k, need) => {
                    let s = of(&mut seen, &mut order, k);
                    match s.need {
                        None => s.need = Some(*need),
                        Some(_) => s.changes.push((now, *need)),
                    }
                }
                Event::Ready(k) => {
                    let s = of(&mut seen, &mut order, k);
                    s.first_ready.get_or_insert(now);
                    if let Some(done) = s.done_at.take() {
                        s.rest = (s.rest.0 + now.saturating_sub(done), s.rest.1 + 1);
                    }
                }
                Event::Stepped(k, took) => {
                    let s = of(&mut seen, &mut order, k);
                    s.step = (s.step.0 + *took, s.step.1 + 1);
                    s.in_turn += 1;
                }
                Event::Done(k) => {
                    let s = of(&mut seen, &mut order, k);
                    s.turns = (s.turns.0 + s.in_turn, s.turns.1 + 1);
                    s.in_turn = 0;
                    s.done_at = Some(now);
                }
            }
        }
        let mean = |(sum, n): (Duration, u32)| if n == 0 { Duration::ZERO } else { sum / n };
        let minds = order
            .into_iter()
            .filter_map(|k| {
                let s = seen.remove(&k)?;
                // A mind never ready in the trace never competed; replaying her, she never does.
                let starts = s.first_ready?;
                let need = s.need.unwrap_or_default(); // JUSTIFIED: a mind that never said her need runs at the need the slicer gives an unannounced one
                let turn = if s.turns.1 == 0 { s.in_turn.max(1) } else { s.turns.0 / u64::from(s.turns.1) };
                Some(MindLoad {
                    mind: k,
                    need,
                    changes: s.changes,
                    step: mean(s.step),
                    turn: turn.max(1),
                    rest: mean(s.rest),
                    starts,
                })
            })
            .collect();
        Workload { minds, switch: mean(switch) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Urgency;
    use crate::slice::control::Fixed;
    use crate::slice::trace::replay;
    use crate::Policy;

    fn policy() -> Policy {
        Policy { wait_bound: Duration::from_secs(1), smoothing: 0.25 }
    }

    /// Three residents as the M5 runs them: 20 ms steps, turns of a few hundred tokens, seconds of
    /// tool work off the lane between turns.
    fn residents() -> Workload<char> {
        let mind = |mind, urgency, turn, rest_ms| MindLoad {
            mind,
            need: Need { urgency, waiting: 0 },
            changes: Vec::new(),
            step: Duration::from_millis(20),
            turn,
            rest: Duration::from_millis(rest_ms),
            starts: Duration::ZERO,
        };
        Workload {
            minds: vec![mind('a', Urgency::Want, 400, 3_000), mind('b', Urgency::Want, 250, 1_500), mind('c', Urgency::Urgent, 600, 5_000)],
            switch: Duration::from_millis(26),
        }
    }

    // what this catches: a control law that cannot be compared before it reaches a live node. Two
    // laws on the same load, side by side: the throughput loop beats switching every step and holds
    // the bound; a long fixed slice gets throughput by breaking the bound.
    #[test]
    fn control_laws_compare_on_one_workload() {
        let load = residents();
        let length = Duration::from_secs(300);
        let (climb, _) = run(&mut Slicer::new(policy()), &load, length);
        let (every_step, _) = run(&mut Slicer::with(policy(), Box::new(Fixed(0.0))), &load, length);
        let (long, _) = run(&mut Slicer::with(policy(), Box::new(Fixed(5.0))), &load, length);
        assert!(climb.throughput > every_step.throughput * 1.3, "climb {:.1} vs every step {:.1}", climb.throughput, every_step.throughput);
        assert!(climb.worst().is_none_or(|w| w <= Duration::from_secs(1)), "climb waited {:?}", climb.worst());
        // The wall caps even a fixed law: the slicer, not the law, keeps the bound.
        assert!(long.worst().is_none_or(|w| w <= Duration::from_secs(1)), "fixed 5 s waited {:?}", long.worst());
        assert_eq!((climb.control, every_step.control), ("throughput-climb", "fixed"));
    }

    // what this catches: a slicer that is not a pure function of its inputs (a hidden clock, a hash
    // order). A recorded run replays decision for decision.
    #[test]
    fn a_trace_replays_exactly() {
        let (_, trace) = run(&mut Slicer::new(policy()), &residents(), Duration::from_secs(120));
        let decisions = replay(&trace, &mut Slicer::new(policy()));
        assert!(matches!(decisions, Ok(n) if n > 1_000), "replayed {decisions:?}");
    }

    // what this catches: a live trace that cannot drive the simulator. The workload fitted from a
    // run's trace gives back each mind's step, turn length and need, and the lane's switch.
    #[test]
    fn a_trace_fits_back_into_its_workload() {
        let load = residents();
        let (_, trace) = run(&mut Slicer::new(policy()), &load, Duration::from_secs(300));
        let fitted = Workload::fit(&trace);
        assert_eq!(fitted.switch, load.switch);
        for (want, got) in load.minds.iter().zip(&fitted.minds) {
            assert_eq!((got.mind, got.step, got.turn, got.need), (want.mind, want.step, want.turn, want.need));
            let rest = got.rest.as_secs_f64() / want.rest.as_secs_f64();
            assert!((rest - 1.0).abs() < 0.05, "{}: rest {:?} vs {:?}", want.mind, got.rest, want.rest);
        }
    }
}
