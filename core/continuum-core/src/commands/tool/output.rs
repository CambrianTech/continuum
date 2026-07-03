//! `tool/output` — page and grep a flood-sized tool result that was spilled to
//! disk. The other half of the flood-protection loop: when a tool (a build, a
//! giant file read) overflows the context budget, the executor saves the WHOLE
//! result and hands back a preview naming a `handle`; this command lets the
//! persona recover the rest — most importantly, GREP it to find the error in the
//! crap, the way Claude Code greps its own large outputs.
//!
//! Scoped per-persona by construction: it resolves the spill under the caller's
//! own id ([`Ctx::caller`]'s `peer_id`, which equals the `persona_id` the
//! executor spilled under), so a persona can only ever read back its own output.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::tool_executor::spill;
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// Default context lines around each grep match (like `grep -C2`).
const DEFAULT_CONTEXT_LINES: usize = 2;
/// Default cap on grep match windows — enough to see the failures, bounded so a
/// pattern that matches everything can't re-flood.
const DEFAULT_MAX_MATCHES: usize = 50;
/// Char budget for the rendered slice. Below the executor's fold cap (16k) so a
/// normal investigation isn't itself re-spilled.
const RENDER_BUDGET_CHARS: usize = 12_000;

/// Inputs to `tool/output`. `handle` is required (from the preview); everything
/// else selects WHAT to pull back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/tool/ToolOutputParams.ts")]
pub struct ToolOutputParams {
    /// The output id from the elision marker (e.g. `"deadbeefcafe0001"`). This is
    /// the spill the preview told you was saved.
    pub handle: String,
    /// A regex to grep for — the failure-hunting path. e.g. `"error|panic|failed"`
    /// to jump straight to what broke. Omit to read a range (or the tail).
    #[ts(optional)]
    pub pattern: Option<String>,
    /// Lines of context to keep around each grep match (default 2, like `grep -C`).
    #[ts(optional)]
    #[ts(type = "number")]
    pub context_lines: Option<usize>,
    /// Read an explicit 1-based line range instead of grepping: the first line.
    #[ts(optional)]
    #[ts(type = "number")]
    pub start_line: Option<usize>,
    /// Read an explicit 1-based line range instead of grepping: the last line.
    #[ts(optional)]
    #[ts(type = "number")]
    pub end_line: Option<usize>,
    /// Cap on the number of grep match windows returned (default 50).
    #[ts(optional)]
    #[ts(type = "number")]
    pub max_matches: Option<usize>,
}

/// Result of paging a spilled output: the scale of the full thing plus the
/// bounded, line-numbered slice you asked for.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/tool/ToolOutputResult.ts")]
pub struct ToolOutputResult {
    /// Echo of the handle read.
    pub handle: String,
    /// Total lines in the full spilled output.
    #[ts(type = "number")]
    pub total_lines: usize,
    /// Total bytes in the full spilled output.
    #[ts(type = "number")]
    pub total_bytes: usize,
    /// When grepping: how many lines matched `pattern` across the WHOLE file
    /// (pre-cap), so you know if there were more hits than shown. `0` otherwise.
    #[ts(type = "number")]
    pub total_matches: usize,
    /// The rendered, line-numbered slice (grep windows / explicit range / tail).
    pub content: String,
    /// `true` if the rendered slice itself hit the budget and was cut — narrow
    /// the pattern or shrink the range to see the rest.
    pub truncated: bool,
}

/// `tool/output` — recover a spilled tool result by handle. Stateless; AiSafe by
/// default. Scopes to the caller's own persona id.
#[derive(Default)]
pub struct ToolOutput;

#[async_trait]
impl ActionCommand for ToolOutput {
    const NAME: &'static str = "tool/output";
    const DESCRIPTION: &'static str =
        "Page or grep a large tool result that was saved to disk because it was \
         too big to show in full. Pass the `handle` from the elision marker. To \
         FIND AN ERROR in a noisy build/test log, grep with `pattern` (a regex, \
         e.g. \"error|panic|failed\") — you get the matching lines with a few \
         lines of context around each. Or read an exact slice with `startLine`/\
         `endLine`. With neither, you get the tail (where build verdicts live).";
    type Params = ToolOutputParams;
    type Output = ToolOutputResult;

    async fn run(
        &self,
        ctx: &Ctx,
        params: ToolOutputParams,
    ) -> Result<ToolOutputResult, CommandError> {
        // WHO is asking — a spilled output belongs to the persona that produced
        // it, and the directory layout enforces that. No caller = no scope.
        let persona_id = ctx
            .caller
            .as_ref()
            .map(|c| c.peer_id.as_uuid())
            .ok_or_else(|| {
                CommandError::Invalid(
                    "tool/output needs an authenticated caller to scope the output to".into(),
                )
            })?;

        let path = spill::resolve(persona_id, &params.handle)
            .map_err(|e| CommandError::Invalid(e.to_string()))?;
        let content = std::fs::read_to_string(&path).map_err(|_| {
            CommandError::Invalid(format!(
                "no saved output with handle `{}` — it may have aged out, or the \
                 handle is mistyped. Use the handle from the most recent elision marker.",
                params.handle
            ))
        })?;

        let range = match (params.start_line, params.end_line) {
            (Some(s), Some(e)) => Some((s, e)),
            // A half-given range is ambiguous — name it rather than guess.
            (Some(_), None) | (None, Some(_)) => {
                return Err(CommandError::Invalid(
                    "give BOTH startLine and endLine for a range read, or neither".into(),
                ))
            }
            (None, None) => None,
        };

        let inv = spill::investigate(
            &content,
            params.pattern.as_deref(),
            params.context_lines.unwrap_or(DEFAULT_CONTEXT_LINES),
            range,
            params.max_matches.unwrap_or(DEFAULT_MAX_MATCHES),
            RENDER_BUDGET_CHARS,
        )
        .map_err(CommandError::Invalid)?;

        Ok(ToolOutputResult {
            handle: params.handle,
            total_lines: inv.total_lines,
            total_bytes: inv.total_bytes,
            total_matches: inv.total_matches,
            content: inv.rendered,
            truncated: inv.result_truncated,
        })
    }
}
crate::register_stateless_command!(ToolOutput);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: name mirrors path so the persona reaches it by the name
    // she'd guess from the elision marker.
    #[test]
    fn name_is_tool_output() {
        assert_eq!(ToolOutput::NAME, "tool/output");
    }

    // what this catches: with no authenticated caller there is no persona to
    // scope the output to — it must refuse, not read some default directory.
    #[tokio::test]
    async fn refuses_without_a_caller() {
        let err = ToolOutput
            .run(
                &Ctx::default(),
                ToolOutputParams {
                    handle: "deadbeefcafe0001".into(),
                    ..Default::default()
                },
            )
            .await
            .expect_err("must refuse");
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
