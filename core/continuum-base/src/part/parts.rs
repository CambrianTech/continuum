//! The generic owner of a part's parts (object model §3.1, `Parts<K, P>`): one part per key, in
//! registration order, walked once for every verb.
//!
//! Invariants, each held by construction:
//! - **One lock.** Generations, live parts, retiring parts and bound operations live under one
//!   mutex, so no check and its action are ever split across two acquisitions.
//! - **Bound before it runs.** An operation, and a retiring part's shutdown, are installed under
//!   the lock before any task exists; tasks are spawned after it is released, so an executor that
//!   polls inline cannot deadlock on it and a task can never finish before it is held.
//! - **Only the owner starts work.** [`Parts::act`] mints the operation's id and binds it in the same
//!   step. [`Parts::rejoin`] joins a bound operation and never runs anything, so asking again can
//!   never execute a verb twice. An id no longer held answers [`Rejoin::Expired`].
//! - **A finished shutdown is never redone.** A retiring part is held until its shutdown is `Done`,
//!   and is released in the same step that records it `Done`, under the lock.
//!   Only a shutdown that ended without finishing (`Failed`, `Cancelled`, `NotStarted`) can be
//!   retried, and its successor replaces it under the lock, so observers always see one attempt.

use std::sync::{Arc, Weak};

use futures::task::{Spawn, SpawnExt};
use parking_lot::Mutex;

use super::completion::{Answer, Completion, OperationId, Request};
use super::sequence::{Sequence, Turn};
use super::{Generation, Lifecycle, Outcome, Verb};

/// The executor a part's owner runs its verbs on: the node's worker, behind the `futures` seam so
/// this crate names no runtime.
pub type Executor = Arc<dyn Spawn + Send + Sync>;

/// What [`Parts::rejoin`] found.
pub enum Rejoin<K, E> {
    /// The operation is held: its completion, settled or not.
    Joined(Completion<K, E>),
    /// The operation was settled and a newer one of the same verb has been bound since.
    Expired,
}

/// What [`Parts::retry_retiring`] did.
pub enum Retry<K, E> {
    /// The last shutdown ended without finishing; this new attempt follows it.
    Started(Completion<K, E>),
    /// A shutdown is still running; it is returned, and nothing new runs.
    Running(Completion<K, E>),
    /// No part is retiring under that key and generation.
    NotRetiring,
}

struct Entry<K, P: ?Sized> {
    key: K,
    generation: Generation,
    part: Arc<P>,
    /// This generation's admission order: its verbs run one at a time, in the order admitted.
    sequence: Arc<Sequence>,
}

impl<K: Clone, P: ?Sized> Entry<K, P> {
    /// Admit this part to an operation, taking its next turn. Called under the owner's lock.
    fn admit(&self) -> Admitted<K, P> {
        Admitted { key: self.key.clone(), generation: self.generation, part: Arc::clone(&self.part), turn: self.sequence.ticket() }
    }
}

/// One part admitted to an operation, with its place in that part's order.
struct Admitted<K, P: ?Sized> {
    key: K,
    generation: Generation,
    part: Arc<P>,
    turn: Turn,
}

/// A replaced part, held until its shutdown is done.
struct Retiring<K, P: ?Sized + Lifecycle> {
    entry: Entry<K, P>,
    shutdown: Completion<K, P::Error>,
}

struct State<K, P: ?Sized + Lifecycle> {
    generations: u64,
    live: Vec<Entry<K, P>>,
    retiring: Vec<Retiring<K, P>>,
    /// Every operation still running, and the newest settled one of each verb.
    operations: Vec<Completion<K, P::Error>>,
}

struct Inner<K, P: ?Sized + Lifecycle> {
    state: Mutex<State<K, P>>,
    executor: Executor,
}

/// An operation bound under the lock, and its tasks, to spawn once the lock is released.
type Bound<K, P> = (Completion<K, <P as Lifecycle>::Error>, Vec<Task<K, P>>);

/// One part's verb, bound and ready to spawn once the lock is released.
struct Task<K: Clone + Send + Sync, P: ?Sized + Lifecycle> {
    part: Arc<P>,
    turn: Turn,
    watch: Watch<K, P::Error>,
    /// Set for a retiring part's shutdown, so its success releases it.
    owner: Option<Weak<Inner<K, P>>>,
}

/// One part per key, in registration order. Cheap to clone; clones share the same parts.
pub struct Parts<K, P: ?Sized + Lifecycle> {
    inner: Arc<Inner<K, P>>,
}

impl<K, P: ?Sized + Lifecycle> Clone for Parts<K, P> {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

impl<K, P> Parts<K, P>
where
    K: Clone + PartialEq + Send + Sync + 'static,
    P: ?Sized + Lifecycle + 'static,
{
    #[must_use]
    pub fn new(executor: Executor) -> Self {
        let state = State { generations: 0, live: Vec::new(), retiring: Vec::new(), operations: Vec::new() };
        Self { inner: Arc::new(Inner { state: Mutex::new(state), executor }) }
    }

    /// Add `part` under `key` with a new generation. A part already under `key` keeps its place in
    /// the order for its replacement, and starts its own shutdown under its own generation.
    pub fn insert(&self, key: K, part: Arc<P>) -> Generation {
        let (generation, tasks) = {
            let mut state = self.inner.state.lock();
            state.generations += 1;
            let generation = Generation::from_counter(state.generations);
            let entry = Entry { key, generation, part, sequence: Arc::default() };
            let tasks = if let Some(index) = state.live.iter().position(|present| present.key == entry.key) {
                let old = std::mem::replace(&mut state.live[index], entry);
                self.retire(&mut state, old, None).1
            } else {
                state.live.push(entry);
                Vec::new()
            };
            drop(state); // released before any task is spawned
            (generation, tasks)
        };
        self.spawn(tasks, Verb::Shutdown);
        generation
    }

    #[must_use]
    pub fn get(&self, key: &K) -> Option<Arc<P>> {
        self.inner.state.lock().live.iter().find(|entry| &entry.key == key).map(|entry| Arc::clone(&entry.part))
    }

    /// The live parts' keys and generations, in order.
    #[must_use]
    pub fn keys(&self) -> Vec<(K, Generation)> {
        self.inner.state.lock().live.iter().map(|entry| (entry.key.clone(), entry.generation)).collect()
    }

    /// Run `verb` on every live part as one new operation. Its id is
    /// [`Completion::operation`]; asking again is [`Parts::rejoin`], which never runs anything.
    #[must_use = "the completion is how the caller learns what each part answered"]
    pub fn act(&self, verb: Verb) -> Completion<K, P::Error> {
        let (completion, tasks) = {
            let mut state = self.inner.state.lock();
            let admitted = state.live.iter().map(Entry::admit).collect();
            let bound = Self::bind(&mut state, Request { operation: OperationId::mint(), verb, follows: None }, admitted, None);
            drop(state); // released before any task is spawned
            bound
        };
        self.spawn(tasks, verb);
        completion
    }

    /// The operation `operation`: how an observer that detached, or a caller retrying, reattaches.
    #[must_use]
    pub fn rejoin(&self, operation: OperationId) -> Rejoin<K, P::Error> {
        let state = self.inner.state.lock();
        state.operations.iter().find(|known| known.operation() == operation).map_or(Rejoin::Expired, |known| Rejoin::Joined(known.attach()))
    }

    /// The replaced parts whose shutdown is not yet released, with that shutdown's completion.
    #[must_use]
    pub fn retiring(&self) -> Vec<(K, Generation, Completion<K, P::Error>)> {
        let state = self.inner.state.lock();
        state.retiring.iter().map(|held| (held.entry.key.clone(), held.entry.generation, held.shutdown.attach())).collect()
    }

    /// Retry the shutdown of the retiring part `key` at `generation`. Only a shutdown that ended
    /// without finishing is retried; the new attempt names the one it follows and replaces it under
    /// the lock.
    pub fn retry_retiring(&self, key: &K, generation: Generation) -> Retry<K, P::Error> {
        let (completion, tasks) = {
            let mut state = self.inner.state.lock();
            let Some(index) = state.retiring.iter().position(|held| &held.entry.key == key && held.entry.generation == generation) else {
                return Retry::NotRetiring;
            };
            let last = state.retiring[index].shutdown.snapshot();
            if !last.is_settled() {
                return Retry::Running(state.retiring[index].shutdown.attach());
            }
            let held = state.retiring.remove(index);
            let follows = Some(held.shutdown.operation());
            let bound = self.retire(&mut state, held.entry, follows);
            drop(state); // released before any task is spawned
            bound
        };
        self.spawn(tasks, Verb::Shutdown);
        Retry::Started(completion)
    }

    /// Hold `old` as retiring under a newly bound shutdown, returning it; the caller spawns the task
    /// once the lock is released.
    fn retire(&self, state: &mut State<K, P>, old: Entry<K, P>, follows: Option<OperationId>) -> Bound<K, P> {
        let admitted = vec![old.admit()];
        let request = Request { operation: OperationId::mint(), verb: Verb::Shutdown, follows };
        let (shutdown, tasks) = Self::bind(state, request, admitted, Some(&Arc::downgrade(&self.inner)));
        state.retiring.push(Retiring { entry: old, shutdown: shutdown.attach() });
        (shutdown, tasks)
    }

    /// Bind one operation over `admitted` and keep it findable; nothing runs yet.
    fn bind(
        state: &mut State<K, P>,
        request: Request,
        admitted: Vec<Admitted<K, P>>,
        owner: Option<&Weak<Inner<K, P>>>,
    ) -> Bound<K, P> {
        let completion = Completion::new(request, admitted.iter().map(|admitted| (admitted.key.clone(), admitted.generation)));
        state.operations.retain(|known| known.verb() != request.verb || !known.snapshot().is_settled());
        state.operations.push(completion.attach());
        let tasks = admitted
            .into_iter()
            .enumerate()
            .map(|(slot, admitted)| Task { part: admitted.part, turn: admitted.turn, watch: Watch::new(&completion, slot), owner: owner.cloned() })
            .collect();
        (completion, tasks)
    }

    /// Spawn bound tasks. Called only with the lock released.
    fn spawn(&self, tasks: Vec<Task<K, P>>, verb: Verb) {
        for task in tasks {
            // A refused task is dropped unpolled, so its slot already reads `NotStarted`; the
            // refusal itself goes to the log, where the node's operator reads it.
            if let Err(refused) = self.inner.executor.spawn(drive(task, verb)) {
                tracing::warn!(error = %refused, ?verb, "the executor refused a part's verb");
            }
        }
    }
}

/// Run one part's verb to its final answer, driving a pending answer's rest. A retiring part whose
/// shutdown is done is released by its owner.
async fn drive<K, P>(task: Task<K, P>, verb: Verb)
where
    K: Clone + PartialEq + Send + Sync + 'static,
    P: ?Sized + Lifecycle + 'static,
{
    let Task { part, turn, mut watch, owner } = task;
    // Every verb admitted on this part before this one settles first: a late Resume can never run
    // after the Shutdown admitted behind it. A task dropped while waiting never ran: `NotStarted`.
    turn.arrive().await;
    watch.stage = Stage::Running;
    let answer = match part.act(verb).await {
        Outcome::Done => Answer::Done,
        Outcome::Failed(error) => Answer::Failed(error),
        Outcome::Pending(rest) => {
            watch.completion.record(watch.slot, Answer::Pending);
            match rest.await {
                Ok(()) => Answer::Done,
                Err(error) => Answer::Failed(error),
            }
        }
    };
    // A retiring part's shutdown is recorded Done and the part released in one step, under its
    // owner's lock: no observer ever sees a finished shutdown still held, so none can retry it.
    let owner = owner.filter(|_| matches!(answer, Answer::Done)).and_then(|owner| owner.upgrade());
    if let Some(owner) = owner {
        let mut state = owner.state.lock();
        let operation = watch.completion.operation();
        watch.settle(answer);
        state.retiring.retain(|held| held.shutdown.operation() != operation);
    } else {
        watch.settle(answer);
    }
    drop(turn); // the next verb admitted on this part may run
}

/// How far one part's task got, so a dropped task records the truth.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Never polled: dropped now, the verb never ran.
    Queued,
    /// Polled: dropped now, the verb was cancelled mid-way.
    Running,
    Settled,
}

/// One part's slot in its operation's record. If the executor drops the task before it answers,
/// the drop records `NotStarted` or `Cancelled`, so no part is shown running forever.
struct Watch<K: Clone + Send + Sync, E: Clone + Send + Sync> {
    completion: Completion<K, E>,
    slot: usize,
    stage: Stage,
}

impl<K: Clone + Send + Sync, E: Clone + Send + Sync> Watch<K, E> {
    fn new(completion: &Completion<K, E>, slot: usize) -> Self {
        Self { completion: completion.attach(), slot, stage: Stage::Queued }
    }

    fn settle(&mut self, answer: Answer<E>) {
        self.stage = Stage::Settled;
        self.completion.record(self.slot, answer);
    }
}

impl<K: Clone + Send + Sync, E: Clone + Send + Sync> Drop for Watch<K, E> {
    fn drop(&mut self) {
        match self.stage {
            Stage::Queued => self.completion.record(self.slot, Answer::NotStarted),
            Stage::Running => self.completion.record(self.slot, Answer::Cancelled),
            Stage::Settled => {}
        }
    }
}


#[cfg(test)]
#[path = "parts_tests.rs"]
mod tests;
