//! `chat/history` — a room's DURABLE transcript, read from the airc daemon's
//! own store (the `InboxRequest` bounded query), as a typed self-routing
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand).
//!
//! ## Why this verb exists (the missing read half, found 2026-08-31)
//!
//! The daemon has always been the durable record of a room: every chat line,
//! every radiated 💭 intent and ⚙ act receipt lands in its per-channel
//! transcript with replay cursors. But the ONLY durable read the interface
//! had was `chat/poll` over the operator `chat_messages` collection — which
//! citizens' airc-published speech never touches. Result, measured live: a
//! solve room with 67 radiated receipts answered `chat/poll` with ZERO rows,
//! so clicking into a run showed only the live tail and none of the story.
//! The store existed; the verb didn't ([[when-the-substrate-lacks-a-verb-build-the-verb]]).
//!
//! `chat/history` asks the daemon for the newest `limit` events on the room's
//! channel (`since: None` = most recent window — the hydrator's shape) and
//! decodes them through the SAME seams the live bridge uses
//! ([`decode_wire_event`] + the chat-transcript recovery in `realtime_wire`),
//! so history and live rows can never disagree about what a message is.

//! ## Where the page comes from (changed 2026-09-04)
//!
//! It first asked the daemon for the newest `limit` events on the channel
//! (all kinds) and kept the conversation-shaped ones. Measured live: a busy
//! run room answered 2 rows for `limit: 40` — the daemon's window is a ring
//! the board's System events flood, so almost nothing conversational survives
//! the cut. Meanwhile the core's own chat store has been the projection of the
//! wire since `airc.chat.projected` (every plain airc message — chat lines,
//! 💭 intents, ⚙ receipts — lands in `chat_messages` under its EVENT id), so
//! the premise above ("citizens' speech never touches chat_messages") is no
//! longer true. History now reads that store through the same seam the
//! citizens' store-backed catch-up uses (`durable_history::room_rows`): one
//! durable page for humans and citizens, and the two can never disagree.
use crate::sdk_codegen::CommandError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/chat/ChatHistoryParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryParams {
    /// The room whose transcript to read.
    #[ts(type = "string")]
    pub room_id: uuid::Uuid,
    /// Max messages to return. Defaults to 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
    /// The scroll-back cursor: the oldest message the reader already holds. The page
    /// is the `limit` messages just before it; absent = the newest page. Without it
    /// every scroll-up re-read the newest page, the reader deduped it to nothing and
    /// took the room's history for exhausted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub before_message_id: Option<uuid::Uuid>,
    /// The page-down cursor: the newest message the reader holds when it dropped the
    /// rows below it (a reader parked far up keeps a bounded span). The page is the
    /// `limit` messages just after it. Mutually exclusive with `beforeMessageId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub after_message_id: Option<uuid::Uuid>,
}

impl ChatHistoryParams {
    /// The page the cursors name; both at once has no coherent order.
    fn page(&self) -> Result<crate::persona::durable_history::RoomPage, CommandError> {
        use crate::persona::durable_history::RoomPage;
        match (self.before_message_id, self.after_message_id) {
            (Some(_), Some(_)) => Err(CommandError::Invalid(
                "chat/history: beforeMessageId and afterMessageId are mutually exclusive — page one direction at a time"
                    .to_string(),
            )),
            (Some(anchor), None) => Ok(RoomPage::Before(anchor)),
            (None, Some(anchor)) => Ok(RoomPage::After(anchor)),
            (None, None) => Ok(RoomPage::Newest),
        }
    }
}

/// One durable transcript row — the wire twin of the live `chat:posted`
/// fold, deliberately shaped like `chat/poll`'s rows so the client's
/// history projection consumes either without a second parser.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/chat/ChatHistoryMessage.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryMessage {
    /// The event's durable id (the daemon's event id — stable across reads).
    pub id: String,
    /// The sender's peer uuid — typed, not text ([[uuids-are-not-strings-and-never-hand-drawn]]).
    #[ts(type = "string")]
    pub sender_id: uuid::Uuid,
    /// Message text (chat line, 💭 intent, or ⚙ receipt block — the
    /// transcript carries the WORK, not just the words).
    pub text: String,
    /// Unix ms.
    #[ts(type = "number")]
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/chat/ChatHistoryResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryResult {
    pub count: u32,
    /// Oldest → newest within the returned window.
    pub messages: Vec<ChatHistoryMessage>,
}

crate::action_command! {
    /// Read a room's durable transcript — chat lines AND radiated work receipts (💭/⚙),
    /// the full story of the activity — from the core's chat store, the projection of
    /// the wire (the same page the citizens' catch-up reads).
    pub struct ChatHistory;
    name: "chat/history",
    access: AiSafe,
    params: ChatHistoryParams,
    output: ChatHistoryResult,
    run(_this, _ctx, p) => {
        let limit = p.limit.unwrap_or(50).min(500) as usize; // JUSTIFIED unwrap_or: the declared wire default (param doc says 50)
        let rows = crate::persona::durable_history::room_rows(p.room_id, p.page()?, limit)
            .await
            .map_err(|e| CommandError::Internal(format!(
                "chat/history: durable page failed for room {}: {e}",
                p.room_id
            )))?;
        // chat/poll already returns the page in its (timestamp, id) order.
        let messages: Vec<ChatHistoryMessage> = rows
            .into_iter()
            .map(|r| ChatHistoryMessage {
                id: r.id.to_string(),
                sender_id: r.sender,
                text: r.text,
                timestamp: r.occurred_at_ms as i64,
            })
            .collect();
        Ok(ChatHistoryResult { count: messages.len() as u32, messages })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the routing key + access tier of the ONE durable-transcript
    // read verb. Renaming the path orphans every hydrator; narrowing access re-creates
    // the everyone-offline class (a read-only room transcript is AiSafe by definition).
    #[test]
    fn history_is_aisafe_under_its_wire_name() {
        assert_eq!(ChatHistory::NAME, "chat/history");
        assert_eq!(ChatHistory::ACCESS, AccessLevel::AiSafe);
    }

    // what this catches: the cursors' wire names. The web sends `beforeMessageId`
    // and `afterMessageId`; a snake_case or misspelled field deserializes to None, and
    // every scroll-up silently re-reads the newest page (the bug this field fixed).
    #[test]
    fn the_cursors_arrive_under_their_camel_case_wire_names() {
        let anchor = uuid::Uuid::new_v4();
        let p: ChatHistoryParams = serde_json::from_value(serde_json::json!({
            "roomId": uuid::Uuid::new_v4(), "beforeMessageId": anchor, "limit": 50,
        }))
        .expect("the web's chat/history params parse");
        assert_eq!(p.page().ok(), Some(crate::persona::durable_history::RoomPage::Before(anchor)));
        let down: ChatHistoryParams = serde_json::from_value(serde_json::json!({
            "roomId": uuid::Uuid::new_v4(), "afterMessageId": anchor,
        }))
        .expect("the page-down params parse");
        assert_eq!(down.page().ok(), Some(crate::persona::durable_history::RoomPage::After(anchor)));
    }

    // what this catches: both cursors at once silently picking one direction. The
    // request must be refused before any read.
    #[test]
    fn both_cursors_at_once_are_refused() {
        let anchor = uuid::Uuid::new_v4();
        let p: ChatHistoryParams = serde_json::from_value(serde_json::json!({
            "roomId": uuid::Uuid::new_v4(), "beforeMessageId": anchor, "afterMessageId": anchor,
        }))
        .expect("parses");
        assert!(matches!(p.page(), Err(CommandError::Invalid(_))));
    }
}
