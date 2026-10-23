//! Generations this seat serves concurrently for other nodes (card b3e5b582, slice 2a).
//!
//! The command pump used to await each served `ai/generate` inline, so one generation
//! holding a lane for minutes stopped the seat from reading anything else: a 2-lane
//! seat answered remote requesters one at a time (IntelMac -> M5, 2026-10-23). The
//! handler now admits a generation here, runs it as its own task, and returns.
//!
//! Custody is explicit and bounded: a generation is admitted only while fewer than the
//! seat's live served-lane count are outstanding (each waiting generation retains its
//! whole request), a refused one is answered at once with a typed overload error, and
//! dropping the registry (the pump stops) aborts every task still running. The registry
//! is keyed by correlation, the hook slice 2b's requester cancel uses.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use tokio::task::AbortHandle;
use uuid::Uuid;

/// Outstanding served generations, keyed by request correlation.
#[derive(Default)]
pub(super) struct ServedGenerations {
    /// `None` between admission and spawn; the task's abort handle after.
    outstanding: Mutex<HashMap<Uuid, Option<AbortHandle>>>,
}

/// Why a generation was not admitted. Rendered into the requester's error reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Refusal {
    /// Every admissible slot is held by a generation still being served.
    Overloaded { outstanding: usize, bound: usize },
    /// A generation with this correlation is already being served.
    Duplicate { correlation: Uuid },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overloaded { outstanding, bound } => write!(
                f,
                "seat overloaded: {outstanding} generations outstanding for {bound} served lanes; \
                 retry elsewhere or later"
            ),
            Self::Duplicate { correlation } => {
                write!(f, "generation {correlation} is already being served here")
            }
        }
    }
}

/// One admitted generation's hold on its slot. Dropping it (the task ends, by any path)
/// releases the slot. It holds the registry weakly, so it never keeps a stopped pump's
/// registry alive.
pub(super) struct Admission {
    registry: Weak<ServedGenerations>,
    correlation: Uuid,
}

impl Drop for Admission {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.outstanding.lock().remove(&self.correlation);
        }
    }
}

impl ServedGenerations {
    /// Admit `correlation` if fewer than `bound` generations are outstanding.
    pub(super) fn admit(
        self: &Arc<Self>,
        correlation: Uuid,
        bound: usize,
    ) -> Result<Admission, Refusal> {
        let mut outstanding = self.outstanding.lock();
        if outstanding.contains_key(&correlation) {
            return Err(Refusal::Duplicate { correlation });
        }
        if outstanding.len() >= bound {
            return Err(Refusal::Overloaded {
                outstanding: outstanding.len(),
                bound,
            });
        }
        outstanding.insert(correlation, None);
        Ok(Admission {
            registry: Arc::downgrade(self),
            correlation,
        })
    }

    /// Record the spawned task for an admitted generation, so a stop can abort it. A task
    /// that already finished (its admission released the entry) is not re-inserted.
    pub(super) fn attach(&self, correlation: Uuid, task: AbortHandle) {
        if let Some(slot) = self.outstanding.lock().get_mut(&correlation) {
            *slot = Some(task);
        }
    }

    pub(super) fn outstanding(&self) -> usize {
        self.outstanding.lock().len()
    }
}

impl Drop for ServedGenerations {
    /// The pump stopped: every generation it was serving is aborted, not orphaned.
    fn drop(&mut self) {
        for task in self.outstanding.get_mut().values().flatten() {
            task.abort();
        }
    }
}

/// The seat's admission bound for served generations: its live served-lane count, the
/// same number the serving gate is sized to. Before the first serving plan that count is
/// 0; one outstanding generation is what the inline pump always allowed, so it is the
/// floor, never less.
// derived-or-floor: the live lane count, floored at the inline pump's prior capacity of 1.
pub(super) fn admission_bound() -> usize {
    crate::inference::resource_admission::served_lane_count().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: custody is bounded. A burst while every slot is held is refused
    // with the counts (not queued in unbounded tasks), a duplicate correlation is refused,
    // and a finished generation frees its slot.
    #[test]
    fn admission_is_bounded_and_a_finished_generation_frees_its_slot() {
        let registry = Arc::new(ServedGenerations::default());
        let first = registry
            .admit(Uuid::from_u128(1), 2)
            .expect("first admitted");
        let _second = registry
            .admit(Uuid::from_u128(2), 2)
            .expect("second admitted");
        assert_eq!(
            registry.admit(Uuid::from_u128(3), 2).err(),
            Some(Refusal::Overloaded {
                outstanding: 2,
                bound: 2
            })
        );
        assert_eq!(
            registry.admit(Uuid::from_u128(2), 3).err(),
            Some(Refusal::Duplicate {
                correlation: Uuid::from_u128(2)
            })
        );
        drop(first);
        assert_eq!(registry.outstanding(), 1);
        assert!(
            registry.admit(Uuid::from_u128(3), 2).is_ok(),
            "the freed slot is admissible"
        );
    }

    // what this catches: a stopped pump does not orphan the generations it was serving.
    // Dropping the registry aborts every attached task; the tasks hold it only weakly.
    #[tokio::test]
    async fn dropping_the_registry_aborts_every_outstanding_generation() {
        let registry = Arc::new(ServedGenerations::default());
        let correlation = Uuid::from_u128(7);
        let admission = registry.admit(correlation, 1).expect("admitted");
        let task = tokio::spawn(async move {
            let _admission = admission;
            std::future::pending::<()>().await;
        });
        registry.attach(correlation, task.abort_handle());
        drop(registry);
        let joined = tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .expect("the aborted task ends promptly");
        assert!(
            joined.is_err_and(|e| e.is_cancelled()),
            "the task was cancelled, not completed"
        );
    }
}
