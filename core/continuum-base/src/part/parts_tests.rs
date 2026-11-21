use super::*;
use crate::part::{Identity, Part, Snapshot};
use async_trait::async_trait;
use futures::channel::oneshot;
use futures::executor::{block_on, ThreadPool};
use futures::future::FutureObj;
use futures::task::SpawnError;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

type Error = &'static str;

/// A kind of part the owner holds through `dyn`.
trait Kind: Lifecycle<Error = Error> {}

/// How a test part answers one verb.
enum Step {
    Fails,
    /// Waits for the gate, then is done.
    Gated(oneshot::Receiver<()>),
    /// Answers `Pending`; the rest finishes when the gate opens.
    PendingUntil(oneshot::Receiver<()>),
}

/// A part that answers each verb with its next scripted step (done once the script runs out), and
/// counts how many times a verb actually ran.
struct Probe {
    name: &'static str,
    script: Mutex<VecDeque<Step>>,
    acts: AtomicUsize,
}

impl Probe {
    fn scripted(name: &'static str, steps: impl IntoIterator<Item = Step>) -> Arc<Self> {
        Arc::new(Self { name, script: Mutex::new(steps.into_iter().collect()), acts: AtomicUsize::new(0) })
    }
    fn done(name: &'static str) -> Arc<Self> {
        Self::scripted(name, [])
    }
    fn acts(&self) -> usize {
        self.acts.load(Ordering::SeqCst)
    }
}

fn gate() -> (oneshot::Sender<()>, oneshot::Receiver<()>) {
    oneshot::channel()
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
        let step = self.script.lock().pop_front();
        match step {
            None => Outcome::Done,
            Some(Step::Fails) => Outcome::Failed("device lost"),
            Some(Step::Gated(gate)) => match gate.await {
                Ok(()) => Outcome::Done,
                Err(oneshot::Canceled) => Outcome::Failed("gate dropped"),
            },
            Some(Step::PendingUntil(gate)) => Outcome::pending(async move { gate.await.map_err(|oneshot::Canceled| "gate dropped") }),
        }
    }
}

impl Kind for Probe {}

fn pool() -> Executor {
    match ThreadPool::builder().pool_size(4).create() {
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


fn joined(rejoin: Rejoin<&'static str, Error>) -> Completion<&'static str, Error> {
    match rejoin {
        Rejoin::Joined(completion) => completion,
        Rejoin::Expired => panic!("the operation is still held"),
    }
}

fn only_retiring(parts: &Parts<&'static str, dyn Kind>) -> (&'static str, Generation, Completion<&'static str, Error>) {
    let Ok([held]) = <[_; 1]>::try_from(parts.retiring()) else { panic!("exactly one part is retiring") };
    held
}

/// Run `call` on `callers` threads at once: every thread is spawned before any is joined, so the
/// calls race. A thread that panicked has already failed the test; its panic is re-raised.
fn concurrently<T: Send>(callers: usize, call: impl Fn() -> T + Sync) -> Vec<T> {
    std::thread::scope(|scope| {
        let mut threads = Vec::with_capacity(callers);
        for _ in 0..callers {
            threads.push(scope.spawn(&call));
        }
        threads
            .into_iter()
            .map(|thread| match thread.join() {
                Ok(value) => value,
                Err(panic) => std::panic::resume_unwind(panic),
            })
            .collect()
    })
}

// what this catches: a walk that runs parts one after another, drops a failure from the report, or
// reorders the parts.
#[test]
fn a_slow_and_a_failing_part_hold_back_no_one_and_both_are_reported() {
    let parts = owner();
    let (open, slow_gate) = gate();
    parts.insert("slow", Probe::scripted("slow", [Step::Gated(slow_gate)]));
    parts.insert("broken", Probe::scripted("broken", [Step::Fails]));
    parts.insert("fast", Probe::done("fast"));
    let walk = parts.act(Verb::Shutdown);
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
    let (open, slow_gate) = gate();
    parts.insert("slow", Probe::scripted("slow", [Step::Gated(slow_gate)]));
    parts.insert("fast", Probe::done("fast"));
    let observer = parts.act(Verb::Suspend);
    let operation = observer.operation();
    wait_for(&observer, "fast", &Answer::Done);
    drop(observer);
    let reattached = joined(parts.rejoin(operation));
    assert_eq!(answers(&reattached.snapshot()), vec![("slow", Answer::Running), ("fast", Answer::Done)]);
    assert!(open.send(()).is_ok());
    assert!(block_on(reattached.settled()).is_done());
}

// what this catches: two callers retrying one operation at the same moment running its verb twice
// (Codex, review of e59cccfe28: check-then-start across a released lock).
#[test]
fn concurrent_rejoins_of_one_operation_run_each_verb_once() {
    let parts = owner();
    let (open, slow_gate) = gate();
    let slow = Probe::scripted("slow", [Step::Gated(slow_gate)]);
    parts.insert("slow", Arc::clone(&slow) as Arc<dyn Kind>);
    let operation = parts.act(Verb::Prepare).operation();
    let rejoined = concurrently(8, || joined(parts.rejoin(operation)).operation());
    assert!(rejoined.iter().all(|seen| *seen == operation));
    assert!(open.send(()).is_ok());
    assert!(block_on(joined(parts.rejoin(operation)).settled()).is_done());
    assert_eq!(slow.acts(), 1, "a rejoin ran the verb again");
}

// what this catches: a settled operation pruned by a newer operation of another verb, so a late
// replay of it runs again (Codex's A/B/A), and a superseded one replaying instead of expiring.
#[test]
fn a_b_a_rejoins_the_first_and_a_superseded_operation_expires_without_running() {
    let parts = owner();
    let part = Probe::done("kokoro");
    parts.insert("kokoro", Arc::clone(&part) as Arc<dyn Kind>);
    let a = parts.act(Verb::Prepare);
    assert!(block_on(a.settled()).is_done());
    let b = parts.act(Verb::Suspend);
    assert!(block_on(b.settled()).is_done());
    assert!(block_on(joined(parts.rejoin(a.operation())).settled()).is_done(), "A is still held after B");
    let newer = parts.act(Verb::Prepare);
    assert!(block_on(newer.settled()).is_done());
    assert!(matches!(parts.rejoin(a.operation()), Rejoin::Expired), "A is superseded by a newer Prepare");
    assert_eq!(part.acts(), 3, "a rejoin ran a verb");
}

// what this catches: a replaced part released before its shutdown is done (its only strong handle
// handed out and dropped), a pending answer that can never become done, and a late shutdown
// reaching the replacement.
#[test]
fn a_replaced_part_is_held_until_its_pending_shutdown_is_done_and_its_replacement_is_untouched() {
    let parts = owner();
    let (finish, rest) = gate();
    let first = Probe::scripted("whisper", [Step::PendingUntil(rest)]);
    let first_alive = Arc::downgrade(&first);
    parts.insert("whisper", first);
    parts.insert("kokoro", Probe::done("kokoro"));
    let replacement = Probe::done("whisper");
    let newest = parts.insert("whisper", Arc::clone(&replacement) as Arc<dyn Kind>);
    assert_eq!(parts.keys().iter().map(|(key, _)| *key).collect::<Vec<_>>(), vec!["whisper", "kokoro"]);

    let (_, retired, shutdown) = only_retiring(&parts);
    assert!(retired < newest);
    wait_for(&shutdown, "whisper", &Answer::Pending);
    assert!(first_alive.upgrade().is_some(), "a pending shutdown released its part");

    assert!(finish.send(()).is_ok());
    wait_for(&shutdown, "whisper", &Answer::Done);
    assert!(parts.retiring().is_empty(), "a done shutdown is still held: Done and release are one step");
    assert!(first_alive.upgrade().is_none(), "the old part is released once its shutdown is done");
    assert_eq!(replacement.acts(), 0, "the replacement was shut down by its predecessor");
}

// what this catches: a failed shutdown letting its part go, two concurrent retries both starting a
// shutdown, an observer seeing no attempt mid-retry, and a retry not linked to the failure (Codex,
// review of e59cccfe28).
#[test]
fn concurrent_retries_of_a_failed_shutdown_start_exactly_one_linked_attempt() {
    let parts = owner();
    let (open, second_attempt) = gate();
    let piper = Probe::scripted("piper", [Step::Fails, Step::Gated(second_attempt)]);
    parts.insert("piper", Arc::clone(&piper) as Arc<dyn Kind>);
    parts.insert("piper", Probe::done("piper"));
    let (key, generation, failed) = only_retiring(&parts);
    wait_for(&failed, "piper", &Answer::Failed("device lost"));

    let outcomes = concurrently(8, || match parts.retry_retiring(&key, generation) {
        Retry::Started(attempt) => (true, attempt.operation(), attempt.follows()),
        Retry::Running(attempt) => (false, attempt.operation(), attempt.follows()),
        Retry::NotRetiring => panic!("the failed part is still retiring"),
    });
    assert_eq!(outcomes.iter().filter(|(started, _, _)| *started).count(), 1, "exactly one retry starts an attempt");
    let attempt = outcomes[0].1;
    assert!(outcomes.iter().all(|(_, operation, follows)| *operation == attempt && *follows == Some(failed.operation())));
    let (_, _, successor) = only_retiring(&parts);
    assert_eq!(successor.operation(), attempt, "an observer saw no attempt, or a different one, mid-retry");

    assert!(open.send(()).is_ok());
    wait_for(&successor, "piper", &Answer::Done);
    assert!(parts.retiring().is_empty(), "a done shutdown is still held: Done and release are one step");
    assert_eq!(piper.acts(), 2, "one failed shutdown and one retry, nothing more");
    assert!(matches!(parts.retry_retiring(&key, generation), Retry::NotRetiring), "a released part was retried again");
}

// what this catches: generations minted outside the lock, so two concurrent replacements of one
// key leave an older generation live and a newer one retiring.
#[test]
fn concurrent_replacements_of_one_key_leave_the_newest_generation_live() {
    let parts = owner();
    let issued = concurrently(16, || parts.insert("silero", Probe::done("silero")));
    let Ok([(_, live)]) = <[_; 1]>::try_from(parts.keys()) else { panic!("one live part") };
    assert_eq!(Some(&live), issued.iter().max(), "the newest generation is the live one");
    assert!(parts.retiring().iter().all(|(_, retiring, _)| *retiring < live));
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
        let (_open, silero_gate) = gate();
        parts.insert("silero", Probe::scripted("silero", [Step::Gated(silero_gate)]));
        let walk = parts.act(Verb::Prepare);
        assert_eq!(answers(&walk.snapshot()), vec![("silero", expected)]);
    }
}
