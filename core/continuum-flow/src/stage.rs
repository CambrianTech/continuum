//! A stage: CBAR's `QueueThread<T>` (`cb-mobile-sdk/cpp/utility/Threads.h`) in Rust. A bounded
//! input (reliable by default; a signal stage keeps the newest), one worker
//! task that wakes only on a push, and a class. A stage is its handler and nothing else: the
//! queue, its bound, waking and shutdown live here, never in the subsystem.

use crate::Qos;
use std::collections::VecDeque;
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
    /// How many inputs it holds before `OVERFLOW` applies.
    const DEPTH: usize = 1;
    /// What a full stage does: never drops unless a signal stage opts in.
    const OVERFLOW: Overflow = Overflow::Reliable;
    fn handle(&mut self, input: Self::Input) -> impl Future<Output = ()> + Send;
}

struct Inbox<T> {
    producers: AtomicUsize,
    items: Mutex<VecDeque<T>>,
    signal: Notify,
    depth: usize,
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
    /// Insert and signal. A `KeepNewest` stage drops its oldest past its depth and returns how
    /// many it dropped; a `Reliable` stage at its depth hands the item back (`Err`) and drops
    /// nothing. The lock is held for the insert alone, never across work (CBAR's `addItem`).
    pub fn push(&self, item: T) -> Result<usize, T> {
        let dropped = {
            let mut items = self.inbox.items.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned queue still holds plain data: keep serving it
            let mut dropped = 0;
            if items.len() >= self.inbox.depth {
                match self.inbox.overflow {
                    Overflow::Reliable => return Err(item),
                    Overflow::KeepNewest => {
                        while items.len() >= self.inbox.depth {
                            items.pop_front();
                            dropped += 1;
                        }
                    }
                }
            }
            items.push_back(item);
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
    let inbox = Arc::new(Inbox { producers: AtomicUsize::new(1), items: Mutex::new(VecDeque::new()), signal: Notify::new(), depth: S::DEPTH.max(1), overflow: S::OVERFLOW });
    let input = StageInput { inbox: Arc::clone(&inbox) };
    tokio::spawn(async move {
        loop {
            let (next, closed) = {
                let mut items = inbox.items.lock().unwrap_or_else(|e| e.into_inner()); // see push
                (items.pop_front(), inbox.producers.load(Ordering::Acquire) == 0)
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
