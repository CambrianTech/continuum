//! `memory/share` — the engram-handoff primitive: one agent hands a learned lesson
//! directly into ANOTHER agent's memory.
//!
//! The WRITE mirror of [`memory/remember`](super::remember), but where `remember` writes
//! into the authoring agent's OWN corpus (self-learned), `share` writes into a *recipient*
//! agent's corpus with **shared-by provenance** — so the recipient (and recall) can see the
//! lesson came from another agent, not learned first-hand. This is the seed of agent-to-agent
//! knowledge transfer ("telepathy"): a lesson BigMama learned once can land in M5's memory,
//! or a persona's, without either re-deriving it. The complement to the K3 capability axis —
//! agents that don't just get smarter, they share what they learn.
//!
//! Same durable-first / hydrate-on-miss discipline as `remember`: the shared lesson persists
//! to the recipient's durable store BEFORE the cache, so it survives a core restart — the
//! exact amnesia the whole memory bridge exists to end, now extended across agents.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::AppendResult;
use crate::log_info;
use crate::logging::TimingGuard;
use crate::memory::{CorpusMemory, MemoryRecord};
use crate::modules::memory::MemoryState;
use crate::sdk_codegen::CommandError;

fn default_importance() -> f64 {
    // A deliberately-shared lesson defaults slightly above neutral, like `remember` — an
    // agent bothering to hand it over is signal it matters.
    0.6
}

/// Params for `memory/share`. Flat + CLI-friendly — the substrate assembles the shared record.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryShareParams.ts"
)]
pub struct MemoryShareParams {
    /// The RECIPIENT agent's persona id (airc peer id) — the corpus that receives the lesson.
    pub to_persona_id: String,
    /// The SHARING agent's persona id (airc peer id) — recorded as shared-by provenance.
    pub from_persona_id: String,
    /// The lesson to share. Free text; serde escapes it into the record.
    pub content: String,
    /// Project / room scope — becomes the recall `room_id`, a tag, and part of context.
    pub scope: String,
    /// The sharing agent's session/conversation the lesson came from. Traceability.
    #[serde(default)]
    #[ts(optional)]
    pub session: Option<String>,
    /// Importance (0..1). Defaults slightly above neutral.
    #[serde(default = "default_importance")]
    pub importance: f64,
}

/// Pure: assemble a SHARED [`MemoryRecord`] destined for the recipient's corpus, carrying
/// shared-by provenance. Separated from the command so the provenance shaping is
/// unit-testable without the manager. `id` + `timestamp` are passed in (the command
/// generates them) to keep this deterministic.
pub(super) fn build_shared_record(
    to_persona_id: &str,
    from_persona_id: &str,
    content: String,
    scope: &str,
    session: Option<String>,
    importance: f64,
    id: String,
    timestamp: String,
) -> MemoryRecord {
    let context = serde_json::json!({
        // The corpus owner — the recipient (agent = its own peer, like `remember`).
        "agent_peer_id": to_persona_id,
        // Provenance: who handed it over, and the original author (same agent for a
        // first-hop share; a re-share would carry the original through).
        "shared_by": from_persona_id,
        "original_author": from_persona_id,
        "session": session,
        "scope": scope,
    });
    MemoryRecord {
        id,
        // Lands in the RECIPIENT's corpus.
        persona_id: to_persona_id.to_string(),
        // `shared`, not `agent` — a recipient's recall / consolidation can weight a
        // received lesson differently from one it learned itself.
        memory_type: "shared".to_string(),
        content,
        context,
        timestamp,
        importance,
        access_count: 0,
        // Scope tag + an explicit shared-from tag so a recall can filter to shared lessons.
        tags: vec![scope.to_string(), format!("shared-from:{from_persona_id}")],
        related_to: Vec::new(),
        // Provenance: received from `from`, distinct from `remember`'s `agent:<peer>`.
        source: Some(format!("shared:{from_persona_id}")),
        last_accessed_at: None,
        layer: None,
        relevance_score: None,
    }
}

crate::action_command! {
    /// Share an agent-authored lesson INTO another agent's memory (agent-to-agent knowledge
    /// transfer). Builds a shared-origin memory record server-side (uuid + timestamp filled,
    /// shared-by provenance set) and appends it to the RECIPIENT's corpus. The cross-agent
    /// mirror of `memory/remember`.
    pub struct MemoryShare { state: Arc<MemoryState> }
    name: "memory/share",
    access: AiSafe,
    params: MemoryShareParams,
    output: AppendResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_share");
        // We append into the RECIPIENT's corpus — hydrate THAT from durable truth first so we
        // never append onto a cold cache that then overwrites the recipient's persisted history.
        if let Some(loaded) = super::hydrate_corpus_if_missing(&this.state, &p.to_persona_id).await? {
            log_info!(
                "module", "memory_share",
                "Hydrated recipient corpus for {} from durable store ({loaded} memories)", p.to_persona_id
            );
        }

        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let record = build_shared_record(
            &p.to_persona_id,
            &p.from_persona_id,
            p.content,
            &p.scope,
            p.session,
            p.importance,
            id,
            timestamp,
        );
        // embedding None: computed by the pipeline like every other append.
        let memory = CorpusMemory { record, embedding: None };

        // Durable FIRST, cache second — a shared lesson that only landed in the in-process
        // corpus would die on the next core restart. Fail loud if the durable write fails.
        super::persist_memory(&this.state, &p.to_persona_id, &memory).await?;
        this.state
            .memory_manager
            .append_memory(&p.to_persona_id, memory)
            .map_err(|e| CommandError::Internal(format!("memory/share append failed: {e}")))?;

        log_info!(
            "module", "memory_share",
            "Shared lesson from {} to {} (scope={})", p.from_persona_id, p.to_persona_id, p.scope
        );
        Ok(AppendResult { appended: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: THE SHARED-PROVENANCE SHAPE. A shared lesson lands in the
    // RECIPIENT's corpus (persona_id = to), records who shared it (source = "shared:<from>",
    // context.shared_by = from), and is typed `shared` (not `agent`) — so a recipient can
    // tell a handed-over lesson from a self-learned one. Regression here = shared lessons
    // masquerading as the recipient's own, or landing in the wrong corpus.
    #[test]
    fn shared_record_lands_in_recipient_corpus_with_shared_by_provenance() {
        let r = build_shared_record(
            "recipient-peer",
            "sharer-peer",
            "always web-check the latest dep versions".to_string(),
            "continuum",
            Some("sess-1".to_string()),
            0.7,
            "id-1".to_string(),
            "2026-07-26T00:00:00Z".to_string(),
        );
        // Lands in the recipient's corpus.
        assert_eq!(r.persona_id, "recipient-peer");
        // Provenance: shared, from the sharer.
        assert_eq!(r.memory_type, "shared");
        assert_eq!(r.source.as_deref(), Some("shared:sharer-peer"));
        assert_eq!(r.context["shared_by"], "sharer-peer");
        assert_eq!(r.context["original_author"], "sharer-peer");
        // The corpus-owner provenance is the recipient (mirrors remember's agent_peer_id).
        assert_eq!(r.context["agent_peer_id"], "recipient-peer");
        // Filterable as a shared lesson.
        assert!(r.tags.contains(&"shared-from:sharer-peer".to_string()));
        assert!(r.tags.contains(&"continuum".to_string()));
    }
}
