//! The generic owner of a part's parts (object model §3.1, `Parts<K, P>`): one part per key, in
//! registration order, walked once for every verb.
//!
//! Each admitted part's verb runs as its own task on the owner's executor, never inside an
//! observer's future, and records its answer into the operation's [`Completion`] as it arrives. A
//! replaced part is kept, with its generation and its handle, until its own shutdown is done.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use futures::task::{Spawn, SpawnExt};
use parking_lot::{Mutex, RwLock};

use super::completion::{Answer, Completion, OperationId};
use super::{Generation, Lifecycle, Outcome, Verb};

/// The executor a part's owner runs its verbs on: the node's worker, behind the `futures` seam so
/// this crate names no runtime.
pub type Executor = Arc<dyn Spawn + Send + Sync>;

struct Entry<K, P: ?Sized> {
    key: K,
    generation: Generation,
    part: Arc<P>,
}

/// A replaced part, held until its shutdown is done.
struct Retiring<K, P: ?Sized + Lifecycle> {
    key: K,
    generation: Generation,
    part: Arc<P>,
    shutdown: Completion<K, P::Error>,
}

struct Inner<K, P: ?Sized + Lifecycle> {
    next: AtomicU64,
    live: RwLock<Vec<Entry<K, P>>>,
    retiring: Mutex<Vec<Retiring<K, P>>>,
    /// The newest operation for each verb, and every operation still running, so a retry or a new
    /// observer finds it by id.
    operations: Mutex<Vec<Completion<K, P::Error>>>,
    executor: Executor,
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
        let inner = Inner {
            next: AtomicU64::new(0),
            live: RwLock::new(Vec::new()),
            retiring: Mutex::new(Vec::new()),
            operations: Mutex::new(Vec::new()),
            executor,
        };
        Self { inner: Arc::new(inner) }
    }

    /// Add `part` under `key` with a new generation. A part already under `key` keeps its place in
    /// the order for its replacement, and starts its own shutdown under its own generation.
    pub fn insert(&self, key: K, part: Arc<P>) -> Generation {
        let generation = Generation::from_counter(self.inner.next.fetch_add(1, Ordering::Relaxed));
        let replaced = {
            let mut live = self.inner.live.write();
            let entry = Entry { key, generation, part };
            if let Some(present) = live.iter_mut().find(|present| present.key == entry.key) {
                Some(std::mem::replace(present, entry))
            } else {
                live.push(entry);
                None
            }
        };
        if let Some(old) = replaced {
            self.retire(old, None);
        }
        generation
    }

    #[must_use]
    pub fn get(&self, key: &K) -> Option<Arc<P>> {
        self.inner.live.read().iter().find(|entry| &entry.key == key).map(|entry| Arc::clone(&entry.part))
    }

    /// The live parts' keys and generations, in order.
    #[must_use]
    pub fn keys(&self) -> Vec<(K, Generation)> {
        self.inner.live.read().iter().map(|entry| (entry.key.clone(), entry.generation)).collect()
    }

    /// Run `verb` on every live part, as one operation. Asking again with an `operation` already
    /// known returns its completion and runs nothing a second time.
    #[must_use = "the completion is how the caller learns what each part answered"]
    pub fn act(&self, operation: OperationId, verb: Verb) -> Completion<K, P::Error> {
        if let Some(known) = self.completion(operation) {
            return known;
        }
        let admitted: Vec<(K, Generation, Arc<P>)> =
            self.inner.live.read().iter().map(|entry| (entry.key.clone(), entry.generation, Arc::clone(&entry.part))).collect();
        self.start(operation, verb, None, admitted, None)
    }

    /// The operation `operation`, if this owner still holds it: how a new observer reattaches.
    #[must_use]
    pub fn completion(&self, operation: OperationId) -> Option<Completion<K, P::Error>> {
        self.inner.operations.lock().iter().find(|known| known.operation() == operation).cloned()
    }

    /// The replaced parts whose shutdown is not yet done, with that shutdown's completion.
    #[must_use]
    pub fn retiring(&self) -> Vec<(K, Generation, Completion<K, P::Error>)> {
        self.inner.retiring.lock().iter().map(|held| (held.key.clone(), held.generation, held.shutdown.clone())).collect()
    }

    /// Retry the shutdown of the retiring part `key` at `generation`, when its last shutdown settled
    /// without finishing. The new attempt names the one it follows. A shutdown still running is
    /// returned as it is; nothing runs twice.
    pub fn retry_retiring(&self, key: &K, generation: Generation) -> Option<Completion<K, P::Error>> {
        // Found, judged and taken under one lock, so two retries cannot both start a shutdown.
        let held = {
            let mut retiring = self.inner.retiring.lock();
            let index = retiring.iter().position(|held| &held.key == key && held.generation == generation)?;
            if !retiring[index].shutdown.snapshot().is_settled() {
                return Some(retiring[index].shutdown.clone());
            }
            retiring.remove(index)
        };
        let previous = held.shutdown.operation();
        Some(self.retire(Entry { key: held.key, generation, part: held.part }, Some(previous)))
    }

    fn retire(&self, old: Entry<K, P>, follows: Option<OperationId>) -> Completion<K, P::Error> {
        let retiring = Retirement { owner: Arc::downgrade(&self.inner), key: old.key.clone(), generation: old.generation };
        let admitted = vec![(old.key.clone(), old.generation, Arc::clone(&old.part))];
        let shutdown = self.start(OperationId::new(), Verb::Shutdown, follows, admitted, Some(&retiring));
        self.inner.retiring.lock().push(Retiring { key: old.key, generation: old.generation, part: old.part, shutdown: shutdown.clone() });
        // A shutdown that finished before it was held has already tried to release it; release now.
        if shutdown.snapshot().is_done() {
            release(&self.inner, &shutdown);
        }
        shutdown
    }

    fn start(
        &self,
        operation: OperationId,
        verb: Verb,
        follows: Option<OperationId>,
        admitted: Vec<(K, Generation, Arc<P>)>,
        retiring: Option<&Retirement<K, P>>,
    ) -> Completion<K, P::Error> {
        let completion = Completion::new(operation, verb, follows, admitted.iter().map(|(key, generation, _)| (key.clone(), *generation)).collect());
        self.remember(&completion);
        for (slot, (_, _, part)) in admitted.into_iter().enumerate() {
            let watch = Watch::new(completion.clone(), slot);
            let retiring = retiring.cloned();
            let task = drive(part, verb, watch, retiring);
            // A refused task is dropped unpolled, so its slot already reads `NotStarted`.
            if self.inner.executor.spawn(task).is_err() {
                completion.record(slot, Answer::NotStarted);
            }
        }
        completion
    }

    /// Keep `completion` findable, dropping older settled operations of the same verb.
    fn remember(&self, completion: &Completion<K, P::Error>) {
        let mut operations = self.inner.operations.lock();
        operations.retain(|known| known.verb() != completion.verb() || !known.snapshot().is_settled());
        operations.push(completion.clone());
    }
}

/// Run one part's verb to its final answer, driving a pending answer's rest.
async fn drive<K, P>(part: Arc<P>, verb: Verb, mut watch: Watch<K, P::Error>, retiring: Option<Retirement<K, P>>)
where
    K: Clone + PartialEq + Send + Sync + 'static,
    P: ?Sized + Lifecycle + 'static,
{
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
    let done = matches!(answer, Answer::Done);
    watch.settle(answer);
    if let (true, Some(retiring)) = (done, retiring) {
        retiring.release_now(&watch.completion);
    }
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
    const fn new(completion: Completion<K, E>, slot: usize) -> Self {
        Self { completion, slot, stage: Stage::Queued }
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

/// Which retiring part a shutdown belongs to, so its success releases exactly that part.
struct Retirement<K, P: ?Sized + Lifecycle> {
    owner: Weak<Inner<K, P>>,
    key: K,
    generation: Generation,
}

impl<K: Clone, P: ?Sized + Lifecycle> Clone for Retirement<K, P> {
    fn clone(&self) -> Self {
        Self { owner: Weak::clone(&self.owner), key: self.key.clone(), generation: self.generation }
    }
}

impl<K: Clone + PartialEq, P: ?Sized + Lifecycle> Retirement<K, P> {
    fn release_now(&self, shutdown: &Completion<K, P::Error>) {
        if let Some(owner) = self.owner.upgrade() {
            release(&owner, shutdown);
        }
    }
}

/// Release the retiring part whose shutdown is `shutdown`, now that it is done.
fn release<K: Clone + PartialEq, P: ?Sized + Lifecycle>(owner: &Inner<K, P>, shutdown: &Completion<K, P::Error>) {
    owner.retiring.lock().retain(|held| held.shutdown.operation() != shutdown.operation());
}

#[cfg(test)]
#[path = "parts_tests.rs"]
mod tests;
