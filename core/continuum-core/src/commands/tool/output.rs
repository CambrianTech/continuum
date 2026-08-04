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
// Char budget for the rendered slice — a fraction of the caller's LIVE served window
// (`ContextBudget::render_slice_chars`), kept below the result-fold bound so a normal
// investigation isn't itself re-spilled. Never a constant: the old `RENDER_BUDGET_CHARS =
// 12_000` clipped a 1M-context mind to the same slice as a 16k one.
// [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
use crate::cognition::context_budget::ContextBudget;

/// Prebuilt failure-hunting filters, so a persona navigates a flood WITHOUT having to
/// know regex — the PX "hit the ground running" affordance for the overwhelming case
/// (a build/test/launch log torrent). Each maps to a battle-tested pattern covering the
/// common toolchains (rustc/cargo, node, python, clang, generic). Discoverable: the enum
/// values show up in `commands/help`, so the persona sees the menu of filters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../../protocol/typescript/tool/OutputFilter.ts")]
pub enum OutputFilter {
    /// Everything that looks like a hard failure — the default "what broke?" filter.
    Errors,
    /// Compiler/linter warnings.
    Warnings,
    /// Test failures + assertion/panic sites.
    Failures,
    /// Build/test verdict + progress lines (the "how did it end?" skim).
    Summary,
}

impl OutputFilter {
    /// The regex this preset greps for — spans common toolchains so one word works
    /// whether she just ran cargo, npm, pytest, or clang.
    fn pattern(self) -> &'static str {
        match self {
            OutputFilter::Errors => {
                r"(?i)\berror\b|error\[|panic|fatal|exception|traceback|\bfailed\b|undefined reference|cannot find"
            }
            OutputFilter::Warnings => r"(?i)\bwarning\b|\bwarn\b|deprecated",
            OutputFilter::Failures => {
                r"(?i)test result: FAILED|FAILED|assertion.*failed|panicked at|AssertionError|✗|✖|\bFAIL\b"
            }
            OutputFilter::Summary => {
                r"(?i)test result:|error\[|warning:|Compiling |Finished |Building |^error:|passed|failed|Exit code|BUILD (SUCCEEDED|FAILED)"
            }
        }
    }
}

/// Inputs to `tool/output`. `handle` is required (from the preview); everything
/// else selects WHAT to pull back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/tool/ToolOutputParams.ts")]
pub struct ToolOutputParams {
    /// The output id from the elision marker (e.g. `"deadbeefcafe0001"`). This is
    /// the spill the preview told you was saved.
    pub handle: String,
    /// A PREBUILT filter — the easy path, no regex needed: `errors` (what broke?),
    /// `warnings`, `failures` (which tests failed?), or `summary` (how did it end?).
    /// Start here on a flooded build/test log. Overridden by an explicit `pattern`.
    #[ts(optional)]
    pub filter: Option<OutputFilter>,
    /// A regex to grep for — the power-user path when a preset isn't specific enough.
    /// e.g. `"error\\[E0308\\]"` for one exact error. Omit to use `filter`, a range,
    /// or the tail.
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
        "Page or grep a large tool result that was saved to disk because it was too big \
         to show in full. Pass the `handle` from the elision marker. EASIEST: set \
         `filter` to a prebuilt preset — `errors` (what broke?), `warnings`, `failures` \
         (which tests failed?), or `summary` (how did it end?) — no regex needed. For a \
         specific hunt use `pattern` (a regex, e.g. \"error\\[E0308\\]\"). Or read an \
         exact slice with `startLine`/`endLine`. With none of these, you get the tail \
         (where build verdicts live).";
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

        // An explicit `pattern` wins; otherwise a `filter` preset supplies one — so the
        // common "just show me the errors" needs zero regex. Neither → range/tail.
        let effective_pattern = params
            .pattern
            .as_deref()
            .or_else(|| params.filter.map(|f| f.pattern()));

        let inv = spill::investigate(
            &content,
            effective_pattern,
            params.context_lines.unwrap_or(DEFAULT_CONTEXT_LINES),
            range,
            params.max_matches.unwrap_or(DEFAULT_MAX_MATCHES),
            ContextBudget::live().render_slice_chars(),
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

    // what this catches: every prebuilt filter must be a VALID regex (a typo would
    // make the preset path fail loudly at use) AND actually match what it claims —
    // so `filter: "errors"` really does surface a rustc error line. Guards the PX
    // promise that a persona can navigate a flood with one word, no regex knowledge.
    #[test]
    fn every_filter_preset_is_valid_and_matches() {
        use regex::Regex;
        for f in [
            OutputFilter::Errors,
            OutputFilter::Warnings,
            OutputFilter::Failures,
            OutputFilter::Summary,
        ] {
            Regex::new(f.pattern()).unwrap_or_else(|e| panic!("{f:?} bad regex: {e}"));
        }
        let log = "   Compiling foo\nerror[E0308]: mismatched types\nwarning: unused var\n\
                   test result: FAILED. 1 passed; 2 failed";
        assert!(Regex::new(OutputFilter::Errors.pattern()).unwrap().is_match(log));
        assert!(Regex::new(OutputFilter::Warnings.pattern()).unwrap().is_match(log));
        assert!(Regex::new(OutputFilter::Failures.pattern()).unwrap().is_match(log));
        assert!(Regex::new(OutputFilter::Summary.pattern()).unwrap().is_match(log));
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
