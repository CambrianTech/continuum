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
