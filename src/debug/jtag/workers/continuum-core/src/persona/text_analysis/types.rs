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

// --- Phase 2: Validation types ---

/// Garbage check reason codes (matches TypeScript GarbageReason exactly)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/GarbageReason.ts")]
pub enum GarbageReason {
    #[serde(rename = "")]
    None,
    #[serde(rename = "unicode_garbage")]
    UnicodeGarbage,
    #[serde(rename = "repetition")]
    Repetition,
    #[serde(rename = "encoding_errors")]
    EncodingErrors,
    #[serde(rename = "empty")]
    Empty,
    #[serde(rename = "truncation_marker")]
    TruncationMarker,
    #[serde(rename = "excessive_punctuation")]
    ExcessivePunctuation,
    #[serde(rename = "token_boundary_garbage")]
    TokenBoundaryGarbage,
    #[serde(rename = "inference_error")]
    InferenceError,
    #[serde(rename = "fabricated_conversation")]
    FabricatedConversation,
}

/// Result of garbage detection
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/GarbageCheckResult.ts")]
pub struct GarbageCheckResult {
    pub is_garbage: bool,
    pub reason: GarbageReason,
    pub details: String,
    pub score: f64,
}

/// Combined result of ALL validation gates (1 IPC call replaces 4 gates)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ValidationResult.ts")]
pub struct ValidationResult {
    /// Whether all gates passed
    pub passed: bool,
    /// Which gate failed (if any)
    #[ts(optional)]
    pub gate_failed: Option<String>,
    /// Garbage detection result
    pub garbage_result: GarbageCheckResult,
    /// Response loop detection
    pub is_response_loop: bool,
    /// Number of duplicate responses found in window
    #[ts(type = "number")]
    pub loop_duplicate_count: u64,
    /// Truncated tool call detected
    pub has_truncated_tool_call: bool,
    /// Semantic loop check result
    pub semantic_result: SemanticLoopResult,
    /// Total validation time in microseconds
    #[ts(type = "number")]
    pub total_time_us: u64,
}

// --- Phase 3: Mention detection + response cleaning types ---

/// Combined result of mention detection (1 IPC call for both checks)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/MentionCheckResult.ts")]
pub struct MentionCheckResult {
    /// Whether THIS persona is mentioned (@name, @uniqueid, or direct address at start)
    pub is_persona_mentioned: bool,
    /// Whether ANY directed @mention exists (used to prevent dog-piling)
    pub has_directed_mention: bool,
    /// Computation time in microseconds
    #[ts(type = "number")]
    pub compute_time_us: u64,
}

/// Result of response prefix cleaning
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/CleanedResponse.ts")]
pub struct CleanedResponse {
    /// Cleaned text with prefixes stripped
    pub text: String,
    /// Whether any cleaning was applied
    pub was_cleaned: bool,
    /// Computation time in microseconds
    #[ts(type = "number")]
    pub compute_time_us: u64,
}
