//! Signature inputs without lived payload; count-only readers use pending_counts.
use super::*;
use crate::genome::fine_tuning::types::{LoRAHyperparams, ScheduleParams, TrainingExample};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArrivalSummary {
    pub persona_name: String,
    pub examples: Vec<TrainingExample>,
    pub lora: Option<LoRAHyperparams>,
    pub schedule: Option<ScheduleParams>,
}
impl ArrivalSummary {
    pub(super) fn of(batch: &PendingBatch) -> Self {
        Self {
            persona_name: batch.persona_name.clone(),
            // Signature minting uses prompt/completion only. Construct those fields
            // directly: cloning the full example first would copy its lived payload.
            examples: batch.examples.iter().map(|e| TrainingExample {
                prompt: e.prompt.clone(), completion: e.completion.clone(),
                metadata: None, lived: None,
            }).collect(),
            lora: batch.lora.clone(), schedule: batch.schedule.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ArrivalHeader {
    pub id: Uuid,
    pub sequence: u64,
    pub persona_id: Uuid,
    pub trait_kind: String,
    pub base_model: String,
    // Match Submission: rows predating in-place learning have no committed examples.
    #[serde(default)]
    pub learned: u32,
    pub arrival_summary: Option<ArrivalSummary>,
}
impl DurableStore {
    /// Explicit migration of one immutable journal body, then a metadata-only ack.
    /// Never keeps a page of old bodies alive or rewrites their training payload.
    pub(super) async fn migrate_arrival_summary(&self, id: Uuid) -> Result<ArrivalSummary, String> {
        let row = self.submissions.find_by_id(id).await.map_err(|e| e.to_string())?
            .ok_or_else(|| format!("submission {id} disappeared during summary migration"))?;
        let summary = ArrivalSummary::of(&row.batch);
        drop(row);
        let result = self.adapter.batch(vec![crate::orm::types::BatchOperation {
            operation_type: crate::orm::types::BatchOperationType::Update,
            collection: Submission::COLLECTION.into(), id: Some(id.to_string()),
            data: Some(serde_json::json!({"arrivalSummary": summary})),
        }]).await;
        if !result.success { return Err(format!("submission {id} summary migration failed: {:?}", result.error)); }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::types::{BatchOperation, BatchOperationType};

    #[test]
    fn projected_header_reads_pre_learning_rows_without_inventing_progress() {
        let (id, row) = super::super::arrivals::tests::submission(1, 2);
        let mut header = serde_json::json!({
            "id": id, "sequence": row.sequence, "personaId": row.persona_id,
            "traitKind": row.trait_kind, "baseModel": row.base_model,
            "learned": null, "arrivalSummary": null,
        });
        let decoded: ArrivalHeader = crate::orm::record_decode::decode_row(header.clone())
            .expect("test: historical NULL learned column");
        assert_eq!(decoded.learned, 0);
        header["learned"] = serde_json::json!(2);
        let decoded: ArrivalHeader = crate::orm::record_decode::decode_row(header.clone())
            .expect("test: committed progress");
        assert_eq!(decoded.learned, 2);
        header["sequence"] = serde_json::Value::Null;
        assert!(crate::orm::record_decode::decode_row::<ArrivalHeader>(header).is_err());
    }

    #[tokio::test]
    async fn discovery_does_not_decode_the_journal_body() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: schema");
        let (id, row) = super::super::arrivals::tests::submission(1, 2);
        store.submissions.save(id, &row).await.expect("test: save");
        let changed = store.adapter.batch(vec![BatchOperation {
            operation_type: BatchOperationType::Update,
            collection: Submission::COLLECTION.into(), id: Some(id.to_string()),
            data: Some(serde_json::json!({"batch": "intentionally invalid body"})),
        }]).await;
        assert!(changed.success);
        let arrivals = store.arrivals(0, 256).await.expect("metadata discovery must not decode batch");
        assert_eq!(arrivals.len(), 1);
        assert_eq!(arrivals[0].examples.len(), 2);
        assert!(store.submission_examples(id).await.is_err(), "the actual body remains strictly validated when requested");
    }

    #[tokio::test]
    async fn old_row_migrates_summary_without_changing_the_payload() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: schema");
        let (id, mut row) = super::super::arrivals::tests::submission(1, 2);
        row.arrival_summary = None;
        let before = serde_json::to_value(&row.batch).expect("test: batch");
        store.submissions.save(id, &row).await.expect("test: save");
        let arrivals = store.arrivals(0, 256).await.expect("test: migrate old summary column");
        assert_eq!(arrivals.len(), 1);
        assert_eq!(arrivals[0].learned, 0);
        let after = store.submissions.find_by_id(id).await.expect("test: read").expect("test: present");
        assert!(after.arrival_summary.is_some());
        assert_eq!(before, serde_json::to_value(after.batch).expect("test: unchanged batch"));
    }
}
