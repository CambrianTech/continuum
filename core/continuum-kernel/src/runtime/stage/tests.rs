use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use super::*;

fn incarnation(epoch: u64) -> StageIncarnation {
    StageIncarnation {
        stage: StageId::named("recall"),
        mind: MindId(Uuid::from_u128(7)),
        epoch,
    }
}

fn rev(topic: &'static str, through: u64) -> InputRevision {
    InputRevision {
        topic: TopicName(topic),
        through,
    }
}

// what this catches: a stage id that differs across restarts or nodes (an id parsed from or
// randomised at startup), which would break adoption of an old incarnation's obligations.
#[test]
fn a_stage_id_is_the_same_wherever_its_name_is() {
    assert_eq!(StageId::named("recall"), StageId::named("recall"));
    assert_ne!(StageId::named("recall"), StageId::named("reflex"));
}

// what this catches: a stale incarnation mutating after a newer one took over, and a fence
// that wrongly crosses stages or minds.
#[test]
fn only_a_later_incarnation_of_the_same_stage_and_mind_supersedes() {
    assert!(incarnation(1).is_superseded_by(&incarnation(2)));
    assert!(!incarnation(2).is_superseded_by(&incarnation(1)));
    assert!(!incarnation(1).is_superseded_by(&incarnation(1)));
    let other_mind = StageIncarnation {
        mind: MindId(Uuid::from_u128(8)),
        ..incarnation(9)
    };
    assert!(!incarnation(1).is_superseded_by(&other_mind));
}

// what this catches: triggers that choose one condition instead of composing (a stage with an
// input AND a completion AND a floor must wake on any of them), and a floor that fires early.
#[test]
fn triggers_compose_and_the_floor_is_only_a_floor() {
    let t = Triggers {
        inputs: vec![TopicName("perception")],
        completions: true,
        floor: Some(Duration::from_secs(60)),
    };
    assert!(t.fires(&[TopicName("perception")], false, Duration::ZERO));
    assert!(t.fires(&[], true, Duration::ZERO));
    assert!(t.fires(&[], false, Duration::from_secs(60)));
    assert!(!t.fires(&[TopicName("board")], false, Duration::from_secs(59)));
}

// what this catches: a late or overlapping run overwriting state derived from newer input
// (both ways: newer input publishes, older is refused, an unrelated topic is independent).
#[test]
fn an_output_from_older_input_never_overwrites_newer_state() {
    let mut published = Publication::default();
    assert_eq!(
        published.try_publish(&[rev("perception", 10)]),
        Published::Accepted
    );
    assert_eq!(
        published.try_publish(&[rev("perception", 9)]),
        Published::Stale
    );
    assert_eq!(
        published.try_publish(&[rev("perception", 12), rev("board", 3)]),
        Published::Accepted
    );
    assert_eq!(
        published.try_publish(&[rev("perception", 12), rev("board", 2)]),
        Published::Stale
    );
    assert_eq!(
        published.try_publish(&[rev("board", 3)]),
        Published::Accepted,
        "equal input republishes"
    );
}

/// A stage that does a unit of work per yield point and honours control at each.
struct Counting;

#[async_trait]
impl Stage for Counting {
    type Output = u64;
    fn id(&self) -> StageId {
        StageId::named("counting")
    }
    fn triggers(&self) -> Triggers {
        Triggers {
            inputs: vec![TopicName("numbers")],
            ..Triggers::default()
        }
    }
    fn cost(&self, _input: &[InputRevision]) -> Cost {
        Cost {
            execution: ExecutionCost::Cpu {
                estimated: Duration::from_millis(1),
            },
            residency: ResidencyCost::default(),
        }
    }
    async fn run(&self, ctx: &StageContext, input: &[InputRevision]) -> RunOutcome<u64> {
        for _ in 0..3 {
            match ctx.requested() {
                ControlRequest::Continue => {}
                ControlRequest::Yield => {
                    return RunOutcome::Stopped(StopReceipt::Yielded {
                        run: ctx.run,
                        residency_kept: ResidencyCost::default(),
                    })
                }
                ControlRequest::Cancel => {
                    return RunOutcome::Stopped(StopReceipt::Cancelled {
                        run: ctx.run,
                        committed_effects: 0,
                    })
                }
                ControlRequest::Pause => {
                    return RunOutcome::Stopped(StopReceipt::Refused {
                        run: ctx.run,
                        refusal: StopRefusal::PauseUnsupported,
                    })
                }
            }
        }
        RunOutcome::Produced(vec![(
            input.len() as u64,
            Provenance {
                by: ctx.run,
                from: input.to_vec(),
            },
        )])
    }
}

// what this catches: a stage that ignores control at its yield points, and a pause the stage
// cannot honour being reported as paused instead of refused (both ways: it runs to completion
// when left alone, and stops with the right receipt when asked).
#[tokio::test]
async fn a_stage_honours_control_with_the_receipt_that_proves_it() {
    let run = RunId {
        incarnation: incarnation(1),
        seq: 1,
    };
    let input = [rev("numbers", 4)];
    let (tx, rx) = tokio::sync::watch::channel(ControlRequest::Continue);
    let ctx = StageContext { run, control: rx };
    match Counting.run(&ctx, &input).await {
        RunOutcome::Produced(out) => assert_eq!(
            out[0].1,
            Provenance {
                by: run,
                from: input.to_vec()
            }
        ),
        other => panic!("left alone it completes: {other:?}"), // test: the assertion failing loudly
    }
    tx.send(ControlRequest::Yield)
        .expect("test: the receiver is alive"); // test: a fixture step that must succeed
    assert!(matches!(
        Counting.run(&ctx, &input).await,
        RunOutcome::Stopped(StopReceipt::Yielded { .. })
    ));
    tx.send(ControlRequest::Pause)
        .expect("test: the receiver is alive"); // test: a fixture step that must succeed
    assert!(matches!(
        Counting.run(&ctx, &input).await,
        RunOutcome::Stopped(StopReceipt::Refused {
            refusal: StopRefusal::PauseUnsupported,
            ..
        })
    ));
}
