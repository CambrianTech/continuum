//! Substrate-wide grid routing primitives — the universal addressing
//! layer every consumer (commands, events, debug, observability)
//! reaches through.
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md` (Slice P):
//! every load-bearing operation has a `CommandUri` address; the same
//! grammar serves command dispatch, log-event tagging, debug pokes,
//! probe stream routing, and event subscription. ONE primitive, N
//! consumers — Joel's compression principle applied to the
//! substrate's outermost surface.
//!
//! This module currently exposes:
//! - [`CommandUri`] — the typed enum for the grammar
//!   `airc://[peer[@node]][:env]/[path][?query][#fragment]`
//! - [`PeerRef`], [`NodeId`], [`EnvSelector`], [`UriParseError`] —
//!   the typed components
//! - parser + `Display` round-trip
//!
//! Future commits add: dispatcher hooks (`Commands.execute()` accepts
//! `CommandUri` OR bare path), transport selection
//! (`route(uri) -> TransportDispatch`), auth gate (typed `Verdict`),
//! tracing-span URI propagation, `/debug/` namespace routing,
//! `probe!`/`time!`/`stack!` macros, and the env registry +
//! `Context::environment()` accessor.

pub mod airc_command_protocol;
pub mod airc_event_adapters;
pub mod airc_event_protocol;
pub mod airc_event_publisher;
pub mod airc_event_transport;
pub mod airc_transport;
pub mod auth_policy;
pub mod capped_appender;
pub mod command_handler;
pub mod command_uri;
pub mod environment;
pub mod epoch_watermark;
pub mod grant_issuance;
pub mod grid_capability;
pub mod grid_trust_policy;
#[macro_use]
pub mod macros;
pub mod presented_grant_store;
pub mod probe_file_sink;
pub mod probe_router;
pub(crate) mod probe_span_meta;
pub mod route_decision;
pub mod tracing_init;
pub mod transport;
pub mod uri_layer;
pub mod verdict;

pub use airc_command_protocol::{
    AircCommandRequest, AircCommandResponse, COMMAND_REQUEST_BODY_HINT, COMMAND_RESPONSE_BODY_HINT,
    HEADER_COMMAND_ENV, HEADER_COMMAND_KIND, HEADER_COMMAND_PATH, HEADER_COMMAND_STATUS,
    HEADER_CONTINUUM_BODY_HINT,
};
pub use airc_event_adapters::{
    EventPublishAdapter, EventSubscribeAdapter, EventUnsubscribeAdapter, PUBLISH_ADAPTER_NAME,
    SUBSCRIBE_ADAPTER_NAME, UNSUBSCRIBE_ADAPTER_NAME,
};
pub use airc_event_protocol::{
    AircEventDeliver, AircEventPublish, AircEventPublishAck, AircEventSubscribe,
    AircEventSubscribeAck, AircEventUnsubscribe, AircEventUnsubscribeAck, EVENT_ACK_BODY_HINT,
    EVENT_DELIVER_BODY_HINT, EVENT_PUBLISH_BODY_HINT, EVENT_SUBSCRIBE_BODY_HINT,
    EVENT_UNSUBSCRIBE_BODY_HINT, HEADER_EVENT_KIND, HEADER_EVENT_SUBSCRIPTION_ID,
    HEADER_EVENT_TOPIC,
};
pub use airc_event_publisher::{
    build_deliver_frame, build_publish_ack, build_subscribe_ack, build_unsubscribe_ack,
    matches_filter, parse_publish_envelope, parse_subscribe_envelope, parse_unsubscribe_envelope,
    ActiveSubscription, AircEventPublisher, EventPublisherState, MatchedSubscription,
    ParsedPublish, ParsedSubscribe, ParsedUnsubscribe,
};
pub use airc_event_transport::{
    AircEventTransport, EventSubscription, DEFAULT_DELIVERY_QUEUE_CAPACITY, DEFAULT_EVENT_DEADLINE,
};
pub use airc_transport::{AircTransport, LateBoundAircTransport, DEFAULT_DEADLINE};
pub use auth_policy::{
    defer_path_prefix, deny_path_prefix, AllowAllPolicy, AuthPolicy, CallerIdentity, CallerSource,
    ClosurePolicy,
};
pub use command_handler::{CommandRequestHandler, ParsedEnvelope, HANDLER_NAME};
pub use command_uri::{CommandUri, NodeId, PeerRef, UriParseError};
pub use environment::{EnvironmentId, WellKnownEnv};
pub use grid_trust_policy::{caller_trust, GridTrustAuthPolicy, PeerTrustSource};
pub use probe_file_sink::{
    JsonlProbeFileSink, ProbeFileSinkError, DEFAULT_MAX_LOG_FILES, ENV_PROBE_CLASSES, ENV_PROBE_DIR,
};
pub use probe_router::{ProbeEvent, ProbeRouterLayer, DEFAULT_CHANNEL_CAPACITY};
pub use route_decision::{route, RouteDecision, RouteKind};
pub use tracing_init::{
    install_probe_tracing, installed_probe_router, ProbeInstall, ProbeTracingConfig,
};
pub use transport::{ClosureTransport, NotImplementedRemoteTransport, Transport};
pub use uri_layer::{current_uri_chain, UriCaptureLayer, UriFrame};
pub use verdict::{DeferredReason, ForbiddenReason, Verdict};
