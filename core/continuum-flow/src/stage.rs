//! A stage: CBAR's `QueueThread<T>` (`cb-mobile-sdk/cpp/utility/Threads.h`) in Rust. A bounded
//! input that keeps the newest (`push` drops the oldest past `DEPTH` and signals), one worker
//! task that wakes only on a push, and a class. A stage is its handler and nothing else: the
//! queue, its bound, waking and shutdown live here, never in the subsystem.

use crate::Qos;
use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

pub trait Stage: Send + 'static {
    /// What wakes it: a signal, a digest handle, a frame.
    type Input: Send + 'static;
    /// Its class, for any capacity it submits to.
    const QOS: Qos;
    /// How many inputs it holds; past this the oldest is dropped. 1 = only the newest matters.
    const DEPTH: usize = 1;
    fn handle(&mut self, input: Self::Input) -> impl Future<Output = ()> + Send;
}

struct Inbox<T> {
    items: Mutex<VecDeque<T>>,
    signal: Notify,
    depth: usize,
}

/// The handle producers push into. Cheap to clone. Dropping every clone ends the stage once
/// its queued inputs are handled.
pub struct StageInput<T> {
    inbox: Arc<Inbox<T>>,
    alive: Arc<()>,
}

impl<T> Clone for StageInput<T> {
    fn clone(&self) -> Self {
        Self { inbox: Arc::clone(&self.inbox), alive: Arc::clone(&self.alive) }
    }
}

impl<T> StageInput<T> {
    /// Insert, dropping the oldest past the stage's depth, and signal. Returns how many it dropped.
    /// The lock is held for the insert alone, never across work (CBAR's `addItem`).
    pub fn push(&self, item: T) -> usize {
        let dropped = {
            let mut items = self.inbox.items.lock().unwrap_or_else(|e| e.into_inner()); // a poisoned queue still holds plain data: keep serving it
            let mut dropped = 0;
            while items.len() >= self.inbox.depth {
                items.pop_front();
                dropped += 1;
            }
            items.push_back(item);
            dropped
        };
        self.inbox.signal.notify_one();
        dropped
    }
}

impl<T> Drop for StageInput<T> {
    fn drop(&mut self) {
        // The last producer gone: wake the worker so it can drain and end.
        if Arc::strong_count(&self.alive) == 1 {
            self.inbox.signal.notify_one();
        }
    }
}

/// Start `stage` on its own task; returns its input.
pub fn spawn_stage<S: Stage>(mut stage: S) -> StageInput<S::Input> {
    let inbox = Arc::new(Inbox { items: Mutex::new(VecDeque::new()), signal: Notify::new(), depth: S::DEPTH.max(1) });
    let alive = Arc::new(());
    let input = StageInput { inbox: Arc::clone(&inbox), alive: Arc::clone(&alive) };
    let watch = Arc::downgrade(&alive);
    drop(alive);
    tokio::spawn(async move {
        loop {
            let next = inbox.items.lock().unwrap_or_else(|e| e.into_inner()).pop_front(); // see push
            match next {
                Some(item) => stage.handle(item).await,
                None if watch.strong_count() == 0 => return,
                None => inbox.signal.notified().await,
            }
        }
    });
    input
}
