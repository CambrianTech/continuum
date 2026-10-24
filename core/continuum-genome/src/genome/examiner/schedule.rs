//! PURE: which trials the examiner owes a sitting, from the trial file and its receipts.
//! Idempotent: the same state always yields the same work, so a wake that arrives twice, or
//! a pass cut short by a stop, repeats nothing that already landed.

use super::receipt::{ExamOutcome, GeneExamReceipt, RestartEvidence};
use crate::genome::gene_trial::{GeneTrial, TrialState};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExamKind {
    /// A trial with no receipt: sit the acceptance test.
    Initial,
    /// A promoted gene admitted by an earlier core process: sit it again after the restart.
    Retention,
    /// A receipt already decided this trial, but the trial file never took the decision (a
    /// stop between the two writes): apply it, sit nothing.
    Settle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExamDue {
    pub trial: Uuid,
    pub kind: ExamKind,
}

/// The sittings owed, in trial-file order. `boot` is this core process.
pub fn due(trials: &[GeneTrial], receipts: &[GeneExamReceipt], boot: Uuid) -> Vec<ExamDue> {
    trials
        .iter()
        .filter_map(|t| {
            let receipt = receipts.iter().find(|r| r.trial_id == t.id);
            let kind = match (t.state, receipt) {
                (TrialState::Trial, None) => ExamKind::Initial,
                (TrialState::Trial, Some(r)) => match r.outcome {
                    ExamOutcome::Admitted { .. } | ExamOutcome::Rejected { .. } => ExamKind::Settle,
                    ExamOutcome::Unexaminable { .. } => return None,
                },
                (TrialState::Promoted, Some(r)) => match (&r.restart, r.deciding_verdict()) {
                    (RestartEvidence::Due { examined_boot }, _) if *examined_boot != boot => ExamKind::Retention,
                    (RestartEvidence::Sat { .. }, Some(v)) if !v.admit => ExamKind::Settle,
                    _ => return None,
                },
                // A gene promoted before the examiner existed, or decided and done.
                (TrialState::Promoted, None) | (TrialState::Retired | TrialState::Expired, _) => return None,
            };
            Some(ExamDue { trial: t.id, kind })
        })
        .collect()
}
