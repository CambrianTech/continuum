//! continuum-positron — Continuum's positron substrate.
//!
//! Positron is the cross-language UI contract: substrates produce
//! `ViewState`, hosts render it, AI observers perceive the same state
//! humans see. This crate is the SUBSTRATE side for continuum —
//! *projecting* typed `StateEnvelope`s from the state airc owns and
//! consuming typed `CommandEnvelope`s through `Commands.execute`.
//!
//! ## State ownership: airc is the source, this crate is a projection
//!
//! airc owns identity, rooms, presence, and the generic scoped-state /
//! wall layer (`[[airc-native-identity-rooms-security]]`,
//! `[[airc-generic-per-user-room-state]]`): `room_id == airc RoomId`,
//! the roster is airc presence (surfaced through `RoomRosterSource`),
//! room messages ride airc's room event stream, and per-(user, room)
//! metadata ("where was I last", the menu cursor, walls) lives in
//! airc's `scoped_state` bag keyed by `uir:<peer>:<room>`.
//!
//! So the typed payloads here (`ChatViewState`, …) are a *view* onto
//! airc-owned state, and [`SubstrateStateCache`] is a snapshot-
//! projection cell for the resync contract — NOT a parallel source of
//! truth. Holding a second mutable copy of room/wall/scoped state would
//! re-introduce exactly the drift the airc scoped-state layer exists to
//! kill (Joel's "SAME LAYER" call — a widget, a persona's RAG, and the
//! CLI all read/write the EXACT same airc row; positron never keeps its
//! own copy). The event source that drives [`Substrate::store`] is
//! airc's stream (roster / message / wall / scoped-state changes), not
//! a continuum-internal bus.
//!
//! The resync `last_seen` revision cursor is itself per-(user, room)
//! state — the same primitive as the menu's sticky cursor — so it can
//! persist into airc's `scoped_state` bag rather than a bespoke store
//! (deferred to the wire-binding slice; the session task holds it in
//! memory per-connection until then).
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
//!   `Value::Object`. The kind is the view's own `KIND` const (a
//!   `&'static str` owned by the type), single-sourced through
//!   `ViewState::kind()` — never re-stringified at a call site.
//! - `[[fallbacks-are-illegal-fail-loud]]`: kinds are OPEN and self-
//!   registered — each `ViewState` owns its `KIND`, like a self-routing
//!   command; there is NO central kind enum to edit when a view is
//!   added (that closed catalog was the same central-registry anti-
//!   pattern the command layer already deleted). A kind without a
//!   registered renderer/builder fails loud at the dispatch seam ("no
//!   renderer registered for kind X"), never silently coerced to a
//!   default.
//! - `[[shared-decode-per-persona-perspective]]`: the substrate decode
//!   (event → typed payload) runs ONCE per arrival; per-observer
//!   perspective (which observer subscribes which layer) is the cheap
//!   layer above the typed decode.
//!
//! ## Status
//!
//! Landed (slice 2D-1/2D-2, against `positron-core@v0.1.1`): the typed
//! payload schema + revision tracker + [`StateBuilder`]; the substrate
//! primitives — [`SubstrateStateCache`] (snapshot), [`Broadcast`]
//! (per-kind `watch` live fan-out), [`Substrate`] (the two behind one
//! shared `Arc`); and the per-connection state machine [`Connection`]
//! driving [`apply_subscribe`] / [`apply_observe`] / [`apply_command`]
//! under the snapshot-then-live + exact-equality skip contract.
//!
//! Landed (slice 2D-3, first step): the **async session task**
//! [`run_session`] (this crate — it's transport-generic substrate
//! orchestration, so it's unit-tested here with in-memory channels + a
//! scripted dispatcher): a future that reads `ClientMessage` from an
//! inbound channel, drives [`Connection::handle`], attaches
//! [`Broadcast`] `watch::Receiver`s for the live kinds *before* the
//! snapshot read (no lost-update window — #794's structural fix), fans
//! `Some(env)` through `ServerMessage::State` to the outbound sink, and
//! quantizes each observer's `budget_hz`.
//!
//! Remaining (slice 2D-3):
//!
//! 1. The WS adapter + production [`CommandDispatch`] impl over
//!    continuum-core's `CommandExecutor` (in `continuum-core`, which
//!    depends on this crate — the trait is the seam that keeps this
//!    crate free of a continuum-core dependency and the circular dep
//!    away).
//! 3. The **airc source wiring** (in `continuum-core`) — subscribe
//!    continuum to airc's roster / message / wall / scoped-state stream
//!    and call [`Substrate::store`] on each change, so the projection
//!    tracks the airc-owned truth (see "State ownership" above).

pub mod broadcast;
pub mod cache;
pub mod chat;
pub mod connection;
pub mod dispatch;
pub mod foundry;
pub mod kanban;
pub mod nav;
pub mod observer;
pub mod revisions;
pub mod scoping;
pub mod session;
pub mod system_metrics;
pub mod session_task;
pub mod state;
pub mod substrate;
pub mod wall;

pub use broadcast::Broadcast;
pub use cache::SubstrateStateCache;
pub use chat::{
    ChatMessageView, ChatViewState, Loadout, Provenance, RosterSlotView, RosterViewState,
    SenderKind,
};
pub use connection::Connection;
pub use dispatch::{apply_command, CommandDispatch};
pub use kanban::{
    KanbanCardState, KanbanCardView, KanbanLaneState, KanbanLaneView, KanbanPriority,
    KanbanPullRequest, KanbanViewState,
};
pub use observer::{apply_observe, ObserverRegistration};
pub use revisions::Revisions;
pub use session::{apply_subscribe, Subscription};
pub use session_task::run_session;
pub use state::StateBuilder;
pub use substrate::Substrate;
pub use wall::{WallPostView, WallViewState};

// Re-export positron's wire + session types so consumers get them
// under one path. The typed payloads in this crate fill
// `StateEnvelope.payload`; the session protocol's frames are what
// substrate-side handlers consume + emit.
pub use positron_core::session::{ClientMessage, KindRevision, ServerMessage};
pub use positron_core::wire::{CommandEnvelope, CommandSource, StateEnvelope, StateLayer};
