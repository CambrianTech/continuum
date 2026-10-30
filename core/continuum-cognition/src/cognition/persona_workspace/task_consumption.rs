//! Durable task consumption uses the registered resident and its existing checkpoint
//! owner. No constructor-owned writer or remembered dispatch handle participates.
use super::*;
use continuum_kernel::runtime::tasks::TaskEnd;

#[derive(Debug, thiserror::Error)]
pub enum TaskConsumptionError {
    #[error("resident checkpoint admission is closed")]
    Stopped,
    #[error("task owner {0} is not a registered resident")]
    Absent(Uuid),
    #[error(transparent)]
    Receipt(#[from] super::super::working_memory::TaskReceiptError),
    #[error("task consumption checkpoint failed: {0}")]
    Checkpoint(#[from] std::io::Error),
}

impl PersonaWorkspaceRegistry {
    /// Success receipts exactly the snapshot containing this end and its wake
    /// obligation. A replay also checkpoints: a prior insertion may have failed IO.
    /// Registration/replacement is excluded until the snapshot is committed; the
    /// next incarnation restores it through the existing registration path.
    pub fn consume_task_end(
        &self,
        owner: Uuid,
        task: Uuid,
        end: TaskEnd,
    ) -> Result<VolatileReceipt, TaskConsumptionError> {
        let stopped = self.checkpoint_stopped.lock();
        if *stopped {
            return Err(TaskConsumptionError::Stopped);
        }
        let _assembly = self.assembly.lock();
        let memory = self
            .cycles
            .lock()
            .get(&owner)
            .map(|resident| Arc::clone(&resident.working_memory))
            .ok_or(TaskConsumptionError::Absent(owner))?;
        memory.stage_task_end(task, end)?;
        Ok(save_volatile(owner, SaveReason::Periodic, None, &memory)?)
    }
}

/// Async adapter for the registered owner. Filesystem work runs on a blocking
/// worker and is awaited; a worker failure cannot be acknowledged as consumption.
pub struct ResidentTaskSink(Arc<PersonaWorkspaceRegistry>);

impl ResidentTaskSink {
    pub fn new(registry: Arc<PersonaWorkspaceRegistry>) -> Self {
        Self(registry)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TaskSinkError {
    #[error(transparent)]
    Consumption(#[from] TaskConsumptionError),
    #[error("resident task checkpoint worker failed: {0}")]
    Worker(#[from] tokio::task::JoinError),
}

#[async_trait::async_trait]
impl continuum_kernel::runtime::tasks::TaskOwnerSink for ResidentTaskSink {
    type Error = TaskSinkError;

    async fn consume(&self, owner: Uuid, task: Uuid, end: &TaskEnd) -> Result<(), Self::Error> {
        let registry = Arc::clone(&self.0);
        let end = end.clone();
        // Carry the existing test filesystem scope onto the blocking worker.
        #[cfg(test)]
        let test_home = crate::paths::NativeHomeOverride::current();
        tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            let _native = test_home
                .as_deref()
                .map(crate::paths::NativeHomeOverride::install);
            registry.consume_task_end(owner, task, end)
        })
        .await??;
        Ok(())
    }
}
