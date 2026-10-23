use super::*;

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
    executor: Option<std::sync::Arc<crate::runtime::command_executor::CommandExecutor>>,
    // The identity the pipeline's steps act as — the spawner's (`acting_caller`);
    // `None` only for a recipe with no pipeline or a substrate-internal birth.
    caller: Option<crate::routing::CallerIdentity>,
) -> Result<ActivitySpawnResult, CommandError> {
    let recipe_def = resolve_recipe(
        recipe,
        &crate::experience::source::RecipeExperienceSource::overlay_dir(
            &crate::paths::continuum_root(),
        ),
    )?;
    // THE SCHEMA GATE: a pipeline is checked against the command registry's own
    // schemas BEFORE a room is born, like a request against an API spec. A recipe
    // with an issue never half-runs; the refusal names every step. Cheap and off
    // every hot path: once per spawn, over a lookup built once per process.
    let issues = crate::recipe::pipeline_issues(&recipe_def, |name| {
        crate::recipe::registry_lookup().get(name).cloned()
    });
    if !issues.is_empty() {
        let lines: Vec<String> = issues.iter().map(ToString::to_string).collect();
        return Err(CommandError::Invalid(format!(
            "recipe {recipe:?} has a pipeline this node cannot run — fix the file, then retry:\n  {}",
            lines.join("\n  ")
        )));
    }
    let resolved_params = resolve_params(&recipe_def, params)?;
    // The pipeline reads the SAME resolved params the binding records (`$args.*`).
    let pipeline_args = serde_json::Value::Object(
        resolved_params.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    );
    // Read before `resolved_params` moves into the binding below.
    let child_suite = resolved_params
        .get("suite")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    // WHERE IT ROOTS: an explicit parent wins (a sub-activity nests under the activity
    // that spawned it); else the recipe's declared BASE (a benchmark round is learning →
    // `academy`, by what it is); else nowhere in particular (the spawner's context). The
    // base room is resolved by name WITHOUT moving the spawner's focus (airc's Keep join).
    // The parent ROOM (not just its id) is kept when we resolved it ourselves: the child
    // record below is published on the parent's wall, and that needs a `Room`.
    let mut parent_room: Option<airc_lib::Room> = None;
    let parent = match parent {
        Some(p) => Some(p),
        None => match recipe_def.base.as_deref() {
            Some(base) => match airc.subscribe_room(base).await {
                Ok(room) => {
                    parent_room = Some(room.clone());
                    Some(room.channel)
                }
                Err(source) => {
                    continuum_base::probe!(
                        class = "activity.base_unresolved",
                        recipe = %recipe,
                        base = %base,
                        error = %source.to_string(),
                        "the recipe's base room could not be resolved — the activity roots \
                         top-level this time"
                    );
                    None
                }
            },
            None => None,
        },
    };
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
        // A reused name resolves to the SAME room. Read its type before publishing
        // anything: a benchmark spawn once rebound the live project room, after
        // which benchmark cleanup evicted its coding peers as "finished".
        let posts = airc
            .wall_posts_in(&room, Some(RECIPE_WALL_CATEGORY))
            .await
            .map_err(|e| {
                CommandError::Internal(format!("cannot read recipe for room {name:?}: {e}"))
            })?;
        validate_spawn_binding(name, recipe, &posts)?;
        // Every spawned room enters the node's adoption set the moment it exists, so
        // the presence emitter bridges its transcript (chat store + rail) on the next
        // refresh — the same minute, like a bench run room, not never (card 3d4b3d9c).
        crate::experience::spawned_rooms::record(room.channel.as_uuid(), name);

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
        let body = serde_json::to_string(&binding)
            .map_err(|source| CommandError::Internal(format!("encode recipe binding: {source}")))?;
        let post_id = airc
            .publish_wall_post_in(&room, RECIPE_WALL_CATEGORY.to_string(), body, None)
            .await
            .map_err(|source| {
                CommandError::Internal(format!(
                    "room {} was created but its recipe binding could not be published: \
                     {source} — the room exists and is addressable, but until a binding \
                     lands it will project as a plain chat room",
                    room.channel
                ))
            })?;

        // THE CHILD ANNOUNCES ITSELF TO ITS PARENT (experience::children). A remote node's
        // resume pass reads the parent's wall and seats its residents by NAME; without
        // this record a round existed only on the node that spawned it. Never fails the
        // spawn: the room and its binding stand; a missing record is said, not hidden.
        if let Some(parent_id) = parent {
            let parent_room = match parent_room.take() {
                Some(r) => Some(r),
                None => airc.subscription_set().await.ok().and_then(|set| {
                    set.all()
                        .map(|sub| sub.as_room())
                        .find(|r| r.channel == parent_id)
                }),
            };
            match parent_room {
                Some(parent_room) => {
                    let record = crate::experience::children::RoomChildRecord {
                        room_id: room.channel.as_uuid(),
                        name: name.to_string(),
                        recipe: recipe.to_string(),
                        suite: child_suite,
                        spawned_at_ms: continuum_base::time::EpochMs::now().millis(), // pre-1970 reads 0: a pre-epoch clock stamps 0 — the record still names the room
                    };
                    match serde_json::to_string(&record) {
                        Ok(body) => match airc
                            .publish_wall_post_in(
                                &parent_room,
                                crate::experience::children::CHILD_WALL_CATEGORY.to_string(),
                                body,
                                None,
                            )
                            .await
                        {
                            Ok(_) => continuum_base::probe!(
                                class = "activity.child_announced",
                                room = %name,
                                parent = %parent_room.name,
                                recipe = %recipe,
                                "the spawned activity is on its parent's wall — any node reading the parent can seat its residents"
                            ),
                            Err(source) => continuum_base::probe!(
                                class = "activity.child_announce_failed",
                                room = %name,
                                parent = %parent_room.name,
                                error = %source.to_string(),
                                "the room exists but its parent never heard of it — remote nodes cannot seat into it until a re-spawn"
                            ),
                        },
                        Err(source) => {
                            tracing::warn!(%source, room = %name, "child record could not be encoded")
                        }
                    }
                }
                None => continuum_base::probe!(
                    class = "activity.child_announce_failed",
                    room = %name,
                    parent = %parent_id.as_uuid(),
                    error = "parent room not in this peer's subscriptions",
                    "the parent is known by id only — no wall to announce on"
                ),
            }
        }
        // THE ACTIVITY DOES WHAT ITS RECIPE SAYS (S3). Until now a recipe could
        // describe a room and nothing else; every activity with behaviour was Rust.
        // The pipeline runs here, in the one birth path, so every caller gets it —
        // with the room seeded (`$room.id`, `$room.name`) and the resolved params as
        // `$args`. A caller with no executor at hand and a recipe that declares a
        // pipeline is a WIRING fact, probed by name, never a silent no-op.
        let pipeline = if recipe_def.pipeline.is_empty() {
            None
        } else {
            match executor {
                Some(exec) => {
                    let seed = vec![(
                        "room".to_string(),
                        serde_json::json!({
                            "id": room.channel.as_uuid().to_string(),
                            "name": name,
                            "recipe": recipe,
                        }),
                    )];
                    // THE BUNDLE SINK: every `saves` in this recipe lands on THIS room
                    // as its activity-state record (kind = the recipe purpose), so
                    // whoever resumes the activity reads the bundle, not a process map.
                    let sink: std::sync::Arc<dyn crate::experience::activity_state::StepStateSink> =
                        std::sync::Arc::new(crate::experience::activity_state::BoundStateSink::new(
                            std::sync::Arc::new(crate::experience::activity_state::WallActivityStateStore::new(std::sync::Arc::new(airc.clone()))),
                            room.clone(),
                            recipe_def.purpose.clone(),
                        ));
                    let receipt = match crate::recipe::PipelineExecutor::new(exec)
                        .with_caller(caller.clone())
                        .with_origin(crate::recipe::run_record::RunOrigin {
                            recipe: Some((recipe_def.id.0, recipe_def.version)),
                            room: Some(room.channel.as_uuid()),
                        })
                        .with_state_sink(sink)
                        .run_with(&recipe_def.purpose, &recipe_def.pipeline, pipeline_args, seed)
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            // A FAILED BIRTH IS ARCHIVED, never left as a bound, empty room the
                            // rail lists and the reconciler reseats (2026-09-12: three Rust rounds
                            // and a seed-4 round born with zero cards, card 5f36ce37).
                            let note = format!("birth failed: {e}");
                            let standing = RoomStanding { archived: true, protected: false, note: Some(note.clone()) };
                            match serde_json::to_string(&standing) { // boundary: the standing wall record — airc's wire + store
                                Ok(body) => {
                                    let _ = airc
                                        .publish_wall_post_in(&room, STANDING_WALL_CATEGORY.to_string(), body, None)
                                        .await;
                                }
                                Err(_) => {}
                            }
                            continuum_base::probe!(
                                class = "activity.birth_failed_archived",
                                room = %room.channel,
                                error = %e,
                                "the recipe's pipeline failed after the room was bound — archived, not left half-born"
                            );
                            return Err(CommandError::Internal(format!(
                                "room {} was created and bound, but its recipe's pipeline \
                                 failed: {e} — the room is archived",
                                room.channel
                            )));
                        }
                    };
                    Some(ActivityPipelineReceipt {
                        run_id: receipt.run_id,
                        phase: receipt.phase,
                        steps_run: receipt.steps_run,
                        steps_skipped: receipt.steps_skipped,
                        held_at: receipt.held_at,
                        trace: receipt.trace,
                    })
                }
                None => {
                    continuum_base::probe!(
                        class = "activity.pipeline.unrun",
                        room = %name,
                        recipe = %recipe,
                        steps = recipe_def.pipeline.len() as u64,
                        "the recipe declares a pipeline but this spawn seam has no command \
                         executor — the room exists, its behaviour did not run"
                    );
                    None
                }
            }
        };
        Ok(ActivitySpawnResult {
            room_id: room.channel,
            name: room.name,
            recipe: recipe.to_string(),
            binding_post_id: post_id,
            pipeline,
        })
    }
}

// ─────────────────────────── standing ───────────────────────────
/// The standing of ONE room the caller named — never "whatever room the
/// scope's pointer happens to be on".
///
/// Joel, 2026-09-05: "You never work in rooms. You just use whatever one of
/// you reuses." Every activity verb that means a specific room must be able
/// to say so; the pointer default silently archives a plausible wrong room and
/// nothing in the receipt says which. Same split airc makes for
/// `wall_posts` / `wall_posts_in` and `work_board` / `work_board_in`.
pub(super) async fn standing_in(airc: &Airc, room: &airc_lib::Room) -> Result<RoomStanding, CommandError> {
    let posts = airc
        .wall_posts_in(room, Some(STANDING_WALL_CATEGORY))
        .await
        .map_err(|source| {
            CommandError::Internal(format!(
                "could not read room standing for #{}: {source}",
                room.name
            ))
        })?;
    project_standing(&posts).map_err(|source| CommandError::Internal(source.to_string()))
}

/// Publish a merged standing on ONE room, preserving every field the caller
/// did not set.
pub(super) async fn publish_standing_in(
    airc: &Airc,
    room: &airc_lib::Room,
    next: &RoomStanding,
) -> Result<String, CommandError> {
    let body = serde_json::to_string(next)
        .map_err(|source| CommandError::Internal(format!("encode standing: {source}")))?;
    airc.publish_wall_post_in(room, STANDING_WALL_CATEGORY.to_string(), body, None)
        .await
        .map(|id| id.to_string())
        .map_err(|source| {
            CommandError::Internal(format!(
                "could not publish room standing on #{}: {source}",
                room.name
            ))
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
    /// The room whose activity this concludes — its id or its name. Omitted =
    /// the caller's current room. Name it: a verb that means a specific room
    /// must never depend on where the scope's pointer happens to stand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room: Option<String>,
    /// `true` concludes the activity, `false` reopens it. Reversible on purpose —
    /// concluding is a judgement, and judgements get revised.
    #[serde(default = "default_true")]
    pub archived: bool,
    /// Why this activity is finished (or live again).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
}

pub(super) fn default_true() -> bool {
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
        let room = resolve_room(&airc, p.room.as_deref()).await?;
        let mut standing = standing_in(&airc, &room).await?;
        standing.archived = p.archived;
        if p.note.is_some() {
            standing.note = p.note;
        }
        let post_id = publish_standing_in(&airc, &room, &standing).await?;
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
    /// The room to protect (or release) — its id or its name. Omitted = the
    /// caller's current room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room: Option<String>,
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
        let room = resolve_room(&airc, p.room.as_deref()).await?;
        let mut standing = standing_in(&airc, &room).await?;
        standing.protected = p.protected;
        if p.note.is_some() {
            standing.note = p.note;
        }
        let post_id = publish_standing_in(&airc, &room, &standing).await?;
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
// ─────────────────────────── activity/state ───────────────────────────
/// Read an activity's saved bundle — the state its recipe and its tracker saved on
/// the room (card c9ddb911). The same pipe a resuming node reads; a human or a
/// citizen standing in the room sees exactly what a resume would restore.
pub struct ActivityState {
    pub registry: PersonaAircRuntimeRegistry,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/experience/ActivityStateParams.ts")]
pub struct ActivityStateParams {
    /// Room name or id; the caller's current room when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room: Option<String>,
    /// One recipe kind (`benchmark/round`); every kind's newest bundle when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/experience/ActivityStateResult.ts")]
pub struct ActivityStateResult {
    #[ts(type = "string")]
    pub room_id: uuid::Uuid,
    pub room: String,
    /// Newest bundle per kind (one entry when `kind` was given).
    pub bundles: Vec<crate::experience::activity_state::ActivityStateRecord>,
}

#[async_trait]
impl ActionCommand for ActivityState {
    const NAME: &'static str = "activity/state";
    const ALIASES: &'static [&'static str] = &["activity_state"];
    // NOT native: a read of the bundle is a resume-time/operator verb, not a per-turn
    // act — offering it to every persona would grow the agentic tool surface past
    // its ceiling (llm_deliberation_faculty: shrink first, #333). Discoverable via
    // commands/list; callable by name.
    const NATIVE: bool = false;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Read the room's saved activity state — what its recipe and tracker saved on the \
         room (the bundle a resume restores from). Newest per recipe kind.";
    type Params = ActivityStateParams;
    type Output = ActivityStateResult;

    async fn run(&self, ctx: &Ctx, p: ActivityStateParams) -> Result<ActivityStateResult, CommandError> {
        use crate::experience::activity_state::{project_activity_state, ACTIVITY_STATE_WALL_CATEGORY};
        let airc = caller_airc(&self.registry, ctx)?;
        let room = resolve_room(&airc, p.room.as_deref()).await?;
        let posts = airc
            .wall_posts_in(&room, Some(ACTIVITY_STATE_WALL_CATEGORY))
            .await
            .map_err(|e| CommandError::Internal(format!("activity-state wall read failed: {e}")))?;
        let kinds: Vec<String> = match &p.kind {
            Some(k) => vec![k.clone()],
            None => {
                let mut seen = Vec::new();
                for post in posts.iter().rev() {
                    if let Ok(rec) = serde_json::from_str::<crate::experience::activity_state::ActivityStateRecord>(&post.body) { // ORM boundary: a record another writer could not encode is not a bundle
                        if !seen.contains(&rec.kind) {
                            seen.push(rec.kind);
                        }
                    }
                }
                seen
            }
        };
        let bundles = kinds
            .iter()
            .filter_map(|k| project_activity_state(&posts, k))
            .collect();
        Ok(ActivityStateResult { room_id: room.channel.as_uuid(), room: room.name.clone(), bundles })
    }
}

crate::register_command!(ActivitySpawn);

crate::register_command!(ActivityState);

crate::register_command!(ActivityRecipes);

crate::register_command!(ActivityInvite);

crate::register_command!(ActivityArchive);

crate::register_command!(ActivityProtect);

// ─────────────────────────── module ───────────────────────────
/// Hosts the `activity/*` verbs.
pub struct ActivityModule {
    pub(super) registry: PersonaAircRuntimeRegistry,
    /// See [`ActivitySpawn::executor_slot`] — this module owns the slot and hands a clone
    /// to the verb, exactly as `WorkModule` does for `benchmark/dispatch`.
    pub(super) executor_slot: std::sync::Arc<
        crate::runtime::LateBound<crate::runtime::command_executor::CommandExecutor>,
    >,
}

impl ActivityModule {
    pub fn new(registry: PersonaAircRuntimeRegistry) -> Self {
        Self {
            registry,
            executor_slot: std::sync::Arc::new(crate::runtime::LateBound::new(
                "activity::executor",
            )),
        }
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
                executor_slot: self.executor_slot.clone(),
            }),
            Arc::new(ActivityRecipes),
            Arc::new(ActivityInvite {
                registry: self.registry.clone(),
            }),
            Arc::new(ActivityArchive {
                registry: self.registry.clone(),
            }),
            Arc::new(ActivityProtect {
                registry: self.registry.clone(),
            }),
            Arc::new(ActivityState {
                registry: self.registry.clone(),
            }),
        ]
    }

    fn install_executor(
        &self,
        executor: std::sync::Arc<crate::runtime::command_executor::CommandExecutor>,
    ) {
        self.executor_slot.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
