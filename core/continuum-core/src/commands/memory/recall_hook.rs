//! `memory/recall-hook` — project multi-layer recall into a Claude Code **SessionStart
//! hook envelope**, built by serde (valid + escaped by construction), so the agent-memory
//! bridge plugin's `session-recall.sh` is a 3-line passthrough with ZERO shell JSON.
//!
//! The robustness point (BigMama, 2026-07-25, after the bridge dogfooded live): a hook
//! script hand-building `{"hookSpecificOutput":{...}}` in shell needs `jq`/`python` to
//! escape newlines + quotes in the recalled text — fragile, and it violates the no-python
//! / no-hand-built-wire-format rules. The ROBUST answer is the substrate emitting the
//! envelope through serde, where escaping is correct by construction. `continuum` prints a
//! command's raw result JSON (`continuum.rs`: `to_string_pretty(&result)`), so
//! `continuum memory/recall-hook` writes exactly this envelope to stdout and the hook pipes it
//! straight through.
//!
//! A thin PROJECTION over [`super::multi_layer_recall`] — it reuses
//! `MemoryState::multi_layer_recall` (no command-in-command dispatch) and only owns the
//! rendering + the CC envelope shape.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::log_info;
use crate::logging::TimingGuard;
use crate::memory::MultiLayerRecallRequest;
use crate::modules::memory::MemoryState;
use crate::sdk_codegen::CommandError;

fn default_max_results() -> usize {
    8
}

fn default_header() -> String {
    "## Relevant memories".to_string()
}

fn default_max_chars_per_memory() -> usize {
    400
}

/// Cap one recalled memory's rendered text to `max_chars` (0 = no cap), truncating on a
/// char boundary and appending `…`. The per-bullet complement to the caller's total budget:
/// a low `max_results` still floods if each memory is a verbose multi-KB body, so bound the
/// length too — critical for the `compact` source, where re-injection must not refill the
/// context compaction just freed.
fn cap_memory_text(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if max_chars == 0 || text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{}…", truncated.trim_end())
}

/// Params for `memory/recall-hook`. Mirror of `memory/multi-layer-recall`'s recall inputs
/// (so the projection is 1:1) plus the markdown `header` for the injected block.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/MemoryRecallHookParams.ts"
)]
pub struct MemoryRecallHookParams {
    /// Which persona's corpus to recall from (for an agent: its airc peer id).
    pub persona_id: crate::identity::PersonaRef,
    /// The semantic query. Absent ⇒ the semantic layer degrades to non-semantic recall.
    #[serde(default)]
    #[ts(optional)]
    pub query_text: Option<String>,
    /// Room / scope for the recall.
    pub room_id: String,
    /// Max memories to inject. Small by default — the SessionStart (esp. `compact`)
    /// re-injection must not refill the context that compaction just freed.
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    /// Markdown header for the injected block.
    #[serde(default = "default_header")]
    pub header: String,
    /// Cap on each recalled memory's rendered length (chars; 0 = uncapped). Bounds a
    /// verbose memory so a low `max_results` still can't flood — the per-bullet complement
    /// to the caller's total-char budget. Default 400.
    #[serde(default = "default_max_chars_per_memory")]
    pub max_chars_per_memory: usize,
}

/// The Claude Code SessionStart hook output envelope. Field names are the CC contract
/// (camelCase, via serde rename) — serde serializes valid, escaped JSON by construction,
/// which is the whole point (no shell `jq`/`python`). Shape grounded in current CC v2.1+
/// docs (BigMama's `claude-code-guide` research, 2026-07-25).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/SessionStartHookOutput.ts"
)]
pub struct SessionStartHookOutput {
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: HookSpecificOutput,
}

/// The `hookSpecificOutput` body Claude Code silently injects into the session context.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/memory/HookSpecificOutput.ts"
)]
pub struct HookSpecificOutput {
    /// Always `"SessionStart"` for this projection.
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,
    /// The markdown block injected into context — header + one bullet per recalled memory.
    /// Empty when nothing is recalled (a valid envelope that injects nothing, never errors).
    #[serde(rename = "additionalContext")]
    pub additional_context: String,
}

crate::action_command! {
    /// Recall a persona's most relevant memories and project them into a Claude Code
    /// SessionStart hook envelope (`hookSpecificOutput.additionalContext`), serde-built so
    /// the plugin hook needs ZERO shell JSON. A thin projection over
    /// `memory/multi-layer-recall`; empty recall yields an empty-but-valid envelope.
    pub struct MemoryRecallHook { state: Arc<MemoryState> }
    name: "memory/recall-hook",
    access: AiSafe,
    params: MemoryRecallHookParams,
    output: SessionStartHookOutput,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "memory_recall_hook");
        // Recall reads THROUGH to durable truth after a cold start — same hydrate the
        // primary recall API does, so a freshly-booted agent isn't told it has no past.
        if let Some(loaded) = super::hydrate_corpus_if_missing(&this.state, &p.persona_id).await? {
            log_info!(
                "module", "memory_recall_hook",
                "Hydrated corpus for {} from durable store ({loaded} memories)", p.persona_id
            );
        }
        let req = MultiLayerRecallRequest {
            query_text: p.query_text,
            room_id: p.room_id,
            max_results: p.max_results,
            layers: None,
        };
        let resp = this
            .state
            .memory_manager
            .multi_layer_recall(&p.persona_id, &req)
            .await
            .map_err(|e| CommandError::Internal(format!("memory/recall-hook recall failed: {e}")))?;

        // Render recalled memories as markdown bullets. serde escapes the whole string when
        // it serializes `additional_context`, so newlines/quotes in the lessons are safe by
        // construction — this is the fragility that shell JSON-building could not get right.
        let additional_context = if resp.memories.is_empty() {
            String::new()
        } else {
            let mut ctx = p.header.clone();
            for m in &resp.memories {
                ctx.push_str("\n- ");
                ctx.push_str(&cap_memory_text(&m.content, p.max_chars_per_memory));
            }
            ctx
        };
        log_info!(
            "module", "memory_recall_hook",
            "recall-hook for {}: {} memories -> {} context chars",
            p.persona_id, resp.memories.len(), additional_context.len()
        );

        Ok(SessionStartHookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "SessionStart".to_string(),
                additional_context,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the CC envelope serializes to the EXACT shape the SessionStart
    // hook consumes — `hookSpecificOutput.{hookEventName,additionalContext}` (camelCase),
    // and multi-line lesson text is escaped by serde (a literal newline becomes \n in the
    // JSON), which is the whole robustness argument vs shell JSON-building.
    #[test]
    fn envelope_serializes_to_the_cc_hook_shape_with_escaping() {
        let out = SessionStartHookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "SessionStart".to_string(),
                additional_context: "## Relevant memories\n- use continuum, not ctm\n- a \"quoted\" bit"
                    .to_string(),
            },
        };
        let json = serde_json::to_string(&out).unwrap();
        assert!(json.contains("\"hookSpecificOutput\""), "camelCase envelope key: {json}");
        assert!(json.contains("\"hookEventName\":\"SessionStart\""), "event name: {json}");
        assert!(json.contains("\"additionalContext\""), "context key: {json}");
        // The literal newline + quote are escaped by serde, not left raw — the fragility a
        // shell here-string / jq hand-build would have to get exactly right.
        assert!(json.contains("\\n- use continuum"), "newline escaped by serde: {json}");
        assert!(json.contains("\\\"quoted\\\""), "inner quotes escaped by serde: {json}");
        // Round-trips back to the same struct.
        let back: SessionStartHookOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(back.hook_specific_output.hook_event_name, "SessionStart");
    }

    // what this catches: a verbose memory is bounded to the per-bullet cap (on a char
    // boundary, ellipsized) so low max_results × long content can't flood a post-compact
    // re-injection; a short memory and cap=0 pass through untouched. Multi-byte safe.
    #[test]
    fn cap_memory_text_bounds_verbose_and_passes_short() {
        let long = "x".repeat(1000);
        let capped = cap_memory_text(&long, 400);
        assert_eq!(capped.chars().count(), 401, "400 chars + the … ellipsis");
        assert!(capped.ends_with('…'));
        // short memory: untouched (just trimmed), no ellipsis
        assert_eq!(cap_memory_text("  brief lesson  ", 400), "brief lesson");
        // cap = 0 ⇒ uncapped
        assert_eq!(cap_memory_text(&long, 0), long);
        // multi-byte content truncates on a char boundary without panicking
        let emoji = "🧠".repeat(10);
        let c = cap_memory_text(&emoji, 4);
        assert!(c.starts_with("🧠🧠🧠🧠") && c.ends_with('…'));
    }
}
