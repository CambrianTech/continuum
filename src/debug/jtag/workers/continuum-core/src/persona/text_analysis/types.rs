//! Text Analysis Types
//!
//! Single source of truth for text analysis result types.
//! Exported to TypeScript via ts-rs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Result of a text similarity comparison
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/TextSimilarityResult.ts")]
pub struct TextSimilarityResult {
    /// Word + bigram Jaccard similarity (semantic-level)
    pub ngram_similarity: f64,
    /// Character bigram Jaccard similarity (character-level, for loop detection)
    pub char_similarity: f64,
    /// Computation time in microseconds
    #[ts(type = "number")]
    pub compute_time_us: u64,
}

/// Result of a semantic loop check against conversation history
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/SemanticLoopResult.ts")]
pub struct SemanticLoopResult {
    /// Whether the response should be blocked
    pub should_block: bool,
    /// Maximum similarity found against any recent message
    pub similarity: f64,
    /// Human-readable reason
    pub reason: String,
}

/// Lightweight message representation for cross-boundary transfer
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ConversationMessage.ts")]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    #[ts(optional)]
    pub name: Option<String>,
}
