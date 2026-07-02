//! airc-protocol — wire-shape types for the substrate's airc command +
//! event protocols.
//!
//! Two ends speak this protocol: continuum-core (the substrate, both
//! server-side handler and cross-grid transport) and continuum-client
//! (the client lib that CLI + mobile SDKs consume). Living in one shared
//! crate prevents wire drift between client and server.
//!
//! Substrate-internal coupling — turning a `RouteDecision` into an
//! `AircCommandRequest`, the cross-grid `Transport` impl per se (the
//! struct + async I/O), the peer-side handler — stays in continuum-core.
//! This crate owns the serializable wire shapes AND the pure helpers
//! that produce/consume them (e.g. `event::resolve_subscribe`,
//! `event::decode_deliver_frame`, `event::matches_subscription`), so the
//! client and the substrate compose the SAME helpers with their
//! respective `airc_lib::Airc` handles.

pub mod command;
pub mod event;
pub mod ws;

pub use command::{
    AircCommandRequest, AircCommandResponse, COMMAND_REQUEST_BODY_HINT, COMMAND_RESPONSE_BODY_HINT,
    DEFAULT_COMMAND_DEADLINE, HEADER_COMMAND_ENV, HEADER_COMMAND_KIND, HEADER_COMMAND_PATH,
    HEADER_COMMAND_STATUS, HEADER_CONTINUUM_BODY_HINT, KIND_BROADCAST, KIND_LOCAL, KIND_PEER,
    KIND_ROOM,
};
pub use ws::{WsClientMessage, WsServerMessage};
pub use event::{
    // Helper functions — shared by substrate AND client
    decode_deliver_frame,
    decode_subscribe_ack,
    decode_unsubscribe_ack,
    matches_subscription,
    resolve_subscribe,
    resolve_unsubscribe,
    // Envelope types
    AircEventDeliver,
    AircEventSubscribe,
    AircEventSubscribeAck,
    AircEventUnsubscribe,
    AircEventUnsubscribeAck,
    // Body-hint constants
    EVENT_ACK_BODY_HINT,
    EVENT_DELIVER_BODY_HINT,
    EVENT_SUBSCRIBE_BODY_HINT,
    EVENT_UNSUBSCRIBE_BODY_HINT,
    // Header name constants
    HEADER_EVENT_KIND,
    HEADER_EVENT_SUBSCRIPTION_ID,
    HEADER_EVENT_TOPIC,
};
