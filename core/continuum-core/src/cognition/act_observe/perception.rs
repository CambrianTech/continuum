//! Perception predicates: pure structural-fact DETECTORS over the working-memory
//! trail and a tool batch. Extracted verbatim from `act_observe` (pure code-motion).
//! Each answers one yes/no about what the mind has already done this concern —
//! touched paths, per-concern scope, already-satisfied, orientation class, redundant
//! re-survey. No steering, no side effects: proprioception, not policy.

use crate::ai::types::ToolCall;
use crate::cognition::working_memory::{WmEntry, WmKind};

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
pub(super) fn all_calls_already_satisfied(
    recent: &[String],
    calls: &[ToolCall],
    fold: Option<usize>,
) -> bool {
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

/// The typed acts inside a slice of working-memory entries, oldest → newest —
/// flattened across every receipt so a batch that ran N calls contributes N acts
/// in order. The typed sibling of scanning receipt PROSE for `I ran …`, which is
/// the seam-5 live-drift bug this refactor kills: the live receipt render is a
/// bare `name(args)` (the `I ran ` opener was removed for #158), so the old string
/// scans NEVER fired against a real recency window — `[no-deliverable]` and
/// `[unobserved]` were structurally dead. Reading `ToolVerb` off the typed
/// `Observation` is immune to that drift.
fn acts_in<'a>(
    entries: &'a [WmEntry],
) -> impl Iterator<Item = &'a super::observation::Observation> {
    entries.iter().flat_map(|e| e.acts.iter())
}

/// Index of the last SETTLEMENT boundary in a typed entry slice, or `None`.
fn last_settlement(entries: &[WmEntry]) -> Option<usize> {
    entries
        .iter()
        .rposition(|e| matches!(e.kind, WmKind::Settlement))
}

/// TRUE if any entry in this slice is a real tool RECEIPT — the typed kind query
/// that replaces string-scanning rendered text for a numbered `[action #n]` line.
/// The `[action #n]` teaching texts and the proprioception facts both used to
/// render with brackets, so a bare `contains("[action #")` matched the mention
/// and the confab backstop went blind after its own first firing (the 2026-07-12
/// suppression onion). `WmKind::Receipt` cannot be spoofed by prose.
pub(super) fn any_real_receipt(entries: &[WmEntry]) -> bool {
    entries
        .iter()
        .any(|e| matches!(e.kind, WmKind::Receipt { .. }))
}

/// Did this Speak CLAIM completed work on a named file that no tool act backs?
///
/// Returns the first file name (a backtick-quoted or bare `name.ext` token) that
/// appears in the same text as a completion-claim verb (created / implemented /
/// wrote / added / finished / ready) when the working-memory snapshot contains
/// NO mutating act (`code/write`/`code/edit`/…) whose TYPED `paths` name that
/// file. Text tokens × typed act paths — exact typed membership from her OWN
/// call input, immune to the receipt head-truncation that dropped a filename off
/// the tail of a long result. Deliberately conservative — no claim verbs → None,
/// so ordinary discussion of files is never taxed; and the recorded fact says
/// only "my memory shows no act", because a finite trace can't disprove
/// past-session work.
pub(super) fn claimed_file_without_act(text: &str, recent: &[WmEntry]) -> Option<String> {
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
        // Unbacked iff no MUTATING act names this file in its typed paths.
        !acts_in(recent).any(|a| {
            a.output.verb.mutates()
                && a.output
                    .paths
                    .iter()
                    .any(|p| p.to_string_lossy().contains(f.as_str()))
        })
    })
}

/// Did the CURRENT concern mutate the workspace without a later observation act?
///
/// From the last settlement boundary: true when a mutating act (`ToolVerb::mutates`)
/// appears with NO subsequent observing act (`ToolVerb::observes`) after the LAST
/// mutation. Typed verbs, not receipt prose — the seam-5 fix (`acts_in`). Pure
/// geometry over her own acts; no judgment about whether the artifact needed
/// observing, the recorded fact leaves that to her.
pub(super) fn wrote_without_observation(recent: &[WmEntry]) -> bool {
    let start = last_settlement(recent).map_or(0, |i| i + 1);
    let verbs: Vec<&super::observation::ToolVerb> =
        acts_in(&recent[start..]).map(|a| &a.output.verb).collect();
    let Some(m) = verbs.iter().rposition(|v| v.mutates()) else {
        return false;
    };
    !verbs[m + 1..].iter().any(|v| v.observes())
}

/// Did THIS concern actually change the workspace? True iff a mutating act
/// (`ToolVerb::mutates`) sits after the last settlement marker — the same
/// concern-scoping [`wrote_without_observation`] uses, so the two agree by
/// construction about what a mutation is.
///
/// Deliberately act-based on the TYPED verb, not string prose: a turn can spend
/// acts on `code/tree` + `code/read` and still have produced nothing a diff-grader
/// will see (the live sympy-21379 shape — one act, zero bytes). Only a mutating
/// act that really executed counts.
/// CRITICAL ordering detail: `settle_step`'s Speak arm records its settlement
/// marker BEFORE returning (which is why that arm snapshots `pre_settle` first).
/// So by the time this runs the marker is already the tail, and scanning "after
/// the last marker" would read an EMPTY span and call every turn unmutated. The
/// concern that just settled is the span ENDING at that marker.
pub(super) fn mutated_workspace(recent: &[WmEntry]) -> bool {
    let is_settle = |e: &WmEntry| matches!(e.kind, WmKind::Settlement);
    let end = recent.iter().rposition(is_settle).unwrap_or(recent.len());
    let start = recent[..end]
        .iter()
        .rposition(is_settle)
        .map_or(0, |i| i + 1);
    acts_in(&recent[start..end]).any(|a| a.output.verb.mutates())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::{ToolCall, ToolResult};
    use crate::cognition::act_observe::observation::{
        extract_paths, ActStatus, Observation, ToolOutput, ToolVerb,
    };

    // Build ONE typed act (call + id-correlated result + precomputed verb/paths)
    // the way the live act seam does — the fixtures the string scans could never
    // reach (the live receipt render is a bare `name(args)`, no `I ran ` opener,
    // so the old `"I ran code/write("` fixtures were the GREEN≠REACHABLE defect).
    fn act(name: &str, path: Option<&str>) -> Observation {
        let input = match path {
            Some(p) => serde_json::json!({ "file_path": p }),
            None => serde_json::json!({}),
        };
        Observation {
            call: ToolCall {
                id: "c".into(),
                name: name.into(),
                input: input.clone(),
            },
            output: ToolOutput {
                result: ToolResult {
                    tool_use_id: "c".into(),
                    content: "ok".into(),
                    is_error: None,
                spill_handle: None,
                },
                verb: ToolVerb::classify(name),
                paths: extract_paths(&input),
            },
            status: ActStatus::Executed,
        }
    }
    // A receipt WmEntry carrying the typed acts (text is irrelevant to the typed
    // predicates — they read `acts`, never re-parse the string).
    fn receipt(acts: Vec<Observation>) -> WmEntry {
        WmEntry {
            kind: WmKind::Receipt { n: 1 },
            text: String::new(),
            acts,
        }
    }
    fn settle() -> WmEntry {
        WmEntry {
            kind: WmKind::Settlement,
            text: crate::cognition::working_memory::WM_SETTLEMENT_PREFIX.to_string(),
            acts: Vec::new(),
        }
    }

    // what this catches: the ORDERING TRAP in `mutated_workspace`. `settle_step`'s Speak
    // arm records its settlement marker BEFORE the driver's arm runs, so a naive
    // "scan after the last marker" reads an EMPTY tail and calls EVERY turn unmutated —
    // which would fire the re-perception at a persona who had just written the file.
    // The concern that settled is the span ENDING at that marker. Typed over ToolVerb
    // now (seam-5): the old `"I ran code/edit("` string fixtures matched a receipt render
    // the live path never emits, so the predicate was dead against real recency.
    #[test]
    fn mutation_is_read_from_the_concern_that_just_settled_not_the_empty_tail() {
        let wrote = vec![
            receipt(vec![act("code/read", Some("x.py"))]),
            receipt(vec![act("code/edit", Some("x.py"))]),
            settle(),
        ];
        assert!(
            mutated_workspace(&wrote),
            "an edit inside the concern that just settled COUNTS — the marker at the tail must not hide it"
        );

        let only_looked = vec![receipt(vec![act("code/tree", None)]), settle()];
        assert!(
            !mutated_workspace(&only_looked),
            "acts that only LOOK are not a deliverable — the live sympy-21379 shape"
        );

        // A prior concern's edit must not launder the current one.
        let stale = vec![
            receipt(vec![act("code/write", Some("a.py"))]),
            settle(),
            receipt(vec![act("code/read", Some("b.py"))]),
            settle(),
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
    // no backing mutating act (typed `paths`) yields the file; a claim WITH a
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
        // A write act whose TYPED path names the file backs the claim → None.
        let backed = vec![receipt(vec![act("code/write", Some("game_of_life.rs"))])];
        assert_eq!(claimed_file_without_act(claim, &backed), None);
        // A read of the same file does NOT back a completion claim (only a mutation does).
        let only_read = vec![receipt(vec![act("code/read", Some("game_of_life.rs"))])];
        assert_eq!(
            claimed_file_without_act(claim, &only_read).as_deref(),
            Some("game_of_life.rs"),
            "reading a file is not implementing it — the claim stays unbacked"
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
    // that mutated the workspace with no LATER observation act (run/shell/read/
    // screenshot) is unobserved; observation BEFORE the mutation doesn't count;
    // a prior settled concern's writes don't nag the next one. Typed over ToolVerb.
    #[test]
    fn unobserved_mutation_geometry() {
        let w = || receipt(vec![act("code/write", Some("game.rs"))]);
        let r = || receipt(vec![act("code/shell", None)]);
        let read_first = || receipt(vec![act("code/read", Some("game.rs"))]);

        // write with no later observation → unobserved
        assert!(wrote_without_observation(&[w()]));
        // write then run → observed
        assert!(!wrote_without_observation(&[w(), r()]));
        // read BEFORE the write doesn't count as observing the write
        assert!(wrote_without_observation(&[read_first(), w()]));
        // a prior settled concern's write never leaks into this concern
        assert!(!wrote_without_observation(&[w(), settle(), r()]));
        // no mutation at all → nothing to observe
        assert!(!wrote_without_observation(&[receipt(vec![act(
            "code/tree",
            None
        )])]));
    }

    // what this catches: the typed receipt-presence query that replaces the
    // brackets-in-prose scan (`has_real_action_receipt`). A window with a Receipt
    // reads true; Facts/Settlements/Thoughts — even ones rendering `[action #n]`
    // teaching text — read false (the 2026-07-12 suppression onion).
    #[test]
    fn any_real_receipt_reads_the_kind_not_the_prose() {
        assert!(any_real_receipt(&[receipt(vec![act(
            "code/read",
            Some("x.py")
        )])]));
        assert!(!any_real_receipt(&[settle()]));
        let facty = WmEntry {
            kind: WmKind::Fact,
            text: "[unfulfilled] real executions leave [action #n] receipts".to_string(),
            acts: Vec::new(),
        };
        assert!(
            !any_real_receipt(&[facty]),
            "a Fact that MENTIONS [action #n] is not a receipt — kind, not prose"
        );
    }
}
