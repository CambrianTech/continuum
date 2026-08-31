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

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::chat::types::{ChatSendParams, ChatSendResult};
use crate::modules::chat::ChatModule;
use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::CommandError;

/// The caller-facing wire shape — `senderId` is OPTIONAL here and resolved by
/// the priority chain the kernel doc assigns to "the caller": explicit
/// `senderId` → the authenticated calling peer → the operator self-peer (#27).
/// The kernel (`ChatModule::send`) keeps requiring a resolved UUID; this
/// boundary is the ONE place the chain lives. Before it, a human at the CLI
/// had to hand-type their own uuid to say hello — the exact "chat should be
/// easy and common" gap (caught live 2026-08-31, spawn→invite→ask flow).
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/chat/ChatSendWireParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct ChatSendWireParams {
    /// Destination room (already-resolved UUID; name lookup is `room/join`'s world).
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Sender identity. Omit to speak as yourself: the calling peer, or — for
    /// the substrate-local operator at the CLI — the operator self-peer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub sender_id: Option<Uuid>,
    /// Message text.
    pub text: String,
    /// Optional thread anchor (reply-to link on both the stored row and the wire).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub reply_to_id: Option<Uuid>,
}

crate::action_command! {
    /// Send a chat message to a room — post a message addressed to others. Params carry
    /// the destination room (`roomId`), the message `text`, an optional `replyToId`
    /// thread anchor, and an optional `senderId` — omit it to speak as yourself
    /// (personas act as themselves; the operator's CLI speaks as the operator
    /// self-peer). This is a persona's primary voice: it stores the message locally
    /// AND broadcasts it over airc.
    pub struct ChatSend { executor_slot: Arc<LateBound<CommandExecutor>> }
    name: "chat/send",
    access: AiSafe,
    params: ChatSendWireParams,
    output: ChatSendResult,
    run(this, ctx, p) => {
        let sender_id = p
            .sender_id
            .or_else(|| ctx.caller.as_ref().map(|c| c.peer_id.as_uuid()))
            .or_else(|| {
                crate::persona::operator_peer::operator_runtime()
                    .map(|rt| rt.airc().peer_id().as_uuid())
            })
            .ok_or_else(|| CommandError::Invalid(
                "no sender: pass senderId, or wait for the operator self-peer to come \
                 online this boot (it starts beside the citizens)".into(),
            ))?;
        ChatModule::from_slot(this.executor_slot.clone())
            .send(ChatSendParams {
                room_id: p.room_id,
                sender_id,
                text: p.text,
                reply_to_id: p.reply_to_id,
            })
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
