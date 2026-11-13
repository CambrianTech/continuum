//! The examiner's durable receipt: one row per gene trial, in the ORM, carrying the four
//! fields of the continual-learning acceptance test (correction, unseen, restart, unrelated)
//! and the decision they made. A citizen reads it through her own data commands
//! (`data/list` on `genome_exam_receipts`), the same pipe every client uses.

use super::verdict::{ArmsEvidence, CorrectionSummary, ExamSitting, ExamVerdict, NllEvidence};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The retention re-run: the same sitting after a normal restart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RestartEvidence {
    /// Admitted in core process `examined_boot`; due once a later process runs.
    #[serde(rename_all = "camelCase")]
    Due { examined_boot: Uuid },
    /// Sat again in core process `boot`. `verdict.admit` false means she lost it: retired.
    #[serde(rename_all = "camelCase")]
    Sat {
        boot: Uuid,
        unseen: ArmsEvidence,
        unrelated: ArmsEvidence,
        nll: NllEvidence,
        verdict: ExamVerdict,
    },
    /// Nothing to retain: the gene was rejected or could not be examined.
    NotApplicable,
}

/// What the first sitting decided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExamOutcome {
    Admitted { verdict: ExamVerdict },
    Rejected { verdict: ExamVerdict },
    /// No honest sitting exists for this gene (its training provenance is unknown, so no task
    /// can be proven unseen; or no task is left once its trained tasks are held out). It
    /// stays in her head as it was, unjudged, and the reason is here.
    Unexaminable { why: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "genome_exam_receipts")]
pub struct GeneExamReceipt {
    #[entity(indexed)]
    pub trial_id: Uuid,
    #[entity(indexed)]
    pub persona_id: Uuid,
    pub gene: String,
    pub gene_path: String,
    pub base_model_id: String,
    /// What she learned from.
    #[entity(json)]
    pub correction: Option<CorrectionSummary>,
    /// Related tasks the gene never trained on.
    #[entity(json)]
    pub unseen: Option<ArmsEvidence>,
    /// Retention after a normal restart.
    #[entity(json)]
    pub restart: RestartEvidence,
    /// Unrelated tasks: what she already knew stayed.
    #[entity(json)]
    pub unrelated: Option<ArmsEvidence>,
    #[entity(json)]
    pub nll: Option<NllEvidence>,
    #[entity(json)]
    pub outcome: ExamOutcome,
    pub examined_at_ms: u64,
    /// The core process that sat the first exam (`this_process_origin`).
    pub examined_boot: Uuid,
}

/// The receipt's identity in its collection: the trial it judges (one receipt per trial).
pub fn receipt_id(trial: Uuid) -> Uuid {
    trial
}

impl GeneExamReceipt {
    /// The receipt of a first sitting.
    pub fn of_sitting(trial: &crate::genome::gene_trial::GeneTrial, s: &ExamSitting, v: ExamVerdict, boot: Uuid, now_ms: u64) -> Self {
        let admitted = v.admit;
        Self {
            correction: s.correction.clone(),
            unseen: Some(s.unseen.clone()),
            unrelated: Some(s.unrelated.clone()),
            nll: Some(s.nll.clone()),
            restart: if admitted { RestartEvidence::Due { examined_boot: boot } } else { RestartEvidence::NotApplicable },
            outcome: if admitted { ExamOutcome::Admitted { verdict: v } } else { ExamOutcome::Rejected { verdict: v } },
            ..Self::blank(trial, boot, now_ms)
        }
    }

    /// The receipt of a gene no honest sitting exists for.
    pub fn unexaminable(trial: &crate::genome::gene_trial::GeneTrial, why: String, boot: Uuid, now_ms: u64) -> Self {
        Self { outcome: ExamOutcome::Unexaminable { why }, ..Self::blank(trial, boot, now_ms) }
    }

    fn blank(trial: &crate::genome::gene_trial::GeneTrial, boot: Uuid, now_ms: u64) -> Self {
        Self {
            trial_id: trial.id,
            persona_id: trial.persona_id,
            gene: trial.alias.clone(),
            gene_path: trial.path.to_string_lossy().into_owned(),
            base_model_id: trial.base_model_id.clone(),
            correction: None,
            unseen: None,
            restart: RestartEvidence::NotApplicable,
            unrelated: None,
            nll: None,
            outcome: ExamOutcome::Unexaminable { why: String::new() },
            examined_at_ms: now_ms,
            examined_boot: boot,
        }
    }

    /// The verdict that decides the trial now: the retention re-run's once sat, else the
    /// first sitting's. `None` for an unexaminable gene.
    pub fn deciding_verdict(&self) -> Option<&ExamVerdict> {
        match (&self.restart, &self.outcome) {
            (RestartEvidence::Sat { verdict, .. }, _) => Some(verdict),
            (_, ExamOutcome::Admitted { verdict } | ExamOutcome::Rejected { verdict }) => Some(verdict),
            (_, ExamOutcome::Unexaminable { .. }) => None,
        }
    }

    /// The unseen-task arms behind [`Self::deciding_verdict`].
    pub fn deciding_unseen(&self) -> Option<&ArmsEvidence> {
        match &self.restart {
            RestartEvidence::Sat { unseen, .. } => Some(unseen),
            _ => self.unseen.as_ref(),
        }
    }
}
