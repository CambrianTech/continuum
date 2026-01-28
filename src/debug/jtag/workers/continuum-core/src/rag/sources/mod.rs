//! RAG Sources
//!
//! Each source implements the RagSource trait for parallel loading

mod identity;
mod conversation;

pub use identity::PersonaIdentitySource;
pub use conversation::ConversationHistorySource;

use super::budget::SourceConfig;
use super::types::{RagOptions, RagSection};

/// Trait for RAG sources - implement this to add new sources
pub trait RagSource: Send + Sync {
    /// Source name (for logging/debugging)
    fn name(&self) -> &str;

    /// Source configuration (priority, budget allocation)
    fn config(&self) -> SourceConfig;

    /// Check if this source applies to the current context
    fn is_applicable(&self, options: &RagOptions) -> bool;

    /// Load content from this source
    /// This is called in parallel with other sources via rayon
    fn load(&self, options: &RagOptions, allocated_budget: usize) -> RagSection;
}

/// Mock source for testing parallel loading
#[cfg(test)]
pub struct MockSource {
    name: String,
    delay_ms: u64,
    tokens: usize,
    skip_voice: bool,
}

#[cfg(test)]
impl MockSource {
    pub fn new(name: &str, delay_ms: u64, tokens: usize) -> Self {
        Self {
            name: name.to_string(),
            delay_ms,
            tokens,
            skip_voice: false,
        }
    }

    pub fn new_skip_voice(name: &str, delay_ms: u64, tokens: usize) -> Self {
        Self {
            name: name.to_string(),
            delay_ms,
            tokens,
            skip_voice: true,
        }
    }
}

#[cfg(test)]
impl RagSource for MockSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn config(&self) -> SourceConfig {
        SourceConfig {
            name: self.name.clone(),
            priority: 50,
            default_percent: 20,
            min_tokens: 100,
        }
    }

    fn is_applicable(&self, options: &RagOptions) -> bool {
        // Skip voice mode if configured
        if self.skip_voice && options.is_voice_mode() {
            return false;
        }
        true
    }

    fn load(&self, _options: &RagOptions, _allocated_budget: usize) -> RagSection {
        // Simulate load time
        std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));

        RagSection {
            source_name: self.name.clone(),
            token_count: self.tokens,
            load_time_ms: self.delay_ms as f64,
            messages: vec![],
            system_prompt_section: Some(format!("Mock content from {}", self.name)),
            metadata: Default::default(),
        }
    }
}
