//! The examiner's owner: one pass at a time, over what [`super::due`] says is owed.

use super::host::{ExamHost, ExamRefusal, GeneExam};
use super::receipt::{receipt_id, GeneExamReceipt, RestartEvidence};
use super::schedule::{due, ExamDue, ExamKind};
use super::verdict::decide;
use crate::genome::gene_trial::{apply_decision, GeneTrial, GeneTrials, TrialState, TrialVerdict};
use crate::orm::OrmStore;
use std::sync::Arc;
use uuid::Uuid;

/// The examiner. `boot` is the core process it runs in: a receipt admitted under another
/// boot owes a retention re-run.
pub struct Examiner {
    trials: GeneTrials,
    receipts: OrmStore<GeneExamReceipt>,
    host: Arc<dyn ExamHost>,
    boot: Uuid,
}

/// What one pass did, for its caller and its tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PassReport {
    pub decided: Vec<(Uuid, TrialState)>,
    pub unexaminable: Vec<Uuid>,
    /// Sittings refused for now; the pass re-arms on the next serving snapshot.
    pub not_now: Vec<Uuid>,
}

impl Examiner {
    pub fn new(trials: GeneTrials, receipts: OrmStore<GeneExamReceipt>, host: Arc<dyn ExamHost>, boot: Uuid) -> Self {
        Self { trials, receipts, host, boot }
    }

    /// Start serving for the life of the process: the first pass now (the retention re-run
    /// after a restart), then one per wake. Subscribes before the first pass, so a trial
    /// opened meanwhile is never missed; a lagged receiver wakes a pass too (it is idempotent).
    pub fn start(self: Arc<Self>) {
        let wake = Arc::new(tokio::sync::Notify::new());
        let rearm = Arc::new(std::sync::atomic::AtomicBool::new(false));
        match crate::runtime::MessageBus::global() {
            Some(bus) => {
                let mut rx = bus.receiver();
                let (wake, rearm) = (Arc::clone(&wake), Arc::clone(&rearm));
                tokio::spawn(async move {
                    use tokio::sync::broadcast::error::RecvError;
                    loop {
                        match rx.recv().await {
                            Ok(e) if e.name == super::TRIAL_OPENED_EVENT => wake.notify_one(),
                            Ok(e) if e.name == super::SERVING_SNAPSHOT_EVENT
                                && rearm.swap(false, std::sync::atomic::Ordering::AcqRel) =>
                            {
                                wake.notify_one()
                            }
                            Ok(_) => {}
                            Err(RecvError::Lagged(_)) => wake.notify_one(),
                            Err(RecvError::Closed) => break,
                        }
                    }
                });
            }
            None => continuum_base::probe!(
                class = "genome.examiner.no_bus",
                "no message bus: the examiner runs its boot pass only; trials opened later wait for the next process"
            ),
        }
        tokio::spawn(async move {
            loop {
                match self.pass().await {
                    Ok(report) => rearm.store(!report.not_now.is_empty(), std::sync::atomic::Ordering::Release),
                    Err(error) => continuum_base::probe!(
                        class = "genome.examiner.pass_failed",
                        error = error.as_str(),
                        "the examiner could not read what it owes: no gene was judged this pass"
                    ),
                }
                wake.notified().await;
            }
        });
    }

    /// One pass: every sitting owed, in order.
    pub async fn pass(&self) -> Result<PassReport, String> {
        let trials = self.trials.load()?;
        let receipts: Vec<GeneExamReceipt> =
            self.receipts.find_all().await.map_err(|e| e.to_string())?.into_iter().map(|(_, r)| r).collect();
        let mut report = PassReport::default();
        for ExamDue { trial, kind } in due(&trials, &receipts, self.boot) {
            let Some(t) = trials.iter().find(|t| t.id == trial) else { continue };
            let receipt = receipts.iter().find(|r| r.trial_id == trial);
            let outcome = match (kind, receipt) {
                (ExamKind::Initial, _) => self.initial(t).await,
                (ExamKind::Retention, Some(r)) => self.retention(t, r.clone()).await,
                (ExamKind::Settle, Some(r)) => self.settle(t, r).map(Some),
                // `due` names Retention and Settle only for a trial with a receipt.
                (ExamKind::Retention | ExamKind::Settle, None) => continue,
            };
            match outcome {
                Ok(Some(state)) => report.decided.push((trial, state)),
                Ok(None) => report.unexaminable.push(trial),
                // `initial` receipts an unexaminable gene itself; any refusal reaching here is
                // transient (not online, cold engine, a write that did not land).
                Err(ExamRefusal::NotNow(why) | ExamRefusal::Unexaminable(why)) => {
                    continuum_base::probe!(
                        class = "genome.examiner.not_now",
                        trial = %trial,
                        persona = %t.persona_id,
                        why = why.as_str(),
                        "the examiner could not sit or settle this gene now: it re-arms on the next serving snapshot"
                    );
                    report.not_now.push(trial);
                }
            }
        }
        Ok(report)
    }

    fn exam(t: &GeneTrial) -> GeneExam {
        GeneExam {
            persona: t.persona_id,
            gene: crate::genome::gene_trial::page_of(t),
            base_model_id: t.base_model_id.clone(),
            trained_from: t.trained_from.clone(),
        }
    }

    /// The first sitting. `Ok(None)`: receipted as unexaminable, the trial left open.
    async fn initial(&self, t: &GeneTrial) -> Result<Option<TrialState>, ExamRefusal> {
        let now_ms = continuum_base::time::EpochMs::now().millis();
        let receipt = match self.host.sit(&Self::exam(t)).await {
            Ok(sitting) => {
                let verdict = decide(&sitting);
                GeneExamReceipt::of_sitting(t, &sitting, verdict, self.boot, now_ms)
            }
            Err(ExamRefusal::Unexaminable(why)) => {
                let receipt = GeneExamReceipt::unexaminable(t, why.clone(), self.boot, now_ms);
                self.create(&receipt).await?;
                continuum_base::probe!(
                    class = "genome.examiner.verdict",
                    trial = %t.id,
                    persona = %t.persona_id,
                    gene = t.alias.as_str(),
                    outcome = "unexaminable",
                    reason = why.as_str(),
                    "no honest sitting exists for this gene: it stays in her head unjudged"
                );
                return Ok(None);
            }
            Err(not_now) => return Err(not_now),
        };
        // The receipt first: a stop before the trial file takes it leaves a Settle, never a
        // second sitting.
        self.create(&receipt).await?;
        self.settle(t, &receipt).map(Some)
    }

    /// The retention re-run after a normal restart, against the same check.
    async fn retention(&self, t: &GeneTrial, mut receipt: GeneExamReceipt) -> Result<Option<TrialState>, ExamRefusal> {
        let sitting = self.host.sit(&Self::exam(t)).await.map_err(|refusal| match refusal {
            // A gene admitted once is examinable; if it no longer is, that is not now.
            ExamRefusal::Unexaminable(why) => ExamRefusal::NotNow(why),
            not_now => not_now,
        })?;
        let verdict = decide(&sitting);
        receipt.restart = RestartEvidence::Sat {
            boot: self.boot,
            unseen: sitting.unseen,
            unrelated: sitting.unrelated,
            nll: sitting.nll,
            verdict,
        };
        self.receipts
            .update(receipt_id(receipt.trial_id), &receipt)
            .await
            .map_err(|e| ExamRefusal::NotNow(format!("retention receipt {} did not write: {e}", receipt.trial_id)))?;
        self.settle(t, &receipt).map(Some)
    }

    /// Carry the receipt's deciding verdict into the trial file and its effects.
    fn settle(&self, t: &GeneTrial, receipt: &GeneExamReceipt) -> Result<TrialState, ExamRefusal> {
        let (Some(verdict), Some(unseen)) = (receipt.deciding_verdict(), receipt.deciding_unseen()) else {
            return Err(ExamRefusal::NotNow("the receipt carries no verdict to settle".into()));
        };
        let state = if verdict.admit { TrialState::Promoted } else { TrialState::Retired };
        let retention = matches!(receipt.restart, RestartEvidence::Sat { .. });
        continuum_base::probe!(
            class = "genome.examiner.verdict",
            trial = %t.id,
            persona = %t.persona_id,
            gene = t.alias.as_str(),
            outcome = if verdict.admit { "admitted" } else { "rejected" },
            retention,
            p_better = verdict.p_better,
            unrelated_delta = verdict.unrelated_delta,
            reason = verdict.reason.as_str(),
            "the examiner decided a gene on the acceptance test"
        );
        if state == t.state {
            return Ok(state); // a retained gene: already promoted, nothing moves
        }
        let trial_verdict = TrialVerdict {
            with_gene: unseen.with_gene,
            without_gene: unseen.without_gene,
            reason: verdict.reason.clone(),
            receipt: Some(receipt_id(t.id)),
        };
        let now_ms = continuum_base::time::EpochMs::now().millis();
        match self.trials.decide(t.id, state, trial_verdict, now_ms) {
            Ok(Some(decided)) => {
                apply_decision(&decided, now_ms);
                Ok(state)
            }
            Ok(None) => Ok(state),
            Err(error) => Err(ExamRefusal::NotNow(format!("the trial file did not take the decision: {error}"))),
        }
    }

    async fn create(&self, receipt: &GeneExamReceipt) -> Result<(), ExamRefusal> {
        self.receipts
            .save(receipt_id(receipt.trial_id), receipt)
            .await
            .map_err(|e| ExamRefusal::NotNow(format!("receipt {} did not write: {e}", receipt.trial_id)))
    }
}
