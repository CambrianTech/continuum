//! Cognition introspection as a COMMAND surface — citizens debug each other.
//!
//! The workspace-trace and prompt-capture harnesses (the glass box on a persona's
//! RAG-in / decision-out and verbatim LLM tokens) are written as per-persona JSONL.
//! These commands expose that same analysis I (or any citizen) do by hand:
//!
//! - `cognition/trace`  — recent workspace ticks for a persona: every faculty bid
//!   (recall/grounding/deliberation) with salience + content, the assembled context
//!   the decider saw, and the decision. "What did their mind assemble, and choose?"
//! - `cognition/prompt` — recent verbatim LLM calls: the exact system prompt +
//!   message thread sent + raw response (text/reasoning/finish_reason/tool_calls).
//!   "What exact tokens were they fed, and what did they emit?"
//!
//! This is what makes the grid META: a persona can read ANOTHER persona's cognition
//! and reason about why it decided what it did — the same debugging loop humans run,
//! now available to citizens ([[cognition-half-the-work-is-harnesses]]).
//!
//! Access: `Privileged` → `Trusted`. Reading another mind's full trace is for
//! trusted local citizens (and the owner), never an arbitrary remote `Provisional`
//! peer. Read-only; a missing trace returns an empty result, not an error.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Default number of recent records returned when the caller doesn't set `limit`.
const DEFAULT_LIMIT: usize = 10;
/// Hard cap so a huge trace can't blow up the caller's context.
const MAX_LIMIT: usize = 100;

/// Read the last `limit` JSONL lines from a per-persona fixture file under
/// `~/.continuum/fixtures/<subdir>/<persona_id>.jsonl`. Missing file → empty.
fn tail_persona_jsonl(subdir: &str, persona_id: &str, limit: usize) -> Result<Vec<String>, CommandError> {
    // persona_id is a path component — validate it's a plain UUID-ish token so a
    // caller can't traverse out of the fixtures dir.
    if persona_id.is_empty()
        || !persona_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(CommandError::Invalid(format!(
            "persona_id '{persona_id}' is not a valid id token"
        )));
    }
    let home = std::env::var("HOME")
        .map_err(|_| CommandError::Internal("HOME unset; no fixtures root".into()))?;
    let path = std::path::Path::new(&home)
        .join(".continuum/fixtures")
        .join(subdir)
        .join(format!("{persona_id}.jsonl"));
    let body = match std::fs::read_to_string(&path) {
        Ok(b) => b,
        // No trace yet (persona hasn't run, or capture off) is not an error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(CommandError::Internal(format!("read {}: {e}", path.display()))),
    };
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    let n = limit.min(MAX_LIMIT);
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].iter().map(|s| s.to_string()).collect())
}

// ─────────────────────────── cognition/trace ─────────────────────

#[derive(Default)]
pub struct CognitionTrace;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionTraceParams {
    /// The persona (UUID) whose cognition to inspect — yours or a peer's.
    pub persona_id: crate::identity::PersonaRef,
    /// How many recent ticks to return (newest last). Default 10, max 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CognitionTraceResult {
    pub persona_id: crate::identity::PersonaRef,
    pub count: u32,
    /// Each entry is one tick's JSON record: world_state, bids (faculty +
    /// salience + content), context (what the decider saw), decision.
    pub records: Vec<String>,
}

#[async_trait]
impl ActionCommand for CognitionTrace {
    const NAME: &'static str = "cognition/trace";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Inspect a persona's recent cognition: per-tick faculty bids (recall/grounding/\
         deliberation) with salience + content, the assembled context the decider saw, and the \
         decision. Use to debug why a persona (yours or a peer's) decided what it did.";
    type Params = CognitionTraceParams;
    type Output = CognitionTraceResult;

    async fn run(&self, _ctx: &Ctx, p: CognitionTraceParams) -> Result<CognitionTraceResult, CommandError> {
        let limit = p.limit.map(|n| n as usize).unwrap_or(DEFAULT_LIMIT);
        let records = tail_persona_jsonl("workspace-traces", p.persona_id.as_str(), limit)?;
        Ok(CognitionTraceResult {
            persona_id: p.persona_id,
            count: records.len() as u32,
            records,
        })
    }
}

// ─────────────────────────── cognition/prompt ────────────────────

#[derive(Default)]
pub struct CognitionPrompt;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionPromptParams {
    /// The persona (UUID) whose verbatim LLM I/O to inspect.
    pub persona_id: crate::identity::PersonaRef,
    /// How many recent LLM calls to return (newest last). Default 10, max 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CognitionPromptResult {
    pub persona_id: crate::identity::PersonaRef,
    pub count: u32,
    /// Each entry is one LLM call's JSON record: the exact system prompt, the
    /// message thread sent, and the raw response (text/reasoning/finish_reason/
    /// tool_calls).
    pub records: Vec<String>,
}

#[async_trait]
impl ActionCommand for CognitionPrompt {
    const NAME: &'static str = "cognition/prompt";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Inspect the verbatim LLM I/O of a persona's recent turns: the exact system prompt, the \
         message thread it was sent, and its raw response (text, reasoning, tool_calls). Use to \
         debug exactly what tokens a persona was fed and what it emitted.";
    type Params = CognitionPromptParams;
    type Output = CognitionPromptResult;

    async fn run(&self, _ctx: &Ctx, p: CognitionPromptParams) -> Result<CognitionPromptResult, CommandError> {
        let limit = p.limit.map(|n| n as usize).unwrap_or(DEFAULT_LIMIT);
        let records = tail_persona_jsonl("prompt-captures", p.persona_id.as_str(), limit)?;
        Ok(CognitionPromptResult {
            persona_id: p.persona_id,
            count: records.len() as u32,
            records,
        })
    }
}

// ─────────────────────────── cognition/personas ──────────────────

#[derive(Default)]
pub struct CognitionPersonas;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionPersonasParams {}

#[derive(Debug, Clone, Serialize, TS)]
pub struct PersonaRosterEntry {
    /// The persona's UUID — pass this to `cognition/eval`, `cognition/trace`, etc.
    pub persona_id: crate::identity::PersonaRef,
    /// The persona's display name (`None` for a pure-cognition mind with no hands).
    #[ts(optional)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CognitionPersonasResult {
    pub count: u32,
    /// Every persona with a live `WorkspaceCycle` in THIS process — the set you
    /// can actually drive (eval/trace/prompt). Empty before any persona spawns.
    pub personas: Vec<PersonaRosterEntry>,
}

#[async_trait]
impl ActionCommand for CognitionPersonas {
    const NAME: &'static str = "cognition/personas";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "List every persona with a live cognition (a resident WorkspaceCycle) in this process: \
         their UUID + name. These are the personas you can drive with cognition/eval, \
         cognition/trace, and cognition/prompt. Use to discover who is spawned before scoring \
         or inspecting them.";
    type Params = CognitionPersonasParams;
    type Output = CognitionPersonasResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: CognitionPersonasParams,
    ) -> Result<CognitionPersonasResult, CommandError> {
        let mut personas: Vec<PersonaRosterEntry> = crate::cognition::persona_workspace::global()
            .roster()
            .into_iter()
            .map(|(id, name)| PersonaRosterEntry {
                persona_id: id.to_string().into(),
                name,
            })
            .collect();
        // Stable, name-first ordering so the roster reads the same across calls
        // (HashMap iteration order is otherwise arbitrary).
        personas.sort_by(|a, b| a.name.cmp(&b.name).then(a.persona_id.cmp(&b.persona_id)));
        Ok(CognitionPersonasResult {
            count: personas.len() as u32,
            personas,
        })
    }
}

// Stateless → self-register onto the ONE registry (descriptor + runtime object),
// no host module. Available to any Trusted citizen as a cognition-debugging tool.
crate::register_stateless_command!(CognitionTrace);
crate::register_stateless_command!(CognitionPrompt);
crate::register_stateless_command!(CognitionPersonas);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: path-traversal guard — a persona_id with slashes/.. is
    // refused, so the command can't read outside the fixtures dir.
    #[test]
    fn rejects_non_id_persona_tokens() {
        assert!(tail_persona_jsonl("workspace-traces", "../../etc/passwd", 5).is_err());
        assert!(tail_persona_jsonl("workspace-traces", "a/b", 5).is_err());
    }

    // what this catches: a missing trace is an empty result, not an error — "no
    // trace yet" is a normal state (persona hasn't run / capture off).
    #[test]
    fn missing_trace_is_empty_not_error() {
        let r = tail_persona_jsonl("workspace-traces", "00000000-0000-0000-0000-000000000000", 5);
        assert!(matches!(r, Ok(v) if v.is_empty()));
    }
}
