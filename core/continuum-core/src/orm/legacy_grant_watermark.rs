//! One-time import of the pre-ORM grant epoch watermark (card bbc5b0b6).
//!
//! Before the watermark was an ORM entity it was a hand-rolled table in the same file,
//! `grant_epoch_watermark (grantee TEXT PRIMARY KEY, epoch INTEGER, updated_at_ms
//! INTEGER)`. Starting a node with an empty watermark would reopen the replay window the
//! watermark exists to close, so the first open of the ORM store carries every legacy row
//! into the entity's table — keeping the HIGHER epoch where a row already exists — and
//! drops the legacy table, all in ONE transaction. A row that cannot be carried exactly
//! (an unparseable grantee, a negative or non-integer epoch, one past the exact range)
//! aborts the whole import: the store refuses to open rather than start empty.
//!
//! SQL lives here because the ORM is the boundary SQL may not cross; nothing outside
//! `orm/` names a table.

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use super::sqlite::SqliteAdapter;

/// The legacy table's name, exactly as the pre-ORM store created it.
const LEGACY_TABLE: &str = "grant_epoch_watermark";

/// Why the legacy watermark could not be carried into the ORM store. Every variant
/// refuses the open: losing the anti-replay state must never look like a fresh start.
#[derive(Debug, Clone, thiserror::Error)]
pub enum LegacyWatermarkImportError {
    #[error("the SQLite adapter is not open: {0}")]
    Adapter(String),
    #[error("legacy watermark import failed in SQLite: {0}")]
    Sqlite(String),
    #[error("legacy watermark row {grantee:?} cannot be carried exactly: {reason}")]
    CorruptRow { grantee: String, reason: String },
    #[error("legacy watermark import task did not finish: {0}")]
    Join(String),
}

impl From<rusqlite::Error> for LegacyWatermarkImportError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e.to_string())
    }
}

/// Carry the legacy table in `adapter`'s file into the entity table `collection` (which
/// the caller's `OrmStore::new` has already created), then drop the legacy table — one
/// transaction. `Ok(None)` when there is no legacy table; `Ok(Some(rows))` when one was
/// imported. Epochs above `max_epoch` are refused, not rounded.
pub async fn import(
    adapter: &SqliteAdapter,
    collection: &'static str,
    max_epoch: u64,
) -> Result<Option<usize>, LegacyWatermarkImportError> {
    let writer = adapter.writer().map_err(LegacyWatermarkImportError::Adapter)?;
    tokio::task::spawn_blocking(move || {
        let mut conn = writer
            .lock()
            .map_err(|e| LegacyWatermarkImportError::Adapter(format!("writer lock poisoned: {e}")))?;
        import_in(&mut conn, &super::adapter::naming::to_table_name(collection), max_epoch)
    })
    .await
    .map_err(|e| LegacyWatermarkImportError::Join(e.to_string()))?
}

fn import_in(
    conn: &mut Connection,
    table: &str,
    max_epoch: u64,
) -> Result<Option<usize>, LegacyWatermarkImportError> {
    let tx = conn.transaction()?;
    let exists = tx
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![LEGACY_TABLE],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(None);
    }
    let rows: Vec<(rusqlite::types::Value, rusqlite::types::Value, rusqlite::types::Value)> = {
        let mut stmt = tx.prepare(&format!("SELECT grantee, epoch, updated_at_ms FROM {LEGACY_TABLE}"))?;
        let mapped = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        mapped.collect::<Result<_, _>>()?
    };
    let now = chrono::Utc::now().to_rfc3339();
    let upsert = format!(
        "INSERT INTO {table} (id, created_at, updated_at, version, epoch, updated_at_ms) \
         VALUES (?1, ?2, ?2, 1, ?3, ?4) \
         ON CONFLICT(id) DO UPDATE SET \
         epoch = MAX(epoch, excluded.epoch), \
         updated_at_ms = MAX(updated_at_ms, excluded.updated_at_ms), \
         updated_at = excluded.updated_at"
    );
    for (grantee, epoch, updated_at_ms) in &rows {
        let grantee = exact_grantee(grantee)?;
        let epoch = exact_count(&grantee, "epoch", epoch, max_epoch)?;
        let updated_at_ms = exact_count(&grantee, "updated_at_ms", updated_at_ms, i64::MAX as u64)?;
        tx.execute(&upsert, params![grantee.to_string(), now, epoch, updated_at_ms])?;
    }
    tx.execute(&format!("DROP TABLE {LEGACY_TABLE}"), [])?;
    tx.commit()?;
    Ok(Some(rows.len()))
}

fn exact_grantee(value: &rusqlite::types::Value) -> Result<Uuid, LegacyWatermarkImportError> {
    match value {
        rusqlite::types::Value::Text(text) => Uuid::parse_str(text).map_err(|e| {
            LegacyWatermarkImportError::CorruptRow { grantee: text.clone(), reason: format!("grantee is not a UUID: {e}") }
        }),
        other => Err(LegacyWatermarkImportError::CorruptRow {
            grantee: format!("{other:?}"),
            reason: "grantee is not text".to_string(),
        }),
    }
}

/// A non-negative integer no larger than `max`, as SQLite's i64.
fn exact_count(
    grantee: &Uuid,
    column: &str,
    value: &rusqlite::types::Value,
    max: u64,
) -> Result<i64, LegacyWatermarkImportError> {
    let corrupt = |reason: String| LegacyWatermarkImportError::CorruptRow { grantee: grantee.to_string(), reason };
    match value {
        rusqlite::types::Value::Integer(n) if *n >= 0 && (*n as u64) <= max => Ok(*n),
        rusqlite::types::Value::Integer(n) => Err(corrupt(format!("{column} {n} is outside 0..={max}"))),
        other => Err(corrupt(format!("{column} is not an integer: {other:?}"))),
    }
}

/// Fixtures that write and inspect the legacy table, so tests outside `orm/` never
/// carry SQL themselves.
#[cfg(test)]
pub(crate) mod fixture {
    use super::*;

    /// The pre-ORM store's DDL, verbatim from the deleted `SqliteEpochWatermark`.
    const LEGACY_DDL: &str = "CREATE TABLE IF NOT EXISTS grant_epoch_watermark (\
         grantee TEXT PRIMARY KEY, \
         epoch INTEGER NOT NULL, \
         updated_at_ms INTEGER NOT NULL\
     )";

    /// Create the legacy table at `path` with the old DDL and these rows, as the old
    /// store left it on a node's disk. Values are written as given, so a test can plant
    /// a corrupt row.
    /// `epoch` is a JSON integer or string, stored with that SQLite type.
    pub(crate) fn write_legacy(path: &std::path::Path, rows: &[(&str, serde_json::Value, i64)]) {
        use rusqlite::types::Value;
        let conn = Connection::open(path).expect("open legacy file");
        conn.execute(LEGACY_DDL, []).expect("legacy DDL");
        for (grantee, epoch, updated_at_ms) in rows {
            let epoch = match epoch {
                serde_json::Value::Number(n) => Value::Integer(n.as_i64().expect("an i64 epoch")),
                serde_json::Value::String(text) => Value::Text(text.clone()),
                other => panic!("fixture epoch must be an integer or a string, got {other}"),
            };
            conn.execute(
                "INSERT INTO grant_epoch_watermark (grantee, epoch, updated_at_ms) VALUES (?1, ?2, ?3)",
                params![grantee, epoch, updated_at_ms],
            )
            .expect("legacy row");
        }
    }

    /// Whether the legacy table still exists at `path`.
    pub(crate) fn legacy_table_exists(path: &std::path::Path) -> bool {
        let conn = Connection::open(path).expect("open file");
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![LEGACY_TABLE],
            |_| Ok(()),
        )
        .optional()
        .expect("read schema")
        .is_some()
    }
}
