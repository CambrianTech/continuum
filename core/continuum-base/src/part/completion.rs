//! A command's answers, recorded as each arrives (object model §3.1, `Completion`). The record is
//! bound at ingress to its operation, its verb and exactly the parts it admitted. Observers read a
//! snapshot with its revision and wait for a later one, so a detached observer loses nothing and a
//! new one reattaches by holding a clone.

use std::sync::Arc;

use event_listener::Event;
use parking_lot::Mutex;
use uuid::Uuid;

use super::{Generation, Verb};

/// One request. Asking again with the same id rejoins it; a new id is a new request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(pub Uuid);

impl OperationId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

/// What an owner is asked to do: the operation, its verb, and the failed operation it retries, if any.
#[derive(Debug, Clone, Copy)]
pub(super) struct Request {
    pub(super) operation: OperationId,
    pub(super) verb: Verb,
    pub(super) follows: Option<OperationId>,
}

/// How many answers a record has taken: every change of any slot advances it by one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Revision(pub u64);

/// Where one admitted part's answer stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer<E> {
    /// Its verb is running.
    Running,
    /// It answered `Pending`; its owner is driving the rest.
    Pending,
    Done,
    Failed(E),
    /// The executor refused the task, so the verb never ran.
    NotStarted,
    /// The executor dropped the task before it answered.
    Cancelled,
}

impl<E> Answer<E> {
    /// Whether this part's answer is final.
    #[must_use]
    pub const fn is_settled(&self) -> bool {
        !matches!(self, Self::Running | Self::Pending)
    }
}

/// The record as one observer sees it at one revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot<K, E> {
    pub revision: Revision,
    pub answers: Vec<(K, Generation, Answer<E>)>,
}

impl<K, E> Snapshot<K, E> {
    /// Whether every admitted part has a final answer.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.answers.iter().all(|(_, _, answer)| answer.is_settled())
    }

    /// Whether every admitted part is done.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.answers.iter().all(|(_, _, answer)| matches!(answer, Answer::Done))
    }
}

struct Record<K, E> {
    operation: OperationId,
    verb: Verb,
    /// The failed operation this one retries, when it is a retry.
    follows: Option<OperationId>,
    admitted: Vec<(K, Generation)>,
    state: Mutex<(Revision, Vec<Answer<E>>)>,
    changed: Event,
}

/// A handle on one operation's record. Cloning it is attaching another observer.
pub struct Completion<K, E> {
    record: Arc<Record<K, E>>,
}

impl<K, E> Clone for Completion<K, E> {
    fn clone(&self) -> Self {
        self.attach()
    }
}

impl<K, E> Completion<K, E> {
    /// Another observer of the same record.
    #[must_use]
    pub fn attach(&self) -> Self {
        Self { record: Arc::clone(&self.record) }
    }

    #[must_use]
    pub fn operation(&self) -> OperationId {
        self.record.operation
    }

    #[must_use]
    pub fn verb(&self) -> Verb {
        self.record.verb
    }

    #[must_use]
    pub fn follows(&self) -> Option<OperationId> {
        self.record.follows
    }
}

impl<K: Clone + Send + Sync, E: Clone + Send + Sync> Completion<K, E> {
    pub(super) fn new(request: Request, admitted: Vec<(K, Generation)>) -> Self {
        let answers = vec![Answer::Running; admitted.len()];
        let Request { operation, verb, follows } = request;
        let record = Record { operation, verb, follows, admitted, state: Mutex::new((Revision(0), answers)), changed: Event::new() };
        Self { record: Arc::new(record) }
    }

    /// The answers that have arrived, now. Never waits.
    #[must_use]
    pub fn snapshot(&self) -> Snapshot<K, E> {
        let state = self.record.state.lock();
        let answers = self.record.admitted.iter().zip(state.1.iter()).map(|((key, generation), answer)| (key.clone(), *generation, answer.clone())).collect();
        Snapshot { revision: state.0, answers }
    }

    /// A snapshot later than `seen`: at once if one exists, otherwise at the next answer. The
    /// listener is registered before the revision is read, so the last wake is never missed.
    pub async fn after(&self, seen: Revision) -> Snapshot<K, E> {
        loop {
            let listener = self.record.changed.listen();
            let snapshot = self.snapshot();
            if snapshot.revision > seen {
                return snapshot;
            }
            listener.await;
        }
    }

    /// The final snapshot, once every admitted part has settled.
    pub async fn settled(&self) -> Snapshot<K, E> {
        let mut snapshot = self.snapshot();
        while !snapshot.is_settled() {
            snapshot = self.after(snapshot.revision).await;
        }
        snapshot
    }

    /// Record `answer` for the part in `slot`. A settled answer is final: later writes to it are
    /// ignored, so a late result can never overwrite what an observer already acted on.
    pub(super) fn record(&self, slot: usize, answer: Answer<E>) {
        {
            let mut state = self.record.state.lock();
            let Some(current) = state.1.get_mut(slot) else { return };
            if current.is_settled() {
                return;
            }
            *current = answer;
            state.0 = Revision(state.0 .0 + 1);
        }
        self.record.changed.notify(usize::MAX);
    }
}
