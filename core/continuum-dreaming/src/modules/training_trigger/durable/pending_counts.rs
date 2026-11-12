//! One count-only projection for status and bucket admission. Training text is
//! not inspection metadata; old rows are migrated individually, without a page
//! of historical bodies resident at once.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PendingCount {
    persona_name: String,
    examples: u64,
}

impl PendingCount {
    pub(super) fn of(batch: &PendingBatch) -> Self {
        Self { persona_name: batch.persona_name.clone(), examples: batch.examples.len() as u64 }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    id: Uuid,
    sequence: u64,
    persona_id: Uuid,
    trait_kind: String,
    base_model: String,
    #[serde(default)]
    learned: u32,
    pending_count: Option<PendingCount>,
}

pub(super) struct PendingCountView {
    pub sequence: u64,
    pub key: BucketKey,
    pub persona_name: String,
    pub unlearned: u32,
}

impl DurableStore {
    pub(super) async fn pending_counts(&self, key: Option<&BucketKey>, after: u64, limit: usize) -> Result<Vec<PendingCountView>, String> {
        let query = match key {
            Some(key) => key.query(Submission::COLLECTION),
            None => QueryBuilder::new(Submission::COLLECTION),
        };
        let records = self.adapter.query(query.filter_eq("isPending", true)
            .filter("sequence", QueryOperator::Gt(after.into())).sort_asc("sequence").limit(limit)
            .select(["sequence", "personaId", "traitKind", "baseModel", "learned", "pendingCount"].into_iter().map(String::from).collect())
            .build()).await.into_result()?;
        let mut counts = Vec::with_capacity(records.len());
        for record in records {
            let header: Header = record.decode().map_err(|e| e.to_string())?;
            let count = match header.pending_count {
                Some(count) => count,
                None => {
                    let row = self.submissions.find_by_id(header.id).await.map_err(|e| e.to_string())?
                        .ok_or_else(|| format!("submission {} disappeared during count migration", header.id))?;
                    let count = PendingCount::of(&row.batch);
                    drop(row);
                    self.adapter.batch(vec![crate::orm::types::BatchOperation {
                        operation_type: crate::orm::types::BatchOperationType::Update,
                        collection: Submission::COLLECTION.into(), id: Some(header.id.to_string()),
                        data: Some(serde_json::json!({"pendingCount": count})),
                    }]).await.into_result()?;
                    count
                }
            };
            let remaining = count.examples.checked_sub(u64::from(header.learned)).ok_or("learned count exceeds submitted examples")?;
            counts.push(PendingCountView {
                sequence: header.sequence,
                key: BucketKey { persona_id: header.persona_id, trait_kind: header.trait_kind, base_model: header.base_model },
                persona_name: count.persona_name,
                unlearned: u32::try_from(remaining).map_err(|e| e.to_string())?,
            });
        }
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::types::{BatchOperation, BatchOperationType};

    #[test]
    fn legacy_null_progress_is_zero_without_defaulting_identity() {
        let mut value = serde_json::json!({
            "id": Uuid::nil(), "sequence": 1, "personaId": Uuid::nil(),
            "traitKind": "code", "baseModel": "base", "learned": null,
            "pendingCount": {"personaName": "mind", "examples": 3},
        });
        let header: Header = crate::orm::record_decode::decode_row(value.clone()).expect("old NULL progress");
        assert_eq!(header.learned, 0);
        value["personaId"] = serde_json::Value::Null;
        assert!(crate::orm::record_decode::decode_row::<Header>(value).is_err());
    }

    #[tokio::test]
    async fn counts_migrate_once_then_ignore_all_training_text() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("schema");
        let (id, mut row) = super::super::arrivals::tests::submission(1, 3);
        row.pending_count = None;
        row.learned = 1;
        let before = serde_json::to_value(&row.batch).expect("batch");
        store.submissions.save(id, &row).await.expect("save");
        let page = store.pending_counts(None, 0, 1).await.expect("migrate");
        assert_eq!(page[0].unlearned, 2);
        let saved = store.submissions.find_by_id(id).await.expect("read").expect("row");
        assert_eq!(serde_json::to_value(saved.batch).expect("batch"), before);
        store.adapter.batch(vec![BatchOperation {
            operation_type: BatchOperationType::Update, collection: Submission::COLLECTION.into(),
            id: Some(id.to_string()), data: Some(serde_json::json!({"batch": "bad body", "arrivalSummary": "bad text summary"})),
        }]).await.into_result().expect("damage only payload");
        assert_eq!(store.pending_counts(None, 0, 1).await.expect("count without body")[0].unlearned, 2);
        assert_eq!(store.unlearned_in(&page[0].key).await.expect("bucket count without body"), 2);
        assert!(store.pending_counts(None, 1, 1).await.expect("cursor").is_empty());
        assert!(store.submission_examples(id).await.is_err());
    }
}
