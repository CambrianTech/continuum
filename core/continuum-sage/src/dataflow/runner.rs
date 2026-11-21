//! One stage, run on its own task: the machinery every stage gets once (own task, latest-wins
//! input, provenance on every output, the publication compare-and-set, panic quarantine), moved
//! here from the cognition deferred lane.
//!
//! Scope, deliberately narrow (Codex at f5611b59): this runner serves SELF-CONTAINED stages,
//! whose run produces outputs and leaves no work behind it (recall). It is not the lifetime
//! owner of dispatched work: a run returning [`RunOutcome::Dispatched`] is a contract violation
//! here, said and counted, until the task owner's registration and completion interface exists
//! (Codex's seam). Dropping the runner aborts its task with no quiescence receipt, which is
//! honest only for stages with no committed effects. It accepts no control requests yet: a
//! request is observable control only once its receipt reaches the task owner.
//!
//! Conforms to the concurrency style guide: own `tokio::spawn` task, `catch_unwind` around every
//! run (background and inline alike), `watch` channels (no lock held across an await),
//! event-triggered by input changes (never a sleep loop), and a quarantine after repeated panics.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::FutureExt;
use tokio::sync::watch;

use super::control::{ControlRequest, StopReceipt};
use super::identity::{RunId, StageIncarnation};
use super::publication::{Publication, Published};
use super::stage::{RunOutcome, Stage, StageContext};
use super::topic::{InputRevision, Provenance};

/// After this many consecutive panics a stage stops running and keeps serving its last
/// published output: a bad backend degrades the stage to stale, it never takes the mind down
/// (moved unchanged from the cognition deferred lane; the monitor's three-strike rule).
const QUARANTINE_AFTER: u32 = 3;

/// An input handed to the stage, with the revisions that say exactly which versions it is.
#[derive(Clone)]
struct Submitted<I> {
    input: I,
    revisions: Vec<InputRevision>,
}

/// The latest output the stage published, with its provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedOutput<O> {
    pub output: O,
    pub provenance: Provenance,
}

/// What a stage has published: the compare-and-set state and the visible output, held in ONE
/// watch value so accepting a revision and replacing the output are one critical section (the
/// watch's own lock). Two publishers can never accept in one order and become visible in the
/// other.
#[derive(Debug, Clone)]
pub struct PublishedState<O> {
    publication: Publication,
    pub latest: Option<PublishedOutput<O>>,
}

/// What one run did, typed, for the caller (inline) or the last-outcome watch (background).
#[derive(Debug, Clone, PartialEq)]
pub enum Ran<O> {
    /// Outputs the publication rule accepted, and how many it refused as older than published.
    Produced {
        accepted: Vec<O>,
        refused: usize,
    },
    Idle,
    Stopped(StopReceipt),
    /// The run panicked; `consecutive` counts toward the quarantine.
    Panicked {
        consecutive: u32,
    },
    /// Quarantined: the stage did not run, and serves its last published output.
    Quarantined,
    /// The run handed work to the task layer, which this runner cannot own (see module docs).
    DispatchedUnowned,
}

/// What the background task and an inline run share: one stage, one run sequence, one panic
/// count, one published state.
struct Shared<S: Stage> {
    stage: Arc<S>,
    incarnation: StageIncarnation,
    seq: AtomicU64,
    panics: AtomicU32,
    published: watch::Sender<PublishedState<S::Output>>,
    last_run: watch::Sender<Option<(RunId, Ran<()>)>>,
    control: watch::Receiver<ControlRequest>,
}

impl<S: Stage> Shared<S> {
    /// Run once under the panic boundary and quarantine, publish what it produced, and record
    /// the outcome. The one path both the background task and an inline caller take.
    async fn run_once(&self, input: &S::Input, revisions: &[InputRevision]) -> Ran<S::Output> {
        if self.panics.load(Ordering::Acquire) >= QUARANTINE_AFTER {
            return Ran::Quarantined;
        }
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let run = RunId {
            incarnation: self.incarnation,
            seq,
        };
        let ctx = StageContext {
            run,
            control: self.control.clone(),
        };
        let caught = std::panic::AssertUnwindSafe(self.stage.run(&ctx, input, revisions))
            .catch_unwind()
            .await;
        let ran = match caught {
            Err(_) => {
                let consecutive = self.panics.fetch_add(1, Ordering::AcqRel) + 1;
                continuum_base::probe!(
                    class = "stage.run.panicked",
                    stage = %self.incarnation.stage.as_uuid(),
                    consecutive = consecutive as u64,
                    "a stage run panicked; at the quarantine count it stops and serves its last good"
                );
                Ran::Panicked { consecutive }
            }
            Ok(outcome) => {
                self.panics.store(0, Ordering::Release);
                match outcome {
                    RunOutcome::Produced(outputs) => self.publish(outputs),
                    RunOutcome::Idle => Ran::Idle,
                    RunOutcome::Stopped(receipt) => Ran::Stopped(receipt),
                    RunOutcome::Dispatched => {
                        continuum_base::probe!(
                            class = "stage.run.dispatched_unowned",
                            stage = %self.incarnation.stage.as_uuid(),
                            "a stage dispatched work through a runner that cannot own it (task owner interface pending)"
                        );
                        Ran::DispatchedUnowned
                    }
                }
            }
        };
        self.last_run
            .send_replace(Some((run, ran.without_outputs())));
        ran
    }

    /// Publish each output under the compare-and-set, atomically with making it visible.
    fn publish(&self, outputs: Vec<(S::Output, Provenance)>) -> Ran<S::Output> {
        let (mut accepted, mut refused) = (Vec::new(), 0);
        for (output, provenance) in outputs {
            self.published.send_if_modified(|state| {
                let fresh = state.publication.try_publish(&provenance.from) == Published::Accepted;
                if fresh {
                    accepted.push(output.clone());
                    state.latest = Some(PublishedOutput { output, provenance });
                } else {
                    refused += 1;
                }
                fresh
            });
        }
        Ran::Produced { accepted, refused }
    }
}

impl<O> Ran<O> {
    fn without_outputs(&self) -> Ran<()> {
        match self {
            Ran::Produced { accepted, refused } => Ran::Produced {
                accepted: vec![(); accepted.len()],
                refused: *refused,
            },
            Ran::Idle => Ran::Idle,
            Ran::Stopped(receipt) => Ran::Stopped(receipt.clone()),
            Ran::Panicked { consecutive } => Ran::Panicked {
                consecutive: *consecutive,
            },
            Ran::Quarantined => Ran::Quarantined,
            Ran::DispatchedUnowned => Ran::DispatchedUnowned,
        }
    }
}

/// A running self-contained stage. Dropping it aborts its background task (see module docs).
pub struct StageRunner<S: Stage> {
    shared: Arc<Shared<S>>,
    input_tx: watch::Sender<Option<Submitted<S::Input>>>,
    task: tokio::task::JoinHandle<()>,
}

impl<S: Stage> Drop for StageRunner<S> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl<S: Stage> StageRunner<S> {
    /// Start `stage` as `incarnation` on its own task.
    pub fn spawn(stage: Arc<S>, incarnation: StageIncarnation) -> Self {
        let (input_tx, mut input_rx) = watch::channel::<Option<Submitted<S::Input>>>(None);
        let (published, _) = watch::channel(PublishedState {
            publication: Publication::default(),
            latest: None,
        });
        let (last_run, _) = watch::channel(None);
        // No control requests are accepted yet (module docs); every run sees Continue.
        let (_control_tx, control) = watch::channel(ControlRequest::Continue);
        let shared = Arc::new(Shared {
            stage,
            incarnation,
            seq: AtomicU64::new(0),
            panics: AtomicU32::new(0),
            published,
            last_run,
            control,
        });
        let worker = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            while input_rx.changed().await.is_ok() {
                let Some(submitted) = input_rx.borrow_and_update().clone() else {
                    continue;
                };
                worker
                    .run_once(&submitted.input, &submitted.revisions)
                    .await;
            }
        });
        Self {
            shared,
            input_tx,
            task,
        }
    }

    /// Hand the stage a new input for its background task (latest wins: an input it has not
    /// started yet is replaced).
    pub fn submit(&self, input: S::Input, revisions: Vec<InputRevision>) {
        let _ = self.input_tx.send(Some(Submitted { input, revisions })); // a send fails only once the task ended, which drop already stopped
    }

    /// Run the stage now, in the caller's task, on the same path as the background task (run
    /// sequence, panic boundary, quarantine, publication rule): for a caller that must have this
    /// input's result before it goes on (an addressed turn's orienting response, a cold start).
    pub async fn run_inline(
        &self,
        input: &S::Input,
        revisions: &[InputRevision],
    ) -> Ran<S::Output> {
        self.shared.run_once(input, revisions).await
    }

    /// The latest published output, with provenance; `None` until the first lands.
    pub fn latest(&self) -> Option<PublishedOutput<S::Output>> {
        self.shared.published.borrow().latest.clone()
    }

    /// A receiver that wakes when a new output is published.
    pub fn subscribe(&self) -> watch::Receiver<PublishedState<S::Output>> {
        self.shared.published.subscribe()
    }

    /// The last run's typed outcome (outputs elided), from either path.
    pub fn last_run(&self) -> Option<(RunId, Ran<()>)> {
        self.shared.last_run.borrow().clone()
    }
}
