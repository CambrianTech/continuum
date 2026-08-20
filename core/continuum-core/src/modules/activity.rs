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

    /// Which recipe to build from — the EXACT `purpose` key of an authored recipe.
    ///
    /// Exact, because [`crate::experience::RecipeExperienceSource`] keys on the
    /// literal string and an unknown purpose resolves to `None`. Family names do
    /// not work: the authored benchmark recipe's purpose is `benchmark/hard-rs`,
    /// so `benchmark` matches nothing and the room falls through to rendering as
    /// plain chat. This doc used to list `chat, benchmark, video-chat, profile`
    /// and that middle one was never real.
    ///
    /// Not enumerated here on purpose: recipes are DATA, overlaid from disk by
    /// `builtins_with_overlay`, so any list in this comment is stale the moment
    /// someone authors a new one. Read the catalogue instead.
    ///
    /// The recipe decides the room's regions, verbs and layout; this command only
    /// decides that a room exists and which recipe it follows.
    pub recipe: String,

    /// Optional parent activity — activities spawn activities, and the graph is
    /// POINTERS (parent id here, child ids on the parent), never nested blobs.
    /// A `RoomId`, because that is what a parent activity IS. `schemars(with =
    /// "String")` describes the WIRE (a uuid string, per `#[serde(transparent)]`) to
    /// the tool schema while Rust keeps the type — the caller sends text, the command
    /// receives a parsed id, and an unparseable one is rejected at the boundary
    /// instead of flowing inward as a plausible-looking String.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "string")]
    #[schemars(with = "Option<String>")]
    pub parent: Option<RoomId>,

    /// Parameter overrides for the recipe's declared knobs (#433). Omit
    /// entirely (or leave empty) for the recipe's defaults — a zero-arg spawn
    /// always works. An unknown name or a value whose JSON type differs from
    /// the declared default is refused, naming the declared set.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    #[ts(type = "Record<string, unknown>")]
    pub params: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct ActivitySpawnResult {
    /// The airc channel id of the new room — the id every surface addresses it by.
    ///
    /// The TYPE, not a string that happens to hold a uuid: `RoomId` is a
    /// `uuid_newtype!` and a room id must never be interchangeable with a peer id,
    /// a card id, or free text ([[uuids-are-not-strings-and-never-hand-drawn]]).
    /// `#[ts(type = "string")]` because airc's id newtypes carry no `TS` derive by
    /// design (identity/mod.rs:83) and `#[serde(transparent)]` already puts the bare
    /// hyphenated uuid on the wire — so the TS side is unchanged while Rust keeps
    /// the distinction the compiler can enforce.
    #[ts(type = "string")]
    pub room_id: RoomId,
    /// The room's name, as created.
    pub name: String,
    /// The recipe this room follows.
    pub recipe: String,
    /// The wall post that binds room → recipe, so the binding is auditable.
    ///
    /// A bare `Uuid` rather than a newtype because airc's `publish_wall_post` returns
    /// one — there is no `WallPostId` upstream to borrow. Carrying the `Uuid` instead
    /// of stringifying it at least keeps the value in the type system on this side;
    /// the missing newtype is an airc-side gap, named here so it is not mistaken for
    /// a choice.
    #[ts(type = "string")]
    pub binding_post_id: uuid::Uuid,
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
    // The recipe names used to be listed inline here as "(chat, benchmark, …)".
    // Two things were wrong with that. It NAMED A PURPOSE THAT DOES NOT EXIST —
    // the authored recipe's purpose is `benchmark/hard-rs`, never `benchmark` —
    // and `RecipeExperienceSource` keys on the exact string, with
    // `unknown_purpose_yields_none` pinning that an unknown purpose resolves to
    // None. So a caller following this description got a room bound to a purpose
    // nothing can project, which (per the binding comment in `run`) then renders
    // as a plain chat room. A benchmark run that silently becomes a chat room is
    // precisely the academy failure.
    //
    // And a hardcoded list goes stale by construction: recipes are DATA, overlaid
    // from disk by `builtins_with_overlay`, so the catalogue grows without
    // touching this file. Naming members here re-hardcodes what the recipe loader
    // exists to keep dynamic.
    //
    // So this points at the live catalogue instead of enumerating it. The layer
    // below it — VALIDATION — shipped with #431: `validate_recipe` (below) resolves
    // the string against the live overlaid catalogue and REFUSES an unknown purpose,
    // naming the known recipes. A typo can no longer silently mint a chat room, which
    // is what [[fallbacks-are-illegal-fail-loud]] demands of a subsystem whose own
    // loader cites it. #433 added the same treatment to caller-supplied params.
    //
    // (This paragraph said "is NOT done" for some time after it WAS done. Cost: a
    // later reader trusted the comment over the function 40 lines below it and went
    // looking for a gate that already existed. A comment describing absent code is a
    // lying receipt of the #151/#357 class — it just lies to engineers instead of to
    // citizens. If you change this behaviour, change this paragraph in the same edit.)
    const DESCRIPTION: &'static str =
        "Create a new room from a recipe. `name` is what people call this instance; \
         `recipe` is the `purpose` of an authored recipe — use the exact purpose \
         string from the recipe catalogue (e.g. `benchmark/hard-rs`), not a family \
         name. Everything is a room — a chat, a benchmark run, a doc, a settings \
         pane — so spawn one whenever an idea needs its own shared space. \
         Returns the room_id.";
    type Params = ActivitySpawnParams;
    type Output = ActivitySpawnResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: ActivitySpawnParams,
    ) -> Result<ActivitySpawnResult, CommandError> {
        let airc = caller_airc(&self.registry, ctx)?;
        spawn_activity_room(&airc, &p.name, &p.recipe, p.parent, &p.params).await
    }
}

/// Refuse a recipe string that names no known recipe purpose (#431).
///
/// A recipe string is a reference into the recipe REGISTRY, not free text.
/// Before this gate, an unknown one "worked" — the room got made, the binding
/// published — and then resolved to no manifest, so every client projected the
/// room as plain chat. That silent downgrade is how every benchmark run room
/// rendered as chat for a full campaign: dispatch bound `"benchmark"` while the
/// shipped recipe's purpose is `"benchmark/hard-rs"`. The refusal names the
/// actionable set, per the registry's own `ids()`/`purposes()` design note.
/// `overlay_dir` is the SAME directory the positron projection resolves from
/// (`RecipeExperienceSource::overlay_dir`), so an authored on-disk recipe is
/// spawnable the moment the file exists (#432) — validation and resolution
/// stay one set by construction. A malformed overlay file surfaces HERE as the
/// parse error naming the file: the author's loudest, most actionable surface.
pub fn validate_recipe(recipe: &str, overlay_dir: &std::path::Path) -> Result<(), CommandError> {
    resolve_recipe(recipe, overlay_dir).map(|_| ())
}

/// As [`validate_recipe`], but return the MATCHED recipe (later wins by
/// purpose, the registry's overlay law) — `activity/spawn` needs its param
/// declarations (#433), not just a yes/no on the purpose string.
pub fn resolve_recipe(
    recipe: &str,
    overlay_dir: &std::path::Path,
) -> Result<crate::experience::recipe::ExperienceRecipe, CommandError> {
    let known = crate::experience::source::RecipeExperienceSource::known_recipes(overlay_dir)
        .map_err(|e| {
            CommandError::Invalid(format!(
                "recipe overlay under {} failed to load — fix or remove the named \
                 file, then retry: {e}",
                overlay_dir.display()
            ))
        })?;
    let purposes: Vec<&str> = known.iter().map(|r| r.purpose.as_str()).collect();
    known
        .iter()
        .filter(|r| r.purpose == recipe)
        .next_back()
        .cloned()
        .ok_or_else(|| {
            CommandError::Invalid(format!(
                "unknown recipe {recipe:?} — no recipe declares that purpose, so the \
                 room would project as plain chat. Known recipes: {}",
                purposes.join(", ")
            ))
        })
}

/// Merge caller overrides over the recipe's declared defaults (#433).
///
/// The declared DEFAULT is the type authority: a supplied value must carry the
/// same JSON type. Unknown names and type mismatches refuse loudly, naming the
/// declared set with its docs — the author's actionable surface, same doctrine
/// as the recipe-string gate above.
pub fn resolve_params(
    recipe: &crate::experience::recipe::ExperienceRecipe,
    given: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>, CommandError> {
    fn json_type(v: &serde_json::Value) -> &'static str {
        match v {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "bool",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    }
    let declared_set = || {
        if recipe.params.is_empty() {
            "this recipe declares NO parameters".to_string()
        } else {
            recipe
                .params
                .iter()
                .map(|(k, d)| format!("{k} ({}: {})", json_type(&d.default), d.doc))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let mut resolved: std::collections::BTreeMap<String, serde_json::Value> = recipe
        .params
        .iter()
        .map(|(k, d)| (k.clone(), d.default.clone()))
        .collect();
    for (name, value) in given {
        let decl = recipe.params.get(name).ok_or_else(|| {
            CommandError::Invalid(format!(
                "unknown parameter {name:?} for recipe {:?} — declared: {}",
                recipe.purpose,
                declared_set()
            ))
        })?;
        if json_type(value) != json_type(&decl.default) {
            return Err(CommandError::Invalid(format!(
                "parameter {name:?} expects a {} (the declared default's type), got a \
                 {} — declared: {}",
                json_type(&decl.default),
                json_type(value),
                declared_set()
            )));
        }
        resolved.insert(name.clone(), value.clone());
    }
    Ok(resolved)
}

/// Birth a room from a recipe on an ALREADY-RESOLVED airc handle.
///
/// This is the whole of `activity/spawn` minus the caller-identity lookup, split
/// out because **`activity/spawn` is not the only thing that needs to make a
/// room**. A benchmark run needs its own room too, and the alternative — a second
/// creator inside `benchmark/dispatch` — is exactly the parallel-birth-path this
/// module's header warns about: rooms made by hand carry no recipe and no purpose,
/// and then every client projects them as plain chat.
///
/// One birth path, two callers. A third (activity templates, scheduled runs) slots
/// in here rather than growing its own.
pub async fn spawn_activity_room(
    airc: &Airc,
    name: &str,
    recipe: &str,
    parent: Option<RoomId>,
    params: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<ActivitySpawnResult, CommandError> {
    let recipe_def = resolve_recipe(
        recipe,
        &crate::experience::source::RecipeExperienceSource::overlay_dir(
            &crate::modules::persona_instance_manager::resolve_continuum_root(),
        ),
    )?;
    let resolved_params = resolve_params(&recipe_def, params)?;
    {
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
        let room = airc.join(name).await.map_err(|source| {
            CommandError::Invalid(format!("could not create room {name:?}: {source}"))
        })?;

        // Bind the room to its recipe ON THE WALL, where every participant sees the
        // same answer to "what is this room". Without this the room forgets which
        // recipe it is and every client falls back to projecting it as a plain chat.
        //
        // Serialized from the SHARED [`RoomRecipeBinding`] type, never a hand-authored
        // `json!` — the reader (`ipc::recipe_room_purpose`) deserializes that same type,
        // so the two sides agree by construction. This was an inline literal for as
        // long as the binding had no reader at all, which is exactly how a field-name
        // typo here would have cost nothing and been noticed by nobody.
        let binding = RoomRecipeBinding {
            recipe: recipe.to_string(),
            parent,
            params: resolved_params,
        };
        let body = serde_json::to_string(&binding).map_err(|source| {
            CommandError::Internal(format!("encode recipe binding: {source}"))
        })?;
        let post_id = airc
            .publish_wall_post(RECIPE_WALL_CATEGORY.to_string(), body, None)
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
            room_id: room.channel,
            name: room.name,
            recipe: recipe.to_string(),
            binding_post_id: post_id,
        })
    }
}

// ─────────────────────────── standing ───────────────────────────

/// Read the current standing of the caller's current room, or the default
/// (live, unprotected) when nobody has ever declared one.
///
/// The type, the wall category, and the rule for turning posts into a standing
/// all live in [`crate::experience::standing`] — this is only the "fetch the
/// current room's posts" half. They were born here as a private `async fn` over
/// `&Airc`, which meant the only code that could read a standing was the command
/// that wrote it, and `archived` shipped as a declaration nothing consumed.
async fn current_standing(airc: &Airc) -> Result<RoomStanding, CommandError> {
    let posts = airc
        .wall_posts(Some(STANDING_WALL_CATEGORY))
        .await
        .map_err(|source| {
            CommandError::Internal(format!("could not read room standing: {source}"))
        })?;
    project_standing(&posts).map_err(|source| CommandError::Internal(source.to_string()))
}

/// Publish a merged standing, preserving every field the caller did not set.
async fn publish_standing(airc: &Airc, next: &RoomStanding) -> Result<String, CommandError> {
    let body = serde_json::to_string(next)
        .map_err(|source| CommandError::Internal(format!("encode standing: {source}")))?;
    airc.publish_wall_post(STANDING_WALL_CATEGORY.to_string(), body, None)
        .await
        .map(|id| id.to_string())
        .map_err(|source| {
            CommandError::Internal(format!("could not publish room standing: {source}"))
        })
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct StandingResult {
    /// The standing now in effect.
    pub archived: bool,
    pub protected: bool,
    /// The wall post that declared it.
    pub post_id: String,
}

// ─────────────────────────── activity/archive ───────────────────────────

/// Mark the current room concluded — or reopen it.
pub struct ActivityArchive {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct ActivityArchiveParams {
    /// `true` concludes the activity, `false` reopens it. Reversible on purpose —
    /// concluding is a judgement, and judgements get revised.
    #[serde(default = "default_true")]
    pub archived: bool,
    /// Why this activity is finished (or live again).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
}

fn default_true() -> bool {
    true
}

#[async_trait]
impl ActionCommand for ActivityArchive {
    const NAME: &'static str = "activity/archive";
    const ALIASES: &'static [&'static str] = &["conclude_activity"];
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Mark this room's activity concluded (or reopen it with archived=false). The \
         room stays completely readable — its transcript, cards and history are \
         untouched. Archiving only says the work here is finished, so it stops being \
         offered as somewhere to pick up new work.";
    type Params = ActivityArchiveParams;
    type Output = StandingResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: ActivityArchiveParams,
    ) -> Result<StandingResult, CommandError> {
        let airc = caller_airc(&self.registry, ctx)?;
        let mut standing = current_standing(&airc).await?;
        standing.archived = p.archived;
        if p.note.is_some() {
            standing.note = p.note;
        }
        let post_id = publish_standing(&airc, &standing).await?;
        Ok(StandingResult {
            archived: standing.archived,
            protected: standing.protected,
            post_id,
        })
    }
}

// ─────────────────────────── activity/protect ───────────────────────────

/// Make the current room refuse deletion — or release that.
pub struct ActivityProtect {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct ActivityProtectParams {
    /// `true` protects, `false` releases protection.
    #[serde(default = "default_true")]
    pub protected: bool,
    /// Why this record is load-bearing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
}

#[async_trait]
impl ActionCommand for ActivityProtect {
    const NAME: &'static str = "activity/protect";
    const ALIASES: &'static [&'static str] = &[];
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Protect this room from deletion (or release it with protected=false). Use it \
         on any room whose record is load-bearing — a benchmark run's room IS that \
         run's evidence, and a stray delete would take the evidence with it.";
    type Params = ActivityProtectParams;
    type Output = StandingResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: ActivityProtectParams,
    ) -> Result<StandingResult, CommandError> {
        let airc = caller_airc(&self.registry, ctx)?;
        let mut standing = current_standing(&airc).await?;
        standing.protected = p.protected;
        if p.note.is_some() {
            standing.note = p.note;
        }
        let post_id = publish_standing(&airc, &standing).await?;
        Ok(StandingResult {
            archived: standing.archived,
            protected: standing.protected,
            post_id,
        })
    }
}

// Descriptors for the three verbs above. Their CONSTRUCTORS come from
// `ActivityModule::commands()` (they hold the airc registry, so they are not
// `Default`-constructible and cannot use `register_stateless_command!`); the
// descriptor is type-only, so it is declared here at the same site the command is.
// Both halves are required — a constructor without a descriptor routes but is
// INVISIBLE to `commands/list`, the persona tool surface, the ACL and codegen, which
// is exactly how `activity/spawn` — the verb that mints every room, benchmark rooms
// included — became undiscoverable while the catalog promised "Listed == callable".
// `ModuleRegistry::register` now refuses to boot on the omission.
crate::register_command!(ActivitySpawn);
crate::register_command!(ActivityArchive);
crate::register_command!(ActivityProtect);

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
        vec![
            Arc::new(ActivitySpawn {
                registry: self.registry.clone(),
            }),
            Arc::new(ActivityArchive {
                registry: self.registry.clone(),
            }),
            Arc::new(ActivityProtect {
                registry: self.registry.clone(),
            }),
        ]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the exact live bug that made EVERY benchmark run room
    // render as plain chat — dispatch bound the recipe string "benchmark" while
    // the shipped recipe declares purpose "benchmark/hard-rs", and nothing
    // validated the string against the registry. The gate must refuse the bad
    // string LOUDLY (naming the actionable set) and pass every shipped purpose.
    // regression for #431 / commit at benchmark.rs:1091
    #[test]
    fn validate_recipe_refuses_unknown_and_passes_every_shipped_purpose() {
        let empty_overlay = tempfile::tempdir().expect("tempdir");
        for purpose in crate::experience::source::RecipeExperienceSource::shipped_purposes() {
            assert!(
                validate_recipe(&purpose, empty_overlay.path()).is_ok(),
                "shipped purpose {purpose:?} must validate"
            );
        }
        let err = validate_recipe("benchmark", empty_overlay.path())
            .expect_err("the old dispatch literal must refuse");
        let msg = format!("{err}");
        assert!(
            msg.contains("benchmark/hard-rs"),
            "refusal must name the actionable set, got: {msg}"
        );
    }

    // what this catches (#432): validation and resolution staying ONE set once
    // the disk overlay is live. An authored on-disk recipe must be spawnable
    // the moment the file exists — if validate_recipe only consulted the
    // embedded set, every authored recipe would be refused at activity/spawn
    // while the projection happily resolved it (or vice versa). Also pins the
    // fail-loud arm: a malformed overlay file refuses NAMING the file, never a
    // silent skip.
    #[test]
    fn validate_recipe_accepts_overlay_authored_purpose_and_refuses_malformed_overlay() {
        let overlay = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            overlay.path().join("wordstats.json"),
            r#"{ "purpose": "bench/wordstats", "regions": [], "affordances": [] }"#,
        )
        .expect("write authored recipe");
        match validate_recipe("bench/wordstats", overlay.path()) {
            Ok(()) => {}
            Err(e) => panic!("authored overlay purpose must validate, got: {e}"),
        }

        std::fs::write(overlay.path().join("broken.json"), "{ not json")
            .expect("write malformed recipe");
        let err = validate_recipe("bench/wordstats", overlay.path())
            .expect_err("a malformed overlay file must refuse loudly");
        assert!(
            format!("{err}").contains("broken.json"),
            "refusal must name the malformed file, got: {err}"
        );
    }

    // what this catches: core call sites bind rooms by shipped CONSTANT; the
    // purpose string must come from the recipe JSON, resolved through
    // shipped_purpose — if the mapping breaks, dispatch would silently bind a
    // wrong (or no) purpose again.
    #[test]
    fn shipped_benchmark_constant_resolves_to_its_declared_purpose() {
        assert_eq!(
            crate::experience::source::RecipeExperienceSource::shipped_purpose(
                crate::experience::source::shipped::BENCHMARK_HARD_RS
            )
            .as_deref(),
            Some("benchmark/hard-rs")
        );
    }

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
    // what this catches: a room nobody has ever marked reading as LIVE and
    // UNPROTECTED. The default is load-bearing — if `archived` ever defaulted true,
    // every ordinary room would silently stop offering its work; if `protected`
    // defaulted true, nothing could ever be deleted and the verb would look broken.
    #[test]
    fn an_unmarked_room_is_live_and_unprotected() {
        let s = RoomStanding::default();
        assert!(!s.archived);
        assert!(!s.protected);
        assert!(s.note.is_none());
    }

    // what this catches: standing round-tripping through the wall body. These verbs
    // MERGE — archiving must not clear protection and vice versa — so the encoded
    // form has to carry both fields, not just the one the caller set.
    #[test]
    fn standing_round_trips_both_flags_so_the_verbs_can_merge() {
        let s = RoomStanding {
            archived: true,
            protected: true,
            note: Some("K3 bring-up finished; evidence".into()),
        };
        let encoded = serde_json::to_string(&s).expect("encode");
        let back: RoomStanding = serde_json::from_str(&encoded).expect("decode");
        assert!(back.archived && back.protected);
        assert_eq!(back.note.as_deref(), Some("K3 bring-up finished; evidence"));

        // and a body written by an older client that only knew one field still reads
        // — a missing flag is `false`, never a parse failure that would strand the room.
        let partial: RoomStanding =
            serde_json::from_str(r#"{"archived":true}"#).expect("forward-compatible");
        assert!(partial.archived && !partial.protected);
    }

    #[test]
    fn spawning_a_room_is_an_ordinary_citizen_act() {
        assert!(matches!(ActivitySpawn::ACCESS, AccessLevel::AiSafe));
        assert!(ActivitySpawn::NATIVE);
    }

    mod params {
        use super::*;
        use crate::experience::recipe::ExperienceRecipe;
        use std::collections::BTreeMap;

        fn recipe_with_params() -> ExperienceRecipe {
            ExperienceRecipe::from_json(
                r#"{
                    "purpose": "bench/param-fixture",
                    "regions": [],
                    "affordances": [],
                    "params": {
                        "suite": { "default": "swe-lite", "doc": "which benchmark suite" },
                        "instances": { "default": 1, "doc": "how many instances to run" },
                        "disclose": { "default": true, "doc": "announce full capacity" }
                    }
                }"#,
            )
            .expect("fixture recipe parses")
        }

        // what this catches (#433): the zero-arg contract. "A zero-arg spawn
        // always works" is the whole point of default-carrying decls — if an
        // empty override map didn't resolve to exactly the declared defaults,
        // every existing caller (benchmark dispatch passes an empty map) would
        // break or silently under-describe its room.
        #[test]
        fn empty_overrides_resolve_to_the_declared_defaults() {
            let resolved = resolve_params(&recipe_with_params(), &BTreeMap::new())
                .expect("zero-arg spawn always works");
            assert_eq!(resolved["suite"], serde_json::json!("swe-lite"));
            assert_eq!(resolved["instances"], serde_json::json!(1));
            assert_eq!(resolved["disclose"], serde_json::json!(true));
        }

        // what this catches (#433): an override MERGING over the defaults, not
        // replacing them — a caller setting one knob must still get every other
        // knob's default on the binding, or the room stops being self-describing.
        #[test]
        fn an_override_merges_over_the_remaining_defaults() {
            let given = BTreeMap::from([("instances".to_string(), serde_json::json!(5))]);
            let resolved =
                resolve_params(&recipe_with_params(), &given).expect("valid override");
            assert_eq!(resolved["instances"], serde_json::json!(5));
            assert_eq!(resolved["suite"], serde_json::json!("swe-lite"));
        }

        // what this catches (#433): a typo'd param name silently vanishing.
        // If unknown names were dropped instead of refused, a caller targeting
        // "instance" (singular) would run the default count and never learn why
        // — the refusal must name the declared set WITH docs, the author's
        // actionable surface.
        #[test]
        fn an_unknown_name_is_refused_naming_the_declared_set() {
            let given = BTreeMap::from([("instance".to_string(), serde_json::json!(5))]);
            let err = resolve_params(&recipe_with_params(), &given)
                .expect_err("unknown param must refuse");
            let msg = format!("{err}");
            assert!(msg.contains("instance"), "names the offender: {msg}");
            assert!(msg.contains("instances"), "names the declared set: {msg}");
            assert!(
                msg.contains("how many instances to run"),
                "carries the docs so the caller can self-correct: {msg}"
            );
        }

        // what this catches (#433): the default-IS-the-type contract. A string
        // where the declared default is a number must refuse — otherwise the
        // binding publishes a value no reader can trust the shape of, and the
        // type error surfaces far from the caller who made it.
        #[test]
        fn a_value_of_the_wrong_json_type_is_refused() {
            let given = BTreeMap::from([("instances".to_string(), serde_json::json!("five"))]);
            let err = resolve_params(&recipe_with_params(), &given)
                .expect_err("type mismatch must refuse");
            let msg = format!("{err}");
            assert!(msg.contains("number"), "names the expected type: {msg}");
            assert!(msg.contains("string"), "names the supplied type: {msg}");
        }

        // what this catches (#433): a parameterless recipe (every shipped one
        // today) refusing ANY override — and saying so plainly rather than
        // implying a declared set that doesn't exist.
        #[test]
        fn a_parameterless_recipe_refuses_any_override_plainly() {
            let bare = ExperienceRecipe::from_json(
                r#"{ "purpose": "bench/bare", "regions": [], "affordances": [] }"#,
            )
            .expect("bare recipe parses");
            assert!(
                resolve_params(&bare, &BTreeMap::new())
                    .expect("no params, no overrides — fine")
                    .is_empty()
            );
            let err = resolve_params(
                &bare,
                &BTreeMap::from([("anything".to_string(), serde_json::json!(1))]),
            )
            .expect_err("override on a parameterless recipe must refuse");
            assert!(
                format!("{err}").contains("NO parameters"),
                "says the recipe declares nothing: {err}"
            );
        }

        // what this catches (#433): resolve_recipe honoring the registry's
        // later-wins-by-purpose law. An overlay file re-declaring a shipped
        // purpose must be the one whose param decls govern the spawn — if the
        // embedded copy won, authored param additions would silently not exist.
        #[test]
        fn resolve_recipe_returns_the_overlay_copy_when_purposes_collide() {
            let overlay = tempfile::tempdir().expect("tempdir");
            std::fs::write(
                overlay.path().join("chat.json"),
                r#"{
                    "purpose": "chat",
                    "regions": [],
                    "affordances": [],
                    "params": { "topic": { "default": "open", "doc": "what this chat is for" } }
                }"#,
            )
            .expect("write overlay chat");
            let resolved = resolve_recipe("chat", overlay.path())
                .expect("shipped purpose still resolves");
            assert!(
                resolved.params.contains_key("topic"),
                "the OVERLAY copy (with its param decls) must win, got params: {:?}",
                resolved.params.keys().collect::<Vec<_>>()
            );
        }

        // what this catches (#433): the SHIPPED benchmark recipe's declared
        // param types drifting from what benchmark dispatch actually sends
        // (suite: string, instances: array, team: array). Dispatch builds
        // this exact map shape at benchmark.rs — if a decl's default changed
        // JSON type, every run-room spawn would refuse at the type gate and
        // no unit test on either side alone would say why.
        #[test]
        fn shipped_benchmark_recipe_accepts_dispatchs_targeting_map() {
            let empty_overlay = tempfile::tempdir().expect("tempdir");
            let recipe = resolve_recipe("benchmark/hard-rs", empty_overlay.path())
                .expect("shipped benchmark recipe resolves");
            for declared in ["suite", "instances", "team", "budget"] {
                assert!(
                    recipe.params.contains_key(declared),
                    "shipped benchmark recipe must declare {declared:?} (the catalog's \
                     targeting set), got: {:?}",
                    recipe.params.keys().collect::<Vec<_>>()
                );
            }
            let dispatch_shaped = std::collections::BTreeMap::from([
                ("suite".to_string(), serde_json::json!("swe-bench-lite")),
                (
                    "instances".to_string(),
                    serde_json::json!(["sympy__sympy-24152"]),
                ),
                ("team".to_string(), serde_json::json!(["Asha", "Anwen"])),
            ]);
            let resolved = resolve_params(&recipe, &dispatch_shaped)
                .expect("dispatch's targeting map must type-check against the decls");
            assert_eq!(resolved["suite"], serde_json::json!("swe-bench-lite"));
            assert_eq!(
                resolved["budget"],
                serde_json::json!("4h"),
                "unset budget rides the declared default onto the binding"
            );
        }
    }
}
