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
