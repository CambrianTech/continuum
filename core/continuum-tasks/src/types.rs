//! The task contract's types.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A task's handle: what its owner holds, names it by, and stops it with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(Uuid);

impl TaskId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Who started a task: the identity its end is delivered to (a persona's peer id). `None`
/// for the substrate's own work, whose end reaches only the sinks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Owner(pub Uuid);

/// What kind of work a task is. A command is the one-step case; a sentinel is a pipeline of
/// steps, one kind of which is a mind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskKind {
    Command(String),
    Sentinel(String),
}

/// How a task ended. Every task ends exactly once with one of these.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskEnd {
    Done(serde_json::Value),
    Failed(String),
    TimedOut,
    /// Its owner (or an operator) stopped it.
    Stopped,
    /// It ended without saying how: it panicked, was aborted, or its process died.
    Lost(String),
}

impl TaskEnd {
    pub fn succeeded(&self) -> bool {
        matches!(self, Self::Done(_))
    }
}

/// A task as the registry knows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskRow {
    pub id: TaskId,
    pub owner: Option<Owner>,
    pub kind: TaskKind,
    pub started_ms: u64,
}

/// Where ends go. The kernel installs one that publishes `command:completed`; the mind's
/// delivery files the end into the owner's working memory and wakes her.
pub trait EndSink: Send + Sync {
    fn ended(&self, row: &TaskRow, end: &TaskEnd);
}

/// What waiting on a task for one window gave: its end, or still running.
#[derive(Clone, Debug, PartialEq)]
pub enum Landing {
    Landed(TaskEnd),
    Running(TaskId),
}

/// Why a stop was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopRefused {
    /// No running task has this id (it already ended, or never existed).
    NotRunning(TaskId),
    /// Only the owner stops its task.
    NotOwner(TaskId),
}

impl std::fmt::Display for StopRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRunning(id) => write!(f, "task {id} is not running: it already ended, or never existed"),
            Self::NotOwner(id) => write!(f, "task {id} belongs to another owner: only its owner stops it"),
        }
    }
}
