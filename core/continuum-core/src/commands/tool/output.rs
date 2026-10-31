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

// No size is decided here (HER-TOOLS: the view is the only sizer). This command returns the
// selection she asked for; her view shows it at the level her window allows, with the handle
// that pages the rest (`fit_newest_result`). The old window fraction, 50-match cap, 2-line
// context default and 80-line tail each chose for her.

/// Prebuilt failure-hunting filters, so a persona navigates a flood WITHOUT having to
/// know regex — the PX "hit the ground running" affordance for the overwhelming case
/// (a build/test/launch log torrent). Each maps to a battle-tested pattern covering the
/// common toolchains (rustc/cargo, node, python, clang, generic). Discoverable: the enum
/// values show up in `commands/help`, so the persona sees the menu of filters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool/OutputFilter.ts"
)]
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

/// Inputs to `tool/output`. Exactly one of `outputId` (from the preview) or `act` (one of
/// her act numbers); everything else selects WHAT to pull back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool/ToolOutputParams.ts"
)]
pub struct ToolOutputParams {
    /// The output id from the elision marker (e.g. `"deadbeefcafe0001"`). This is
    /// the spill the preview told you was saved.
    // `outputId` on the wire, NOT `handle`: every dispatch crosses
    // `CommandRequest::from_value`, whose own `handle: Option<HandleRef>` claims that
    // name and refuses a string for it (`expected struct HandleRef`) — so the call the
    // spill-recovery hint told every citizen to make could not be made (card ea28d2f6).
    #[serde(rename = "outputId")]
    #[ts(optional)]
    pub handle: Option<String>,
    /// One of your act numbers (the `#n` your actions name, folded ones included), in
    /// place of `outputId`: reopens that act's saved output. Give exactly one of the two.
    #[ts(optional)]
    #[ts(type = "number")]
    pub act: Option<u64>,
    /// A PREBUILT filter — the easy path, no regex needed: `errors` (what broke?),
    /// `warnings`, `failures` (which tests failed?), or `summary` (how did it end?).
    /// Start here on a flooded build/test log. Overridden by an explicit `pattern`.
    #[ts(optional)]
    pub filter: Option<OutputFilter>,
    /// A regex to grep for — the power-user path when a preset isn't specific enough.
    /// e.g. `"error\\[E0308\\]"` for one exact error. Omit to use `filter`, a range,
    /// or the whole output.
    #[ts(optional)]
    pub pattern: Option<String>,
    /// Lines of context to keep around each grep match (like `grep -C`; none unless asked).
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
    /// Return only the first N grep matches (all of them unless asked); the total count and
    /// `nextByteOffset` still cover the whole output.
    #[ts(optional)]
    #[ts(type = "number")]
    pub max_matches: Option<usize>,
    /// Absolute byte position for exact, non-consuming retrieval. Mutually exclusive
    /// with line/regex queries. Follow nextByteOffset to recover the remaining bytes.
    #[ts(optional)]
    #[ts(type = "number")]
    pub byte_offset: Option<u64>,
    /// Requested byte count; omitted, the read runs to the end. No byte is discarded:
    /// nextByteOffset continues a page.
    #[ts(optional)]
    #[ts(type = "number")]
    pub byte_length: Option<usize>,
}

/// Result of paging a spilled output: the scale of the full thing plus the
/// bounded, line-numbered slice you asked for.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool/ToolOutputResult.ts"
)]
pub struct ToolOutputResult {
    /// Echo of the output id read.
    #[serde(rename = "outputId")]
    pub handle: String,
    /// Total lines in the full spilled output; null for an exact byte page.
    #[ts(type = "number | null")]
    pub total_lines: Option<usize>,
    /// Total bytes in the full spilled output.
    #[ts(type = "number")]
    pub total_bytes: usize,
    /// When grepping: how many lines matched `pattern` across the WHOLE file
    /// (pre-cap), so you know if there were more hits than shown. Zero for a line
    /// range/tail, null for a byte page (no regex scan was performed).
    #[ts(type = "number | null")]
    pub total_matches: Option<usize>,
    /// The rendered, line-numbered slice (grep windows / explicit range / tail).
    pub content: String,
    /// `true` if selected lines or matches remain beyond this page — narrow
    /// the pattern or shrink the range to see the rest.
    pub truncated: bool,
    /// utf8 for text; base64 for a byte extent that is not independently valid UTF-8.
    pub encoding: String,
    #[ts(type = "number | null")]
    pub byte_start: Option<u64>,
    #[ts(type = "number | null")]
    pub byte_end: Option<u64>,
    /// Next unread byte in exact mode, or first omitted source byte in line mode.
    #[ts(type = "number | null")]
    pub next_byte_offset: Option<u64>,
}

/// `tool/output` — recover a spilled tool result by handle. Stateless; AiSafe by
/// default. Scopes to the caller's own persona id.
#[derive(Default)]
pub struct ToolOutput;

#[async_trait]
impl ActionCommand for ToolOutput {
    const NAME: &'static str = "tool/output";
    // THE RECOVERY VERB MUST BE IN HER VOCABULARY. Measured 2026-08-28: the
    // elision marker tells her to "jump straight to what broke" with
    // `tool/output`, and this command declared NO aliases — so under
    // `OfferStyle::TrainedReflex` (which offers each command's PRIMARY alias,
    // `code/read` → `read_file`) it never landed on the menu in a name her
    // training reaches for, and across every log on this box it had been called
    // exactly ZERO times. The substrate was handing her a handle and naming a
    // verb she did not hold: the tail of every flood-sized error — which is
    // usually the actual failure — was unreadable to her.
    //
    // Inbound resolution accepts every style (`from_wire_name`), so adding
    // aliases can only widen what she can say, never narrow it.
    const ALIASES: &'static [&'static str] =
        &["read_output", "grep_output", "tail_output", "search_output"];
    const DESCRIPTION: &'static str =
        "Page or grep a large tool result that was saved to disk because it was too big \
         to show in full. Pass the `outputId` from the elision marker, OR `act` with one of \
         your act numbers (the #n your actions name, folded ones included). EASIEST: set \
         `filter` to a prebuilt preset — `errors` (what broke?), `warnings`, `failures` \
         (which tests failed?), or `summary` (how did it end?) — no regex needed. For a \
         specific hunt use `pattern` (a regex, e.g. \"error\\[E0308\\]\"). Or read an \
         exact slice with `startLine`/`endLine`. With none of these, you get the whole \
         output, shown at the level your window allows with how to page the rest. Exact byte \
         pages use byteOffset and optional byteLength (to the end when omitted); follow \
         nextByteOffset. Check encoding (utf8 or base64). Line and regex scans read every \
         line whole.";
    // Native: her view folds results and names this verb as the way back to each whole
    // (`perception.rs`, `actions.rs`, the open-views banner). A verb the view names but the
    // surface never offers made her re-read files instead (the M5, 2026-10-31: Aiko 19
    // reads in an hour, 0 reopens), so the reopen verb rides her hands.
    const NATIVE: bool = true;
    type Params = ToolOutputParams;
    type Output = ToolOutputResult;

    async fn run(
        &self,
        ctx: &Ctx,
        mut params: ToolOutputParams,
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

        let act_handle =
            |act| crate::cognition::persona_workspace::global().act_output_handle(persona_id, act);
        let handle = named_output(params.handle.take(), params.act, act_handle)?;
        let path = spill::resolve(persona_id, &handle)
            .map_err(|e| CommandError::Invalid(e.to_string()))?;
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

        if params.byte_length.is_some() && params.byte_offset.is_none() {
            return Err(CommandError::Invalid(
                "byteLength requires byteOffset".into(),
            ));
        }
        if params.byte_offset.is_some()
            && (range.is_some()
                || params.pattern.is_some()
                || params.filter.is_some()
                || params.context_lines.is_some()
                || params.max_matches.is_some())
        {
            return Err(CommandError::Invalid(
                "byteOffset cannot be combined with line or regex selectors".into(),
            ));
        }
        // Disk scanning belongs off the async executor. The same spill owner handles
        // both views; no second content registry or private whole-file retrieval path.
        tokio::task::spawn_blocking(move || read_saved(&path, handle, params, range))
            .await
            .map_err(|e| CommandError::Invalid(format!("saved output reader failed: {e}")))?
            .map_err(CommandError::Invalid)
    }
}
crate::register_stateless_command!(ToolOutput);

/// The saved output a call names: its `outputId`, or her act number resolved by `act_handle`
/// (her working memory's record). Exactly one; an act that does not resolve is its typed
/// refusal, never another output.
fn named_output(
    handle: Option<String>,
    act: Option<u64>,
    act_handle: impl FnOnce(u64) -> Result<String, crate::cognition::working_memory::ActOutputRefusal>,
) -> Result<String, CommandError> {
    match (handle, act) {
        (Some(handle), None) => Ok(handle),
        (None, Some(act)) => {
            act_handle(act).map_err(|refusal| CommandError::Invalid(refusal.to_string()))
        }
        _ => Err(CommandError::Invalid(
            "give exactly one of outputId (from the preview) or act (one of your act numbers)"
                .into(),
        )),
    }
}

fn read_saved(
    path: &std::path::Path,
    handle: String,
    params: ToolOutputParams,
    range: Option<(usize, usize)>,
) -> Result<ToolOutputResult, String> {
    if let Some(offset) = params.byte_offset {
        let extent = spill::read_extent(&path, offset, params.byte_length)?;
        return Ok(ToolOutputResult {
            handle,
            total_lines: None,
            total_bytes: usize::try_from(extent.total_bytes)
                .map_err(|_| "saved output length exceeds this platform")?,
            total_matches: None,
            content: extent.content,
            truncated: extent.next_byte_offset.is_some(),
            encoding: extent.encoding.into(),
            byte_start: Some(extent.start),
            byte_end: Some(extent.end),
            next_byte_offset: extent.next_byte_offset,
        });
    }
    let effective_pattern = params
        .pattern
        .as_deref()
        .or_else(|| params.filter.map(|f| f.pattern()));
    let inv = spill::investigate_file(
        &path,
        effective_pattern,
        params.context_lines.unwrap_or_default(), // unwrap_or_default: no context unless she asks
        range,
        params.max_matches,
    )?;
    Ok(ToolOutputResult {
        handle,
        total_lines: Some(inv.total_lines),
        total_bytes: inv.total_bytes,
        total_matches: Some(inv.total_matches),
        content: inv.rendered,
        truncated: inv.result_truncated,
        encoding: "utf8".into(),
        byte_start: None,
        byte_end: None,
        next_byte_offset: inv.next_byte_offset,
    })
}

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
        assert!(Regex::new(OutputFilter::Errors.pattern())
            .unwrap()
            .is_match(log));
        assert!(Regex::new(OutputFilter::Warnings.pattern())
            .unwrap()
            .is_match(log));
        assert!(Regex::new(OutputFilter::Failures.pattern())
            .unwrap()
            .is_match(log));
        assert!(Regex::new(OutputFilter::Summary.pattern())
            .unwrap()
            .is_match(log));
    }

    // what this catches: with no authenticated caller there is no persona to
    // scope the output to — it must refuse, not read some default directory.
    #[tokio::test]
    async fn refuses_without_a_caller() {
        let err = ToolOutput
            .run(
                &Ctx::default(),
                ToolOutputParams {
                    handle: Some("deadbeefcafe0001".into()),
                    ..Default::default()
                },
            )
            .await
            .expect_err("must refuse");
        assert!(matches!(err, CommandError::Invalid(_)));
    }

    // what this catches: a folded act she cannot reopen. Her folded acts name only "#a-#b", so
    // `act` must reach exactly the output its handle does (resolved through her working memory's
    // record), and an act with no record or no saved output is a typed refusal naming the act,
    // never some other output.
    #[test]
    fn an_act_number_reopens_what_its_handle_does_and_an_unknown_or_unsaved_act_refuses() {
        use crate::ai::types::ResultCustody;
        use crate::cognition::working_memory::WorkingMemory;
        let memory = WorkingMemory::new(8);
        memory.record_receipt_typed(
            &[],
            "saved result",
            None,
            ResultCustody::Whole {
                handle: "h1".into(),
            },
        );
        memory.record_receipt_typed(&[], "unsaved result", None, ResultCustody::Unsaved);
        let Ok(dir) = tempfile::tempdir() else {
            panic!("a temp dir for the saved output")
        };
        let path = dir.path().join("h1");
        assert!(std::fs::write(&path, "line one\nerror here\nline three\n").is_ok());
        let Ok(by_act) = named_output(None, Some(1), |act| memory.saved_handle(act)) else {
            panic!("act #1 was saved")
        };
        let Ok(by_handle) = named_output(Some("h1".into()), None, |act| memory.saved_handle(act))
        else {
            panic!("a handle names itself")
        };
        assert_eq!(by_act, by_handle);
        let read = |handle: String| read_saved(&path, handle, ToolOutputParams::default(), None);
        let (Ok(act_read), Ok(handle_read)) = (read(by_act), read(by_handle)) else {
            panic!("the saved output reads back")
        };
        assert_eq!(
            (act_read.content, act_read.handle),
            (handle_read.content, handle_read.handle)
        );
        for (act, says) in [
            (2, "act #2 has no saved output"),
            (99, "act #99 is not on your record"),
        ] {
            match named_output(None, Some(act), |act| memory.saved_handle(act)) {
                Err(CommandError::Invalid(refusal)) => {
                    assert!(refusal.starts_with(says), "{refusal}")
                }
                other => panic!("act #{act} must refuse, got {other:?}"),
            }
        }
        assert!(
            matches!(
                named_output(Some("h1".into()), Some(1), |act| memory.saved_handle(act)),
                Err(CommandError::Invalid(_))
            ),
            "both: refused"
        );
        assert!(
            matches!(
                named_output(None, None, |act| memory.saved_handle(act)),
                Err(CommandError::Invalid(_))
            ),
            "neither: refused"
        );
    }

    // what this catches: the real command projection losing exact byte continuation, split
    // UTF-8, whole-file regex counts, or reading to the end when she gives no byteLength.
    #[test]
    fn saved_output_pages_round_trip_and_queries_keep_counts() {
        use base64::Engine;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.log");
        let body = "é🙂\r\nerror one\nnoise\nerror two\n";
        std::fs::write(&path, body).unwrap();
        let mut actual = Vec::new();
        let mut offset = 0;
        loop {
            let result = read_saved(
                &path,
                "id".into(),
                ToolOutputParams {
                    byte_offset: Some(offset),
                    byte_length: Some(2),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
            assert_eq!(result.byte_start, Some(offset));
            assert_eq!(
                result.total_lines, None,
                "byte retrieval does not pretend to have scanned lines"
            );
            actual.extend(if result.encoding == "base64" {
                base64::engine::general_purpose::STANDARD
                    .decode(result.content)
                    .unwrap()
            } else {
                result.content.into_bytes()
            });
            match result.next_byte_offset {
                Some(next) => {
                    assert!(next > offset);
                    offset = next;
                }
                None => break,
            }
        }
        assert_eq!(actual, body.as_bytes());
        let result = read_saved(
            &path,
            "id".into(),
            ToolOutputParams {
                pattern: Some("error".into()),
                context_lines: Some(0),
                max_matches: Some(1),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(result.total_lines, Some(4));
        assert_eq!(result.total_matches, Some(2));
        assert!(result.content.contains("error one"));
        assert!(!result.content.contains("error two"));
        assert_eq!(
            result.next_byte_offset,
            Some(body.find("error two").unwrap() as u64)
        );
        assert!(result.truncated);
        let rest = read_saved(
            &path,
            "id".into(),
            ToolOutputParams { byte_offset: Some(3), ..Default::default() },
            None,
        )
        .unwrap();
        assert_eq!(rest.next_byte_offset, None, "no byteLength reads to the end");
        assert_eq!(rest.byte_end, Some(body.len() as u64));
    }
}
