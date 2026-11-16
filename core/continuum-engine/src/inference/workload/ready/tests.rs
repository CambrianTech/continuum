//! The replay fold on a fixed clock.
use std::time::Duration;

use continuum_base::capture::{At, CompleteInterval, Recorded, Stamped};

use super::super::{BlockedOn, EngineStep, StepWork};
use super::*;

const A: EngineIncarnation = EngineIncarnation { pid: 7, started_s: 1_762_300_000, port: 8_085 };
const B: EngineIncarnation = EngineIncarnation { pid: 8, started_s: 1_762_300_001, port: 8_086 };

fn ms(n: u64) -> At {
    At::from_origin(Duration::from_millis(n))
}

fn mind(n: u128) -> MindId {
    MindId(uuid::Uuid::from_u128(n))
}

fn seq(at: u64, engine: EngineIncarnation, id: u64, who: MindId, change: SequenceChange) -> Stamped<WorkloadEvent> {
    Stamped { at: ms(at), event: WorkloadEvent::Sequence { engine, sequence: SequenceId(id), mind: who, change } }
}

fn step(at: u64, engine: EngineIncarnation, took: u64, ran: &[u64]) -> Stamped<WorkloadEvent> {
    let ran = ran.iter().map(|&id| StepWork { sequence: SequenceId(id), depth: 0, prefilled: 0, decoded: 1, accepted_draft: 0 }).collect();
    Stamped { at: ms(at), event: WorkloadEvent::Step { engine, step: EngineStep { took: Duration::from_millis(took), ran } } }
}

/// The `index`-th recorded interval, covering `span` milliseconds on the ring clock.
fn interval(index: u64, span: std::ops::Range<u64>, events: Vec<Stamped<WorkloadEvent>>) -> CompleteInterval<WorkloadEvent> {
    Recorded { index, from: ms(span.start), to: ms(span.end), events }.into()
}

const ARRIVED: SequenceChange = SequenceChange::Arrived { resident: 0, fresh: 10 };

// what this catches (Codex on #6052): an act arriving in one drained interval and completing in the
// next silently vanished, because each interval replayed from an empty state.
#[test]
fn an_act_that_spans_two_intervals_is_one_act() {
    let her = mind(1);
    let mut replay = Replay::new();
    replay.feed(&interval(0, 0..100, vec![seq(10, A, 1, her, ARRIVED), seq(20, A, 1, her, SequenceChange::Ready)])).unwrap(); // test: this interval continues the last
    replay.feed(&interval(1, 100..200, vec![step(150, A, 30, &[1]), seq(150, A, 1, her, SequenceChange::Completed { decoded: 1 })])).unwrap(); // test: this interval continues the last
    let report = replay.report();
    assert_eq!((report.minds[&her].acts, report.minds[&her].act_time), (1, Duration::from_millis(140)));
    assert_eq!(report.minds[&her].left_waiting, Duration::from_millis(100), "runnable from 20 until her step began at 120");
}

// what this catches (Codex on #6052): a missing or repeated interval, a discontinuity, or a change to
// a sequence that never arrived must refuse, never replay a workload with a hole in it.
#[test]
fn a_missing_interval_or_an_unknown_sequence_refuses() {
    let her = mind(1);
    let mut replay = Replay::new();
    replay.feed(&interval(0, 0..100, vec![])).unwrap(); // test: this interval continues the last
    assert_eq!(replay.feed(&interval(2, 100..200, vec![])), Err(ReplayError::Gap { expected: 1, got: 2 }));
    assert_eq!(replay.feed(&interval(1, 150..200, vec![])), Err(ReplayError::Discontinuous { expected: ms(100), got: ms(150) }));
    let unknown = Replay::new().feed(&interval(0, 0..100, vec![seq(10, A, 9, her, SequenceChange::Ready)]));
    assert_eq!(unknown, Err(ReplayError::UnknownSequence { sequence: (A, SequenceId(9)) }));
}

// what this catches (Codex on #6052): engine A's step listed engine B's runnable sequence as passed
// over, inflating starvation whenever an independent engine was added. Each step's ready set is its
// own engine's; B's mind waits only the time she was actually runnable and not run.
#[test]
fn a_step_on_one_engine_never_passes_over_another_engines_mind() {
    let (on_a, on_b) = (mind(1), mind(2));
    let mut replay = Replay::new();
    replay
        .feed(&interval(0, 0..100, vec![
            seq(0, A, 1, on_a, ARRIVED),
            seq(0, A, 1, on_a, SequenceChange::Ready),
            seq(0, B, 1, on_b, ARRIVED),
            seq(0, B, 1, on_b, SequenceChange::Ready),
            step(10, A, 10, &[1]),
            step(10, B, 10, &[1]),
            step(20, A, 10, &[1]),
            seq(20, A, 1, on_a, SequenceChange::Completed { decoded: 2 }),
            step(20, B, 10, &[1]),
            seq(20, B, 1, on_b, SequenceChange::Completed { decoded: 2 }),
        ]))
        .unwrap(); // test: this interval continues the last
    let report = replay.report();
    assert!(report.steps.iter().all(|s| s.ready.iter().all(|key| key.0 == s.engine)), "a step's ready set is its own engine's");
    assert_eq!((report.minds[&on_a].left_waiting, report.minds[&on_b].left_waiting), (Duration::ZERO, Duration::ZERO));
}

// what this catches (Codex on #6052): a runnable mind on an engine that ran no step for a minute
// reported zero waiting, hiding exactly the stalled engine the instrument exists to expose.
#[test]
fn a_runnable_mind_on_a_stalled_engine_accrues_wait_with_no_steps() {
    let her = mind(1);
    let mut replay = Replay::new();
    replay.feed(&interval(0, 0..60_000, vec![seq(0, A, 1, her, ARRIVED), seq(0, A, 1, her, SequenceChange::Ready)])).unwrap(); // test: this interval continues the last
    let report = replay.report();
    assert_eq!(report.minds[&her].left_waiting, Duration::from_secs(60));
    assert_eq!(report.minds[&her].longest_wait, Duration::from_secs(60));
}

// what this catches: a mind blocked on memory or a page-in is not the scheduler's to serve, so her
// blocked time must not count as waiting on it.
#[test]
fn blocked_time_is_not_scheduler_wait() {
    let her = mind(1);
    let mut replay = Replay::new();
    replay
        .feed(&interval(0, 0..100, vec![
            seq(0, A, 1, her, ARRIVED),
            seq(0, A, 1, her, SequenceChange::Blocked { on: BlockedOn::PageIn }),
            seq(50, A, 1, her, SequenceChange::Resumed),
            step(70, A, 20, &[1]),
            seq(70, A, 1, her, SequenceChange::Completed { decoded: 1 }),
        ]))
        .unwrap(); // test: this interval continues the last
    assert_eq!(replay.report().minds[&her].left_waiting, Duration::ZERO);
}
