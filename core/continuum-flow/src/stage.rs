//! A stage: CBAR's `QueueThread<T>` (`cb-mobile-sdk/cpp/utility/Threads.h`) in Rust. A bounded
//! input (reliable by default; a signal stage keeps the newest), one worker
//! task that wakes only on a push, and a class. A stage is its handler and nothing else: the
//! queue, its bound, waking and shutdown live here, never in the subsystem.

use crate::Qos;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Notify;

/// What a full stage does with a new input. Signals and reliable inputs are different types
/// (Codex on FLOW plan 6d18b4564): a dropped tool result is lost work; a dropped wake signal is
/// not, because it only references content already committed elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    /// Never drop. A full stage hands the item back to the producer (backpressure), who must
    /// handle it. The default.
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
    signal: Notify,
    overflow: Overflow,
}

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
    /// Put `item` in the pending slot and signal. A `Reliable` stage whose slot is full hands the
    /// item back (`Err`) and drops nothing; a `KeepNewest` stage replaces the pending input and
    /// returns how many it dropped (0 or 1). The lock is held for the insert alone, never across
    /// work (CBAR's `addItem`).
    pub fn push(&self, item: T) -> Result<usize, T> {
        let dropped = {
            let mut pending = self.inbox.pending.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned slot still holds plain data: keep serving it
            let dropped = match (pending.is_some(), self.inbox.overflow) {
                (true, Overflow::Reliable) => return Err(item),
                (true, Overflow::KeepNewest) => 1,
                (false, _) => 0,
            };
            *pending = Some(item);
            dropped
        };
        self.inbox.signal.notify_one();
        Ok(dropped)
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
    let inbox = Arc::new(Inbox { producers: AtomicUsize::new(1), pending: Mutex::new(None), signal: Notify::new(), overflow: S::OVERFLOW });
    let input = StageInput { inbox: Arc::clone(&inbox) };
    tokio::spawn(async move {
        loop {
            let (next, closed) = {
                let mut pending = inbox.pending.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned slot still holds plain data, as in push
                (pending.take(), inbox.producers.load(Ordering::Acquire) == 0)
            };
            match next {
                Some(item) => stage.handle(item).await,
                None if closed => return,
                None => inbox.signal.notified().await,
            }
        }
    });
    input
}
