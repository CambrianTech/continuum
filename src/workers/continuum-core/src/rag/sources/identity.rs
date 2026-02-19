//! Persona Identity Source
//!
//! Loads persona identity (name, bio, system prompt)
//! Priority: 95 (highest - identity is foundational)

use super::{RagSource, SourceConfig};
use crate::rag::types::{RagOptions, RagSection};
use tracing::debug;

/// Persona identity source - who is this AI?
pub struct PersonaIdentitySource {
    // TODO: Database connection for persona lookup
}

impl PersonaIdentitySource {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for PersonaIdentitySource {
    fn default() -> Self {
        Self::new()
    }
}

impl RagSource for PersonaIdentitySource {
    fn name(&self) -> &str {
        "persona-identity"
    }

    fn config(&self) -> SourceConfig {
        SourceConfig {
            name: self.name().to_string(),
            priority: 95, // Highest - identity is foundational
            default_percent: 10,
            min_tokens: 200,
        }
    }

    fn is_applicable(&self, _options: &RagOptions) -> bool {
        // Always applicable - every response needs identity
        true
    }

    fn load(&self, options: &RagOptions, allocated_budget: usize) -> RagSection {
        debug!(
            "Loading persona identity for {} (budget: {} tokens)",
            options.persona_id, allocated_budget
        );

        // TODO: Load from database
        // For now, return placeholder
        let system_prompt = format!(
            "## Persona Identity\n\nYou are a helpful AI assistant.\nPersona ID: {}",
            options.persona_id
        );

        let tokens = system_prompt.len() / 4; // Rough estimate

        RagSection {
            source_name: self.name().to_string(),
            token_count: tokens,
            load_time_ms: 0.0, // Will be set by engine
            messages: vec![],
            system_prompt_section: Some(system_prompt),
            metadata: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_identity_always_applicable() {
        let source = PersonaIdentitySource::new();
        let options = RagOptions {
            room_id: Uuid::new_v4(),
            persona_id: Uuid::new_v4(),
            ..Default::default()
        };

        assert!(source.is_applicable(&options));
    }

    #[test]
    fn test_identity_high_priority() {
        let source = PersonaIdentitySource::new();
        assert_eq!(source.config().priority, 95);
    }

    #[test]
    fn test_identity_loads_system_prompt() {
        let source = PersonaIdentitySource::new();
        let options = RagOptions {
            room_id: Uuid::new_v4(),
            persona_id: Uuid::new_v4(),
            ..Default::default()
        };

        let section = source.load(&options, 1000);

        assert!(section.system_prompt_section.is_some());
        assert!(section.system_prompt_section.unwrap().contains("Persona Identity"));
    }
}
