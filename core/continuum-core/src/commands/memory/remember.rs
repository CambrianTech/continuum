//! `memory/remember` — the WRITE mirror of [`super::recall_hook`]: build an agent-authored
//! memory record from flat CLI params and append it, so the agent-memory bridge plugin's
//! `remember.sh` is a 1-liner with ZERO shell JSON.
//!
//! The symmetry (BigMama, 2026-07-25): `recall_hook` made the READ side serde-robust; this
//! makes the WRITE side match. `memory/append-memory` takes a fully-formed `CorpusMemory`,
//! so a shell caller had to hand-build + escape the whole record JSON (`\`, `"`, newlines)
//! — the same fragility `recall_hook` killed. Here the substrate takes the raw fields and
//! BUILDS the record via serde: `id` (uuid) and `timestamp` are filled server-side, the
//! agent provenance (`source`, `memory_type`, `context`, `tags`) is set consistently, and
//! serde escapes everything by construction. `remember.sh` becomes:
//! `continuum memory/remember --persona-id <peer> --content "$1" --scope <proj>`.
//!
//! Provenance mirrors the corpus half of the bridge (per [[agent-memory-bridge-agents-use-the-engram-substrate]]):
//! `memory_type = "agent"`, `source = "agent:<peer>"`, `context = {agent_peer_id, session,
//! scope}`, `tags = [scope]` — so `multi-layer-recall` (and `recall-hook`) surface it and
//! its authorship travels with the row if it ever leaves this store.

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
    // A deliberate /remember is slightly-above-neutral importance — the persona chose to
    // keep it. Recall still ranks primarily by semantic relevance; this only breaks ties.
    0.6
}

/// Params for `memory/remember`. Flat + CLI-friendly — the substrate assembles the record.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryRememberParams.ts"
)]
pub struct MemoryRememberParams {
    /// The authoring agent's persona id — its airc peer id. Also the corpus key and the
    /// `agent_peer_id` provenance (agent = its own peer).
    pub persona_id: String,
    /// The lesson to remember. Free text; serde escapes it into the record.
    pub content: String,
    /// Project / room scope — becomes the recall `room_id`, a tag, and part of context.
    pub scope: String,
    /// The agent session/conversation that produced the lesson. Traceability; `None` for a
    /// migrated `.md` engram.
    #[serde(default)]
    #[ts(optional)]
    pub session: Option<String>,
    /// Importance (0..1). A deliberate remember defaults slightly above neutral.
    #[serde(default = "default_importance")]
    pub importance: f64,
}

/// Pure: assemble an agent-authored [`MemoryRecord`] from flat inputs. Separated from the
/// command so the provenance shaping is unit-testable without the manager. `id` + `timestamp`
/// are passed in (the command generates them) to keep this deterministic.
pub(super) fn build_agent_record(
    persona_id: &str,
    content: String,
    scope: &str,
    session: Option<String>,
    importance: f64,
    id: String,
    timestamp: String,
) -> MemoryRecord {
    let context = serde_json::json!({
        "agent_peer_id": persona_id,
        "session": session,
        "scope": scope,
    });
    MemoryRecord {
        id,
        persona_id: persona_id.to_string(),
        memory_type: "agent".to_string(),
        content,
        context,
        timestamp,
        importance,
        access_count: 0,
        tags: vec![scope.to_string()],
        related_to: Vec::new(),
        source: Some(format!("agent:{persona_id}")),
        last_accessed_at: None,
        layer: None,
        relevance_score: None,
    }
}

crate::action_command! {
    /// Remember an agent-authored lesson: build the agent-origin memory record server-side
    /// (uuid + timestamp filled, provenance set) and append it. The WRITE mirror of
    /// `memory/recall-hook` — `remember.sh` needs ZERO shell JSON.
    pub struct MemoryRemember { state: Arc<MemoryState> }
    name: "memory/remember",
    access: AiSafe,
    params: MemoryRememberParams,
    output: AppendResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_remember");
        // Append writes into the cached corpus; hydrate it from durable truth first so we
        // never append onto a cold cache that then overwrites the persisted history.
        if let Some(loaded) = super::hydrate_corpus_if_missing(&this.state, &p.persona_id).await? {
            log_info!(
                "module", "memory_remember",
                "Hydrated corpus for {} from durable store ({loaded} memories)", p.persona_id
            );
        }

        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let record = build_agent_record(
            &p.persona_id,
            p.content,
            &p.scope,
            p.session,
            p.importance,
            id,
            timestamp,
        );
        // embedding None: computed by the pipeline like every other append (the shell path
        // that dogfooded semantic recall passed no embedding either).
        let memory = CorpusMemory { record, embedding: None };

        // Durable FIRST, cache second — exactly as memory/append-memory does. A lesson that
        // only landed in the in-process corpus would die on the next core restart, which is
        // the precise amnesia the whole bridge exists to end. Fail loud if the durable write
        // fails; the cache is left untouched so truth never trails it.
        super::persist_memory(&this.state, &p.persona_id, &memory).await?;
        this.state
            .memory_manager
            .append_memory(&p.persona_id, memory)
            .map_err(|e| CommandError::Internal(format!("memory/remember append failed: {e}")))?;

        log_info!(
            "module", "memory_remember",
            "Remembered agent lesson for {} (scope={})", p.persona_id, p.scope
        );
        Ok(AppendResult { appended: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an agent lesson gets the agent provenance the bridge relies on —
    // memory_type "agent", source "agent:<peer>", tags [scope], and context carrying
    // agent_peer_id/session/scope — so multi-layer-recall / recall-hook surface it and its
    // authorship is self-describing. If the shaping drifts, recall by origin/scope breaks.
    #[test]
    fn agent_record_carries_the_bridge_provenance() {
        let r = build_agent_record(
            "peer-abc",
            "use continuum for same-machine, not ctm".to_string(),
            "continuum",
            Some("sess-1".to_string()),
            0.6,
            "id-1".to_string(),
            "2026-07-25T00:00:00Z".to_string(),
        );
        assert_eq!(r.memory_type, "agent");
        assert_eq!(r.source.as_deref(), Some("agent:peer-abc"));
        assert_eq!(r.tags, vec!["continuum".to_string()]);
        assert_eq!(r.persona_id, "peer-abc");
        assert_eq!(r.context["agent_peer_id"], "peer-abc");
        assert_eq!(r.context["scope"], "continuum");
        assert_eq!(r.context["session"], "sess-1");
        // A migrated lesson (no session) still builds a valid record with null session.
        let migrated = build_agent_record(
            "peer-abc", "old lesson".to_string(), "continuum", None, 0.6,
            "id-2".to_string(), "2026-07-25T00:00:00Z".to_string(),
        );
        assert!(migrated.context["session"].is_null(), "no session ⇒ null, still valid");
    }
}
