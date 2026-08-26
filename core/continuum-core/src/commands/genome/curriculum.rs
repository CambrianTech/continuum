//! curriculum.rs — the **salience → curriculum** seam, teacher generalized.
//!
//! The afferent nerve ([`crate::cognition::experience`]) turns a lived episode into
//! a salient [`ExperienceRecord`]; the efferent organ ([`super::teach`]) turns a
//! task set into a validated corpus. Between them sat a hard-coded pipe: the only
//! thing that drove synthesis was the `genome/teach` command over a *static gym
//! file*. This module is the seam that generalizes it — a polymorphic
//! [`CurriculumSynthesizer`] the self-improvement loop drives over a persona's OWN
//! salience-selected failures, exactly the connection
//! [[attention-salience-selects-what-becomes-curriculum]] calls for.
//!
//! ## Why a trait (the outlier discipline)
//!
//! There is more than one way to turn salient experience into curriculum, and the
//! honest state today is that only ONE is instrumented end-to-end:
//!
//! - **Remediation** (built — [`RemediationSynthesizer`], outlier A): a *test-graded
//!   failure* → teach the write→error→fix→pass correction. The grader validates every
//!   trajectory, so the lesson can never be confident garbage.
//! - **Expansion** (the frontier — NOT built): a salient-but-*untestable* episode →
//!   synthesize a generalizing lesson. Named as the extension point, not faked: with
//!   no test the corrected trajectory cannot be objectively validated, and an
//!   unvalidated "lesson" is the exact failure the measurement spine forbids
//!   ([[fallbacks-are-illegal-fail-loud]]). It slots in as outlier B when a real
//!   validation signal for untestable episodes exists.
//!
//! Per the methodical process (build outlier A, define the seam so the maximally
//! different outlier B drops in, then STOP), this ships remediation and the trait —
//! not a second speculative synthesizer.
//!
//! ## Layering
//!
//! The trait lives here, next to the [`synthesize_remediation`] core it wraps, and
//! imports the salience vocabulary DOWN from `cognition::experience` (commands →
//! cognition is the correct direction — cognition never depends on commands). It is
//! a pure `records → corpus` transform: it does NOT write a dataset or forge a gene
//! (the caller — the command, or the L2 producer — owns packaging), so it composes
//! into either driver without dragging their side-effects along.

use async_trait::async_trait;

use super::teach::{synthesize_remediation, RemediationCorpus};
use super::teach::{DEFAULT_MAX_FIX_ITERS, DEFAULT_TEMPERATURE};
use crate::cognition::eval::EvalTask;
use crate::cognition::experience::{
    salient_teach_set, ErrorSalience, ExperienceRecord, ExperienceSource, SalienceDetector,
};
use crate::cognition::inference_session::resolve_model;
use crate::sdk_codegen::CommandError;
use serde_json::{json, Value};

/// Turn a persona's salient lived episodes into a validated training corpus. The
/// generalized efferent organ: one seam, driven the same way whether the input is a
/// static gym set (the `genome/teach` command) or a persona's own measured failures
/// (the self-improvement loop). Polymorphic so remediation (today) and expansion
/// (the frontier) share it without either driver knowing which strategy ran.
#[async_trait]
pub trait CurriculumSynthesizer: Send + Sync {
    /// A short, stable name — for provenance on the produced corpus ("which teacher
    /// made this lesson").
    fn name(&self) -> &'static str;

    /// Salient lived episodes → a validated corpus. An empty `records`, or records
    /// with no salient+synthesizable episode, yields an **empty corpus** (an honest
    /// "nothing to learn here"), never an error — absence of a fitness gap is a
    /// legitimate outcome, not a fault.
    async fn synthesize(
        &self,
        records: &[ExperienceRecord],
    ) -> Result<RemediationCorpus, CommandError>;
}

/// **Outlier A — remediation.** Select the salient, test-graded failures (via a
/// [`SalienceDetector`]) and teach corrections for them through the reusable
/// [`synthesize_remediation`] core. This is the strategy `genome/teach` already runs
/// over a static set; wrapping it here lets the self-improvement loop run the SAME
/// synthesis over a persona's own salience-selected episodes.
///
/// Mirror-and-challenge: it teaches HER real failed tasks (mirror) and the fix-loop
/// stretches past the first wrong attempt (challenge). Measurement stays elsewhere
/// (`cognition/eval`, isolated) — this only PRODUCES curriculum.
pub struct RemediationSynthesizer {
    /// The selector deciding which episodes are worth a lesson. `ErrorSalience` by
    /// default — the one proxy instrumented end-to-end; boxed so a composite detector
    /// slots in without touching this type.
    detector: Box<dyn SalienceDetector>,
    /// The model that writes + fixes. `None` = resolve the locally-served model at
    /// synthesis time (runs with no external dep); `Some` points at a stronger
    /// peer/gateway teacher for higher yield.
    teacher_model: Option<String>,
    /// Decoding temperature — defaults to [`DEFAULT_TEMPERATURE`] (low: correct,
    /// convergent code, not wandering).
    temperature: f32,
    /// Max fix attempts per task before it's dropped unsolved — defaults to
    /// [`DEFAULT_MAX_FIX_ITERS`].
    max_fix_iters: u32,
}

impl Default for RemediationSynthesizer {
    fn default() -> Self {
        Self {
            detector: Box::new(ErrorSalience),
            teacher_model: None,
            temperature: DEFAULT_TEMPERATURE,
            max_fix_iters: DEFAULT_MAX_FIX_ITERS,
        }
    }
}

impl RemediationSynthesizer {
    /// A remediation synthesizer with the honest defaults (`ErrorSalience`, local
    /// teacher, [`DEFAULT_TEMPERATURE`]/[`DEFAULT_MAX_FIX_ITERS`]).
    pub fn new() -> Self {
        Self::default()
    }

    /// Point at an explicit teacher model instead of the locally-served one.
    pub fn with_teacher(mut self, model: impl Into<String>) -> Self {
        self.teacher_model = Some(model.into());
        self
    }

    /// Swap in a different salience selector (e.g. a future composite detector).
    pub fn with_detector(mut self, detector: Box<dyn SalienceDetector>) -> Self {
        self.detector = detector;
        self
    }

    /// The selection sub-step, exposed so the record→remediable-task projection is
    /// unit-testable WITHOUT running a live teacher: salient + test-graded episodes →
    /// the `EvalTask`s remediation will teach. This is the deterministic half of
    /// `synthesize`; the other half (teacher generation) is inherently an inference
    /// path covered by `genome/teach`'s own tests.
    pub fn select(&self, records: &[ExperienceRecord]) -> Vec<EvalTask> {
        salient_teach_set(records, self.detector.as_ref())
    }
}

#[async_trait]
impl CurriculumSynthesizer for RemediationSynthesizer {
    fn name(&self) -> &'static str {
        "remediation"
    }

    async fn synthesize(
        &self,
        records: &[ExperienceRecord],
    ) -> Result<RemediationCorpus, CommandError> {
        let tasks = self.select(records);
        // No salient, test-graded failure → nothing to remediate. An empty corpus is
        // the honest result (no fitness gap to teach), NOT a resolve-a-teacher-for-
        // nothing round trip — so we short-circuit before touching inference.
        if tasks.is_empty() {
            return Ok(RemediationCorpus {
                examples: Vec::new(),
                outcomes: Vec::new(),
                with_correction: 0,
            });
        }
        // Resolve the teacher only once we know there's work: explicit → local. Fail
        // loud if nothing serves (no silent skip) — same discipline as `genome/teach`.
        let teacher_model = match &self.teacher_model {
            Some(m) => m.clone(),
            None => resolve_model(None).await.map_err(|e| {
                CommandError::Internal(format!("teacher model resolve failed: {e:?}"))
            })?,
        };
        synthesize_remediation(&tasks, &teacher_model, self.temperature, self.max_fix_iters).await
    }
}

/// **Expansion — the received-lesson path (the honest floor of outlier B).** Turn
/// salient UNtestable episodes into SFT `{"messages":[…]}` examples for the SAME
/// dataset→train→eval→page-in flywheel the remediation path feeds — the efferent organ
/// for [`crate::cognition::experience::expansion_teach_set`]. The examples are written
/// as a dataset (mirroring `dataset/from-turns`) and handed to the existing
/// `genome/job-create`; validation is per-CONSOLIDATION by whole-being benchmark lift
/// (#59), never per-trajectory, which is exactly what makes untestable material safe to
/// train on ([[lived-and-eval-experience-are-one-stream-one-being]]).
///
/// Today this builds the **Received** axis directly: a shared lesson's content is
/// *deliberately-authored true knowledge* (not a synthesized guess), so it becomes a
/// teaching pair without an LLM teacher — instilling into the GENOME what `memory/share`
/// (#2025) handed into the corpus. That is the telepathy→weights step: a lesson one
/// agent learns becomes another's trained-in capability, not just a retrieved note.
///
/// The **Lived** axis (generalize a non-converged trajectory into a lesson) is skipped
/// here on purpose: without a known-correct answer, turning a failure into a "lesson"
/// needs an LLM teacher to avoid confident garbage — named as the enrichment path
/// (outlier B of this outlier B), never faked with a low-quality template
/// ([[fallbacks-are-illegal-fail-loud]]). Empty in → empty out (no fitness gap is a
/// legitimate outcome, not a fault).
pub fn expansion_examples(records: &[ExperienceRecord]) -> Vec<Value> {
    records
        .iter()
        // Only the received axis is directly teachable today; lived-generalization is
        // the named LLM-teacher path, not faked here.
        .filter(|r| r.source == ExperienceSource::Received)
        .filter(|r| !r.answer.trim().is_empty())
        .map(|r| {
            // The lesson content IS the knowledge (the assistant turn). Frame it by its
            // topic — `task.prompt` carries the shared lesson's scope; a bare lesson with
            // no scope still stands on its own as the thing to know.
            let topic = if r.task.prompt.trim().is_empty() {
                "What have you learned that's worth remembering?".to_string()
            } else {
                format!("What have you learned about {}?", r.task.prompt.trim())
            };
            json!({
                "messages": [
                    { "role": "user", "content": topic },
                    { "role": "assistant", "content": r.answer.trim() },
                ]
            })
        })
        .collect()
}

/// **Expansion — the LIVED axis (outlier B of outlier B).** The received path
/// ([`expansion_examples`]) teaches deliberately-authored true knowledge directly, but a
/// salient LIVED turn is a FAILURE with no known-correct answer, so it needs an LLM teacher
/// to avoid confident garbage — the path `expansion_examples` names but skips. This is that
/// teacher, structured exactly like [`RemediationSynthesizer`]: a deterministic
/// [`select`](Self::select) (unit-testable with no inference) picks the salient lived
/// stimuli, and [`synthesize`](Self::synthesize) resolves a teacher and re-answers each,
/// yielding `{question → answer}` SFT pairs.
///
/// The teacher re-answers the STIMULUS she stalled on — never her stalled answer (that would
/// teach the failure) — and the corpus is validated per-CONSOLIDATION by benchmark lift
/// (#59), the same whole-being gate that makes the received axis safe on untestable material.
/// It is a sibling of [`RemediationSynthesizer`], not an impl of [`CurriculumSynthesizer`]:
/// the return type is bare SFT `{messages}` (like [`expansion_examples`]), NOT a
/// test-validated [`RemediationCorpus`] — the differing type is the honest signal that these
/// are two distinct efferent organs, not one behind a fake corpus.
pub struct LivedExpansionSynthesizer {
    /// The salience selector — `ErrorSalience` by default, so ONLY lived failures (the ones
    /// worth a lesson) feed the teacher; a cleanly-settled lived turn is not salient and is
    /// never re-taught. Boxed so a composite detector slots in without touching this type.
    detector: Box<dyn SalienceDetector>,
    /// `None` = resolve the locally-served model at synthesis time (runs with no external
    /// dep); `Some` points at a stronger peer/gateway teacher for higher-quality answers.
    teacher_model: Option<String>,
    /// Decoding temperature — the same low default as remediation (correct, convergent
    /// answers, not wandering).
    temperature: f32,
}

impl Default for LivedExpansionSynthesizer {
    fn default() -> Self {
        Self {
            detector: Box::new(ErrorSalience),
            teacher_model: None,
            temperature: DEFAULT_TEMPERATURE,
        }
    }
}

impl LivedExpansionSynthesizer {
    /// A lived-expansion synthesizer with the honest defaults (`ErrorSalience`, local
    /// teacher, [`DEFAULT_TEMPERATURE`]).
    pub fn new() -> Self {
        Self::default()
    }

    /// Point at an explicit teacher model instead of the locally-served one.
    pub fn with_teacher(mut self, model: impl Into<String>) -> Self {
        self.teacher_model = Some(model.into());
        self
    }

    /// Swap in a different salience selector.
    pub fn with_detector(mut self, detector: Box<dyn SalienceDetector>) -> Self {
        self.detector = detector;
        self
    }

    /// A stable provenance name for the corpus this makes.
    pub fn name(&self) -> &'static str {
        "lived-expansion"
    }

    /// The deterministic selection sub-step (unit-testable WITHOUT a teacher): the salient
    /// LIVED episodes' stimuli — what she was asked and stalled on, to be re-answered. A
    /// received or eval record is not lived; a lived record the detector doesn't flag (a
    /// clean turn) or one with no stimulus is dropped. Mirrors
    /// [`RemediationSynthesizer::select`] — selection here, inference in `synthesize`.
    pub fn select(&self, records: &[ExperienceRecord]) -> Vec<String> {
        records
            .iter()
            .filter(|r| r.source == ExperienceSource::Lived)
            .filter(|r| self.detector.assess(r).is_some())
            .map(|r| r.task.prompt.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Salient lived episodes → `{question → answer}` SFT examples via the teacher. Empty
    /// selection → empty (no teacher round-trip); fail-loud if a teacher can't be resolved
    /// once there IS work — same discipline as [`RemediationSynthesizer::synthesize`].
    pub async fn synthesize(
        &self,
        records: &[ExperienceRecord],
    ) -> Result<Vec<Value>, CommandError> {
        let stimuli = self.select(records);
        if stimuli.is_empty() {
            return Ok(Vec::new());
        }
        let teacher_model = match &self.teacher_model {
            Some(m) => m.clone(),
            None => resolve_model(None).await.map_err(|e| {
                CommandError::Internal(format!("teacher model resolve failed: {e:?}"))
            })?,
        };
        super::teach::synthesize_lived_expansion(&stimuli, &teacher_model, self.temperature).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_task(id: &str, with_test: bool) -> EvalTask {
        EvalTask {
            id: id.to_string(),
            max_acts: None,
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

    fn record(id: &str, ok: bool, with_test: bool) -> ExperienceRecord {
        ExperienceRecord {
            task: eval_task(id, with_test),
            ok,
            grade: if ok {
                "tests passed".into()
            } else {
                "error[E0308]".into()
            },
            answer: String::new(),
            world_state: String::new(),
            acts: 1,
            source: crate::cognition::experience::ExperienceSource::Eval,
            room: uuid::Uuid::from_u128(7),
        }
    }

    // What this catches: the generalized synthesizer selects EXACTLY the salient,
    // test-graded failures out of a mixed batch of a persona's lived episodes —
    // dropping passes (no gap) and untestable failures (can't validate a correction).
    // This is the deterministic contract that lets the self-improvement loop feed the
    // teacher its OWN measured failures instead of a static gym file; a regression
    // here would either waste teacher compute on already-known tasks or (worse) feed
    // an unvalidatable "lesson" into training.
    #[test]
    fn remediation_selects_only_salient_testable_failures() {
        let synth = RemediationSynthesizer::new();
        let batch = [
            record("failed-testable", false, true),    // keep
            record("passed", true, true),              // drop: no gap
            record("failed-untestable", false, false), // drop: can't validate
        ];

        let tasks = synth.select(&batch);

        assert_eq!(
            tasks.len(),
            1,
            "only the failed, test-graded task feeds remediation"
        );
        assert_eq!(tasks[0].id, "failed-testable");
    }

    // What this catches: a batch with NO salient failure selects to an empty task set
    // — so `synthesize` can honor its "empty corpus, not an error, and no teacher
    // round-trip" contract. Proves the short-circuit's precondition: absence of a
    // fitness gap is a legitimate outcome the loop must tolerate silently, never a
    // fault that would spam errors on every clean turn.
    #[test]
    fn no_salient_failure_selects_empty() {
        let synth = RemediationSynthesizer::new();
        let all_passed = [record("a", true, true), record("b", true, true)];
        assert!(
            synth.select(&all_passed).is_empty(),
            "no failure → nothing to teach → empty (the loop's quiet-when-healthy path)"
        );
    }

    // What this catches: the provenance name is stable — the produced corpus can be
    // attributed to the strategy that made it ("remediation"), which matters once
    // expansion (outlier B) joins and a lesson's origin must be distinguishable.
    #[test]
    fn name_is_stable() {
        assert_eq!(RemediationSynthesizer::new().name(), "remediation");
    }

    // What this catches: the expansion floor — a RECEIVED lesson becomes a real SFT
    // {messages} example (the lesson content as the assistant turn, keyed on its topic),
    // so telepathy reaches the GENOME: what memory/share wrote into the corpus becomes
    // trained-in capability. A LIVED non-convergence is skipped (needs an LLM teacher,
    // not faked), and a received lesson with empty content is dropped. This is the
    // dataset the dream hands to genome/job-create, validated downstream by benchmark lift.
    #[test]
    fn expansion_examples_teaches_received_lessons_and_skips_lived() {
        let received = ExperienceRecord {
            task: EvalTask {
                prompt: "continuum".to_string(),
                ..EvalTask::default()
            },
            ok: true,
            grade: "received lesson from BigMama".to_string(),
            answer: "the call room IS the airc room — never mint a rogue call_id".to_string(),
            world_state: String::new(),
            acts: 0,
            source: ExperienceSource::Received,
            room: uuid::Uuid::from_u128(7),
        };
        // A lived turn is salient-untestable too, but not directly teachable — LLM path.
        let lived = ExperienceRecord {
            task: EvalTask::default(),
            ok: false,
            grade: "lived turn: did not converge".to_string(),
            answer: "half-finished attempt".to_string(),
            world_state: String::new(),
            acts: 8,
            source: ExperienceSource::Lived,
            room: uuid::Uuid::from_u128(7),
        };
        // An empty received lesson has nothing to teach.
        let empty_received = ExperienceRecord {
            answer: "   ".to_string(),
            ..received.clone()
        };

        let examples = expansion_examples(&[received, lived, empty_received]);
        assert_eq!(
            examples.len(),
            1,
            "only the non-empty received lesson is directly teachable"
        );

        let msgs = examples[0]["messages"].as_array().expect("messages array");
        assert_eq!(msgs[0]["role"], "user");
        assert!(
            msgs[0]["content"].as_str().unwrap().contains("continuum"),
            "the topic frames the lesson"
        );
        assert_eq!(msgs[1]["role"], "assistant");
        assert!(
            msgs[1]["content"]
                .as_str()
                .unwrap()
                .contains("call room IS the airc room"),
            "the lesson content is the trained-in knowledge"
        );

        assert!(
            expansion_examples(&[]).is_empty(),
            "empty in → empty out (no gap is a legitimate outcome)"
        );
    }

    /// Build a lived ExperienceRecord directly (the from_lived_turn shape without a
    /// SettleOutcome): a stimulus she faced, a salience-carrying grade, ok toggled.
    fn lived_record(stimulus: &str, ok: bool) -> ExperienceRecord {
        ExperienceRecord {
            task: EvalTask {
                prompt: stimulus.to_string(),
                ..EvalTask::default()
            },
            ok,
            grade: if ok {
                "lived turn: settled".into()
            } else {
                "lived turn: did not converge".into()
            },
            answer: "half-finished attempt".to_string(),
            world_state: String::new(),
            acts: 8,
            source: ExperienceSource::Lived,
            room: uuid::Uuid::from_u128(7),
        }
    }

    // What this catches: the LIVED-axis selection is the deterministic half of the
    // LLM-teacher path — it picks ONLY salient (failed) lived episodes' STIMULI (what she
    // stalled on, to be re-answered by the teacher), never her stalled answer. A clean lived
    // turn (ok=true → not salient) is dropped, a received/eval record is not lived, and an
    // empty stimulus has nothing to re-pose. This is what lets the teacher path be tested
    // without live inference; a regression here would re-teach clean turns (wasted compute)
    // or feed the teacher a blank prompt.
    #[test]
    fn lived_expansion_selects_only_salient_lived_stimuli() {
        let synth = LivedExpansionSynthesizer::new();

        let failed_lived = lived_record("how does build_workspace_cycle settle a turn?", false);
        let clean_lived = lived_record("say hi to the room", true); // ok → not salient → drop
        let empty_stimulus = lived_record("   ", false); // salient but nothing to re-pose → drop
                                                         // A received lesson is untestable+salient but NOT lived — it has its own direct path
                                                         // (expansion_examples), never the teacher.
        let received = ExperienceRecord {
            task: EvalTask {
                prompt: "continuum".to_string(),
                ..EvalTask::default()
            },
            ok: true,
            grade: "received lesson from BigMama".into(),
            answer: "the call room IS the airc room".into(),
            world_state: String::new(),
            acts: 0,
            source: ExperienceSource::Received,
            room: uuid::Uuid::from_u128(7),
        };

        let stimuli = synth.select(&[failed_lived, clean_lived, empty_stimulus, received]);

        assert_eq!(
            stimuli.len(),
            1,
            "only the salient lived turn with a real stimulus is selected"
        );
        assert_eq!(stimuli[0], "how does build_workspace_cycle settle a turn?");
    }

    // What this catches: no salient lived turn → empty selection, so synthesize honors its
    // "empty → no teacher round-trip" contract (the quiet-when-healthy path). A batch of
    // clean lived turns and received lessons yields nothing for the lived teacher.
    #[test]
    fn lived_expansion_empty_when_nothing_salient_and_lived() {
        let synth = LivedExpansionSynthesizer::new();
        let batch = [
            lived_record("all good", true),
            lived_record("also fine", true),
        ];
        assert!(
            synth.select(&batch).is_empty(),
            "no salient lived failure → nothing to re-teach → empty (no teacher spin-up)"
        );
        assert_eq!(synth.name(), "lived-expansion");
    }
}
