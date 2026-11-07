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
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx, ProjectsOutcome, ToolVerdict, VerbRef};

// It never sizes what she sees (ONE-BUDGET: the view is the only sizer). It returns the
// selection she asked for, whole: no selection reopens the whole output (`ToolEffect::Reopens`),
// a grep returns every match with the context she asks for, and her view's one allocation shows
// it whole, as its summary or as its pointer. The read's memory bound is the saved output's own
// measured length, which was held in memory once when it was saved. Before 2026-11-06 it cut its
// own slice to a fraction of the window (`render_slice_chars`), a second budget the turn's
// allocation never saw, with grep defaults of 2 context lines and 50 matches.

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
    /// The saved output's id.
    // `outputId` on the wire, NOT `handle`: every dispatch crosses
    // `CommandRequest::from_value`, whose own `handle: Option<HandleRef>` claims that
    // name and refuses a string for it (`expected struct HandleRef`) — so the call the
    // spill-recovery hint told every citizen to make could not be made (card ea28d2f6).
    #[serde(rename = "outputId")]
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
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
    //
    // Aliases were not enough: it was not NATIVE, so a native-call model could not emit it at
    // all (measured 2026-11-06 on the M5: no act called it, while every folded act, saved result
    // and thought handle named it). It is the verb every handle names, and a handle can appear in
    // any surface she is offered, a room's narrowed one included, so it declares `Always`: the
    // only tier the tool surface keeps whatever a recipe declares (`ToolSurface::declared`).
    // Every handle names it through `TOOL_OUTPUT`, which only compiles while that holds.
    const NATIVE: bool = true;
    const DISCLOSURE: continuum_base::tool_disclosure::Disclosure =
        continuum_base::tool_disclosure::Disclosure::Always;
    const EFFECT: continuum_base::tool_effect::ToolEffect =
        continuum_base::tool_effect::ToolEffect::Reopens;
    const ALIASES: &'static [&'static str] =
        &["read_output", "grep_output", "tail_output", "search_output"];
    // In every surface (`Always`), so every word here rides every act: the how lives on the
    // params, each a few words.
    const DESCRIPTION: &'static str =
        "Reopen a saved result by outputId or act: whole, or grep it, or read lines or bytes.";
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
                CommandError::Invalid(format!(
                    "{TOOL_OUTPUT} needs an authenticated caller to scope the output to"
                ))
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
        tokio::task::spawn_blocking(move || read_saved(&path, handle, params, range, spill::Allowance::Whole))
            .await
            .map_err(|e| CommandError::Invalid(format!("saved output reader failed: {e}")))?
            .map_err(CommandError::Invalid)
    }
}
crate::register_stateless_command!(ToolOutput);
continuum_kernel::register_outcome!(ToolOutput);

/// The verb every handle she is shown names: it renders as her tool list offers it (today
/// `read_output`, its first alias), and it only builds while `tool/output` is native and in every
/// surface. Text that reopens something renders a [`Reopen`], which carries it.
pub const TOOL_OUTPUT: VerbRef<ToolOutput> = VerbRef::always();

/// THE reopen affordance: one verb, one call form, its real parameter names (the command's own
/// params, serialized), rendered once. Every handle she is shown (a folded act, a repeat, a saved
/// result, a shed thought, a collapsed receipt, a too-large result) names its way back with this,
/// so there is one form to learn and it is always a call she can make as written:
/// `read_output({"act":3})`, `read_output({"outputId":"deadbeef"})`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reopen<'a> {
    /// One of her act numbers.
    Act(u64),
    /// A saved output, by the id its preview named.
    Output(&'a str),
}

impl Reopen<'_> {
    fn params(self) -> ToolOutputParams {
        match self {
            Reopen::Act(act) => ToolOutputParams { act: Some(act), ..Default::default() },
            Reopen::Output(handle) => ToolOutputParams { handle: Some(handle.to_owned()), ..Default::default() },
        }
    }
}

impl std::fmt::Display for Reopen<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A params struct of strings and numbers always serializes; an error is a formatter error.
        let args = serde_json::to_string(&self.params()).map_err(|_| std::fmt::Error)?; // boundary: her tool-call wire, the arguments she reads and sends back as written
        write!(f, "{TOOL_OUTPUT}({args})")
    }
}

/// A read that brought nothing back is a failure she sees as one, like `grep` exiting 1: a
/// pattern with no match, a range past the end, an empty page. A refusal (no such output, a bad
/// selector) is already an `Err`.
impl ProjectsOutcome for ToolOutput {
    fn outcome(output: &ToolOutputResult) -> ToolVerdict {
        ToolVerdict::of_success(!output.content.is_empty())
    }
}

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
    allowance: spill::Allowance,
) -> Result<ToolOutputResult, String> {
    if let Some(offset) = params.byte_offset {
        let extent = spill::read_extent(path, offset, params.byte_length, allowance)?;
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
        params.context_lines.unwrap_or_default(), // unwrap_or_default: no context asked for = the matching lines alone
        range,
        params.max_matches,
        allowance,
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

    // what this catches: the recovery verb every handle names off her native surface or out of a
    // room's narrowed one (2026-11-06, the M5: not native, so no act could call it), and a read
    // that brought nothing back rendering as a success.
    #[test]
    fn the_verb_every_handle_names_is_native_in_every_surface_and_an_empty_read_fails() {
        use crate::sdk_codegen::CommandSpec;
        assert!(<ToolOutput as CommandSpec>::NATIVE);
        assert!(TOOL_OUTPUT.disclosure().in_every_surface());
        assert_eq!(TOOL_OUTPUT.canonical(), <ToolOutput as CommandSpec>::NAME);
        assert!(
            crate::sdk_codegen::outcome_projector(TOOL_OUTPUT.canonical()).is_some(),
            "a native tool projects its outcome (ONE-ACT 6b)"
        );
        let read = |content: &str| ToolOutputResult {
            handle: "h".into(),
            total_lines: Some(3),
            total_bytes: 30,
            total_matches: Some(0),
            content: content.into(),
            truncated: false,
            encoding: "utf8".into(),
            byte_start: None,
            byte_end: None,
            next_byte_offset: None,
        };
        assert_eq!(ToolOutput::outcome(&read("     2 error here\n")), ToolVerdict::Succeeded);
        assert_eq!(ToolOutput::outcome(&read("")), ToolVerdict::Failed);
    }

    // what this catches: the read sizing what she sees (a second budget beside her view's one
    // allocation: it cut every reopen to a fraction of the window, and every grep to 50 matches).
    // A reopen with no selection is the whole output, and a grep returns every match; her view
    // decides whether she sees it whole, as its summary or as its pointer.
    #[test]
    fn a_reopen_returns_what_she_asked_for_whole_and_never_sizes_it() {
        let Ok(dir) = tempfile::tempdir() else { panic!("a temp dir") };
        let path = dir.path().join("whole");
        let body: String = (1..=500).map(|n| format!("line {n} error\n")).collect();
        assert!(std::fs::write(&path, &body).is_ok());
        let read = |params| read_saved(&path, "h".into(), params, None, spill::Allowance::Whole);
        let Ok(whole) = read(ToolOutputParams::default()) else { panic!("the whole reads back") };
        assert!(whole.content.contains("line 1 error") && whole.content.contains("line 500 error"), "{}", whole.content);
        assert!(!whole.truncated);
        let Ok(grep) = read(ToolOutputParams { pattern: Some("error".into()), ..Default::default() }) else {
            panic!("the grep reads back")
        };
        assert_eq!((grep.total_matches, grep.content.lines().count()), (Some(500), 500));
    }

    // what this catches: a reopen line she cannot make as written. It names the verb her tool
    // list offers (the wire name, never the slash name, which matches nothing she holds), and
    // its arguments are the command's own params, so they parse back to the same target.
    #[test]
    fn a_reopen_is_one_call_she_can_make_as_written() {
        use crate::sdk_codegen::wire_name;
        let offered = wire_name(
            <ToolOutput as ActionCommand>::NAME,
            crate::sdk_codegen::primary_alias(<ToolOutput as ActionCommand>::ALIASES),
            crate::sdk_codegen::offer_style_for(None),
        );
        assert_eq!(offered, "read_output", "her list offers the trained reflex, its first alias");
        assert_eq!(Reopen::Act(3).to_string(), r#"read_output({"act":3})"#);
        assert_eq!(Reopen::Output("deadbeef").to_string(), r#"read_output({"outputId":"deadbeef"})"#);
        for reopen in [Reopen::Act(3), Reopen::Output("deadbeef")] {
            let shown = reopen.to_string();
            let Some(args) = shown.strip_prefix(&format!("{offered}(")).and_then(|s| s.strip_suffix(')')) else {
                panic!("{shown} is not a call of {offered}")
            };
            let Ok(parsed) = serde_json::from_str::<ToolOutputParams>(args) else {
                panic!("{args} does not parse as the command's params")
            };
            assert_eq!((parsed.act, parsed.handle.as_deref()), match reopen {
                Reopen::Act(n) => (Some(n), None),
                Reopen::Output(h) => (None, Some(h)),
            });
        }
    }

    // what this catches: a handle shown in a surface that does not carry its verb. Built from the
    // REAL native registry: her core hands, a room that narrows her hands to one subtree, and a
    // room that declares a single verb each offer `tool/output`, because a folded act or a saved
    // result can appear in any of them.
    #[test]
    fn every_surface_she_is_offered_holds_the_verb_her_handles_name() {
        use crate::cognition::tool_surface::ToolSurface;
        let surface = ToolSurface::build(crate::cognition::persona_tools::native_offers(), Vec::new(), |s| s);
        for declared in [vec![], vec!["code/git/**".to_string()], vec!["code/read".to_string()]] {
            let selected = surface.select(&declared, &[]);
            let names = surface.names_of(&selected);
            assert!(
                names.iter().any(|name| name == TOOL_OUTPUT.canonical()),
                "{declared:?} drops the verb her handles name: {names:?}"
            );
        }
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
        let read =
            |handle: String| read_saved(&path, handle, ToolOutputParams::default(), None, spill::Allowance::Whole);
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

    // what this catches: the real command projection preserves exact byte continuation,
    // split UTF-8 and whole-file regex counts without loading the retained file in memory.
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
                spill::Allowance::Chars(12),
            )
            .unwrap();
            assert!(result.content.len() <= 12);
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
            spill::Allowance::Chars(256),
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
        std::fs::write(&path, "x".repeat(257)).unwrap();
        let error = read_saved(
            &path,
            "id".into(),
            ToolOutputParams {
                pattern: Some("never".into()),
                ..Default::default()
            },
            None,
            spill::Allowance::Chars(256),
        )
        .unwrap_err();
        assert!(error.contains("byteOffset 0"));
        assert!(error.contains("not evaluated on a partial line"));
    }
}
