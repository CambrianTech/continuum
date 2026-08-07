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
use crate::cognition::workspace::Decision;
use crate::memory::types::MemoryRecord;
use serde::{Deserialize, Serialize};

/// WHICH axis of the being's one experience an episode came from. The unification
/// made literal: a lived chat turn, a graded exam, and a lesson another agent
/// handed over are the SAME being's experience in different contexts — one stream,
/// tagged by origin, not three separate pipelines. Detectors key on this to select
/// what becomes curriculum. (See [[lived-and-eval-experience-are-one-stream-one-being]].)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperienceSource {
    /// A graded exam episode — the eval-curriculum flywheel ([`ExperienceRecord::from_eval`]).
    /// Carries a ground-truth `test`; feeds objective remediation.
    Eval,
    /// A real room turn she took — lived experience ([`ExperienceRecord::from_lived_turn`]).
    /// No grader; salience is convergence/fault; feeds expansion.
    Lived,
    /// A lesson another agent handed INTO her memory ([`ExperienceRecord::from_shared_lesson`]).
    /// Received, not lived — BigMama's `memory/share` (#2025). Provenance IS the salience:
    /// someone deliberately chose to teach it, so it is inherently worth integrating.
    Received,
}

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
    /// Another agent deliberately handed this lesson over ([`ExperienceSource::Received`]).
    /// Provenance IS the signal — no proxy needed, someone CHOSE to teach it. This is
    /// honest, not faked: the salience is the deliberate act of sharing, fully instrumented
    /// by `memory/share`'s `shared_by` provenance (#2025).
    Received,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// WHICH axis of the one experience this came from — lived, eval, or received.
    /// The unification made self-describing: detectors key on this (e.g. `ReceivedSalience`
    /// selects `Received`), and it makes lived-vs-eval explicit instead of implicit in the
    /// presence of a `test`. One being, one stream, tagged by origin.
    pub source: ExperienceSource,
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
            source: ExperienceSource::Eval,
        }
    }

    /// Capture a LIVED turn — a real room turn, not an exam — into the SAME
    /// experience stream as [`from_eval`]. This is the unification: a lived chat
    /// turn and an exam task are the SAME being acting in different contexts, so
    /// they must not be two separate learning paths (the eval-curriculum flywheel
    /// here, the recorder→dataset path there). One stream, one being.
    /// (See [[lived-and-eval-experience-are-one-stream-one-being]] — eval may be a
    /// different ENVIRONMENT, never a different BEING.)
    ///
    /// The lived-vs-eval difference is honestly encoded in the DATA, not a fork in
    /// the machinery:
    /// - **No ground-truth `test`.** A lived turn has no objective grader, so the
    ///   synthetic [`EvalTask`] carries only the `stimulus` as its `prompt` and
    ///   leaves `test`/`expect` empty. [`salient_teach_set`] already gates
    ///   remediation on `test.is_some()`, so a lived episode can NEVER become a
    ///   remediation teach task from a grade it never had — it flows to the
    ///   *expansion* synthesizer (outlier B). The objective-remediation loop stays
    ///   uncorrupted; the being still grows from lived experience.
    /// - **`ok` means "settled CLEANLY", not "correct".** With no objective grade,
    ///   the honest, end-to-end-instrumented lived verdict is whether she *converged*
    ///   — reached a real settle-verdict (`Speak`/`Pass`/`RaiseUnprompted`) without
    ///   an inference/serving fault. Two magic-number-free failure signals make a
    ///   lived turn salient: an infra fault (`inference_error`), and NON-CONVERGENCE
    ///   — the settle loop ran out its action budget mid-`Act` and never reached a
    ///   verdict (`SettleOutcome.decision` is still `Act`, the "did not finish" the
    ///   eval grader already recognizes). Both are structural facts of the outcome,
    ///   not thresholds — no `acts >= 6` constant to drift ([[audit-for-clamps]]).
    ///   `ErrorSalience` then selects these lived episodes with NO new detector: the
    ///   data honestly carries the gap, one detector reads it (compression). Richer
    ///   proxies (REPETITION via the spin brick, graded lived quality) are the named
    ///   frontier — real signals, not faked here.
    pub fn from_lived_turn(stimulus: &str, settled: &SettleOutcome) -> Self {
        // Converged = she reached a real verdict. A final `Act` means the drive-loop
        // hit its budget mid-action and never settled — the lived analog of the
        // eval's "did not finish", and an honest capability signal with no threshold.
        let converged = !matches!(settled.decision, Decision::Act { .. });
        let clean = converged && settled.inference_error.is_none();
        Self {
            // The stimulus IS the task for a lived turn: the message she answered,
            // posed as the prompt. No `test`/`expect` — nothing objective to grade
            // against — so this record is expansion input, never remediation.
            task: EvalTask {
                prompt: stimulus.to_string(),
                ..EvalTask::default()
            },
            // Settled cleanly = ok; infra fault OR non-convergence = not ok. NOT a
            // correctness claim — a lived turn has no grader (see doc above).
            ok: clean,
            grade: match (&settled.inference_error, converged) {
                (Some(cause), _) => format!("lived turn: inference fault — {cause}"),
                (None, false) => {
                    "lived turn: did not converge (action budget exhausted mid-act)".to_string()
                }
                (None, true) => "lived turn: settled".to_string(),
            },
            answer: settled.spoken.clone().unwrap_or_default(),
            world_state: settled.world_state.clone(),
            acts: settled.acts as u32,
            source: ExperienceSource::Lived,
        }
    }

    /// Capture an objectively graded KANBAN artifact — a citizen wrote a file
    /// through the work loop and `benchmark/grade` / `benchmark/swe-grade` judged
    /// it against the held-out check (#319: restore learning on benchmark paths).
    ///
    /// Lesson-not-paper: the record retains HER artifact (`answer`) and the REAL
    /// check output (`grade` — the compiler/assertion text is the lesson). No
    /// gold answer exists at this site to leak; the task's held-out `test` rides
    /// along only so the remediation loop can re-pose and objectively re-grade
    /// (the same role it plays for [`Self::from_eval`] records).
    ///
    /// No trajectory exists at the grading site — grading is stateless over the
    /// artifact — so `world_state` is empty and `acts` is 0: honest absence,
    /// never a fabricated trace.
    pub fn from_kanban_grade(task: &EvalTask, artifact: &str, pass: bool, detail: &str) -> Self {
        Self {
            task: task.clone(),
            ok: pass,
            grade: detail.to_string(),
            answer: artifact.to_string(),
            world_state: String::new(),
            acts: 0,
            source: ExperienceSource::Eval,
        }
    }

    /// Lift a RECEIVED lesson — one another agent handed into her memory via
    /// `memory/share` (#2025) — into the SAME experience stream as lived and eval.
    /// The THIRD axis: the being learns from everything it DOES (lived + eval) AND
    /// everything it's TOLD (received). One stream, one being — BigMama's producer
    /// lane feeding my consumer lane, the advanced-intelligence convergence.
    ///
    /// A shared lesson is a [`MemoryRecord`] with `memory_type == "shared"` and
    /// `source == "shared:<from>"`; `context.shared_by` names the teacher. It has
    /// NO ground-truth `test` — nothing to objectively grade a correction against —
    /// so by the SAME `test.is_some()` gate as a lived turn, it flows to *expansion*,
    /// never grade-driven remediation. The objective benchmark loop stays uncorrupted.
    ///
    /// `ok` is `true`: a received lesson is knowledge to integrate, not a failure. Its
    /// salience is NOT `ErrorSalience` (it isn't a gap) — it is [`ReceivedSalience`],
    /// where the deliberate act of sharing IS the signal (provenance, not a proxy).
    /// The lesson `content` becomes the teaching material; `shared_by` is retained in
    /// the grade + world-state so the curriculum knows it was received, not derived.
    pub fn from_shared_lesson(record: &MemoryRecord) -> Self {
        let shared_by = record
            .context
            .get("shared_by")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        // The scope (the room/project the lesson pertains to) frames the lesson as its
        // "topic"; there is no stimulus (nothing was asked) — the content IS the lesson.
        let scope = record
            .context
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        Self {
            task: EvalTask {
                prompt: scope.to_string(),
                ..EvalTask::default()
            },
            // Received knowledge, not a graded outcome — never a failure signal.
            ok: true,
            grade: format!("received lesson from {shared_by}"),
            // The lesson itself is the teaching material the synthesizer integrates.
            answer: record.content.clone(),
            world_state: format!("received via memory/share from {shared_by}"),
            acts: 0,
            source: ExperienceSource::Received,
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

/// The detector for the THIRD axis: a lesson another agent deliberately handed over
/// is salient because it was SHARED, not because she failed. Provenance IS the signal
/// — no proxy, no threshold, no faked number. It fires iff the episode's
/// [`ExperienceSource`] is `Received` (BigMama's `memory/share` #2025 set the
/// `shared_by` marker that `from_shared_lesson` carried into the record). The
/// "weakness" it names is the lesson content — the teaching material to integrate,
/// not a gap to remediate. Complementary to [`ErrorSalience`], not a replacement: the
/// composite that runs both selects lived+eval failures AND received lessons in one
/// pass — the whole being's curriculum from all three axes.
pub struct ReceivedSalience;

impl SalienceDetector for ReceivedSalience {
    fn name(&self) -> &'static str {
        "received"
    }

    fn assess(&self, episode: &ExperienceRecord) -> Option<SalienceSignal> {
        if episode.source != ExperienceSource::Received {
            return None;
        }
        Some(SalienceSignal {
            score: 1.0,
            kind: SalienceKind::Received,
            // The lesson itself is what to learn — the synthesizer integrates it as
            // received teaching material (the grade names who taught it).
            weakness: episode.answer.clone(),
        })
    }
}

/// The composite selector: salient if ANY constituent detector fires — the whole
/// being's curriculum from all three axes in ONE pass. [`all_axes`](Self::all_axes)
/// composes [`ErrorSalience`] (lived + eval failures) with [`ReceivedSalience`]
/// (taught lessons), so a single sweep over the unified experience stream selects
/// everything worth learning from, whatever its origin. This is the seam the dream
/// consolidation reads: gather the stream, run one detector, get the full teach set.
/// Polymorphic and open — a later `StruggleSalience` / `SurpriseSalience` joins the
/// composite without touching any caller (the detector list is the extension point).
pub struct AnySalience {
    detectors: Vec<Box<dyn SalienceDetector>>,
}

impl AnySalience {
    /// The full being: failures (lived + eval) AND received lessons. Order is priority —
    /// the first detector to fire names the salience kind, so `ErrorSalience` (the gap)
    /// wins over `ReceivedSalience` on the rare record that is both.
    pub fn all_axes() -> Self {
        Self {
            detectors: vec![Box::new(ErrorSalience), Box::new(ReceivedSalience)],
        }
    }

    /// Compose an explicit set (tests, future tuning). Empty = never salient.
    pub fn of(detectors: Vec<Box<dyn SalienceDetector>>) -> Self {
        Self { detectors }
    }
}

impl SalienceDetector for AnySalience {
    fn name(&self) -> &'static str {
        "any"
    }

    fn assess(&self, episode: &ExperienceRecord) -> Option<SalienceSignal> {
        // First firing detector wins — priority order, no blending. A composite that
        // maxes/blends scores across kinds is the named next step; ANY-fires is the
        // honest first cut (a record is salient iff some axis says so).
        self.detectors.iter().find_map(|d| d.assess(episode))
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

/// The COMPLEMENT of [`salient_teach_set`]: the salient episodes that cannot be
/// objectively remediated (no `test`) — the input to the **expansion** synthesizer
/// (outlier B, the frontier the `CurriculumSynthesizer` doc deliberately deferred).
/// Lived turns and received lessons land here: a detector flagged them as worth
/// learning from, but they carry no ground-truth grader.
///
/// Why this is now SAFE to build (it wasn't before): the expansion path was deferred
/// because "an unvalidated lesson is the confident-garbage the measurement spine
/// forbids" — a per-*trajectory* validation was missing. Joel's spine supplies a
/// different, sufficient one: the lesson is validated NOT per-trajectory but
/// per-**consolidation**, by whole-being **benchmark lift** (the humane-snapshot gate
/// #59, page-in only if score ≥ prior). Untestable material can drive training because
/// the gate measures the WHOLE being afterward and rejects anything that didn't lift.
/// Keeping remediation and expansion as two honest partitions of ONE salient stream is
/// exactly the routing the dream consolidation needs: `salient_teach_set` → the
/// test-validated remediation teacher; `expansion_teach_set` → the benchmark-validated
/// expansion teacher. Same stream, same detector, two efferent organs.
pub fn expansion_teach_set(
    records: &[ExperienceRecord],
    detector: &dyn SalienceDetector,
) -> Vec<ExperienceRecord> {
    records
        .iter()
        .filter(|r| detector.assess(r).is_some())
        .filter(|r| r.task.test.is_none())
        .cloned()
        .collect()
}

/// The persona's durable experience stream: `<citizen peer dir>/experience.jsonl`,
/// one [`ExperienceRecord`] per line, append-only. This is the SPINE the
/// salience→curriculum seam was missing (#319): producers (`benchmark/grade`,
/// `benchmark/swe-grade`, the lived-turn settle site) append here; the curriculum
/// drain loads from here. It lives INSIDE the citizen's peer dir, which is
/// already an accounted disk class (`system_resources::disk_reporters::
/// standard_tracked_dirs` → "citizens"), so it inherits that class's tracking +
/// eviction story rather than minting a new unbounded cache dir.
pub fn experience_stream_path(peer_dir: &std::path::Path) -> std::path::PathBuf {
    peer_dir.join("experience.jsonl")
}

/// Append one record to the persona's durable experience stream. Errors surface
/// to the caller — whether a failed append fails the whole operation is the
/// CALLER's contract (a grade verb keeps its verdict primary and logs loud; a
/// dedicated capture path may choose to hard-fail).
pub fn append_experience(
    peer_dir: &std::path::Path,
    record: &ExperienceRecord,
) -> std::io::Result<()> {
    use std::io::Write;
    let line = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(experience_stream_path(peer_dir))?;
    writeln!(f, "{line}")
}

/// Load the persona's experience stream. A missing file is an empty stream (a
/// fresh mind has no history — not an error). Unparseable lines are counted and
/// WARNED, never silently dropped: one corrupt line must not brick learning
/// forever, but the loss is visible in the log, not swallowed.
pub fn load_experiences(peer_dir: &std::path::Path) -> Vec<ExperienceRecord> {
    let path = experience_stream_path(peer_dir);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut bad = 0usize;
    let records: Vec<ExperienceRecord> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| match serde_json::from_str(l) {
            Ok(r) => Some(r),
            Err(_) => {
                bad += 1;
                None
            }
        })
        .collect();
    if bad > 0 {
        tracing::warn!(
            path = %path.display(),
            unparseable = bad,
            loaded = records.len(),
            "experience stream carries unparseable lines — records lost to a \
             schema drift or partial write (visible loss, not silent)"
        );
    }
    records
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
            workspace_root: None,
            ui_checks: Vec::new(),
            target: None,
            ui_pass_threshold: None,
            needs_tools: None,
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
            source: ExperienceSource::Eval,
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
            source: ExperienceSource::Eval,
        };
        let passed_testable = ExperienceRecord {
            task: eval_task("passed", true),
            ok: true,
            grade: "tests passed".to_string(),
            answer: "ok".to_string(),
            world_state: String::new(),
            acts: 1,
            source: ExperienceSource::Eval,
        };
        let failed_untestable = ExperienceRecord {
            task: eval_task("no-test", false),
            ok: false,
            grade: "no match".to_string(),
            answer: String::new(),
            world_state: String::new(),
            acts: 2,
            source: ExperienceSource::Eval,
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

    // What this catches: the UNIFICATION — a lived turn enters the SAME experience
    // stream as an eval episode (untruncated answer, world-state, effort all
    // retained, one being), yet because it carries NO ground-truth `test`, it can
    // never leak into the objective-remediation set. A cleanly-settled lived turn
    // is ok=true; an infra-faulted one is ok=false — but NEITHER becomes a
    // remediation teach task (no test to validate a correction against). Lived
    // experience grows the being via the expansion path, never by corrupting the
    // grade-driven loop. Regression for the "one being, one stream" spine
    // ([[lived-and-eval-experience-are-one-stream-one-being]]).
    #[test]
    fn from_lived_turn_joins_the_stream_but_never_the_remediation_set() {
        let long_answer = "y".repeat(500); // same anti-truncation guarantee as eval
        let settled = settled_speaking(&long_answer, 5, "answered in #general after code/search");

        let lived =
            ExperienceRecord::from_lived_turn("what does build_workspace_cycle do?", &settled);

        // Same rich record shape as from_eval — the being is not stripped in either context.
        assert_eq!(
            lived.answer.len(),
            500,
            "lived answer survives whole, like eval"
        );
        assert_eq!(lived.acts, 5, "lived effort is retained");
        assert!(
            lived.world_state.contains("code/search"),
            "lived trajectory retained"
        );
        assert!(
            lived.ok,
            "a cleanly-settled lived turn is ok (no infra fault) — NOT a correctness claim"
        );
        assert_eq!(lived.task.prompt, "what does build_workspace_cycle do?");
        assert!(
            lived.task.test.is_none(),
            "a lived turn has no objective grader"
        );

        // A lived turn that died on a serving fault: ok=false, honest grade — but STILL untestable.
        let faulted = SettleOutcome::infra_failure("lane 58057 refused qwen3");
        let lived_fault = ExperienceRecord::from_lived_turn("ping", &faulted);
        assert!(
            !lived_fault.ok,
            "an inference-faulted lived turn is not ok (same honesty as eval)"
        );
        assert!(
            lived_fault.grade.contains("inference fault"),
            "the fault cause is named in the grade"
        );

        // NON-CONVERGENCE: the drive-loop ran out its action budget mid-Act and never
        // reached a verdict — the lived analog of the eval's "did not finish". A
        // structural fact of the outcome (decision is still Act), not a threshold —
        // ok=false with NO magic-number struggle constant. ErrorSalience selects it.
        let unconverged = SettleOutcome {
            decision: Decision::Act {
                calls: Vec::new(),
                intent: "re-run the failing test".into(),
            },
            spoken: None,
            acts: 8,
            world_state: "budget exhausted after 8 acts".into(),
            metrics: TurnMetrics::default(),
            inference_error: None,
        };
        let lived_stuck = ExperienceRecord::from_lived_turn("fix the build", &unconverged);
        assert!(
            !lived_stuck.ok,
            "a lived turn that never converged is not ok — the honest struggle signal"
        );
        assert!(
            lived_stuck.grade.contains("did not converge"),
            "non-convergence is named, not faked as a threshold"
        );
        assert!(
            ErrorSalience.assess(&lived_stuck).is_some(),
            "ErrorSalience selects the non-converged lived turn with NO new detector — one detector, honest data"
        );

        // THE SAFETY GUARANTEE: none of the salient lived records can reach remediation
        // curriculum, because salient_teach_set gates on test.is_some() and a lived turn
        // has none. Salient lived experience feeds EXPANSION only.
        let set = salient_teach_set(&[lived, lived_fault, lived_stuck], &ErrorSalience);
        assert!(
            set.is_empty(),
            "lived turns feed EXPANSION, never grade-driven remediation — the objective loop stays uncorrupted"
        );
    }

    /// A shared lesson as BigMama's `memory/share` (#2025) builds it: a MemoryRecord
    /// with `memory_type: "shared"`, `context.shared_by`, `source: "shared:<from>"`.
    fn shared_lesson(from: &str, scope: &str, content: &str) -> MemoryRecord {
        MemoryRecord {
            id: "lesson-1".to_string(),
            persona_id: "recipient".to_string(),
            memory_type: "shared".to_string(),
            content: content.to_string(),
            context: serde_json::json!({ "shared_by": from, "scope": scope }),
            timestamp: "2026-07-26T00:00:00Z".to_string(),
            importance: 0.7,
            access_count: 0,
            tags: vec![scope.to_string(), format!("shared-from:{from}")],
            related_to: Vec::new(),
            source: Some(format!("shared:{from}")),
            last_accessed_at: None,
            layer: None,
            relevance_score: None,
            origin_node: None,
            origin_seq: None,
        }
    }

    // What this catches: THE THIRD AXIS. A lesson another agent hands over (BigMama's
    // memory/share #2025) enters the SAME experience stream as lived + eval — one
    // being that learns from everything it DOES and everything it's TOLD. Its salience
    // is NOT ErrorSalience (a received lesson is knowledge, ok=true, not a gap) but
    // ReceivedSalience, where provenance IS the signal (no proxy, no threshold). And by
    // the SAME test.is_some() gate as a lived turn, a received lesson has no ground-truth
    // test → it feeds expansion, never grade-driven remediation. Producer lane (share)
    // meets consumer lane (this) — the convergence.
    #[test]
    fn from_shared_lesson_is_the_third_axis_received_salience_never_remediation() {
        let record = shared_lesson(
            "BigMama",
            "continuum",
            "the call room IS the airc room — never mint a rogue call_id",
        );
        let lesson = ExperienceRecord::from_shared_lesson(&record);

        // Same stream, tagged Received — the being learns from what it's told, not just what it does.
        assert_eq!(lesson.source, ExperienceSource::Received);
        assert!(
            lesson.ok,
            "a received lesson is knowledge to integrate, not a failure"
        );
        assert_eq!(
            lesson.answer, record.content,
            "the lesson content is the teaching material"
        );
        assert!(
            lesson.grade.contains("BigMama"),
            "the teacher is named — received, not derived"
        );
        assert!(
            lesson.task.test.is_none(),
            "a received lesson has no ground-truth test"
        );

        // ReceivedSalience selects it (provenance IS the signal); ErrorSalience does NOT
        // (it isn't a gap — ok=true). Two complementary detectors, one stream.
        let recv = ReceivedSalience
            .assess(&lesson)
            .expect("a shared lesson is salient by provenance");
        assert_eq!(recv.kind, SalienceKind::Received);
        assert!(
            ErrorSalience.assess(&lesson).is_none(),
            "a received lesson is not an error/gap"
        );

        // A lived/eval failure is NOT received — ReceivedSalience must stay silent on it
        // (the axis tag is honest, not a catch-all).
        let failed_eval = ExperienceRecord {
            task: eval_task("rev-x", true),
            ok: false,
            grade: "no match".to_string(),
            answer: String::new(),
            world_state: String::new(),
            acts: 2,
            source: ExperienceSource::Eval,
        };
        assert!(
            ReceivedSalience.assess(&failed_eval).is_none(),
            "ReceivedSalience fires ONLY on received provenance"
        );

        // SAFETY GUARANTEE holds for the third axis too: no test → never remediation.
        let set = salient_teach_set(&[lesson], &ReceivedSalience);
        assert!(
            set.is_empty(),
            "received lessons feed EXPANSION, never grade-driven remediation"
        );
    }

    // What this catches: the composite selects the whole being's curriculum from all
    // three axes in ONE pass — an eval failure (Error), a received lesson (Received),
    // and it drops what nobody flags (a passed eval). This is the single detector the
    // dream consolidation runs over the unified stream: gather lived+eval+received,
    // sweep once, get everything worth learning from. Priority order means Error wins
    // the naming on a record that is both a gap and received.
    #[test]
    fn any_salience_selects_the_whole_being_curriculum_in_one_pass() {
        let detector = AnySalience::all_axes();

        let eval_failure = ExperienceRecord {
            task: eval_task("rev-9", true),
            ok: false,
            grade: "error[E0308]".to_string(),
            answer: String::new(),
            world_state: String::new(),
            acts: 3,
            source: ExperienceSource::Eval,
        };
        let received = ExperienceRecord::from_shared_lesson(&shared_lesson(
            "BigMama",
            "continuum",
            "provenance IS the salience signal",
        ));
        let passed_eval = ExperienceRecord {
            ok: true,
            grade: "tests passed".to_string(),
            ..eval_failure.clone()
        };

        // Error wins the kind (priority), Received fires for the lesson, pass is dropped.
        assert_eq!(
            detector.assess(&eval_failure).unwrap().kind,
            SalienceKind::Error
        );
        assert_eq!(
            detector.assess(&received).unwrap().kind,
            SalienceKind::Received
        );
        assert!(
            detector.assess(&passed_eval).is_none(),
            "a passed eval is nobody's gap and nobody's lesson"
        );

        // Empty composite is never salient (honest degenerate case).
        assert!(AnySalience::of(vec![]).assess(&eval_failure).is_none());
    }

    // What this catches: the DREAM'S ROUTING — one salient sweep of the unified stream
    // splits cleanly into two efferent organs. A testable eval failure → remediation
    // (test-validated teacher); an untestable lived turn AND an untestable received
    // lesson → expansion (benchmark-validated teacher); a passed eval → neither. The two
    // partitions are complementary and exhaustive over the salient set — no salient
    // episode is dropped, none is double-routed. This is the routing that lets the dream
    // consolidate the whole being's experience without corrupting the graded loop.
    #[test]
    fn remediation_and_expansion_partition_the_salient_stream_exhaustively() {
        let any = AnySalience::all_axes();

        let testable_failure = ExperienceRecord {
            task: eval_task("exam-fail", true),
            ok: false,
            grade: "error[E0308]".to_string(),
            answer: String::new(),
            world_state: String::new(),
            acts: 4,
            source: ExperienceSource::Eval,
        };
        let stuck = SettleOutcome {
            decision: Decision::Act {
                calls: Vec::new(),
                intent: "keep trying".into(),
            },
            spoken: None,
            acts: 8,
            world_state: String::new(),
            metrics: TurnMetrics::default(),
            inference_error: None,
        };
        let lived = ExperienceRecord::from_lived_turn("a hard live question", &stuck);
        let received = ExperienceRecord::from_shared_lesson(&shared_lesson(
            "BigMama",
            "continuum",
            "the being learns from what it's told",
        ));
        let passed = ExperienceRecord {
            ok: true,
            grade: "tests passed".to_string(),
            ..testable_failure.clone()
        };

        let batch = [testable_failure, lived, received, passed];

        // Remediation: ONLY the testable failure (objective grader can validate the fix).
        let remediation = salient_teach_set(&batch, &any);
        assert_eq!(
            remediation.len(),
            1,
            "only the testable failure is remediation-eligible"
        );
        assert_eq!(remediation[0].id, "exam-fail");

        // Expansion: the untestable-but-salient lived + received (benchmark-validated).
        let expansion = expansion_teach_set(&batch, &any);
        assert_eq!(
            expansion.len(),
            2,
            "the lived turn and the received lesson are expansion-eligible"
        );
        assert!(expansion
            .iter()
            .any(|r| r.source == ExperienceSource::Lived));
        assert!(expansion
            .iter()
            .any(|r| r.source == ExperienceSource::Received));

        // Exhaustive + disjoint over the salient set: 1 + 2 = 3 salient, the pass in neither.
        assert_eq!(
            remediation.len() + expansion.len(),
            3,
            "every salient episode is routed exactly once; the pass is dropped"
        );
    }

    mod durable_stream {
        use super::*;

        // what this catches: the #319 spine — a kanban-graded outcome appended to
        // the per-persona stream must survive a write→load roundtrip INTACT (her
        // artifact, the real check output, the verdict, the source tag), and a
        // corrupt line must cost exactly itself, never the rest of the stream.
        #[test]
        fn append_load_roundtrip_survives_a_corrupt_line() {
            let dir = std::env::temp_dir().join(format!("exp-stream-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();

            let task = EvalTask {
                id: "sum_evens".into(),
                prompt: "Write fn sum_evens…".into(),
                test: Some("assert_eq!(sum_evens(&[2,3]), 2);".into()),
                ..EvalTask::default()
            };
            let fail = ExperienceRecord::from_kanban_grade(
                &task,
                "fn sum_evens(n:&[i32])->i32{0}",
                false,
                "assertion failed: left 0, right 2",
            );
            let pass =
                ExperienceRecord::from_kanban_grade(&task, "fn sum_evens…correct", true, "ALL ASSERTIONS PASSED");
            append_experience(&dir, &fail).unwrap();
            // A partial write / schema drift in the middle of the stream:
            {
                use std::io::Write;
                let mut f = std::fs::OpenOptions::new()
                    .append(true)
                    .open(experience_stream_path(&dir))
                    .unwrap();
                writeln!(f, "{{\"not\": \"an experience record\"").unwrap();
            }
            append_experience(&dir, &pass).unwrap();

            let loaded = load_experiences(&dir);
            assert_eq!(loaded.len(), 2, "both real records load; the corrupt line costs only itself");
            assert!(!loaded[0].ok && loaded[0].grade.contains("assertion failed"));
            assert_eq!(loaded[0].answer, "fn sum_evens(n:&[i32])->i32{0}");
            assert!(matches!(loaded[0].source, ExperienceSource::Eval));
            assert!(loaded[1].ok);
            let _ = std::fs::remove_dir_all(&dir);
        }

        // what this catches: a kanban-grade record must be REMEDIATION-ELIGIBLE —
        // its task keeps the held-out test (so the teacher can re-pose + objectively
        // re-grade), and a FAILED grade is selected by the error detector while a
        // PASS is not. This is the exact contract that lets graded board work feed
        // the existing salience→curriculum seam with zero changes to it.
        #[test]
        fn kanban_grade_failure_enters_the_teach_set() {
            let task = EvalTask {
                id: "fib".into(),
                prompt: "Write fn fib…".into(),
                test: Some("assert_eq!(fib(10), 55);".into()),
                ..EvalTask::default()
            };
            let fail = ExperienceRecord::from_kanban_grade(&task, "fn fib(n:u32)->u64{n as u64}", false, "left 10, right 55");
            let pass = ExperienceRecord::from_kanban_grade(&task, "fn fib…", true, "ALL ASSERTIONS PASSED");
            let teach = salient_teach_set(&[fail, pass], &ErrorSalience);
            assert_eq!(teach.len(), 1, "the failure is remediable; the pass teaches nothing");
            assert_eq!(teach[0].id, "fib");
        }

        // what this catches: a missing stream is an EMPTY stream (a fresh mind),
        // never an error — the drain must not fail a persona who has no history.
        #[test]
        fn missing_stream_is_empty_not_an_error() {
            let dir = std::env::temp_dir().join(format!("exp-none-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            assert!(load_experiences(&dir).is_empty());
        }
    }
}
