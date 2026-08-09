//! Perception predicates: pure structural-fact DETECTORS over the working-memory
//! trail and a tool batch. Extracted verbatim from `act_observe` (pure code-motion).
//! Each answers one yes/no about what the mind has already done this concern —
//! touched paths, per-concern scope, already-satisfied, orientation class, redundant
//! re-survey. No steering, no side effects: proprioception, not policy.

use crate::ai::types::ToolCall;
use crate::cognition::working_memory::WM_SETTLEMENT_PREFIX;

use super::recency::summarize_args_for_recency;

/// Collect the file paths a tool batch NAMES: any string under a `file_path` or
/// `path` key in a call's input, appended to `touched` in first-touch order,
/// deduped. Mechanical extraction from HER OWN calls — never inferred, never a
/// steer; this is the investigation-trail STATE an N-chances retry hands back.
pub(super) fn collect_touched_paths(touched: &mut Vec<String>, calls: &[ToolCall]) {
    for call in calls {
        for key in ["file_path", "path"] {
            if let Some(p) = call.input.get(key).and_then(|v| v.as_str()) {
                if !touched.iter().any(|t| t == p) {
                    touched.push(p.to_string());
                }
            }
        }
    }
}

/// The tail of the working-memory buffer SINCE the persona last settled (produced an
/// utterance). Everything before the most recent [`WM_SETTLEMENT_PREFIX`] boundary
/// belongs to a concern she already ANSWERED — an identical tool call over there is
/// not a spin, so re-issuing it for the current concern is legitimate. This is the
/// scope that separates "how many commands?" → answer → "list them again" (both emit
/// `commands/list({})`, legitimately) from re-issuing the SAME call within one
/// unanswered settling (the spin). Mirrors the `responded_through` boundary the
/// `ActThenSpeak` test faculty tracks — content-driven, not a turn counter.
pub(super) fn entries_since_last_settlement(recent: &[String]) -> &[String] {
    match recent
        .iter()
        .rposition(|e| e.starts_with(crate::cognition::working_memory::WM_SETTLEMENT_PREFIX))
    {
        Some(i) => &recent[i + 1..],
        None => recent,
    }
}

/// True when EVERY call in this batch has ALREADY been carried out SINCE THE LAST
/// SETTLEMENT — the identical `(name, args)` already appears as a satisfied `I ran …`
/// trace in the not-yet-answered tail of the working-memory recency channel. Keyed on
/// the SAME `name(args)` rendering [`apply_act`] records below (`I ran {name}({args})`),
/// so detection and recording can never drift; scoped to the current concern by
/// [`entries_since_last_settlement`] so an identical call the mind already answered in
/// a PRIOR concern (still lingering in the volatile buffer) does not false-trigger.
///
/// This is content-driven proprioception, NOT an iteration counter: it fires only on a
/// byte-identical re-request whose result the mind already holds THIS concern —
/// precisely the signal the `[action #n]` stamp shift was MEANT to convey but
/// empirically does not. A greedy instruct model re-emits the identical `Act` despite
/// the shifted window (proven live 2026-07-02 via `cognition/prompt`, nil-room eval:
/// working memory carried `commands/list` + its full result and she re-issued
/// `commands/list` on every act, never converting the result to an answer). A MIXED
/// batch — any genuinely new call — is NOT a repeat: the new call yields new
/// perception and must run.
pub(super) fn all_calls_already_satisfied(recent: &[String], calls: &[ToolCall], fold: Option<usize>) -> bool {
    if calls.is_empty() {
        return false;
    }
    let scope = entries_since_last_settlement(recent);
    calls.iter().all(|call| {
        // Bounded (#165): a whole-file `content` arg must not be echoed back verbatim ahead
        // of the RESULT — see `summarize_args_for_recency`.
        let args = summarize_args_for_recency(&call.input, fold);
        // Keyed on the `name(args)` core of the receipt render below — kept in sync
        // with it (the render dropped the first-person "I ran " opener, #158, so a
        // base model can't copy a receipt as a message opener; the fingerprint is
        // the stable `name(args)` substring both channels still carry).
        let signature = format!("{}({})", call.name, args);
        scope.iter().any(|trace| trace.contains(&signature))
    })
}

/// ORIENTATION commands — they SURVEY what the mind already carries and never read a
/// specific file or change the workspace. Two axes:
///   - tool-surface: `commands/help`/`commands/list` (the tool menu is already inline);
///   - workspace: `code/tree` (the `[workspace-map]` is already grounded in the prompt).
/// Matched on the canonical (slash) name after the wire-dialect map has run
/// (`help`→`commands/help`, `list_commands`→`commands/list`, `file_tree`→`code/tree`),
/// normalizing the underscore form defensively. NOT `code/list`: a one-dir listing to
/// get exact filenames before an edit is a legitimate narrowing step, not a survey.
pub(super) fn is_orientation_call(name: &str) -> bool {
    matches!(
        name.replace('_', "/").as_str(),
        "commands/help" | "commands/list" | "code/tree"
    )
}

/// True when this batch is ALL orientation AND the current concern already holds a
/// discovery receipt — a redundant re-list that returns byte-identical perception.
/// Sibling of [`all_calls_already_satisfied`], keyed on the SAME `name(` receipt
/// render and the SAME per-concern scope ([`entries_since_last_settlement`]), so
/// detection and recording can never drift.
///
/// Why a dedicated demotion and not the exact-repeat guard: `commands/help(code/write)`
/// and `commands/help(code/run)` are DIFFERENT args, so the exact-repeat guard lets
/// them all through — yet each returns a surface the mind already has. Under the
/// `[Acting]` pressure a base model reaches for these as the cheapest schema-valid
/// "action" and files its real intent into prose (glass-boxed 2026-07-16: 1855/3288
/// live tool calls were this filler, e.g. nine straight `commands/help` turns on a
/// one-line `add(a,b)` while the answer sat ready). The FIRST orientation per concern
/// is honest; a REPEAT once any survey receipt is in the concern is spin.
///
/// The `code/tree` case (glass-boxed 2026-07-16, benchmark): after the tool-surface
/// demote crushed the `help`/`list_commands` filler 99%, the SAME act-pressure
/// disposition displaced to `code/tree` — 156 of 169 tool calls, arg-JITTERED
/// (`apps/cli` vs `apps/cli/`, `max_depth` 1 vs 2) to evade the exact-repeat guard,
/// re-surveying a tree the mind already fetched AND already has in `[workspace-map]`.
/// Demoting by CLASS + prior-receipt (ignoring args entirely) is immune to that jitter.
pub(super) fn is_redundant_orientation(recent: &[String], calls: &[ToolCall]) -> bool {
    if calls.is_empty() || !calls.iter().all(|c| is_orientation_call(&c.name)) {
        return false;
    }
    let scope = entries_since_last_settlement(recent);
    scope.iter().any(|trace| {
        trace.contains("commands/list(")
            || trace.contains("commands/help(")
            || trace.contains("code/tree(")
    })
}
