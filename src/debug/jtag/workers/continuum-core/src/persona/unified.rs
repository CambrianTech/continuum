//! Unified Per-Persona Cognitive State
//!
//! All per-persona state in a single struct — one DashMap entry, one lock.
//!
//! Before: 7 separate DashMap<Uuid, T> — 7 lock acquisitions per command,
//! related state scattered across cache lines, no atomic cross-field access.
//!
//! After: 1 DashMap<Uuid, PersonaCognition> — 1 lock, contiguous memory,
//! atomic access to engine + rate_limiter + sleep_state + adapters + genome.

use crate::persona::cognition::PersonaCognitionEngine;
use crate::persona::evaluator::{RateLimiterState, SleepState};
use crate::persona::genome_paging::GenomePagingEngine;
use crate::persona::inbox::PersonaInbox;
use crate::persona::model_selection::AdapterRegistry;
use crate::rag::RagEngine;
use std::sync::Arc;
use uuid::Uuid;

/// All cognitive state for a single persona — single lock, cache-local.
pub struct PersonaCognition {
    pub engine: PersonaCognitionEngine,
    pub inbox: PersonaInbox,
    pub rate_limiter: RateLimiterState,
    pub sleep_state: SleepState,
    pub adapter_registry: AdapterRegistry,
    pub genome_engine: GenomePagingEngine,
}

impl PersonaCognition {
    /// Create a new PersonaCognition with default sub-states.
    /// Engine and inbox require persona_id; everything else uses defaults.
    pub fn new(
        persona_id: Uuid,
        persona_name: String,
        rag_engine: Arc<RagEngine>,
    ) -> Self {
        let (_, shutdown_rx) = tokio::sync::watch::channel(false);
        Self {
            engine: PersonaCognitionEngine::new(
                persona_id,
                persona_name,
                rag_engine,
                shutdown_rx,
            ),
            inbox: PersonaInbox::new(persona_id),
            rate_limiter: RateLimiterState::default(),
            sleep_state: SleepState::default(),
            adapter_registry: AdapterRegistry::default(),
            genome_engine: GenomePagingEngine::new(200.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persona_cognition_defaults() {
        let id = Uuid::new_v4();
        let rag = Arc::new(RagEngine::new());
        let pc = PersonaCognition::new(id, "TestBot".into(), rag);

        assert_eq!(pc.engine.persona_id(), id);
        assert!(pc.inbox.is_empty());
        assert!(!pc.rate_limiter.has_reached_response_cap(Uuid::new_v4()));
        assert_eq!(pc.sleep_state.mode, crate::persona::evaluator::SleepMode::Active);
        assert!(pc.adapter_registry.adapters.is_empty());
        assert!((pc.genome_engine.memory_pressure() - 0.0).abs() < 0.001);
    }
}
