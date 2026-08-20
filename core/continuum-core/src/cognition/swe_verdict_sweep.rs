//! Every artifact that holds work gets a verdict — with no operator in the loop.
//!
//! # Why this exists (2026-08-18, and the number it recovered)
//!
//! The grade tail was built and correct, and it still needed a human to fire it. On the night
//! it landed, 104 staged trees sat on this box; 17 of them held real citizen patches that had
//! never been scored, and two of those were PASSES that had been sitting ungraded for over a
//! day. The verdicts only appeared because an operator ran `benchmark/swe-grade` twenty times
//! by hand. That is not a benchmark — that is a person with a shell.
//!
//! Joel, on being shown the recovered number: *"Needs to automatically work too"*.
//!
//! # The doctrine this obeys, and the one it must not break
//!
//! [[the-whole-system-is-event-based-not-polling]] forbids scanning the board on a clock to ask
//! "has anything become gradeable yet" — a condition-poll that duplicates an event. This is NOT
//! that. It is the same shape as [`crate::cognition::swe_bench::reap_orphaned_solve_runs`]: a
//! BOOT RECONCILIATION that enumerates durable state once and makes it consistent, then stops.
//! Boot owns the process tree, reap-or-adopt, for every service (#452,
//! [[boot-owns-the-process-tree-reap-or-adopt-never-fight-yourself]]) — and an orphaned
//! ARTIFACT is the same class of thing as an orphaned process. A patch nobody scored is a run
//! nobody reaped.
//!
//! The axis Joel named is DETERMINISM, not tick-vs-event:
//!
//! > *"if it's deterministic and not scan it or polling it's reliable"*
//!
//! So this sweep is deterministic by construction, and each property is load-bearing:
//!
//! - **Enumerates ALL staged instances, sorted.** No cap, no recency sort, no sampling. The
//!   board's own artifact scan has all three and lost 12 of 13 artifacts to them — same input,
//!   different answer depending on timing, which is the definition of unreliable.
//! - **Idempotent.** An instance with a recorded verdict is skipped, so re-running changes
//!   nothing and a restart mid-sweep resumes rather than re-grades.
//! - **Refuses rather than guesses.** Ambiguity and absence are outcomes, not zeros.
//!
//! Given the same disk, it always produces the same set of grades.
//!
//! # What it will never do
//!
//! It never manufactures a score. The three guards that keep the durable record honest live in
//! [`crate::cognition::swe_bench::record_verdict`] and are inherited here, not re-implemented:
//! a gold patch is a control and never counts, an errored verdict is an environment fault and
//! never counts, and an empty candidate is an ABSENCE and never counts
//! ([[a-perception-fact-is-honesty-not-an-actuator]]). This module adds one more of the same
//! family: two citizens holding worked copies of one instance is ambiguity, and grading either
//! would score one citizen's diff against the other's card, so it grades neither and says so.

use std::path::PathBuf;

use crate::persona::staged_workspace::{grade_target, owners_of, GradeTarget};

/// An artifact awaiting a verdict: the instance, and the one worked copy to score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingGrade {
    pub instance: String,
    pub workspace: PathBuf,
}

/// What the sweep decided for one instance — every non-grade outcome is NAMED, because
/// "we skipped it" and "it scored zero" must never be the same row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepDecision {
    /// Exactly one worked copy and no verdict on file. Score it.
    Grade(PathBuf),
    /// A verdict already exists. Skipping is what makes the sweep idempotent.
    AlreadyGraded,
    /// Nothing was ever written here. An absence, never a zero.
    NoWork,
    /// Two or more citizens hold worked copies. Refuse and name the candidates.
    Ambiguous(Vec<PathBuf>),
}

/// The rule alone, with the filesystem taken out of it.
///
/// Split from [`pending`] for the same reason
/// [`crate::persona::staged_workspace::grade_target`] is split from `owners_of`: the DECISION
/// is tested against a table, not against a disk fixture that re-derives it. A test that
/// rebuilds the predicate in its own body cannot fail when the real one changes.
pub fn decide(has_verdict: bool, target: GradeTarget) -> SweepDecision {
    if has_verdict {
        // Checked FIRST and deliberately: idempotence must not depend on the tree still
        // being dirty. A graded artifact whose workspace was since cleaned would otherwise
        // read as NoWork and churn a decision every boot.
        return SweepDecision::AlreadyGraded;
    }
    match target {
        GradeTarget::One(path) => SweepDecision::Grade(path),
        GradeTarget::NoWork => SweepDecision::NoWork,
        GradeTarget::Ambiguous(paths) => SweepDecision::Ambiguous(paths),
    }
}

/// Every instance staged into ANY citizen's workspace, sorted and deduped.
///
/// Deterministic by construction — see the module doc. The sort is not cosmetic: it fixes
/// grading ORDER, so a sweep interrupted halfway resumes at the same place rather than at
/// whatever `read_dir` happened to yield first.
pub fn all_staged_instances() -> Vec<String> {
    let Ok(home) = crate::commands::benchmark::continuum_home() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(home.join("citizens").join("peers")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| uuid::Uuid::parse_str(n).ok()))
        .flat_map(|peer| crate::persona::staged_workspace::staged_instances(&peer))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Every artifact that holds work and has no verdict, in a stable order.
pub fn pending() -> Vec<PendingGrade> {
    let mut out = Vec::new();
    for instance in all_staged_instances() {
        let has_verdict = crate::cognition::swe_bench::read_verdict(&instance).is_some();
        // Skip the `git status` fan-out entirely when a verdict already exists — `owners_of`
        // shells out once per staged copy, and on a box with 100+ trees that is the whole
        // cost of the sweep. Cheap check first.
        if has_verdict {
            continue;
        }
        match decide(false, grade_target(&owners_of(&instance))) {
            SweepDecision::Grade(workspace) => out.push(PendingGrade { instance, workspace }),
            SweepDecision::Ambiguous(paths) => crate::probe!(
                class = "benchmark.verdict.sweep_ambiguous",
                instance = instance.as_str(),
                candidates = paths.len(),
                "two citizens hold worked copies — refusing to grade either (#419)",
            ),
            SweepDecision::NoWork | SweepDecision::AlreadyGraded => {}
        }
    }
    out
}

/// What one sweep did — reported as a probe so the run is legible from the state pipe
/// rather than from a log parse.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SweepReport {
    pub graded: usize,
    pub resolved: usize,
    pub ungradeable: usize,
    pub errored: usize,
}

/// Grade every pending artifact, sequentially, recording each verdict.
///
/// SEQUENTIAL on purpose. Each grade takes a fresh clone at `base_commit` and runs a real test
/// suite; N of those in parallel would compete with the citizens' own serving lane for the
/// machine the round is being measured on ([[measured-work-gets-an-exclusive-warm-slot]]).
/// The sweep is background work that must never become the reason a turn is slow.
pub async fn sweep() -> SweepReport {
    let mut report = SweepReport::default();
    let work = pending();
    if work.is_empty() {
        return report;
    }
    crate::probe!(
        class = "benchmark.verdict.sweep_start",
        pending = work.len(),
        "boot artifact sweep — artifacts holding work with no verdict on file",
    );
    for item in work {
        // The ONE grader. Never an inline second reading of her work — that drift already cost
        // a credential leak once (see `SOLUTION_PATH_EXCLUDES`). `grade_swe` records the
        // verdict itself, so this loop only tallies.
        let params = crate::commands::benchmark::SweGradeParams {
            instance: item.instance.clone(),
            dataset: None,
            gold: None,
            patch: None,
            workspace: Some(item.workspace.to_string_lossy().into_owned()),
        };
        match crate::commands::benchmark::grade_swe(params).await {
            Ok(result) if result.error.is_some() => {
                report.ungradeable += 1;
                crate::probe!(
                    class = "benchmark.verdict.sweep_ungradeable",
                    instance = item.instance.as_str(),
                    "environment fault, NOT a capability zero — nothing recorded",
                );
            }
            Ok(result) => {
                report.graded += 1;
                if result.resolved {
                    report.resolved += 1;
                }
            }
            Err(e) => {
                report.errored += 1;
                // Loud, and never fatal: one instance that cannot be graded must not cost the
                // sweep every artifact behind it.
                tracing::warn!(
                    instance = %item.instance,
                    error = %e,
                    "artifact sweep could not grade this instance — continuing",
                );
            }
        }
    }
    crate::probe!(
        class = "benchmark.verdict.sweep_done",
        graded = report.graded,
        resolved = report.resolved,
        ungradeable = report.ungradeable,
        errored = report.errored,
        "boot artifact sweep complete — every worked artifact now carries a verdict",
    );
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: the sweep manufacturing a score out of an absence or an ambiguity —
    /// the #384/#386 laundering class, which is why the grade tail existed at all. A worked
    /// copy grades; nothing else does, and each refusal keeps its own name.
    #[test]
    fn only_a_single_worked_copy_is_ever_graded_and_every_refusal_keeps_its_name() {
        let a = PathBuf::from("/peers/a/workspace/swe/sympy__sympy-24152");
        let b = PathBuf::from("/peers/b/workspace/swe/sympy__sympy-24152");

        assert_eq!(
            decide(false, GradeTarget::One(a.clone())),
            SweepDecision::Grade(a.clone()),
            "exactly one worked copy is the only thing that grades"
        );
        assert_eq!(
            decide(false, GradeTarget::NoWork),
            SweepDecision::NoWork,
            "an unworked tree is an ABSENCE — it must never become a zero"
        );
        assert_eq!(
            decide(false, GradeTarget::Ambiguous(vec![a.clone(), b.clone()])),
            SweepDecision::Ambiguous(vec![a.clone(), b]),
            "two worked copies must refuse, not pick — grading either scores the wrong citizen"
        );
    }

    /// what this catches: a sweep that re-grades on every boot, which would burn hours of test
    /// runs and rewrite verdicts that were already true. Idempotence is the property that lets
    /// this run unattended at all.
    #[test]
    fn a_recorded_verdict_short_circuits_every_target_state() {
        let a = PathBuf::from("/peers/a/workspace/swe/x");
        for target in [
            GradeTarget::One(a.clone()),
            GradeTarget::NoWork,
            GradeTarget::Ambiguous(vec![a.clone(), a]),
        ] {
            assert_eq!(
                decide(true, target),
                SweepDecision::AlreadyGraded,
                "a graded artifact is skipped REGARDLESS of what its tree looks like now — \
                 idempotence must not depend on the workspace still being dirty"
            );
        }
    }
}
