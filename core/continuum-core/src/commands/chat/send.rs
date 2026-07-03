//! `chat/send` — store + broadcast a chat message, as a typed self-routing
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand).
//!
//! Dep-holding: the command captures the owning
//! [`ChatModule`](crate::modules::chat::ChatModule)'s shared late-bound
//! [`CommandExecutor`](crate::runtime::CommandExecutor) slot. The body reconstructs
//! a transient `ChatModule` over that same slot (`from_slot`) and delegates to the
//! canonical [`ChatModule::send`](crate::modules::chat::ChatModule::send) — the
//! dual-write (data + airc) composition lives in one place. Assembled by
//! [`command_objects`](super::command_objects).

use std::sync::Arc;

use crate::modules::chat::types::{ChatSendParams, ChatSendResult};
use crate::modules::chat::ChatModule;
use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Send a chat message to a room — post a message addressed to others. Params carry
    /// the destination room (`roomId`), the already-resolved sender (`senderId`), the
    /// message `text`, and an optional `replyToId` thread anchor. This is a persona's
    /// primary voice: it stores the message locally AND broadcasts it over airc.
    pub struct ChatSend { executor_slot: Arc<LateBound<CommandExecutor>> }
    name: "chat/send",
    access: AiSafe,
    params: ChatSendParams,
    output: ChatSendResult,
    run(this, _ctx, p) => {
        ChatModule::from_slot(this.executor_slot.clone())
            .send(p)
            .await
            .map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the command carries its `chat/send` wire name (the routing key
    // every caller binds to) and stays AiSafe — chat/send WRITES, but posting a message
    // is a normal persona action, not the Owner-locked data/delete class. A regression
    // that renamed the path or narrowed access (gagging personas) is caught here.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(ChatSend::NAME, "chat/send");
        assert_eq!(ChatSend::ACCESS, AccessLevel::AiSafe);
    }
}
