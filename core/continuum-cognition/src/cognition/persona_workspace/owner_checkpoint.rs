//! One resident-state commit boundary, shared by task delivery and perception.
//! The payload owner stages typed state; this owner supplies admission, replacement
//! exclusion, the existing checkpoint writer and publication after persistence.
use super::*;

#[derive(Debug, thiserror::Error)]
pub enum OwnerCheckpointError {
    #[error("resident checkpoint admission is closed")]
    Stopped,
    #[error("owner {0} is not a registered resident")]
    Absent(Uuid),
    #[error("owner checkpoint failed: {0}")]
    Checkpoint(#[from] std::io::Error),
}

impl PersonaWorkspaceRegistry {
    /// Commit through the registered resident's existing snapshot and writer.
    /// A stage must validate its complete input before mutation. An IO failure may
    /// leave staged memory, so replay must be idempotent and must checkpoint again.
    /// Staging alone never authorizes an acknowledgement or publishes a wake.
    ///
    /// Payloads, source cursors and wake obligations must all be part of this same
    /// saved snapshot. This boundary cannot make an external/in-memory queue durable.
    /// Incarnation-sensitive callers validate their expected incarnation in `stage`,
    /// while replacement is excluded. Do not call registry operations from `stage`.
    pub fn commit_owner_state<E: From<OwnerCheckpointError>>(
        &self,
        owner: Uuid,
        stage: impl FnOnce(&WorkingMemory) -> Result<(), E>,
    ) -> Result<VolatileReceipt, E> {
        let stopped = self.checkpoint_stopped.lock();
        if *stopped {
            return Err(OwnerCheckpointError::Stopped.into());
        }
        let _assembly = self.assembly.lock();
        let memory = self
            .cycles
            .lock()
            .get(&owner)
            .map(|resident| Arc::clone(&resident.working_memory))
            .ok_or_else(|| E::from(OwnerCheckpointError::Absent(owner)))?;
        stage(&memory)?;
        save_volatile(owner, SaveReason::Periodic, None, &memory)
            .map_err(|error| E::from(OwnerCheckpointError::Checkpoint(error)))
    }
}
