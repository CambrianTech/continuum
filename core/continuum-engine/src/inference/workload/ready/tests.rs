//! The replay fold on a fixed clock.
use std::time::Duration;

use continuum_base::capture::{At, CaptureId, CompleteInterval, Incomplete, Interval, Missing, Stamped};

use super::super::{BlockedOn, StepWork};
use super::*;

const A: EngineIncarnation = EngineIncarnation { pid: 7, started_s: 1_762_300_000, port: 8_085 };
const B: EngineIncarnation = EngineIncarnation { pid: 8, started_s: 1_762_300_001, port: 8_086 };

fn ms(n: u64) -> At {
    At::from_origin(Duration::from_millis(n))
}

fn mind(n: u128) -> OwnerId {
    OwnerId(uuid::Uuid::from_u128(n))
}

fn seq(at: u64, (engine, id): (EngineIncarnation, u64), who: OwnerId, change: SequenceChange) -> Stamped<WorkloadEvent> {
    Stamped { at: ms(at), event: WorkloadEvent::Sequence { engine, sequence: SequenceId(id), mind: who, change } }
}

fn began(at: u64, (engine, step): (EngineIncarnation, u64), chosen: &[u64]) -> Stamped<WorkloadEvent> {
    let chosen = chosen.iter().map(|&id| SequenceId(id)).collect();
    Stamped { at: ms(at), event: WorkloadEvent::StepBegan { engine, step: StepId(step), chosen } }
}

fn ended(at: u64, (engine, step): (EngineIncarnation, u64), ran: &[u64]) -> Stamped<WorkloadEvent> {
    let work = ran.iter().map(|&id| StepWork { sequence: SequenceId(id), depth: 0, prefilled: 0, decoded: 1, accepted_draft: None }).collect();
    Stamped { at: ms(at), event: WorkloadEvent::StepEnded { engine, step: StepId(step), work } }
}

/// The `index`-th interval of `capture`, covering `span` milliseconds, through the same completeness
/// check a drained interval takes.
fn recorded(capture: CaptureId, index: u64, span: std::ops::Range<u64>, events: Vec<Stamped<WorkloadEvent>>) -> CompleteInterval<WorkloadEvent> {
    let interval = Interval { capture, index, from: ms(span.start), to: ms(span.end), events, missing: Missing::default() };
    interval.complete().expect("test: a fixture holding everything in order") // test: every fixture here is whole
}

const ARRIVED: SequenceChange = SequenceChange::Arrived { resident: 0, fresh: 10 };

// what this catches (Codex on #6052): an act arriving in one drained interval and completing in the
// next silently vanished; and a step spanning a drain boundary must stay one step.
#[test]
fn an_act_and_a_step_that_span_two_intervals_are_one_each() {
    let (capture, her) = (CaptureId::mint(), mind(1));
    let mut replay = Replay::new();
    replay.feed(&recorded(capture, 0, 0..100, vec![seq(10, (A, 1), her, ARRIVED), seq(20, (A, 1), her, SequenceChange::Ready), began(90, (A, 1), &[1])])).expect("test: the capture's first interval"); // test: fixture
    replay.feed(&recorded(capture, 1, 100..200, vec![ended(120, (A, 1), &[1]), seq(120, (A, 1), her, SequenceChange::Completed { decoded: 1 })])).expect("test: continues the first"); // test: fixture
    let report = replay.report();
    assert_eq!((report.minds[&her].acts, report.minds[&her].act_time), (1, Duration::from_millis(110)));
    assert_eq!(report.minds[&her].left_waiting, Duration::from_millis(70), "runnable from 20 until her step began at 90");
    assert_eq!((report.steps.len(), report.steps[0].began, report.steps[0].ended), (1, ms(90), ms(120)));
}

// what this catches (Codex on #6052): the ready set taken when a step was reported, not when it
// began, let a mind that arrived during the step appear available to it, so a counterfactual policy
// could schedule work before it existed.
#[test]
fn a_mind_arriving_during_a_step_is_not_in_that_steps_ready_set() {
    let (capture, first, late) = (CaptureId::mint(), mind(1), mind(2));
    let mut replay = Replay::new();
    replay
        .feed(&recorded(capture, 0, 0..100, vec![
            seq(0, (A, 1), first, ARRIVED),
            seq(0, (A, 1), first, SequenceChange::Ready),
            began(0, (A, 1), &[1]),
            seq(5, (A, 2), late, ARRIVED),
            seq(5, (A, 2), late, SequenceChange::Ready),
            ended(10, (A, 1), &[1]),
        ]))
        .expect("test: the capture's first interval"); // test: fixture
    let step = &replay.report().steps[0];
    assert_eq!(step.ready, vec![(A, SequenceId(1))], "only what was runnable when the step began");
}

// what this catches (Codex on #6052): a replay started mid-capture, or fed another capture, a gap or
// a discontinuity, cannot know the full ready set; each refuses.
#[test]
fn a_replay_refuses_a_missing_start_another_capture_or_a_gap() {
    let (capture, other) = (CaptureId::mint(), CaptureId::mint());
    assert_eq!(Replay::new().feed(&recorded(capture, 1, 100..200, vec![])), Err(ReplayError::MissingStart { index: 1, from: ms(100) }));
    let mut replay = Replay::new();
    replay.feed(&recorded(capture, 0, 0..100, vec![])).expect("test: the capture's first interval"); // test: fixture
    assert_eq!(replay.feed(&recorded(other, 1, 100..200, vec![])), Err(ReplayError::OtherCapture { expected: capture, got: other }));
    assert_eq!(replay.feed(&recorded(capture, 2, 100..200, vec![])), Err(ReplayError::Gap { expected: 1, got: 2 }));
    assert_eq!(replay.feed(&recorded(capture, 1, 150..200, vec![])), Err(ReplayError::Discontinuous { expected: ms(100), got: ms(150) }));
    let unknown = Replay::new().feed(&recorded(capture, 0, 0..100, vec![seq(10, (A, 9), mind(1), SequenceChange::Ready)]));
    assert_eq!(unknown, Err(ReplayError::UnknownSequence { sequence: (A, SequenceId(9)) }));
}

// what this catches (Codex on #6052): a recorded fixture bypassing the completeness check; a fixture
// that dropped events, or holds them out of order, never becomes a replayable interval.
#[test]
fn a_recorded_fixture_takes_the_same_completeness_check() {
    let capture = CaptureId::mint();
    let lossy = Interval::<WorkloadEvent> { capture, index: 0, from: ms(0), to: ms(100), events: vec![], missing: Missing { dropped: 1, misordered: 0 } };
    assert!(matches!(lossy.complete(), Err(Incomplete::Missing { .. })));
    let unordered = Interval { capture, index: 0, from: ms(0), to: ms(100), events: vec![seq(50, (A, 1), mind(1), ARRIVED), seq(10, (A, 2), mind(2), ARRIVED)], missing: Missing::default() };
    assert!(matches!(unordered.complete(), Err(Incomplete::Unordered { .. })));
}

// what this catches (Codex on #6052): engine A's step listed engine B's runnable sequence, inflating
// starvation whenever an independent engine was added.
#[test]
fn a_step_on_one_engine_never_lists_another_engines_mind() {
    let (capture, on_a, on_b) = (CaptureId::mint(), mind(1), mind(2));
    let mut replay = Replay::new();
    replay
        .feed(&recorded(capture, 0, 0..100, vec![
            seq(0, (A, 1), on_a, ARRIVED),
            seq(0, (A, 1), on_a, SequenceChange::Ready),
            seq(0, (B, 1), on_b, ARRIVED),
            seq(0, (B, 1), on_b, SequenceChange::Ready),
            began(0, (A, 1), &[1]),
            began(0, (B, 1), &[1]),
            ended(10, (A, 1), &[1]),
            seq(10, (A, 1), on_a, SequenceChange::Completed { decoded: 1 }),
            ended(10, (B, 1), &[1]),
            seq(10, (B, 1), on_b, SequenceChange::Completed { decoded: 1 }),
        ]))
        .expect("test: the capture's first interval"); // test: fixture
    let report = replay.report();
    assert!(report.steps.iter().all(|s| s.ready.iter().all(|key| key.0 == s.engine)), "a step's ready set is its own engine's");
    assert_eq!((report.minds[&on_a].left_waiting, report.minds[&on_b].left_waiting), (Duration::ZERO, Duration::ZERO));
}

// what this catches (Codex on #6052): a runnable mind on an engine that ran no step for a minute
// reported zero waiting, hiding exactly the stalled engine the instrument exists to expose.
#[test]
fn a_runnable_mind_on_a_stalled_engine_accrues_wait_with_no_steps() {
    let (capture, her) = (CaptureId::mint(), mind(1));
    let mut replay = Replay::new();
    replay.feed(&recorded(capture, 0, 0..60_000, vec![seq(0, (A, 1), her, ARRIVED), seq(0, (A, 1), her, SequenceChange::Ready)])).expect("test: the capture's first interval"); // test: fixture
    let report = replay.report();
    assert_eq!((report.minds[&her].left_waiting, report.minds[&her].longest_wait), (Duration::from_secs(60), Duration::from_secs(60)));
}

// what this catches: a mind blocked on memory or a page-in is not the scheduler's to serve, so her
// blocked time must not count as waiting on it.
#[test]
fn blocked_time_is_not_scheduler_wait() {
    let (capture, her) = (CaptureId::mint(), mind(1));
    let mut replay = Replay::new();
    replay
        .feed(&recorded(capture, 0, 0..100, vec![
            seq(0, (A, 1), her, ARRIVED),
            seq(0, (A, 1), her, SequenceChange::Blocked { on: BlockedOn::PageIn }),
            seq(50, (A, 1), her, SequenceChange::Resumed),
            began(50, (A, 1), &[1]),
            ended(70, (A, 1), &[1]),
            seq(70, (A, 1), her, SequenceChange::Completed { decoded: 1 }),
        ]))
        .expect("test: the capture's first interval"); // test: fixture
    assert_eq!(replay.report().minds[&her].left_waiting, Duration::ZERO);
}
