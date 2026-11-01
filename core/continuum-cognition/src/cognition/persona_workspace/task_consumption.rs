//! Durable task consumption uses the registered resident and its existing checkpoint
//! owner. No constructor-owned writer or remembered dispatch handle participates.
use super::*;
use continuum_kernel::runtime::tasks::TaskEnd;

#[derive(Debug, thiserror::Error)]
pub enum TaskConsumptionError {
    #[error(transparent)]
    Owner(#[from] OwnerCheckpointError),
    #[error("task read receipt belongs to a replaced memory incarnation for {0}")]
    Replaced(Uuid),
    #[error(transparent)]
    Receipt(#[from] super::super::working_memory::TaskReceiptError),
}

impl PersonaWorkspaceRegistry {
    /// Only a submitted terminal occurrence can retire a wake. Fold blocks and
    /// retired entries are not evidence that this end was shown. The checkpoint
    /// below rechecks incarnation under admission, so replacement during matching
    /// cannot acknowledge the next resident's memory.
    pub fn record_rendered_tasks(
        &self,
        receipt: &crate::cognition::render_receipt::RenderReceipt,
    ) -> Result<Option<VolatileReceipt>, TaskConsumptionError> {
        use continuum_context::level::Level;
        let memory = self
            .cycles
            .lock()
            .get(&receipt.owner)
            .map(|resident| Arc::clone(&resident.working_memory))
            .ok_or(OwnerCheckpointError::Absent(receipt.owner))?;
        if memory.incarnation() != receipt.incarnation {
            return Err(TaskConsumptionError::Replaced(receipt.owner));
        }
        let shown: std::collections::BTreeSet<_> = receipt
            .view
            .items
            .iter()
            .filter(|item| {
                !item.retired && matches!(item.level, Level::Full | Level::Summary | Level::Pointer)
            })
            .filter_map(|item| item.element)
            .collect();
        let tasks: Vec<_> = memory
            .task_terminals()
            .into_iter()
            .filter(|end| end.wake_pending)
            .filter(|end| {
                shown.contains(&crate::cognition::context_streams::tasks::terminal_element(
                    receipt.owner,
                    end.task,
                    &end.fingerprint,
                ))
            })
            .map(|end| end.task)
            .collect();
        if tasks.is_empty() {
            return Ok(None);
        }
        self.record_task_reads(receipt.owner, receipt.incarnation, &tasks)
            .map(Some)
    }

    /// Success receipts exactly the snapshot containing this end and its wake
    /// obligation. A replay also checkpoints: a prior insertion may have failed IO.
    /// Registration/replacement is excluded until the snapshot is committed; the
    /// next incarnation restores it through the existing registration path.
    pub fn consume_task_ends(
        &self,
        owner: Uuid,
        ends: Vec<(Uuid, TaskEnd)>,
    ) -> Result<VolatileReceipt, TaskConsumptionError> {
        self.commit_owner_state(owner, |memory| {
            memory.stage_task_ends(ends)?;
            Ok(())
        })
    }

    /// Commit only reads reported by this registered memory incarnation. A repeated
    /// receipt still saves: an earlier staging may have survived a failed write.
    pub fn record_task_reads(
        &self,
        owner: Uuid,
        incarnation: Uuid,
        tasks: &[Uuid],
    ) -> Result<VolatileReceipt, TaskConsumptionError> {
        self.commit_owner_state(owner, |memory| {
            if memory.incarnation() != incarnation {
                return Err(TaskConsumptionError::Replaced(owner));
            }
            memory.stage_task_reads(tasks)?;
            Ok(())
        })
    }
}

/// Async adapter for the registered owner. Filesystem work runs on a blocking
/// worker and is awaited; a worker failure cannot be acknowledged as consumption.
pub struct ResidentTaskSink(Arc<PersonaWorkspaceRegistry>);

impl ResidentTaskSink {
    pub fn new(registry: Arc<PersonaWorkspaceRegistry>) -> Self {
        Self(registry)
    }

    /// Await the same checkpoint owner used for incoming ends. The caller keeps
    /// this persistence result separate from the generation that produced the
    /// receipt: failure here cannot turn a served generation into an inference fault.
    pub async fn record_rendered(
        &self,
        receipt: crate::cognition::render_receipt::RenderReceipt,
    ) -> Result<Option<VolatileReceipt>, TaskSinkError> {
        self.commit(move |registry| registry.record_rendered_tasks(&receipt))
            .await
    }

    async fn commit<R: Send + 'static>(
        &self,
        operation: impl FnOnce(&PersonaWorkspaceRegistry) -> Result<R, TaskConsumptionError>
            + Send
            + 'static,
    ) -> Result<R, TaskSinkError> {
        let registry = Arc::clone(&self.0);
        // Carry the existing test filesystem scope onto the blocking worker.
        #[cfg(test)]
        let test_home = crate::paths::NativeHomeOverride::current();
        Ok(tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            let _native = test_home
                .as_deref()
                .map(crate::paths::NativeHomeOverride::install);
            operation(&registry)
        })
        .await??)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TaskSinkError {
    #[error("receipt checkpoint owner {0} no longer exists")]
    OwnerGone(Uuid),
    #[error("render receipt does not belong to cycle {owner} incarnation {incarnation}")]
    CustodyMismatch { owner: Uuid, incarnation: Uuid },
    #[error(transparent)]
    Consumption(#[from] TaskConsumptionError),
    #[error("resident task checkpoint worker failed: {0}")]
    Worker(#[from] tokio::task::JoinError),
}

#[async_trait::async_trait]
impl continuum_kernel::runtime::tasks::TaskOwnerSink for ResidentTaskSink {
    type Error = TaskSinkError;

    async fn consume(
        &self,
        owner: Uuid,
        ends: &[(Uuid, TaskEnd)],
    ) -> Result<std::collections::BTreeSet<Uuid>, Self::Error> {
        let offered: std::collections::BTreeSet<_> = ends.iter().map(|(task, _)| *task).collect();
        let ends = ends.to_vec();
        let receipt = self
            .commit(move |registry| registry.consume_task_ends(owner, ends))
            .await?;
        Ok(receipt
            .perceived_tasks()
            .intersection(&offered)
            .copied()
            .collect())
    }
}
