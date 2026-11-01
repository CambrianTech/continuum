use std::sync::Arc;
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
    type Input = ();
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
                measured: Some(Duration::from_millis(1)),
            },
            residency: ResidencyCost::default(),
        }
    }
    async fn run(
        &self,
        ctx: &StageContext,
        _input: &(),
        input: &[InputRevision],
    ) -> RunOutcome<u64> {
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
    match Counting.run(&ctx, &(), &input).await {
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
        Counting.run(&ctx, &(), &input).await,
        RunOutcome::Stopped(StopReceipt::Yielded { .. })
    ));
    tx.send(ControlRequest::Pause)
        .expect("test: the receiver is alive"); // test: a fixture step that must succeed
    assert!(matches!(
        Counting.run(&ctx, &(), &input).await,
        RunOutcome::Stopped(StopReceipt::Refused {
            refusal: StopRefusal::PauseUnsupported,
            ..
        })
    ));
}

/// A stage that echoes its input, or panics when asked to.
struct Echo;

#[async_trait]
impl Stage for Echo {
    type Input = i64;
    type Output = i64;
    fn id(&self) -> StageId {
        StageId::named("echo")
    }
    fn triggers(&self) -> Triggers {
        Triggers {
            inputs: vec![TopicName("numbers")],
            ..Triggers::default()
        }
    }
    fn cost(&self, _revisions: &[InputRevision]) -> Cost {
        Cost {
            execution: ExecutionCost::Cpu {
                measured: Some(Duration::from_millis(1)),
            },
            residency: ResidencyCost::default(),
        }
    }
    async fn run(
        &self,
        ctx: &StageContext,
        input: &i64,
        revisions: &[InputRevision],
    ) -> RunOutcome<i64> {
        assert!(*input >= 0, "test: a negative input panics the run"); // test: the deliberate panic under test
        RunOutcome::Produced(vec![(
            *input,
            Provenance {
                by: ctx.run,
                from: revisions.to_vec(),
            },
        )])
    }
}

async fn published(runner: &StageRunner<Echo>, want: i64) -> PublishedOutput<i64> {
    let mut sub = runner.subscribe();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(out) = sub.borrow_and_update().latest.clone() {
                if out.output == want {
                    return out;
                }
            }
            sub.changed().await.expect("test: the runner is alive"); // test: a fixture step that must succeed
        }
    })
    .await
    .expect("test: the output is published in time") // test: the assertion failing loudly
}

// what this catches: a stage's output landing without provenance, or a late output derived from
// older input replacing a newer one (the deferred lane's stale-finding guard, now the one rule).
#[tokio::test]
async fn the_runner_publishes_with_provenance_and_refuses_older_input() {
    let runner = StageRunner::spawn(Arc::new(Echo), incarnation(1));
    runner.submit(5, vec![rev("numbers", 5)]);
    let out = published(&runner, 5).await;
    assert_eq!(out.provenance.from, vec![rev("numbers", 5)]);
    assert_eq!(out.provenance.by.incarnation, incarnation(1));
    runner.submit(3, vec![rev("numbers", 3)]); // older input, arriving late
    runner.submit(9, vec![rev("numbers", 9)]);
    assert_eq!(
        published(&runner, 9).await.provenance.from,
        vec![rev("numbers", 9)]
    );
    assert_eq!(
        runner.latest().map(|o| o.output),
        Some(9),
        "the older input never replaced the newer"
    );
}

// what this catches: a stage whose backend keeps failing taking the mind down, or losing its last
// good output; both ways: a panic is survived, and three in a row quarantine it on its last good.
#[tokio::test]
async fn a_panicking_stage_is_quarantined_on_its_last_good_output() {
    let runner = StageRunner::spawn(Arc::new(Echo), incarnation(1));
    runner.submit(1, vec![rev("numbers", 1)]);
    published(&runner, 1).await;
    for (n, through) in [(-1, 2), (-2, 3), (-3, 4)] {
        runner.submit(n, vec![rev("numbers", through)]);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    runner.submit(7, vec![rev("numbers", 5)]);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        runner.latest().map(|o| o.output),
        Some(1),
        "quarantined: serves its last good, runs no more"
    );
}

// what this catches: a caller's inline run (an addressed turn's orienting response, a cold
// start) publishing around the one rule: it lands like a background run, and an inline run on
// input older than what is published is refused (both ways).
#[tokio::test]
async fn an_inline_run_publishes_under_the_same_rule() {
    let runner = StageRunner::spawn(Arc::new(Echo), incarnation(1));
    assert_eq!(
        runner.run_inline(&4, &[rev("numbers", 4)]).await,
        Ran::Produced {
            accepted: vec![4],
            refused: 0
        }
    );
    assert_eq!(runner.latest().map(|o| o.output), Some(4));
    assert_eq!(
        runner.run_inline(&2, &[rev("numbers", 2)]).await,
        Ran::Produced {
            accepted: vec![],
            refused: 1
        },
        "older input refused inline too, and the refusal is reported"
    );
    assert_eq!(runner.latest().map(|o| o.output), Some(4));
    runner.submit(6, vec![rev("numbers", 6)]);
    assert_eq!(
        published(&runner, 6).await.provenance.by.seq,
        3,
        "inline and background runs share one sequence"
    );
}

// what this catches: the publication compare-and-set and the visible output separating, so two
// publishers accept in one order and become visible in the other (Codex at f5611b59). Many
// concurrent inline publishers in a fixed shuffled order: the visible output never moves
// backwards and ends on the newest input.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_publishers_never_expose_an_older_output() {
    let runner = Arc::new(StageRunner::spawn(Arc::new(Echo), incarnation(1)));
    let mut watcher = runner.subscribe();
    let seen = tokio::spawn(async move {
        let mut high = 0i64;
        while watcher.changed().await.is_ok() {
            let now = watcher
                .borrow_and_update()
                .latest
                .as_ref()
                .map_or(0, |o| o.output);
            assert!(
                now >= high,
                "visible output moved backwards: {high} -> {now}"
            );
            high = now;
            if high == 200 {
                break;
            }
        }
        high
    });
    let mut order: Vec<i64> = (1..=200).collect();
    order.reverse();
    order.rotate_left(73);
    let runs: Vec<_> = order
        .into_iter()
        .map(|n| {
            let runner = Arc::clone(&runner);
            tokio::spawn(async move { runner.run_inline(&n, &[rev("numbers", n as u64)]).await })
        })
        .collect();
    for run in runs {
        run.await.expect("test: a publisher task completes"); // test: a fixture step that must succeed
    }
    assert_eq!(runner.latest().map(|o| o.output), Some(200));
    let high = tokio::time::timeout(Duration::from_secs(2), seen)
        .await
        .expect("test: the watcher sees the newest in time"); // test: the assertion failing loudly
    assert_eq!(high.expect("test: the watcher completes"), 200); // test: the assertion failing loudly
}

// what this catches: an inline run escaping the panic boundary the background path has (Codex
// at f5611b59): an inline panic is caught, counts toward the one quarantine, and a quarantined
// stage refuses inline runs too.
#[tokio::test]
async fn an_inline_panic_is_caught_and_counts_toward_the_quarantine() {
    let runner = StageRunner::spawn(Arc::new(Echo), incarnation(1));
    for (n, consecutive) in [(-1, 1), (-2, 2), (-3, 3)] {
        assert_eq!(
            runner.run_inline(&n, &[rev("numbers", 1)]).await,
            Ran::Panicked { consecutive }
        );
    }
    assert_eq!(
        runner.run_inline(&5, &[rev("numbers", 5)]).await,
        Ran::Quarantined
    );
    assert_eq!(runner.latest(), None, "nothing was ever published");
}
