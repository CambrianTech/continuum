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

use std::future::Future;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::experience::ExperienceRecord;
use crate::commands::training_trigger::submit::SubmitOutcome;
use crate::log_info;
use crate::logging::TimingGuard;
use crate::memory::MemoryRecord;
use crate::modules::data::DataListResult;
use crate::modules::memory::MemoryState;
use crate::persona::domain_classifier::DomainClassifier;
use crate::persona::training_producer::{build_submit_params, plan_received, submit_training};
use crate::routing::CallerIdentity;
use crate::runtime::InProcessTransport;
use crate::sdk_codegen::CommandError;
use continuum_client::{ClientError, Connection};

/// Params for `memory/consolidate`. Flat + CLI-friendly.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryConsolidateParams.ts"
)]
pub struct MemoryConsolidateParams {
    /// The persona whose received lessons to consolidate — its airc peer id / corpus key.
    pub persona_id: crate::identity::PersonaRef,
    /// Display name carried into training provenance. Defaults to the persona id.
    #[serde(default)]
    #[ts(optional)]
    pub persona_name: Option<String>,
    /// The base model the lesson-gene trains on. The operator names it for this explicit
    /// command; the autonomic tick will resolve it from the serving snapshot instead.
    pub base_model: String,
    /// Resume watermark: consolidate ONLY shared lessons with a timestamp strictly
    /// after this (rfc3339, lexicographically ordered). Omit to consolidate all (the
    /// explicit first run). The autonomic tick persists [`ConsolidateResult::latest_consolidated_ts`]
    /// and passes it back here next cycle. A partial timestamp group may be retried;
    /// this watermark does not provide exactly-once delivery into the trigger.
    #[serde(default)]
    #[ts(optional)]
    pub since_timestamp: Option<String>,
}

/// Whether a shared lesson is newer than the resume watermark. Pure so the
/// watermark contract is unit-testable without the executor. rfc3339 timestamps compare
/// lexicographically, so a plain `>` is a correct chronological test for same-format
/// stamps; an absent `since` admits everything (the first, full run).
pub(crate) fn is_after_watermark(record_ts: &str, since: Option<&str>) -> bool {
    match since {
        Some(watermark) => record_ts > watermark,
        None => true,
    }
}

/// What `memory/consolidate` did — a synchronous receipt of the trigger's accepted
/// submissions. Acceptance can mean buffered examples; it does not prove trained weights.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/ConsolidateResult.ts"
)]
pub struct ConsolidateResult {
    /// Shared lessons found in the persona's corpus (after the `since_timestamp` watermark).
    pub shared_lessons: usize,
    /// Of those, how many the trigger accepted (buffered or dispatched).
    pub consolidated: usize,
    /// Newest timestamp in the contiguous prefix of fully accepted timestamp groups.
    /// No refused or unprocessed lesson is skipped. `None` if no group completed, even
    /// when part of the first group was accepted. Retrying a partial group can replay
    /// accepted examples; DispatchFailed also retains examples in a volatile bucket.
    #[serde(default)]
    #[ts(optional)]
    pub latest_consolidated_ts: Option<String>,
}

/// Decode a complete snapshot before submitting anything. data/list has no default
/// order or page limit; explicitly requesting ascending order below and checking its
/// total prevents a capped/partial result from silently advancing past unseen lessons.
fn shared_lessons_from_list(
    listed: serde_json::Value,
    since: Option<&str>,
) -> Result<Vec<MemoryRecord>, CommandError> {
    let listed: DataListResult = serde_json::from_value(listed).map_err(|e| {
        CommandError::Internal(format!("memory/consolidate: invalid data/list result: {e}"))
    })?;
    if listed.items.len() != listed.total as usize {
        return Err(CommandError::Internal(format!(
            "memory/consolidate: incomplete shared-lesson snapshot: received {} of {} rows; watermark unchanged",
            listed.items.len(), listed.total
        )));
    }
    let mut records = Vec::with_capacity(listed.items.len());
    for (index, item) in listed.items.into_iter().enumerate() {
        let data = match item {
            serde_json::Value::Object(mut obj) => obj.remove("data"),
            _ => None,
        }
        .ok_or_else(|| {
            CommandError::Internal(format!(
                "memory/consolidate: shared-lesson row {index} has no data; watermark unchanged"
            ))
        })?;
        // Fold only ORM top-level names; move the values rather than cloning the corpus.
        let data = match data {
            serde_json::Value::Object(obj) => serde_json::Value::Object(
                obj.into_iter()
                    .map(|(k, v)| (crate::orm::adapter::naming::to_snake_case(&k), v))
                    .collect(),
            ),
            other => other,
        };
        let record: MemoryRecord = serde_json::from_value(data).map_err(|e| {
            CommandError::Internal(format!(
                "memory/consolidate: invalid shared-lesson row {index}: {e}; watermark unchanged"
            ))
        })?;
        // Empty lessons are intentionally ineligible, not failed submissions. Validate
        // the whole snapshot first, then exclude them before grouping or counting.
        if is_after_watermark(&record.timestamp, since) && !record.content.trim().is_empty() {
            records.push(record);
        }
    }
    records.sort_unstable_by(|a, b| a.timestamp.cmp(&b.timestamp).then(a.id.cmp(&b.id)));
    Ok(records)
}

/// Commit only complete timestamp groups. A strict timestamp cursor cannot represent
/// a partially accepted group, so stop on the first refusal and leave that group for
/// retry. This is at-least-once retry behavior, not trigger-side deduplication.
async fn consolidate_in_order<'a, F, Fut>(
    records: &'a [MemoryRecord],
    mut submit: F,
) -> ConsolidateResult
where
    F: FnMut(&'a MemoryRecord) -> Fut,
    Fut: Future<Output = Result<SubmitOutcome, ClientError>>,
{
    let mut result = ConsolidateResult {
        shared_lessons: records.len(),
        consolidated: 0,
        latest_consolidated_ts: None,
    };
    for group in records.chunk_by(|a, b| a.timestamp == b.timestamp) {
        for record in group {
            match submit(record).await {
                Ok(outcome) if outcome.success => {}
                Ok(outcome) => {
                    log_info!(
                        "module", "memory_consolidate",
                        "shared lesson {} not accepted; stopping before advancing its timestamp ({}): {}",
                        record.id,
                        outcome.error_kind.as_deref().unwrap_or("unspecified"),
                        outcome.error.as_deref().unwrap_or("no diagnostic")
                    );
                    return result;
                }
                Err(e) => {
                    log_info!(
                        "module", "memory_consolidate",
                        "submit failed for shared lesson {}; stopping before advancing its timestamp: {e}",
                        record.id
                    );
                    return result;
                }
            }
            result.consolidated += 1;
        }
        result.latest_consolidated_ts = Some(group[0].timestamp.clone());
    }
    result
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

        // #164: accept the short-form persona id rosters display, not just a full UUID —
        // the same id_resolve primitive persona/* and work/* already use.
        let persona_uuid = crate::id_resolve::resolve(
            p.persona_id.as_str(),
            &crate::persona::card::ids(),
            "persona",
        )
        .map_err(CommandError::Invalid)?;
        let persona_name = p.persona_name.clone().unwrap_or_else(|| p.persona_id.to_string());

        let executor = this.state.executor().map_err(CommandError::Internal)?;

        // Read the persona's SHARED lessons from durable truth (the rows memory/share wrote),
        // filtered server-side to memory_type "shared" — the same data/list path hydrate uses.
        let mut filter = serde_json::json!({
            "persona_id": p.persona_id, "memory_type": "shared"
        });
        if let Some(since) = &p.since_timestamp {
            filter["timestamp"] = serde_json::json!({ "$gt": since });
        }
        let listed = executor
            .execute_json(
                "data/list",
                serde_json::json!({
                    "collection": super::MEMORIES_COLLECTION,
                    "dbPath": super::persona_db_handle(&p.persona_id),
                    "filter": filter,
                    "sort": [
                        { "field": "timestamp", "direction": "asc" },
                        { "field": "id", "direction": "asc" }
                    ],
                }),
            )
            .await
            .map_err(|e| {
                CommandError::Internal(format!("memory/consolidate: data/list failed: {e}"))
            })?;

        let records = shared_lessons_from_list(listed, p.since_timestamp.as_deref())?;

        // Dispatch AS the persona: its LocalPersona identity gates the Privileged submit,
        // exactly like the live-turn producer ([[persona-is-a-client]]).
        let conn = Connection::new(InProcessTransport::new(
            executor,
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(persona_uuid),
            )),
        ));
        let classifier = DomainClassifier::new();
        let result = consolidate_in_order(&records, |record| {
            let conn = &conn;
            let classifier = &classifier;
            let persona_name = &persona_name;
            let base_model = &p.base_model;
            async move {
                // ONE source of truth for received → (topic, lesson): from_shared_lesson.
                let episode = ExperienceRecord::from_shared_lesson(record);
                let plan = plan_received(classifier, &episode.task.prompt, &episode.answer);
                let params = build_submit_params(
                    persona_uuid, persona_name, base_model, &plan, "received-lesson"
                );
                submit_training(conn, params).await
            }
        }).await;

        log_info!(
            "module",
            "memory_consolidate",
            "Consolidated {}/{} shared lessons for {} into the training flywheel",
            result.consolidated, result.shared_lessons,
            p.persona_id
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::{consolidate_in_order, is_after_watermark, shared_lessons_from_list};
    use crate::persona::training_producer::submit_training;
    use continuum_client::{mock::MockTransport, ClientError, Connection};
    use serde_json::json;

    // what this catches: the resume cursor excludes completed timestamp groups;
    // failed or partial groups must remain above that cursor for the next run.
    #[test]
    fn watermark_admits_only_strictly_newer_lessons() {
        // First run: no watermark → every lesson is a candidate.
        assert!(is_after_watermark("2026-07-26T00:00:00Z", None));

        // A lesson strictly newer than the watermark is admitted...
        assert!(is_after_watermark(
            "2026-07-26T12:00:00Z",
            Some("2026-07-26T08:00:00Z")
        ));
        // ...one AT the watermark is not (already consolidated on the run that set it)...
        assert!(!is_after_watermark(
            "2026-07-26T08:00:00Z",
            Some("2026-07-26T08:00:00Z")
        ));
        // ...and an older one is not (a re-run must not re-train it).
        assert!(!is_after_watermark(
            "2026-07-25T23:59:59Z",
            Some("2026-07-26T08:00:00Z")
        ));
    }

    // what this catches: card 3eaabbc6 — a later success must never move the cursor
    // over an earlier refusal, including an accepted sibling with the same timestamp.
    // Exercise typed transport receipts through the production prefix driver so
    // treating Ok(success:false) as acceptance cannot evade this regression.
    #[tokio::test]
    async fn rejected_lesson_stops_at_the_last_complete_timestamp_group() {
        let first = "2026-09-08T00:00:00Z";
        let tied = "2026-09-08T01:00:00Z";
        let last = "2026-09-08T02:00:00Z";
        let items = [
            ("later", last, "later lesson"),
            ("tie-b", tied, "second tied lesson"),
            ("blank", first, "  "),
            ("first", first, "first lesson"),
            ("tie-a", tied, "first tied lesson"),
        ]
        .into_iter()
        .map(|(id, timestamp, content)| {
            json!({ "data": super::super::share::build_shared_record(
                "recipient", "teacher", content.to_string(), "scope", None, 0.6,
                id.to_string(), timestamp.to_string(),
            ) })
        })
        .collect::<Vec<_>>();
        let records =
            shared_lessons_from_list(json!({ "total": items.len(), "items": items }), None)
                .expect("complete valid snapshot");
        assert_eq!(
            records.len(),
            4,
            "empty lessons remain intentionally ineligible"
        );
        for (refused, failure, expected_calls, accepted, watermark) in [
            ("first", "InconsistentBucket", vec!["first"], 0, None),
            (
                "tie-a",
                "InconsistentBucket",
                vec!["first", "tie-a"],
                1,
                Some(first),
            ),
            (
                "tie-b",
                "DispatchFailed",
                vec!["first", "tie-a", "tie-b"],
                2,
                Some(first),
            ),
            (
                "tie-b",
                "malformed",
                vec!["first", "tie-a", "tie-b"],
                2,
                Some(first),
            ),
            (
                "tie-b",
                "transport",
                vec!["first", "tie-a", "tie-b"],
                2,
                Some(first),
            ),
            (
                "none",
                "none",
                vec!["first", "tie-a", "tie-b", "later"],
                4,
                Some(last),
            ),
        ] {
            let transport = MockTransport::new();
            for record in &records {
                let should_fail = record.id == refused;
                transport.respond_to("genome/training-trigger/submit", move |_| {
                    if !should_fail {
                        return Ok(json!({
                            "success": true, "outcome": "BatchAppended",
                            "currentCount": 1, "threshold": 16,
                        }));
                    }
                    match failure {
                        "transport" => Err(ClientError::Transport("connection lost".into())),
                        "malformed" => Ok(json!({ "outcome": "BatchAppended" })),
                        kind => Ok(json!({
                            "success": false, "errorKind": kind, "error": "submission refused",
                        })),
                    }
                });
            }
            let conn = Connection::new(transport);
            let mut calls = Vec::new();
            let result = consolidate_in_order(&records, |record| {
                calls.push(record.id.as_str());
                submit_training(&conn, json!({}))
            })
            .await;
            assert_eq!(
                calls, expected_calls,
                "no work after {failure} at {refused}"
            );
            assert_eq!(result.shared_lessons, 4);
            assert_eq!(result.consolidated, accepted);
            assert_eq!(result.latest_consolidated_ts.as_deref(), watermark);
        }
    }

    // what this catches: card 3eaabbc6 — a silently truncated or malformed data/list
    // snapshot must fail before any submit, rather than skip a row and advance past it.
    #[test]
    fn invalid_or_partial_lesson_snapshots_cannot_advance_a_watermark() {
        for listed in [
            json!({ "items": [], "total": 1 }),
            json!({ "items": [{}], "total": 1 }),
            json!({ "items": [{ "data": { "timestamp": "2026-09-08T00:00:00Z" } }], "total": 1 }),
            json!({ "items": [] }),
        ] {
            assert!(shared_lessons_from_list(listed, None).is_err());
        }
        assert!(
            shared_lessons_from_list(json!({ "items": [], "total": 0 }), None)
                .expect("empty complete snapshot")
                .is_empty()
        );
    }
}
