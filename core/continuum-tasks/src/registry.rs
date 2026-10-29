//! The registry: running tasks, their stop signals, and the sinks every end is delivered to.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use tokio::sync::watch;

use crate::types::{EndSink, Landing, Owner, StopRefused, TaskEnd, TaskId, TaskKind, TaskRow};

struct Running {
    row: TaskRow,
    stop: watch::Sender<bool>,
}

/// The process table of a node's tasks. Holds only RUNNING tasks: an end leaves the table and
/// is delivered to every sink, so the table never grows with history.
#[derive(Default)]
pub struct TaskRegistry {
    running: Mutex<HashMap<TaskId, Running>>,
    sinks: RwLock<Vec<Arc<dyn EndSink>>>,
}

impl TaskRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Every end from now on is delivered to `sink` too.
    pub fn add_sink(&self, sink: Arc<dyn EndSink>) {
        self.sinks.write().push(sink);
    }

    /// Start a task: its handle exists before any work runs. The guard is held by whatever
    /// runs the work and ends it; the watch is held by whoever may wait on it for a window.
    pub fn start(self: &Arc<Self>, owner: Option<Owner>, kind: TaskKind) -> (TaskGuard, TaskWatch) {
        let id = TaskId::new();
        let row = TaskRow { id, owner, kind, started_ms: now_ms() };
        let (stop, stop_rx) = watch::channel(false);
        let (ended_tx, ended_rx) = watch::channel(None);
        self.running.lock().insert(id, Running { row, stop });
        let guard = TaskGuard { registry: Arc::clone(self), id, ended: Some(ended_tx), stop: stop_rx };
        (guard, TaskWatch { id, ended: ended_rx })
    }

    /// The running tasks `owner` started (`None`: every running task).
    pub fn list(&self, owner: Option<Owner>) -> Vec<TaskRow> {
        let running = self.running.lock();
        let mut rows: Vec<TaskRow> = running
            .values()
            .filter(|r| owner.is_none() || r.row.owner == owner)
            .map(|r| r.row.clone())
            .collect();
        rows.sort_by_key(|r| r.started_ms);
        rows
    }

    /// Ask a running task to stop. Its runner sees the request and ends it as `Stopped`.
    /// `by: None` is the substrate's own authority (an operator, a shutdown); an owner stops
    /// only its own task.
    pub fn stop(&self, id: TaskId, by: Option<Owner>) -> Result<(), StopRefused> {
        let running = self.running.lock();
        let task = running.get(&id).ok_or(StopRefused::NotRunning(id))?;
        if by.is_some() && by != task.row.owner {
            return Err(StopRefused::NotOwner(id));
        }
        task.stop.send_replace(true);
        Ok(())
    }

    /// The one terminal transition: out of the table, then to every sink, outside the lock.
    fn finish(&self, id: TaskId, end: &TaskEnd) {
        let Some(task) = self.running.lock().remove(&id) else { return };
        let sinks: Vec<Arc<dyn EndSink>> = self.sinks.read().clone();
        for sink in sinks {
            sink.ended(&task.row, end);
        }
    }
}

/// Held by whatever runs a task. Ending it consumes the guard, so a task ends once; dropping
/// it unended (a panic, an abort, a runner that died) ends it as `Lost`, so no path is silent.
pub struct TaskGuard {
    registry: Arc<TaskRegistry>,
    id: TaskId,
    ended: Option<watch::Sender<Option<TaskEnd>>>,
    stop: watch::Receiver<bool>,
}

impl TaskGuard {
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Resolves when the task is asked to stop; the runner then ends it as `Stopped`.
    pub fn stop_requested(&self) -> watch::Receiver<bool> {
        self.stop.clone()
    }

    pub fn end(mut self, end: TaskEnd) {
        self.finish(end);
    }

    fn finish(&mut self, end: TaskEnd) {
        let Some(ended) = self.ended.take() else { return };
        self.registry.finish(self.id, &end);
        ended.send_replace(Some(end));
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        let why = if std::thread::panicking() { "its runner panicked" } else { "its runner ended without reporting an end (aborted, or its process died)" };
        self.finish(TaskEnd::Lost(why.to_string()));
    }
}

/// Held by whoever may wait on a task for a window (an act).
pub struct TaskWatch {
    id: TaskId,
    ended: watch::Receiver<Option<TaskEnd>>,
}

impl TaskWatch {
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Its end if it comes within `within`, else `Running`: the act's wait, never the work's
    /// limit. A task still running keeps running, and its end still reaches every sink.
    pub async fn land(&mut self, within: Duration) -> Landing {
        match tokio::time::timeout(within, self.ended.wait_for(Option::is_some)).await {
            Ok(Ok(end)) => match end.clone() {
                Some(end) => Landing::Landed(end),
                None => Landing::Running(self.id),
            },
            // The guard always sends before it is dropped; a closed channel without an end
            // can only mean the guard was leaked, which is a lost task.
            Ok(Err(_)) => Landing::Landed(TaskEnd::Lost("its guard closed without an end".to_string())),
            Err(_) => Landing::Running(self.id),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64) // a clock before 1970 reads 0: display only, never a decision
}
