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
        // An AGENT-driven session (actorKind claim from the CLI env) speaks as
        // the AGENT self-peer — never as the human operator. Attribution is
        // identity's whole point (Joel, 2026-09-01).
        let agent_session = ctx.claimed_actor_kind.as_deref() == Some("agent");
        let sender_id = p
            .sender_id
            .or_else(|| ctx.caller.as_ref().map(|c| c.peer_id.as_uuid()))
            .or_else(|| {
                if agent_session {
                    crate::persona::operator_peer::agent_runtime()
                        .map(|rt| rt.airc().peer_id().as_uuid())
                } else {
                    crate::persona::operator_peer::operator_runtime()
                        .map(|rt| rt.airc().peer_id().as_uuid())
                }
            })
            .ok_or_else(|| CommandError::Invalid(
                "no sender: pass senderId, or wait for the operator self-peer to come \
                 online this boot (it starts beside the citizens)".into(),
            ))?;
        // ONE row per line (2026-09-03): resolve the sender's own runtime FIRST.
        // With a runtime, the say below is the wire leg and the envelope leg is
        // skipped; both used to run and every window read the operator twice.
        let runtime = crate::persona::PersonaAircRuntimeRegistry::try_global()
            .and_then(|r| r.get(sender_id))
            .or_else(|| {
                crate::persona::operator_peer::operator_runtime()
                    .filter(|rt| rt.airc().peer_id().as_uuid() == sender_id)
            })
            .or_else(|| {
                crate::persona::operator_peer::agent_runtime()
                    .filter(|rt| rt.airc().peer_id().as_uuid() == sender_id)
            });
        let chat = ChatModule::from_slot(this.executor_slot.clone());
        let params = ChatSendParams {
            room_id: p.room_id,
            sender_id,
            text: p.text.clone(),
            reply_to_id: p.reply_to_id,
        };
        let wire = if runtime.is_some() {
            crate::modules::chat::WireLeg::CallerSpeaks
        } else {
            crate::modules::chat::WireLeg::Envelope
        };
        let mut result = chat
            .send_with_wire(params.clone(), wire)
            .await
            .map_err(CommandError::Internal)?;
        // ── The DAEMON half of the voice (found live 2026-08-31) ──
        // Citizens hear rooms through the airc DAEMON transcript (`say`), and
        // `chat/history` reads the same store. Speak it through the SENDER'S
        // own runtime (persona from the registry; the operator self-peer only
        // for its own id — never misattributed). If the say fails, the
        // envelope leg runs as the fallback so the line still reaches the wire
        // — and the miss is named in `warning`, never silent.
        match runtime {
            Some(rt) => {
                let mut publish_err = crate::persona::airc_citizen::publish_text_in_room(
                    rt.airc(),
                    p.room_id,
                    &p.text,
                )
                .await
                .err();
                let is_self_peer = crate::persona::operator_peer::operator_runtime()
                    .map(|o| o.airc().peer_id().as_uuid() == sender_id)
                    .unwrap_or(false) // unwrap_or: self-peer not up this boot = the sender simply isn't it
                    || crate::persona::operator_peer::agent_runtime()
                        .map(|a| a.airc().peer_id().as_uuid() == sender_id)
                        .unwrap_or(false); // unwrap_or: same — absence of the agent peer is a fact, not an error
                if publish_err.is_some() && is_self_peer {
                    if let Some(name) =
                        crate::persona::airc_citizen::room_name_by_id(p.room_id).await
                    {
                        if rt.airc().subscribe_room(&name).await.is_ok() {
                            crate::probe!(
                                class = "chat.send.join_on_send",
                                room = %p.room_id,
                                room_name = %name,
                                sender = %sender_id,
                                "self-peer subscribed on send — its scope had no membership \
                                 for a room it deliberately addressed; retrying the say",
                            );
                            publish_err = crate::persona::airc_citizen::publish_text_in_room(
                                rt.airc(),
                                p.room_id,
                                &p.text,
                            )
                            .await
                            .err();
                        }
                    }
                }
                if let Some(e) = publish_err {
                    // The say missed: fall back to the envelope leg so the line
                    // still reaches the wire, and say why.
                    let fallback = chat
                        .broadcast_envelope(
                            result.message_id,
                            &params,
                            crate::modules::chat::now_ms(),
                        )
                        .await
                        .map_err(CommandError::Internal)?;
                    result.event_id = fallback.event_id;
                    result.warning = Some(format!(
                        "the daemon-room say failed ({e}); the line went out as an \
                         envelope instead{}",
                        fallback
                            .warning
                            .map(|w| format!(" — and that missed too: {w}"))
                            .unwrap_or_default() // unwrap_or: no second miss = nothing to append
                    ));
                }
            }
            None => {
                result.warning = Some(format!(
                    "message stored + live-fed, but no airc runtime speaks for sender \
                     {sender_id} — citizens in the room did not hear it"
                ));
            }
        }
        Ok(result)
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
