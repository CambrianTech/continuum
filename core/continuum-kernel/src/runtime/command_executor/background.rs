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
        // Admission precedes the first storage await. A close either refuses
        // this request without effects or sees custody throughout acceptance.
        let custody = self.registry.task_admission.admit_owned().ok_or_else(|| {
            DispatchFailure::not_committed("runtime is draining background tasks".into())
        })?;
        let ledger = Arc::clone(
            self.task_ledger()
                .await
                .map_err(DispatchFailure::not_committed)?,
        );
        // The execution task owns persistence too. Dropping the caller while an
        // insert is in flight must not strand a committed acceptance without work.
        let (accepted, receipt) = tokio::sync::oneshot::channel();
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let _custody = custody;
            let handle = match ledger.accept(owner, command.to_string()).await {
                Ok(handle) => handle,
                Err(error) => {
                    let _ = accepted.send(Err(DispatchFailure::not_committed(error)));
                    return;
                }
            };
            // A forgotten receipt does not cancel accepted work; its durable
            // terminal remains available to the authenticated owner's replay.
            let _ = accepted.send(Ok(handle));
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
            let winner = match ledger.finish(handle, owner, end).await {
                Ok(winner) => winner,
                Err(error) => {
                    tracing::error!(%handle, %owner, %error, "task ended but terminal persistence failed; acceptance remains unresolved");
                    this.registry
                        .task_persistence_failed
                        .store(true, std::sync::atomic::Ordering::Release);
                    return;
                }
            };
            // A competing supervisor may have persisted a different terminal first.
            // Every delivery must project that durable winner, never this writer's loser.
            let outcome = completion(winner);
            this.emit_command_completed(
                command.path(),
                &outcome,
                start.elapsed().as_millis() as u64,
                Some(handle),
            );
        });
        receipt.await.map_err(|_| {
            DispatchFailure::uncertain(
                "background acceptance owner stopped before reporting".into(),
            )
        })?
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

    #[tokio::test]
    async fn closed_runtime_refuses_before_resolving_storage() -> Result<(), String> {
        let registry = Arc::new(ModuleRegistry::new());
        registry.task_admission.close();
        // No data module or injected ledger: a request that gets past admission
        // would fail at storage resolution with a different error.
        let executor = Arc::new(CommandExecutor::new(registry.clone()));
        let caller =
            CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(Uuid::new_v4()));
        let result = executor
            .dispatch_background("test/result", Value::Null, Some(caller))
            .await;
        let Err(error) = result else {
            return Err("closed runtime admitted background work".into());
        };
        assert_eq!(error.message, "runtime is draining background tasks");
        assert_eq!(error.outcome, DispatchOutcome::NotCommitted);
        assert_eq!(registry.task_admission.in_flight(), 0);
        Ok(())
    }

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
    async fn dropping_acceptance_waiter_does_not_orphan_execution() -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let ledger = Arc::new(TaskLedger::new(adapter).await?);
        let gate = Arc::new(GatedResult {
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            unwind: false,
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
        {
            let waiting = executor.dispatch_background("test/result", Value::Null, Some(caller));
            tokio::pin!(waiting);
            // On this current-thread runtime the new execution task cannot run
            // during poll: abandon its receipt before it starts acceptance.
            assert!(futures::poll!(waiting.as_mut()).is_pending());
        }
        tokio::time::timeout(std::time::Duration::from_secs(5), gate.entered.notified())
            .await
            .map_err(|error| error.to_string())?;
        assert_eq!(executor.registry.task_admission.in_flight(), 1);
        let drain = executor.registry.task_admission.close_and_wait();
        tokio::pin!(drain);
        assert!(futures::poll!(drain.as_mut()).is_pending());
        gate.release.notify_one();
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
        let handle = event.payload["handle"]
            .as_str()
            .ok_or("completion has no task handle")?;
        let handle = Uuid::parse_str(handle).map_err(|error| error.to_string())?;
        assert!(matches!(
            ledger.pending(handle, owner).await?,
            Some(TaskEnd::Done(_))
        ));
        tokio::time::timeout(std::time::Duration::from_secs(5), drain)
            .await
            .map_err(|error| error.to_string())?;
        assert_eq!(executor.registry.task_admission.in_flight(), 0);
        Ok(())
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
