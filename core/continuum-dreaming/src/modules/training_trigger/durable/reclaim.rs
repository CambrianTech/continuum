//! The batch path's open dispatches, taken back (card 4114a2c6): a migration, run at start.
//!
//! Before in-place learning, a full bucket's submissions were assigned to a dispatch intent and
//! left pending=false while a batch job trained them. That path is gone, so an intent still active
//! names examples nothing will learn: each of its submissions that the intent still holds goes back
//! to pending, where in-place learning takes it, and the intent is closed. A submission retired or
//! held by another dispatch is left as it is. Idempotent: a second run finds no active intent. An
//! example whose job had already trained may be learned once more; none is lost.

use super::*;

impl DurableStore {
    /// Return every active dispatch's submissions to pending and close the intent, then every row a
    /// closed intent left stranded; the count of submissions returned.
    pub(in crate::modules::training_trigger) async fn reclaim_dispatched(&self) -> Result<usize, String> {
        let mut returned = 0;
        // the active-intent index only, a page at a time: each intent closed here leaves the index,
        // so the next page is the next active ones, and history is never scanned
        loop {
        let result = self.adapter.query(QueryBuilder::new(DispatchIntent::COLLECTION).filter_eq("isActive", true).limit(64).build()).await;
        if !result.success {
            return Err(result.error.unwrap_or_else(|| "active dispatch query refused".into())); // the query failed; this supplies its missing diagnostic, never rows
        }
        let rows = result.data.ok_or("active dispatch query omitted rows")?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            let mut intent: DispatchIntent = row.decode().map_err(|e| e.to_string())?; // decode the persisted ORM row into its typed owner
            let id = entity_id(&intent.base)?;
            for submission in &intent.submission_ids {
                let Some(mut row) = self.submissions.find_by_id(*submission).await.map_err(|e| e.to_string())? else {
                    continue; // a purged submission: nothing left to learn
                };
                if row.dispatch_id == Some(id) && row.retirement_id.is_none() {
                    row.is_pending = true;
                    row.dispatch_id = None;
                    self.submissions.update(*submission, &row).await.map_err(|e| e.to_string())?;
                    returned += 1;
                }
            }
            intent.is_active = false;
            self.dispatches.update(id, &intent).await.map_err(|e| e.to_string())?;
        }
        }
        Ok(returned + self.reclaim_stranded().await?)
    }

    /// Return every submission still naming a dispatch once no intent is active: the batch path
    /// closed some intents without handing their rows back (the M5's code/owner rows on intent
    /// 1d75cba2, card 997f7177), and nothing else writes `dispatch_id`, so after
    /// [`Self::reclaim_dispatched`] each such row is stranded. A learned row names no dispatch and
    /// a retired one keeps its receipt; neither is touched. Each row returned leaves the filter, so
    /// the next page is the next stranded ones.
    async fn reclaim_stranded(&self) -> Result<usize, String> {
        let mut returned = 0;
        loop {
            let result = self
                .adapter
                .query(
                    QueryBuilder::new(Submission::COLLECTION)
                        .filter_eq("isPending", false)
                        .filter("dispatchId", QueryOperator::IsNotNull)
                        .filter("retirementId", QueryOperator::IsNull)
                        .limit(64)
                        .build(),
                )
                .await;
            if !result.success {
                return Err(result.error.unwrap_or_else(|| "stranded submission query refused".into())); // the query failed; this supplies its missing diagnostic, never rows
            }
            let rows = result.data.ok_or("stranded submission query omitted rows")?;
            if rows.is_empty() {
                return Ok(returned);
            }
            for row in rows {
                let mut submission: Submission = row.decode().map_err(|e| e.to_string())?; // decode the persisted ORM row into its typed owner
                let id = entity_id(&submission.base)?;
                submission.is_pending = true;
                submission.dispatch_id = None;
                self.submissions.update(id, &submission).await.map_err(|e| e.to_string())?;
                returned += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::training_trigger::durable::arrivals::tests::submission;

    fn intent(submissions: &[Uuid], active: bool) -> (Uuid, DispatchIntent) {
        let base = BaseEntity::for_new_record();
        let id = entity_id(&base).expect("test: a fresh entity id");
        let row = DispatchIntent {
            base,
            persona_id: Uuid::nil(),
            trait_kind: "code".into(),
            base_model: "qwen".into(),
            submission_ids: submissions.to_vec(),
            phase: serde_json::json!({}),
            is_active: active,
            attempt: None,
        };
        (id, row)
    }

    // what this catches (card 997f7177, the M5 2026-11-11): a reviewed correction stranded forever
    // because its rows name a dispatch the batch path closed without handing them back; and the
    // repair taking a learned or retired row, which must keep its state.
    #[tokio::test]
    async fn rows_held_by_an_open_or_a_closed_dispatch_return_to_pending_and_nothing_else_moves() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: store");
        let (open_row, mut open) = submission(1, 2);
        let (stranded_row, mut stranded) = submission(2, 2);
        let (learned_row, mut learned) = submission(3, 2);
        let (retired_row, mut retired) = submission(4, 2);
        let (open_id, open_intent) = intent(&[open_row], true);
        let (closed_id, closed_intent) = intent(&[stranded_row, retired_row], false);
        for (row, dispatch) in [(&mut open, Some(open_id)), (&mut stranded, Some(closed_id)), (&mut retired, Some(closed_id))] {
            row.is_pending = false;
            row.dispatch_id = dispatch;
        }
        retired.retirement_id = Some(Uuid::new_v4());
        learned.is_pending = false;
        learned.learned = 2;
        for (id, row) in [(open_row, &open), (stranded_row, &stranded), (learned_row, &learned), (retired_row, &retired)] {
            store.submissions.save(id, row).await.expect("test: saved");
        }
        store.dispatches.save(open_id, &open_intent).await.expect("test: open intent");
        store.dispatches.save(closed_id, &closed_intent).await.expect("test: closed intent");

        assert_eq!(store.reclaim_dispatched().await.expect("test: reclaimed"), 2, "the open intent's row and the stranded row");
        let state = |id| {
            let store = &store;
            async move {
                let row = store.submissions.find_by_id(id).await.expect("test: read").expect("test: present");
                (row.is_pending, row.dispatch_id.is_some())
            }
        };
        assert_eq!(state(open_row).await, (true, false));
        assert_eq!(state(stranded_row).await, (true, false));
        assert_eq!(state(learned_row).await, (false, false), "a learned row stays learned");
        assert_eq!(state(retired_row).await, (false, true), "a retired row keeps its receipt");
        assert_eq!(store.reclaim_dispatched().await.expect("test: second pass"), 0, "idempotent");
    }
}
