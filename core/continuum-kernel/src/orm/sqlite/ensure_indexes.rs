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

/// A declared index the database refused to build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexFailure {
    /// The entity's collection.
    pub collection: String,
    /// The declared index name.
    pub index: String,
    /// The database's error, verbatim.
    pub error: String,
}

/// What a pass did. Indexes are performance, not correctness: one that cannot be
/// built is counted and reported, and the store stays open without it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexPass {
    /// Indexes the schemas declare.
    pub declared: usize,
    /// Indexes whose columns all exist, ensured with `IF NOT EXISTS`.
    pub ensured: usize,
    /// Declared indexes over a column the table does not have yet (a lazily created
    /// table gains a column on the first write that carries it); the next pass
    /// ensures them.
    pub awaiting_columns: usize,
    /// Indexes the database refused to build (or whose table could not be read).
    pub failed: Vec<IndexFailure>,
}

impl IndexPass {
    fn absorb(&mut self, other: IndexPass) {
        self.declared += other.declared;
        self.ensured += other.ensured;
        self.awaiting_columns += other.awaiting_columns;
        self.failed.extend(other.failed);
    }
}

/// Every index `schema` declares (single-field `indexed` fields and composites), for
/// the columns `table` has. A missing table is not an error: nothing was written yet.
/// A failure is recorded per index and the pass continues with the next one.
pub(super) fn do_ensure_indexes(conn: &Connection, schema: &CollectionSchema) -> IndexPass {
    let table = naming::to_table_name(&schema.collection);
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
    let declared: Vec<(String, Vec<String>, bool)> = single.chain(composite).collect();
    let mut pass = IndexPass { declared: declared.len(), ..IndexPass::default() };
    let failure = |index: &str, error: String| IndexFailure {
        collection: schema.collection.clone(),
        index: index.to_string(),
        error,
    };

    let columns = match conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .and_then(|mut stmt| stmt.query_map([], |row| row.get::<_, String>(1))?.collect::<Result<Vec<_>, _>>())
    {
        Ok(columns) => columns,
        Err(e) => {
            let error = format!("read columns of {table}: {e}");
            pass.failed.extend(declared.iter().map(|(name, _, _)| failure(name, error.clone())));
            return pass;
        }
    };
    if columns.is_empty() {
        return pass;
    }

    for (name, cols, unique) in declared {
        if !cols.iter().all(|c| columns.contains(c)) {
            pass.awaiting_columns += 1;
            continue;
        }
        let unique = if unique { "UNIQUE " } else { "" };
        let sql = format!("CREATE {unique}INDEX IF NOT EXISTS {name} ON {table} ({})", cols.join(", "));
        match conn.execute(&sql, []) {
            Ok(_) => pass.ensured += 1,
            Err(e) => pass.failed.push(failure(&name, e.to_string())),
        }
    }
    pass
}

impl SqliteAdapter {
    /// Apply each schema's declared indexes over the tables that exist. Run when the
    /// data layer opens a store: collections written by rows rather than through
    /// `ensure_schema` otherwise never get the indexes their entity declares. Never
    /// fails: what could not be built is in `failed`, and the store serves without it.
    pub async fn ensure_declared_indexes(&self, schemas: Vec<CollectionSchema>) -> IndexPass {
        let declared: usize = schemas.iter().map(|s| s.indexes.len() + s.fields.iter().filter(|f| f.indexed).count()).sum();
        let unavailable = |error: String| IndexPass {
            declared,
            failed: vec![IndexFailure { collection: "*".to_string(), index: "*".to_string(), error }],
            ..IndexPass::default()
        };
        let conn = match self.get_writer() {
            Ok(conn) => conn,
            Err(e) => return unavailable(e),
        };
        let pass = tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|_| "SQLite writer lock poisoned".to_string())?;
            Ok::<_, String>(schemas.iter().fold(IndexPass::default(), |mut total, schema| {
                total.absorb(do_ensure_indexes(&conn, schema));
                total
            }))
        })
        .await;
        match pass {
            Ok(Ok(pass)) => pass,
            Ok(Err(e)) => unavailable(e),
            Err(e) => unavailable(format!("spawn_blocking failed: {e}")),
        }
    }
}
