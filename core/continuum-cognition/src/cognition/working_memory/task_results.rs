//! Owner consumption state. Notification delivery is not wake admission, and
//! neither an in-memory insertion nor this snapshot alone acknowledges the ledger.
use super::WorkingMemory;
use continuum_kernel::runtime::tasks::TaskEnd;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct TaskResults {
    entries: BTreeMap<Uuid, ConsumedEnd>,
    // Runtime publication fence; durable restoration publishes its saved entries.
    #[serde(skip)]
    eligible: BTreeSet<Uuid>,
}

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
        self.stage_task_ends(vec![(task, end)])
    }

    /// Validate a complete owner page before changing any consumption state.
    pub fn stage_task_ends(&self, ends: Vec<(Uuid, TaskEnd)>) -> Result<bool, TaskReceiptError> {
        let mut incoming = BTreeMap::new();
        for (task, end) in ends {
            let fingerprint: [u8; 32] = Sha256::digest(serde_json::to_vec(&end)?).into(); // Disk boundary: persist integrity fingerprints, not duplicate ledger output.
            if let Some(previous) = incoming.insert(task, fingerprint) {
                if previous != fingerprint {
                    return Err(TaskReceiptConflict { task }.into());
                }
            }
        }
        let mut results = self.task_results.lock();
        for (task, fingerprint) in &incoming {
            if let Some(previous) = results.entries.get(task) {
                if previous.fingerprint != *fingerprint {
                    return Err(TaskReceiptConflict { task: *task }.into());
                }
            }
        }
        let mut changed = false;
        for (task, fingerprint) in incoming {
            if let std::collections::btree_map::Entry::Vacant(entry) = results.entries.entry(task) {
                entry.insert(ConsumedEnd {
                    fingerprint,
                    wake_pending: true,
                });
                changed = true;
            }
        }
        Ok(changed)
    }

    /// The shared checkpoint writer calls this only after syncing its exact snapshot.
    pub(crate) fn publish_task_checkpoint(&self, saved: &[Uuid]) {
        let mut results = self.task_results.lock();
        for task in saved {
            if results.entries.contains_key(task) {
                results.eligible.insert(*task);
            }
        }
    }

    pub fn pending_task_wakes(&self) -> Vec<Uuid> {
        let results = self.task_results.lock();
        results
            .entries
            .iter()
            .filter_map(|(task, entry)| {
                (entry.wake_pending && results.eligible.contains(task)).then_some(*task)
            })
            .collect()
    }

    /// Called only when the owning turn admits this task's wake. Sending to a
    /// channel, reading output, and acknowledging the ledger are not admission.
    /// Returns false if no pending obligation exists; never fabricates a receipt.
    pub fn admit_task_wake(&self, task: Uuid) -> bool {
        let mut results = self.task_results.lock();
        if !results.eligible.contains(&task) {
            return false;
        }
        match results.entries.get_mut(&task) {
            Some(entry) if entry.wake_pending => {
                entry.wake_pending = false;
                true
            }
            Some(_) | None => false,
        }
    }
}

impl TaskResults {
    pub(crate) fn publish_restored(&mut self) {
        self.eligible = self.entries.keys().copied().collect();
    }

    /// A checkpoint receipt must use IDs from its serialized snapshot, not from
    /// a second read of mutable working memory after IO completes.
    pub(crate) fn consumed_ids(&self) -> Vec<Uuid> {
        self.entries.keys().copied().collect()
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
    fn conflicting_batch_stages_nothing() {
        let memory = WorkingMemory::new(8);
        let existing = Uuid::new_v4();
        let new = Uuid::new_v4();
        memory
            .stage_task_end(existing, TaskEnd::Lost)
            .expect("existing");
        assert!(memory
            .stage_task_ends(vec![(new, TaskEnd::Stopped), (existing, TaskEnd::Stopped)])
            .is_err());
        assert_eq!(
            memory.snapshot().task_results.consumed_ids(),
            vec![existing]
        );
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
        assert!(memory.pending_task_wakes().is_empty());
        assert!(!memory.admit_task_wake(first));
        memory.publish_task_checkpoint(&snapshot.task_results.consumed_ids());
        assert_eq!(memory.pending_task_wakes(), vec![first]);
        assert!(!memory.admit_task_wake(later));
        assert!(!memory.admit_task_wake(Uuid::new_v4()));
    }
}
