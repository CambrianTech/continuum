//! The EXAMINER: runs the continual-learning acceptance test on every gene that lands, and
//! its verdict alone keeps or retires the gene (README: work -> experience -> curriculum ->
//! training -> gene -> phenotype -> evaluation -> adopt/reject).
//!
//! THE TEST (Joel, canonical): a reviewed correction applied on UNSEEN related tasks,
//! retained after a normal restart, unrelated retention intact, against the same
//! base/tools/context WITHOUT the gene. A finished training job is liveness, never this.
//! Grading lives here, on the learning side only; her normal work is never scored.
//!
//! Triggers, never a poll: [`TRIAL_OPENED_EVENT`] (published by the one adoption seam,
//! `gene_trial::Adoption::adopt`) wakes a pass; the first pass of every core process is the
//! retention re-run for genes an earlier process admitted; a pass that could not sit now
//! ([`ExamRefusal::NotNow`]) re-arms on the next serving snapshot. Each pass reads what is
//! owed from the trial file and the receipts ([`schedule::due`]), so it is idempotent.
//!
//! Every verdict is a durable [`GeneExamReceipt`] (ORM) and a `genome.examiner.verdict` probe.

mod host;
mod receipt;
mod run;
mod schedule;
mod verdict;

pub use host::{ExamHost, ExamRefusal, GeneExam};
pub use receipt::{receipt_id, ExamOutcome, GeneExamReceipt, RestartEvidence};
pub use run::Examiner;
pub use schedule::{due, ExamDue, ExamKind};
pub use verdict::{
    decide, p_gene_better, ArmTally, ArmsEvidence, CorrectionSummary, ExamSitting, ExamVerdict, NllEvidence,
    ADMIT_AT, UNRELATED_TASKS_TOLERATED,
};

/// Published when a gene trial opens: the examiner's wake.
pub const TRIAL_OPENED_EVENT: &str = "genome:trial:opened";
/// Published by the serving owner on every snapshot change: re-arms a pass that could not sit.
pub const SERVING_SNAPSHOT_EVENT: &str = "serving.snapshot";

/// Wake the examiner. No bus (a test, a tool): probed, nothing else; the next core process's
/// first pass finds the trial anyway.
pub fn wake(event: &'static str, trial: uuid::Uuid) {
    match crate::runtime::MessageBus::global() {
        Some(bus) => bus.publish_async_only(event, serde_json::json!({ "trial": trial })),
        None => continuum_base::probe!(
            class = "genome.examiner.no_bus",
            trial = %trial,
            "no message bus in this process: the examiner hears of this trial at its next pass"
        ),
    }
}

#[cfg(test)]
mod tests;
