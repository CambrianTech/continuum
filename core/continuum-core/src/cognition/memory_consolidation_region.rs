//! Continuous working-memory consolidation — the CHEAP sibling of
//! [`crate::cognition::dream_consolidation`].
//!
//! # Why this exists
//!
//! `memory::consolidation_pipeline::run_consolidation_pass` was written,
//! documented and unit-tested, and then **never called from production**. Five
//! mentions in the whole crate: its own doc, its definition, two calls inside
//! its own `#[cfg(test)]` block, and a `pub use`. Its own doc says why it was
//! left dangling:
//!
//! > What this does NOT own: WorkingMemory — the SOURCE of `thoughts`. The
//! > caller (future snoop loop) provides this vec. **Rust WorkingMemory
//! > primitive is still absent.**
//!
//! That primitive later landed ([`crate::cognition::working_memory`]) and
//! nobody went back to reconnect them. A new variant of the class this codebase
//! keeps hitting: not built-and-never-called, but **built, blocked, unblocked
//! by someone else, never revisited**.
//!
//! # Why it is a SIBLING and not part of the dream
//!
//! `DreamConsolidationRegion` declares [`ComputeClass::InferenceHeavy`] because
//! it runs LLM distillation. This pass does not: the only production
//! [`ConsolidationAdapter`] is `RawMemoryAdapter`, whose `consolidate` is a pure
//! 1:1 map (clone content, copy importance, carry `context_id`;
//! `synthesis_count: 0`, `embeddings_generated: 0`). Hosting a pure map inside
//! an inference-heavy region would gate it on VRAM and queue pressure it never
//! consumes — asleep exactly when it could safely run. Continuous compression
//! that only runs when the GPU is idle is not continuous.
//!
//! If the adapter ever becomes LLM-backed, THAT is when it earns
//! `InferenceHeavy`, and the split makes that a one-line metadata change rather
//! than a re-architecture.
//!
//! # The latency contract (non-negotiable)
//!
//! [`Orientation::SelfDirected`] — this draws from the interiority budget, never
//! the reactive one, so the governor's floored share means a consolidation tick
//! **can never preempt a responding tick under contention**. Memory work is a
//! separate threaded process; fast inference is never held back for it.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::cognition::working_memory::{WmKind, WorkingMemory};
use crate::memory::consolidation_adapter::{ConsolidationAdapter, ConsolidationContext, Thought};
use crate::memory::consolidation_pipeline::run_consolidation_pass;
use crate::memory::consolidator::Consolidator;
use crate::memory::PersonaMemoryManager;
use crate::runtime::brain_region::{
    BrainRegion, CadenceHint, ComputeClass, MemoryClass, Orientation, PressureProfile,
    PressureSignalKind, RegionContext, RegionId, TickOutcome,
};

/// What the region needs for ONE persona. Mirrors `PersonaReflector` in the
/// dream region: everything is `Arc` — shared, never owned here.
pub struct PersonaConsolidand {
    pub working_memory: Arc<WorkingMemory>,
    pub manager: Arc<PersonaMemoryManager>,
    pub adapter: Arc<dyn ConsolidationAdapter>,
    /// The persona's room, carried onto each memory so recall can find it by
    /// room after a reconnect (the corpus writer reads `contextId`/`roomId`).
    pub context_id: Option<Uuid>,
    pub session_id: Uuid,
    pub persona_name: String,
}

/// How the region discovers who to consolidate for. A trait for the same reason
/// `PersonaReflectionSource` is one: the region must not know how personas are
/// stored, and must never hold a parallel persona map.
pub trait PersonaConsolidationSource: Send + Sync {
    /// Personas with live working memory this pass.
    fn live_personas(&self) -> Vec<Uuid>;
    /// `None` when this persona has nothing consolidatable — the region sleeps
    /// for her this tick rather than inventing work.
    fn consolidand_for(&self, persona_id: Uuid) -> Option<PersonaConsolidand>;
}

pub struct MemoryConsolidationRegion {
    source: Arc<dyn PersonaConsolidationSource>,
    /// Thought text already consolidated, per persona.
    ///
    /// The region tracks this ITSELF rather than draining WorkingMemory —
    /// exactly how `DreamConsolidationRegion` keeps "per-persona sets of
    /// consolidated engram ids". Mutating another faculty's buffer from a
    /// background tick is how you race the reactive path; remembering what you
    /// already did is not.
    seen: Mutex<std::collections::HashMap<Uuid, HashSet<String>>>,
    /// Never two concurrent passes for one persona.
    in_flight: Mutex<HashSet<Uuid>>,
    /// `Option` so a pass can TAKE ownership for the duration of its await and
    /// put it back after.
    ///
    /// `run_consolidation_pass` needs `&mut Consolidator` across an `.await`,
    /// and holding a `std::sync::MutexGuard` across an await makes the future
    /// non-`Send` — which the governor cannot spawn, and which the concurrency
    /// style guide names as a forbidden move outright. Taking the value out
    /// also gives correct serialisation for free: a second tick finds `None`
    /// and sleeps rather than queueing behind a lock.
    consolidator: Mutex<Option<Consolidator>>,
}

impl MemoryConsolidationRegion {
    pub fn new(source: Arc<dyn PersonaConsolidationSource>) -> Self {
        Self {
            source,
            seen: Mutex::new(std::collections::HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            consolidator: Mutex::new(Some(Consolidator::default())),
        }
    }

    /// Thoughts this persona has produced that we have not consolidated yet.
    /// Pure over (entries, seen) so the selection is testable without a live
    /// persona — the region's one real decision.
    fn unconsolidated(entries: &[(WmKind, String)], seen: &HashSet<String>) -> Vec<String> {
        entries
            .iter()
            .filter(|(kind, _)| matches!(kind, WmKind::Thought))
            .map(|(_, text)| text.clone())
            .filter(|text| !text.trim().is_empty() && !seen.contains(text))
            .collect()
    }

    async fn consolidate(&self, persona_id: Uuid) -> TickOutcome {
        // in-flight gate FIRST: cheapest possible bail.
        {
            let mut flight = match self.in_flight.lock() {
                Ok(g) => g,
                Err(_) => return sleep(),
            };
            if !flight.insert(persona_id) {
                return sleep();
            }
        }
        let outcome = self.consolidate_inner(persona_id).await;
        if let Ok(mut flight) = self.in_flight.lock() {
            flight.remove(&persona_id);
        }
        outcome
    }

    async fn consolidate_inner(&self, persona_id: Uuid) -> TickOutcome {
        let Some(c) = self.source.consolidand_for(persona_id) else {
            return sleep();
        };

        let entries: Vec<(WmKind, String)> = c
            .working_memory
            .recent_entries()
            .into_iter()
            .map(|e| (e.kind, e.text))
            .collect();

        let fresh = {
            let seen = match self.seen.lock() {
                Ok(g) => g,
                Err(_) => return sleep(),
            };
            let empty = HashSet::new();
            Self::unconsolidated(&entries, seen.get(&persona_id).unwrap_or(&empty))
        };
        if fresh.is_empty() {
            // Rest, not a clock — nothing new to compress.
            return sleep();
        }

        let now_ms = now_ms();
        let thoughts: Vec<Thought> = fresh
            .iter()
            .map(|text| Thought {
                // DETERMINISTIC, not random. `seen` lives in memory, and
                // restarts are commonplace here by design — so after a restart
                // every thought still in working memory would be consolidated
                // AGAIN. With a v4 id each repeat is a new corpus row and the
                // continuous consolidator becomes a continuous memory leak,
                // across a process boundary where the in-region dedup cannot
                // see it.
                //
                // A v5 id over (persona, content) makes the write IDEMPOTENT
                // instead: re-consolidating the same thought produces the same
                // id, so it collapses rather than duplicating. That also side-
                // steps the hazard M5 flagged — EntityStore has no transaction
                // and no atomic compare-and-set, so anything needing
                // read-modify-write is unsafe against a concurrent admission.
                // An idempotent append needs neither.
                id: deterministic_thought_id(persona_id, text),
                content: text.clone(),
                thought_type: "observation".to_string(),
                domain: None,
                context_id: c.context_id,
                importance: 0.5,
                created_at_ms: now_ms,
                // Working-memory reasoning is the persona's own interior. Not
                // broadcastable until something decides otherwise — default to
                // private, because the inverse mistake (leaking a citizen's
                // private thought onto the grid) is unrecoverable.
                shareable: false,
            })
            .collect();

        let ctx = ConsolidationContext {
            persona_id,
            persona_name: c.persona_name.clone(),
            session_id: c.session_id,
            timestamp_ms: now_ms,
        };

        // TAKE the consolidator — no guard may be held across the await below.
        let Some(mut consolidator) = self.consolidator.lock().ok().and_then(|mut g| g.take()) else {
            // Another pass owns it; rest rather than queue.
            return sleep();
        };

        let result = run_consolidation_pass(
            &mut consolidator,
            &thoughts,
            &ctx,
            c.adapter.as_ref(),
            c.manager.as_ref(),
        )
        .await;

        // Put it back on EVERY path, including the error path — dropping it
        // would silently disable consolidation for the rest of the process,
        // which is precisely the never-runs-again failure this region exists
        // to end.
        if let Ok(mut slot) = self.consolidator.lock() {
            *slot = Some(consolidator);
        }

        match result {
            Ok(res) => {
                // Only mark what actually landed. A failed pass must be retried,
                // never silently swallowed — that would be memory loss reported
                // as success.
                if let Ok(mut seen) = self.seen.lock() {
                    seen.entry(persona_id).or_default().extend(fresh);
                }
                crate::probe!(
                    class = "memory.consolidation.pass",
                    persona = %persona_id,
                    memories = res.memories.len(),
                    "consolidated working-memory thoughts into the corpus",
                );
                TickOutcome::idle()
            }
            Err(error) => {
                crate::probe!(
                    class = "memory.consolidation.failed",
                    persona = %persona_id,
                    error = %error,
                    "consolidation pass failed — thoughts stay unconsolidated and retry next tick",
                );
                sleep()
            }
        }
    }
}

#[async_trait::async_trait]
impl BrainRegion for MemoryConsolidationRegion {
    fn id(&self) -> RegionId {
        RegionId::from_static("memory-consolidation")
    }

    fn pressure_profile(&self) -> PressureProfile {
        PressureProfile {
            // Holds per-persona sets of consolidated thought text, bounded by
            // working-memory capacity. Light.
            memory_class: MemoryClass::Light,
            // A clone-and-map. NOT CpuVectorized — there is no vector work here
            // — and emphatically not InferenceHeavy: declaring a compute class
            // this region does not consume would make the governor gate it on
            // VRAM it never touches, i.e. sleep exactly when it could run.
            compute_class: ComputeClass::Cpu,
            // Memory-pressure signals, not GPU ones. Back off when the box is
            // tight on RAM or the user is actively engaged.
            responds_to: vec![
                PressureSignalKind::SystemMemHigh,
                PressureSignalKind::UserActive,
            ],
        }
    }

    /// Interiority budget, never the reactive one — see the module doc. This is
    /// the property that makes "never hold back fast inference" structural
    /// rather than conventional.
    fn orientation(&self) -> Orientation {
        Orientation::SelfDirected
    }

    async fn tick(&self, ctx: &RegionContext) -> TickOutcome {
        match ctx.persona_scope {
            Some(persona_id) => self.consolidate(persona_id).await,
            // A global tick has no working memory to compress.
            None => sleep(),
        }
    }
}

/// Stable id for "this persona's memory of this exact thought".
///
/// UUID v5 (SHA-1 over namespace + name) is deterministic, so the same thought
/// consolidated twice — across a restart, a re-spawn, or a duplicated tick —
/// yields the SAME id and collapses instead of duplicating. The namespace is a
/// fixed constant so the mapping is stable across processes and machines,
/// which also means two nodes consolidating the same shared thought agree on
/// its identity rather than each minting their own.
fn deterministic_thought_id(persona_id: Uuid, content: &str) -> Uuid {
    // Fixed namespace for continuum working-memory consolidation. Never change
    // this value: it would re-mint every id and duplicate the entire corpus.
    const NS: Uuid = Uuid::from_u128(0x5f3a_9c21_7d44_4e6b_9a12_c8e0_51b7_2d40);
    let name = format!("{persona_id}:{content}");
    Uuid::new_v5(&NS, name.as_bytes())
}

/// Unix milliseconds. Local helper: several cognition modules keep their own
/// private `now_ms`, so this follows the neighbourhood convention rather than
/// inventing a shared one nobody asked for.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Sleep until experience accrues — resting, not spinning a clock.
fn sleep() -> TickOutcome {
    TickOutcome {
        cadence_hint: Some(CadenceHint::Sleep),
        ..TickOutcome::idle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wm(pairs: &[(WmKind, &str)]) -> Vec<(WmKind, String)> {
        pairs
            .iter()
            .map(|(k, t)| (k.clone(), t.to_string()))
            .collect()
    }

    /// what this catches: consolidating the same thought forever. The pass has
    /// no natural idempotence — `RawMemoryAdapter` maps 1:1, so re-feeding a
    /// thought writes a DUPLICATE corpus memory every tick. Without the seen-set
    /// a continuous consolidator becomes a continuous memory leak.
    #[test]
    fn already_consolidated_thoughts_are_never_reconsolidated() {
        let entries = wm(&[
            (WmKind::Thought, "the board is per-room"),
            (WmKind::Thought, "atlas cannot name the prior message"),
        ]);
        let mut seen = HashSet::new();
        seen.insert("the board is per-room".to_string());

        let fresh = MemoryConsolidationRegion::unconsolidated(&entries, &seen);
        assert_eq!(fresh, vec!["atlas cannot name the prior message".to_string()]);
    }

    /// what this catches: consolidating things that are not thoughts. Working
    /// memory carries receipts, facts and dispatch events alongside reasoning;
    /// sweeping those into the corpus would fill long-term memory with the
    /// persona's own action ledger — which is exactly the #166 pollution the
    /// episodic store already suffers from.
    #[test]
    fn only_thought_entries_are_consolidated() {
        let entries = wm(&[
            (WmKind::Thought, "a real thought"),
            (WmKind::Receipt { n: 12 }, "[action #12] work/list -> {...}"),
            (WmKind::Fact, "[repetition] 3 of your recent messages"),
        ]);
        let fresh = MemoryConsolidationRegion::unconsolidated(&entries, &HashSet::new());
        assert_eq!(fresh, vec!["a real thought".to_string()]);
    }

    /// what this catches: RESTART AMNESIA turning a continuous consolidator
    /// into a continuous memory leak. `seen` is in-process, and restarts are
    /// commonplace by design — so after one, every thought still in working
    /// memory is consolidated again. With a random id each repeat is a NEW
    /// corpus row and the duplicate is invisible to the in-region dedup,
    /// because that dedup died with the process. A deterministic id makes the
    /// repeat collapse instead.
    #[test]
    fn the_same_thought_always_gets_the_same_memory_id() {
        let persona = Uuid::from_u128(7);
        let a = deterministic_thought_id(persona, "the board is per-room");
        let b = deterministic_thought_id(persona, "the board is per-room");
        assert_eq!(a, b, "a restart must not mint a second row for one thought");

        // Different thought, or different persona, must NOT collide — one
        // citizen's memory becoming another's would be worse than a duplicate.
        assert_ne!(a, deterministic_thought_id(persona, "a different thought"));
        assert_ne!(a, deterministic_thought_id(Uuid::from_u128(8), "the board is per-room"));
    }

    /// what this catches: an empty or whitespace thought becoming a corpus row.
    /// Cheap to write, permanent once stored.
    #[test]
    fn blank_thoughts_are_not_memories() {
        let entries = wm(&[(WmKind::Thought, "   "), (WmKind::Thought, "")]);
        assert!(MemoryConsolidationRegion::unconsolidated(&entries, &HashSet::new()).is_empty());
    }
}
