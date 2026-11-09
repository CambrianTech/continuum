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
    /// Return every active dispatch's submissions to pending and close the intent; the count of
    /// submissions returned.
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
            return Ok(returned);
        }
        for row in rows {
            let mut intent: DispatchIntent = serde_json::from_value(row.data).map_err(|e| e.to_string())?; // decode the persisted ORM row into its typed owner
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
    }
}
