//! The sequenced append: a row numbered by a commit-ordered sequence, deduplicated by a unique
//! field. Runs only inside [`do_batch`](super::open_connection)'s `BEGIN IMMEDIATE` transaction
//! on the single writer connection, so the `max + 1` read and the insert can never interleave
//! with another writer: sequence order is commit order. Numbers come from a durable high-water,
//! so pruning rows never re-issues one.

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};

use super::value_to_sql_boxed;
use crate::orm::adapter::naming;
use crate::orm::types::{DataRecord, SequenceSpec, Sequenced};

/// Append `record` under `spec`; the caller is inside the batch transaction.
pub(super) fn append_sequenced(
    conn: &Connection,
    mut record: DataRecord,
    spec: &SequenceSpec,
) -> Result<Sequenced, String> {
    let table = naming::to_table_name(&record.collection);
    let seq_col = naming::to_snake_case(&spec.sequence);
    let unique_col = naming::to_snake_case(&spec.unique);
    let unique_value = record.data.get(&spec.unique).ok_or_else(|| {
        format!(
            "append: the record has no `{}` to deduplicate by",
            spec.unique
        )
    })?;
    let existing: Option<i64> = conn
        .query_row(
            &format!("SELECT {seq_col} FROM {table} WHERE {unique_col} = ?1"),
            [value_to_sql_boxed(unique_value)],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("append: reading the existing row failed: {e}"))?;
    if let Some(seq) = existing {
        return Ok(Sequenced {
            seq: seq as u64,
            fresh: false,
        });
    }
    // The next sequence comes from a durable per-collection high-water that only rises, never from
    // MAX over surviving rows: a pruned newest row (or a collection pruned to empty) must not
    // re-issue a sequence a cursor already passed (Cormac on #5694). First use seeds it from any
    // existing rows. Same transaction as the insert, so a rollback returns the number.
    conn.execute_batch("CREATE TABLE IF NOT EXISTS orm_sequence_heads (collection TEXT PRIMARY KEY, high INTEGER NOT NULL)")
        .map_err(|e| format!("append: preparing the sequence heads failed: {e}"))?;
    let next: i64 = conn
        .query_row(
            &format!(
                "INSERT INTO orm_sequence_heads (collection, high) VALUES (?1, (SELECT COALESCE(MAX({seq_col}), 0) + 1 FROM {table})) \
                 ON CONFLICT(collection) DO UPDATE SET high = high + 1 RETURNING high"
            ),
            [&table],
            |row| row.get(0),
        )
        .map_err(|e| format!("append: advancing the sequence failed: {e}"))?;
    match &mut record.data {
        Value::Object(fields) => {
            fields.insert(spec.sequence.clone(), json!(next));
        }
        _ => return Err("append: the record's data is not an object".to_string()),
    }
    let created = super::do_create(conn, record, None);
    if !created.success {
        return Err(created
            .error
            .unwrap_or_else(|| "append: create failed".to_string())); // the adapter reported failure without detail; the operation still failed
    }
    Ok(Sequenced {
        seq: next as u64,
        fresh: true,
    })
}

/// The store-owned revision of each sequenced collection: the highest sequence committed. It is
/// published by the blocking write itself, right after `COMMIT` and before that write returns, so
/// a caller dropped at the commit await cannot strand a committed row unannounced: the bump has
/// the commit's custody, not the caller's. A consumer treats it as a hint and drains the store
/// from its own cursor; a bump naming a sequence it already drained is a no-op.
#[derive(Default)]
pub(crate) struct SequenceRevisions(
    std::sync::Mutex<std::collections::HashMap<String, tokio::sync::watch::Sender<u64>>>,
);

impl SequenceRevisions {
    fn sender(&self, collection: &str) -> tokio::sync::watch::Sender<u64> {
        let mut senders = self.0.lock().unwrap_or_else(|p| p.into_inner()); // poisoned = read the last map; a revision is a hint, never lost data
        senders
            .entry(collection.to_string())
            .or_insert_with(|| tokio::sync::watch::channel(0).0)
            .clone()
    }

    /// A receiver that wakes when `collection`'s committed high-water rises.
    pub(crate) fn subscribe(&self, collection: &str) -> tokio::sync::watch::Receiver<u64> {
        self.sender(collection).subscribe()
    }

    /// Raise `collection`'s high-water to `seq` (never lowers it).
    pub(super) fn publish(&self, collection: &str, seq: u64) {
        self.sender(collection).send_if_modified(|high| {
            let rises = seq > *high;
            if rises {
                *high = seq;
            }
            rises
        });
    }
}
