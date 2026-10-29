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
//! Proof, once per saved copy, never under a mind's lock and never in `budget()`: at shed time
//! each candidate's body is copied out (the lock held only for the copy) and compared with what
//! its saved copy loads back to through the spill owner's one read (`spill::reproduces`, the
//! same text her view's `load` renders). A proven `(persona, handle)` is cached: the output is
//! content-addressed and live-protected while a record names it, so the proof holds. Only
//! proven bodies are planned and moved; an unproven one counts as pinned.
//!
//! NOT FULL GOVERNOR COVERAGE: the shed target is the host's RAM overrun past this priority's
//! gate (`over_gate`). Pressure raised by swap activity alone, with RAM use under the gate,
//! makes `over_gate` return 0 and sheds nothing here.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;

use continuum_kernel::system_resources::{
    MemoryBudgetSpec, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
    PressureSnapshot,
};
use tokio::sync::watch;
use uuid::Uuid;

use super::PersonaWorkspaceRegistry;
use crate::cognition::tool_executor::spill;
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
    pub(crate) fn reason(&self) -> &'static str {
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

/// Prove `candidate`'s saved copy loads back to exactly its resident body. `None` when the
/// record moved on since it was read (nothing to prove). Reads the file with no lock held.
pub(crate) fn prove(
    persona: Uuid,
    memory: &WorkingMemory,
    candidate: &ShedCandidate,
) -> Option<Result<(), LastCopy>> {
    let body = memory.resident_body(candidate.seq, &candidate.handle)?;
    Some(match spill::reproduces(persona, &candidate.handle, &body) {
        Ok(true) => Ok(()),
        Ok(false) => Err(LastCopy::Mismatch),
        Err(error) => Err(LastCopy::Unreadable(error)),
    })
}

/// The saved copies proven to reproduce their resident bodies, per `(persona, handle)`. Pruned
/// at every shed to the candidates that still exist, so it never outgrows what is resident.
#[derive(Debug, Default)]
pub(crate) struct ProvenCopies(HashSet<(Uuid, String)>);

impl ProvenCopies {
    pub(crate) fn contains(&self, persona: Uuid, handle: &str) -> bool {
        self.0.contains(&(persona, handle.to_owned()))
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

/// One mind's resident result bodies: what a shed may move (saved, not the newest, and its
/// copy proven) and what it may not.
#[derive(Debug, Clone)]
pub(crate) struct Residency {
    pub resident_bytes: u64,
    /// The newest result, every unsaved body, and every body whose copy is not proven yet.
    pub pinned_bytes: u64,
    /// Proven candidates, oldest first.
    pub evictable: Vec<ShedCandidate>,
}

/// `memory`'s residency against the proofs in hand: memory only, no filesystem access.
pub(crate) fn residency(persona: Uuid, memory: &WorkingMemory, proven: &ProvenCopies) -> Residency {
    let ResidentResults {
        resident_bytes,
        candidates,
    } = memory.resident_results();
    let evictable: Vec<ShedCandidate> = candidates
        .into_iter()
        .filter(|candidate| proven.contains(persona, &candidate.handle))
        .collect();
    let movable: u64 = evictable.iter().map(|candidate| candidate.bytes).sum();
    Residency {
        resident_bytes,
        pinned_bytes: resident_bytes - movable,
        evictable,
    }
}

/// What the reporter declares: everything resident preferred, the unprovable floor as minimum.
/// Reads the proof cache only: the monitor calls this under its reporters lock.
pub(crate) fn budget_of(minds: &Minds, proven: &Mutex<ProvenCopies>) -> MemoryBudgetSpec {
    let proven = proven.lock();
    let (resident, pinned) =
        minds
            .iter()
            .fold((0u64, 0u64), |(resident, pinned), (persona, memory)| {
                let mind = residency(*persona, memory, &proven);
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

/// Prove every saved candidate not proven yet, each with no lock held across its read; a copy
/// that cannot reproduce its body stays unproven and says why (`LastCopy`). Then forget proofs
/// for handles no longer resident anywhere.
fn prove_candidates(minds: &Minds, proven: &Mutex<ProvenCopies>) {
    let mut resident: HashSet<(Uuid, String)> = HashSet::new();
    for (persona, memory) in minds {
        for candidate in memory.resident_results().candidates {
            let key = (*persona, candidate.handle.clone());
            if !proven.lock().0.contains(&key) {
                match prove(*persona, memory, &candidate) {
                    Some(Ok(())) => {
                        proven.lock().0.insert(key.clone());
                    }
                    Some(Err(last_copy)) => last_copy.probe(*persona, &candidate.handle),
                    None => {}
                }
            }
            resident.insert(key);
        }
    }
    proven.lock().0.retain(|key| resident.contains(key));
}

/// Shed every mind's resident bodies down to `allocation` bytes in total, oldest first, moving
/// only bodies whose saved copy is proven to load back to them.
pub(crate) fn shed_to(minds: &Minds, allocation: u64, proven: &Mutex<ProvenCopies>) {
    prove_candidates(minds, proven);
    let residencies: Vec<Residency> = {
        let proven = proven.lock();
        minds
            .iter()
            .map(|(persona, memory)| residency(*persona, memory, &proven))
            .collect()
    };
    let plans = plan_shed(&residencies, allocation);
    for (((_, memory), mind), chosen) in minds.iter().zip(&residencies).zip(plans) {
        if chosen.is_empty() {
            continue;
        }
        let handles: HashMap<u64, &str> = mind
            .evictable
            .iter()
            .filter(|candidate| chosen.contains(&candidate.seq))
            .map(|candidate| (candidate.seq, candidate.handle.as_str()))
            .collect();
        let movable: u64 = mind
            .evictable
            .iter()
            .filter(|c| handles.contains_key(&c.seq))
            .map(|c| c.bytes)
            .sum();
        // Under her lock: only a planned record still naming its proven handle.
        memory.evict_resident_results_over(
            mind.resident_bytes.saturating_sub(movable),
            &|seq, handle| {
                handles
                    .get(&seq)
                    .is_some_and(|proven_handle| *proven_handle == handle)
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
    /// Saved copies proven at shed time; `budget()` reads only this.
    proven: Mutex<ProvenCopies>,
}

impl MindResultsReporter {
    pub fn new(
        registry: Arc<PersonaWorkspaceRegistry>,
        pressure: watch::Receiver<PressureSnapshot>,
    ) -> Self {
        Self {
            registry,
            pressure,
            proven: Mutex::new(ProvenCopies::default()),
        }
    }
}

impl MemoryReporter for MindResultsReporter {
    fn name(&self) -> &'static str {
        NAME
    }

    /// It wants to keep everything it holds (`preferred = max = resident`); its floor is what no
    /// shed may move yet: each mind's newest result, every unsaved body, and every body whose
    /// copy is not proven. No filesystem access: proofs are made at shed time and cached.
    fn budget(&self) -> MemoryBudgetSpec {
        budget_of(&self.registry.resident_working_memories(), &self.proven)
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
        shed_to(&minds, before.saturating_sub(over), &self.proven);
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
