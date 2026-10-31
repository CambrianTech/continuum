//! `cognition/cache-message` — push a message into a persona's recent-message ring
//! buffer (typed, dep-holding).
//!
//! The recent-message cache backs echo-chamber detection and post-inference adequacy
//! checks. This command captures the owning
//! [`CognitionModule`](crate::modules::cognition::CognitionModule)'s shared
//! [`CognitionState`](crate::modules::cognition::CognitionState) and pushes onto the
//! per-persona [`RecentMessageCache`](crate::persona::message_cache::RecentMessageCache),
//! lazily creating the persona's cognition via `get_or_create_persona`.
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::cognition::CognitionState;
use crate::persona::message_cache::{CachedMessage, SenderCategory};
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CacheMessageParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CacheMessageParams {
    /// Persona whose recent-message cache receives the message.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// Room the message belongs to (per-room ring buffer).
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// The message's own id.
    #[ts(type = "string")]
    pub message_id: Uuid,
    /// Sender's id.
    #[ts(type = "string")]
    pub sender_id: Uuid,
    /// Wire sender-type ("human" | "ai" | "persona" | "agent" | "system" | "bot").
    /// Projected onto the gating category (human vs AI); unknown values fail loud.
    pub sender_type: String,
    /// Sender's display name.
    pub sender_name: String,
    /// Message text (defaults to empty).
    #[serde(default)]
    pub content: String,
    /// Message timestamp in epoch milliseconds.
    #[ts(type = "number")]
    pub timestamp: u64,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/CacheMessageResult.ts"
)]
pub struct CacheMessageResult {
    pub success: bool,
    pub cached: bool,
}

crate::action_command! {
    /// Push a message into a persona's recent-message ring buffer (per room). Backs
    /// echo-chamber detection and adequacy checks. Host-invoked; not a persona verb.
    pub struct CacheMessage { state: Arc<CognitionState> }
    name: "cognition/cache-message",
    access: Internal,
    params: CacheMessageParams,
    output: CacheMessageResult,
    run(this, _ctx, p) => {
        let msg = CachedMessage {
            id: p.message_id,
            sender_id: p.sender_id,
            sender_type: SenderCategory::from_wire(&p.sender_type).map_err(CommandError::Invalid)?,
            sender_name: p.sender_name,
            content_text: p.content,
            timestamp_ms: p.timestamp,
        };
        this.state
            .get_or_create_persona(p.persona_id)
            .message_cache
            .push(p.room_id, msg);
        Ok(CacheMessageResult { success: true, cached: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. cache-message is host-driven
    // cognition IPC (feeds the recent-message cache), so it is Internal — registered
    // and grid-routable, never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(CacheMessage::NAME, "cognition/cache-message");
        assert_eq!(CacheMessage::ACCESS, AccessLevel::Internal);
    }
}
