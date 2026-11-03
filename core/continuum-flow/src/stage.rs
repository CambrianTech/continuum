//! A stage: CBAR's `QueueThread<T>` (`cb-mobile-sdk/cpp/utility/Threads.h`) in Rust. One input in
//! hand and one pending (reliable by default; a signal stage keeps the newest), one worker task that
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
    KeepNewest,
}

pub trait Stage: Send + 'static {
    /// What wakes it: a signal, a digest handle, a frame, a result.
    type Input: Send + 'static;
    /// Its class, for any capacity it submits to.
    const QOS: Qos;
    /// What a full stage does: never drops unless a signal stage opts in.
    const OVERFLOW: Overflow = Overflow::Reliable;
    fn handle(&mut self, input: Self::Input) -> impl Future<Output = ()> + Send;
}

/// A stage holds one input in hand (the one it is handling) and one pending: CBAR's double
/// buffer. A producer that finds the pending slot full gets its input back (`Reliable`) or replaces
/// the pending one (`KeepNewest`), so no stage sizes a queue of its own.
struct Inbox<T> {
    producers: AtomicUsize,
    pending: Mutex<Option<T>>,
    /// Wakes the worker: an input arrived, or the last producer left.
    signal: Notify,
    /// Wakes a waiting producer: the worker took the pending input, or the stage stopped.
    space: Notify,
    /// The worker has ended (every producer gone and the slot drained, or its handler panicked):
    /// nothing more will be taken, so an input is handed back instead of waiting forever.
    stopped: AtomicBool,
    overflow: Overflow,
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
            let dropped = match (pending.is_some(), self.inbox.overflow) {
                (true, Overflow::Reliable) => return Err(PushError::Full(item)),
                (true, Overflow::KeepNewest) => 1,
                (false, _) => 0,
            };
            *pending = Some(item);
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

/// Start `stage` on its own task; returns its input.
pub fn spawn_stage<S: Stage>(mut stage: S) -> StageInput<S::Input> {
    let inbox = Arc::new(Inbox {
        producers: AtomicUsize::new(1),
        pending: Mutex::new(None),
        signal: Notify::new(),
        space: Notify::new(),
        stopped: AtomicBool::new(false),
        overflow: S::OVERFLOW,
    });
    let input = StageInput { inbox: Arc::clone(&inbox) };
    tokio::spawn(async move {
        // Marks the stage stopped however the worker ends: drained and closed, or its handler
        // panicked (the task's future is dropped as the panic unwinds). A waiting producer wakes
        // and gets its input back instead of waiting on a slot nothing will take.
        let _stops = StopsOnExit(Arc::clone(&inbox));
        loop {
            let (next, closed) = {
                let mut pending = inbox.pending.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned slot still holds plain data, as in push
                (pending.take(), inbox.producers.load(Ordering::Acquire) == 0)
            };
            match next {
                Some(item) => {
                    inbox.space.notify_one();
                    stage.handle(item).await;
                }
                None if closed => return,
                None => inbox.signal.notified().await,
            }
        }
    });
    input
}

struct StopsOnExit<T>(Arc<Inbox<T>>);

impl<T> Drop for StopsOnExit<T> {
    fn drop(&mut self) {
        // Under the slot's lock, so a push either landed before (and is drained or dropped with
        // the stage) or sees `stopped` and keeps its input.
        let pending = self.0.pending.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned slot still holds plain data, as in push
        self.0.stopped.store(true, Ordering::Release);
        drop(pending);
        self.0.space.notify_waiters();
    }
}
