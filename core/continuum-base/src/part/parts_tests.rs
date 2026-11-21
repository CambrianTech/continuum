use super::*;
use crate::part::{Identity, Part, Snapshot};
use async_trait::async_trait;
use futures::channel::oneshot;
use futures::executor::{block_on, ThreadPool};
use futures::future::FutureObj;
use futures::task::SpawnError;
use std::sync::atomic::AtomicUsize;

type Error = &'static str;

/// A kind of part the owner holds through `dyn`.
trait Kind: Lifecycle<Error = Error> {}

/// How a test part answers a verb.
enum Plan {
    Done,
    Fails,
    /// Waits for the gate, then is done.
    Gated(Mutex<Option<oneshot::Receiver<()>>>),
    /// Answers `Pending`; the rest finishes when the gate opens.
    PendingUntil(Mutex<Option<oneshot::Receiver<()>>>),
}

struct Probe {
    name: &'static str,
    plan: Plan,
    acts: AtomicUsize,
}

impl Probe {
    fn new(name: &'static str, plan: Plan) -> Arc<Self> {
        Arc::new(Self { name, plan, acts: AtomicUsize::new(0) })
    }
    fn done(name: &'static str) -> Arc<Self> {
        Self::new(name, Plan::Done)
    }
    fn gated(name: &'static str) -> (Arc<Self>, oneshot::Sender<()>) {
        let (open, gate) = oneshot::channel();
        (Self::new(name, Plan::Gated(Mutex::new(Some(gate)))), open)
    }
    fn pending(name: &'static str) -> (Arc<Self>, oneshot::Sender<()>) {
        let (open, gate) = oneshot::channel();
        (Self::new(name, Plan::PendingUntil(Mutex::new(Some(gate)))), open)
    }
    fn acts(&self) -> usize {
        self.acts.load(Ordering::SeqCst)
    }
}

impl Part for Probe {
    fn identity(&self) -> Identity {
        Identity { name: self.name, description: "a test part" }
    }
}

#[async_trait]
impl Lifecycle for Probe {
    type Error = Error;
    async fn act(&self, _verb: Verb) -> Outcome<Error> {
        self.acts.fetch_add(1, Ordering::SeqCst);
        match &self.plan {
            Plan::Done => Outcome::Done,
            Plan::Fails => Outcome::Failed("device lost"),
            Plan::Gated(gate) => {
                let gate = gate.lock().take();
                let Some(gate) = gate else { return Outcome::Failed("gate already used") };
                match gate.await {
                    Ok(()) => Outcome::Done,
                    Err(oneshot::Canceled) => Outcome::Failed("gate dropped"),
                }
            }
            Plan::PendingUntil(gate) => {
                let gate = gate.lock().take();
                gate.map_or(Outcome::Done, |gate| Outcome::pending(async move { gate.await.map_err(|oneshot::Canceled| "gate dropped") }))
            }
        }
    }
}

impl Kind for Probe {}

fn pool() -> Executor {
    match ThreadPool::builder().pool_size(2).create() {
        Ok(pool) => Arc::new(pool),
        Err(error) => panic!("a test thread pool: {error}"),
    }
}

fn owner() -> Parts<&'static str, dyn Kind> {
    Parts::new(pool())
}

fn answers(snapshot: &Snapshot<&'static str, Error>) -> Vec<(&'static str, Answer<Error>)> {
    snapshot.answers.iter().map(|(key, _, answer)| (*key, answer.clone())).collect()
}

/// Wait until `completion` shows `key` with `answer`.
fn wait_for(completion: &Completion<&'static str, Error>, key: &str, answer: &Answer<Error>) {
    block_on(async {
        let mut snapshot = completion.snapshot();
        while !snapshot.answers.iter().any(|(k, _, a)| *k == key && a == answer) {
            snapshot = completion.after(snapshot.revision).await;
        }
    });
}

// what this catches: a walk that runs parts one after another, drops a failure from the report, or
// reorders the parts.
#[test]
fn a_slow_and_a_failing_part_hold_back_no_one_and_both_are_reported() {
    let parts = owner();
    let (slow, open) = Probe::gated("slow");
    let fast = Probe::done("fast");
    parts.insert("slow", slow);
    parts.insert("broken", Probe::new("broken", Plan::Fails));
    parts.insert("fast", Arc::clone(&fast) as Arc<dyn Kind>);
    let walk = parts.act(OperationId::new(), Verb::Shutdown);
    wait_for(&walk, "fast", &Answer::Done);
    wait_for(&walk, "broken", &Answer::Failed("device lost"));
    assert_eq!(answers(&walk.snapshot())[0], ("slow", Answer::Running), "the slow part is still releasing");
    assert!(open.send(()).is_ok());
    let settled = block_on(walk.settled());
    assert_eq!(answers(&settled), vec![("slow", Answer::Done), ("broken", Answer::Failed("device lost")), ("fast", Answer::Done)]);
    assert!(!settled.is_done());
}

// what this catches: an observer's future driving the work, so dropping it loses a finished
// sibling's answer or cancels the unfinished one (Codex, review of 2ef42e0966).
#[test]
fn a_dropped_observer_cancels_nothing_and_a_new_one_reattaches_by_operation() {
    let parts = owner();
    let (slow, open) = Probe::gated("slow");
    parts.insert("slow", Arc::clone(&slow) as Arc<dyn Kind>);
    parts.insert("fast", Probe::done("fast"));
    let operation = OperationId::new();
    let observer = parts.act(operation, Verb::Suspend);
    wait_for(&observer, "fast", &Answer::Done);
    drop(observer);
    let Some(reattached) = parts.completion(operation) else { panic!("the owner still holds the operation") };
    assert_eq!(answers(&reattached.snapshot()), vec![("slow", Answer::Running), ("fast", Answer::Done)]);
    assert!(open.send(()).is_ok());
    assert!(block_on(reattached.settled()).is_done());
}

// what this catches: a retry of the same operation reissuing a verb a part already ran.
#[test]
fn the_same_operation_asked_twice_runs_each_verb_once() {
    let parts = owner();
    let (slow, open) = Probe::gated("slow");
    parts.insert("slow", Arc::clone(&slow) as Arc<dyn Kind>);
    let operation = OperationId::new();
    let first = parts.act(operation, Verb::Prepare);
    let again = parts.act(operation, Verb::Prepare);
    assert_eq!(first.operation(), again.operation());
    assert!(open.send(()).is_ok());
    assert!(block_on(again.settled()).is_done());
    assert_eq!(slow.acts(), 1, "the retry ran the verb a second time");
}

// what this catches: a replaced part released before its shutdown is done (its only strong handle
// handed out and dropped), a pending answer that can never become done, and a late shutdown
// reaching the replacement.
#[test]
fn a_replaced_part_is_held_until_its_pending_shutdown_is_done_and_its_replacement_is_untouched() {
    let parts = owner();
    let (first, finish) = Probe::pending("whisper");
    let first_alive = Arc::downgrade(&first);
    parts.insert("whisper", first);
    parts.insert("kokoro", Probe::done("kokoro"));
    let replacement = Probe::done("whisper");
    let newest = parts.insert("whisper", Arc::clone(&replacement) as Arc<dyn Kind>);
    assert_eq!(parts.keys().iter().map(|(key, _)| *key).collect::<Vec<_>>(), vec!["whisper", "kokoro"]);

    let Ok([(_, retired, shutdown)]) = <[_; 1]>::try_from(parts.retiring()) else { panic!("one part is retiring") };
    assert!(retired < newest);
    wait_for(&shutdown, "whisper", &Answer::Pending);
    assert!(first_alive.upgrade().is_some(), "a pending shutdown released its part");

    assert!(finish.send(()).is_ok());
    wait_for(&shutdown, "whisper", &Answer::Done);
    block_on(async {
        while !parts.retiring().is_empty() {
            futures::pending!();
        }
    });
    assert!(first_alive.upgrade().is_none(), "the old part is released once its shutdown is done");
    assert_eq!(replacement.acts(), 0, "the replacement was shut down by its predecessor");
}

// what this catches: a failed shutdown releasing its part, and a retry that is not linked to it.
#[test]
fn a_failed_shutdown_stays_retiring_and_its_retry_names_the_failure() {
    let parts = owner();
    parts.insert("piper", Probe::new("piper", Plan::Fails));
    parts.insert("piper", Probe::done("piper"));
    let Ok([(key, generation, failed)]) = <[_; 1]>::try_from(parts.retiring()) else { panic!("one part is retiring") };
    wait_for(&failed, "piper", &Answer::Failed("device lost"));
    assert_eq!(parts.retiring().len(), 1, "a failed shutdown let its part go");
    let Some(retry) = parts.retry_retiring(&key, generation) else { panic!("the retiring part is found") };
    assert_eq!(retry.follows(), Some(failed.operation()));
}

/// An executor that misbehaves in one of the three ways an executor can.
enum Faulty {
    /// Refuses the task.
    Refuses,
    /// Accepts the task and drops it unpolled (an executor shutting down).
    DropsUnpolled,
    /// Polls the task once and then drops it (an executor cancelling running work).
    DropsAfterOnePoll,
}

impl Spawn for Faulty {
    fn spawn_obj(&self, mut future: FutureObj<'static, ()>) -> Result<(), SpawnError> {
        match self {
            Self::Refuses => Err(SpawnError::shutdown()),
            Self::DropsUnpolled => Ok(()),
            Self::DropsAfterOnePoll => {
                let mut context = std::task::Context::from_waker(futures::task::noop_waker_ref());
                assert!(futures::FutureExt::poll_unpin(&mut future, &mut context).is_pending(), "the gated part waits");
                Ok(())
            }
        }
    }
}

// what this catches: an executor's refusal or cancellation leaving a part shown as running forever,
// or a verb that never ran reported as cancelled mid-way.
#[test]
fn a_task_the_executor_drops_reads_not_started_or_cancelled_never_running() {
    for (executor, expected) in [
        (Faulty::Refuses, Answer::NotStarted),
        (Faulty::DropsUnpolled, Answer::NotStarted),
        (Faulty::DropsAfterOnePoll, Answer::Cancelled),
    ] {
        let parts: Parts<&'static str, dyn Kind> = Parts::new(Arc::new(executor));
        let (silero, _gate) = Probe::gated("silero");
        parts.insert("silero", silero);
        let walk = parts.act(OperationId::new(), Verb::Prepare);
        assert_eq!(answers(&walk.snapshot()), vec![("silero", expected)]);
    }
}
