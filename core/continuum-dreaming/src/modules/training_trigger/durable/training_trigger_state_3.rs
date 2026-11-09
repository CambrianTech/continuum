use super::*;

impl TrainingTriggerState {


    /// Withdraw pending submissions before they train (`genome/training-trigger/retire`).
    /// Only rows still pending are examined: by card, that is a bounded scan of the
    /// pending rows across every bucket (a card's dispatched history is not searched;
    /// a job's examples come back through `return`). Each touched bucket is handled
    /// under its own submit gate: the receipt is written first, the rows flip to
    /// retired, and the bucket cache is rebuilt from the rows, so a dispatch racing
    /// this call sees either the rows or their absence, never a half-flipped bucket.
    pub(crate) async fn retire(
        self: &Arc<Self>,
        selection: RetireSelection,
        reason: String,
        retired_by: Option<Uuid>,
    ) -> Result<RetireReport, (&'static str, String)> {
        self.require_ready()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let store = self
            .durable
            .require()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let mut report = RetireReport::default();
        // Which rows, grouped by the bucket that owns them.
        let mut by_key: std::collections::BTreeMap<(Uuid, String, String), Vec<Uuid>> =
            std::collections::BTreeMap::new();
        let card = match &selection {
            RetireSelection::Card(card) => Some(*card),
            RetireSelection::Submissions(_) => None,
        };
        match selection {
            RetireSelection::Submissions(ids) => {
                for id in ids {
                    match store
                        .submissions
                        .find_by_id(id)
                        .await
                        .map_err(|e| ("PersistenceFailed", e.to_string()))?
                    {
                        Some(row) => by_key
                            .entry((row.persona_id, row.trait_kind.clone(), row.base_model.clone()))
                            .or_default()
                            .push(id),
                        None => report.refused.push((
                            id,
                            None,
                            "no submission with this id on this node".into(),
                        )),
                    }
                }
            }
            RetireSelection::Card(card) => {
                for row in store
                    .pending_rows_on_card(card)
                    .await
                    .map_err(|e| ("PersistenceFailed", e))?
                {
                    let id = entity_id(&row.base).map_err(|e| ("PersistenceFailed", e))?;
                    by_key
                        .entry((row.persona_id, row.trait_kind.clone(), row.base_model.clone()))
                        .or_default()
                        .push(id);
                }
            }
        }
        for ((persona_id, trait_kind, base_model), ids) in by_key {
            let key = BucketKey {
                persona_id,
                trait_kind,
                base_model,
            };
            let reason = reason.clone();
            let part = self
                .run_owned(key, move |state, key| async move {
                    state.retire_in_bucket(&key, card, ids, reason, retired_by).await
                })
                .await
                .map_err(|e| ("RecoveryRequired", e))?
                .map_err(|e| ("PersistenceFailed", e))?;
            report.absorb(part);
        }
        Ok(report)
    }

    /// The gated half of [`Self::retire`] for one bucket: eligibility is judged on the
    /// rows as persisted NOW (a dispatch may have taken them since the selection),
    /// the receipt names exactly the rows that flip, and the cache is rebuilt from
    /// the rows afterwards. A crash after the receipt and before the last flip
    /// leaves rows a rerun of the same selection finishes under a second receipt,
    /// reporting the first's rows as `already_retired`.
    pub(super) async fn retire_in_bucket(
        self: Arc<Self>,
        key: &BucketKey,
        card: Option<Uuid>,
        ids: Vec<Uuid>,
        reason: String,
        retired_by: Option<Uuid>,
    ) -> Result<RetireReport, String> {
        let store = self.durable.require()?;
        let mut report = RetireReport::default();
        let mut eligible = Vec::new();
        for id in ids {
            match store.submissions.find_by_id(id).await.map_err(|e| e.to_string())? {
                None => report
                    .refused
                    .push((id, None, "no submission with this id on this node".into())),
                Some(row) if row.key() != *key => report.refused.push((
                    id,
                    None,
                    "the submission belongs to another bucket".into(),
                )),
                Some(row) if row.retirement_id.is_some() => report.already_retired.push(id),
                Some(row) if !row.is_pending => report.refused.push((
                    id,
                    row.dispatch_id,
                    "already learned in place (or carried into a job before in-place learning): nothing pending to withdraw".into(),
                )),
                Some(row) => eligible.push((id, row)),
            }
        }
        if eligible.is_empty() {
            return Ok(report);
        }
        let retirement_id = Uuid::new_v4();
        let examples: u32 = eligible
            .iter()
            .map(|(_, row)| row.batch.examples.len() as u32)
            .sum();
        let mut base = BaseEntity::for_new_record();
        base.id = retirement_id.to_string();
        let receipt = Retirement {
            base,
            card_id: card,
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: eligible.iter().map(|(id, _)| *id).collect(),
            examples,
            reason: reason.clone(),
            retired_by,
            retired_at_ms: continuum_base::time::EpochMs::now().millis(), // a clock before 1970 stamps 0: the receipt still names its rows and reason
        };
        store
            .retirements
            .save(retirement_id, &receipt)
            .await
            .map_err(|e| e.to_string())?;
        // Withdrawn examples owe serving nothing: their demand (a parked example returned to her
        // bucket keeps it) is cleared before the rows flip, or it would reserve a window for
        // examples that will never train (card 4545eafd; FOCUS-CONTRACT: no stale demand). A
        // failure here leaves the rows pending under this receipt; rerunning finishes it.
        let withdrawn: Vec<String> = eligible
            .iter()
            .flat_map(|(_, row)| row.batch.examples.iter().map(crate::genome::fine_tuning::training_demand::TrainingDemand::key_of))
            .collect();
        crate::genome::fine_tuning::training_demand::TrainingDemand::for_persona(key.persona_id)
            .clear(&withdrawn)
            .map_err(|e| format!("retirement {retirement_id}: the withdrawn examples' demand could not be cleared ({e}); rerun the same retirement to finish"))?;
        for (id, mut row) in eligible {
            let count = row.batch.examples.len() as u32;
            row.is_pending = false;
            row.retirement_id = Some(retirement_id);
            store
                .submissions
                .update(id, &row)
                .await
                .map_err(|e| format!("submission {id} could not be retired after receipt {retirement_id} was written: {e}; rerun the same retirement to finish"))?;
            report.retired.push((id, count));
            report.examples_retired += count;
        }
        report.retirement_ids.push(retirement_id);
        continuum_base::probe!(
            class = "training.trigger.retired",
            persona = %key.persona_id,
            trait_kind = %key.trait_kind,
            card = ?card,
            retirement = %retirement_id,
            submissions = report.retired.len(),
            examples = report.examples_retired,
            reason = %reason,
            "pending submissions withdrawn before training; the next job will not see them"
        );
        Ok(report)
    }

}








/// What [`TrainingTriggerState::retire`] withdraws.
#[derive(Debug, Clone)]
pub(crate) enum RetireSelection {
    /// Every pending submission whose examples were settled from this card.
    Card(Uuid),
    /// Exactly these submissions.
    Submissions(Vec<Uuid>),
}

/// The crate-internal result of a retirement; the verb projects it onto the wire.
#[derive(Debug, Default)]
pub(crate) struct RetireReport {
    pub(crate) retirement_ids: Vec<Uuid>,
    /// `(submission, examples it carried)`.
    pub(crate) retired: Vec<(Uuid, u32)>,
    pub(crate) already_retired: Vec<Uuid>,
    /// `(submission, the dispatch that carried it if that is why, why)`.
    pub(crate) refused: Vec<(Uuid, Option<Uuid>, String)>,
    pub(crate) examples_retired: u32,
}

impl RetireReport {
    pub(super) fn absorb(&mut self, other: RetireReport) {
        self.retirement_ids.extend(other.retirement_ids);
        self.retired.extend(other.retired);
        self.already_retired.extend(other.already_retired);
        self.refused.extend(other.refused);
        self.examples_retired += other.examples_retired;
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OperationPoint {
    BeforeSchemaSetup,
    AcceptanceCommitted,
}

/// Pause the actual owner at a persistence boundary; no replacement storage or
/// dispatch implementation participates in cancellation regressions.
#[cfg(test)]
pub(in crate::modules::training_trigger) struct OperationPause {
    pub(super) point: OperationPoint,
    pub(super) entered: tokio::sync::Notify,
    pub(super) resume: tokio::sync::Notify,
}

#[cfg(test)]
impl TrainingTriggerState {
    pub(super) async fn pause_operation(&self, point: OperationPoint) {
        let pause = {
            let mut slot = self.operation_pause.lock().unwrap();
            if slot.as_ref().is_some_and(|pause| pause.point == point) {
                slot.take()
            } else {
                None
            }
        };
        if let Some(pause) = pause {
            pause.entered.notify_one();
            pause.resume.notified().await;
        }
    }
}
