//! The declared indexes of a collection, applied over the table as it stands.
//!
//! One owner for index DDL: `do_ensure_schema` calls it after creating a declared
//! table, and [`SqliteAdapter::ensure_declared_indexes`] calls it over tables that
//! writes created lazily from row data (`ensure_table_exists`). A lazily created
//! table never saw its declared schema, so it carried only its primary key:
//! `chat_messages` (103,319 rows on the M5, 2026-10-26) answered every room page
//! with a full SCAN plus a temp B-tree, 27-29 ms, because the declared
//! `idx_chat_messages_room_timestamp` never reached the database.
use super::*;

/// What one pass did for one collection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IndexPass {
    /// Indexes whose columns all exist, ensured with `IF NOT EXISTS`.
    pub ensured: usize,
    /// Declared indexes over a column the table does not have yet (a lazily created
    /// table gains a column on the first write that carries it); the next pass
    /// ensures them.
    pub awaiting_columns: usize,
}

/// Every index `schema` declares (single-field `indexed` fields and composites), for
/// the columns `table` has. A missing table is not an error: nothing was written yet.
pub(super) fn do_ensure_indexes(conn: &Connection, schema: &CollectionSchema) -> Result<IndexPass, String> {
    let table = naming::to_table_name(&schema.collection);
    let columns: Vec<String> = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(|e| format!("Read columns of {table} failed: {e}"))?;
    let mut pass = IndexPass::default();
    if columns.is_empty() {
        return Ok(pass);
    }

    // BaseEntity columns are skipped: `id` is the PRIMARY KEY (implicitly indexed)
    // and the adapter owns the timestamp/version columns.
    let single = schema.fields.iter().filter(|f| f.indexed).filter_map(|f| {
        let col = naming::to_snake_case(&f.name);
        (!super::super::entity::is_base_entity_column(&col))
            .then(|| (format!("idx_{table}_{col}"), vec![col], false))
    });
    let composite = schema.indexes.iter().map(|index| {
        let cols = index.fields.iter().map(|f| naming::to_snake_case(f)).collect();
        (naming::to_snake_case(&index.name), cols, index.unique)
    });

    for (name, cols, unique) in single.chain(composite) {
        if !cols.iter().all(|c| columns.contains(c)) {
            pass.awaiting_columns += 1;
            continue;
        }
        let unique = if unique { "UNIQUE " } else { "" };
        let sql = format!("CREATE {unique}INDEX IF NOT EXISTS {name} ON {table} ({})", cols.join(", "));
        conn.execute(&sql, [])
            .map_err(|e| format!("Create index {name} on {table} failed: {e}"))?;
        pass.ensured += 1;
    }
    Ok(pass)
}

impl SqliteAdapter {
    /// Apply each schema's declared indexes over the tables that exist. Run when the
    /// data layer opens a store: collections written by rows rather than through
    /// `ensure_schema` otherwise never get the indexes their entity declares.
    pub async fn ensure_declared_indexes(&self, schemas: Vec<CollectionSchema>) -> Result<IndexPass, String> {
        let conn = self.get_writer()?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|_| "SQLite writer lock poisoned".to_string())?;
            schemas.iter().try_fold(IndexPass::default(), |total, schema| {
                let pass = do_ensure_indexes(&conn, schema)?;
                Ok(IndexPass {
                    ensured: total.ensured + pass.ensured,
                    awaiting_columns: total.awaiting_columns + pass.awaiting_columns,
                })
            })
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
    }
}
