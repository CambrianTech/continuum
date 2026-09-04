//! Substrate-pure sentinel completion dispatcher.
//!
//! When a sentinel reaches a terminal state (completed / failed /
//! cancelled), this module is what the [`super::SentinelModule`]
//! completion handler calls. It does three things, mirroring the TS
//! `SentinelEscalationService` that this module replaces:
//!
//!  1. **Persist** the [`SentinelExecutionResult`] into the `sentinels`
//!     entity's execution history.
//!  2. **Escalate** an [`InboxTaskRecord`] into the `tasks` collection
//!     so the owning persona's tick (`ChannelModule::tick`) picks it
//!     up and routes it through `PersonaInbox`. Same path for online
//!     and offline personas — channel.rs is the single rendezvous.
//!  3. **Store** the terminal event as a [`SentinelMemoryRecord`] in
//!     the persona's `memories` collection so it surfaces in
//!     subsequent RAG recalls (engram pipeline).
//!
//! ## Why one path, not two
//!
//! The TS version branched on `getPersonaInbox(...)` — online personas
//! got an in-memory `inbox.enqueue`, offline personas got a `DataCreate`.
//! Two code paths, same outcome. The substrate-native version unifies
//! on `data/create` because [`super::super::channel::ChannelModule::tick`]
//! polls the `tasks` collection on every tick and routes pending tasks
//! into the live `PersonaInbox` per persona. The channel tick IS the
//! online/offline rendezvous — escalation doesn't need to know.
//!
//! ## Why typed structs all the way down
//!
//! Per Joel's reminder during the Stage B kickoff ("use good structs,
//! well formulated"): the wire payloads for `data/create` / `data/update`
//! are constructed from typed records here, not `serde_json::json!`
//! blobs at the call site. Each downstream collection has its own
//! private struct ([`InboxTaskRecord`], [`SentinelMemoryRecord`],
//! [`SentinelExecutionResult`]) so the field set is enforced at the
//! type level and a future reader can grep `pub struct
//! SentinelMemoryRecord` to find every field that's written. The
//! serializer renames to camelCase to match what the `data/*` module
//! and the TS-side readers expect.
//!
//! ## Doctrinal alignment
//!
//! - `[[no-fallbacks-ever]]` — no TS round-trip, no fallthrough into
//!   the legacy `execute_ts_json("sentinel/escalate", ...)` bridge.
//!   This module IS the dispatcher.
//! - `[[rust-is-the-core-node-is-the-shell]]` — every collection
//!   referenced (`sentinels`, `tasks`, `memories`) is owned by the
//!   Rust `DataModule`. The dispatcher composes Rust commands.
//! - `[[rethink-dont-port-commands-to-rust]]` — the substrate-native
//!   shape consolidates the two TS code paths (online/offline) into
//!   one because the channel-tick rendezvous makes the branch
//!   unnecessary.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::warn;
use uuid::Uuid;

use crate::runtime::CommandExecutor;

use super::types::{
    default_escalation_rules, EscalationAction, EscalationCondition, EscalationRule,
    SentinelEscalation,
};

// ─── Public event surface ────────────────────────────────────────────

/// Terminal status the sentinel reached. Mirrors the TS union literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentinelTerminalStatus {
    Completed,
    Failed,
    Cancelled,
}

impl SentinelTerminalStatus {
    /// Status string written into the sentinels entity + carried in
    /// `metadata.sentinelStatus` on the inbox task. Same literal set
    /// as the TS `EscalationCondition` for the matching path.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// The condition that completed/failed paths look up in the
    /// caller-supplied escalation rules. Mirrors the TS
    /// `condition === 'completed' ? 'complete' : 'error'`.
    pub fn matched_condition(self) -> EscalationCondition {
        match self {
            Self::Completed => EscalationCondition::Complete,
            // Failed + cancelled both route through the error rule —
            // matches the TS behavior (else branch on the ternary).
            Self::Failed | Self::Cancelled => EscalationCondition::Error,
        }
    }

    /// The `taskType` slug on the inbox task. Mirrors the TS ternary
    /// in `escalateToPersonaInbox`.
    pub fn inbox_task_type(self) -> &'static str {
        match self {
            Self::Completed => "sentinel-complete",
            Self::Failed => "sentinel-failed",
            Self::Cancelled => "sentinel-escalation",
        }
    }

    /// Memory importance scalar in `[0.0, 1.0]`. Successful completion
    /// is the most surfaceable signal; failures get mid-tier importance;
    /// cancellations decay quickly.
    pub fn memory_importance(self) -> f64 {
        match self {
            Self::Completed => 0.7,
            Self::Failed => 0.5,
            Self::Cancelled => 0.3,
        }
    }
}

/// What the [`super::SentinelModule`] completion handler hands to
/// [`dispatch`]. Bundles the terminal status + the escalation metadata
/// captured at sentinel start. Cheaper than threading 6 individual
/// parameters; readable at the call site.
#[derive(Debug, Clone)]
pub struct SentinelEscalationEvent {
    /// Sentinel handle ID (the Rust-side correlation token).
    pub handle: String,
    /// What state the sentinel landed in.
    pub status: SentinelTerminalStatus,
    /// Wall-clock duration, milliseconds. `None` for sentinels whose
    /// start time we lost track of (mid-restart resumes).
    pub duration_ms: Option<u64>,
    /// Error string if `status` is `Failed`.
    pub error: Option<String>,
    /// Escalation metadata captured at sentinel start — owning
    /// persona, entity ID, name, optional caller-supplied rules.
    pub escalation: SentinelEscalation,
}

// ─── Wire structs for each downstream collection ─────────────────────

/// One row in the `sentinels` entity's `executions` field. Mirrors the
/// TS `SentinelExecutionResult`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SentinelExecutionResult {
    handle: String,
    success: bool,
    started_at: String, // RFC3339
    completed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// One row in the `tasks` collection. Shape matches what
/// [`super::super::channel::ChannelModule::data_to_task_queue_item`]
/// reads out, plus the sentinel-specific metadata bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InboxTaskRecord {
    /// Outer record id — distinct from `task_id` so the channel reader
    /// can hash records without losing semantic-task identity.
    id: Uuid,
    /// Discriminator that `PersonaInbox` uses to dispatch. Always
    /// `"task"` for sentinel escalations.
    #[serde(rename = "type")]
    record_type: &'static str,
    /// Semantic task identity. Multiple inbox records can share a
    /// `task_id` (consolidation); each gets a unique `id`.
    task_id: Uuid,
    /// Owning persona — the assignee + the original creator
    /// (the persona's subconscious sentinel created it).
    assignee_id: Uuid,
    created_by: Uuid,
    /// `"sentinel"` so [`super::super::channel::ChannelModule::tick`]
    /// can filter by domain.
    domain: &'static str,
    /// `"sentinel-complete"` / `"sentinel-failed"` /
    /// `"sentinel-escalation"`.
    task_type: &'static str,
    /// SentinelEntity ID — what the persona is being woken up about.
    context_id: Uuid,
    /// Human-readable summary surfaced in the inbox UI.
    description: String,
    /// Inbox priority scalar in `[0.0, 1.0]` — comes from
    /// [`EscalationPriority::inbox_priority`] on the matched rule.
    priority: f64,
    /// Always `"pending"` on creation.
    status: &'static str,
    /// Creation time (ms since UNIX epoch).
    timestamp: u64,
    /// Sentinel-specific fields the inbox UI surfaces. Kept as a
    /// nested object (matches TS shape) so the channel tick doesn't
    /// flatten unrelated fields into its filter.
    metadata: SentinelInboxTaskMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SentinelInboxTaskMetadata {
    sentinel_name: String,
    sentinel_entity_id: Option<String>,
    sentinel_handle: String,
    sentinel_status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// One row in the persona's `memories` collection. Mirrors the TS
/// `MemoryEntity` (partial — only the fields the TS service writes).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SentinelMemoryRecord {
    id: Uuid,
    persona_id: String,
    session_id: &'static str,
    /// MemoryType.SENTINEL — the TS enum value is `"sentinel"`.
    #[serde(rename = "type")]
    memory_type: &'static str,
    content: String,
    context: SentinelMemoryContext,
    /// RFC3339 timestamp matching the TS `ISOString` helper.
    timestamp: String,
    importance: f64,
    access_count: u32,
    related_to: Vec<String>,
    tags: Vec<String>,
    source: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SentinelMemoryContext {
    sentinel_name: String,
    sentinel_entity_id: Option<String>,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// ─── Public dispatcher ───────────────────────────────────────────────

/// Run the substrate-side completion pipeline for a finished sentinel.
///
/// Errors from individual stages are logged via `tracing::warn!` and
/// the next stage runs anyway (matches the TS service's per-stage
/// try/catch). The caller (SentinelModule completion path) doesn't
/// abort the sentinel's own cleanup on dispatch failure — the
/// sentinel finished; the escalation is a downstream notification.
pub async fn dispatch(executor: &Arc<CommandExecutor>, event: SentinelEscalationEvent) {
    let started_at = started_at_iso(event.duration_ms);
    let completed_at = now_iso();
    let success = matches!(event.status, SentinelTerminalStatus::Completed);

    // Stage 1 — persist into sentinels entity (if the caller gave us an
    // entityId; standalone sentinels without an entity don't write).
    if let Some(entity_id) = event.escalation.entity_id.as_deref() {
        let result = SentinelExecutionResult {
            handle: event.handle.clone(),
            success,
            started_at: started_at.clone(),
            completed_at: completed_at.clone(),
            duration_ms: event.duration_ms,
            error: event.error.clone(),
        };
        if let Err(e) = persist_execution_result(executor, entity_id, &result, event.status).await {
            warn!(
                handle = %event.handle,
                entity_id = %entity_id,
                error = %e,
                "sentinel/escalate: persist_execution_result failed; \
                 downstream stages still run",
            );
        }
    }

    // Stage 2 — escalate into the owning persona's inbox. Only if the
    // matching escalation rule says `Notify` (TS branched on
    // `rule.action !== 'pause'`; we make `Notify` explicit, leaving
    // `Pause` and `Abort` as no-ops — the abort path is handled by
    // the sentinel runner upstream of escalation).
    let rules = event
        .escalation
        .escalation_rules
        .clone()
        .unwrap_or_else(default_escalation_rules);
    let matched_rule = rules
        .iter()
        .copied()
        .find(|r| r.condition == event.status.matched_condition());

    if let Some(parent_persona_id) = event.escalation.parent_persona_id.as_deref() {
        if let Some(rule) = matched_rule {
            if rule.action == EscalationAction::Notify {
                if let Err(e) =
                    escalate_to_persona_inbox(executor, &event, parent_persona_id, rule).await
                {
                    warn!(
                        handle = %event.handle,
                        persona_id = %parent_persona_id,
                        error = %e,
                        "sentinel/escalate: escalate_to_persona_inbox failed; \
                         memory stage still runs",
                    );
                }
            }
        }

        // Stage 3 — store sentinel memory regardless of rule action.
        // Memory write captures the audit trail even when the rule
        // says don't-wake-the-persona (pause/abort).
        if let Err(e) = store_sentinel_memory(executor, &event, parent_persona_id).await {
            warn!(
                handle = %event.handle,
                persona_id = %parent_persona_id,
                error = %e,
                "sentinel/escalate: store_sentinel_memory failed",
            );
        }
    }
}

// ─── Stage implementations ───────────────────────────────────────────

async fn persist_execution_result(
    executor: &Arc<CommandExecutor>,
    entity_id: &str,
    result: &SentinelExecutionResult,
    status: SentinelTerminalStatus,
) -> Result<(), String> {
    // Read current entity to splice the new execution into the
    // existing history (capped at 50 entries — matches TS).
    let list = executor
        .execute_json(
            "data/list",
            json!({
                "collection": "sentinels",
                "filter": { "id": entity_id },
                "limit": 1,
            }),
        )
        .await
        .map_err(|e| format!("data/list failed: {e}"))?;

    let Some(entity) = list
        .get("items")
        .and_then(|i| i.as_array())
        .and_then(|arr| arr.first())
    else {
        // No entity by that id — nothing to update. Matches TS early-
        // return; not an error worth surfacing.
        return Ok(());
    };

    let mut executions: Vec<serde_json::Value> = Vec::with_capacity(50);
    executions.push(serde_json::to_value(result).map_err(|e| format!("serialize result: {e}"))?);
    if let Some(existing) = entity.get("executions").and_then(|v| v.as_array()) {
        for item in existing.iter().take(49) {
            executions.push(item.clone());
        }
    }

    let execution_count = entity
        .get("executionCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        + 1;

    let now = now_iso();
    let patch = json!({
        "collection": "sentinels",
        "id": entity_id,
        "data": {
            "executions": executions,
            "status": status.as_str(),
            "activeHandle": serde_json::Value::Null,
            "executionCount": execution_count,
            "lastSuccess": result.success,
            "lastRunAt": result.started_at,
            "updatedAt": now,
        },
    });

    executor
        .execute_json("data/update", patch)
        .await
        .map(|_| ())
        .map_err(|e| format!("data/update failed: {e}"))
}

async fn escalate_to_persona_inbox(
    executor: &Arc<CommandExecutor>,
    event: &SentinelEscalationEvent,
    parent_persona_id: &str,
    rule: EscalationRule,
) -> Result<(), String> {
    let persona_uuid = Uuid::parse_str(parent_persona_id)
        .map_err(|e| format!("invalid parent_persona_id ({parent_persona_id}): {e}"))?;
    let context_uuid = event
        .escalation
        .entity_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(persona_uuid);

    let description = build_inbox_description(event);

    let record = InboxTaskRecord {
        id: Uuid::new_v4(),
        record_type: "task",
        task_id: Uuid::new_v4(),
        assignee_id: persona_uuid,
        created_by: persona_uuid,
        domain: "sentinel",
        task_type: event.status.inbox_task_type(),
        context_id: context_uuid,
        description,
        priority: rule.priority.inbox_priority(),
        status: "pending",
        timestamp: now_ms(),
        metadata: SentinelInboxTaskMetadata {
            sentinel_name: event.escalation.sentinel_name.clone(),
            sentinel_entity_id: event.escalation.entity_id.clone(),
            sentinel_handle: event.handle.clone(),
            sentinel_status: event.status.as_str(),
            error: event.error.clone(),
        },
    };

    let payload = json!({
        "collection": "tasks",
        "data": serde_json::to_value(&record).map_err(|e| format!("serialize task: {e}"))?,
    });

    executor
        .execute_json("data/create", payload)
        .await
        .map(|_| ())
        .map_err(|e| format!("data/create tasks failed: {e}"))
}

async fn store_sentinel_memory(
    executor: &Arc<CommandExecutor>,
    event: &SentinelEscalationEvent,
    parent_persona_id: &str,
) -> Result<(), String> {
    let duration_str = match event.duration_ms {
        Some(ms) => format!("{:.1}s", ms as f64 / 1000.0),
        None => "unknown".to_string(),
    };

    let content = match event.status {
        SentinelTerminalStatus::Completed => format!(
            "Sentinel \"{name}\" completed successfully in {duration_str}",
            name = event.escalation.sentinel_name,
        ),
        SentinelTerminalStatus::Failed => format!(
            "Sentinel \"{name}\" failed after {duration_str}: {err}",
            name = event.escalation.sentinel_name,
            err = event.error.as_deref().unwrap_or("unknown error"),
        ),
        SentinelTerminalStatus::Cancelled => format!(
            "Sentinel \"{name}\" was cancelled after {duration_str}",
            name = event.escalation.sentinel_name,
        ),
    };

    let related_to = event
        .escalation
        .entity_id
        .clone()
        .map(|id| vec![id])
        .unwrap_or_default();

    let record = SentinelMemoryRecord {
        id: Uuid::new_v4(),
        persona_id: parent_persona_id.to_string(),
        session_id: "sentinel-lifecycle",
        memory_type: "sentinel",
        content,
        context: SentinelMemoryContext {
            sentinel_name: event.escalation.sentinel_name.clone(),
            sentinel_entity_id: event.escalation.entity_id.clone(),
            status: event.status.as_str(),
            duration_ms: event.duration_ms,
            error: event.error.clone(),
        },
        timestamp: now_iso(),
        importance: event.status.memory_importance(),
        access_count: 0,
        related_to,
        tags: vec![
            "sentinel".to_string(),
            event.escalation.sentinel_name.clone(),
            event.status.as_str().to_string(),
        ],
        source: "sentinel-escalation",
    };

    let payload = json!({
        "collection": "memories",
        "data": serde_json::to_value(&record).map_err(|e| format!("serialize memory: {e}"))?,
    });

    executor
        .execute_json("data/create", payload)
        .await
        .map(|_| ())
        .map_err(|e| format!("data/create memories failed: {e}"))
}

// ─── Local helpers ───────────────────────────────────────────────────

fn build_inbox_description(event: &SentinelEscalationEvent) -> String {
    let name = &event.escalation.sentinel_name;
    match event.status {
        SentinelTerminalStatus::Completed => {
            format!("Sentinel \"{name}\" completed successfully")
        }
        SentinelTerminalStatus::Failed => {
            let err = event.error.as_deref().unwrap_or("unknown error");
            format!("Sentinel \"{name}\" failed: {err}")
        }
        SentinelTerminalStatus::Cancelled => format!("Sentinel \"{name}\" was cancelled"),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn started_at_iso(duration_ms: Option<u64>) -> String {
    let now = chrono::Utc::now();
    match duration_ms {
        Some(ms) => (now - chrono::Duration::milliseconds(ms as i64)).to_rfc3339(),
        None => now.to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the inbox priority scalar must match the TS
    // PRIORITY_MAP literal so existing inbox UI thresholds keep working
    // across the migration boundary.
    #[test]
    fn inbox_priority_matches_ts_map() {
        use crate::modules::sentinel::types::EscalationPriority;
        assert!((EscalationPriority::Low.inbox_priority() - 0.3).abs() < f64::EPSILON);
        assert!((EscalationPriority::Normal.inbox_priority() - 0.5).abs() < f64::EPSILON);
        assert!((EscalationPriority::High.inbox_priority() - 0.7).abs() < f64::EPSILON);
        assert!((EscalationPriority::Urgent.inbox_priority() - 0.9).abs() < f64::EPSILON);
    }

    // what this catches: the failed + cancelled paths must both route
    // through the Error rule (matches TS ternary). A future refactor
    // that introduces a Cancelled-specific condition would flip this
    // and warrant explicit reconsideration.
    #[test]
    fn failed_and_cancelled_share_error_condition() {
        assert_eq!(
            SentinelTerminalStatus::Failed.matched_condition(),
            EscalationCondition::Error
        );
        assert_eq!(
            SentinelTerminalStatus::Cancelled.matched_condition(),
            EscalationCondition::Error
        );
        assert_eq!(
            SentinelTerminalStatus::Completed.matched_condition(),
            EscalationCondition::Complete
        );
    }

    // what this catches: the task_type slug surfaced to PersonaInbox
    // is what the UI / cognition keys filters on. A typo here is a
    // silent miss in every inbox view.
    #[test]
    fn task_type_slugs_match_ts_literals() {
        assert_eq!(
            SentinelTerminalStatus::Completed.inbox_task_type(),
            "sentinel-complete"
        );
        assert_eq!(
            SentinelTerminalStatus::Failed.inbox_task_type(),
            "sentinel-failed"
        );
        assert_eq!(
            SentinelTerminalStatus::Cancelled.inbox_task_type(),
            "sentinel-escalation"
        );
    }

    // what this catches: default escalation rules must include
    // Complete=Notify+Low so successful sentinels don't silently
    // skip the inbox. The TS service relies on this for its
    // "fall-through to default rules" path.
    #[test]
    fn default_rules_include_complete_notify() {
        use crate::modules::sentinel::types::EscalationPriority;
        let rules = default_escalation_rules();
        let complete = rules
            .iter()
            .find(|r| r.condition == EscalationCondition::Complete)
            .expect("default rules must cover the Complete condition");
        assert_eq!(complete.action, EscalationAction::Notify);
        assert_eq!(complete.priority, EscalationPriority::Low);
    }

    // what this catches: InboxTaskRecord -> JSON must roundtrip with
    // camelCase field names. ChannelModule::data_to_task_queue_item
    // reads camelCase; a serde rename slip-up here silently breaks
    // every sentinel escalation.
    #[test]
    fn inbox_task_record_serializes_camelcase() {
        let record = InboxTaskRecord {
            id: Uuid::nil(),
            record_type: "task",
            task_id: Uuid::nil(),
            assignee_id: Uuid::nil(),
            created_by: Uuid::nil(),
            domain: "sentinel",
            task_type: "sentinel-complete",
            context_id: Uuid::nil(),
            description: "ok".to_string(),
            priority: 0.5,
            status: "pending",
            timestamp: 0,
            metadata: SentinelInboxTaskMetadata {
                sentinel_name: "demo".to_string(),
                sentinel_entity_id: None,
                sentinel_handle: "h".to_string(),
                sentinel_status: "completed",
                error: None,
            },
        };
        let json = serde_json::to_value(&record).unwrap();
        assert!(json.get("taskId").is_some(), "taskId must be camelCase");
        assert!(
            json.get("assigneeId").is_some(),
            "assigneeId must be camelCase"
        );
        assert!(json.get("taskType").is_some(), "taskType must be camelCase");
        assert!(
            json.get("contextId").is_some(),
            "contextId must be camelCase"
        );
        assert!(
            json.get("metadata")
                .and_then(|m| m.get("sentinelName"))
                .is_some(),
            "metadata.sentinelName must be camelCase"
        );
    }
}
