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

use std::path::PathBuf;
use std::sync::Arc;

use continuum_kernel::system_resources::{
    MemoryBudgetSpec, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
    PressureSnapshot,
};
use tokio::sync::watch;
use uuid::Uuid;

use super::PersonaWorkspaceRegistry;
use crate::cognition::working_memory::{Residency, WorkingMemory};

const NAME: &str = "mind_results";

/// A cache of state that can spill: every body it sheds is already saved whole to its handle
/// and reloads when her view grants it. That is `Background` ("caches, pre-computed data, idle
/// resources"), not `Interactive` (state a mind cannot work without: the newest result and the
/// last copies are that, and the shed never moves them).
const PRIORITY: MemoryPriority = MemoryPriority::Background;

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
        .map(|(persona, memory)| memory.residency(&|handle| saved(*persona, handle)))
        .collect();
    for ((persona, memory), budget) in minds.iter().zip(plan_shed(&residencies, allocation)) {
        if let Some(budget) = budget {
            memory.evict_resident_results_over(budget, &|handle| saved(*persona, handle));
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
                    let residency =
                        memory.residency(&|handle| saved_output(*persona, handle).is_ok());
                    (
                        resident + residency.resident_bytes,
                        pinned + residency.pinned_bytes,
                    )
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
