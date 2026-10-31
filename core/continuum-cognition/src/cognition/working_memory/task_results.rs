//! Owner consumption state. Notification delivery is not wake admission, and
//! neither an in-memory insertion nor this snapshot alone acknowledges the ledger.
use super::WorkingMemory;
use continuum_kernel::runtime::tasks::{TaskEnd, TaskEndState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct TaskResults {
    entries: BTreeMap<Uuid, ConsumedEnd>,
    // Runtime publication fence; durable restoration publishes its saved entries.
    #[serde(skip)]
    published: BTreeMap<Uuid, bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ConsumedEnd {
    fingerprint: [u8; 32],
    state: TaskEndState,
    wake_pending: bool,
}

/// A published terminal fact. The fingerprint identifies the immutable ledger
/// result, including its body; it is not merely the coarse Done/Failed status.
pub(crate) struct TaskTerminal {
    pub task: Uuid,
    pub fingerprint: [u8; 32],
    pub state: TaskEndState,
    pub wake_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("task {task} was already consumed with a different terminal result")]
pub struct TaskReceiptConflict {
    pub task: Uuid,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskReceiptError {
    #[error("task {0} has no published consumption receipt")]
    Unpublished(Uuid),
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
            if let Some((previous, _)) = incoming.insert(task, (fingerprint, end.state())) {
                if previous != fingerprint {
                    return Err(TaskReceiptConflict { task }.into());
                }
            }
        }
        let mut results = self.task_results.lock();
        for (task, (fingerprint, _)) in &incoming {
            if let Some(previous) = results.entries.get(task) {
                if previous.fingerprint != *fingerprint {
                    return Err(TaskReceiptConflict { task: *task }.into());
                }
            }
        }
        let mut changed = false;
        for (task, (fingerprint, state)) in incoming {
            if let std::collections::btree_map::Entry::Vacant(entry) = results.entries.entry(task) {
                entry.insert(ConsumedEnd {
                    fingerprint,
                    state,
                    wake_pending: true,
                });
                changed = true;
            }
        }
        Ok(changed)
    }

    /// The shared checkpoint writer calls this only after syncing its exact snapshot.
    pub(crate) fn publish_task_checkpoint(&self, saved: TaskPublication) {
        let mut results = self.task_results.lock();
        for (task, pending) in saved.0 {
            if results.entries.contains_key(&task) {
                results.published.insert(task, pending);
            }
        }
    }

    pub fn pending_task_wakes(&self) -> Vec<Uuid> {
        let results = self.task_results.lock();
        results
            .published
            .iter()
            .filter_map(|(task, pending)| pending.then_some(*task))
            .collect()
    }

    pub(crate) fn task_terminals(&self) -> Vec<TaskTerminal> {
        let results = self.task_results.lock();
        results.entries.iter().filter_map(|(task, end)| {
            results.published.get(task).map(|pending| TaskTerminal {
                task: *task,
                fingerprint: end.fingerprint,
                state: end.state,
                wake_pending: *pending,
            })
        }).collect()
    }

    /// Stage retirement only after a successful rendered-context read receipt.
    /// The wake remains published until the checkpoint containing retirement commits.
    /// Notification delivery and entering the service loop are not perception.
    /// Returns whether staging changed. False does not prove persistence: an earlier
    /// stage may still need a checkpoint retry. Unknown tasks cannot create receipts.
    pub fn stage_task_reads(&self, tasks: &[Uuid]) -> Result<bool, TaskReceiptError> {
        let mut results = self.task_results.lock();
        for task in tasks {
            if !results.published.contains_key(task) || !results.entries.contains_key(task) {
                return Err(TaskReceiptError::Unpublished(*task));
            }
        }
        let mut changed = false;
        for task in tasks {
            if let Some(entry) = results.entries.get_mut(task) {
                changed |= entry.wake_pending;
                entry.wake_pending = false;
            }
        }
        Ok(changed)
    }
}

/// Exact wake projection of one serialized snapshot. Constructed only by its owner.
pub(crate) struct TaskPublication(BTreeMap<Uuid, bool>);

impl TaskResults {
    pub(crate) fn publication(&self) -> TaskPublication {
        TaskPublication(
            self.entries
                .iter()
                .map(|(id, end)| (*id, end.wake_pending))
                .collect(),
        )
    }

    pub(crate) fn publish_restored(&mut self) {
        self.published = self.publication().0;
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
        assert!(restored.stage_task_reads(&[task]).expect("published task"));
        assert!(!restored
            .stage_task_end(task, TaskEnd::Stopped)
            .expect("replay after admission"));
        assert_eq!(
            restored.pending_task_wakes(),
            vec![task],
            "staged retirement is not a committed read receipt"
        );
        restored.publish_task_checkpoint(restored.snapshot().task_results.publication());
        assert!(restored.pending_task_wakes().is_empty());
        let next_life = WorkingMemory::new(8);
        next_life.restore(restored.snapshot());
        assert!(next_life.pending_task_wakes().is_empty());
        assert_eq!(next_life.snapshot().task_results.consumed_ids(), vec![task]);
    }

    #[test]
    fn read_batch_refuses_unknown_task_without_partial_retirement() {
        let memory = WorkingMemory::new(8);
        let task = Uuid::new_v4();
        memory.stage_task_end(task, TaskEnd::Stopped).expect("stage");
        memory.publish_task_checkpoint(memory.snapshot().task_results.publication());
        assert!(memory.stage_task_reads(&[task, Uuid::new_v4()]).is_err());
        memory.publish_task_checkpoint(memory.snapshot().task_results.publication());
        assert_eq!(memory.pending_task_wakes(), vec![task]);
    }

    #[test]
    fn older_checkpoint_cannot_publish_a_later_read() {
        let memory = WorkingMemory::new(8);
        let task = Uuid::new_v4();
        memory
            .stage_task_end(task, TaskEnd::Stopped)
            .expect("stage end");
        memory.publish_task_checkpoint(memory.snapshot().task_results.publication());
        let earlier = memory.snapshot().task_results.publication();
        assert!(memory.stage_task_reads(&[task]).expect("published task"));
        memory.publish_task_checkpoint(earlier);
        assert_eq!(memory.pending_task_wakes(), vec![task]);
        memory.publish_task_checkpoint(memory.snapshot().task_results.publication());
        assert!(memory.pending_task_wakes().is_empty());
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
        assert!(memory.stage_task_reads(&[first]).is_err());
        memory.publish_task_checkpoint(snapshot.task_results.publication());
        assert_eq!(memory.pending_task_wakes(), vec![first]);
        assert!(memory.stage_task_reads(&[later]).is_err());
        assert!(memory.stage_task_reads(&[Uuid::new_v4()]).is_err());
    }
}
