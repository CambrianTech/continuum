//! Perception predicates: pure structural-fact DETECTORS over the working-memory
//! trail and a tool batch. Extracted verbatim from `act_observe` (pure code-motion).
//! Each answers one yes/no about what the mind has already done this concern —
//! touched paths, per-concern scope, already-satisfied, orientation class, redundant
//! re-survey. No steering, no side effects: proprioception, not policy.

use crate::ai::types::ToolCall;
use crate::cognition::working_memory::{WmEntry, WmKind};

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

/// Evidence for a repeated batch, independent of per-attempt correlation IDs.
/// Compare actual typed results, never rendered/nudged working-memory prose.
#[derive(PartialEq)]
pub(super) struct BatchActEvidence {
    name: String,
    input: serde_json::Value,
    content: String,
    error: Option<bool>,
    verdict: crate::sdk_codegen::ActVerdict,
}

pub(super) fn batch_evidence(
    calls: &[ToolCall],
    recent: &[super::observation::Observation],
) -> Option<Vec<BatchActEvidence>> {
    if calls.is_empty() || recent.len() < calls.len() {
        return None;
    }
    let latest = &recent[recent.len() - calls.len()..];
    calls.iter().map(|call| {
        let act = latest.iter().find(|act| act.call.id == call.id)?;
        // Missing/full-result-spilled evidence and unfinished operations cannot
        // establish an unchanged completed observation. They reset the streak.
        if !matches!(act.status, super::observation::ActStatus::Executed)
            || act.output.result.custody.is_pointer()
            || act.output.verdict.running()
            || matches!(act.output.verdict, crate::sdk_codegen::ActVerdict::Undecodable)
        {
            return None;
        }
        Some(BatchActEvidence {
            name: call.name.clone(),
            input: call.input.clone(),
            content: act.output.result.content.clone(),
            error: act.output.result.is_error,
            verdict: act.output.verdict,
        })
    }).collect()
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
            a.mutated_workspace()
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
/// observing, the recorded fact leaves that to her. Batch-only disk change cannot
/// establish where a shell mutation fell relative to reads within that batch;
/// this ordered predicate therefore uses successful per-call evidence only.
pub fn wrote_without_observation(recent: &[WmEntry]) -> bool {
    let start = last_settlement(recent).map_or(0, |i| i + 1);
    let acts: Vec<_> = acts_in(&recent[start..]).collect();
    let Some(m) = acts.iter().rposition(|act| act.mutated_workspace()) else {
        return false;
    };
    !acts[m + 1..].iter().any(|act| {
        act.completed_successfully() && act.output.verb.observes()
    })
}

/// The caller knows whether settlement has already appended its boundary.
#[derive(Clone, Copy)]
pub(super) enum ConcernPhase {
    Ongoing,
    JustSettled,
}

/// Successful mutating calls or a measured checkout change in THIS concern.
/// Batch disk evidence belongs to the receipt, not to an arbitrary shell call.
pub(super) fn mutated_workspace(recent: &[WmEntry], phase: ConcernPhase) -> bool {
    let end = match phase {
        ConcernPhase::Ongoing => recent.len(),
        ConcernPhase::JustSettled => last_settlement(recent).unwrap_or(recent.len()),
    };
    let start = last_settlement(&recent[..end]).map_or(0, |i| i + 1);
    recent[start..end].iter().any(|entry| {
        matches!(entry.kind, WmKind::Receipt { .. })
            && (entry.disk_change == super::disk_change::DiskChange::Changed
                || entry.acts.iter().any(super::observation::Observation::mutated_workspace))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::act_observe::test_support::{act, receipt, settle};
    use crate::cognition::act_observe::ActStatus;

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
            mutated_workspace(&wrote, ConcernPhase::JustSettled),
            "an edit inside the concern that just settled COUNTS — the marker at the tail must not hide it"
        );

        let only_looked = vec![receipt(vec![act("code/tree", None)]), settle()];
        assert!(
            !mutated_workspace(&only_looked, ConcernPhase::JustSettled),
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
            !mutated_workspace(&stale, ConcernPhase::JustSettled),
            "mutation is scoped to THIS concern — an earlier concern's write does not count"
        );
    }

    // what this catches: a previous concern or unsuccessful edit suppressing current no-deliverable feedback.
    #[test]
    fn ongoing_concern_does_not_inherit_previous_writes_or_failed_edits() {
        use crate::sdk_codegen::{ActVerdict, ToolVerdict};
        let mut entries = vec![receipt(vec![act("code/edit", Some("old.rs"))]), settle()];
        entries.push(receipt(vec![act("code/read", Some("current.rs"))]));
        assert!(!mutated_workspace(&entries, ConcernPhase::Ongoing));
        assert!(mutated_workspace(&entries, ConcernPhase::JustSettled));
        for verdict in [
            ActVerdict::Declared(ToolVerdict::Failed),
            ActVerdict::Declared(ToolVerdict::Running),
            ActVerdict::Undecodable,
        ] {
            let mut failed = act("code/edit", Some("current.rs"));
            failed.output.verdict = verdict;
            entries.push(receipt(vec![failed]));
            assert!(!mutated_workspace(&entries, ConcernPhase::Ongoing));
            assert!(!wrote_without_observation(&entries));
        }
        let mut transport_error = act("code/edit", Some("current.rs"));
        transport_error.output.result.is_error = Some(true);
        entries.push(receipt(vec![transport_error]));
        let mut refused = act("code/edit", Some("current.rs"));
        refused.status = ActStatus::Errored { message: "not executed".into() };
        entries.push(receipt(vec![refused]));
        assert!(!mutated_workspace(&entries, ConcernPhase::Ongoing));
        assert!(!wrote_without_observation(&entries));
        entries.push(receipt(vec![act("code/edit", Some("current.rs"))]));
        assert!(wrote_without_observation(&entries));
        let mut failed_read = act("code/read", Some("current.rs"));
        failed_read.output.result.is_error = Some(true);
        entries.push(receipt(vec![failed_read]));
        assert!(wrote_without_observation(&entries), "failed read does not observe the edit");
        assert!(mutated_workspace(&entries, ConcernPhase::Ongoing));
        entries.push(settle());
        assert!(mutated_workspace(&entries, ConcernPhase::JustSettled));
        assert!(!mutated_workspace(&entries, ConcernPhase::Ongoing));
    }

    // what this catches: real shell effects lost between fingerprinting, checkpointed receipt and settlement.
    #[test]
    fn measured_shell_change_survives_receipt_checkpoint_and_settlement() {
        use super::super::disk_change::{checkout_fingerprint, DiskChange};
        use crate::cognition::working_memory::WorkingMemory;
        use crate::ai::types::ResultCustody;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for args in [vec!["init", "-q"], vec!["-c", "user.name=test", "-c", "user.email=t@t", "commit", "--allow-empty", "-qm", "base"]] {
            assert!(std::process::Command::new("git").arg("-C").arg(root).args(args).output().unwrap().status.success());
        }
        let before = checkout_fingerprint(root);
        assert!(before.is_some());
        // A real checkout write through a shell-class receipt, not a mutating verb.
        std::fs::write(root.join("authored.rs"), "fn authored() {}\n").unwrap();
        let disk = DiskChange::between(before, checkout_fingerprint(root));
        assert_eq!(disk, DiskChange::Changed);
        let wm = WorkingMemory::new(32);
        wm.record_receipt_measured(&[act("code/shell", None)], "shell completed", None, ResultCustody::Unsaved, disk);
        assert!(mutated_workspace(&wm.recent_entries(), ConcernPhase::Ongoing));
        let snap = serde_json::to_value(wm.snapshot()).unwrap();
        let restored = WorkingMemory::new(32);
        restored.restore(serde_json::from_value(snap).unwrap());
        assert!(mutated_workspace(&restored.recent_entries(), ConcernPhase::Ongoing));
        restored.record_settlement("done", None);
        assert!(mutated_workspace(&restored.recent_entries(), ConcernPhase::JustSettled));
        restored.record_receipt_typed(&[act("code/read", None)], "read completed", None, ResultCustody::Unsaved);
        assert!(!mutated_workspace(&restored.recent_entries(), ConcernPhase::Ongoing));
        let legacy: WmEntry = serde_json::from_value(serde_json::json!({
            "kind": {"Receipt": {"n": 1}}, "text": "legacy", "acts": []
        })).unwrap();
        assert_eq!(legacy.disk_change, DiskChange::Unknown);
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
            occurrence: 0,
            disk_change: Default::default(),
            native_calls: Vec::new(),
            kind: WmKind::Fact,
            text: "[unfulfilled] real executions leave [action #n] receipts".to_string(),
            acts: Vec::new(),
            scope: None,
        };
        assert!(
            !any_real_receipt(&[facty]),
            "a Fact that MENTIONS [action #n] is not a receipt — kind, not prose"
        );
    }
}
