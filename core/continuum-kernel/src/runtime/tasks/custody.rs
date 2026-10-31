//! Terminal custody: a task's proposed end is held until the ledger durably commits it, and only the
//! durable winner is ever delivered. Before this, a failed `finish` logged and returned, so the end was
//! lost and its durable acceptance stayed unresolved forever (Codex on #5585, background.rs).
//!
//! The rules, each a test below:
//! - first proposal per task wins and is immutable while held;
//! - a commit never holds the custody lock across the store's IO;
//! - one commit attempt per task at a time (a claim), so a finish and a flush racing cannot both
//!   deliver; a dropped (cancelled) attempt releases its claim and keeps the terminal held;
//! - success yields the store's DURABLE winner (a competing supervisor may have committed another
//!   end first), never this writer's losing proposal;
//! - a failed commit keeps the terminal held; it is retried by `flush` on the existing lifecycle tick,
//!   never by a private timer;
//! - `shutdown` closes admission, waits for in-flight attempts, tries once more and returns every
//!   terminal it still holds, typed, so the supervisor reports them instead of dropping them.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::Notify;
use uuid::Uuid;

use super::{TaskEnd, TaskLedger};

#[cfg(test)]
mod tests;

/// Where a terminal is made durable: the ledger in production, a failing store in tests.
#[async_trait::async_trait]
pub trait TerminalStore: Send + Sync {
    /// Commit `end` for `task` and return the durable winner (the first terminal ever committed).
    async fn finish(&self, task: Uuid, owner: Uuid, end: TaskEnd) -> Result<TaskEnd, String>;
}

#[async_trait::async_trait]
impl TerminalStore for TaskLedger {
    async fn finish(&self, task: Uuid, owner: Uuid, end: TaskEnd) -> Result<TaskEnd, String> {
        TaskLedger::finish(self, task, owner, end).await
    }
}

/// A terminal held until it is durable, and what delivering it needs.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldTerminal {
    pub task: Uuid,
    pub owner: Uuid,
    pub end: TaskEnd,
    /// The command path the completion event names.
    pub command: String,
    /// How long the task ran, for the completion event.
    pub elapsed_ms: u64,
}

/// A terminal made durable: the held record and the store's winning end, which is what is delivered.
#[derive(Clone, Debug, PartialEq)]
pub struct Committed {
    pub held: HeldTerminal,
    pub winner: TaskEnd,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CustodyError {
    /// Shutdown closed admission; the caller reports the terminal itself.
    Closed,
}

#[derive(Default)]
struct State {
    held: HashMap<Uuid, HeldTerminal>,
    committing: HashSet<Uuid>,
    closed: bool,
}

#[derive(Default)]
pub struct TerminalCustody {
    state: Mutex<State>,
    settled: Notify,
}

/// Releases a commit claim however the attempt ends, including a cancelled future.
struct Claim<'a> {
    custody: &'a TerminalCustody,
    task: Uuid,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.custody.state.lock().committing.remove(&self.task);
        self.custody.settled.notify_waiters();
    }
}

impl TerminalCustody {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Take custody of a proposed end. The first proposal for a task is kept; a later one for the
    /// same task is ignored (one terminal per task).
    pub fn hold(&self, terminal: HeldTerminal) -> Result<(), CustodyError> {
        let mut state = self.state.lock();
        if state.closed {
            return Err(CustodyError::Closed);
        }
        state.held.entry(terminal.task).or_insert(terminal);
        Ok(())
    }

    /// Try to make one held terminal durable. `Ok(Some)` when this attempt committed it (deliver the
    /// winner), `Ok(None)` when it is not held or another attempt has it claimed, `Err` when the store
    /// failed (the terminal stays held for the next flush).
    pub async fn commit(&self, store: &dyn TerminalStore, task: Uuid) -> Result<Option<Committed>, String> {
        let held = {
            let mut state = self.state.lock();
            let Some(held) = state.held.get(&task).cloned() else { return Ok(None) };
            if !state.committing.insert(task) {
                return Ok(None);
            }
            held
        };
        let _claim = Claim { custody: self, task };
        let winner = store.finish(held.task, held.owner, held.end.clone()).await?;
        self.state.lock().held.remove(&task);
        Ok(Some(Committed { held, winner }))
    }

    /// Try every held terminal not already being committed; the ones made durable are returned for
    /// delivery, the rest stay held. Called on the existing lifecycle tick.
    pub async fn flush(&self, store: &dyn TerminalStore) -> Vec<Committed> {
        let tasks: Vec<Uuid> = {
            let state = self.state.lock();
            state.held.keys().filter(|task| !state.committing.contains(task)).copied().collect()
        };
        let mut committed = Vec::new();
        for task in tasks {
            if let Ok(Some(done)) = self.commit(store, task).await {
                committed.push(done);
            }
        }
        committed
    }

    /// How many terminals are held (committed or not yet).
    pub fn held(&self) -> usize {
        self.state.lock().held.len()
    }

    /// Close admission, wait for in-flight attempts, flush once more, and return what was committed
    /// (to deliver) and every terminal still held (to report: never dropped silently).
    pub async fn shutdown(&self, store: &dyn TerminalStore) -> (Vec<Committed>, Vec<HeldTerminal>) {
        self.state.lock().closed = true;
        loop {
            let settled = self.settled.notified();
            if self.state.lock().committing.is_empty() {
                break;
            }
            settled.await;
        }
        let committed = self.flush(store).await;
        let unresolved = self.state.lock().held.values().cloned().collect();
        (committed, unresolved)
    }
}
