//! `chat/poll` — fetch recent messages for a room, as a typed self-routing
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand).
//!
//! Dep-holding: the command captures the owning
//! [`ChatModule`](crate::modules::chat::ChatModule)'s shared late-bound
//! [`CommandExecutor`](crate::runtime::CommandExecutor) slot. The body reconstructs
//! a transient `ChatModule` over that same slot (`from_slot`) and delegates to the
//! canonical [`ChatModule::poll`](crate::modules::chat::ChatModule::poll) — one
//! implementation, shared by the command and the module's direct-method tests.
//! Assembled by [`command_objects`](super::command_objects).

use std::sync::Arc;

use crate::modules::chat::types::{ChatPollParams, ChatPollResult};
use crate::modules::chat::ChatModule;
use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Fetch recent messages for a room — read the latest conversation. Params carry
    /// the room (`roomId`, optional: omit for latest across all rooms) and how many
    /// messages to retrieve (`limit`, default 50). Pass `afterMessageId` to page
    /// forward and return only messages newer than one you've already seen.
    pub struct ChatPoll { executor_slot: Arc<LateBound<CommandExecutor>> }
    name: "chat/poll",
    access: AiSafe,
    params: ChatPollParams,
    output: ChatPollResult,
    run(this, _ctx, p) => {
        ChatModule::from_slot(this.executor_slot.clone())
            .poll(p)
            .await
            .map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the command carries its `chat/poll` wire name (the routing
    // key every caller binds to) and stays AiSafe (read-only conversation fetch). A
    // regression that renamed the path or widened access to a write level is caught here.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(ChatPoll::NAME, "chat/poll");
        assert_eq!(ChatPoll::ACCESS, AccessLevel::AiSafe);
    }
}
