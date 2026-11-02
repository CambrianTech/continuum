//! Durable task facts. Acceptance precedes execution; the first terminal insert wins.
//! A terminal fact is also the outbox: it remains pending until its owner acknowledges it.
//! Execution supervision and the owner sink are separate from this persistence contract.

use crate::orm::query::{QueryBuilder, QueryOperator};
use crate::orm::types::{CollectionSchema, FieldType, SchemaField};
use crate::orm::{adapter::StorageAdapter, entity::OrmEntity, store::OrmStore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::num::NonZeroUsize;
use std::sync::Arc;
use uuid::Uuid;

mod output;
pub use output::TaskOutput;
mod execution;
pub use execution::{FinishError, TaskExecution};
mod delivery;
pub use delivery::{DeliveryError, DeliveryPage, TaskOwnerSink};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskEnd {
    Done(TaskOutput),
    Failed {
        message: String,
        outcome: super::command_executor::DispatchOutcome,
    },
    TimedOut,
    Stopped,
    Lost,
}

/// The terminal fact without its ledger-owned output. Consumers may project this
/// status but must not equate it with having read the result body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskEndState {
    Done,
    Failed,
    TimedOut,
    Stopped,
    Lost,
}

impl TaskEnd {
    pub fn state(&self) -> TaskEndState {
        match self {
            Self::Done(_) => TaskEndState::Done,
            Self::Failed { .. } => TaskEndState::Failed,
            Self::TimedOut => TaskEndState::TimedOut,
            Self::Stopped => TaskEndState::Stopped,
            Self::Lost => TaskEndState::Lost,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Fact {
    Accepted {
        command: String,
        execution: TaskExecution,
    },
    Ended(TaskEnd),
    Delivered,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TaskFact {
    task: Uuid,
    owner: Uuid,
    fact: Fact,
}

impl OrmEntity for TaskFact {
    const COLLECTION: &'static str = "kernel_task_facts";

    fn collection_schema() -> CollectionSchema {
        let mut fields = crate::orm::entity::base_entity_fields();
        for (name, field_type) in [
            ("task", FieldType::Uuid),
            ("owner", FieldType::Uuid),
            ("fact", FieldType::Json),
        ] {
            fields.push(SchemaField {
                name: name.into(),
                field_type,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            });
        }
        CollectionSchema {
            collection: Self::COLLECTION.into(),
            fields,
            indexes: vec![],
        }
    }
}

/// One ledger may be reopened or shared by multiple supervisors. INSERT, rather than a
/// process-local lock or equal-version monotonic upsert, arbitrates terminal races.
pub struct TaskLedger {
    facts: OrmStore<TaskFact>,
}

/// A bounded reconciliation page. Advance even when `ends` is empty: acknowledged
/// rows still consume scan space. A new reconciliation starts with no cursor.
pub struct PendingPage {
    pub ends: Vec<(Uuid, TaskEnd)>,
    pub after: Option<Uuid>,
    pub exhausted: bool,
}

impl TaskLedger {
    pub async fn new(adapter: Arc<dyn StorageAdapter>) -> Result<Self, String> {
        Ok(Self {
            facts: OrmStore::new(adapter).await.map_err(|e| e.to_string())?,
        })
    }

    pub async fn accept(
        &self,
        owner: Uuid,
        command: String,
        execution: TaskExecution,
    ) -> Result<Uuid, String> {
        let task = Uuid::new_v4();
        let acceptance = TaskFact {
            task,
            owner,
            fact: Fact::Accepted { command, execution },
        };
        match self.facts.save(task, &acceptance).await {
            Ok(()) => Ok(task),
            Err(write_error) => {
                // A failed acknowledgement does not prove the write failed.
                // Resolve this exact attempted acceptance, without a second
                // insert or execution owner. Only an identical durable fact
                // authorizes execution; absence or read failure stays uncertain.
                match self.facts.find_by_id(task).await {
                    Ok(Some(stored)) if stored == acceptance => Ok(task),
                    Ok(_) => Err(write_error.to_string()),
                    Err(read_error) => Err(format!(
                        "acceptance write failed: {write_error}; readback failed: {read_error}"
                    )),
                }
            }
        }
    }

    /// Recover the exact execution owner without guessing from a forgotten handle
    /// or a process snapshot. Only the recipient may inspect this registration.
    pub async fn execution(&self, task: Uuid, owner: Uuid) -> Result<TaskExecution, FinishError> {
        match self
            .facts
            .find_by_id(task)
            .await
            .map_err(|e| FinishError::Persistence(e.to_string()))?
        {
            Some(TaskFact {
                task: stored_task,
                owner: stored,
                fact: Fact::Accepted { execution, .. },
            }) if stored == owner && stored_task == task => Ok(execution),
            _ => Err(FinishError::Authorization(
                "task is absent or belongs to another owner".into(),
            )),
        }
    }

    async fn authorize(&self, task: Uuid, owner: Uuid) -> Result<(), String> {
        self.execution(task, owner)
            .await
            .map(|_| ())
            .map_err(String::from)
    }

    /// Only a supervisor that has observed execution ending calls this. A cancellation
    /// request alone is not an end. Competing endings return the durable winner.
    pub async fn finish(
        &self,
        task: Uuid,
        owner: Uuid,
        execution: TaskExecution,
        end: TaskEnd,
    ) -> Result<TaskEnd, FinishError> {
        let accepted = self.execution(task, owner).await?;
        if accepted != execution {
            return Err(FinishError::ExecutionMismatch {
                task,
                accepted,
                offered: execution,
            });
        }
        let id = Uuid::new_v5(&task, b"terminal");
        let row = TaskFact {
            task,
            owner,
            fact: Fact::Ended(end.clone()),
        };
        match self.facts.save(id, &row).await {
            Ok(()) => Ok(end),
            Err(write_error) => match self
                .facts
                .find_by_id(id)
                .await
                .map_err(|e| FinishError::Persistence(e.to_string()))?
            {
                Some(TaskFact {
                    task: stored_task,
                    owner: stored_owner,
                    fact: Fact::Ended(winner),
                }) if stored_task == task && stored_owner == owner => Ok(winner),
                // Never reinterpret an arbitrary store failure as a successful completion.
                _ => Err(FinishError::Persistence(write_error.to_string())),
            },
        }
    }

    pub async fn pending(&self, task: Uuid, owner: Uuid) -> Result<Option<TaskEnd>, String> {
        self.authorize(task, owner).await?;
        let delivered = self
            .facts
            .find_by_id(Uuid::new_v5(&task, b"delivered"))
            .await
            .map_err(|e| e.to_string())?;
        match delivered {
            Some(TaskFact {
                task: stored_task,
                owner: stored_owner,
                fact: Fact::Delivered,
            }) if stored_task == task && stored_owner == owner => return Ok(None),
            Some(_) => return Err("task delivery fact has an invalid identity or shape".into()),
            None => {}
        }
        self.read_terminal(task, owner).await
    }

    /// Resolve the durable terminal even after delivery acknowledgement. Resident
    /// checkpoints carry references; the ledger remains the one output owner.
    pub async fn terminal(&self, task: Uuid, owner: Uuid) -> Result<Option<TaskEnd>, String> {
        self.authorize(task, owner).await?;
        self.read_terminal(task, owner).await
    }

    async fn read_terminal(&self, task: Uuid, owner: Uuid) -> Result<Option<TaskEnd>, String> {
        match self
            .facts
            .find_by_id(Uuid::new_v5(&task, b"terminal"))
            .await
            .map_err(|e| e.to_string())?
        {
            Some(TaskFact {
                task: stored_task,
                owner: stored_owner,
                fact: Fact::Ended(end),
            }) if stored_task == task && stored_owner == owner => Ok(Some(end)),
            None => Ok(None),
            _ => Err("task terminal fact has an invalid identity or shape".into()),
        }
    }

    /// Called only after the sink durably consumes the terminal fact. A repeated delivery
    /// uses the same task id; the sink must deduplicate before acknowledging here.
    pub async fn acknowledge(&self, task: Uuid, owner: Uuid) -> Result<(), String> {
        self.authorize(task, owner).await?;
        let terminal = self
            .facts
            .find_by_id(Uuid::new_v5(&task, b"terminal"))
            .await
            .map_err(|e| e.to_string())?;
        if !matches!(
            terminal,
            Some(TaskFact {
                task: stored_task, owner: stored_owner, fact: Fact::Ended(_),
            }) if stored_task == task && stored_owner == owner
        ) {
            return Err("cannot acknowledge a task without a terminal fact".into());
        }
        let id = Uuid::new_v5(&task, b"delivered");
        let row = TaskFact {
            task,
            owner,
            fact: Fact::Delivered,
        };
        match self.facts.save(id, &row).await {
            Ok(()) => Ok(()),
            Err(error) => match self.facts.find_by_id(id).await.map_err(|e| e.to_string())? {
                Some(stored) if stored == row => Ok(()),
                _ => Err(error.to_string()),
            },
        }
    }

    /// Reconstruct delivery from durable ownership, never from remembered handles.
    /// This scan is not a live subscription: concurrent inserts are picked up by the
    /// next sweep. Notification triggers and boot recovery both start a fresh sweep.
    pub async fn pending_page(
        &self,
        owner: Uuid,
        after: Option<Uuid>,
        limit: NonZeroUsize,
    ) -> Result<PendingPage, String> {
        let mut query = QueryBuilder::new(TaskFact::COLLECTION)
            .filter_eq("owner", owner.to_string())
            .sort_asc("id")
            .limit(limit.get());
        if let Some(cursor) = after {
            query = query.filter("id", QueryOperator::Gt(Value::String(cursor.to_string())));
        }
        let rows = self
            .facts
            .find_matching(query.build())
            .await
            .map_err(|e| e.to_string())?;
        let exhausted = rows.len() < limit.get();
        let next = rows.last().map(|(id, _)| *id);
        let mut ends = Vec::new();
        for (id, row) in rows {
            if row.owner != owner {
                return Err("task owner query returned another owner".into());
            }
            if matches!(row.fact, Fact::Accepted { .. }) {
                if id != row.task {
                    return Err("task acceptance has an invalid identity".into());
                }
                if let Some(end) = self.pending(row.task, owner).await? {
                    ends.push((row.task, end));
                }
            }
        }
        Ok(PendingPage {
            ends,
            after: next,
            exhausted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn accepted_execution_survives_reopen_and_is_not_replaced_by_new_executor(
    ) -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let ledger = TaskLedger::new(adapter.clone()).await?;
        let owner = Uuid::new_v4();
        let execution = TaskExecution::new_command_executor();
        let task = ledger.accept(owner, "code/shell".into(), execution).await?;
        drop(ledger);
        let replacement = TaskExecution::new_command_executor();
        assert_ne!(execution, replacement);
        let reopened = TaskLedger::new(adapter).await?;
        assert_eq!(reopened.execution(task, owner).await?, execution);
        assert!(reopened.execution(task, Uuid::new_v4()).await.is_err());
        assert_eq!(reopened.pending(task, owner).await?, None);
        // An old incarnation is unresolved, not automatically Lost at reopen.
        assert_eq!(reopened.terminal(task, owner).await?, None);
        assert_eq!(
            reopened
                .finish(task, owner, replacement, TaskEnd::Lost)
                .await,
            Err(FinishError::ExecutionMismatch {
                task,
                accepted: execution,
                offered: replacement
            })
        );
        assert_eq!(reopened.terminal(task, owner).await?, None);
        // Late evidence from the accepted incarnation remains valid after reopen.
        assert_eq!(
            reopened
                .finish(task, owner, execution, TaskEnd::Stopped)
                .await?,
            TaskEnd::Stopped
        );
        // Even after a terminal exists, a replacement cannot claim its completion.
        assert_eq!(
            reopened
                .finish(task, owner, replacement, TaskEnd::Lost)
                .await,
            Err(FinishError::ExecutionMismatch {
                task,
                accepted: execution,
                offered: replacement
            })
        );
        assert_eq!(
            reopened.terminal(task, owner).await?,
            Some(TaskEnd::Stopped)
        );
        Ok(())
    }

    #[tokio::test]
    async fn concurrent_ends_survive_reopen_and_delivery_retry() -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let first = TaskLedger::new(adapter.clone()).await?;
        let second = TaskLedger::new(adapter.clone()).await?;
        let owner = Uuid::new_v4();
        let task = first
            .accept(
                owner,
                "code/shell".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        assert!(first.acknowledge(task, owner).await.is_err());
        let (a, b) = tokio::join!(
            first.finish(
                task,
                owner,
                first.execution(task, owner).await?,
                TaskEnd::Done(TaskOutput::Json(Value::Null))
            ),
            second.finish(
                task,
                owner,
                second.execution(task, owner).await?,
                TaskEnd::Stopped
            ),
        );
        let winner = a?;
        assert_eq!(winner, b?);
        drop(first);
        drop(second);
        let reopened = TaskLedger::new(adapter).await?;
        assert_eq!(reopened.pending(task, owner).await?, Some(winner.clone()));
        let page = reopened
            .pending_page(owner, None, NonZeroUsize::new(10).ok_or("test page size")?)
            .await?;
        assert_eq!(page.ends, vec![(task, winner.clone())]);
        assert_eq!(
            reopened
                .finish(
                    task,
                    owner,
                    reopened.execution(task, owner).await?,
                    TaskEnd::Lost
                )
                .await?,
            winner
        );
        assert!(reopened.pending(task, Uuid::new_v4()).await.is_err());
        reopened.acknowledge(task, owner).await?;
        reopened.acknowledge(task, owner).await?;
        assert_eq!(reopened.pending(task, owner).await?, None);
        Ok(())
    }

    #[tokio::test]
    async fn corrupt_delivery_cannot_hide_an_unconsumed_end() -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let ledger = TaskLedger::new(adapter).await?;
        let owner = Uuid::new_v4();
        let task = ledger
            .accept(
                owner,
                "code/shell".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        ledger
            .finish(
                task,
                owner,
                ledger.execution(task, owner).await?,
                TaskEnd::Stopped,
            )
            .await?;
        ledger
            .facts
            .save(
                Uuid::new_v5(&task, b"delivered"),
                &TaskFact {
                    task,
                    owner: Uuid::new_v4(),
                    fact: Fact::Delivered,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        assert!(ledger.pending(task, owner).await.is_err());
        assert!(ledger.acknowledge(task, owner).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn replay_pages_cross_empty_pages_without_leaking_other_owners() -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let ledger = TaskLedger::new(adapter).await?;
        let owner = Uuid::new_v4();
        let foreign = Uuid::new_v4();
        let consumed = ledger
            .accept(
                owner,
                "consumed".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        ledger
            .finish(
                consumed,
                owner,
                ledger.execution(consumed, owner).await?,
                TaskEnd::Stopped,
            )
            .await?;
        ledger.acknowledge(consumed, owner).await?;
        let pending = ledger
            .accept(
                owner,
                "pending".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        ledger
            .finish(
                pending,
                owner,
                ledger.execution(pending, owner).await?,
                TaskEnd::Done(TaskOutput::Json(Value::Null)),
            )
            .await?;
        let other = ledger
            .accept(
                foreign,
                "other".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        ledger
            .finish(
                other,
                foreign,
                ledger.execution(other, foreign).await?,
                TaskEnd::Stopped,
            )
            .await?;
        let mut after = None;
        let mut found = Vec::new();
        let mut exhausted = false;
        // Five facts for this owner, plus the empty last page. This bound also
        // catches a cursor that fails to advance on an empty-result page.
        for _ in 0..7 {
            let page = ledger.pending_page(owner, after, NonZeroUsize::MIN).await?;
            found.extend(page.ends);
            if page.exhausted {
                exhausted = true;
                break;
            }
            assert_ne!(page.after, after);
            after = page.after;
        }
        assert!(exhausted);
        assert_eq!(
            found,
            vec![(pending, TaskEnd::Done(TaskOutput::Json(Value::Null)))]
        );
        Ok(())
    }
    use crate::orm::gated_adapter::GatedAdapter;

    #[tokio::test]
    async fn unreadable_acceptance_is_not_an_authorization_refusal() -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let gated = Arc::new(GatedAdapter::new(adapter));
        let ledger = TaskLedger::new(gated.clone()).await?;
        let owner = Uuid::new_v4();
        let execution = TaskExecution::new_command_executor();
        let task = ledger.accept(owner, "code/shell".into(), execution).await?;
        gated.fail_next_query();
        assert!(matches!(
            ledger
                .finish(task, owner, execution, TaskEnd::Stopped)
                .await,
            Err(FinishError::Persistence(_))
        ));
        assert_eq!(ledger.terminal(task, owner).await?, None);
        assert!(matches!(
            ledger
                .finish(task, Uuid::new_v4(), execution, TaskEnd::Stopped)
                .await,
            Err(FinishError::Authorization(_))
        ));
        assert_eq!(
            ledger
                .finish(task, owner, execution, TaskEnd::Stopped)
                .await?,
            TaskEnd::Stopped
        );
        Ok(())
    }

    #[tokio::test]
    async fn lost_terminal_and_delivery_replies_survive_reopen_without_duplicate_facts(
    ) -> Result<(), String> {
        use crate::orm::adapter::StorageAdapter;
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let adapter = Arc::new(GatedAdapter::new(adapter));
        let ledger = TaskLedger::new(adapter.clone()).await?;
        let owner = Uuid::new_v4();
        let task = ledger
            .accept(
                owner,
                "test/reply-loss".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        adapter.fail_after_next_create();
        assert_eq!(
            ledger
                .finish(
                    task,
                    owner,
                    ledger.execution(task, owner).await?,
                    TaskEnd::Stopped
                )
                .await?,
            TaskEnd::Stopped
        );
        drop(ledger);
        let reopened = TaskLedger::new(adapter.clone()).await?;
        assert_eq!(reopened.pending(task, owner).await?, Some(TaskEnd::Stopped));
        adapter.fail_after_next_create();
        reopened.acknowledge(task, owner).await?;
        drop(reopened);
        let reopened = TaskLedger::new(adapter.clone()).await?;
        assert_eq!(reopened.pending(task, owner).await?, None);
        assert_eq!(
            reopened
                .finish(
                    task,
                    owner,
                    reopened.execution(task, owner).await?,
                    TaskEnd::Lost
                )
                .await?,
            TaskEnd::Stopped
        );
        reopened.acknowledge(task, owner).await?;
        assert_eq!(reopened.pending(task, owner).await?, None);
        let count = adapter
            .count(QueryBuilder::new(TaskFact::COLLECTION).build())
            .await;
        assert!(count.success);
        assert_eq!(
            count.data,
            Some(3),
            "one acceptance, one end, one delivery fact"
        );
        Ok(())
    }

    async fn notified(signal: &tokio::sync::Notify) -> Result<(), String> {
        tokio::time::timeout(std::time::Duration::from_secs(5), signal.notified())
            .await
            .map_err(|error| error.to_string())
    }

    #[tokio::test]
    async fn cancelled_terminal_write_cannot_replace_winner_or_resurrect_delivery(
    ) -> Result<(), String> {
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let gated = Arc::new(GatedAdapter::new(adapter.clone()));
        let delayed = Arc::new(TaskLedger::new(gated.clone()).await?);
        let current = TaskLedger::new(adapter.clone()).await?;
        let owner = Uuid::new_v4();
        let task = current
            .accept(
                owner,
                "code/shell".into(),
                TaskExecution::new_command_executor(),
            )
            .await?;
        let pause = gated.pause_next_write();
        let writer = tokio::spawn(async move {
            delayed
                .finish(
                    task,
                    owner,
                    delayed.execution(task, owner).await?,
                    TaskEnd::Lost,
                )
                .await
        });
        notified(&pause.entered).await?;
        writer.abort();
        assert!(matches!(writer.await, Err(error) if error.is_cancelled()));

        // The competing supervisor observed the actual execution ending. Its result
        // reaches the sink while the abandoned storage operation is still pending.
        let winner = TaskEnd::Done(TaskOutput::Json(serde_json::json!({"saved": true})));
        assert_eq!(
            current
                .finish(
                    task,
                    owner,
                    current.execution(task, owner).await?,
                    winner.clone()
                )
                .await?,
            winner
        );
        assert_eq!(current.pending(task, owner).await?, Some(winner.clone()));
        current.acknowledge(task, owner).await?;
        pause.release.notify_one();
        notified(&pause.done).await?;
        drop(current);

        let reopened = TaskLedger::new(adapter).await?;
        assert_eq!(
            reopened
                .finish(
                    task,
                    owner,
                    reopened.execution(task, owner).await?,
                    TaskEnd::Lost
                )
                .await?,
            winner
        );
        assert_eq!(reopened.pending(task, owner).await?, None);
        assert!(reopened
            .pending_page(owner, None, NonZeroUsize::new(10).ok_or("test page size")?)
            .await?
            .ends
            .is_empty());
        Ok(())
    }
}
