//! A recipe run that outlives the task running it.
//!
//! One [`RecipeRunRecord`] per run, in the node's ORM (`recipe_runs`). It is the
//! truth about the run; a room's wall bundle and ViewState are projections of it.
//! The record holds an immutable snapshot of the steps it runs, so an edit to the
//! recipe file never changes a run already under way, and every dispatch it makes
//! is an [`Invocation`] with a stable id, written BEFORE the dispatch and settled
//! after it.
//!
//! The rule that makes it safe to stop anywhere (Astra / Codex root review,
//! 2026-10-13): an invocation whose outcome is not known is never repeated. A
//! dispatch the gate or the router refused never reached a handler
//! ([`DispatchOutcome::NotCommitted`]) and may be retried or skipped as the step
//! declares; anything that reached a handler and failed, and every timeout, is
//! [`DispatchOutcome::Uncertain`], and the run stops in
//! [`RunPhase::RecoveryRequired`] holding that invocation.
//!
//! Provenance only, never authority: the record keeps who started the run
//! ([`CallerRecord`]) for receipts. It never rebuilds a [`CallerIdentity`] from
//! the row — a resumed run is re-authorized as a live caller.

use crate::orm::{adapter::StorageAdapter, entity::BaseEntity, query::QueryBuilder, Entity, OrmEntity, OrmStore};
use std::sync::Arc;
use crate::recipe::types::RecipeStep;
use crate::routing::{CallerIdentity, CallerSource};
use crate::runtime::command_executor::DispatchOutcome;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;
use uuid::Uuid;

/// Where a run is. `is_active` on the record is exactly "not terminal".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RunPhase.ts")]
pub enum RunPhase {
    /// A task is walking the steps.
    Running,
    /// Stopped before `step`, which needs a human's yes. Nothing at or after it ran.
    Held { step: u32 },
    /// An invocation's outcome is unknown, so the run stopped rather than repeat
    /// it. `invocation` names it; a hand or a reconcile settles it.
    RecoveryRequired {
        #[ts(type = "string")]
        invocation: Uuid,
        reason: String,
    },
    /// Found `Running` at boot with no invocation in flight: the task died
    /// between invocations. Nothing is uncertain; continuing is safe.
    Interrupted,
    Completed,
    Failed { error: String },
    /// Cancelled at a boundary. An uncertain invocation is never "cancelled":
    /// a run cancelled while one is unknown stays `RecoveryRequired` with
    /// `cancel_requested` set.
    Cancelled,
}

impl RunPhase {
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunPhase::Completed | RunPhase::Failed { .. } | RunPhase::Cancelled)
    }
}

/// What one dispatch did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/recipe/InvocationState.ts")]
pub enum InvocationState {
    /// Written before the dispatch. A record that still says this after a crash
    /// is an invocation whose outcome nobody knows.
    Dispatching { attempt: u32 },
    Committed {
        #[ts(type = "unknown")]
        result: Value,
    },
    /// Refused before any handler ran. Safe to retry or skip.
    NotCommitted { error: String },
    /// Reached a handler (or timed out) and failed. Never retried or skipped.
    Uncertain { error: String },
}

/// One dispatch of one step (or one `each` item), with the params it was sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/recipe/Invocation.ts")]
pub struct Invocation {
    /// Stable: derived from (run, step, item), so the same dispatch has the same
    /// id across a crash and a resume, and a reconcile can ask about it by name.
    #[ts(type = "string")]
    pub id: Uuid,
    pub step: u32,
    #[ts(optional)]
    pub item: Option<u32>,
    pub command: String,
    #[ts(type = "unknown")]
    pub params: Value,
    pub state: InvocationState,
}

impl Invocation {
    pub fn id_for(run_id: Uuid, step: u32, item: Option<u32>) -> Uuid {
        let name = match item {
            Some(i) => format!("{step}/{i}"),
            None => format!("{step}"),
        };
        Uuid::new_v5(&run_id, name.as_bytes())
    }
}

/// Who started a run, for receipts. Deliberately not a [`CallerIdentity`]: the
/// granted capabilities a boundary verified for ONE dispatch are never stored and
/// never restored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/recipe/CallerRecord.ts")]
pub struct CallerRecord {
    #[ts(type = "string")]
    pub peer_id: Uuid,
    pub source: String,
}

impl CallerRecord {
    pub fn of(caller: &CallerIdentity) -> Self {
        let source = match &caller.source {
            CallerSource::Local => "local".to_string(),
            CallerSource::Airc => "airc".to_string(),
            CallerSource::LocalPersona => "local_persona".to_string(),
            CallerSource::Tcp => "tcp".to_string(),
            CallerSource::Ws => "ws".to_string(),
            CallerSource::PositronObserver { observer_id } => format!("positron_observer:{observer_id}"),
        };
        Self { peer_id: caller.peer_id.as_uuid(), source }
    }
}

/// Where the walk is: the next step to look at, and inside an `each` step the
/// next item.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RunCursor.ts")]
pub struct RunCursor {
    pub step: u32,
    #[ts(optional)]
    pub item: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "recipe_runs")]
#[entity(index(name = "idx_recipe_run_active", fields = ["isActive", "id"]))]
pub struct RecipeRunRecord {
    #[serde(flatten)]
    pub base: BaseEntity,
    pub purpose: String,
    /// The authored recipe this run snapshots, when it came from one. `None` for a
    /// pipeline handed to the executor directly.
    pub recipe_id: Option<Uuid>,
    pub recipe_version: Option<u32>,
    /// The steps as they were when the run began.
    #[entity(json)]
    pub pipeline: Vec<RecipeStep>,
    #[entity(json)]
    pub args: Value,
    #[entity(json)]
    pub seed: Map<String, Value>,
    #[entity(json)]
    pub caller: Option<CallerRecord>,
    /// The activity room this run belongs to, if it is a birth's pipeline.
    pub room_id: Option<Uuid>,
    #[entity(json)]
    pub phase: RunPhase,
    pub is_active: bool,
    /// Set by `recipe/cancel`; read at every boundary, so it survives the task.
    pub cancel_requested: bool,
    /// Bumped on every write by the execution owner.
    pub revision: u64,
    #[entity(json)]
    pub cursor: RunCursor,
    /// Every `outputTo` binding so far.
    #[entity(json)]
    pub bindings: Map<String, Value>,
    /// `each` arrays, frozen the first time the step is reached, keyed by step.
    #[entity(json)]
    pub frozen_each: Map<String, Value>,
    #[entity(json)]
    pub invocations: Vec<Invocation>,
    /// A `saves` patch committed with its step and not yet written to the room
    /// bundle. Retried without re-dispatching anything.
    #[entity(json)]
    pub pending_saves: Option<Map<String, Value>>,
    #[entity(json)]
    pub trace: Vec<String>,
    pub steps_run: u32,
    pub steps_skipped: u32,
}

/// Where a run's steps came from and which room it belongs to.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RunOrigin {
    pub recipe: Option<(Uuid, u32)>,
    pub room: Option<Uuid>,
}

impl RecipeRunRecord {
    /// A new run: `Running`, cursor at the first step, nothing written yet.
    pub fn begin(
        origin: RunOrigin,
        purpose: &str,
        pipeline: Vec<RecipeStep>,
        args: Value,
        seed: Map<String, Value>,
        caller: Option<CallerRecord>,
    ) -> Self {
        Self {
            base: BaseEntity::for_new_record(),
            purpose: purpose.to_string(),
            recipe_id: origin.recipe.map(|(id, _)| id),
            recipe_version: origin.recipe.map(|(_, version)| version),
            pipeline,
            args,
            seed,
            caller,
            room_id: origin.room,
            phase: RunPhase::Running,
            is_active: true,
            cancel_requested: false,
            revision: 0,
            cursor: RunCursor::default(),
            bindings: Map::new(),
            frozen_each: Map::new(),
            invocations: Vec::new(),
            pending_saves: None,
            trace: Vec::new(),
            steps_run: 0,
            steps_skipped: 0,
        }
    }

    /// The run's id: the row's primary key, which [`RecipeRunRecord::begin`] writes
    /// as a UUID. A row whose key is not one was not written by this module.
    pub fn run_id(&self) -> Result<Uuid, String> {
        Uuid::parse_str(&self.base.id)
            .map_err(|e| format!("recipe_runs row `{}` has a non-UUID id: {e}", self.base.id))
    }
}

/// Which invocation outcome a dispatch failure is, in record terms.
pub fn settled(outcome: DispatchOutcome, error: String) -> InvocationState {
    match outcome {
        DispatchOutcome::NotCommitted => InvocationState::NotCommitted { error },
        DispatchOutcome::Uncertain => InvocationState::Uncertain { error },
    }
}

/// Where the execution owner writes a run, and where a cancel is read back from.
/// The owner writes every field except `cancel_requested`; only `recipe/cancel`
/// writes that one, so neither can overwrite the other's fact.
#[async_trait::async_trait]
pub trait RunLedger: Send + Sync {
    async fn write(&self, run: &RecipeRunRecord) -> Result<(), String>;
    async fn cancel_requested(&self, run_id: Uuid) -> Result<bool, String>;
}

/// The node's `recipe_runs` collection.
pub struct RecipeRuns {
    store: OrmStore<RecipeRunRecord>,
}

/// Every field the execution owner writes: the record minus `cancel_requested`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OwnerFields<'a> {
    phase: &'a RunPhase,
    is_active: bool,
    revision: u64,
    cursor: &'a RunCursor,
    bindings: &'a Map<String, Value>,
    frozen_each: &'a Map<String, Value>,
    invocations: &'a [Invocation],
    pending_saves: &'a Option<Map<String, Value>>,
    trace: &'a [String],
    steps_run: u32,
    steps_skipped: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelField {
    cancel_requested: bool,
}

impl RecipeRuns {
    pub async fn open(adapter: Arc<dyn StorageAdapter>) -> Result<Self, String> {
        Ok(Self { store: OrmStore::new(adapter).await.map_err(|e| e.to_string())? })
    }

    pub async fn get(&self, run_id: Uuid) -> Result<Option<RecipeRunRecord>, String> {
        self.store.find_by_id(run_id).await.map_err(|e| e.to_string())
    }

    /// Every run not yet terminal.
    pub async fn active(&self) -> Result<Vec<RecipeRunRecord>, String> {
        let query = QueryBuilder::new(RecipeRunRecord::COLLECTION).filter_eq("isActive", true).build();
        let rows = self.store.find_matching(query).await.map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(|(_, run)| run).collect())
    }

    /// Record that a run is to stop at its next boundary. Writes only the flag.
    pub async fn request_cancel(&self, run_id: Uuid) -> Result<(), String> {
        self.store.update_fields(run_id, &CancelField { cancel_requested: true }).await.map_err(|e| e.to_string())
    }

    /// After a cancel is recorded: a run no task is walking (held for approval, or
    /// interrupted by a restart) is cancelled now, since no owner will reach a
    /// boundary to see the flag. A running run sees it at its next boundary. A run
    /// in `RecoveryRequired` stays there with the cancel recorded: its open
    /// invocation's outcome is still unknown, and cancelling cannot make it known.
    pub async fn settle_cancel_if_idle(&self, run_id: Uuid) -> Result<RecipeRunRecord, String> {
        let mut run = self.get(run_id).await?.ok_or_else(|| format!("no recipe run {run_id}"))?;
        if matches!(run.phase, RunPhase::Held { .. } | RunPhase::Interrupted) {
            run.phase = RunPhase::Cancelled;
            run.is_active = false;
            run.revision += 1;
            self.write(&run).await?;
        }
        Ok(run)
    }

    /// The boot pass (PR-A): no task owns a run across a restart, so every run the
    /// last process left `Running` is exposed for what it is. One with an
    /// invocation still `Dispatching` has an unknown outcome and becomes
    /// `RecoveryRequired`; one stopped between invocations is `Interrupted`.
    /// Nothing is re-dispatched. Returns how many of each.
    pub async fn expose_interrupted(&self) -> Result<(usize, usize), String> {
        let (mut recovery, mut interrupted) = (0, 0);
        for mut run in self.active().await? {
            if run.phase != RunPhase::Running {
                continue;
            }
            match run.invocations.iter().find(|inv| matches!(inv.state, InvocationState::Dispatching { .. })) {
                Some(inv) => {
                    run.phase = RunPhase::RecoveryRequired {
                        invocation: inv.id,
                        reason: "the core stopped while this invocation was being dispatched; its outcome is unknown".into(),
                    };
                    recovery += 1;
                }
                None => {
                    run.phase = RunPhase::Interrupted;
                    interrupted += 1;
                }
            }
            run.revision += 1;
            self.write(&run).await?;
            crate::probe!(class = "recipe.run.exposed_at_boot", run = %run.base.id, recipe = %run.purpose, phase = ?run.phase, "a run left Running by the last process");
        }
        Ok((recovery, interrupted))
    }
}

#[async_trait::async_trait]
impl RunLedger for RecipeRuns {
    async fn write(&self, run: &RecipeRunRecord) -> Result<(), String> {
        let id = run.run_id()?;
        if run.revision <= 1 && self.store.find_by_id(id).await.map_err(|e| e.to_string())?.is_none() {
            return self.store.save(id, run).await.map_err(|e| e.to_string());
        }
        let fields = OwnerFields {
            phase: &run.phase,
            is_active: run.is_active,
            revision: run.revision,
            cursor: &run.cursor,
            bindings: &run.bindings,
            frozen_each: &run.frozen_each,
            invocations: &run.invocations,
            pending_saves: &run.pending_saves,
            trace: &run.trace,
            steps_run: run.steps_run,
            steps_skipped: run.steps_skipped,
        };
        self.store.update_fields(id, &fields).await.map_err(|e| e.to_string())
    }

    async fn cancel_requested(&self, run_id: Uuid) -> Result<bool, String> {
        Ok(self.get(run_id).await?.map(|r| r.cancel_requested).unwrap_or(false)) // unwrap_or: a run not yet written has no cancel
    }
}
