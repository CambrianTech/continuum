//! Owner consumption state. Notification delivery is not wake admission, and
//! neither an in-memory insertion nor this snapshot alone acknowledges the ledger.
use super::WorkingMemory;
use continuum_kernel::runtime::tasks::TaskEnd;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct TaskResults(BTreeMap<Uuid, ConsumedEnd>);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ConsumedEnd {
    fingerprint: [u8; 32],
    wake_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("task {task} was already consumed with a different terminal result")]
pub struct TaskReceiptConflict {
    pub task: Uuid,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskReceiptError {
    #[error(transparent)]
    Conflict(#[from] TaskReceiptConflict),
    #[error("terminal fingerprint encoding failed: {0}")]
    Encoding(#[from] serde_json::Error),
}

impl WorkingMemory {
    /// Identity of this live memory instance; restoration creates a new incarnation.
    pub fn incarnation(&self) -> Uuid {
        self.incarnation
    }

    /// Stage a terminal reference and its wake obligation atomically. Whole output
    /// remains in TaskLedger; the fingerprint detects conflicting replay without
    /// duplicating binary/JSON bodies in every resident checkpoint. The registry must
    /// checkpoint this exact state before acknowledging consumption to the ledger.
    /// A replay is inert, including after its wake has already been admitted.
    pub fn stage_task_end(&self, task: Uuid, end: TaskEnd) -> Result<bool, TaskReceiptError> {
        let fingerprint: [u8; 32] = Sha256::digest(serde_json::to_vec(&end)?).into(); // Disk boundary: terminal integrity fingerprint persisted instead of duplicating the ledger output.
        let mut results = self.task_results.lock();
        match results.0.entry(task) {
            std::collections::btree_map::Entry::Occupied(entry) => {
                if entry.get().fingerprint != fingerprint {
                    return Err(TaskReceiptConflict { task }.into());
                }
                Ok(false)
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(ConsumedEnd {
                    fingerprint,
                    wake_pending: true,
                });
                Ok(true)
            }
        }
    }

    pub fn pending_task_wakes(&self) -> Vec<Uuid> {
        self.task_results
            .lock()
            .0
            .iter()
            .filter_map(|(task, entry)| entry.wake_pending.then_some(*task))
            .collect()
    }

    /// Called only when the owning turn admits this task's wake. Sending to a
    /// channel, reading output, and acknowledging the ledger are not admission.
    /// Returns false if no pending obligation exists; never fabricates a receipt.
    pub fn admit_task_wake(&self, task: Uuid) -> bool {
        let mut results = self.task_results.lock();
        match results.0.get_mut(&task) {
            Some(entry) if entry.wake_pending => {
                entry.wake_pending = false;
                true
            }
            Some(_) | None => false,
        }
    }
}

impl TaskResults {
    /// A checkpoint receipt must use IDs from its serialized snapshot, not from
    /// a second read of mutable working memory after IO completes.
    pub(crate) fn consumed_ids(&self) -> Vec<Uuid> {
        self.0.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgotten_task_survives_snapshot_and_replay_does_not_rearm_admitted_wake() {
        let memory = WorkingMemory::new(8);
        let task = Uuid::new_v4();
        assert!(memory
            .stage_task_end(task, TaskEnd::Stopped)
            .expect("initial end"));
        let snapshot = memory.snapshot();
        let encoded = serde_json::to_vec(&snapshot).expect("encode checkpoint");
        let restored = WorkingMemory::new(8);
        restored.restore(serde_json::from_slice(&encoded).expect("decode checkpoint"));
        assert_eq!(restored.snapshot().task_results.consumed_ids(), vec![task]);
        assert_eq!(restored.pending_task_wakes(), vec![task]);
        assert!(!restored
            .stage_task_end(task, TaskEnd::Stopped)
            .expect("identical replay"));
        assert!(restored.admit_task_wake(task));
        assert!(!restored
            .stage_task_end(task, TaskEnd::Stopped)
            .expect("replay after admission"));
        assert!(restored.pending_task_wakes().is_empty());
        let next_life = WorkingMemory::new(8);
        next_life.restore(restored.snapshot());
        assert!(next_life.pending_task_wakes().is_empty());
        assert_eq!(next_life.snapshot().task_results.consumed_ids(), vec![task]);
    }

    #[test]
    fn conflicting_end_is_refused_and_later_arrival_is_not_in_prior_snapshot() {
        let memory = WorkingMemory::new(8);
        let first = Uuid::new_v4();
        let later = Uuid::new_v4();
        memory
            .stage_task_end(first, TaskEnd::Lost)
            .expect("first end");
        let snapshot = memory.snapshot();
        memory
            .stage_task_end(later, TaskEnd::Stopped)
            .expect("later end");
        assert_eq!(snapshot.task_results.consumed_ids(), vec![first]);
        assert!(matches!(memory.stage_task_end(first, TaskEnd::Stopped),
            Err(TaskReceiptError::Conflict(TaskReceiptConflict { task })) if task == first));
        assert_eq!(memory.pending_task_wakes().len(), 2);
        assert!(!memory.admit_task_wake(Uuid::new_v4()));
    }
}
