use super::*;
use crate::experience::room_path::{RoomPath, RoomPathError};

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
    // THE ROOM TREE (ACTIVITY-ROOM-PATTERNS.md): a room's identity is its PATH, and the airc
    // name IS the path, so the same path is the same room on every node. A name with no `/`
    // goes under its explicit parent, else under its recipe's `base` (a benchmark run under
    // `academy/benchmarks`, by what it is), else at the top.
    let path = activity_path(airc, name, parent, recipe_def.base.as_deref()).await?;
    // Every ancestor is a room too: a missing one is opened (as a category) before its child.
    let parent_room = open_ancestors(airc, &path).await?;
    let parent = parent_room.as_ref().map(|r| r.channel);
    {
        // OPENING NEVER MOVES FOCUS: `subscribe_room` joins without setting the caller's
        // current room (airc `Focus::Keep`). Creating a room and looking at it are different
        // acts; a daemon opening a benchmark must not move a human's or a resident's focus.
        let room = open_path_room(airc, &path, recipe, parent_room.as_ref(), child_suite.clone()).await?;
        // A reused name resolves to the SAME room. Read its type before publishing
        // anything: a benchmark spawn once rebound the live project room, after which
        // benchmark cleanup evicted its coding peers as "finished".
        let posts = airc
            .wall_posts_in(&room, Some(RECIPE_WALL_CATEGORY))
            .await
            .map_err(|e| {
                CommandError::Internal(format!("cannot read recipe for room {}: {e}", path.uri()))
            })?;
        validate_spawn_binding(path.as_str(), recipe, &posts)?;
        // RESUME-OR-CREATE: a room already bound to this recipe IS the activity, resumed:
        // brought back to live if it was archived, with no second binding, no second child
        // record and no second run of its creation pipeline (which would import its cards
        // again). Resuming a benchmark is opening its path again.
        if let Some(bound) = posts.last() {
            crate::experience::spawned_rooms::record(room.channel.as_uuid(), path.as_str());
            let standing = standing_in(airc, &room).await?;
            if standing.archived {
                publish_standing_in(airc, &room, &RoomStanding { archived: false, note: Some("resumed".into()), ..standing }).await?;
            }
            continuum_base::probe!(
                class = "activity.resumed",
                room = %path.uri(),
                recipe = %recipe,
                was_archived = standing.archived,
                "an existing activity was opened again: resumed, not re-created"
            );
            return Ok(ActivitySpawnResult {
                room_id: room.channel,
                name: room.name,
                recipe: recipe.to_string(),
                binding_post_id: bound.post_id,
                path,
                opened: ActivityOpened::Resumed,
                pipeline: None,
            });
        }
        // Every spawned room enters the node's adoption set the moment it exists, so
        // the presence emitter bridges its transcript (chat store + rail) on the next
        // refresh — the same minute, like a bench run room, not never (card 3d4b3d9c).
        crate::experience::spawned_rooms::record(room.channel.as_uuid(), path.as_str());

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
            path: Some(path.clone()),
        };
        let body = serde_json::to_string(&binding) // boundary: the recipe binding wall record — airc's wire + store
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

        // THE CHILD ANNOUNCES ITSELF TO ITS PARENT (experience::children): any node reading
        // the parent can walk to it and seat its residents.
        if let Some(parent_room) = parent_room.as_ref() {
            announce_child(airc, parent_room, &room, &path, recipe, child_suite).await;
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
                            "name": path.as_str(),
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
            path,
            opened: ActivityOpened::Created,
            pipeline,
        })
    }
}

/// The recipe a room on the path to an activity is opened as when nothing has opened it yet:
/// a category (`academy`, `academy/benchmarks`), whose content is the rooms beneath it.
pub const CATEGORY_RECIPE: &str = "category";

/// The place an activity's room has in the room tree. A `name` holding a `/` (or an
/// `airc://room:` URI) IS its path; otherwise it goes inside the explicit `parent` (at that room's
/// own recorded place), else inside the recipe's `base`, else at the top.
pub(super) async fn activity_path(
    airc: &Airc,
    name: &str,
    parent: Option<RoomId>,
    base: Option<&str>,
) -> Result<RoomPath, CommandError> {
    let invalid = |e: RoomPathError| CommandError::Invalid(format!("not a room path: {e}"));
    let bare = name.trim().trim_matches('/');
    if bare.contains('/') || bare.starts_with("airc://") {
        return RoomPath::parse(name).map_err(invalid);
    }
    let under = match parent {
        Some(id) => {
            let set = airc.subscription_set().await.map_err(|e| {
                CommandError::Internal(format!("cannot read this peer's rooms to place {name:?}: {e}"))
            })?;
            let parent = set.all().map(|sub| sub.as_room()).find(|room| room.channel == id).ok_or_else(|| {
                CommandError::Invalid(format!(
                    "parent room {} is not one of this peer's rooms, so {name:?} cannot be placed inside it: \
                     join the parent first, or name the full path (`<parent>/{name}`)",
                    id.as_uuid()
                ))
            })?;
            Some(place_of(airc, &parent).await?)
        }
        None => base.map(RoomPath::parse).transpose().map_err(invalid)?,
    };
    match under {
        Some(under) => under.child(bare).map_err(invalid),
        None => RoomPath::parse(bare).map_err(invalid),
    }
}

/// Where `room` sits in the tree: the path its binding records, else (a room bound before the
/// tree, like `academy`) its channel's own name.
async fn place_of(airc: &Airc, room: &airc_lib::Room) -> Result<RoomPath, CommandError> {
    let posts = airc
        .wall_posts_in(room, Some(RECIPE_WALL_CATEGORY))
        .await
        .map_err(|e| CommandError::Internal(format!("cannot read recipe for room #{}: {e}", room.name)))?;
    let recorded = crate::experience::binding::project_binding(&posts)
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .and_then(|binding| binding.path);
    match recorded {
        Some(path) => Ok(path),
        None => RoomPath::parse(&room.name)
            .map_err(|e| CommandError::Invalid(format!("room #{} has no place in the room tree: {e}", room.name))),
    }
}

/// Open every room `path` sits inside (outermost first) and return its direct parent. One nothing
/// has opened yet is bound as a [`CATEGORY_RECIPE`] room at its place and announced on its own
/// parent, so the whole tree is walkable from its root. Never moves focus.
pub(super) async fn open_ancestors(airc: &Airc, path: &RoomPath) -> Result<Option<airc_lib::Room>, CommandError> {
    let mut parent: Option<airc_lib::Room> = None;
    for ancestor in path.ancestors() {
        let room = airc.subscribe_room(&ancestor.channel_key()).await.map_err(|source| {
            CommandError::Invalid(format!("could not open {}, a room above {}: {source}", ancestor.uri(), path.uri()))
        })?;
        let bound = airc
            .wall_posts_in(&room, Some(RECIPE_WALL_CATEGORY))
            .await
            .map_err(|e| CommandError::Internal(format!("cannot read recipe for room {}: {e}", ancestor.uri())))?;
        if bound.is_empty() {
            let binding = RoomRecipeBinding {
                recipe: CATEGORY_RECIPE.to_string(),
                parent: parent.as_ref().map(|p| p.channel),
                params: Default::default(),
                path: Some(ancestor.clone()),
            };
            let body = serde_json::to_string(&binding) // boundary: the category's recipe binding wall record — airc's wire + store
                .map_err(|source| CommandError::Internal(format!("encode recipe binding: {source}")))?;
            airc.publish_wall_post_in(&room, RECIPE_WALL_CATEGORY.to_string(), body, None)
                .await
                .map_err(|source| CommandError::Internal(format!("could not bind {} as a category: {source}", ancestor.uri())))?;
            crate::experience::spawned_rooms::record(room.channel.as_uuid(), ancestor.as_str());
            if let Some(above) = parent.as_ref() {
                announce_child(airc, above, &room, &ancestor, CATEGORY_RECIPE, None).await;
            }
        }
        parent = Some(room);
    }
    Ok(parent)
}

/// A child room announces itself on its parent's wall ([`crate::experience::children`]), so a
/// node that never opened it can find it. Never fails the opening: a missing record is probed.
/// The room at `path`: its tree channel when this scope holds it, else a run opened before the
/// room tree, adopted, else the room its parent announces at this path (opened first on another
/// node), else a new tree channel. Before the tree a nested activity's channel was its
/// last segment alone, so opening such a run by its tree key would make a second, empty room and
/// orphan the first's cards, board and records. A pre-tree room is adopted only when this scope
/// holds it and it is bound to the same recipe; adopting writes its path into its binding and
/// announces it to its parent, once, and every later open resumes it by that binding.
async fn open_path_room(
    airc: &Airc,
    path: &RoomPath,
    recipe: &str,
    parent: Option<&airc_lib::Room>,
    suite: Option<String>,
) -> Result<airc_lib::Room, CommandError> {
    let key = path.channel_key();
    if let Some(room) = held_room(airc, &key).await? {
        return Ok(room);
    }
    if let Some(pre_tree) = path.pre_tree_channel() {
        if let Some(room) = held_room(airc, pre_tree).await? {
            let posts = airc.wall_posts_in(&room, Some(RECIPE_WALL_CATEGORY)).await.map_err(|e| {
                CommandError::Internal(format!("cannot read recipe for room {pre_tree:?}: {e}"))
            })?;
            let binding = crate::experience::binding::project_binding(&posts)
                .map_err(|e| CommandError::Internal(format!("cannot read recipe for room {pre_tree:?}: {e}")))?;
            match binding {
                Some(binding) if binding.recipe == recipe && binding.path.as_ref() == Some(path) => return Ok(room),
                Some(binding) if binding.recipe == recipe && binding.path.is_none() => {
                    let adopted = RoomRecipeBinding { path: Some(path.clone()), ..binding };
                    let body = serde_json::to_string(&adopted) // boundary: the recipe binding wall record — airc's wire + store
                        .map_err(|source| CommandError::Internal(format!("encode recipe binding: {source}")))?;
                    airc.publish_wall_post_in(&room, RECIPE_WALL_CATEGORY.to_string(), body, None).await.map_err(|source| {
                        CommandError::Internal(format!("room {pre_tree:?} could not take its path {}: {source}", path.uri()))
                    })?;
                    if let Some(parent) = parent {
                        announce_child(airc, parent, &room, path, recipe, suite).await;
                    }
                    continuum_base::probe!(
                        class = "activity.pre_tree_adopted",
                        room = %path.uri(),
                        channel = %pre_tree,
                        recipe = %recipe,
                        "a room opened before the room tree now stands at its path: adopted, never duplicated"
                    );
                    return Ok(room);
                }
                _ => {}
            }
        }
    }
    let channel = match parent {
        Some(parent) => announced_channel(airc, parent, path).await?.unwrap_or(key),
        None => key,
    };
    airc.subscribe_room(&channel)
        .await
        .map_err(|source| CommandError::Invalid(format!("could not open room {}: {source}", path.uri())))
}

/// The channel the parent's wall announces for `path`, when another node opened it first: any
/// node walks the tree to the room that already stands there (a pre-tree run another node
/// adopted keeps its old channel), never makes a second one beside it. A record names the room
/// by id, so the channel is the candidate name whose derived id matches; `None` when the wall
/// names no room at this path. A record naming a room by neither name is refused, never guessed.
async fn announced_channel(airc: &Airc, parent: &airc_lib::Room, path: &RoomPath) -> Result<Option<String>, CommandError> {
    let posts = airc
        .wall_posts_in(parent, Some(crate::experience::children::CHILD_WALL_CATEGORY))
        .await
        .map_err(|e| CommandError::Internal(format!("cannot read the children of {}: {e}", parent.name)))?;
    let Some(announced) = crate::experience::children::project_children(&posts)
        .into_iter()
        .filter(|record| record.name == path.as_str())
        .max_by_key(|record| record.spawned_at_ms)
    else {
        return Ok(None);
    };
    for candidate in std::iter::once(path.channel_key()).chain(path.pre_tree_channel().map(str::to_string)) {
        let derived = airc_lib::Room::from_name(airc.home(), &candidate)
            .map_err(|e| CommandError::Internal(format!("cannot derive channel {candidate:?}: {e}")))?;
        if derived.channel.as_uuid() == announced.room_id {
            return Ok(Some(candidate));
        }
    }
    Err(CommandError::Invalid(format!(
        "{}'s parent announces room {} at this path, a channel this node cannot derive from the path; \
         open it by that id instead of making a second room",
        path.uri(),
        announced.room_id
    )))
}

/// The room this scope already holds under `channel`, without joining it: `None` when it holds
/// none (airc refuses an unheld channel as a route error), any other failure surfaced.
async fn held_room(airc: &Airc, channel: &str) -> Result<Option<airc_lib::Room>, CommandError> {
    match airc.room_by_name_or_channel(channel, "open the activity room").await {
        Ok(room) => Ok(Some(room)),
        Err(airc_lib::AircError::Route(_)) => Ok(None),
        Err(source) => Err(CommandError::Internal(format!("cannot read this scope's rooms for {channel:?}: {source}"))),
    }
}

async fn announce_child(airc: &Airc, parent: &airc_lib::Room, room: &airc_lib::Room, path: &RoomPath, recipe: &str, suite: Option<String>) {
    let record = crate::experience::children::RoomChildRecord {
        room_id: room.channel.as_uuid(),
        name: path.as_str().to_string(),
        recipe: recipe.to_string(),
        suite,
        spawned_at_ms: continuum_base::time::EpochMs::now().millis(), // pre-1970 reads 0: a pre-epoch clock stamps 0 — the record still names the room
    };
    let outcome = match serde_json::to_string(&record) { // boundary: the child record on the parent's wall — airc's wire + store
        Ok(body) => airc
            .publish_wall_post_in(parent, crate::experience::children::CHILD_WALL_CATEGORY.to_string(), body, None)
            .await
            .map(|_| ())
            .map_err(|source| source.to_string()),
        Err(source) => Err(source.to_string()),
    };
    match outcome {
        Ok(()) => continuum_base::probe!(
            class = "activity.child_announced",
            room = %path.uri(),
            parent = %parent.name,
            recipe = %recipe,
            "the activity is on its parent's wall: any node reading the parent can seat its residents"
        ),
        Err(error) => continuum_base::probe!(
            class = "activity.child_announce_failed",
            room = %path.uri(),
            parent = %parent.name,
            error = %error,
            "the room exists but its parent never heard of it: remote nodes cannot seat into it until it is opened again"
        ),
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
    const DISCLOSURE: continuum_base::tool_disclosure::Disclosure = continuum_base::tool_disclosure::Disclosure::OnDemand;
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
    const DISCLOSURE: continuum_base::tool_disclosure::Disclosure = continuum_base::tool_disclosure::Disclosure::OnDemand;
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
