//! The ONE inbound decode point: an airc event becomes either a typed
//! [`ProtocolEnvelope`] (a peer command or event-protocol frame) or a
//! [`ConversationEvent`] (something said or done in a room), decided from
//! HEADER facts only.
//!
//! Headers exist so middleware routes without reading payloads (Joel,
//! 2026-10-30: "headers were designed for middleware so everyone's not just
//! seeing random event traffic ... what if we needed to encrypt?"). The
//! classifier reads the envelope's headers, its addressee
//! ([`MentionTarget`]), its sender and its room, and nothing else: the
//! payload stays opaque until the one consumer the headers route it to
//! decodes it (command ingress for an addressed command, a mind's perception
//! for conversation). A guard test below fails the build if this module
//! starts reading a payload.
//!
//! Why the split is a TYPE (incident 2026-10-30 00:28-00:38Z): an untrusted
//! peer published `{"kind":"peer","path":"persona/placement/reserve",...}`
//! request frames into #general. The perception feed took every room event,
//! so the same frames reached two residents' minds as room speech. Perception
//! now takes only a [`ConversationEvent`], and the only constructor is
//! [`classify`], so a protocol frame cannot reach a mind by construction.
//!
//! What marks a protocol frame, all of it outside the payload:
//! - the transport kind: airc's typed `Command` / `CommandResult` frames (airc#1620).
//!   Commands are received ONLY through the identity's addressed inbox
//!   (`continuum_kernel::routing::addressed_inbox`); one that also surfaces on a room
//!   stream is routed away here and handled by nothing on the room path;
//! - `continuum.body_hint` ([`HEADER_CONTINUUM_BODY_HINT`]): the event protocol's frames
//!   (subscribe / deliver / publish / ack);
//! - `airc.correlation_id` ([`HEADER_AIRC_CORRELATION_ID`]): airc's request/reply
//!   pairing, protocol even without a continuum hint.

use std::sync::Arc;

use airc_core::{MentionTarget, PeerId, RoomId, TranscriptEvent, TranscriptKind};
use airc_protocol::HEADER_AIRC_CORRELATION_ID;

use crate::command::HEADER_CONTINUUM_BODY_HINT;
use crate::event::{
    EVENT_ACK_BODY_HINT, EVENT_DELIVER_BODY_HINT, EVENT_PUBLISH_BODY_HINT,
    EVENT_SUBSCRIBE_BODY_HINT, EVENT_UNSUBSCRIBE_BODY_HINT,
};

/// Which protocol a frame is, by its transport kind or headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolKind {
    /// airc's typed command request (received only through the addressed inbox).
    Command,
    /// airc's typed command result.
    CommandResult,
    EventSubscribe,
    EventDeliver,
    EventUnsubscribe,
    EventPublish,
    EventAck,
    /// A `continuum.body_hint` this build does not name: still protocol, never perception.
    UnknownHint(String),
    /// On airc's request/reply bus (a correlation id) with no continuum hint.
    BusCorrelated,
}

/// Who the frame is addressed to, from the envelope's addressee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Addressing {
    /// One named peer.
    Direct(PeerId),
    /// Everyone in the room it was published to.
    RoomBroadcast,
    /// A sibling room (a cross-room reference).
    SiblingRoom(RoomId),
}

impl Addressing {
    fn of(target: &MentionTarget) -> Self {
        match target {
            MentionTarget::Peer(peer) => Self::Direct(*peer),
            MentionTarget::All => Self::RoomBroadcast,
            MentionTarget::Room(room) => Self::SiblingRoom(*room),
        }
    }
}

/// A protocol frame that surfaced on a room stream. Nothing on the room path handles
/// it: it is named for the probe and routed away from perception.
#[derive(Debug, Clone)]
pub struct ProtocolEnvelope {
    event: Arc<TranscriptEvent>,
    kind: ProtocolKind,
    addressing: Addressing,
}

impl ProtocolEnvelope {
    pub fn kind(&self) -> &ProtocolKind {
        &self.kind
    }

    pub fn addressing(&self) -> Addressing {
        self.addressing
    }

    /// The transport-verified sender (airc checked the signature).
    pub fn sender(&self) -> PeerId {
        self.event.peer_id
    }

    /// The room the frame arrived on.
    pub fn channel(&self) -> RoomId {
        self.event.room_id
    }
}

/// Something said or done in a room: the only thing a mind's perception takes.
#[derive(Debug, Clone)]
pub struct ConversationEvent(Arc<TranscriptEvent>);

impl ConversationEvent {
    pub fn event(&self) -> &Arc<TranscriptEvent> {
        &self.0
    }
}

/// One inbound room event, classified once.
#[derive(Debug, Clone)]
pub enum InboundEvent {
    Protocol(ProtocolEnvelope),
    Conversation(ConversationEvent),
}

/// Classify a room event from its transport kind, headers and addressee. The payload
/// is not read.
pub fn classify(event: Arc<TranscriptEvent>) -> InboundEvent {
    match protocol_kind(event.kind, &event.headers) {
        Some(kind) => {
            let addressing = Addressing::of(&event.target);
            InboundEvent::Protocol(ProtocolEnvelope {
                event,
                kind,
                addressing,
            })
        }
        None => InboundEvent::Conversation(ConversationEvent(event)),
    }
}

fn protocol_kind(transport: TranscriptKind, headers: &airc_core::Headers) -> Option<ProtocolKind> {
    match transport {
        TranscriptKind::Command => return Some(ProtocolKind::Command),
        TranscriptKind::CommandResult => return Some(ProtocolKind::CommandResult),
        _ => {}
    }
    if let Some(hint) = headers.get(HEADER_CONTINUUM_BODY_HINT) {
        return Some(match hint.as_str() {
            EVENT_SUBSCRIBE_BODY_HINT => ProtocolKind::EventSubscribe,
            EVENT_DELIVER_BODY_HINT => ProtocolKind::EventDeliver,
            EVENT_UNSUBSCRIBE_BODY_HINT => ProtocolKind::EventUnsubscribe,
            EVENT_PUBLISH_BODY_HINT => ProtocolKind::EventPublish,
            EVENT_ACK_BODY_HINT => ProtocolKind::EventAck,
            other => ProtocolKind::UnknownHint(other.to_string()),
        });
    }
    headers
        .contains_key(HEADER_AIRC_CORRELATION_ID)
        .then_some(ProtocolKind::BusCorrelated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{Body, ClientId, EventId};

    fn event(
        kind: TranscriptKind,
        target: MentionTarget,
        headers: airc_core::Headers,
        body: Body,
    ) -> Arc<TranscriptEvent> {
        Arc::new(TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind,
            occurred_at_ms: 1,
            lamport: 1,
            target,
            headers,
            body: Some(body),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        })
    }

    // what this catches: the split keyed on the payload. The incident's exact payload
    // (`"kind":"peer"` JSON) on a plain room message is conversation (a line, executed by
    // nothing); airc's typed Command frame over an undecodable binary payload is
    // protocol, routed away from perception. The class follows the transport kind and
    // headers alone, so a payload can neither smuggle a command nor be needed (decrypted,
    // parsed) to route it.
    #[test]
    fn the_class_follows_the_transport_kind_and_headers_never_the_payload() {
        let peer_call = Body::Json(serde_json::json!({
            "kind": "peer", "path": "persona/placement/reserve", "params": {}
        }));
        assert!(matches!(
            classify(event(
                TranscriptKind::Message,
                MentionTarget::All,
                airc_core::Headers::new(),
                peer_call
            )),
            InboundEvent::Conversation(_)
        ));

        let me = PeerId::new();
        match classify(event(
            TranscriptKind::Command,
            MentionTarget::Peer(me),
            airc_core::Headers::new(),
            Body::Binary(vec![0xde, 0xad]),
        )) {
            InboundEvent::Protocol(envelope) => {
                assert_eq!(envelope.kind(), &ProtocolKind::Command);
                assert_eq!(envelope.addressing(), Addressing::Direct(me));
            }
            other => panic!("airc's Command kind is protocol: {other:?}"),
        }

        let mut bus = airc_core::Headers::new();
        bus.insert(HEADER_AIRC_CORRELATION_ID.into(), "c".into());
        assert!(matches!(
            classify(event(TranscriptKind::Message, MentionTarget::All, bus, Body::text("x"))),
            InboundEvent::Protocol(ref e) if e.kind() == &ProtocolKind::BusCorrelated
                && e.addressing() == Addressing::RoomBroadcast
        ));
    }

    // what this catches: the classifier starting to read payloads (Joel: decoding every
    // message for every reader is exponential failure, and impossible once encrypted).
    // The production half of this file may not touch a payload or a decoder.
    #[test]
    fn the_classifier_never_reads_a_payload() {
        let source = include_str!("inbound.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for forbidden in [
            "serde_json",
            "Deserialize",
            "from_value",
            "from_slice",
            "from_str",
            "as_text",
        ] {
            assert!(
                !production.contains(forbidden),
                "inbound classifier decodes a payload: {forbidden}"
            );
        }
        // `.body` as a field access (`continuum.body_hint`, a header name, is allowed).
        let reads_payload = production.match_indices(".body").any(|(at, _)| {
            production[at + ".body".len()..]
                .chars()
                .next()
                .is_some_and(|c| c != '_')
        });
        assert!(
            !reads_payload,
            "inbound classifier reads an event's payload field"
        );
    }
}
