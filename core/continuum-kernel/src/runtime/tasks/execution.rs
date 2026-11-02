//! Durable identity of the execution owner, distinct from the result recipient.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A command executor instance owns its in-process dispatch until quiescence.
/// This is not a PID: a replacement executor gets a new incarnation even in the
/// same process. Absence from a process snapshot does not prove this owner ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskExecution {
    CommandExecutor { incarnation: Uuid },
}

impl TaskExecution {
    pub fn new_command_executor() -> Self {
        Self::CommandExecutor {
            incarnation: Uuid::new_v4(),
        }
    }
}

/// A refused terminal publication, distinct from a completed task's outcome.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FinishError {
    #[error("task completion authorization failed: {0}")]
    Authorization(String),
    #[error(
        "task {task} completion execution mismatch: accepted {accepted:?}, offered {offered:?}"
    )]
    ExecutionMismatch {
        task: Uuid,
        accepted: TaskExecution,
        offered: TaskExecution,
    },
    #[error("task terminal persistence failed: {0}")]
    Persistence(String),
}

// Command/test boundaries that expose textual errors can render the typed refusal;
// reconciliation inside the task owner matches FinishError directly.
impl From<FinishError> for String {
    fn from(error: FinishError) -> Self {
        error.to_string()
    }
}
