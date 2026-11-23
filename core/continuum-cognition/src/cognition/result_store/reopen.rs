//! How a persona reopens a saved result: the one verb that does it, the arguments its call carries,
//! and the call she is shown, rendered once. The store's vocabulary, so every layer that names a way
//! back (working memory, act/observe, her view) names it here, below them all; the `tool/output`
//! command that runs the call takes its name from [`REOPEN_VERB`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::output_id::OutputId;

/// The reopen verb's canonical name: the `tool/output` command's own (`ToolOutput::NAME` is this).
pub const REOPEN_VERB: &str = "tool/output";

/// The reopen verb as her tool list offers it (`tool_output`).
pub fn offered() -> String {
    crate::sdk_codegen::wire_name(REOPEN_VERB)
}

// Prebuilt failure-hunting filters, so a persona navigates a flood WITHOUT having to
// know regex — the PX "hit the ground running" affordance for the overwhelming case
// (a build/test/launch log torrent). Each maps to a battle-tested pattern covering the
// common toolchains (rustc/cargo, node, python, clang, generic). The doc below rides her
// schema every act, so it has none (the field that holds it says what it is).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool/OutputFilter.ts"
)]
pub enum OutputFilter {
    // No variant docs: they would turn her schema's plain enum into a `oneOf` of described
    // constants on an `Always` verb. The names say what each greps (the pattern is below).
    Errors,
    Warnings,
    Failures,
    Summary,
}

impl OutputFilter {
    /// The regex this preset greps for — spans common toolchains so one word works
    /// whether she just ran cargo, npm, pytest, or clang.
    pub(crate) fn pattern(self) -> &'static str {
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

/// Inputs to `tool/output`. `outputId` (from the preview) or `act` (one of her act numbers),
/// or both when they name the same output, or neither for her newest saved output; everything
/// else selects WHAT to pull back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool/ToolOutputParams.ts"
)]
pub struct ToolOutputParams {
    /// The saved output's id.
    // `outputId` on the wire, NOT `handle`: every dispatch crosses
    // `CommandRequest::from_value`, whose own `handle: Option<HandleRef>` claims that
    // name and refuses a string for it (`expected struct HandleRef`) — so the call the
    // spill-recovery hint told every citizen to make could not be made (card ea28d2f6).
    #[serde(rename = "outputId")]
    #[ts(optional)]
    #[ts(type = "string")]
    #[schemars(with = "Option<String>")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<OutputId>,
    /// Or an act number (#n): its saved output.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub act: Option<u64>,
    /// A preset grep, no regex needed.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<OutputFilter>,
    /// A regex to grep for (overrides filter).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Context lines per match.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub context_lines: Option<usize>,
    /// First line of a range (1-based).
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub start_line: Option<usize>,
    /// Last line of the range.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub end_line: Option<usize>,
    /// Most matches shown.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub max_matches: Option<usize>,
    /// Exact bytes from here (alone); continue at nextByteOffset.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub byte_offset: Option<u64>,
    /// Bytes wanted from byteOffset.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number")]
    pub byte_length: Option<usize>,
}


/// THE reopen affordance: one verb, one call form, its real parameter names (the command's own
/// params, serialized), rendered once. Every handle she is shown (a folded act, a repeat, a saved
/// result, a shed thought, a collapsed receipt, a too-large result) names its way back with this,
/// so there is one form to learn and it is always a call she can make as written:
/// `tool_output({"act":3})`, `tool_output({"outputId":"deadbeef"})`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reopen<'a> {
    /// One of her act numbers.
    Act(u64),
    /// A saved output, by the id its preview named.
    Output(&'a str),
    /// A range of a saved output's lines (`super::LineRange`, its end the output's own when
    /// open): where a page of it shown short continues.
    Lines { output: &'a str, range: super::LineRange },
    /// A saved output's bytes from `offset`: where a page cut inside one line continues.
    Bytes { output: &'a str, offset: u64 },
}

/// A reopen's arguments as her call carries them: [`ToolOutputParams`]' own wire names, borrowed,
/// so rendering a handle never parses it (every handle she is shown was minted by the spill) and
/// never fails. `a_reopen_is_one_call_she_can_make_as_written` parses each back as the command's
/// params, so the two cannot drift.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReopenArgs<'a> {
    #[serde(rename = "outputId", skip_serializing_if = "Option::is_none")]
    output: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    act: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    byte_offset: Option<u64>,
}

impl<'a> Reopen<'a> {
    fn args(self) -> ReopenArgs<'a> {
        let none = ReopenArgs { output: None, act: None, start_line: None, end_line: None, byte_offset: None };
        match self {
            Reopen::Act(act) => ReopenArgs { act: Some(act), ..none },
            Reopen::Output(output) => ReopenArgs { output: Some(output), ..none },
            Reopen::Lines { output, range } => ReopenArgs { output: Some(output), start_line: Some(range.start), end_line: range.end, ..none },
            Reopen::Bytes { output, offset } => ReopenArgs { output: Some(output), byte_offset: Some(offset), ..none },
        }
    }
}

impl std::fmt::Display for Reopen<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A struct of strings and numbers always serializes; an error is a formatter error.
        let args = serde_json::to_string(&self.args()).map_err(|_| std::fmt::Error)?; // boundary: her tool-call wire, the arguments she reads and sends back as written
        write!(f, "{}({args})", offered())
    }
}
