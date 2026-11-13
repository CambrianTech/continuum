//! A detached committing task owns the transaction and its revision publication.
use super::{
    query::QueryBuilder,
    types::{BatchOperation, BatchOperationType, DataRecord, RecordMetadata},
};
use serde_json::{json, Value};

impl super::relational::RelationalStore {
    pub async fn batch(&self, operations: Vec<BatchOperation>) -> Result<Vec<Value>, String> {
        self.write_with_revisions(move |store, transaction| {
            Box::pin(async move {
                let mut results = Vec::with_capacity(operations.len());
                let mut committed = Vec::new();
                for op in operations {
                    let id = op.id.ok_or("batch operation requires an id")?;
                    let result = match op.operation_type {
                        BatchOperationType::Read => {
                            let row = store
                                .read_on(transaction, &op.collection, &id)
                                .await?
                                .ok_or_else(|| format!("Record not found: {id}"))?;
                            json!({"success":true, "data":row})
                        }
                        BatchOperationType::Delete => {
                            let deleted = store
                                .delete_on(
                                    transaction,
                                    QueryBuilder::new(&op.collection)
                                        .filter_eq("id", id)
                                        .build(),
                                )
                                .await?;
                            json!({"success":true,"deleted":deleted != 0})
                        }
                        BatchOperationType::Update => {
                            if !store
                                .update_on(
                                    transaction,
                                    &op.collection,
                                    &id,
                                    op.data.ok_or("update requires data")?,
                                    true,
                                )
                                .await?
                            {
                                return Err(format!(
                                    "update row {}:{id} does not exist",
                                    op.collection
                                ));
                            }
                            json!({"success":true})
                        }
                        kind => {
                            let record = DataRecord {
                                id,
                                collection: op.collection,
                                data: op.data.ok_or("write requires data")?,
                                metadata: RecordMetadata::default(),
                            };
                            match kind {
                                BatchOperationType::Create => {
                                    store.insert(transaction, &record).await?;
                                    json!({"success":true,"data":record})
                                }
                                BatchOperationType::Ensure => {
                                    let created = store.ensure_on(transaction, &record).await?;
                                    json!({"success":true,"created":created})
                                }
                                BatchOperationType::Append(spec) => {
                                    let collection = record.collection.clone();
                                    let receipt =
                                        store.append_on(transaction, record, &spec).await?;
                                    committed.push((collection, receipt.seq));
                                    json!({"success":true,"seq":receipt.seq,"fresh":receipt.fresh})
                                }
                                _ => return Err("invalid batch write classification".into()),
                            }
                        }
                    };
                    results.push(result);
                }
                Ok((results, committed))
            })
        })
        .await
    }
}
