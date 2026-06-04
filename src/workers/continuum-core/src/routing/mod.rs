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

pub mod command_uri;
#[macro_use]
pub mod macros;

pub use command_uri::{CommandUri, EnvSelector, NodeId, PeerRef, UriParseError};
