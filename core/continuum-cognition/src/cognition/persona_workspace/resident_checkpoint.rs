//! A checkpoint receipt belongs to one captured memory incarnation, never merely a persona ID.
use super::*;

/// Capture before closing resident admission. Holding this handle preserves her memory on failure.
pub struct ResidentCheckpointTarget {
    persona: Uuid,
    memory: Arc<WorkingMemory>,
}

/// Evidence from the existing durable writer. It does not itself close mutation admission.
pub struct ResidentCheckpoint {
    target: ResidentCheckpointTarget,
    generation: u64,
}

#[derive(Debug)]
pub enum ResidentCheckpointError {
    NotResident,
    Replaced,
    Changed,
    NotWritten,
    Storage(std::io::Error),
}

impl PersonaWorkspaceRegistry {
    pub fn checkpoint_target(&self, persona: Uuid) -> Result<ResidentCheckpointTarget, ResidentCheckpointError> {
        let residents = self.cycles.lock();
        let entry = residents.get(&persona).ok_or(ResidentCheckpointError::NotResident)?;
        Ok(ResidentCheckpointTarget { persona, memory: Arc::clone(&entry.working_memory) })
    }
}

impl ResidentCheckpointTarget {
    fn verify(&self, registry: &PersonaWorkspaceRegistry) -> Result<(), ResidentCheckpointError> {
        let residents = registry.cycles.lock();
        let entry = residents.get(&self.persona).ok_or(ResidentCheckpointError::NotResident)?;
        if !Arc::ptr_eq(&entry.working_memory, &self.memory) {
            return Err(ResidentCheckpointError::Replaced);
        }
        Ok(())
    }

    /// Blocking storage seam; the lifecycle owner must first drain all writers for this resident.
    /// Saves only the captured resident through the same writer and checkpoint exclusion as periodic saves.
    pub fn flush(&self, registry: &PersonaWorkspaceRegistry) -> Result<ResidentCheckpoint, ResidentCheckpointError> {
        // Registration owns this same exclusion while restoring and replacing a memory.
        // Keep replacement out of the save without holding the hot resident lookup lock over IO.
        let _assembly = registry.assembly.lock();
        let _checkpoint = registry.checkpoint_stopped.lock();
        self.verify(registry)?;
        let saved = save_volatile(
            self.persona,
            SaveReason::Seam,
            Some(std::time::Instant::now() + crate::cognition::handoff::STAGED_READ_BOUND),
            &self.memory,
        ).map_err(ResidentCheckpointError::Storage)?;
        let Checkpointed::Written { generation, .. } = saved else {
            return Err(ResidentCheckpointError::NotWritten);
        };
        let receipt = ResidentCheckpoint {
            target: Self { persona: self.persona, memory: Arc::clone(&self.memory) },
            generation,
        };
        receipt.verify(registry)?;
        Ok(receipt)
    }
}

impl ResidentCheckpoint {
    /// Re-check immediately before retirement under the lifecycle owner's closed admission.
    /// A successor or any mutation after the captured snapshot invalidates this receipt.
    pub fn verify(&self, registry: &PersonaWorkspaceRegistry) -> Result<(), ResidentCheckpointError> {
        self.target.verify(registry)?;
        if self.target.memory.checkpoint_generation() != self.generation {
            return Err(ResidentCheckpointError::Changed);
        }
        Ok(())
    }
}
