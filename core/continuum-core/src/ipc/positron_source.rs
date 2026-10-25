//! The airc → positron chat projection (task #29 airc source wiring).
//!
//! ## What this is
//!
//! A passive consumer of the airc room stream on the `MessageBus`, off
//! the transport hot path — the same shape as
//! [`crate::modules::airc_bridge_directive`]. On boot it subscribes via
//! `MessageBus::receiver()` and runs a consume loop on the runtime; each
//! recognized event folds into a [`ChatViewState`] and is written to the
//! thin-client [`Substrate`] with a monotonic revision. WS sessions
//! subscribed to `kind="chat"` (see [`crate::ipc::ws`]) then see the
//! projected view stream down as a `State` frame.
//!
//! ## Why the projection exists — airc owns the truth, positron caches it
//!
//! Per `[[airc-native-identity-rooms-security]]`: airc is the source of
//! room + roster truth. The positron `Substrate` is a *projection* the
//! thin-client fleet reads — never a second store of room state. This
//! module is the seam that keeps the projection tracking airc's owned
//! stream: airc publishes room events on the bus, we fold them into the
//! renderer-shaped [`ChatViewState`], the renderer reads the snapshot.
//! The renderer never re-derives identity from ids (the widget-local
//! cache footgun #794 exists to kill) — the projection resolves a
//! sender's `sender_name`/`sender_kind`/`integrations` ONCE here, by
//! looking the id up in the roster, exactly as `ChatViewState` documents.
//!
//! ## Which airc streams map to `kind="chat"`
//!
//! Three bus streams fold onto the single existing `ChatViewState::KIND`, and
//! the split between them mirrors airc's own message/identity split:
//!
//! - **`chat:posted`** — a posted message. Deserialized into the **thin**
//!   [`AircChatPosted`] (core message facts only — no identity), appended
//!   to the room's bounded message ring. Identity is resolved from the
//!   roster at fold time.
//! - **`presence:updated`** — the room roster changed. Deserialized into
//!   [`AircPresenceUpdate`], each entry an airc member joined with its
//!   identity card (neutral `kind` + opaque `integrations`), replacing
//!   the room's roster. This is the identity lookup table for messages.
//! - **`persona:vitals`** — a persona radiated its live cognition readouts.
//!   Deserialized into [`PersonaVitalsUpdate`] and folded into that member's
//!   [`RosterSlotView::vitals`] by id — a thin overlay kept in its own map so
//!   it survives a roster-replacing presence snapshot (personas emit, this
//!   projection folds; the presence emitter stays persona-agnostic).
//!
//! Wall / coordination / kanban / widget state (task #89) are *different*
//! kinds and are deliberately out of scope here — they get their own
//! `KIND` const + payload structs when those renderers land. This slice
//! projects exactly the chat surface, no more.
//!
//! ## The input contract, and why missing fields are skipped not faked
//!
//! Per `[[strong-typing-across-boundaries]]` + `[[fallbacks-are-illegal-fail-loud]]`:
//! the projection declares the typed shape it needs
//! ([`AircChatPosted`]/[`AircPresenceUpdate`]) rather than scraping the
//! bus `Value` field-by-field. An event that does not deserialize into
//! that shape is simply **not a chat event this projection can render** —
//! it is skipped (classification, exactly like
//! `airc_bridge_directive::classify_inbound` returning `None` for
//! non-directives), never partially rendered with fabricated identity.
//! The continuum-side turn streamer (task #84) is the emitter that fills
//! this contract; until it lands, well-formed events are exercised by the
//! unit tests below, proving the fold end-to-end through the real
//! `Substrate`.
//!
//! ## Single active room (single-cache-per-kind)
//!
//! The `Substrate` cache is keyed by `kind` string alone, so it holds one
//! `chat` envelope at a time — the currently-focused room's view. On an
//! event whose `room_id` differs from the accumulator's current room, the
//! projection resets to the new room (clears ring + roster). Per-room
//! instancing (many rooms cached at once) is kind-instancing, deferred
//! — the revision key would extend from the bare kind string to a
//! `(room_id, kind)` tuple (see `continuum-positron/src/revisions.rs`).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use continuum_positron::{
    ActReceiptView, ChatMessageView, ChatViewState, Loadout, Provenance, RosterSlotView,
    RosterViewState, SenderKind, StateBuilder, Substrate,
};

use crate::experience::{Experience, ExperienceSource, Member, RecipeExperienceSource, Standing};
use crate::cognition::mind_events::{PersonaActUpdate, CHAT_HEARD, PERSONA_ACT};
use crate::persona::roster_slot::provisional_sender_name;
use crate::runtime::MessageBus;

mod rather;
pub(crate) use rather::*;
mod chat_projection;
pub use chat_projection::*;

#[cfg(test)]
mod tests;
