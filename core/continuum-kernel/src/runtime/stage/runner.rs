//! One stage, run on its own task: the machinery every stage gets once (own task, latest-wins
//! input, provenance on every output, the publication compare-and-set, panic quarantine), moved
//! here from the cognition deferred lane so recall, the reflex and the dream share it.
//!
//! Conforms to the concurrency style guide: own `tokio::spawn` task, `catch_unwind` around each
//! run, `watch` channels in both directions (no lock held across an await), event-triggered by
//! input changes (never a sleep loop), and a quarantine after repeated panics.

use std::sync::Arc;

use futures_util::FutureExt;
use tokio::sync::watch;

use super::control::ControlRequest;
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

/// What the background task and an inline run share: one stage, one run sequence, one
/// publication rule, one published output.
struct Shared<S: Stage> {
    stage: Arc<S>,
    incarnation: StageIncarnation,
    seq: std::sync::atomic::AtomicU64,
    publication: std::sync::Mutex<Publication>,
    latest_tx: watch::Sender<Option<PublishedOutput<S::Output>>>,
    control_rx: watch::Receiver<ControlRequest>,
}

impl<S: Stage> Shared<S> {
    fn next_context(&self) -> StageContext {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        StageContext {
            run: RunId {
                incarnation: self.incarnation,
                seq,
            },
            control: self.control_rx.clone(),
        }
    }

    /// Publish each output under the compare-and-set; returns the outputs it accepted.
    fn publish(&self, outputs: Vec<(S::Output, Provenance)>) -> Vec<S::Output> {
        let mut accepted = Vec::new();
        for (output, provenance) in outputs {
            // the lock is never held across an await: this is the whole critical section
            let fresh = self.publication.lock().unwrap_or_else(|p| p.into_inner()).try_publish(&provenance.from) // poisoned lock = read the last state, the same policy as every lock in this crate
                == Published::Accepted;
            if fresh {
                accepted.push(output.clone());
                let _ = self
                    .latest_tx
                    .send(Some(PublishedOutput { output, provenance })); // a send fails only with no reader, which needs no output
            }
        }
        accepted
    }
}

/// A running stage. Dropping it stops its background task.
pub struct StageRunner<S: Stage> {
    shared: Arc<Shared<S>>,
    input_tx: watch::Sender<Option<Submitted<S::Input>>>,
    latest: watch::Receiver<Option<PublishedOutput<S::Output>>>,
    control_tx: watch::Sender<ControlRequest>,
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
        let (latest_tx, latest) = watch::channel::<Option<PublishedOutput<S::Output>>>(None);
        let (control_tx, control_rx) = watch::channel(ControlRequest::Continue);
        let shared = Arc::new(Shared {
            stage,
            incarnation,
            seq: std::sync::atomic::AtomicU64::new(0),
            publication: std::sync::Mutex::new(Publication::default()),
            latest_tx,
            control_rx,
        });
        let worker = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            let mut panics = 0u32;
            while input_rx.changed().await.is_ok() {
                if panics >= QUARANTINE_AFTER {
                    continue; // quarantined: serve the last good, never run again
                }
                let Some(submitted) = input_rx.borrow_and_update().clone() else {
                    continue;
                };
                let ctx = worker.next_context();
                let run = std::panic::AssertUnwindSafe(worker.stage.run(
                    &ctx,
                    &submitted.input,
                    &submitted.revisions,
                ))
                .catch_unwind()
                .await;
                match run {
                    Err(_) => {
                        panics += 1;
                        continuum_base::probe!(
                            class = "stage.run.panicked",
                            stage = %worker.incarnation.stage.as_uuid(),
                            consecutive = panics as u64,
                            "a stage run panicked; at the quarantine count it stops and serves its last good"
                        );
                    }
                    Ok(outcome) => {
                        panics = 0;
                        if let RunOutcome::Produced(outputs) = outcome {
                            worker.publish(outputs);
                        }
                    }
                }
            }
        });
        Self {
            shared,
            input_tx,
            latest,
            control_tx,
            task,
        }
    }

    /// Hand the stage a new input for its background task (latest wins: an input it has not
    /// started yet is replaced).
    pub fn submit(&self, input: S::Input, revisions: Vec<InputRevision>) {
        let _ = self.input_tx.send(Some(Submitted { input, revisions })); // a send fails only once the task ended, which drop already stopped
    }

    /// Run the stage now, in the caller's task, under the same run sequence and publication rule
    /// as the background task: for a caller that must have this input's result before it goes on
    /// (an addressed turn's orienting response, a cold start). Returns the accepted outputs; an
    /// output derived from older input than what is already published is refused.
    pub async fn run_inline(
        &self,
        input: &S::Input,
        revisions: &[InputRevision],
    ) -> Vec<S::Output> {
        let ctx = self.shared.next_context();
        match self.shared.stage.run(&ctx, input, revisions).await {
            RunOutcome::Produced(outputs) => self.shared.publish(outputs),
            _ => Vec::new(),
        }
    }

    /// The latest published output, with provenance; `None` until the first lands.
    pub fn latest(&self) -> Option<PublishedOutput<S::Output>> {
        self.latest.borrow().clone()
    }

    /// A receiver that wakes when a new output is published.
    pub fn subscribe(&self) -> watch::Receiver<Option<PublishedOutput<S::Output>>> {
        self.latest.clone()
    }

    /// Ask the running stage to continue, pause, yield or cancel at its next yield point; the
    /// stage's receipt says what happened.
    pub fn request(&self, control: ControlRequest) {
        let _ = self.control_tx.send(control); // a send fails only once the task ended, which drop already stopped
    }
}
