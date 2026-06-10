//! continuum-positron — Continuum's positron substrate.
//!
//! Positron is the cross-language UI contract: substrates produce
//! `ViewState`, hosts render it, AI observers perceive the same state
//! humans see. This crate is the SUBSTRATE side for continuum —
//! producing typed `StateEnvelope`s from continuum's event bus and
//! consuming typed `CommandEnvelope`s through `Commands.execute`.
//!
//! ## Why this exists
//!
//! Per ALPHA-GAP §6 (UI/Realtime Stability), the alpha P0/P1 blockers
//! are #793 (Node doesn't reconnect when Rust core restarts), #794
//! (AI messages not realtime), and #773 (browser WS reconnect). All
//! three are symptoms of the same root cause: widgets carry local
//! source-of-truth caches that drift from substrate truth across
//! transport failures. Positron's contract makes that class of bug
//! structural: widgets render from `StateEnvelope`, never from a
//! cached blob; transport reconnect resyncs state via `last_seen`
//! revision replay.
//!
//! ## Layering
//!
//! Substrate side (this crate):
//! - Consumer-typed payloads (`ChatViewState`, `RoomRosterState`, …)
//!   shipped as `ts-rs` generated types so the widget side is byte-
//!   identical without hand-mirrored TS.
//! - `Revisions` — monotonic counter per `(kind, layer)` so an
//!   Ephemeral animation churn doesn't bump a Session counter.
//! - `StateBuilder` — helper that frames a typed payload into a
//!   `positron_core::wire::StateEnvelope` with the right revision +
//!   layer + kind tag.
//!
//! Host side (positron-lit, Fable's lane): consumes `StateEnvelope`,
//! renders Lit widgets with NO widget-local source-of-truth cache.
//! Renderer is pure projection.
//!
//! Wire side (Fable's session protocol PR — `df3fb2ab`):
//! `ClientMessage::{Subscribe, Command, Observe}` /
//! `ServerMessage::State`, with `Subscribe { last_seen: [{kind,
//! revision}] }` driving the resync-on-reconnect property.
//!
//! ## Doctrines
//!
//! - `[[strong-typing-across-boundaries]]`: payload variants are typed,
//!   not stringly-tagged. `ChatViewState { messages, roster, … }` not
//!   `Value::Object`. `(kind, layer)` is a typed key for revisions, not
//!   a `String`-concatenation.
//! - `[[no-fallbacks-ever]]`: a kind without a registered builder is
//!   refused at compile time (the typed kind enum is exhaustive), not
//!   silently coerced to `Other`.
//! - `[[shared-decode-per-persona-perspective]]`: the substrate decode
//!   (event → typed payload) runs ONCE per arrival; per-observer
//!   perspective (which observer subscribes which layer) is the cheap
//!   layer above the typed decode.
//!
//! ## Status
//!
//! This crate ships the typed payload schema + revision tracker +
//! state builder. The wire transport binding (subscriber session
//! lifecycle, `last_seen` replay, command dispatch through the
//! substrate's `CommandExecutor`) lands in a follow-up slice once
//! positron's session protocol PR merges. The seam stays narrow on
//! purpose so the wire layer drops in cleanly.

pub mod broadcast;
pub mod cache;
pub mod chat;
pub mod connection;
pub mod dispatch;
pub mod kinds;
pub mod observer;
pub mod revisions;
pub mod session;
pub mod state;
pub mod substrate;

pub use broadcast::Broadcast;
pub use cache::SubstrateStateCache;
pub use chat::{ChatMessageView, ChatViewState, PersonaSlotView, SenderKind};
pub use connection::Connection;
pub use dispatch::{apply_command, CommandDispatch};
pub use kinds::{KnownKind, RevisionKey};
pub use observer::{apply_observe, ObserverRegistration};
pub use revisions::Revisions;
pub use session::{apply_subscribe, Subscription};
pub use state::StateBuilder;
pub use substrate::Substrate;

// Re-export positron's wire + session types so consumers get them
// under one path. The typed payloads in this crate fill
// `StateEnvelope.payload`; the session protocol's frames are what
// substrate-side handlers consume + emit.
pub use positron_core::session::{ClientMessage, KindRevision, ServerMessage};
pub use positron_core::wire::{CommandEnvelope, CommandSource, StateEnvelope, StateLayer};
