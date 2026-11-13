//! A room line's ARRIVAL: the store-owned record of when a message became visible to the core, in
//! commit order (PERCEPTION-FROM-THE-STORE.md, Slice P). Written in the SAME transaction as the
//! message row, by every writer of `chat_messages`; a duplicate import of one message reuses its
//! first arrival. The arrival is the header every perception consumer routes on (room, author,
//! kind, addressee) — the message body is never opened to route.

use airc_core::{MentionTarget, PeerId, RoomId, TranscriptEvent, TranscriptKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::orm::{OrmStoreError, SequenceSpec};

/// The collection's sequence and dedupe fields: `arrivalSeq` is assigned in commit order;
/// `messageId` identifies the line, so a second import of it reuses the first arrival.
pub fn arrival_sequence() -> SequenceSpec {
    SequenceSpec {
        sequence: "arrivalSeq".to_string(),
        unique: "messageId".to_string(),
    }
}

/// What kind of room line arrived, from the transport's own kind header. Exhaustive over the
/// kinds a mind can perceive; [`ArrivalKind::of`] decides every transport kind explicitly, so a
/// new one is a compile error here, never a silent default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrivalKind {
    Message,
    Attachment,
    Receipt,
    Presence,
    System,
}

impl ArrivalKind {
    /// The arrival kind of a transport kind, or `None` for substrate traffic that is not a room
    /// line (lifecycle, protocol, board and identity publications).
    pub fn of(kind: TranscriptKind) -> Option<Self> {
        match kind {
            TranscriptKind::Message => Some(Self::Message),
            TranscriptKind::Attachment => Some(Self::Attachment),
            TranscriptKind::Receipt => Some(Self::Receipt),
            TranscriptKind::Presence => Some(Self::Presence),
            TranscriptKind::System => Some(Self::System),
            TranscriptKind::SessionControl
            | TranscriptKind::PeerArrived
            | TranscriptKind::PeerDeparted
            | TranscriptKind::WireEstablished
            | TranscriptKind::WireLost
            | TranscriptKind::RoomJoined
            | TranscriptKind::RoomParted
            | TranscriptKind::SubscriptionAdvanced
            | TranscriptKind::Command
            | TranscriptKind::CommandResult
            | TranscriptKind::WallPostPublished
            | TranscriptKind::DoctrinePublished
            | TranscriptKind::IdentityPublished
            | TranscriptKind::ChannelPurposePublished => None,
        }
    }
}

/// One arrival: the routing header of one room line, numbered in commit order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "chat_arrivals")]
#[entity(index(name = "idx_chat_arrivals_message", fields = ["messageId"], unique = true))]
#[entity(index(name = "idx_chat_arrivals_seq", fields = ["arrivalSeq"], unique = true))]
pub struct Arrival {
    #[entity(primary_key)]
    pub id: Uuid,
    /// Commit order; assigned by the store inside the writing transaction, never by a caller
    /// (whatever the caller sets is overwritten).
    pub arrival_seq: u64,
    /// The message row this arrival announces (the chat store's row id).
    pub message_id: Uuid,
    #[entity(indexed)]
    pub room: RoomId,
    pub author: PeerId,
    pub kind: ArrivalKind,
    /// The one peer the line is addressed to, when it is addressed to one.
    pub direct_to: Option<PeerId>,
}

impl Arrival {
    /// A new arrival for `message_id` (its sequence is the store's to assign).
    pub fn new(
        message_id: Uuid,
        room: RoomId,
        author: PeerId,
        kind: ArrivalKind,
        direct_to: Option<PeerId>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            arrival_seq: 0,
            message_id,
            room,
            author,
            kind,
            direct_to,
        }
    }

    /// The arrival of a transport event, read from its header alone (kind, ids, addressee);
    /// `None` when the event is not a room line.
    pub fn of_event(event: &TranscriptEvent) -> Option<Self> {
        let kind = ArrivalKind::of(event.kind)?;
        let direct_to = match event.target {
            MentionTarget::Peer(peer) => Some(peer),
            MentionTarget::All | MentionTarget::Room(_) => None,
        };
        Some(Self::new(
            event.event_id.as_uuid(),
            event.room_id,
            event.peer_id,
            kind,
            direct_to,
        ))
    }

    /// The batch operation that appends this arrival under [`arrival_sequence`]; commit it in the
    /// same batch as its message row.
    pub fn append_op(&self) -> Result<crate::orm::types::BatchOperation, OrmStoreError> {
        Ok(crate::orm::types::BatchOperation {
            operation_type: crate::orm::types::BatchOperationType::Append(arrival_sequence()),
            collection: <Self as crate::orm::OrmEntity>::COLLECTION.to_string(),
            id: Some(self.id.to_string()),
            data: Some(serde_json::to_value(self).map_err(OrmStoreError::SerializeFailed)?), // storage boundary: the adapter's row format
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: routing facts read from anything but the header — an addressed line
    // losing its addressee, or a lifecycle event becoming a room line a mind perceives.
    #[test]
    fn an_arrival_is_read_from_the_transport_header_alone() {
        let peer = PeerId::new();
        let mut event = continuum_cognition::cognition::channel_digest::test_event_in(
            RoomId::new(),
            "hello",
            1,
        );
        event.target = MentionTarget::Peer(peer);
        let arrival = Arrival::of_event(&event).expect("a message is a room line"); // test: the assertion failing loudly
        assert_eq!(
            (arrival.kind, arrival.direct_to, arrival.message_id),
            (ArrivalKind::Message, Some(peer), event.event_id.as_uuid())
        );
        event.kind = TranscriptKind::PeerArrived;
        assert_eq!(
            Arrival::of_event(&event),
            None,
            "lifecycle traffic is not a room line"
        );
    }
}
