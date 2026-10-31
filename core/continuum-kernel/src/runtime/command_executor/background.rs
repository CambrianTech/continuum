//! Persist acceptance before execution and the terminal result before notification.
use super::{CommandExecutor, DispatchFailure};
use crate::routing::{CallerIdentity, CommandUri};
use crate::runtime::tasks::{Committed, CustodyError, HeldTerminal, TaskEnd, TaskLedger, TaskOutput};
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
                TaskOutput::try_from(result).map_err(DispatchFailure::uncertain)
            });
            let end = match outcome {
                Ok(output) => TaskEnd::Done(output),
                Err(error) => TaskEnd::Failed {
                    message: error.message,
                    outcome: error.outcome,
                },
            };
            // Custody first: the end is held until the ledger commits it, so a failed commit keeps
            // it for the next flush instead of losing it (Codex on #5585).
            let terminal = HeldTerminal { task: handle, owner, end, command: command.path().to_string(), elapsed_ms: start.elapsed().as_millis() as u64 };
            if this.task_custody.hold(terminal.clone()).is_err() {
                tracing::error!(%handle, %owner, ?terminal.end, "task ended after shutdown closed custody; reported, not delivered");
                return;
            }
            match this.task_custody.commit(ledger.as_ref(), handle).await {
                Ok(Some(committed)) => this.deliver_terminal(committed),
                Ok(None) => {}
                Err(CustodyError::Store(error)) => tracing::warn!(%handle, %owner, %error, "terminal commit failed; held for the next flush"),
                // Shutdown is draining: the terminal is held, and shutdown commits or reports it.
                Err(CustodyError::Closed) => {}
            }
        });
        Ok(handle)
    }
}

impl CommandExecutor {
    /// Retry every held terminal against the ledger and deliver what commits. Called on the
    /// existing lifecycle tick (cognition's task reconcile), never by a timer of its own.
    pub async fn flush_task_terminals(&self) -> Result<usize, String> {
        let ledger = self.task_ledger().await?;
        let report = match self.task_custody.flush(ledger.as_ref()).await {
            Ok(report) => report,
            // Shutdown is draining and makes the final pass itself.
            Err(CustodyError::Closed) => return Ok(0),
            Err(CustodyError::Store(error)) => return Err(error),
        };
        for (task, reason) in &report.failed {
            continuum_base::probe!(
                class = "tasks.terminal.commit_failed",
                task = %task,
                reason = %reason,
                "a held terminal's durable commit failed on this tick; it stays held for the next"
            );
        }
        let delivered = report.committed.len();
        report.committed.into_iter().for_each(|done| self.deliver_terminal(done));
        Ok(delivered)
    }

    /// Close custody at shutdown: wait for in-flight commits, try once more, deliver what committed,
    /// and return every terminal still held so the supervisor reports it (never dropped silently).
    pub async fn shutdown_task_terminals(&self) -> Result<Vec<HeldTerminal>, String> {
        let ledger = self.task_ledger().await?;
        let report = self.task_custody.shutdown(ledger.as_ref()).await;
        for (task, reason) in &report.last_pass.failed {
            continuum_base::probe!(
                class = "tasks.terminal.commit_failed",
                task = %task,
                reason = %reason,
                "shutdown's final commit of a held terminal failed; it is reported, not dropped"
            );
        }
        report.last_pass.committed.into_iter().for_each(|done| self.deliver_terminal(done));
        Ok(report.unresolved)
    }

    /// Deliver a committed terminal: the store's durable winner, never this writer's proposal.
    fn deliver_terminal(&self, committed: Committed) {
        let outcome = completion(committed.winner);
        self.emit_command_completed(&committed.held.command, &outcome, committed.held.elapsed_ms, Some(committed.held.task));
    }
}

fn completion(end: TaskEnd) -> Result<crate::runtime::CommandResult, DispatchFailure> {
    match end {
        TaskEnd::Done(output) => Ok(output.into()),
        TaskEnd::Failed { message, outcome } => Err(DispatchFailure { message, outcome }),
        TaskEnd::TimedOut => Err(DispatchFailure::uncertain("task timed out".into())),
        TaskEnd::Stopped => Err(DispatchFailure::uncertain("task stopped".into())),
        TaskEnd::Lost => Err(DispatchFailure::uncertain("task execution lost".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::super::DispatchOutcome;
    use super::*;
    use crate::runtime::{
        CommandInterceptor, CommandResult, InterceptorOutcome, MessageBus, ModuleRegistry,
    };

    struct GatedResult {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
        unwind: bool,
    }

    #[async_trait::async_trait]
    impl CommandInterceptor for GatedResult {
        fn name(&self) -> &'static str {
            "gated-test-result"
        }
        async fn try_route(
            &self,
            _: &str,
            _: &Value,
            _: Option<&CallerIdentity>,
        ) -> Result<InterceptorOutcome, String> {
            self.entered.notify_one();
            self.release.notified().await;
            if self.unwind {
                // Fault injection: exercise the real supervisor's unwind boundary
                // without relying on a process-global panic hook or aborting process.
                std::panic::resume_unwind(Box::new("injected handler unwind"));
            }
            Ok(InterceptorOutcome::Handled(CommandResult::Json(
                Value::Null,
            )))
        }
    }

    #[tokio::test]
    async fn terminal_delivery_handles_competing_writers_and_unwinding_handlers(
    ) -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let ledger = Arc::new(TaskLedger::new(adapter).await?);
        for unwind in [false, true] {
            let gate = Arc::new(GatedResult {
                entered: tokio::sync::Notify::new(),
                release: tokio::sync::Notify::new(),
                unwind,
            });
            let bus = Arc::new(MessageBus::new());
            let mut events = bus.receiver();
            let executor = Arc::new(
                CommandExecutor::new(Arc::new(ModuleRegistry::new()))
                    .with_message_bus(bus)
                    .with_interceptor(gate.clone()),
            );
            executor.set_task_ledger(ledger.clone())?;
            let owner = Uuid::new_v4();
            let caller = CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(owner));
            let task = executor
                .dispatch_background("test/result", Value::Null, Some(caller))
                .await
                .map_err(|error| error.message)?;
            tokio::time::timeout(std::time::Duration::from_secs(5), gate.entered.notified())
                .await
                .map_err(|error| error.to_string())?;
            // Inject another writer's committed terminal; this tests arbitration,
            // not permission to declare a running command stopped.
            let winner = TaskEnd::Failed {
                message: if unwind {
                    "background command panicked"
                } else {
                    "previous terminal"
                }
                .into(),
                outcome: DispatchOutcome::Uncertain,
            };
            if !unwind {
                ledger.finish(task, owner, winner.clone()).await?;
            }
            gate.release.notify_one();
            let event = tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            assert_eq!(event.name, "command:completed");
            assert_eq!(event.payload["handle"], task.to_string());
            assert_eq!(event.payload["success"], false);
            assert_eq!(
                event.payload["error"],
                if unwind {
                    "background command panicked"
                } else {
                    "previous terminal"
                }
            );
            assert_eq!(ledger.pending(task, owner).await?, Some(winner));
        }
        Ok(())
    }
}
