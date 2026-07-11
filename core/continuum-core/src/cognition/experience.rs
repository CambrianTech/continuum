//! The afferent nerve of self-evolution — turning a lived episode into a
//! **salient experience** worth learning from.
//!
//! See [docs/genome/ANY-ASK-IS-A-CLASS.md] § "What becomes a class — attention/
//! salience is the selection signal". The self-evolution loop already has its
//! efferent organs — `genome/teach` synthesizes corrected trajectories,
//! `forge/train` forges the LoRA, `cognition/eval` measures lift, and the L3
//! completion listener pages in a gene that beats its prior. What was missing is
//! the nerve carrying "something salient just happened to me" into "that becomes
//! a lesson": the path from a lived episode to the teacher's task set.
//!
//! This module is that nerve's vocabulary:
//! - [`ExperienceRecord`] — a lived episode retained *in full* (the lean
//!   `EvalTaskResult` keeps only a 200-char summary; the teacher needs to see
//!   HOW she failed, not just that she did).
//! - [`SalienceDetector`] — the selector in front of the teacher: *is this
//!   episode worth turning into curriculum, and why?* Polymorphic so the honest
//!   error-only detector and later composite detectors share one seam.
//! - [`salient_teach_set`] — the connective projection: salient, test-graded
//!   episodes → the `EvalTask`s `genome/teach` remediates. This is the un-built
//!   seam ("eval failures → teach tasks") made concrete.
//!
//! ## Honesty about the signal (the measurement spine, applied to the INPUT)
//!
//! A salience signal you cannot measure is a class trigger you cannot trust. Of
//! the composite proxies for "the mind attended to this" — error, struggle,
//! attention, surprise, uncertainty, arousal — only **error** is instrumented
//! end-to-end today (the doc's detectability table). So [`ErrorSalience`] keys on
//! error alone; the rest are the named frontier, never faked with a number the
//! substrate cannot yet produce ([[fallbacks-are-illegal-fail-loud]]).

use crate::cognition::act_observe::SettleOutcome;
use crate::cognition::eval::EvalTask;

/// Why an episode was selected as salient. Today the only variant emitted is
/// [`SalienceKind::Error`] — the one proxy instrumented end-to-end. The frontier
/// proxies (each a documented seam in the substrate, not yet a measurable
/// number) will join as variants when their signal is real:
/// - `Struggle` — `SettleOutcome.acts` near the budget / the spin-repeat detector.
/// - `Attention` — `WorkspaceTrace` per-faculty `Contribution.salience`.
/// - `Surprise` — a novelty/prediction-error scorer (the `Volition` faculty).
/// - `Arousal` — a task-difficulty arousal signal (the `Affect` faculty).
///
/// It is a tagged reason, not a closed taxonomy — a single variant now is the
/// honest state, not a placeholder to pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalienceKind {
    /// She got it wrong — a failed test, an empty/no-match answer, an infra
    /// fault surfaced as failure. The most trustworthy salience signal and the
    /// only one instrumented end-to-end.
    Error,
}

/// The verdict of a [`SalienceDetector`]: this episode IS salient, with a score
/// and the specific weakness it exposes. The `weakness` is what a curriculum
/// synthesizer targets — for an eval failure it is the real grader output (the
/// compiler/test error), exactly what `genome/teach` feeds back to remediate.
#[derive(Debug, Clone)]
pub struct SalienceSignal {
    /// How salient, in `[0, 1]`. `1.0` for a clear failure; the score axis is
    /// what a composite detector will grade continuously (near-budget struggle,
    /// high attention) once those proxies are real. A single detector keys on
    /// one kind; a future composite maxes/blends across kinds.
    pub score: f32,
    /// Which proxy fired.
    pub kind: SalienceKind,
    /// The specific weakness this episode exposes — the thing to remediate. For
    /// [`SalienceKind::Error`] this is the grader's verdict (the real error).
    pub weakness: String,
}

/// A lived episode retained *in full* for learning — the INPUT to curriculum
/// synthesis. Unlike the lean `EvalTaskResult` report (a 200-char summary), this
/// keeps the rich trajectory the teacher needs: the task (with its ground-truth
/// `test`), her untruncated answer, the final act→observe world-state, and the
/// effort (`acts`). Sourced from an eval task now; the live `WorkspaceCaptureSink`
/// stream is the next source (outlier B — expansion, not remediation).
#[derive(Debug, Clone)]
pub struct ExperienceRecord {
    /// The stimulus + its ground truth. For an eval-sourced episode this IS the
    /// `EvalTask` — it carries the `test`, so a synthesizer can re-pose the task
    /// and objectively validate the correction it teaches.
    pub task: EvalTask,
    /// Did she succeed? The grade's verdict — the primary salience signal.
    pub ok: bool,
    /// The human-readable grade: the real compiler/test error on failure (the
    /// exact weakness to remediate), or "tests passed" / "substring match".
    pub grade: String,
    /// What she actually SAID once settled — UNTRUNCATED. The report truncates to
    /// 200 chars; the teacher needs the whole answer to see the shape of the miss.
    pub answer: String,
    /// The final act→observe world-state (each action's observation folded in) —
    /// the shape of how she got there, for forensics + expansion synthesis.
    pub world_state: String,
    /// How many times she acted before settling — the effort proxy (near the
    /// budget = struggle). Surfaced today via `SettleOutcome.acts`.
    pub acts: u32,
}

impl ExperienceRecord {
    /// Capture a lived eval episode in full, at the grading site — BEFORE the lean
    /// `EvalTaskResult` truncates the answer and drops the trajectory. `ok`/`grade`
    /// are the grader's verdict; `settled` carries her lived trajectory.
    pub fn from_eval(task: &EvalTask, settled: &SettleOutcome, ok: bool, grade: &str) -> Self {
        Self {
            task: task.clone(),
            ok,
            grade: grade.to_string(),
            answer: settled.spoken.clone().unwrap_or_default(),
            world_state: settled.world_state.clone(),
            acts: settled.acts as u32,
        }
    }
}

/// The selector in front of the teacher: decides whether a lived episode is
/// salient enough to become curriculum, and if so, why. Polymorphic (OpenCV-style)
/// so the honest first detector and later composite/attention detectors slot into
/// one seam — the disposition of *what is worth learning from* is a thing we grow,
/// never a hardcoded `if`.
pub trait SalienceDetector: Send + Sync {
    /// A short, stable name (for logging / provenance).
    fn name(&self) -> &'static str;
    /// `Some(signal)` when the episode is worth learning from; `None` when it is
    /// not (the mundane that washes out). The signal names the weakness so the
    /// synthesizer can target it.
    fn assess(&self, episode: &ExperienceRecord) -> Option<SalienceSignal>;
}

/// The honest first detector: an episode is salient iff she got it WRONG, and the
/// weakness is the grader's verdict (the real compiler/test error) — precisely the
/// thing `genome/teach` reads back to remediate. Keys on the one proxy that is
/// instrumented end-to-end today; a passed episode is not salient (we consolidate
/// near where she fails, not a random rerun — the doc's fitness-gap discipline).
pub struct ErrorSalience;

impl SalienceDetector for ErrorSalience {
    fn name(&self) -> &'static str {
        "error"
    }

    fn assess(&self, episode: &ExperienceRecord) -> Option<SalienceSignal> {
        if episode.ok {
            return None;
        }
        Some(SalienceSignal {
            score: 1.0,
            kind: SalienceKind::Error,
            weakness: episode.grade.clone(),
        })
    }
}

/// The afferent → efferent connection: filter a batch of lived episodes to the
/// salient ones (via `detector`) and project them onto the **remediation teach
/// set** — the `EvalTask`s a test-validated synthesizer (`genome/teach`) can teach
/// corrections for. Only test-graded episodes qualify for remediation: without a
/// `test`, the corrected trajectory cannot be objectively validated, and an
/// unvalidated "lesson" is exactly the confident-garbage failure the measurement
/// spine forbids. (A salient episode WITHOUT a test is still salient — it is the
/// input to the *expansion* synthesizer, outlier B, not this remediation path.)
pub fn salient_teach_set(
    records: &[ExperienceRecord],
    detector: &dyn SalienceDetector,
) -> Vec<EvalTask> {
    records
        .iter()
        .filter(|r| detector.assess(r).is_some())
        .filter(|r| r.task.test.is_some())
        .map(|r| r.task.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::act_observe::SettleOutcome;
    use crate::cognition::workspace::{Decision, TurnMetrics};

    /// Build a SettleOutcome standing in for a lived trajectory — a settled Speak
    /// with an answer longer than the report's 200-char cap, so we can prove the
    /// record keeps it whole.
    fn settled_speaking(answer: &str, acts: usize, world_state: &str) -> SettleOutcome {
        SettleOutcome {
            decision: Decision::Speak {
                text: answer.to_string(),
            },
            spoken: Some(answer.to_string()),
            acts,
            world_state: world_state.to_string(),
            metrics: TurnMetrics::default(),
            inference_error: None,
        }
    }

    fn eval_task(id: &str, with_test: bool) -> EvalTask {
        EvalTask {
            id: id.to_string(),
            prompt: "write a function that reverses a string".to_string(),
            expect: String::new(),
            test: with_test.then(|| "assert_eq!(rev(\"ab\"), \"ba\");".to_string()),
            lang: Some("rust".to_string()),
            dod_shell: None,
            solution_file: None,
            setup_shell: None,
        }
    }

    // What this catches: ErrorSalience is the HONEST selector — it fires on a
    // failed episode (naming the grade as the weakness to remediate) and stays
    // silent on a passed one, so consolidation targets the fitness gap and never
    // wastes a class on what she already knows. Guards the doc's "we consolidate
    // near where she fails" discipline. See [[attention-salience-selects-what-becomes-curriculum]].
    #[test]
    fn error_salience_fires_on_failure_and_ignores_success() {
        let detector = ErrorSalience;

        let failed = ExperienceRecord {
            task: eval_task("rev-1", true),
            ok: false,
            grade: "error[E0308]: mismatched types — expected String, found &str".to_string(),
            answer: "fn rev(s: &str) -> &str { s }".to_string(),
            world_state: String::new(),
            acts: 3,
        };
        let signal = detector
            .assess(&failed)
            .expect("a failed episode must be salient");
        assert_eq!(signal.kind, SalienceKind::Error);
        assert!(
            signal.weakness.contains("E0308"),
            "the weakness must carry the REAL grader error (what genome/teach reads back), got: {}",
            signal.weakness
        );

        let passed = ExperienceRecord {
            ok: true,
            grade: "tests passed".to_string(),
            ..failed.clone()
        };
        assert!(
            detector.assess(&passed).is_none(),
            "a passed episode is not salient — don't spend a class on what she already knows"
        );
    }

    // What this catches: from_eval retains the FULL trajectory the teacher needs —
    // the untruncated answer, the world-state, and the effort count — precisely the
    // rich episode the lean EvalTaskResult (200-char answer, no trajectory) throws
    // away. This is the anti-truncation guarantee the whole synthesis path depends
    // on (you cannot weave a curriculum from a 200-char stub).
    #[test]
    fn from_eval_retains_the_untruncated_episode() {
        let long_answer = "x".repeat(500); // > the 200-char report cap
        let settled = settled_speaking(&long_answer, 4, "ran code/run → thread 'main' panicked");
        let task = eval_task("rev-2", true);

        let record = ExperienceRecord::from_eval(&task, &settled, false, "assertion failed");

        assert_eq!(
            record.answer.len(),
            500,
            "the answer must survive whole — no 200-char truncation on the learning path"
        );
        assert_eq!(record.acts, 4, "the effort/struggle count is retained");
        assert!(
            record.world_state.contains("panicked"),
            "the act→observe world-state is retained for forensics + expansion"
        );
        assert!(!record.ok);
    }

    // What this catches: the afferent→efferent nerve projects ONLY salient,
    // test-graded episodes onto the remediation teach set — a passed task is
    // dropped (not a gap), and a failed-but-untestable task is dropped from
    // REMEDIATION (its correction couldn't be validated — that path is expansion,
    // not genome/teach). This is the exact filter that turns "she failed these"
    // into "teach corrections for these".
    #[test]
    fn salient_teach_set_keeps_only_failed_and_testable() {
        let failed_testable = ExperienceRecord {
            task: eval_task("keep-me", true),
            ok: false,
            grade: "no match".to_string(),
            answer: String::new(),
            world_state: String::new(),
            acts: 8,
        };
        let passed_testable = ExperienceRecord {
            task: eval_task("passed", true),
            ok: true,
            grade: "tests passed".to_string(),
            answer: "ok".to_string(),
            world_state: String::new(),
            acts: 1,
        };
        let failed_untestable = ExperienceRecord {
            task: eval_task("no-test", false),
            ok: false,
            grade: "no match".to_string(),
            answer: String::new(),
            world_state: String::new(),
            acts: 2,
        };

        let set = salient_teach_set(
            &[failed_testable, passed_testable, failed_untestable],
            &ErrorSalience,
        );

        assert_eq!(
            set.len(),
            1,
            "only the failed, test-graded task feeds remediation"
        );
        assert_eq!(set[0].id, "keep-me");
    }
}
