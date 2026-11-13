//! SeaORM connection and schema owner. The adapter delegates all relational work here.
use super::{
    adapter::AdapterConfig,
    query::StorageQuery,
    types::{CollectionSchema, DataRecord},
};
use sea_orm::sea_query::{Alias, Expr, Query};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, TransactionTrait,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct RelationalStore {
    pub db: DatabaseConnection,
    pub in_memory: bool,
    schemas: Arc<parking_lot::RwLock<HashMap<String, Arc<CollectionSchema>>>>,
    schema_gate: Arc<tokio::sync::Mutex<()>>,
    pub writer_gate: Arc<tokio::sync::Mutex<()>>,
    revisions: Arc<parking_lot::Mutex<HashMap<String, tokio::sync::watch::Sender<u64>>>>,
}

impl RelationalStore {
    pub async fn open(config: AdapterConfig) -> Result<Self, String> {
        if config.max_connections == 0 {
            return Err("connection pool requires positive capacity".into());
        }
        if config.namespace.is_some() {
            return Err("SQLite namespaces require separate database owners".into());
        }
        let in_memory = config.connection_string == ":memory:";
        if !in_memory {
            let path = std::path::Path::new(&config.connection_string);
            if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
                continuum_host::confine::open_root(parent)
                    .map_err(|error| format!("database directory {}: {error}", parent.display()))?;
            }
        }
        let url = if in_memory {
            "sqlite::memory:".into()
        } else {
            format!(
                "sqlite://{}?mode=rwc",
                config.connection_string.replace('\\', "/")
            )
        };
        let mut options = ConnectOptions::new(url);
        options
            .max_connections(
                config
                    .max_connections
                    .try_into()
                    .map_err(|_| "connection count exceeds u32")?,
            )
            .connect_timeout(std::time::Duration::from_millis(config.timeout_ms))
            .acquire_timeout(std::time::Duration::from_millis(config.timeout_ms))
            .sqlx_logging(false)
            .map_sqlx_sqlite_opts(|o| {
                o.foreign_keys(true)
                    .journal_mode(sea_orm::sqlx::sqlite::SqliteJournalMode::Wal)
                    .optimize_on_close(true, None)
            });
        let db = Database::connect(options)
            .await
            .map_err(|e| e.to_string())?;
        let heads = sea_orm::Schema::new(DbBackend::Sqlite)
            .create_table_from_entity(super::relational_sequence::Entity)
            .if_not_exists()
            .to_owned();
        db.execute(DbBackend::Sqlite.build(&heads))
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self {
            db,
            in_memory,
            schemas: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            schema_gate: Arc::new(tokio::sync::Mutex::new(())),
            writer_gate: Arc::new(tokio::sync::Mutex::new(())),
            revisions: Arc::new(parking_lot::Mutex::new(HashMap::new())),
        })
    }

    pub async fn ensure(&self, schema: CollectionSchema) -> Result<(), String> {
        let _guard = self.schema_gate.lock().await;
        if let Some(existing) = self.schemas.read().get(&schema.collection) {
            if serde_json::to_value(existing.as_ref()).map_err(|e| e.to_string())?
                != serde_json::to_value(&schema).map_err(|e| e.to_string())?
            {
                return Err(format!(
                    "{} has conflicting entity declarations",
                    schema.collection
                ));
            }
            return Ok(());
        }
        let declared = super::relational_schema::compile(&schema)?;
        let _write = self.writer_gate.lock().await;
        let transaction = self.db.begin().await.map_err(|e| e.to_string())?;
        transaction
            .execute(DbBackend::Sqlite.build(&declared.create))
            .await
            .map_err(|e| e.to_string())?;
        for index in declared.indexes {
            transaction
                .execute(DbBackend::Sqlite.build(&index))
                .await
                .map_err(|e| e.to_string())?;
        }
        transaction.commit().await.map_err(|e| e.to_string())?;
        self.schemas
            .write()
            .insert(schema.collection.clone(), Arc::new(schema));
        Ok(())
    }

    pub fn schema(&self, collection: &str) -> Result<Arc<CollectionSchema>, String> {
        self.schemas
            .read()
            .get(collection)
            .cloned()
            .ok_or_else(|| format!("collection {collection} has no admitted entity schema"))
    }

    pub fn collections(&self) -> Vec<String> {
        let mut collections: Vec<_> = self.schemas.read().keys().cloned().collect();
        collections.sort();
        collections
    }

    pub fn revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64> {
        self.revisions
            .lock()
            .entry(collection.into())
            .or_insert_with(|| tokio::sync::watch::channel(0).0)
            .subscribe()
    }

    pub fn committed(&self, collection: &str, sequence: u64) {
        self.revisions
            .lock()
            .entry(collection.into())
            .or_insert_with(|| tokio::sync::watch::channel(0).0)
            .send_if_modified(|high| {
                if sequence > *high {
                    *high = sequence;
                    true
                } else {
                    false
                }
            });
    }

    pub async fn query(&self, query: StorageQuery) -> Result<Vec<DataRecord>, String> {
        let schema = self.schema(&query.collection)?;
        let statement = super::relational_query::select(&schema, &query, super::relational_query::Selection::Page)?;
        let started = std::time::Instant::now();
        let rows = self
            .db
            .query_all(DbBackend::Sqlite.build(&statement))
            .await
            .map_err(|e| e.to_string())?;
        let database_ms = started.elapsed().as_millis() as u64;
        // The connection has returned to the pool before any JSON body is parsed.
        let decode_started = std::time::Instant::now();
        let count = rows.len();
        let result = rows
            .into_iter()
            .map(|row| super::relational_codec::decode(row, &schema))
            .collect();
        continuum_base::probe!(class = "orm.relational.query", collection = %query.collection,
            rows = count, database_ms, materialization_ms = decode_started.elapsed().as_millis() as u64,
            "projected database retrieval and schema decoding measured separately");
        result
    }

    pub async fn count(&self, query: StorageQuery) -> Result<usize, String> {
        let schema = self.schema(&query.collection)?;
        let statement = super::relational_query::select(&schema, &query, super::relational_query::Selection::Count)?;
        let row = self
            .db
            .query_one(DbBackend::Sqlite.build(&statement))
            .await
            .map_err(|e| e.to_string())?
            .ok_or("count returned no row")?;
        let count: i64 = row.try_get("", "count").map_err(|e| e.to_string())?;
        count
            .try_into()
            .map_err(|_| "count is outside usize".into())
    }

    pub async fn insert<C: ConnectionTrait>(
        &self,
        connection: &C,
        record: &DataRecord,
    ) -> Result<(), String> {
        let insert = self.insert_statement(record)?;
        connection
            .execute(DbBackend::Sqlite.build(&insert))
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn insert_statement(
        &self,
        record: &DataRecord,
    ) -> Result<sea_orm::sea_query::InsertStatement, String> {
        let schema = self.schema(&record.collection)?;
        let fields = record
            .data
            .as_object()
            .ok_or("record data must be an object")?;
        let mut values = Vec::new();
        let mut columns = Vec::new();
        for field in &schema.fields {
            let metadata;
            let value = match field.name.as_str() {
                "id" => {
                    metadata = serde_json::Value::String(record.id.clone());
                    &metadata
                }
                "createdAt" => {
                    metadata = serde_json::Value::String(record.metadata.created_at.clone());
                    &metadata
                }
                "updatedAt" => {
                    metadata = serde_json::Value::String(record.metadata.updated_at.clone());
                    &metadata
                }
                "version" => {
                    metadata = serde_json::json!(record.metadata.version);
                    &metadata
                }
                name => match fields.get(name) {
                    Some(value) => value,
                    None if field.default_value.is_some() => field.default_value.as_ref().ok_or("declared default disappeared")?,
                    None if field.nullable => &serde_json::Value::Null,
                    None => return Err(format!("{}.{} is required", record.collection, name)),
                },
            };
            columns.push(Alias::new(super::adapter::naming::to_snake_case(
                &field.name,
            )));
            values.push(Expr::val(super::relational_codec::encode(field, value)?).into());
        }
        for name in fields.keys() {
            super::relational_query::column(&schema, name)?;
        }
        let mut insert = Query::insert();
        insert
            .into_table(Alias::new(super::adapter::naming::to_table_name(
                &record.collection,
            )))
            .columns(columns)
            .values(values)
            .map_err(|e| e.to_string())?;
        Ok(insert)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::{base_entity_fields, FieldType, QueryBuilder, RecordMetadata, SchemaField};
    #[tokio::test]
    async fn projected_reads_and_counts_never_decode_omitted_payloads(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = RelationalStore::open(AdapterConfig {
            connection_string: ":memory:".into(),
            max_connections: 1,
            ..Default::default()
        })
        .await?;
        let mut schema = CollectionSchema {
            collection: "projected".into(),
            fields: base_entity_fields(),
            indexes: vec![],
        };
        schema.fields.push(SchemaField {
                default_value: None,
            name: "body".into(),
            field_type: FieldType::Json,
            indexed: false,
            unique: false,
            nullable: false,
            max_length: None,
            foreign_key: None,
        });
        store.ensure(schema).await?;
        let record = DataRecord {
            id: "one".into(),
            collection: "projected".into(),
            data: serde_json::json!({"body":{"exact":9007199254740993u64}}),
            metadata: RecordMetadata::default(),
        };
        store.insert(&store.db, &record).await?;
        let rows = store
            .query(QueryBuilder::new("projected").limit(1).build())
            .await?;
        assert_eq!(rows[0].data["body"], record.data["body"]);
        assert_eq!(rows[0].data["id"], record.id);
        // Corrupt only the opaque payload through a provider builder: projections must never touch it.
        let statement = Query::update()
            .table(Alias::new("projected"))
            .value(Alias::new("body"), "invalid JSON")
            .to_owned();
        store
            .db
            .execute(DbBackend::Sqlite.build(&statement))
            .await?;
        let rows = store
            .query(
                QueryBuilder::new("projected")
                    .select(vec!["id".into()])
                    .limit(1)
                    .build(),
            )
            .await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            store.count(QueryBuilder::new("projected").build()).await?,
            1
        );
        assert!(store
            .query(QueryBuilder::new("projected").limit(1).build())
            .await
            .is_err());
        Ok(())
    }
}
