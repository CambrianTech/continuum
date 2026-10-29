//! The one memory owner's view of every mind's resident result bodies.
//!
//! A result's body stays in RAM until the memory pressure monitor says shed: RAM decides, not
//! a number (Joel, 2026-10-29; it replaced `RESULTS_RESIDENT_CHARS = 1 << 20`, a cap per mind
//! at receipt). This reporter is the only thing that moves a body to its handle in production
//! (`WorkingMemory::evict_resident_results_over`). It sheds oldest-first across every resident
//! mind, public and private, and never moves a mind's newest result or a body whose saved copy
//! is not PROVEN to load back to it (the last copy).
//!
//! Accounting is BYTES of resident bodies (`String::len`), never characters: characters are
//! the view's price (`stored_chars`). Each record owns its body (no shared backing), so the sum
//! over records is the RAM they hold; a working memory reached twice is counted once.
//!
//! Proof without a filesystem read under a mind's lock: the shed plans from memory only, reads
//! each planned candidate's saved copy through the spill owner's one read (`spill::saved_whole`,
//! the same text her view's `load` renders) and digests it after the lock drops; under the lock
//! a body moves only if its record still names that handle and its digest matches.
//!
//! NOT FULL GOVERNOR COVERAGE: the shed target is the host's RAM overrun past this priority's
//! gate (`over_gate`). Pressure raised by swap activity alone, with RAM use under the gate,
//! makes `over_gate` return 0 and sheds nothing here.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use continuum_kernel::system_resources::{
    MemoryBudgetSpec, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
    PressureSnapshot,
};
use tokio::sync::watch;
use uuid::Uuid;

use super::PersonaWorkspaceRegistry;
use crate::cognition::tool_executor::spill::{self, WholeDigest};
use crate::cognition::working_memory::{ResidentResults, ShedCandidate, WorkingMemory};

const NAME: &str = "mind_results";

/// Her results are working state for her current task ("active persona state"), not an idle
/// cache: `Interactive`. Its gate (0.80) is where the monitor's Warning starts, so a shed gives
/// up only the overrun past it, not every movable body at the first Warning as `Background`'s
/// 0.60 gate would.
const PRIORITY: MemoryPriority = MemoryPriority::Interactive;

/// Why a body stays resident through a shed: it is the last copy of what she read.
#[derive(Debug)]
pub(crate) enum LastCopy {
    /// Its saved copy could not be read back (no such handle, no file, no permission, not text).
    Unreadable(std::io::Error),
    /// Its saved copy reads back, but not to this body.
    Mismatch,
}

impl LastCopy {
    fn reason(&self) -> &'static str {
        match self {
            Self::Unreadable(_) => "unreadable",
            Self::Mismatch => "mismatch",
        }
    }

    fn probe(&self, persona: Uuid, handle: &str) {
        let detail = match self {
            Self::Unreadable(error) => error.to_string(),
            Self::Mismatch => "its saved copy does not load back to the resident body".to_owned(),
        };
        continuum_base::probe!(
            class = "wm.results.last_copy_kept",
            persona_id = %persona,
            handle = %handle,
            reason = self.reason(),
            detail = %detail,
            "a resident result stays through a shed: its saved copy cannot reproduce it"
        );
    }
}

/// What `persona`'s saved copy for `handle` loads back to, as a digest: read outside every
/// mind's lock, compared with the resident body under it.
fn saved_copy(persona: Uuid, handle: &str) -> Result<WholeDigest, LastCopy> {
    spill::saved_digest(persona, handle).map_err(LastCopy::Unreadable)
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

/// One mind's resident result bodies, read from memory alone: what a shed may plan to move
/// (saved, not the newest) and what it may not.
#[derive(Debug, Clone)]
pub(crate) struct Residency {
    pub resident_bytes: u64,
    /// The newest result and every unsaved body. A saved copy that fails its proof at shed
    /// time stays too; that is found only then, by reading it.
    pub pinned_bytes: u64,
    /// Saved candidates, oldest first.
    pub evictable: Vec<ShedCandidate>,
}

pub(crate) fn residency(memory: &WorkingMemory) -> Residency {
    let ResidentResults {
        resident_bytes,
        candidates,
    } = memory.resident_results();
    let movable: u64 = candidates.iter().map(|candidate| candidate.bytes).sum();
    Residency {
        resident_bytes,
        pinned_bytes: resident_bytes - movable,
        evictable: candidates,
    }
}

/// Per mind, the candidates to move so the total comes down to `allocation`, the oldest
/// received across every mind first.
pub(crate) fn plan_shed(minds: &[Residency], allocation: u64) -> Vec<HashSet<u64>> {
    let total: u64 = minds.iter().map(|mind| mind.resident_bytes).sum();
    let mut chosen = vec![HashSet::new(); minds.len()];
    if total > allocation {
        let mut candidates: Vec<(Option<u64>, u64, usize, u64)> = minds
            .iter()
            .enumerate()
            .flat_map(|(index, mind)| {
                mind.evictable
                    .iter()
                    .map(move |c| (c.received_ms, c.seq, index, c.bytes))
            })
            .collect();
        candidates.sort_unstable();
        let mut need = total - allocation;
        for (_, seq, index, bytes) in candidates {
            if need == 0 {
                break;
            }
            chosen[index].insert(seq);
            need = need.saturating_sub(bytes);
        }
    }
    chosen
}

/// Shed every mind's resident bodies down to `allocation` bytes in total, oldest first. Each
/// planned body moves only once its saved copy is proven to load back to it; a copy that
/// cannot keeps the body resident and says why (`LastCopy`).
pub(crate) fn shed_to(minds: &Minds, allocation: u64) {
    let residencies: Vec<Residency> = minds.iter().map(|(_, memory)| residency(memory)).collect();
    let plans = plan_shed(&residencies, allocation);
    for (((persona, memory), mind), chosen) in minds.iter().zip(&residencies).zip(plans) {
        if chosen.is_empty() {
            continue;
        }
        // Outside her lock: read and digest each planned copy.
        let proven: HashMap<u64, (&str, WholeDigest)> = mind
            .evictable
            .iter()
            .filter(|candidate| chosen.contains(&candidate.seq))
            .filter_map(|candidate| match saved_copy(*persona, &candidate.handle) {
                Ok(digest) => Some((candidate.seq, (candidate.handle.as_str(), digest))),
                Err(last_copy) => {
                    last_copy.probe(*persona, &candidate.handle);
                    None
                }
            })
            .collect();
        let movable: u64 = mind
            .evictable
            .iter()
            .filter(|c| proven.contains_key(&c.seq))
            .map(|c| c.bytes)
            .sum();
        // Under her lock: only a record still naming the proven handle, whose body matches.
        memory.evict_resident_results_over(
            mind.resident_bytes.saturating_sub(movable),
            &|seq, handle, body| {
                let Some((proven_handle, digest)) = proven.get(&seq) else {
                    return false;
                };
                if *proven_handle != handle {
                    return false;
                }
                let reproduces = WholeDigest::of(body) == *digest;
                if !reproduces {
                    LastCopy::Mismatch.probe(*persona, handle);
                }
                reproduces
            },
        );
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
    /// no shed may move (each mind's newest result and every unsaved body). Read from memory
    /// alone: a saved copy that fails its proof is found only when a shed reads it.
    fn budget(&self) -> MemoryBudgetSpec {
        let minds = self.registry.resident_working_memories();
        let (resident, pinned) =
            minds
                .iter()
                .fold((0u64, 0u64), |(resident, pinned), (_, memory)| {
                    let mind = residency(memory);
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
        shed_to(&minds, before.saturating_sub(over));
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
