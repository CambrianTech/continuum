//! Public input types for `analyze`.
//!
//! Kept in its own file so the orchestration and prompt layers can edit
//! independently of the wire-shape callers import. Same modularize-at-
//! layer-boundaries pattern as `cognition/tool_executor/types.rs` and
//! `inference/footprint_registry/types.rs`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// What the analyzer needs to know about a recent message. Minimal
/// shape so the service doesn't have to know about ChatMessageEntity.
///
/// Wire-exported via ts-rs because `PersonaContext` (recipe-layer
/// public surface) carries `Vec<RecentMessage>` and the TS host
/// builds it directly from chat-history queries.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecentMessage.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RecentMessage {
    #[ts(type = "string")]
    pub id: Uuid,
    pub sender_name: String,
    pub text: String,
}

/// Input to `analyze`. Caller (chat path / orchestrator) collects these
/// from the room state.
#[derive(Debug, Clone)]
pub struct AnalysisInput {
    pub message_id: Uuid,
    pub room_id: Uuid,
    /// The new message that triggered this analysis.
    pub text: String,
    /// Recent messages for context. Most-recent last.
    pub recent_history: Vec<RecentMessage>,
    /// Stable specialty identifiers in the room (e.g. ['code',
    /// 'education', 'general']). Caller pulls from the room's
    /// persona registry. The analyzer is told to produce a
    /// `suggested_angles` entry for each.
    pub known_specialties: Vec<String>,
}
