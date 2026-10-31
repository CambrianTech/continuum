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

/// Which seat this persona held in a team activity — attribution the
/// curriculum keys on (a reviewer's episode teaches DIFFERENT skills than a
/// solver's, from the same room).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    /// Held the card — the patch is theirs.
    Solver,
    /// Held the review charge — the catch is theirs.
    Reviewer,
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
    /// Source-labelled supplemental inputs that accompanied this episode's answer.
    /// Old records have none. Only the turn's admitted inputs are stored here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub room_updates: Vec<crate::cognition::conversation::IncomingMessage>,
    /// How many times she acted before settling — the effort proxy (near the
    /// budget = struggle). Surfaced today via `SettleOutcome.acts`.
    pub acts: u32,
    /// The ACTIVITY the turn happened in — its room (#425; A6 attribution).
    /// `None` = honestly not carried (pre-A6 rows, folded eval outcomes,
    /// stateless grades, out-of-turn lessons) — absence is typed, never nil.
    #[serde(default)]
    pub room: Option<uuid::Uuid>,
    /// TEAM attribution (the glass-box half of the teams arc, 2026-08-30):
    /// who else held this activity's room when the episode happened. Empty =
    /// solo (or pre-team rows). This is what lets the curriculum select
    /// "turns where a teammate's review changed the outcome" as a training
    /// corpus — collaboration becoming genome starts with rows that KNOW they
    /// were collaborative.
    #[serde(default)]
    pub teammates: Vec<uuid::Uuid>,
    /// This persona's role in the team, when the episode came from a team
    /// activity. `None` = solo/unknown (pre-team rows load unchanged).
    #[serde(default)]
    pub team_role: Option<TeamRole>,
    /// WHICH axis of the one experience this came from — lived, eval, or received.
    /// The unification made self-describing: detectors key on this (e.g. `ReceivedSalience`
    /// selects `Received`), and it makes lived-vs-eval explicit instead of implicit in the
    /// presence of a `test`. One being, one stream, tagged by origin.
    pub source: ExperienceSource,
}

impl ExperienceRecord {
    /// Render the full learning input at the teacher boundary, preserving the
    /// original stored task and each supplemental event's room and sender.
    pub fn training_prompt(&self) -> String {
        let mut prompt = self.task.prompt.clone();
        self.append_room_updates(&mut prompt);
        prompt
    }

    fn append_room_updates(&self, prompt: &mut String) {
        for message in &self.room_updates {
            prompt.push_str("\n\n");
            prompt.push_str(&message.render_room_update());
        }
    }

    /// [`Self::from_eval`]'s sibling for the seam where the [`SettleOutcome`] is
    /// already folded away (learn-mode's lesson transfer runs AFTER settlement,
    /// over the summarized per-task results). Same stream, same being; the
    /// world_state is honestly absent rather than reconstructed.
    pub fn from_eval_result(
        task: &EvalTask,
        result: &crate::cognition::eval::EvalTaskResult,
    ) -> Self {
        Self {
            task: task.clone(),
            ok: result.ok,
            grade: result.grade.clone(),
            answer: result.answer.clone(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: result.acts,
            source: ExperienceSource::Eval,
            // The outcome is already folded away at this seam — the room went with
            // it. Default (nil) here means "not carried", the same absent-marker
            // pre-A6 rows load with; never a live roomless turn.
            room: None,
            teammates: Vec::new(),
            team_role: None,
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
    ///   lived turn salient: an infra fault (`failure`), and NON-CONVERGENCE
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
        let clean =
            converged && settled.failure.is_none() && settled.capacity_refusal.is_none();
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
            grade: match (
                &settled.failure,
                &settled.capacity_refusal,
                converged,
            ) {
                (Some(crate::cognition::act_observe::SettlementFailure::Inference(cause)), _, _) => {
                    format!("lived turn: inference fault — {cause}")
                }
                (Some(crate::cognition::act_observe::SettlementFailure::ReceiptPersistence(cause)), _, _) => {
                    format!("lived turn: receipt persistence fault — {cause}")
                }
                (None, Some(error), _) => format!("lived turn: prompt capacity refused — {error}"),
                (None, None, false) => {
                    "lived turn: did not converge (action budget exhausted mid-act)".to_string()
                }
                (None, None, true) => "lived turn: settled".to_string(),
            },
            answer: settled.spoken.clone().unwrap_or_default(),
            world_state: settled.world_state.clone(),
            room_updates: settled
                .room_updates
                .iter()
                .map(|m| m.as_ref().clone())
                .collect(),
            acts: settled.acts as u32,
            source: ExperienceSource::Lived,
            room: Some(settled.room),
            teammates: Vec::new(),
            team_role: None,
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
            room_updates: Default::default(),
            acts: 0,
            source: ExperienceSource::Eval,
            // Grading is stateless over the artifact — no turn, no room carried.
            room: None,
            teammates: Vec::new(),
            team_role: None,
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
            room_updates: Default::default(),
            acts: 0,
            source: ExperienceSource::Received,
            // A received lesson arrives out-of-turn — no room carried.
            room: None,
            teammates: Vec::new(),
            team_role: None,
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
        .map(|r| {
            let mut task = r.task.clone();
            r.append_room_updates(&mut task.prompt);
            task
        })
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

/// Record ONE lived room turn into the citizen's own experience stream — the
/// producer this module's doc has named since #319 and that nothing ever called.
///
/// ## Why this exists (measured 2026-08-17)
///
/// [`ExperienceRecord::from_lived_turn`] had **zero production callers** tree-wide.
/// The only live producer was [`ExperienceRecord::from_kanban_grade`], so the
/// experience stream — "the SPINE the salience→curriculum seam was missing" per
/// [`experience_stream_path`]'s own doc — was fed exclusively by GRADES. A citizen
/// could hold a thousand real conversations and her stream stayed empty, which
/// means the curriculum could only ever learn from work someone had scored. The
/// machinery, the salience detector and the doc were all correct and in place; the
/// call was missing. (Same shape as #341 and #362: a built component with a dead
/// wire — [[an-absence-is-an-unfinished-measurement]].)
///
/// ## Why the write is best-effort, and what that is NOT
///
/// A failed append is WARNED and the turn proceeds. This is not a silent fallback
/// ([[no-fallbacks-ever]] still holds): nothing is substituted and no result is
/// fabricated. Learning is a SIDE channel to being — a full disk must not make a
/// citizen mute mid-sentence. The failure is visible in the log with the path, and
/// the honest consequence (this episode never becomes curriculum) is stated there.
///
/// Storage is keyed by [`crate::identity::citizen_peer_dir`] — one spelling of the
/// citizen-layout decision, so this producer cannot drift from the consumer that
/// drains the same stream.
///
/// ## Why there is no `stimulus` parameter
///
/// The stimulus is not passed in: it is READ OFF the outcome
/// (`SettleOutcome.world_state`, set on every return path of the settle driver to
/// the burst it actually deliberated over). A `stimulus: &str` argument would be a
/// second, independent expression of "what she was responding to" — free to
/// disagree with what the turn genuinely perceived, and it already did: the first
/// call site passed only the inbound message text while the mind had settled over
/// the whole rendered burst. Deriving it from the required argument makes the
/// disagreement unrepresentable instead of merely discouraged.
pub fn record_lived_turn(
    root: &std::path::Path,
    peer: crate::identity::PeerId,
    settled: &crate::cognition::act_observe::SettleOutcome,
) {
    let peer_dir = crate::identity::citizen_peer_dir(root, peer);
    if let Err(e) = std::fs::create_dir_all(&peer_dir) {
        tracing::warn!(
            peer = %peer,
            dir = %peer_dir.display(),
            error = %e,
            "could not create the citizen dir for her experience stream — this lived \
             turn will not become curriculum (#319 producer)"
        );
        return;
    }
    let record = ExperienceRecord::from_lived_turn(&settled.world_state, settled);
    if let Err(e) = append_experience(&peer_dir, &record) {
        tracing::warn!(
            peer = %peer,
            path = %experience_stream_path(&peer_dir).display(),
            error = %e,
            "could not append a lived turn to the experience stream — this episode \
             will not become curriculum (#319 producer)"
        );
    }
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

/// Collapse a stream to the LATEST record per task id, in first-seen task order.
/// The cursor-free dedup for the curriculum drain (#319): a later PASS on the same
/// task RETIRES its earlier failures (the lesson was learned — re-teaching it wastes
/// the teacher and risks overfitting a solved gap), and a later failure supersedes
/// an earlier one (teach the CURRENT miss, not the stale one). Records with an empty
/// task id (lived turns pose the stimulus as the prompt with no id) pass through
/// untouched — they are distinct experiences, not retries of one task.
pub fn latest_per_task(records: &[ExperienceRecord]) -> Vec<ExperienceRecord> {
    let mut order: Vec<&str> = Vec::new();
    let mut latest: std::collections::HashMap<&str, &ExperienceRecord> =
        std::collections::HashMap::new();
    let mut untasked: Vec<&ExperienceRecord> = Vec::new();
    for r in records {
        if r.task.id.is_empty() {
            untasked.push(r);
            continue;
        }
        if !latest.contains_key(r.task.id.as_str()) {
            order.push(r.task.id.as_str());
        }
        latest.insert(r.task.id.as_str(), r);
    }
    order
        .into_iter()
        .map(|id| latest[id].clone())
        .chain(untasked.into_iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests;
