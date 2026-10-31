use super::*;

/// Resolve the CALLING peer's own airc handle so the room is created as THEIR
/// identity — the creator is a real peer, never the substrate acting anonymously.
///
/// One resolver, not two: this is [`crate::persona::operator_peer::persona_airc`], which
/// already routes a caller-less invocation to the OPERATOR SELF-PEER (#27). The
/// private duplicate that lived here predated the self-peer and still carried
/// the denial — so the human could dispatch work but could not start an
/// activity, the exact "ask for a project should be easy and common" gap
/// (Joel, 2026-08-31). Same bug at two sites = one missing constraint; the
/// constraint is the shared helper.
pub(super) fn caller_airc(
    registry: &PersonaAircRuntimeRegistry,
    ctx: &Ctx,
) -> Result<Arc<Airc>, CommandError> {
    crate::persona::operator_peer::persona_airc(registry, ctx, "activity verbs")
}

// ─────────────────────────── activity/spawn ───────────────────────────
/// Instantiate a recipe as a new room.
pub struct ActivitySpawn {
    pub registry: PersonaAircRuntimeRegistry,
    /// Late-bound substrate executor (the ChatModule pattern), so spawning a BENCHMARK
    /// activity can run `benchmark/dispatch` through the universal primitive rather than
    /// re-implementing the round it owns. Installed by `install_executor_on_all`.
    pub executor_slot: std::sync::Arc<
        crate::runtime::LateBound<crate::runtime::command_executor::CommandExecutor>,
    >,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct ActivitySpawnParams {
    /// What people will call this activity: name it for the activity, not a subsystem.
    // A subsystem never finishes, so a room named after one reads as a permanent place and
    // quietly becomes the room everyone reuses forever.
    pub name: String,

    /// The recipe's exact `purpose` (activity/recipes lists them), e.g. `project`.
    // Exact, because RecipeExperienceSource keys on the literal string and an unknown
    // purpose resolves to None (a family name like `benchmark` matches nothing and the
    // room renders as plain chat). Not enumerated here: recipes are data, overlaid from
    // disk, so a list in this comment goes stale. The recipe decides the room's regions,
    // verbs and layout; this command only decides that a room exists and which recipe.
    // (Doc comments ship in every citizen's tool schema, so the rationale lives in `//`.)
    pub recipe: String,

    /// Parent activity's room id, if this one belongs under another.
    // A RoomId: the graph is pointers (parent id here, child ids on the parent). schemars
    // describes the wire (a uuid string) while Rust keeps the type, so an unparseable id
    // is refused at the boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "string")]
    #[schemars(with = "Option<String>")]
    pub parent: Option<RoomId>,

    /// Values for the recipe's parameters, e.g. {"repo": "owner/name"}; omit for defaults.
    // #433. Values, never schemas: Kimi (2026-09-28) passed {"enum": [...], "type":
    // "string"} for repo from the old "declared knobs" wording. An unknown name or a value
    // whose JSON type differs from the declared default is refused, naming the declared set.
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
    /// What the recipe's PIPELINE did when this room was born (S3), or `None` when
    /// the recipe declares none — the ordinary page. A held run (`held_at`) is a
    /// human's turn, not a failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pipeline: Option<ActivityPipelineReceipt>,
}

/// The birth-time pipeline receipt, in the spawn result's own shape.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPipelineReceipt {
    /// The birth's recipe run — `recipe/status` reads it again, after a restart too.
    #[ts(type = "string")]
    pub run_id: uuid::Uuid,
    /// Where the run is: completed, held for approval, or waiting on recovery.
    pub phase: crate::recipe::run_record::RunPhase,
    pub steps_run: u32,
    pub steps_skipped: u32,
    #[ts(optional)]
    pub held_at: Option<u32>,
    /// Step-indexed one-line outcomes — what a human reads to see what happened.
    pub trace: Vec<String>,
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
        // The pipeline acts as the same identity that joins the room — one resolver.
        let caller = crate::persona::operator_peer::acting_caller(&self.registry, ctx, "activity verbs")?;
        spawn_activity_room(
            &airc,
            &p.name,
            &p.recipe,
            p.parent,
            &p.params,
            self.executor_slot.get().cloned(),
            Some(caller),
        )
        .await
    }
}

// ─────────────────────────── activity/invite ───────────────────────────
/// Bring named citizens into a room, so a freshly spawned activity has its
/// people in one verb.
pub(crate) struct ActivityInvite {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct ActivityInviteParams {
    /// The room's NAME (airc joins are by name — `activity/spawn` returned it).
    pub room: String,
    /// Agent names to bring in (the roster's names, e.g. `["Kira", "Joaquin"]`).
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct ActivityInviteResult {
    /// Members now subscribed (idempotent — already-in counts as joined).
    pub joined: Vec<String>,
    /// Members that could not be joined, each with the reason — a partial
    /// invite reports honestly instead of failing the whole verb.
    pub failed: Vec<String>,
}

#[async_trait]
impl ActionCommand for ActivityInvite {
    const NAME: &'static str = "activity/invite";
    const ALIASES: &'static [&'static str] = &["invite_to_room"];
    const NATIVE: bool = true;
    /// AiSafe: subscribing a citizen to a room is additive — she hears it and
    /// can speak in it, and `room/leave` remains hers ([[self-determination]]).
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    // The missing third act of "start a project": spawn made the room, but the
    // only ways a citizen entered one were the reconciler's default set or a
    // benchmark dispatch join — a human-started activity sat EMPTY, which is
    // why starting a project with citizens was not "easy and common"
    // (Joel, 2026-08-31). spawn → invite → ask is the whole flow now.
    const DESCRIPTION: &'static str =
        "Invite citizens into a room by name: they subscribe, hear it, and can \
         speak there. Use after activity/spawn to staff a new activity \
         (spawn → invite → say what it's about). Idempotent per member; \
         partial failures are reported per-name, never silently dropped.";
    type Params = ActivityInviteParams;
    type Output = ActivityInviteResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: ActivityInviteParams,
    ) -> Result<ActivityInviteResult, CommandError> {
        if p.members.is_empty() {
            return Err(CommandError::Invalid(
                "members is empty — name at least one citizen to invite (see presence/directory)"
                    .into(),
            ));
        }
        let mut joined = Vec::new();
        let mut failed = Vec::new();
        for name in &p.members {
            match self.registry.get_by_agent_name(name) {
                Some(rt) => match rt.join_room(&p.room).await {
                    Ok(()) => joined.push(name.clone()),
                    Err(e) => failed.push(format!("{name}: join failed — {e}")),
                },
                None => failed.push(format!(
                    "{name}: no live runtime by that agent name (presence/directory lists who is online)"
                )),
            }
        }
        Ok(ActivityInviteResult { joined, failed })
    }
}

// ─────────────────────────── activity/recipes ───────────────────────────
/// List the live recipe catalogue — what `activity/spawn` can build.
pub struct ActivityRecipes;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct ActivityRecipesParams {}

/// One spawnable recipe, as the catalogue declares it.
#[derive(Debug, Clone, Serialize, TS)]
pub struct RecipeCatalogEntry {
    /// The EXACT string `activity/spawn` takes as `recipe`.
    pub purpose: String,
    /// The recipe's declared parameter knobs, each as `name (type: doc)` —
    /// render-ready, the same wording the param refusal uses.
    pub params: Vec<String>,
    /// Everything wrong with the recipe's pipeline against THIS node's command
    /// schemas — empty means spawnable. A recipe with issues is listed (an author
    /// must be able to see it) and refused at spawn (it must never half-run).
    pub issues: Vec<crate::recipe::PipelineIssue>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct ActivityRecipesResult {
    pub count: u32,
    /// Registry order: embedded first, overlay after (later wins by purpose).
    pub recipes: Vec<RecipeCatalogEntry>,
}

#[async_trait]
impl ActionCommand for ActivityRecipes {
    const NAME: &'static str = "activity/recipes";
    const ALIASES: &'static [&'static str] = &["list_recipes"];
    const NATIVE: bool = true;
    /// AiSafe: reading the catalogue is how anyone — citizen or operator —
    /// finds out what they can start.
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    // `activity/spawn`'s description says "use the exact purpose string from the
    // recipe catalogue" — and until this verb, THE CATALOGUE HAD NO READ VERB.
    // Discovery-by-refusal (spawn a wrong name to see the known list in the
    // error) was the only path, which is a doc line standing in for a design
    // ([[foolproof-over-instructions]]; Joel 2026-08-31: starting an activity
    // must be "easy and common").
    const DESCRIPTION: &'static str =
        "List the spawnable recipe catalogue — the exact `purpose` strings \
         `activity/spawn` accepts, with each recipe's declared parameters. \
         Read this before spawning; never guess a purpose string.";
    type Params = ActivityRecipesParams;
    type Output = ActivityRecipesResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: ActivityRecipesParams,
    ) -> Result<ActivityRecipesResult, CommandError> {
        let known = crate::experience::source::RecipeExperienceSource::known_recipes(
            &crate::experience::source::RecipeExperienceSource::overlay_dir(
                &crate::paths::continuum_root(),
            ),
        )
        .map_err(|e| {
            CommandError::Invalid(format!(
                "recipe overlay failed to load — fix or remove the named file, then retry: {e}"
            ))
        })?;
        let recipes: Vec<RecipeCatalogEntry> = known
            .iter()
            .map(|r| RecipeCatalogEntry {
                purpose: r.purpose.clone(),
                params: r
                    .params
                    .iter()
                    .map(|(k, d)| format!("{k} ({})", d.doc))
                    .collect(),
                issues: crate::recipe::pipeline_issues(r, |name| {
                    crate::recipe::registry_lookup().get(name).cloned()
                }),
            })
            .collect();
        Ok(ActivityRecipesResult {
            count: recipes.len() as u32,
            recipes,
        })
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
            // Kimi (2026-09-28) passed {"enum": [...], "type": "string"} for a string: a
            // description of the value in place of the value. Say so, and show the form.
            let schema_shaped = value
                .as_object()
                .is_some_and(|o| o.contains_key("type") || o.contains_key("enum"));
            let schema_note = if schema_shaped {
                " That object describes the value (a schema); pass the value itself."
            } else {
                ""
            };
            let example = match &decl.default {
                serde_json::Value::String(_) => "\"...\"",
                serde_json::Value::Number(_) => "1",
                serde_json::Value::Bool(_) => "true",
                serde_json::Value::Array(_) => "[\"...\"]",
                serde_json::Value::Object(_) => "{}",
                serde_json::Value::Null => "null",
            };
            return Err(CommandError::Invalid(format!(
                "parameter {name:?} expects a {} (the declared default's type), got a \
                 {}.{schema_note} Pass it as \"params\": {{\"{name}\": {example}}}. \
                 Declared: {}",
                json_type(&decl.default),
                json_type(value),
                declared_set()
            )));
        }
        resolved.insert(name.clone(), value.clone());
    }
    Ok(resolved)
}

/// Spawn may resume the same recipe, but it cannot repurpose somebody's activity.
pub(super) fn validate_spawn_binding(
    name: &str,
    recipe: &str,
    posts: &[airc_core::doctrine::WallPostPublished],
) -> Result<(), CommandError> {
    let existing = crate::experience::binding::project_binding(posts).map_err(|e| {
        CommandError::Internal(format!("cannot read recipe for room {name:?}: {e}"))
    })?;
    if let Some(binding) = existing {
        if binding.recipe != recipe {
            return Err(CommandError::Invalid(format!(
                "room {name:?} already belongs to recipe {:?}; cannot spawn {recipe:?} \
                 over it. Choose a new activity name, or join the existing room.",
                binding.recipe
            )));
        }
    }
    Ok(())
}
