//! A row's insert columns, and the single-statement conditional writes built on them.
//!
//! [`upsert_monotonic`](crate::orm::StorageAdapter::upsert_monotonic) and
//! [`delete_matching`](crate::orm::StorageAdapter::delete_matching) each run as ONE
//! statement on the writer connection: the compare happens when the write executes, so
//! a caller cancelled mid-write, a second store over the same adapter, or a write that
//! lands late can never apply a decision made against a stale read.

use rusqlite::Connection;
use serde_json::Value;

use super::{build_where_clause, value_to_sql_boxed, SqliteAdapter};
use crate::orm::adapter::naming;
use crate::orm::query::StorageQuery;
use crate::orm::types::{DataRecord, StorageResult, METADATA_KEYS};

/// The columns and values a new `record` row is written with: the BaseEntity columns
/// (stamped `now`, version 1) and then every data field. THE one place a row's insert
/// shape is built.
pub(super) fn insert_columns(record: &DataRecord, now: &str) -> (Vec<String>, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut columns: Vec<String> = ["id", "created_at", "updated_at", "version"].map(String::from).to_vec();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> =
        vec![Box::new(record.id.clone()), Box::new(now.to_string()), Box::new(now.to_string()), Box::new(1i64)];
    if let Value::Object(data) = &record.data {
        for (key, value) in data.iter().filter(|(key, _)| !METADATA_KEYS.contains(&key.as_str())) {
            columns.push(naming::to_snake_case(key));
            values.push(value_to_sql_boxed(value));
        }
    }
    (columns, values)
}

impl SqliteAdapter {
    pub(super) async fn upsert_monotonic_on_writer(&self, record: DataRecord, field: &str) -> StorageResult<bool> {
        let field = naming::to_snake_case(field);
        self.on_writer(move |conn| upsert_monotonic(conn, &record, &field)).await
    }

    pub(super) async fn delete_matching_on_writer(&self, query: StorageQuery) -> StorageResult<usize> {
        self.on_writer(move |conn| delete_matching(conn, &query)).await
    }

    /// Run `op` on the writer connection off the async executor.
    async fn on_writer<R: Send + 'static>(
        &self,
        op: impl FnOnce(&Connection) -> StorageResult<R> + Send + 'static,
    ) -> StorageResult<R> {
        let writer = match self.get_writer() {
            Ok(writer) => writer,
            Err(e) => return StorageResult::err(e),
        };
        tokio::task::spawn_blocking(move || match writer.lock() {
            Ok(conn) => op(&conn),
            Err(e) => StorageResult::err(format!("writer lock poisoned: {e}")),
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {e}"))) // a panicked blocking task is this write's error, reported, never swallowed
    }
}

/// Insert `record` unless a row with its id exists (left untouched). Returns whether it inserted.
pub(super) fn ensure_row(conn: &Connection, record: &DataRecord) -> Result<bool, String> {
    let table = naming::to_table_name(&record.collection);
    let now = chrono::Utc::now().to_rfc3339();
    let (columns, values) = insert_columns(record, &now);
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({}) ON CONFLICT(id) DO NOTHING",
        columns.join(", "),
        vec!["?"; columns.len()].join(", "),
    );
    let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|b| b.as_ref()).collect();
    conn.execute(&sql, params.as_slice())
        .map(|rows| rows == 1)
        .map_err(|e| format!("ensure failed: {e}"))
}

/// Insert `record`, or overwrite its row only where the stored `field` <= the new one.
fn upsert_monotonic(conn: &Connection, record: &DataRecord, field: &str) -> StorageResult<bool> {
    let table = naming::to_table_name(&record.collection);
    let now = chrono::Utc::now().to_rfc3339();
    let (columns, values) = insert_columns(record, &now);
    if !columns.iter().any(|c| c == field) {
        return StorageResult::err(format!("upsert_monotonic: the record has no `{field}` to compare"));
    }
    let overwrite: Vec<String> = columns
        .iter()
        .filter(|c| !matches!(c.as_str(), "id" | "created_at" | "version"))
        .map(|c| format!("{c} = excluded.{c}"))
        .chain(std::iter::once("version = version + 1".to_string()))
        .collect();
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({}) ON CONFLICT(id) DO UPDATE SET {} WHERE {table}.{field} <= excluded.{field}",
        columns.join(", "),
        vec!["?"; columns.len()].join(", "),
        overwrite.join(", "),
    );
    let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|b| b.as_ref()).collect();
    match conn.execute(&sql, params.as_slice()) {
        Ok(rows) => StorageResult::ok(rows == 1),
        Err(e) => StorageResult::err(format!("upsert_monotonic failed: {e}")),
    }
}

/// Delete every row the filter selects, judged when the delete runs.
fn delete_matching(conn: &Connection, query: &StorageQuery) -> StorageResult<usize> {
    let (where_clause, where_params) = build_where_clause(&query.filter);
    let sql = format!("DELETE FROM {} {where_clause}", naming::to_table_name(&query.collection));
    let params: Vec<Box<dyn rusqlite::ToSql>> = where_params.iter().map(value_to_sql_boxed).collect();
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    match conn.execute(&sql, params_ref.as_slice()) {
        Ok(rows) => StorageResult::ok(rows),
        Err(e) => StorageResult::err(format!("delete_matching failed: {e}")),
    }
}
