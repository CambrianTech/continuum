//! A complete capture interval folded back into what the scheduler faced: the ready set at every
//! step, what it chose from it, and each mind's progress (her acts, their time, and the time she was
//! runnable but left waiting). The same fold reads production and a replayed policy, so the two are
//! compared on one definition.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use continuum_base::capture::{At, CompleteInterval};
use continuum_kernel::runtime::stage::MindId;

use super::{EngineIncarnation, SequenceChange, SequenceId, WorkloadEvent};

/// A sequence across relaunches: an id is only meaningful on its own engine.
pub type SequenceKey = (EngineIncarnation, SequenceId);

/// One step as the scheduler saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepView {
    pub at: At,
    pub took: Duration,
    /// Runnable when the step began, ordered.
    pub ready: Vec<Key>,
    /// What the step ran.
    pub ran: Vec<Key>,
}

/// One mind's share of the interval.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MindProgress {
    pub acts: u32,
    /// Arrival to completion, summed over her completed acts.
    pub act_time: Duration,
    /// Step time during which she was runnable and not run.
    pub left_waiting: Duration,
    /// The longest unbroken run of steps she was runnable for and not run: starvation.
    pub longest_wait: Duration,
}

/// What a complete interval comes to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replayed {
    pub steps: Vec<StepView>,
    pub minds: BTreeMap<MindId, MindProgress>,
}

#[derive(Debug)]
struct Live {
    mind: MindId,
    arrived: At,
    ready: bool,
    waiting: Duration,
}

/// Fold `interval` into the steps the scheduler took and each mind's progress.
pub fn replay(interval: &CompleteInterval<WorkloadEvent>) -> Replayed {
    let mut live: BTreeMap<Key, Live> = BTreeMap::new();
    let mut out = Replayed::default();
    for stamped in interval.events() {
        match &stamped.event {
            WorkloadEvent::Sequence { engine, sequence, mind, change } => {
                let key = (*engine, *sequence);
                match change {
                    SequenceChange::Arrived { .. } => {
                        live.insert(key, Live { mind: *mind, arrived: stamped.at, ready: false, waiting: Duration::ZERO });
                    }
                    SequenceChange::Ready | SequenceChange::Resumed => set_ready(&mut live, key, true),
                    SequenceChange::Blocked { .. } => set_ready(&mut live, key, false),
                    SequenceChange::Completed { .. } => {
                        if let Some(done) = live.remove(&key) {
                            let progress = out.minds.entry(done.mind).or_default();
                            progress.acts += 1;
                            progress.act_time += stamped.at.since_start().saturating_sub(done.arrived.since_start());
                        }
                    }
                    SequenceChange::Cancelled => {
                        live.remove(&key);
                    }
                    SequenceChange::PagedOut { .. } | SequenceChange::PagedIn { .. } => {}
                }
            }
            WorkloadEvent::Step { engine, step } => {
                let ran: BTreeSet<Key> = step.ran.iter().map(|work| (*engine, work.sequence)).collect();
                let ready: Vec<Key> = live.iter().filter(|(_, l)| l.ready).map(|(k, _)| *k).collect();
                for key in &ready {
                    let Some(seq) = live.get_mut(key) else { continue };
                    let progress = out.minds.entry(seq.mind).or_default();
                    if ran.contains(key) {
                        seq.waiting = Duration::ZERO;
                    } else {
                        seq.waiting += step.took;
                        progress.left_waiting += step.took;
                        progress.longest_wait = progress.longest_wait.max(seq.waiting);
                    }
                }
                out.steps.push(StepView { at: stamped.at, took: step.took, ready, ran: ran.into_iter().collect() });
            }
        }
    }
    out
}

fn set_ready(live: &mut BTreeMap<Key, Live>, key: Key, ready: bool) {
    if let Some(seq) = live.get_mut(&key) {
        seq.ready = ready;
        if !ready {
            seq.waiting = Duration::ZERO; // blocked is not waiting on the scheduler
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use continuum_base::capture::{CaptureSink, Ring};

    use super::super::{EngineStep, StepWork};
    use super::*;

    fn engine() -> EngineIncarnation {
        EngineIncarnation { pid: 7, started_s: 1_762_300_000, port: 8_085 }
    }

    fn mind(n: u128) -> MindId {
        MindId(uuid::Uuid::from_u128(n))
    }

    fn seq(sink: &Ring<WorkloadEvent>, id: u64, who: MindId, change: SequenceChange) {
        sink.record(WorkloadEvent::Sequence { engine: engine(), sequence: SequenceId(id), mind: who, change });
    }

    fn step(sink: &Ring<WorkloadEvent>, ms: u64, ran: &[u64]) {
        let ran = ran.iter().map(|&id| StepWork { sequence: SequenceId(id), depth: 0, prefilled: 0, decoded: 1, accepted_draft: 0 }).collect();
        sink.record(WorkloadEvent::Step { engine: engine(), step: EngineStep { took: Duration::from_millis(ms), ran } });
    }

    /// what this catches: a replay built from chosen steps alone sees no starvation; the mind the
    /// policy kept skipping must show her runnable-but-unserved time and its longest unbroken run.
    #[test]
    fn a_runnable_mind_the_policy_skips_shows_her_wait_and_starvation() {
        let sink = Ring::new(NonZeroUsize::new(64).unwrap());
        let (a, b) = (mind(1), mind(2));
        seq(&sink, 1, a, SequenceChange::Arrived { resident: 0, fresh: 10 });
        seq(&sink, 2, b, SequenceChange::Arrived { resident: 0, fresh: 10 });
        seq(&sink, 1, a, SequenceChange::Ready);
        seq(&sink, 2, b, SequenceChange::Ready);
        step(&sink, 30, &[1]);
        step(&sink, 30, &[1]);
        seq(&sink, 1, a, SequenceChange::Completed { decoded: 2 });
        step(&sink, 30, &[2]);
        seq(&sink, 2, b, SequenceChange::Completed { decoded: 1 });
        let replayed = replay(&sink.drain().complete().unwrap());
        assert_eq!(replayed.steps[0].ready.len(), 2);
        assert_eq!(replayed.steps[0].ran.len(), 1);
        let b_progress = replayed.minds[&b];
        assert_eq!(b_progress.left_waiting, Duration::from_millis(60));
        assert_eq!(b_progress.longest_wait, Duration::from_millis(60));
        assert_eq!(replayed.minds[&a].left_waiting, Duration::ZERO);
        assert_eq!((replayed.minds[&a].acts, b_progress.acts), (1, 1));
    }

    /// what this catches: a mind blocked on memory or a page-in is not the scheduler's to serve, so
    /// her blocked time must not count as waiting on it.
    #[test]
    fn blocked_time_is_not_scheduler_wait() {
        let sink = Ring::new(NonZeroUsize::new(64).unwrap());
        let a = mind(1);
        seq(&sink, 1, a, SequenceChange::Arrived { resident: 0, fresh: 10 });
        seq(&sink, 1, a, SequenceChange::Blocked { on: super::super::BlockedOn::PageIn });
        step(&sink, 50, &[]);
        seq(&sink, 1, a, SequenceChange::Resumed);
        step(&sink, 20, &[1]);
        let replayed = replay(&sink.drain().complete().unwrap());
        assert_eq!(replayed.minds.get(&a).map(|p| p.left_waiting), Some(Duration::ZERO));
    }
}
