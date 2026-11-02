//! A recent chat message as a mind's turn context carries it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// What a turn needs to know about a recent message. Minimal shape so
/// callers don't have to know about ChatMessageEntity.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RecentMessage.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RecentMessage {
    #[ts(type = "string")]
    pub id: Uuid,
    pub sender_name: String,
    pub text: String,
}
