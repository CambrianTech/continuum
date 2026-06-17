//! Substrate-side re-export of the shared `airc-protocol` event wire types.
//!
//! The wire envelopes ([`AircEventSubscribe`], [`AircEventSubscribeAck`],
//! [`AircEventDeliver`], [`AircEventUnsubscribe`], [`AircEventUnsubscribeAck`])
//! and the header / body-hint constants live in `airc-protocol`. This
//! module exists so substrate-internal code keeps importing from
//! `crate::routing::airc_event_protocol` after the extraction (no churn
//! in callers).
//!
//! Unlike the command protocol there's no substrate-coupled helper here
//! — the event envelopes are pure wire shape with no `RouteDecision` /
//! `RouteKind` dependency.

pub use continuum_airc_protocol::event::{
    AircEventDeliver, AircEventPublish, AircEventPublishAck, AircEventSubscribe,
    AircEventSubscribeAck, AircEventUnsubscribe, AircEventUnsubscribeAck, EVENT_ACK_BODY_HINT,
    EVENT_DELIVER_BODY_HINT, EVENT_PUBLISH_BODY_HINT, EVENT_SUBSCRIBE_BODY_HINT,
    EVENT_UNSUBSCRIBE_BODY_HINT, HEADER_EVENT_KIND, HEADER_EVENT_SUBSCRIPTION_ID,
    HEADER_EVENT_TOPIC,
};
