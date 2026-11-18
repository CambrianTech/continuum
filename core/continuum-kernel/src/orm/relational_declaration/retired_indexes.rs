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


    // Reproduce the old writer's catalog independently of the migration descriptors.
    async fn reopen_legacy_catalog(unexpected: bool) -> Result<(), Box<dyn std::error::Error>> {
        use crate::orm::{BaseEntity, Entity, OrmEntity};
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, Entity)]
        #[entity(collection = "chat_messages")]
        struct Chat {
            #[serde(flatten)]
            base: BaseEntity,
            room_id: String,
        }

        #[derive(Serialize, Deserialize, Entity)]
        #[entity(collection = "memories")]
        struct Memory {
            #[serde(flatten)]
            base: BaseEntity,
            persona_id: String,
            timestamp: String,
            importance: i64,
        }

        let directory = tempfile::tempdir()?;
        let file = directory.path().join("catalog.db");
        let config = AdapterConfig { connection_string: file.to_str().ok_or("test path encoding")?.into(), ..Default::default() };
        let original = RelationalStore::open(&config).await?;
        original.ensure(&Chat::collection_schema()).await?;
        original.ensure(&Memory::collection_schema()).await?;
        for ddl in [
            "CREATE INDEX idx_chat_messages_room_id ON chat_messages (room_id)",
            "CREATE INDEX idx_memories_persona_id ON memories (persona_id)",
            "CREATE INDEX idx_memories_timestamp ON memories (timestamp)",
            "CREATE INDEX idx_memories_persona_importance ON memories (persona_id, importance)",
            "CREATE INDEX idx_memories_persona_timestamp ON memories (persona_id, timestamp)",
        ] {
            original.db.execute_unprepared(ddl).await?;
        }
        if unexpected {
            original.db.execute_unprepared("CREATE INDEX unexpected ON memories (importance)").await?;
        }
        let reopened = RelationalStore::open(&config).await;
        if unexpected {
            assert!(reopened.is_err(), "an undeclared index must refuse the reopen");
        } else {
            reopened?.db.close().await?;
        }
        let indexes = original.db.query_all(Statement::from_string(DbBackend::Sqlite,
            "SELECT name FROM sqlite_schema WHERE type='index' AND name IN ('idx_chat_messages_room_id','idx_memories_persona_id','idx_memories_timestamp','idx_memories_persona_importance','idx_memories_persona_timestamp','unexpected') ORDER BY name")).await?;
        let actual: Vec<String> = indexes.iter().map(|row| row.try_get("", "name")).collect::<Result<_, _>>()?;
        if unexpected {
            assert_eq!(actual, ["idx_chat_messages_room_id", "idx_memories_persona_id", "idx_memories_persona_importance",
                "idx_memories_persona_timestamp", "idx_memories_timestamp", "unexpected"]);
        } else {
            assert!(actual.is_empty(), "retired indexes survived reopen: {actual:?}");
        }
        original.db.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn reopen_drops_only_the_five_retired_indexes() -> Result<(), Box<dyn std::error::Error>> {
        reopen_legacy_catalog(false).await
    }

    #[tokio::test]
    async fn unexpected_index_rolls_back_retired_indexes_across_tables() -> Result<(), Box<dyn std::error::Error>> {
        reopen_legacy_catalog(true).await
    }

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
