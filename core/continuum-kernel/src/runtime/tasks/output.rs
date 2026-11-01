//! Lossless durable command results. This is storage, not prompt material: consumers
//! budget a reference to the result rather than copying binary bytes into context.

use crate::runtime::{cell_shapes::HandleRef, CommandResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", content = "payload", rename_all = "snake_case")]
pub enum TaskOutput {
    Json(Value),
    Binary { metadata: Value, data: Vec<u8> },
    Handle(HandleRef),
}

impl TryFrom<CommandResult> for TaskOutput {
    type Error = String;

    fn try_from(result: CommandResult) -> Result<Self, Self::Error> {
        match result {
            CommandResult::Json(value) => Ok(Self::Json(value)),
            CommandResult::Binary { metadata, data } => Ok(Self::Binary { metadata, data }),
            CommandResult::Handle(handle) => Ok(Self::Handle(handle)),
            CommandResult::Stream(_) => Err(CommandResult::stream_protocol_error()),
            CommandResult::Lambda(_) => Err(CommandResult::lambda_protocol_error()),
        }
    }
}

impl From<TaskOutput> for CommandResult {
    fn from(output: TaskOutput) -> Self {
        match output {
            TaskOutput::Json(value) => Self::Json(value),
            TaskOutput::Binary { metadata, data } => Self::Binary { metadata, data },
            TaskOutput::Handle(handle) => Self::Handle(handle),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::tasks::{TaskEnd, TaskExecution, TaskLedger};
    use uuid::Uuid;

    #[tokio::test]
    async fn binary_and_handle_survive_ledger_reopen() -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let ledger = TaskLedger::new(adapter.clone()).await?;
        let owner = Uuid::new_v4();
        let bytes: Vec<u8> = (0..=255).collect();
        let binary = TaskOutput::try_from(CommandResult::Binary {
            metadata: serde_json::json!({"format": "arbitrary-bytes"}),
            data: bytes.clone(),
        })?;
        assert!(matches!(&binary, TaskOutput::Binary { data, .. } if data == &bytes));
        let handle = TaskOutput::try_from(CommandResult::handle(
            "producer",
            Uuid::new_v4(),
            "producer::Result",
        ))?;
        assert!(matches!(&handle, TaskOutput::Handle(_)));
        let binary_task = ledger
            .accept(
                owner,
                "producer/binary".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        let handle_task = ledger
            .accept(
                owner,
                "producer/handle".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        ledger
            .finish(binary_task, owner, TaskEnd::Done(binary.clone()))
            .await?;
        ledger
            .finish(handle_task, owner, TaskEnd::Done(handle.clone()))
            .await?;
        ledger.acknowledge(binary_task, owner).await?;
        assert_eq!(ledger.pending(binary_task, owner).await?, None);
        assert!(ledger.terminal(binary_task, Uuid::new_v4()).await.is_err());
        drop(ledger);
        let reopened = TaskLedger::new(adapter).await?;
        assert_eq!(
            reopened.terminal(binary_task, owner).await?,
            Some(TaskEnd::Done(binary))
        );
        assert_eq!(
            reopened.pending(handle_task, owner).await?,
            Some(TaskEnd::Done(handle))
        );
        Ok(())
    }
}
