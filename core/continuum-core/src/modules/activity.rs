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

use airc_lib::Airc;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

/// The wall category that carries a room's recipe binding.
///
/// airc's own `ScopeRef` doc names the split: peer-private room state is
/// `ScopeRef::Room`, but "plan / instructions / **recipe** that every participant
/// must see" belongs on the **wall**. A recipe binding must be shared — every
/// client, human or citizen, has to agree on what this room IS — so it is a wall
/// post, not per-peer state, and not a continuum-side table shadowing the room.
pub const RECIPE_WALL_CATEGORY: &str = "recipe";

/// Resolve the CALLING peer's own airc handle so the room is created as THEIR
/// identity — the creator is a real peer, never the substrate acting anonymously.
fn caller_airc(
    registry: &PersonaAircRuntimeRegistry,
    ctx: &Ctx,
) -> Result<Arc<Airc>, CommandError> {
    let peer = ctx
        .caller
        .as_ref()
        .map(|c| c.peer_id.as_uuid())
        .ok_or_else(|| {
            CommandError::Denied(
                "activity verbs act as the caller's own airc identity, and the \
                 substrate-local operator has none in-core (the self-peer gap, task \
                 #27). Personas calling through their toolbelt act as themselves."
                    .into(),
            )
        })?;
    let rt = registry
        .get(peer)
        .ok_or_else(|| CommandError::NotFound(format!("no live airc runtime for peer {peer}")))?;
    Ok(rt.airc().clone())
}

// ─────────────────────────── activity/spawn ───────────────────────────

/// Instantiate a recipe as a new room.
pub struct ActivitySpawn {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct ActivitySpawnParams {
    /// The room's name — what people will call this instance of the activity.
    ///
    /// Name it for the ACTIVITY, not for a subsystem. A subsystem never finishes,
    /// so a room named after one reads as a permanent place and quietly becomes
    /// the room everyone reuses forever.
    pub name: String,

    /// Which recipe to build from — the `purpose` key of an authored recipe
    /// (`chat`, `benchmark`, `video-chat`, `profile`, or anything dropped into the
    /// recipes directory). The recipe decides the room's regions, verbs and layout;
    /// this command only decides that a room exists and which recipe it follows.
    pub recipe: String,

    /// Optional parent activity id — activities spawn activities, and the graph is
    /// POINTERS (parent id here, child ids on the parent), never nested blobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct ActivitySpawnResult {
    /// The airc channel id of the new room — the id every surface addresses it by.
    pub room_id: String,
    /// The room's name, as created.
    pub name: String,
    /// The recipe this room follows.
    pub recipe: String,
    /// The wall post that binds room → recipe, so the binding is auditable.
    pub binding_post_id: String,
}

#[async_trait]
impl ActionCommand for ActivitySpawn {
    const NAME: &'static str = "activity/spawn";
    const ALIASES: &'static [&'static str] = &["create_room", "spawn_activity"];
    const NATIVE: bool = true;
    /// AiSafe: making a room is the ordinary creative act of a citizen with an
    /// idea. It takes nothing from anyone — a new room is additive, and every
    /// destructive verb in this module is gated separately.
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Create a new room from a recipe. `name` is what people call this instance; \
         `recipe` is which template to build from (chat, benchmark, …). Everything is \
         a room — a chat, a benchmark run, a doc, a settings pane — so spawn one \
         whenever an idea needs its own shared space. Returns the room_id.";
    type Params = ActivitySpawnParams;
    type Output = ActivitySpawnResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: ActivitySpawnParams,
    ) -> Result<ActivitySpawnResult, CommandError> {
        let airc = caller_airc(&self.registry, ctx)?;

        // `join` IS room creation in airc: it derives the channel from the name,
        // subscribes this peer, and publishes presence. Reusing it keeps ONE room
        // birth path instead of a parallel creator that would drift from it.
        //
        // KNOWN GAP — spawning also FOCUSES. Creating a room and opening it are
        // different acts: a tab is an *opened* room, and the UI merely tends to do
        // both at once. But every join path in airc-lib ends in `set_default`
        // (airc.rs:1654/1850/1928) — there is no subscribe-without-focus — so
        // spawning here moves the caller's current-room pointer as a side effect.
        //
        // That matters beyond tidiness: a daemon spawning a benchmark room yanks
        // the operator's focus, which is the same shape as #298 (personas dumped
        // into whatever room the operator's CLI last focused). The fix belongs in
        // airc — a `subscribe` that does not set default — and then `activity/open`
        // becomes the separate focusing verb it should be (#290, NavIntent). Until
        // that lands, callers who must not move focus have to restore the pointer
        // themselves, and this doc is the receipt saying why.
        let room = airc.join(&p.name).await.map_err(|source| {
            CommandError::Invalid(format!("could not create room {:?}: {source}", p.name))
        })?;

        // Bind the room to its recipe ON THE WALL, where every participant sees the
        // same answer to "what is this room". Without this the room forgets which
        // recipe it is and every client falls back to projecting it as a plain chat.
        let binding = serde_json::json!({
            "recipe": p.recipe,
            "parent": p.parent,
        });
        let post_id = airc
            .publish_wall_post(RECIPE_WALL_CATEGORY.to_string(), binding.to_string(), None)
            .await
            .map_err(|source| {
                CommandError::Internal(format!(
                    "room {} was created but its recipe binding could not be published: \
                     {source} — the room exists and is addressable, but until a binding \
                     lands it will project as a plain chat room",
                    room.channel
                ))
            })?;

        Ok(ActivitySpawnResult {
            room_id: room.channel.to_string(),
            name: room.name,
            recipe: p.recipe,
            binding_post_id: post_id.to_string(),
        })
    }
}

// ─────────────────────────── module ───────────────────────────

/// Hosts the `activity/*` verbs.
pub struct ActivityModule {
    registry: PersonaAircRuntimeRegistry,
}

impl ActivityModule {
    pub fn new(registry: PersonaAircRuntimeRegistry) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ServiceModule for ActivityModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "activity",
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        // Every activity verb is a typed object on the one registry (see `commands`).
        Err(format!(
            "activity command '{command}' is a typed object, not prefix-routed"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        vec![Arc::new(ActivitySpawn {
            registry: self.registry.clone(),
        })]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the recipe binding riding a category every reader agrees
    // on. If this constant drifts from what the purpose resolver filters for, a
    // spawned room binds its recipe into a category nobody reads and silently
    // projects as a plain chat — the failure is invisible at the write site.
    #[test]
    fn the_recipe_binding_rides_the_shared_wall_category() {
        assert_eq!(RECIPE_WALL_CATEGORY, "recipe");
    }

    // what this catches: spawn staying AiSafe. A citizen with an idea creating a
    // room for it is the ordinary act this whole model is built around; if this
    // ever regresses to Privileged, citizens go back to reusing whatever room they
    // are standing in, which is exactly the behaviour #274 exists to end.
    #[test]
    fn spawning_a_room_is_an_ordinary_citizen_act() {
        assert!(matches!(ActivitySpawn::ACCESS, AccessLevel::AiSafe));
        assert!(ActivitySpawn::NATIVE);
    }
}
