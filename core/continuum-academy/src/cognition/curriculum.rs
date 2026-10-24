//! curriculum.rs — the **salience → curriculum** seam, lived-expansion teacher.
//!
//! The afferent nerve ([`crate::cognition::experience`]) turns a lived episode into a
//! salient [`ExperienceRecord`]; this teacher re-answers the stimulus she stalled on so the
//! pair can feed the dataset→train→eval flywheel. The remediation synthesizer, its
//! `CurriculumSynthesizer` trait and `expansion_examples` had no caller and were removed.

use crate::cognition::eval::synthesize_lived_expansion;
use crate::genome::teach::DEFAULT_TEMPERATURE;
use crate::cognition::experience::{ErrorSalience, ExperienceRecord, ExperienceSource, SalienceDetector};
use crate::cognition::inference_session::resolve_model;
use crate::sdk_codegen::CommandError;
use serde_json::Value;

/// **Expansion — the LIVED axis.** A salient LIVED turn is a FAILURE with no known-correct
/// answer, so it needs an LLM teacher to avoid confident garbage. A deterministic
/// [`select`](Self::select) (unit-testable with no inference) picks the salient lived
/// stimuli, and [`synthesize`](Self::synthesize) resolves a teacher and re-answers each,
/// yielding `{question → answer}` SFT pairs.
///
/// The teacher re-answers the STIMULUS she stalled on — never her stalled answer (that would
/// teach the failure) — and the corpus is validated per-CONSOLIDATION by benchmark lift
/// (#59), the same whole-being gate that makes the received axis safe on untestable material.
/// The return type is bare SFT `{messages}`, not a test-validated corpus.
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
    /// clean turn) or one with no stimulus is dropped. Selection here, inference in `synthesize`.
    pub fn select(&self, records: &[ExperienceRecord]) -> Vec<String> {
        records
            .iter()
            .filter(|r| r.source == ExperienceSource::Lived)
            .filter(|r| self.detector.assess(r).is_some())
            .map(|r| r.training_prompt().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Salient lived episodes → `{question → answer}` SFT examples via the teacher. Empty
    /// selection → empty (no teacher round-trip); fail-loud if a teacher can't be resolved
    /// once there IS work.
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
        synthesize_lived_expansion(&stimuli, &teacher_model, self.temperature).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::eval::EvalTask;

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
            room_updates: Default::default(),
            acts: 8,
            source: ExperienceSource::Lived,
            teammates: Vec::new(),
            team_role: None,
            room: Some(uuid::Uuid::from_u128(7)),
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
                                                         // (the received lesson path), never the teacher.
        let received = ExperienceRecord {
            task: EvalTask {
                prompt: "continuum".to_string(),
                ..EvalTask::default()
            },
            ok: true,
            grade: "received lesson from BigMama".into(),
            answer: "the call room IS the airc room".into(),
            world_state: String::new(),
            room_updates: Default::default(),
            acts: 0,
            source: ExperienceSource::Received,
            teammates: Vec::new(),
            team_role: None,
            room: Some(uuid::Uuid::from_u128(7)),
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
