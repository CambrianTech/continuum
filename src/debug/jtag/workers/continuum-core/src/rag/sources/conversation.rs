//! Conversation History Source
//!
//! Loads recent conversation messages from the current room
//! Priority: 80 (high - conversation context is critical)

use super::{RagSource, SourceConfig};
use crate::rag::types::{LlmMessage, RagOptions, RagSection};
use tracing::debug;

/// Conversation history source - what's been said in this room?
pub struct ConversationHistorySource {
    // TODO: Database connection for message lookup
}

impl ConversationHistorySource {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ConversationHistorySource {
    fn default() -> Self {
        Self::new()
    }
}

impl RagSource for ConversationHistorySource {
    fn name(&self) -> &str {
        "conversation-history"
    }

    fn config(&self) -> SourceConfig {
        SourceConfig {
            name: self.name().to_string(),
            priority: 80, // High - conversation context is critical
            default_percent: 50,
            min_tokens: 500,
        }
    }

    fn is_applicable(&self, _options: &RagOptions) -> bool {
        // Always applicable - every response needs conversation context
        true
    }

    fn load(&self, options: &RagOptions, allocated_budget: usize) -> RagSection {
        debug!(
            "Loading conversation history for room {} (budget: {} tokens)",
            options.room_id, allocated_budget
        );

        // TODO: Load from database
        // Calculate max messages based on budget (assume ~30 tokens per message)
        let _max_messages = allocated_budget / 30;

        // For now, return empty messages (will be filled by DB query)
        let messages: Vec<LlmMessage> = vec![];

        RagSection {
            source_name: self.name().to_string(),
            token_count: 0, // Will be calculated from actual messages
            load_time_ms: 0.0, // Will be set by engine
            messages,
            system_prompt_section: None, // Conversation goes in messages, not system prompt
            metadata: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_conversation_always_applicable() {
        let source = ConversationHistorySource::new();
        let options = RagOptions {
            room_id: Uuid::new_v4(),
            persona_id: Uuid::new_v4(),
            ..Default::default()
        };

        assert!(source.is_applicable(&options));
    }

    #[test]
    fn test_conversation_priority() {
        let source = ConversationHistorySource::new();
        assert_eq!(source.config().priority, 80);
    }

    #[test]
    fn test_conversation_high_budget() {
        let source = ConversationHistorySource::new();
        assert_eq!(source.config().default_percent, 50); // Gets half the budget
    }
}
