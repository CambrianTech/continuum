//! `memory/consolidate` — the being-loop's received axis, made live-invokable.
//!
//! Reads a persona's SHARED lessons (`memory_type == "shared"`, written by
//! [`memory/share`](super::super) #2025) from its durable corpus and submits each into
//! the SAME training flywheel a live turn uses: `genome/training-trigger/submit` →
//! buckets by trait → gym-gated `evalSet` → auto `genome/job-create` → train → eval-lift
//! (#59) → page-in. This is the LAST WIRE of the received axis: a lesson one agent taught
//! becomes another agent's trained-in capability — adopted only if it lifts the benchmark,
//! so telepathy can never make her worse ([[lived-and-eval-experience-are-one-stream-one-being]]).
//!
//! Explicit + synchronous so the loop is watchable and measurable by hand:
//! ```text
//! continuum memory/consolidate --persona-id <peer> --base-model <base>
//! ```
//! The autonomic dream tick calls the same producer core later (resolving `base_model`
//! from the serving snapshot); this command lets us RUN the loop and SEE a lesson become
//! weights before wiring the tick — the methodical "explicit command first, autonomic
//! second" discipline.
//!
//! Zero new serving code: it reuses the corpus read (`data/list`, the hydrate pattern) and
//! the producer's [`build_submit_params`]/[`plan_received`] — one payload contract for the
//! live-turn and received-lesson sources alike.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::experience::ExperienceRecord;
use crate::log_info;
use crate::logging::TimingGuard;
use crate::memory::MemoryRecord;
use crate::modules::memory::MemoryState;
use crate::persona::domain_classifier::DomainClassifier;
use crate::persona::training_producer::{build_submit_params, plan_received};
use crate::routing::CallerIdentity;
use crate::runtime::InProcessTransport;
use crate::sdk_codegen::CommandError;
use continuum_client::Connection;

/// Params for `memory/consolidate`. Flat + CLI-friendly.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryConsolidateParams.ts"
)]
pub struct MemoryConsolidateParams {
    /// The persona whose received lessons to consolidate — its airc peer id / corpus key.
    pub persona_id: String,
    /// Display name carried into training provenance. Defaults to the persona id.
    #[serde(default)]
    #[ts(optional)]
    pub persona_name: Option<String>,
    /// The base model the lesson-gene trains on. The operator names it for this explicit
    /// command; the autonomic tick will resolve it from the serving snapshot instead.
    pub base_model: String,
}

/// What `memory/consolidate` did — a truthful receipt (synchronous submit, so `consolidated`
/// is what actually reached the trigger, not a fire-and-forget promise).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/ConsolidateResult.ts"
)]
pub struct ConsolidateResult {
    /// Shared lessons found in the persona's corpus.
    pub shared_lessons: usize,
    /// Of those, how many were dispatched into the training flywheel.
    pub consolidated: usize,
}

crate::action_command! {
    /// Consolidate a persona's SHARED lessons (received from other agents via `memory/share`)
    /// into its genome: submit each to the training flywheel, gated per-consolidation by
    /// whole-being benchmark lift (#59). The last wire of the being-loop's received axis —
    /// telepathy reaching weights.
    pub struct MemoryConsolidate { state: Arc<MemoryState> }
    name: "memory/consolidate",
    access: AiSafe,
    params: MemoryConsolidateParams,
    output: ConsolidateResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_consolidate");

        let persona_uuid = uuid::Uuid::parse_str(&p.persona_id).map_err(|e| {
            CommandError::Internal(format!("memory/consolidate: persona_id must be a uuid: {e}"))
        })?;
        let persona_name = p.persona_name.clone().unwrap_or_else(|| p.persona_id.clone());

        let executor = this.state.executor().map_err(CommandError::Internal)?;

        // Read the persona's SHARED lessons from durable truth (the rows memory/share wrote),
        // filtered server-side to memory_type "shared" — the same data/list path hydrate uses.
        let listed = executor
            .execute_json(
                "data/list",
                serde_json::json!({
                    "collection": super::MEMORIES_COLLECTION,
                    "dbPath": super::persona_db_handle(&p.persona_id),
                    "filter": { "persona_id": p.persona_id, "memory_type": "shared" },
                }),
            )
            .await
            .map_err(|e| {
                CommandError::Internal(format!("memory/consolidate: data/list failed: {e}"))
            })?;

        let items = listed
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let shared_lessons = items.len();

        // Dispatch AS the persona: its LocalPersona identity gates the Privileged submit,
        // exactly like the live-turn producer ([[persona-is-a-client]]).
        let conn = Connection::new(InProcessTransport::new(
            executor,
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona_uuid),
            )),
        ));
        let classifier = DomainClassifier::new();
        let mut consolidated = 0usize;

        for item in &items {
            let Some(data) = item.get("data") else { continue };
            // Fold ORM camelCase TOP-LEVEL keys → snake_case (nested `context` untouched),
            // exactly as hydrate_corpus_if_missing does, so MemoryRecord deserializes.
            let data = match data {
                serde_json::Value::Object(obj) => serde_json::Value::Object(
                    obj.iter()
                        .map(|(k, v)| {
                            (crate::orm::adapter::naming::to_snake_case(k), v.clone())
                        })
                        .collect(),
                ),
                other => other.clone(),
            };
            let Ok(record) = serde_json::from_value::<MemoryRecord>(data) else {
                continue;
            };
            // ONE source of truth for received → (topic, lesson): from_shared_lesson.
            let episode = ExperienceRecord::from_shared_lesson(&record);
            if episode.answer.trim().is_empty() {
                continue;
            }
            let plan = plan_received(&classifier, &episode.task.prompt, &episode.answer);
            let params =
                build_submit_params(persona_uuid, &persona_name, &p.base_model, &plan, "received-lesson");
            match conn
                .commands()
                .execute_value("genome/training-trigger/submit", params)
                .await
            {
                Ok(_) => consolidated += 1,
                Err(e) => log_info!(
                    "module",
                    "memory_consolidate",
                    "submit failed for a shared lesson (continuing): {e}"
                ),
            }
        }

        log_info!(
            "module",
            "memory_consolidate",
            "Consolidated {consolidated}/{shared_lessons} shared lessons for {} into the training flywheel",
            p.persona_id
        );
        Ok(ConsolidateResult { shared_lessons, consolidated })
    }
}
