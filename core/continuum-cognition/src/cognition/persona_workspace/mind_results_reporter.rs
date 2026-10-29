//! The one memory owner's view of every mind's resident result bodies.
//!
//! A result's body stays in RAM until the memory pressure monitor says shed: RAM decides, not
//! a number (Joel, 2026-10-29; it replaced `RESULTS_RESIDENT_CHARS = 1 << 20`, a cap per mind
//! at receipt). This reporter is the only thing that moves a body to its handle in production
//! (`WorkingMemory::evict_resident_results_over`). It sheds oldest-first across every resident
//! mind, public and private, and never moves the newest result of a mind or a body whose saved
//! output is not there to reload (the last copy).
//!
//! Accounting is BYTES of resident bodies (`String::len`), never characters: characters are
//! the view's price (`stored_chars`). Each record owns its body (no shared backing), so the sum
//! over records is the RAM they hold; a working memory reached twice is counted once.
//!
//! No filesystem access under a mind's lock: her candidates are read under it, each saved
//! output is checked after it drops, and the eviction moves only the `(seq, handle)` pairs that
//! check vouched for.
//!
//! Known limit: the shed target is the host's RAM overrun past this priority's gate, so a
//! level raised by swap activity alone, with RAM use under the gate, sheds nothing here.

use std::path::PathBuf;
use std::sync::Arc;

use continuum_kernel::system_resources::{
    MemoryBudgetSpec, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
    PressureSnapshot,
};
use tokio::sync::watch;
use uuid::Uuid;

use super::PersonaWorkspaceRegistry;
use crate::cognition::working_memory::{ResidentResults, WorkingMemory};

const NAME: &str = "mind_results";

/// Her results are working state for her current task ("active persona state"), not an idle
/// cache: `Interactive`. Its gate (0.80) is where the monitor's Warning starts, so a shed gives
/// up only the overrun past it, not every movable body at the first Warning as `Background`'s
/// 0.60 gate would.
const PRIORITY: MemoryPriority = MemoryPriority::Interactive;

/// Why a body stays resident through a shed: it is the last copy of what she read.
#[derive(Debug)]
pub(crate) enum LastCopy {
    /// The handle does not name an output this mind can resolve.
    Unresolvable(std::io::Error),
    /// The handle resolves, but its saved output is not there to reload.
    Missing(PathBuf),
}

impl std::fmt::Display for LastCopy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unresolvable(error) => write!(f, "handle does not resolve: {error}"),
            Self::Missing(path) => write!(f, "saved output missing at {}", path.display()),
        }
    }
}

/// Whether `persona`'s saved output for `handle` is there to reload, so its body may leave RAM.
pub(crate) fn saved_output(persona: Uuid, handle: &str) -> Result<(), LastCopy> {
    let path = crate::cognition::tool_executor::spill::resolve(persona, handle)
        .map_err(LastCopy::Unresolvable)?;
    if path.is_file() {
        Ok(())
    } else {
        Err(LastCopy::Missing(path))
    }
}

/// The resident minds the reporter accounts: `(persona, working memory)`, each memory once.
pub(crate) type Minds = Vec<(Uuid, Arc<WorkingMemory>)>;

impl PersonaWorkspaceRegistry {
    /// Every resident mind's working memory: her live cycle's and her private mind cycle's.
    /// Each lock is taken and released on its own, never nested.
    pub(crate) fn resident_working_memories(&self) -> Minds {
        let mut minds: Minds = self
            .cycles
            .lock()
            .iter()
            .map(|(id, entry)| (*id, Arc::clone(&entry.working_memory)))
            .collect();
        minds.extend(
            self.mind_cycles
                .lock()
                .iter()
                .map(|(id, entry)| (*id, Arc::clone(&entry.working_memory))),
        );
        let mut seen = std::collections::HashSet::new();
        minds.retain(|(_, memory)| seen.insert(Arc::as_ptr(memory)));
        minds
    }
}

/// The bytes every mind's resident result bodies hold.
pub(crate) fn resident_bytes(minds: &Minds) -> u64 {
    minds
        .iter()
        .map(|(_, memory)| memory.resident_result_bytes())
        .sum()
}

/// What this consumer gives up: how far the host's used RAM stands past the line the monitor
/// declares this priority sheds at (`MemoryPriority::pressure_gate`). Both numbers are the
/// monitor's own; nothing here is a ratio of ours. `0` before the first poll (no total yet).
pub(crate) fn over_gate(snapshot: &PressureSnapshot) -> u64 {
    let used = snapshot
        .total_bytes
        .saturating_sub(snapshot.available_bytes);
    let gate = (snapshot.total_bytes as f64 * PRIORITY.pressure_gate()) as u64;
    used.saturating_sub(gate)
}

/// One mind's resident result bodies as the memory owner accounts them: what a shed may move
/// (its saved output vouched for, outside her lock) and what it may not.
#[derive(Debug, Clone)]
pub(crate) struct Residency {
    pub resident_bytes: u64,
    /// The newest result and every last copy: what no shed may move.
    pub pinned_bytes: u64,
    /// Vouched candidates, oldest first.
    pub evictable: Vec<crate::cognition::working_memory::ShedCandidate>,
}

/// Read `memory`'s candidates under her lock, then vouch for each saved output after it drops.
pub(crate) fn residency(
    persona: Uuid,
    memory: &WorkingMemory,
    saved: &dyn Fn(Uuid, &str) -> bool,
) -> Residency {
    let ResidentResults {
        resident_bytes,
        candidates,
    } = memory.resident_results();
    let evictable: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| saved(persona, &candidate.handle))
        .collect();
    let movable: u64 = evictable.iter().map(|candidate| candidate.bytes).sum();
    Residency {
        resident_bytes,
        pinned_bytes: resident_bytes - movable,
        evictable,
    }
}

/// Per mind, the bytes to keep resident so the total comes down to `allocation`, moving the
/// oldest bodies across every mind first (`None`: leave that mind untouched). Within a mind
/// the oldest received are the lowest `seq`, so each budget moves exactly that mind's chosen
/// oldest bodies through `evict_resident_results_over`.
pub(crate) fn plan_shed(minds: &[Residency], allocation: u64) -> Vec<Option<u64>> {
    let total: u64 = minds.iter().map(|mind| mind.resident_bytes).sum();
    let mut freed = vec![0u64; minds.len()];
    if total > allocation {
        let mut candidates: Vec<(Option<u64>, u64, usize, u64)> = minds
            .iter()
            .enumerate()
            .flat_map(|(index, mind)| {
                mind.evictable
                    .iter()
                    .map(move |r| (r.received_ms, r.seq, index, r.bytes))
            })
            .collect();
        candidates.sort_unstable();
        let mut need = total - allocation;
        for (_, _, index, bytes) in candidates {
            if need == 0 {
                break;
            }
            freed[index] += bytes;
            need = need.saturating_sub(bytes);
        }
    }
    minds
        .iter()
        .zip(freed)
        .map(|(mind, freed)| (freed > 0).then(|| mind.resident_bytes - freed))
        .collect()
}

/// Shed every mind's resident bodies down to `allocation` bytes in total, oldest first.
/// `saved` vouches that a persona's handle has its output there to reload.
pub(crate) fn shed_to(minds: &Minds, allocation: u64, saved: &dyn Fn(Uuid, &str) -> bool) {
    let residencies: Vec<Residency> = minds
        .iter()
        .map(|(persona, memory)| residency(*persona, memory, saved))
        .collect();
    let budgets = plan_shed(&residencies, allocation);
    for (((_, memory), mind), budget) in minds.iter().zip(&residencies).zip(budgets) {
        if let Some(budget) = budget {
            // Only what was vouched for, and only if the record still names that handle.
            let vouched: std::collections::HashMap<u64, &str> = mind
                .evictable
                .iter()
                .map(|candidate| (candidate.seq, candidate.handle.as_str()))
                .collect();
            memory.evict_resident_results_over(budget, &|seq, handle| {
                vouched.get(&seq).is_some_and(|vouched| *vouched == handle)
            });
        }
    }
}

/// `saved` for production: a body whose output is not there stays, and says why.
fn vouched(persona: Uuid, handle: &str) -> bool {
    match saved_output(persona, handle) {
        Ok(()) => true,
        Err(last_copy) => {
            continuum_base::probe!(
                class = "wm.results.last_copy_kept",
                persona_id = %persona,
                handle = %handle,
                reason = %last_copy,
                "a resident result stays through a shed: its saved output is not there to reload"
            );
            false
        }
    }
}

/// The `MemoryReporter` for every mind's resident result bodies. Registered once at boot
/// beside the persona registry (`persona_registry_modules`).
pub struct MindResultsReporter {
    registry: Arc<PersonaWorkspaceRegistry>,
    /// The monitor's published snapshot: `shed_load` runs right after it is published, so this
    /// is the poll that decided to shed.
    pressure: watch::Receiver<PressureSnapshot>,
}

impl MindResultsReporter {
    pub fn new(
        registry: Arc<PersonaWorkspaceRegistry>,
        pressure: watch::Receiver<PressureSnapshot>,
    ) -> Self {
        Self { registry, pressure }
    }
}

impl MemoryReporter for MindResultsReporter {
    fn name(&self) -> &'static str {
        NAME
    }

    /// It wants to keep everything it holds (`preferred = max = resident`); its floor is what
    /// no shed may move (each mind's newest result and every last copy).
    fn budget(&self) -> MemoryBudgetSpec {
        let minds = self.registry.resident_working_memories();
        let (resident, pinned) =
            minds
                .iter()
                .fold((0u64, 0u64), |(resident, pinned), (persona, memory)| {
                    let mind = residency(*persona, memory, &|persona, handle| {
                        saved_output(persona, handle).is_ok()
                    });
                    (resident + mind.resident_bytes, pinned + mind.pinned_bytes)
                });
        MemoryBudgetSpec {
            name: NAME.to_string(),
            priority: PRIORITY,
            min_bytes: pinned,
            preferred_bytes: resident,
            max_bytes: resident,
        }
    }

    fn report(&self) -> ModuleMemoryReport {
        let minds = self.registry.resident_working_memories();
        let bytes = resident_bytes(&minds);
        ModuleMemoryReport {
            name: NAME.to_string(),
            bytes,
            detail: format!(
                "{} minds, {} KiB of resident result bodies",
                minds.len(),
                bytes / 1024
            ),
            can_shed: true,
        }
    }

    fn can_shed(&self) -> bool {
        true
    }

    fn shed_load(&self, level: PressureLevel) {
        let over = over_gate(&self.pressure.borrow());
        if over == 0 {
            return;
        }
        let minds = self.registry.resident_working_memories();
        let before = resident_bytes(&minds);
        shed_to(&minds, before.saturating_sub(over), &vouched);
        continuum_base::probe!(
            class = "wm.results.shed",
            level = %level,
            over_gate = %over,
            before = %before,
            after = %resident_bytes(&minds),
            "the memory owner moved the oldest resident results to their handles"
        );
    }
}
