//! airc-protocol — wire-shape types for the substrate's airc command +
//! event protocols.
//!
//! Two ends speak this protocol: continuum-core (the substrate, both
//! server-side handler and cross-grid transport) and continuum-client
//! (the client lib that CLI + mobile SDKs consume). Living in one shared
//! crate prevents wire drift between client and server.
//!
//! Substrate-internal coupling — turning a `RouteDecision` into an
//! `AircCommandRequest`, the cross-grid `Transport` impl, the peer-side
//! handler — stays in continuum-core. This crate only owns the
//! serializable wire shapes.

pub mod command;
pub mod event;

pub use command::{
    AircCommandRequest, AircCommandResponse, COMMAND_REQUEST_BODY_HINT, COMMAND_RESPONSE_BODY_HINT,
    DEFAULT_COMMAND_DEADLINE, HEADER_COMMAND_ENV, HEADER_COMMAND_KIND, HEADER_COMMAND_PATH,
    HEADER_COMMAND_STATUS, HEADER_CONTINUUM_BODY_HINT, KIND_BROADCAST, KIND_LOCAL, KIND_PEER,
    KIND_ROOM,
};
pub use event::{
    AircEventDeliver, AircEventSubscribe, AircEventSubscribeAck, AircEventUnsubscribe,
    AircEventUnsubscribeAck, EVENT_ACK_BODY_HINT, EVENT_DELIVER_BODY_HINT,
    EVENT_SUBSCRIBE_BODY_HINT, EVENT_UNSUBSCRIBE_BODY_HINT, HEADER_EVENT_KIND,
    HEADER_EVENT_SUBSCRIPTION_ID, HEADER_EVENT_TOPIC,
};
