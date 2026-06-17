//! Per-persona workspace assembly + registry — the "one soul, many rooms" seam.
//!
//! This is the constructor `ai/should-respond` (and the bring-up harness) resolve
//! a persona's mind through. The load-bearing decision (PERSONA-BRAIN-
//! ARCHITECTURE.md §2.9) is structural: **one `WorkspaceCycle` per persona**,
//! keyed by `persona_id` — NOT by `(persona_id, room_id)`. A persona is one
//! continuous self across every room it services; its unified `AdmissionState`
//! (the hippocampus) spans all its activities. Keying the registry by persona is
//! what makes the citizen continuous instead of *severed* per-room.
//!
//! The same cycle is invoked for whatever room the persona is servicing; the room
//! supplies the per-tick world-state (the consolidated burst), the persona
//! supplies the unified memory + identity + faculties.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use uuid::Uuid;

use super::llm_deliberation_faculty::LlmDeliberationFaculty;
use super::recall_faculty::RecallFaculty;
use super::workspace::{Faculty, SalienceArbiter, WorkspaceCycle};
use crate::ai::adapter::AIProviderAdapter;
use crate::persona::admission_state::AdmissionState;

/// Default bounded workspace capacity — the finite attention "spotlight". Enough
/// for recall + world-model + affect + roster context to coexist; the arbiter
/// keeps it bounded so cost stays O(capacity) no matter how many faculties bid.
pub const DEFAULT_WORKSPACE_CAPACITY: usize = 6;

/// Everything needed to assemble one persona's continuous mind. The `admission`
/// is the persona's UNIFIED hippocampus (shared with the admission pipeline and
/// spanning all the persona's rooms); the `adapter` is the shared model backend,
/// leased inside the deliberation faculty.
pub struct PersonaBrainConfig {
    pub persona_id: Uuid,
    pub persona_name: String,
    /// The persona's identity / deliberation system prompt (from RAG identity).
    pub system_prompt: String,
    pub admission: Arc<AdmissionState>,
    pub adapter: Arc<dyn AIProviderAdapter>,
    /// Bounded workspace capacity; `None` → [`DEFAULT_WORKSPACE_CAPACITY`].
    pub capacity: Option<usize>,
}

/// Assemble a persona's `WorkspaceCycle` from its faculties. This IS the
/// production assembly path — the bring-up harness and the `ai/should-respond`
/// ServiceModule build the cycle the same way, so they cannot diverge.
///
/// v1 faculties: `RecallFaculty` (perception tier — the hippocampus) and
/// `LlmDeliberationFaculty` (deliberation tier — the reasoner). More faculties
/// (world-model, affect, volition) slot into this `Vec` as they land; nothing
/// else changes (open/closed — §2.7).
pub fn build_workspace_cycle(cfg: PersonaBrainConfig) -> WorkspaceCycle {
    let faculties: Vec<Arc<dyn Faculty>> = vec![
        Arc::new(RecallFaculty::new(cfg.persona_id, cfg.admission)),
        Arc::new(LlmDeliberationFaculty::new(
            cfg.persona_id,
            cfg.persona_name,
            cfg.system_prompt,
            cfg.adapter,
        )),
    ];
    WorkspaceCycle::new(
        faculties,
        Arc::new(SalienceArbiter),
        cfg.capacity.unwrap_or(DEFAULT_WORKSPACE_CAPACITY),
    )
}

/// Persona-scoped registry of continuous minds. One `Arc<WorkspaceCycle>` per
/// persona; lookups by `persona_id`. `ai/should-respond` resolves the cycle here,
/// runs it over the room's consolidated burst, and reads the `Decision`.
#[derive(Default)]
pub struct PersonaWorkspaceRegistry {
    cycles: Mutex<HashMap<Uuid, Arc<WorkspaceCycle>>>,
}

impl PersonaWorkspaceRegistry {
    pub fn new() -> Self {
        Self {
            cycles: Mutex::new(HashMap::new()),
        }
    }

    /// Look up a persona's mind. `None` if it hasn't been registered/built yet.
    pub fn get(&self, persona_id: &Uuid) -> Option<Arc<WorkspaceCycle>> {
        self.cycles.lock().unwrap().get(persona_id).cloned()
    }

    /// Register a pre-built cycle for a persona (overwrites any existing).
    pub fn register(&self, persona_id: Uuid, cycle: Arc<WorkspaceCycle>) {
        self.cycles.lock().unwrap().insert(persona_id, cycle);
    }

    /// Get the persona's mind, building + caching it from `cfg` on first access.
    /// Lazy-init so a persona's cycle is assembled once and reused across every
    /// room it services (the "one soul" invariant).
    pub fn get_or_build(&self, cfg: PersonaBrainConfig) -> Arc<WorkspaceCycle> {
        let persona_id = cfg.persona_id;
        let mut cycles = self.cycles.lock().unwrap();
        if let Some(existing) = cycles.get(&persona_id) {
            return existing.clone();
        }
        let cycle = Arc::new(build_workspace_cycle(cfg));
        cycles.insert(persona_id, cycle.clone());
        cycle
    }

    /// How many persona minds are resident.
    pub fn len(&self) -> usize {
        self.cycles.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Process-global persona-workspace registry. One per process; persona minds are
/// assembled into it at spawn (`supervisor::materialize_adapters`) and resolved
/// from it by the `ai/should-respond` ServiceModule. Same pattern as
/// `modules::ai_provider::global_registry()` — the shared seam between the spawn
/// path that builds minds and the command path that runs them.
pub fn global() -> Arc<PersonaWorkspaceRegistry> {
    static GLOBAL: OnceLock<Arc<PersonaWorkspaceRegistry>> = OnceLock::new();
    GLOBAL
        .get_or_init(|| Arc::new(PersonaWorkspaceRegistry::new()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::cognition::workspace::Decision;
    use crate::persona::engram::{ChatMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
    use crate::persona::recall_metadata::{RecallMetadata, RecallMetadataRegistry};

    fn seed_admission(now_ms: u64) -> Arc<AdmissionState> {
        let recall_meta = Arc::new(RecallMetadataRegistry::new());
        let state = Arc::new(AdmissionState::new(recall_meta.clone()));
        let id = Uuid::new_v4();
        let engram = Engram {
            id,
            kind: EngramKind::Episodic,
            content: "the deploy pipeline went green after the 4pm fix".to_string(),
            origin: EngramOrigin::Chat(ChatMessageRef {
                message_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                sender_id: Uuid::new_v4(),
                posted_at_ms: now_ms,
                content_hash: "h".to_string(),
            }),
            recall_keys: Vec::new(),
            admitted_at_ms: now_ms,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        };
        state.push_for_test(engram);
        recall_meta.admit(
            id,
            RecallMetadata {
                salience: 0.7,
                access_count: 0,
                last_accessed_ms: 0,
                protected_until_ms: 0,
                last_decayed_ms: now_ms,
            },
        );
        state
    }

    fn cfg_for(persona_id: Uuid) -> PersonaBrainConfig {
        PersonaBrainConfig {
            persona_id,
            persona_name: "Ivar".to_string(),
            system_prompt: "You are Ivar, an engineer on the grid.".to_string(),
            admission: seed_admission(1_000_000_000),
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            capacity: None,
        }
    }

    // what this catches: the assembled cycle runs a FULL persona mind end-to-end —
    // recall (hippocampus) bids in phase 1, deliberation (real adapter) decides in
    // phase 2 over that context — and yields a Decision. This is the production
    // assembly path; swap the adapter for LlamaCppAdapter and it's a live persona.
    #[tokio::test]
    async fn assembled_cycle_produces_a_decision() {
        let persona = Uuid::new_v4();
        let cycle = build_workspace_cycle(cfg_for(persona));
        let ws = cycle.run("teammate: what's the deploy status?").await;
        // The mind reached a participation verdict (heuristic adapter → Speak).
        assert!(matches!(ws.decision(), Some(Decision::Speak { .. })));
    }

    // what this catches: ONE cycle per persona — get_or_build is idempotent and
    // returns the SAME Arc, so a persona's continuous mind is reused across every
    // room it services (the "one soul, many rooms" / anti-Severance invariant).
    #[tokio::test]
    async fn registry_keeps_one_mind_per_persona() {
        let registry = PersonaWorkspaceRegistry::new();
        let persona = Uuid::new_v4();
        let first = registry.get_or_build(cfg_for(persona));
        let second = registry.get_or_build(cfg_for(persona));
        assert!(
            Arc::ptr_eq(&first, &second),
            "same persona must resolve to the SAME mind across rooms — not severed per-room"
        );
        assert_eq!(registry.len(), 1);
        // A different persona is a different mind.
        let _ = registry.get_or_build(cfg_for(Uuid::new_v4()));
        assert_eq!(registry.len(), 2);
    }
}
