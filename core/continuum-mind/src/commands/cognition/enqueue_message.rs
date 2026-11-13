//! `cognition/enqueue-message` — push a message onto a persona's priority inbox
//! (typed, dep-holding).
//!
//! Takes the domain [`InboxMessage`](crate::cognition::conversation::types::InboxMessage) itself, decoded by
//! serde at the boundary, and enqueues it on the persona's priority heap.
//! Captures the owning module's [`CognitionState`](crate::modules::cognition::CognitionState).
//!
//! `access: Internal` — host-driven cognition IPC, not a persona toolbelt verb.

use std::sync::Arc;

use uuid::Uuid;

use crate::cognition::conversation::types::InboxMessage;
use crate::modules::cognition::CognitionState;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/EnqueueMessageParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueMessageParams {
    /// Persona whose inbox receives the message.
    #[ts(type = "string")]
    pub persona_id: Uuid,
    /// The message to enqueue (decoded by serde at the boundary: typed ids and enums).
    pub message: InboxMessage,
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/EnqueueMessageResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueMessageResult {
    pub enqueued: bool,
    #[ts(type = "number")]
    pub queue_size: usize,
}

crate::action_command! {
    /// Push a message onto the persona's priority inbox. Host-invoked.
    pub struct EnqueueMessage { state: Arc<CognitionState> }
    name: "cognition/enqueue-message",
    access: Internal,
    params: EnqueueMessageParams,
    output: EnqueueMessageResult,
    run(this, _ctx, p) => {
        let inbox_msg = p.message;

        let persona = this.state.get_or_create_persona(p.persona_id);
        persona.inbox.enqueue(inbox_msg);

        Ok(EnqueueMessageResult {
            enqueued: true,
            queue_size: persona.inbox.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. enqueue-message is host-driven
    // cognition IPC, so it is Internal — registered and grid-routable, never a
    // remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(EnqueueMessage::NAME, "cognition/enqueue-message");
        assert_eq!(EnqueueMessage::ACCESS, AccessLevel::Internal);
    }
}
