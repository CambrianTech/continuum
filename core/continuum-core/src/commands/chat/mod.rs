//! `chat/<verb>` — the chat command family (typed, self-routing).
//!
//! `chat/poll` (read the latest conversation) + `chat/send` (store + broadcast a
//! message) are dep-holding [`ActionCommand`](crate::sdk_codegen::ActionCommand)s:
//! each captures the owning [`ChatModule`](crate::modules::chat::ChatModule)'s shared
//! late-bound [`CommandExecutor`](crate::runtime::CommandExecutor) slot and delegates
//! to the module's canonical method (`poll`/`send`) via `ChatModule::from_slot` — one
//! implementation, shared by the commands and the module's direct-method tests.
//!
//! Wired together by [`command_objects`], which the owning `ChatModule` calls from its
//! [`commands()`](crate::runtime::ServiceModule::commands) so both verbs reach the
//! kernel's typed object map (and thus `command_registry()`, the persona tool surface,
//! the ACL, codegen, and `uu`).

use std::sync::Arc;

use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::DynCommand;

pub mod history;
pub mod poll;
pub mod send;

use poll::ChatPoll;
use send::ChatSend;

/// The `chat/*` command objects over the module's shared late-bound executor slot.
/// Called from [`ChatModule::commands`](crate::modules::chat::ChatModule::commands).
pub fn command_objects(executor_slot: Arc<LateBound<CommandExecutor>>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(ChatPoll {
            executor_slot: executor_slot.clone(),
        }),
        Arc::new(ChatSend { executor_slot }),
    ]
}
