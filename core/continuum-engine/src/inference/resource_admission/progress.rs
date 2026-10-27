//! Evidence owned by one admission gate, including the budget actually blocking a request.
use super::{LaneAdmission, LanePriority};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub(super) struct ProgressLedger {
    next: u64,
    physical_grants: u64,
    budget_grants: u64,
    entries: BTreeMap<u64, Entry>,
    ambient_age: BTreeMap<uuid::Uuid, (u64, u64)>,
    seated: BTreeSet<uuid::Uuid>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Budget,
    Physical,
    Granted,
}
#[derive(Debug, Clone, Copy)]
struct Entry {
    persona: Option<uuid::Uuid>,
    phase: Phase,
    budget: bool,
}

/// An observer never owns or recreates the semaphore acquisition.
#[derive(Clone)]
pub struct LaneWait {
    gate: &'static LaneAdmission,
    id: u64,
}
#[derive(Clone, Debug)]
pub struct LaneProgress {
    pub grants: (u64, u64),
    pub holders: Vec<(u64, uuid::Uuid)>,
}
pub(super) struct ProgressGuard {
    gate: &'static LaneAdmission,
    id: u64,
}
impl ProgressGuard {
    pub(super) fn physical(&self, budget: bool) {
        if let Some(entry) = self.gate.progress.lock().entries.get_mut(&self.id) {
            entry.phase = Phase::Physical;
            entry.budget = budget;
        }
    }
    pub(super) fn granted(&self) {
        let mut ledger = self.gate.progress.lock();
        let persona = ledger.entries.get(&self.id).and_then(|entry| entry.persona);
        if let Some(persona) = persona { ledger.ambient_age.remove(&persona); }
        let budget = if let Some(entry) = ledger.entries.get_mut(&self.id) {
            entry.phase = Phase::Granted;
            entry.budget
        } else {
            return;
        };
        ledger.physical_grants = ledger.physical_grants.wrapping_add(1);
        if budget {
            ledger.budget_grants = ledger.budget_grants.wrapping_add(1);
        }
    }
}
impl Drop for ProgressGuard {
    fn drop(&mut self) {
        self.gate.progress.lock().entries.remove(&self.id);
    }
}
impl LaneAdmission {
    /// Debt belongs to a resident's seat, not to an expiring retry or a timer.
    /// Process restart forgives this debt; S3 readmits the interrupted turn.
    pub fn resident_seated(&self, persona: uuid::Uuid) {
        self.progress.lock().seated.insert(persona);
    }

    /// Called after the resident's loop stops or its seat leaves this node.
    pub fn resident_departed(&self, persona: uuid::Uuid) {
        let mut ledger = self.progress.lock();
        ledger.seated.remove(&persona);
        ledger.ambient_age.remove(&persona);
    }

    pub(super) fn observe(
        &'static self,
        persona: Option<uuid::Uuid>,
        priority: LanePriority,
    ) -> (LaneWait, ProgressGuard) {
        let mut ledger = self.progress.lock();
        ledger.next = ledger.next.wrapping_add(1);
        let id = ledger.next;
        ledger.entries.insert(
            id,
            Entry {
                persona,
                phase: if priority == LanePriority::Directed {
                    Phase::Physical
                } else {
                    Phase::Budget
                },
                budget: false,
            },
        );
        (
            LaneWait { gate: self, id },
            ProgressGuard { gate: self, id },
        )
    }
}
impl LaneWait {
    /// Only holders of the required budget count. A budget holder itself queued
    /// for a physical slot links that dependency to the physical slot holders.
    /// Thus an unrelated busy reserved lane cannot hide a leaked work permit.
    pub fn progress(&self) -> LaneProgress {
        let ledger = self.gate.progress.lock();
        let physical = ledger
            .entries
            .get(&self.id)
            .is_some_and(|e| e.phase == Phase::Physical)
            || ledger
                .entries
                .values()
                .any(|e| e.budget && e.phase == Phase::Physical);
        let grants = (
            if physical { ledger.physical_grants } else { 0 },
            ledger.budget_grants,
        );
        let holders = ledger
            .entries
            .iter()
            .filter(|(id, e)| {
                **id != self.id && e.phase == Phase::Granted && (physical || e.budget)
            })
            .filter_map(|(id, e)| e.persona.map(|p| (*id, p)))
            .collect();
        LaneProgress { grants, holders }
    }
}

impl ProgressLedger {
    pub(super) const fn new() -> Self {
        Self {
            next: 0,
            physical_grants: 0,
            budget_grants: 0,
            entries: BTreeMap::new(),
            ambient_age: BTreeMap::new(),
            seated: BTreeSet::new(),
        }
    }
}
impl std::fmt::Debug for ProgressGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ProgressGuard").field(&self.id).finish()
    }
}

impl LaneWait {
    /// One initial cohort may pass before ambient joins the ordinary non-directed FIFO.
    /// A cancelled attempt retains its origin for this resident; an actual grant clears it.
    pub(super) fn ambient_origin(&self, cohort: u64) -> (u64, u64) {
        let mut ledger = self.gate.progress.lock();
        let now = ledger.budget_grants;
        let persona = ledger.entries.get(&self.id).and_then(|entry| entry.persona);
        match persona {
            Some(persona) if ledger.seated.contains(&persona) =>
                *ledger.ambient_age.entry(persona).or_insert((now, cohort.max(1))),
            _ => (now, cohort.max(1)),
        }
    }

    pub(super) fn ambient_aged(&self, origin: (u64, u64)) -> bool {
        self.gate.progress.lock().budget_grants.wrapping_sub(origin.0) >= origin.1
    }
}
