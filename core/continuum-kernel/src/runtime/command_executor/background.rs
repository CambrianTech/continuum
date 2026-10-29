//! Persist acceptance before execution and the terminal result before notification.
use super::{CommandExecutor, DispatchFailure};
use crate::routing::{CallerIdentity, CommandUri};
use crate::runtime::tasks::{TaskEnd, TaskLedger, TaskOutput};
use futures::FutureExt;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

impl CommandExecutor {
    /// Explicit storage injection for an embedded executor or an isolated fixture.
    pub fn set_task_ledger(&self, ledger: Arc<TaskLedger>) -> Result<(), String> {
        self.task_ledger
            .set(ledger)
            .map_err(|_| "task ledger already initialized".into())
    }

    /// Resolve the same node store used by modules; never create a private database.
    pub async fn task_ledger(&self) -> Result<&Arc<TaskLedger>, String> {
        self.task_ledger
            .get_or_try_init(|| async {
                let data = self
                    .registry
                    .get_by_name("data")
                    .ok_or("background tasks require the data module")?;
                let storage = data
                    .as_storage()
                    .ok_or("background tasks require the data storage provider")?;
                let adapter = storage.adapter("main").await?;
                TaskLedger::new(adapter).await.map(Arc::new)
            })
            .await
    }

    /// Await durable acceptance only, not execution. A refusal has no command effects.
    /// The caller identity owns the receipt; command parameters cannot choose its owner.
    pub async fn dispatch_background(
        self: &Arc<Self>,
        command: impl Into<CommandUri>,
        params: Value,
        caller: Option<CallerIdentity>,
    ) -> Result<Uuid, DispatchFailure> {
        let caller = caller.ok_or_else(|| {
            DispatchFailure::not_committed("background tasks require an authenticated owner".into())
        })?;
        let owner = caller.peer_id.as_uuid();
        let command: CommandUri = command.into();
        let ledger = Arc::clone(
            self.task_ledger()
                .await
                .map_err(DispatchFailure::not_committed)?,
        );
        let handle = ledger
            .accept(owner, command.to_string())
            .await
            .map_err(DispatchFailure::not_committed)?;
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let start = std::time::Instant::now();
            // Keep the existing policy and hands admission scopes inside dispatch.
            // Catch an unwinding handler without detaching a second execution task.
            let outcome =
                match std::panic::AssertUnwindSafe(this.dispatch(&command, params, Some(&caller)))
                    .catch_unwind()
                    .await
                {
                    Ok(outcome) => outcome,
                    Err(_) => Err(DispatchFailure::uncertain(
                        "background command panicked".into(),
                    )),
                };
            let outcome = outcome.and_then(|result| {
                TaskOutput::try_from(&result)
                    .map(|output| (result, output))
                    .map_err(DispatchFailure::uncertain)
            });
            let (end, outcome) = match outcome {
                Ok((result, output)) => (TaskEnd::Done(output), Ok(result)),
                Err(error) => (
                    TaskEnd::Failed {
                        message: error.message.clone(),
                        outcome: error.outcome,
                    },
                    Err(error),
                ),
            };
            if let Err(error) = ledger.finish(handle, owner, end).await {
                tracing::error!(%handle, %owner, %error, "task ended but terminal persistence failed; acceptance remains unresolved");
                return;
            }
            this.emit_command_completed(
                command.path(),
                &outcome,
                start.elapsed().as_millis() as u64,
                Some(handle),
            );
        });
        Ok(handle)
    }
}
