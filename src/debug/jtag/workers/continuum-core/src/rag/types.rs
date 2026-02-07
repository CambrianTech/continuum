//! RAG Core Types
//!
//! Single source of truth for RAG types - exported to TypeScript via ts-rs
//! TypeScript should import from shared/generated/rag/index.ts

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Message role in conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../shared/generated/rag/MessageRole.ts")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

/// LLM message format
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/LlmMessage.ts")]
pub struct LlmMessage {
    pub role: MessageRole,
    pub content: String,
        pub name: Option<String>,
        pub timestamp: Option<i64>,
}

/// Section loaded by a RAG source (internal, not exported to TS)
#[derive(Debug, Clone, Default)]
pub struct RagSection {
    pub source_name: String,
    pub token_count: usize,
    pub load_time_ms: f64,
    pub messages: Vec<LlmMessage>,
    pub system_prompt_section: Option<String>,
    pub metadata: RagMetadata,
}

/// Metadata attached to RAG sections
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/RagMetadata.ts")]
pub struct RagMetadata {
    /// Additional metadata as key-value pairs
    #[ts(type = "Record<string, unknown>")]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// Options for RAG context building
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/RagOptions.ts")]
pub struct RagOptions {
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub max_tokens: usize,
    #[ts(optional, type = "string")]
    pub voice_session_id: Option<Uuid>,
    pub skip_semantic_search: bool,
    #[ts(optional)]
    pub current_message: Option<String>,
}

impl RagOptions {
    pub fn is_voice_mode(&self) -> bool {
        self.voice_session_id.is_some()
    }
}

/// Timing info for each source - performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/SourceTiming.ts")]
pub struct SourceTiming {
    pub name: String,
    pub load_time_ms: f64,
    pub token_count: usize,
}

/// Complete RAG context ready for LLM - the output of RAG engine
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/rag/RagContext.ts")]
pub struct RagContext {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub system_prompt: String,
    pub messages: Vec<LlmMessage>,
    pub total_tokens: usize,
    pub composition_time_ms: f64,
    pub source_timings: Vec<SourceTiming>,
}

/// Budget allocation for a source (internal, not exported)
#[derive(Debug, Clone)]
pub struct BudgetAllocation {
    pub source_name: String,
    pub allocated_tokens: usize,
    pub priority: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_mode_detection() {
        let mut opts = RagOptions::default();
        assert!(!opts.is_voice_mode());

        opts.voice_session_id = Some(Uuid::new_v4());
        assert!(opts.is_voice_mode());
    }

    #[test]
    fn test_llm_message_serialization() {
        let msg = LlmMessage {
            role: MessageRole::User,
            content: "Hello".to_string(),
            name: Some("Joel".to_string()),
            timestamp: Some(1234567890),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }
}
