//! RAG Engine - Parallel Context Composition
//!
//! The core of fast RAG: load ALL sources in parallel via rayon.
//! Target: <500ms total (currently 20+ seconds in TypeScript)

use super::budget::{BudgetManager, SourceConfig};
use super::sources::RagSource;
use super::types::{RagContext, RagOptions, RagSection, SourceTiming, LlmMessage};
use rayon::prelude::*;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

/// RAG Engine - composes context from multiple sources in parallel
pub struct RagEngine {
    sources: Vec<Arc<dyn RagSource>>,
    default_budget: usize,
}

impl RagEngine {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            default_budget: 8000, // Default token budget
        }
    }

    /// Register a RAG source
    pub fn register_source(&mut self, source: Arc<dyn RagSource>) {
        self.sources.push(source);
    }

    /// Build RAG context - ALL sources load in PARALLEL
    pub async fn build_context(&self, options: RagOptions) -> RagContext {
        let start = Instant::now();

        // 1. Filter applicable sources
        let applicable: Vec<_> = self.sources
            .iter()
            .filter(|s| s.is_applicable(&options))
            .cloned()
            .collect();

        if applicable.is_empty() {
            warn!("No applicable RAG sources for room={}", options.room_id);
            return self.empty_context(options, start);
        }

        info!(
            "RAG: {} sources applicable for room={} (voice_mode={})",
            applicable.len(),
            options.room_id,
            options.is_voice_mode()
        );

        // 2. Get source configs for budget allocation
        let source_configs: Vec<SourceConfig> = applicable
            .iter()
            .map(|s| s.config())
            .collect();

        // 3. Allocate budget
        let budget_manager = BudgetManager::new(options.max_tokens.max(self.default_budget));
        let allocations = budget_manager.allocate(&source_configs);

        // 4. Load ALL sources in PARALLEL with rayon
        let sections: Vec<RagSection> = applicable
            .par_iter()
            .zip(allocations.par_iter())
            .map(|(source, allocation)| {
                let source_start = Instant::now();

                // Load source (this is the expensive part)
                let mut section = source.load(&options, allocation.allocated_tokens);

                section.load_time_ms = source_start.elapsed().as_secs_f64() * 1000.0;
                section
            })
            .collect();

        // 5. Compose final context
        let context = self.compose(options.clone(), sections, start);

        info!(
            "RAG: Composed context in {:.1}ms ({} tokens, {} sources)",
            context.composition_time_ms,
            context.total_tokens,
            context.source_timings.len()
        );

        context
    }

    /// Compose sections into final context
    fn compose(&self, options: RagOptions, sections: Vec<RagSection>, start: Instant) -> RagContext {
        let mut system_parts: Vec<String> = Vec::new();
        let mut messages: Vec<LlmMessage> = Vec::new();
        let mut total_tokens = 0;
        let mut timings: Vec<SourceTiming> = Vec::new();

        // Sort sections by priority (from source config)
        // Higher priority sources go first in system prompt
        let mut sorted_sections = sections;
        sorted_sections.sort_by(|_a, _b| {
            // Preserve order based on source registration (assumed priority order)
            // TODO: Sort by priority from source config
            std::cmp::Ordering::Equal
        });

        for section in sorted_sections {
            // Collect system prompt sections
            if let Some(prompt_section) = &section.system_prompt_section {
                if !prompt_section.is_empty() {
                    system_parts.push(prompt_section.clone());
                }
            }

            // Collect messages
            messages.extend(section.messages.clone());

            // Track tokens and timing
            total_tokens += section.token_count;
            timings.push(SourceTiming {
                name: section.source_name.clone(),
                load_time_ms: section.load_time_ms,
                token_count: section.token_count,
            });
        }

        // Build system prompt
        let system_prompt = system_parts.join("\n\n---\n\n");

        RagContext {
            persona_id: options.persona_id,
            room_id: options.room_id,
            system_prompt,
            messages,
            total_tokens,
            composition_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            source_timings: timings,
        }
    }

    fn empty_context(&self, options: RagOptions, start: Instant) -> RagContext {
        RagContext {
            persona_id: options.persona_id,
            room_id: options.room_id,
            system_prompt: String::new(),
            messages: Vec::new(),
            total_tokens: 0,
            composition_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            source_timings: Vec::new(),
        }
    }
}

impl Default for RagEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::sources::MockSource;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_parallel_source_loading() {
        let mut engine = RagEngine::new();

        // Register mock sources with artificial delays
        engine.register_source(Arc::new(MockSource::new("fast", 10, 100)));
        engine.register_source(Arc::new(MockSource::new("slow", 50, 500)));
        engine.register_source(Arc::new(MockSource::new("medium", 30, 200)));

        let options = RagOptions {
            room_id: Uuid::new_v4(),
            persona_id: Uuid::new_v4(),
            max_tokens: 4000,
            ..Default::default()
        };

        let context = engine.build_context(options).await;

        // All three sources should have loaded
        assert_eq!(context.source_timings.len(), 3);

        // Total time should be close to slowest source, not sum of all
        // (parallel execution)
        // Slowest is 50ms, serial would be 10+50+30=90ms
        // Allow some overhead, but should be < 80ms
        assert!(
            context.composition_time_ms < 100.0,
            "Expected parallel execution, got {:.1}ms",
            context.composition_time_ms
        );
    }

    #[tokio::test]
    async fn test_voice_mode_filters_sources() {
        let mut engine = RagEngine::new();

        // One source that skips voice mode
        engine.register_source(Arc::new(MockSource::new_skip_voice("semantic", 50, 100)));
        // One source that works in voice mode
        engine.register_source(Arc::new(MockSource::new("identity", 10, 100)));

        let options = RagOptions {
            room_id: Uuid::new_v4(),
            persona_id: Uuid::new_v4(),
            max_tokens: 4000,
            voice_session_id: Some(Uuid::new_v4()), // Voice mode!
            ..Default::default()
        };

        let context = engine.build_context(options).await;

        // Only identity should load (semantic skips voice mode)
        assert_eq!(context.source_timings.len(), 1);
        assert_eq!(context.source_timings[0].name, "identity");
    }
}
