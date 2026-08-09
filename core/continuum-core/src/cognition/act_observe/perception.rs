//! Perception predicates: pure structural-fact DETECTORS over the working-memory
//! trail and a tool batch. Extracted verbatim from `act_observe` (pure code-motion).
//! Each answers one yes/no about what the mind has already done this concern —
//! touched paths, per-concern scope, already-satisfied, orientation class, redundant
//! re-survey. No steering, no side effects: proprioception, not policy.

use crate::ai::types::ToolCall;

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

/// True only for a REAL receipt line — `[action #<digit>`. The proprioception
/// TEACHING texts mention the literal placeholder `[action #n]` ("real
/// executions leave [action #n] receipts"), and a bare `contains("[action #")`
/// matches the mention: the medicine suppressed the diagnosis (glass-boxed
/// 2026-07-12 — the [actions] zero-fact vanished from every prompt the moment
/// any backstop fact rendered, and the confab backstop went blind after its
/// own first firing). Receipts are numbered; placeholders are not.
/// …and numbering alone is not enough: `record_action` numbers EVERY working-
/// memory entry, so the proprioception facts themselves render as
/// `[action #4] [unfulfilled] …` — the facts wore receipt numbering and
/// suppressed the zero-fact all afternoon (glass-boxed 16:50 2026-07-12,
/// second layer of the same onion). A real receipt's body is prose
/// ("I ran code/shell(…) Result: …"); a fact's body opens with another
/// bracket tag. Digit + non-bracket body = receipt.
pub(crate) fn has_real_action_receipt(text: &str) -> bool {
    text.match_indices("[action #").any(|(i, _)| {
        let rest = &text[i + "[action #".len()..];
        let mut chars = rest.chars();
        if !chars.next().is_some_and(|c| c.is_ascii_digit()) {
            return false;
        }
        // Body after "N] " must not open with a bracket tag (a fact), and
        // must exist at all (a bare numbered line is not a receipt).
        rest.split_once(']')
            .map(|(_, body)| {
                let body = body.trim_start();
                !body.is_empty() && !body.starts_with('[')
            })
            .unwrap_or(false)
    })
}

/// Did this Speak CLAIM completed work on a named file that no tool act backs?
///
/// Returns the first file name (a backtick-quoted or bare `name.ext` token) that
/// appears in the same text as a completion-claim verb (created / implemented /
/// wrote / added / finished / ready) when the working-memory snapshot contains
/// NO `code/write`/`code/edit` act mentioning that file. Pure geometry: text
/// tokens × trace lines. Deliberately conservative — no claim verbs → None, so
/// ordinary discussion of files is never taxed; and the recorded fact says only
/// "my memory shows no act", because a finite trace can't disprove past-session
/// work.
pub(super) fn claimed_file_without_act(text: &str, recent: &[String]) -> Option<String> {
    let lower = text.to_lowercase();
    const CLAIM_VERBS: &[&str] = &[
        "i've created",
        "i have created",
        "i created",
        "i've implemented",
        "i have implemented",
        "i implemented",
        "i've written",
        "i have written",
        "i wrote",
        "i've added",
        "i've finished",
        "is ready in",
        "is written and ready",
    ];
    if !CLAIM_VERBS.iter().any(|v| lower.contains(v)) {
        return None;
    }
    // File tokens: word.ext where ext is a short alpha extension (rs, py, css,
    // html, ts, md, …). Scan the original text so the recorded name keeps case.
    let mut candidates = Vec::new();
    for raw in text.split(|c: char| !(c.is_alphanumeric() || c == '.' || c == '_' || c == '-')) {
        if let Some((stem, ext)) = raw.rsplit_once('.') {
            if !stem.is_empty()
                && (1..=4).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphabetic())
            {
                candidates.push(raw.to_string());
            }
        }
    }
    candidates.into_iter().find(|f| {
        !recent.iter().any(|l| {
            (l.contains("I ran code/write(") || l.contains("I ran code/edit(")) && l.contains(f.as_str())
        })
    })
}

/// Did the CURRENT concern mutate the workspace without a later observation act?
///
/// Scans a working-memory snapshot (oldest → newest, taken BEFORE the settlement
/// marker lands) from the last `[settled]` boundary: true when a `code/write` or
/// `code/edit` act appears with NO subsequent run/read/screenshot-class act after
/// the LAST mutation. Pure geometry over the trace — no judgment about whether
/// the artifact needed observing; the recorded fact leaves that to her.
pub(super) fn wrote_without_observation(recent: &[String]) -> bool {
    let start = recent
        .iter()
        .rposition(|l| l.starts_with(crate::cognition::working_memory::WM_SETTLEMENT_PREFIX))
        .map_or(0, |i| i + 1);
    let concern = &recent[start..];
    let last_mutation = concern
        .iter()
        .rposition(|l| l.contains("I ran code/write(") || l.contains("I ran code/edit("));
    let Some(m) = last_mutation else { return false };
    !concern[m + 1..].iter().any(|l| {
        l.contains("I ran code/run(")
            || l.contains("I ran code/shell(")
            || l.contains("I ran code/read(")
            || l.contains("I ran interface/screenshot(")
    })
}

/// Did THIS concern actually change the workspace? True iff a `code/write` /
/// `code/edit` receipt sits after the last settlement marker — the same
/// concern-scoping and the same receipt vocabulary [`wrote_without_observation`]
/// uses, so the two agree by construction about what a mutation is.
///
/// Deliberately receipt-based, not act-count-based: a turn can spend acts on
/// `code/tree` + `code/read` and still have produced nothing a diff-grader will
/// see (the live sympy-21379 shape — one act, zero bytes). Only a receipt of a
/// mutation that really executed counts.
/// CRITICAL ordering detail: `settle_step`'s Speak arm records its settlement
/// marker BEFORE returning (which is why that arm snapshots `pre_settle` first).
/// So by the time this runs the marker is already the tail, and scanning "after
/// the last marker" would read an EMPTY span and call every turn unmutated. The
/// concern that just settled is the span ENDING at that marker.
pub(super) fn mutated_workspace(recent: &[String]) -> bool {
    let is_settle =
        |l: &String| l.starts_with(crate::cognition::working_memory::WM_SETTLEMENT_PREFIX);
    let end = recent.iter().rposition(is_settle).unwrap_or(recent.len());
    let start = recent[..end].iter().rposition(is_settle).map_or(0, |i| i + 1);
    recent[start..end]
        .iter()
        .any(|l| l.contains("I ran code/write(") || l.contains("I ran code/edit("))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the ORDERING TRAP in `mutated_workspace`. `settle_step`'s Speak
    // arm records its settlement marker BEFORE the driver's arm runs, so a naive
    // "scan after the last marker" reads an EMPTY tail and calls EVERY turn unmutated —
    // which would fire the re-perception at a persona who had just written the file.
    // The concern that settled is the span ENDING at that marker.
    #[test]
    fn mutation_is_read_from_the_concern_that_just_settled_not_the_empty_tail() {
        let settle = crate::cognition::working_memory::WM_SETTLEMENT_PREFIX;
        let wrote = vec![
            "[action #1] I ran code/read(file_path: x.py) Result: ok".to_string(),
            "[action #2] I ran code/edit(file_path: x.py) Result: ok".to_string(),
            format!("{settle} here is what I changed"),
        ];
        assert!(
            mutated_workspace(&wrote),
            "an edit inside the concern that just settled COUNTS — the marker at the tail must not hide it"
        );

        let only_looked = vec![
            "[action #1] I ran code/tree(path: .) Result: src/".to_string(),
            format!("{settle} here is my analysis of the bug"),
        ];
        assert!(
            !mutated_workspace(&only_looked),
            "acts that only LOOK are not a deliverable — the live sympy-21379 shape"
        );

        // A prior concern's edit must not launder the current one.
        let stale = vec![
            "[action #1] I ran code/write(file_path: a.py) Result: ok".to_string(),
            format!("{settle} done with the first thing"),
            "[action #2] I ran code/read(file_path: b.py) Result: ok".to_string(),
            format!("{settle} and here is my analysis of the second"),
        ];
        assert!(
            !mutated_workspace(&stale),
            "mutation is scoped to THIS concern — an earlier concern's write does not count"
        );
    }

    // what this catches: the claimed-without-acting geometry (live specimen
    // 2026-07-11: Asha's "I've implemented the game update function in
    // `game_of_life.rs`" with zero tool acts on that file — peers then offered
    // to review code that didn't exist). A completion claim naming a file with
    // no backing write/edit act in the trace yields the file; a claim WITH a
    // backing act yields None; discussion without claim verbs is never taxed.
    #[test]
    fn unacted_claim_geometry() {
        let claim = "I've implemented the game update function in `game_of_life.rs`, \
                     which applies Conway's rules.";
        // No backing act → the claim is unacted.
        assert_eq!(
            claimed_file_without_act(claim, &[]).as_deref(),
            Some("game_of_life.rs")
        );
        // A write act naming the file backs the claim → None.
        let backed = "[action #3] I ran code/write({\"file_path\":\"game_of_life.rs\"}) …";
        assert_eq!(
            claimed_file_without_act(claim, &[backed.to_string()]),
            None
        );
        // Plain discussion of a file without claim verbs is never taxed.
        assert_eq!(
            claimed_file_without_act("let's look at game_of_life.rs together", &[]),
            None
        );
        // Claim verbs without a named file → nothing checkable, no fact.
        assert_eq!(
            claimed_file_without_act("I've implemented the logic we discussed", &[]),
            None
        );
    }

    // what this catches: the observation-gap geometry (Joel 2026-07-11 — the
    // run+observe half of the creation loop is part of THEIR process). A concern
    // that mutated the workspace (code/write / code/edit) with no LATER
    // observation act (run/shell/read/screenshot) is unobserved; observation
    // BEFORE the mutation doesn't count; a prior settled concern's writes don't
    // nag the next one.
    #[test]
    fn unobserved_mutation_geometry() {
        let w = "[action #1] I ran code/write({\"file_path\":\"game.rs\"}) …".to_string();
        let r = "[action #2] I ran code/shell({\"cmd\":\"cargo run\"}) …".to_string();
        let read_first = "[action #0] I ran code/read({\"file_path\":\"game.rs\"}) …".to_string();

        // write with no later observation → unobserved
        assert!(wrote_without_observation(&[w.clone()]));
        // write then run → observed
        assert!(!wrote_without_observation(&[w.clone(), r.clone()]));
        // read BEFORE the write doesn't count as observing the write
        assert!(wrote_without_observation(&[read_first, w.clone()]));
        // a prior settled concern's write never leaks into this concern
        let settled = format!(
            "{} I answered: done",
            crate::cognition::working_memory::WM_SETTLEMENT_PREFIX
        );
        assert!(!wrote_without_observation(&[w, settled, r]));
        // no mutation at all → nothing to observe
        assert!(!wrote_without_observation(&[
            "[action #1] I ran code/tree({}) …".to_string()
        ]));
    }
}
