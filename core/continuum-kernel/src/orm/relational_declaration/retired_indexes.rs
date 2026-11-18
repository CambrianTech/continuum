//! Forward migration of secondary indexes created by the superseded SQLite writer.
//! These descriptors reproduce that writer's exact catalog definitions. An index
//! still named by the recorded declaration remains in its owner's custody.
use super::{catalog, verify_physical, Model, SchemaError};
use sea_orm::{ConnectionTrait, DbBackend, sea_query::Index};

struct RetiredIndex {
    table: &'static str,
    name: &'static str,
    columns: &'static str,
}

impl RetiredIndex {
    fn matches(&self, row: &catalog::Model) -> bool {
        row.tbl_name == self.table && row.name == self.name && row.r#type == "index"
            && row.sql.as_deref() == Some(&format!("CREATE INDEX {} ON {} ({})", self.name, self.table, self.columns))
    }
}

// The prior TypeScript entity declarations and the prior adapter's generated
// field indexes. None is a constraint or contains an expression or predicate.
const RETIRED: &[RetiredIndex] = &[
    RetiredIndex { table: "chat_messages", name: "idx_chat_messages_room_id", columns: "room_id" },
    RetiredIndex { table: "memories", name: "idx_memories_persona_id", columns: "persona_id" },
    RetiredIndex { table: "memories", name: "idx_memories_timestamp", columns: "timestamp" },
    RetiredIndex { table: "memories", name: "idx_memories_persona_importance", columns: "persona_id, importance" },
    RetiredIndex { table: "memories", name: "idx_memories_persona_timestamp", columns: "persona_id, timestamp" },
];

/// Both callers hold the same transaction through the comparison and DDL.
pub(super) async fn reconcile<C: ConnectionTrait>(
    db: &C, receipt: &Model, actual: &[catalog::Model],
) -> Result<(), SchemaError> {
    let Err(mismatch) = verify_physical(receipt, actual) else { return Ok(()); };
    let Some(recorded) = receipt.physical.as_array() else { return Err(mismatch); };
    if actual.len() <= recorded.len() { return Err(mismatch); }
    let (retired, retained): (Vec<_>, Vec<_>) = actual.iter().cloned().partition(|row| {
        RETIRED.iter().any(|index| index.matches(row))
            && !recorded.iter().any(|entry| entry.get("name").and_then(|name| name.as_str()) == Some(row.name.as_str()))
    });
    // The exact original proof must hold before any mutation. All unexpected
    // columns, constraints, indexes and triggers continue to refuse admission.
    verify_physical(receipt, &retained)?;
    for row in retired {
        let drop = Index::drop().name(row.name).to_owned();
        db.execute(DbBackend::Sqlite.build(&drop)).await?;
    }
    Ok(())
}

#[cfg(test)]
mod replay {
    use crate::orm::{adapter::AdapterConfig, relational::RelationalStore};

    /// An explicit local replay against a disposable copy of an actual store.
    #[tokio::test]
    #[ignore = "supply a disposable SQLite backup via CONTINUUM_INDEX_REPLAY_COPY"]
    async fn open_real_index_replay_copy() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::var("CONTINUUM_INDEX_REPLAY_COPY")?;
        let config = AdapterConfig { connection_string: path, ..Default::default() };
        for _ in 0..2 {
            let start = std::time::Instant::now();
            let store = RelationalStore::open(&config).await?;
            store.db.close().await?;
            eprintln!("store open and close: {:?}", start.elapsed());
        }
        Ok(())
    }
}
