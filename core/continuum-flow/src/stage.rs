//! A stage: CBAR's `QueueThread<T>` (`cb-mobile-sdk/cpp/utility/Threads.h`) in Rust. The inputs in
//! hand and up to its declared depth waiting (reliable by default; a signal stage keeps the newest), one worker task that
//! wakes only on a push, and a class. A stage is its handler and nothing else: the slot, waking,
//! backpressure and shutdown live here, never in the subsystem.

use crate::Qos;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Notify;

/// What a full stage does with a new input. Signals and reliable inputs are different types
/// (Codex on FLOW plan 6d18b4564): a dropped tool result is lost work; a dropped wake signal is
/// not, because it only references content already committed elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Never drop. A producer awaits the free slot ([`StageInput::send`]), or `push` hands the
    /// item back when the slot is full. The default.
    Reliable,
    /// Drop the oldest and keep the newest (CBAR's `QueueThread`). Only for signals whose input
    /// references content committed elsewhere: a wake, a frame, a "fresh activity" digest handle.
    /// A displaced input is dropped on the worker, never by the producer; while the worker has a
    /// full depth of displaced inputs still to drop, `push` hands the new one back as
    /// [`PushError::Full`] (backpressure), so a KeepNewest caller handles `Full` too.
    KeepNewest,
}

pub trait Stage: Send + 'static {
    /// What wakes it: a signal, a digest handle, a frame, a result.
    type Input: Send + 'static;
    /// Its class, for any capacity it submits to.
    const QOS: Qos;
    /// What a full stage does: never drops unless a signal stage opts in.
    const OVERFLOW: Overflow = Overflow::Reliable;
    /// How many inputs may wait while it handles one (CBAR's `queueSize`). One is the double buffer
    /// every signal stage wants; a stream of ordered items (a token stream) declares more, so its
    /// producer hands items over without being turned back between handles.
    const DEPTH: usize = 1;
    fn handle(&mut self, input: Self::Input) -> impl Future<Output = ()> + Send;
    /// Handle everything that was waiting, in order, in one wake: on a wake it does many. The
    /// default handles them one by one; a stage that batches (one write for many tokens)
    /// overrides it.
    fn handle_all(&mut self, inputs: Vec<Self::Input>) -> impl Future<Output = ()> + Send
    where
        Self: Sized,
    {
        async move {
            for input in inputs {
                self.handle(input).await;
            }
        }
    }
}

/// A stage holds the inputs it is handling and up to `DEPTH` waiting, in order. A producer that
/// finds it full gets its input back (`Reliable`) or displaces the oldest (`KeepNewest`), so no
/// subsystem sizes or locks a queue of its own.
struct Inbox<T> {
    producers: AtomicUsize,
    pending: Mutex<Batch<T>>,
    depth: usize,
    /// Wakes the worker: an input arrived, or the last producer left.
    signal: Notify,
    /// Wakes a waiting producer: the worker took the pending input, or the stage stopped.
    space: Notify,
    /// The worker has ended (every producer gone and the slot drained, or its handler panicked):
    /// nothing more will be taken, so an input is handed back instead of waiting forever.
    stopped: AtomicBool,
    overflow: Overflow,
}

/// Both queues are bounded by the stage depth. Retired payloads are destroyed on the worker.
struct Batch<T> {
    ready: std::collections::VecDeque<T>,
    retired: Vec<T>,
}

impl<T> Default for Batch<T> {
    fn default() -> Self {
        Self { ready: Default::default(), retired: Vec::new() }
    }
}

/// Why `push` handed an input back.
#[derive(Debug, PartialEq, Eq)]
pub enum PushError<T> {
    /// A `Reliable` stage's pending slot is taken; [`StageInput::send`] waits for it.
    Full(T),
    /// The stage's worker has ended; nothing will take it.
    Stopped(T),
}

/// The stage's worker has ended; `send` hands the input back.
#[derive(Debug, PartialEq, Eq)]
pub struct Stopped<T>(pub T);

/// The handle producers push into. Cheap to clone. Dropping every clone ends the stage once
/// its queued inputs are handled.
pub struct StageInput<T> {
    inbox: Arc<Inbox<T>>,
}

impl<T> Clone for StageInput<T> {
    fn clone(&self) -> Self {
        self.inbox.producers.fetch_add(1, Ordering::Relaxed);
        Self { inbox: Arc::clone(&self.inbox) }
    }
}

impl<T> StageInput<T> {
    /// Put `item` in the pending slot and signal, without waiting. A `Reliable` stage whose slot is
    /// taken hands the item back (`Full`) and drops nothing; a `KeepNewest` stage replaces the
    /// pending input and returns how many it dropped (0 or 1); a stopped stage hands it back
    /// (`Stopped`). The lock is held for the insert alone, never across work (CBAR's `addItem`).
    pub fn push(&self, item: T) -> Result<usize, PushError<T>> {
        let dropped = {
            let mut pending = self.inbox.pending.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned slot still holds plain data: keep serving it
            if self.inbox.stopped.load(Ordering::Acquire) {
                return Err(PushError::Stopped(item));
            }
            let full = pending.ready.len() >= self.inbox.depth;
            let dropped = match (full, self.inbox.overflow) {
                (true, Overflow::Reliable) => return Err(PushError::Full(item)),
                (true, Overflow::KeepNewest) => {
                    if std::mem::needs_drop::<T>() && pending.retired.len() == self.inbox.depth {
                        return Err(PushError::Full(item));
                    }
                    if let Some(oldest) = pending.ready.pop_front() {
                        if std::mem::needs_drop::<T>() {
                            pending.retired.push(oldest);
                        }
                    }
                    1
                }
                (false, _) => 0,
            };
            pending.ready.push_back(item);
            dropped
        };
        self.inbox.signal.notify_one();
        Ok(dropped)
    }

    /// Put `item` in the pending slot, waiting while a `Reliable` stage's slot is taken: the
    /// backpressure a producer awaits instead of spinning or dropping. Hands the item back only
    /// when the stage has stopped.
    pub async fn send(&self, mut item: T) -> Result<(), Stopped<T>> {
        loop {
            // Registered before the slot is looked at, so a take between the look and the wait
            // is never missed.
            let space = self.inbox.space.notified();
            tokio::pin!(space);
            space.as_mut().enable();
            match self.push(item) {
                Ok(_) => return Ok(()),
                Err(PushError::Stopped(back)) => return Err(Stopped(back)),
                Err(PushError::Full(back)) => {
                    item = back;
                    space.await;
                }
            }
        }
    }
}

impl<T> Drop for StageInput<T> {
    fn drop(&mut self) {
        // The last producer gone: wake the worker so it can drain and end.
        if self.inbox.producers.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.inbox.signal.notify_one();
        }
    }
}

/// The single lifecycle owner. Producers receive only the input handle.
#[must_use]
pub struct StageWorker<T> {
    inbox: Arc<Inbox<T>>,
    task: tokio::task::JoinHandle<()>,
}

impl<T> StageWorker<T> {
    /// Close intake immediately. The worker finishes its accepted inputs and releases its state.
    pub fn stop(&self) {
        self.inbox.close();
    }

    /// Wait for drain and teardown after stop, or after every producer has left.
    pub async fn join(self) -> Result<(), tokio::task::JoinError> {
        self.task.await
    }
}

impl<T> Inbox<T> {
    fn close(&self) {
        let guard = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        self.stopped.store(true, Ordering::Release);
        drop(guard);
        self.signal.notify_one();
        self.space.notify_waiters();
    }
}

/// Start a worker and give its producers and lifecycle owner distinct handles.
pub fn spawn_stage<S: Stage>(mut stage: S) -> (StageInput<S::Input>, StageWorker<S::Input>) {
    let inbox = Arc::new(Inbox {
        producers: AtomicUsize::new(1),
        pending: Mutex::new(Batch::default()),
        depth: S::DEPTH.max(1),
        signal: Notify::new(),
        space: Notify::new(),
        stopped: AtomicBool::new(false),
        overflow: S::OVERFLOW,
    });
    let input = StageInput { inbox: Arc::clone(&inbox) };
    let worker_inbox = Arc::clone(&inbox);
    let task = tokio::spawn(async move {
        // Marks the stage stopped however the worker ends: drained and closed, or its handler
        // panicked (the task's future is dropped as the panic unwinds). A waiting producer wakes
        // and gets its input back instead of waiting on a slot nothing will take.
        let _stops = StopsOnExit(Arc::clone(&inbox));
        loop {
            // Everything waiting is taken in one wake and handled in order: the lock is held for the
            // take alone, never across the work.
            let (next, closed) = {
                let mut pending = inbox.pending.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned slot still holds plain data, as in push
                (std::mem::take(&mut *pending), inbox.stopped.load(Ordering::Acquire) || inbox.producers.load(Ordering::Acquire) == 0)
            };
            if !next.ready.is_empty() {
                inbox.space.notify_waiters();
                drop(next.retired);
                stage.handle_all(next.ready.into()).await;
            } else if closed {
                return;
            } else {
                inbox.signal.notified().await;
            }
        }
    });
    (input, StageWorker { inbox: worker_inbox, task })
}

struct StopsOnExit<T>(Arc<Inbox<T>>);

impl<T> Drop for StopsOnExit<T> {
    fn drop(&mut self) {
        // Under the slot's lock, so a push either landed before (and is drained or dropped with
        // the stage) or sees `stopped` and keeps its input.
        let mut pending = self.0.pending.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned slot still holds plain data, as in push
        self.0.stopped.store(true, Ordering::Release);
        let abandoned = std::mem::take(&mut *pending);
        drop(pending);
        self.0.space.notify_waiters();
        drop(abandoned);
    }
}
