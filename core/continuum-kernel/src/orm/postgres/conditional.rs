//! A row's insert columns, and the single-statement conditional writes built on them.
//!
//! [`upsert_monotonic`](crate::orm::StorageAdapter::upsert_monotonic) and
//! [`delete_matching`](crate::orm::StorageAdapter::delete_matching) each run as ONE
//! statement: Postgres takes the row lock for `INSERT ... ON CONFLICT DO UPDATE ... WHERE`
//! and evaluates a `DELETE`'s filter when it runs, so no decision is applied against a
//! stale read.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_postgres::types::ToSql;

use super::{build_where_clause, value_to_pg_typed, PostgresAdapter};
use crate::orm::adapter::naming;
use crate::orm::query::StorageQuery;
use crate::orm::types::{DataRecord, StorageResult, METADATA_KEYS};

type PgParam = Box<dyn ToSql + Sync + Send>;

/// The columns and values a new `record` row is written with: the BaseEntity columns
/// (stamped `now`, version 1) and then every data field, coerced to the column's type.
/// THE one place a row's insert shape is built.
pub(super) fn insert_columns(
    record: &DataRecord,
    now: DateTime<Utc>,
    col_types: &HashMap<String, String>,
) -> (Vec<String>, Vec<PgParam>) {
    let mut columns: Vec<String> = ["id", "created_at", "updated_at", "version"].map(String::from).to_vec();
    let mut params: Vec<PgParam> = vec![Box::new(record.id.clone()), Box::new(now), Box::new(now), Box::new(1_i64)];
    if let Value::Object(data) = &record.data {
        for (key, value) in data.iter().filter(|(key, _)| !METADATA_KEYS.contains(&key.as_str())) {
            let column = naming::to_snake_case(key);
            params.push(value_to_pg_typed(value, col_types.get(&column).map(String::as_str)));
            columns.push(column);
        }
    }
    (columns, params)
}

fn refs(params: &[PgParam]) -> Vec<&(dyn ToSql + Sync)> {
    params.iter().map(|b| &**b as &(dyn ToSql + Sync)).collect()
}

impl PostgresAdapter {
    /// Insert `record`, or overwrite its row only where the stored `field` <= the new one.
    pub(super) async fn upsert_monotonic_statement(&self, record: DataRecord, field: &str) -> StorageResult<bool> {
        let client = match self.pool() {
            Ok(pool) => match pool.get().await {
                Ok(client) => client,
                Err(e) => return StorageResult::err(format!("Pool error: {e}")),
            },
            Err(e) => return StorageResult::err(e),
        };
        let field = naming::to_snake_case(field);
        let col_types = self.cached_column_types(&client, &naming::to_table_name(&record.collection)).await;
        let (columns, params) = insert_columns(&record, Utc::now(), &col_types);
        if !columns.iter().any(|c| *c == field) {
            return StorageResult::err(format!("upsert_monotonic: the record has no `{field}` to compare"));
        }
        let overwrite: Vec<String> = columns
            .iter()
            .filter(|c| !matches!(c.as_str(), "id" | "created_at" | "version"))
            .map(|c| format!("{c} = EXCLUDED.{c}"))
            .chain(std::iter::once("version = current.version + 1".to_string()))
            .collect();
        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${i}")).collect();
        let sql = format!(
            "INSERT INTO {} AS current ({}) VALUES ({}) ON CONFLICT (id) DO UPDATE SET {} WHERE current.{field} <= EXCLUDED.{field}",
            self.table_ref(&record.collection),
            columns.join(", "),
            placeholders.join(", "),
            overwrite.join(", "),
        );
        match client.execute(&sql, &refs(&params)).await {
            Ok(rows) => StorageResult::ok(rows == 1),
            Err(e) => StorageResult::err(format!("upsert_monotonic failed: {e}")),
        }
    }

    /// Delete every row the filter selects, judged when the delete runs.
    pub(super) async fn delete_matching_statement(&self, query: StorageQuery) -> StorageResult<usize> {
        let client = match self.pool() {
            Ok(pool) => match pool.get().await {
                Ok(client) => client,
                Err(e) => return StorageResult::err(format!("Pool error: {e}")),
            },
            Err(e) => return StorageResult::err(e),
        };
        let col_types = self.cached_column_types(&client, &naming::to_table_name(&query.collection)).await;
        let (where_clause, params) = build_where_clause(&query.filter, 0, &col_types);
        let sql = format!("DELETE FROM {} {where_clause}", self.table_ref(&query.collection));
        match client.execute(&sql, &refs(&params)).await {
            Ok(rows) => StorageResult::ok(rows as usize),
            Err(e) => StorageResult::err(format!("delete_matching failed: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::orm::adapter::{AdapterConfig, StorageAdapter};
    use crate::orm::query::{QueryBuilder, QueryOperator};
    use crate::orm::types::{CollectionSchema, DataRecord, FieldType, RecordMetadata, SchemaField};
    use crate::orm::PostgresAdapter;
    use serde_json::json;

    fn number(name: &str) -> SchemaField {
        SchemaField {
            name: name.to_string(),
            field_type: FieldType::Number,
            indexed: false,
            unique: false,
            nullable: false,
            max_length: None,
            foreign_key: None,
        }
    }

    fn mark(id: &str, epoch: u64, at: u64) -> DataRecord {
        DataRecord {
            id: id.to_string(),
            collection: "monotonic_marks".to_string(),
            data: json!({ "epoch": epoch, "updatedAtMs": at }),
            metadata: RecordMetadata::default(),
        }
    }

    // what this catches: the Postgres statement behind upsert_monotonic/delete_matching —
    // a first write creates, a lower one is refused, an equal or higher one overwrites,
    // and the conditional delete removes only rows its filter selects when it runs.
    #[tokio::test]
    #[ignore] // Requires live Postgres (CI: `orm::postgres --ignored`)
    async fn pg_upsert_monotonic_never_lowers_and_delete_matching_judges_current_rows() {
        let mut adapter = PostgresAdapter::new();
        adapter
            .initialize(AdapterConfig {
                connection_string: std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "postgres://continuum:continuum@localhost:5432/continuum_test".to_string()),
                namespace: Some("test_orm_monotonic".to_string()),
                timeout_ms: 30_000,
                max_connections: 2,
            })
            .await
            .expect("PostgreSQL connection failed - is Postgres running?");
        let mut fields = crate::orm::entity::base_entity_fields();
        fields.extend([number("epoch"), number("updatedAtMs")]);
        let schema = CollectionSchema { collection: "monotonic_marks".to_string(), fields, indexes: vec![] };
        let ensured = adapter.ensure_schema(schema).await;
        assert!(ensured.success, "ensure_schema: {:?}", ensured.error);
        assert!(adapter.truncate("monotonic_marks").await.success);
        let id = uuid::Uuid::new_v4().to_string();
        let write = |epoch, at| adapter.upsert_monotonic(mark(&id, epoch, at), "epoch");
        assert_eq!(write(5, 1_000).await.data, Some(true), "first write creates");
        assert_eq!(write(3, 2_000).await.data, Some(false), "a lower epoch is refused");
        assert_eq!(write(5, 3_000).await.data, Some(true), "an equal epoch overwrites");
        assert_eq!(write(7, 4_000).await.data, Some(true), "a higher epoch overwrites");
        let row = adapter.read("monotonic_marks", &id).await.data.expect("row");
        assert_eq!(row.data["epoch"].as_f64(), Some(7.0));
        let stale = |cutoff: u64| {
            QueryBuilder::new("monotonic_marks").filter("updatedAtMs", QueryOperator::Lt(json!(cutoff))).build()
        };
        assert_eq!(adapter.delete_matching(stale(4_000)).await.data, Some(0), "the row is not stale");
        assert_eq!(adapter.delete_matching(stale(5_000)).await.data, Some(1));
        assert!(adapter.read("monotonic_marks", &id).await.data.is_none());
    }
}
