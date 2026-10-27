//! `activity/*` — creating a room from a recipe, and the explicit verbs that
//! change a room's standing. **Task #274.**
//!
//! ## Why this module exists
//!
//! From ten thousand feet: **a recipe is a content-type and a room is content.**
//! The recipe is registered once and stamps out many instances; each room is one
//! document — its own identity, its own participants, its own history. Everything
//! is a room: a chat, a DM, a benchmark run, a settings pane, a SCADA tab, a doc
//! tab, a game someone asked for. That equivalence — `recipe → activity → airc
//! room` — is the whole mechanism by which an arbitrary idea becomes a shared
//! positronic substrate humans and citizens work in together, because the room is
//! the coordination layer that everything else hangs off.
//!
//! Three acts that are NOT the same act, and must not be fused:
//!
//! | act | what it means |
//! |---|---|
//! | **spawn** | the content exists — stamp the type into an instance |
//! | **subscribe** | you are a party to it; comes WITH the spawn, for every member |
//! | **open** | you are looking at it — a tab IS an opened room |
//!
//! You can be subscribed to a room for weeks without opening it, the way an unread
//! document still belongs to you. The UI typically spawns-and-opens in one gesture,
//! which is convenience, not identity.
//!
//! Until this module, there was no verb that turned a recipe into a room. The
//! manifest layer ([`crate::experience`]) could project a recipe, and airc could
//! host a room, and **nothing joined them**. So rooms got made by hand — and a
//! hand-made room carries no recipe and no purpose. That is not a hypothetical:
//! a bring-up room made by hand for one model was still collecting citizens two
//! weeks after its activity finished, because reusing it was easier than making a
//! new one.
//!
//! ## Nothing here is automatic
//!
//! No timer expires, archives, or deletes a room. **A room is the durable record of
//! its activity** — a benchmark run's room IS that run's evidence, and a room that
//! vanished on its own would take the evidence with it. Standing changes only when
//! a person or a citizen invokes a verb, and a protected room refuses deletion
//! outright. Sleeping, leaving and rejoining stay what they already are in airc
//! (`part` / `join`) and are not lifecycle events at all.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use airc_core::RoomId;
use airc_lib::Airc;

use crate::experience::standing::{project_standing, RoomStanding, STANDING_WALL_CATEGORY};
use crate::persona::room_resolve::resolve_room;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

/// The wall category + typed body that carry a room's recipe binding.
///
/// Both live in [`crate::experience::binding`] — with the READER, not with this
/// writer. A category const and a payload shape owned by the only code that
/// writes them is how the binding spent its whole life un-read: nothing outside
/// this module could name what it was looking for.
pub use crate::experience::binding::{RoomRecipeBinding, RECIPE_WALL_CATEGORY};

mod caller_airc;
pub use caller_airc::*;
mod spawn_activity_room;
pub use spawn_activity_room::*;

#[cfg(test)]
mod tests;
