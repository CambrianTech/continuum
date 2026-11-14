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

use super::storage_error::{ConfigFault, StorageError};

#[derive(Clone)]
pub(super) struct RelationalStore {
    pub db: DatabaseConnection,
    pub in_memory: bool,
    schema_url: String,
    schemas: Arc<parking_lot::RwLock<HashMap<String, Arc<CollectionSchema>>>>,
    schema_gate: Arc<tokio::sync::Mutex<()>>,
    pub writer_gate: Arc<tokio::sync::Mutex<()>>,
    revisions: Arc<
        parking_lot::Mutex<
            HashMap<(super::relational_sequence::Domain, String), tokio::sync::watch::Sender<u64>>,
        >,
    >,
}

impl RelationalStore {
    pub async fn open(config: AdapterConfig) -> Result<Self, StorageError> {
        if config.max_connections == 0 {
            return Err(StorageError::Config(ConfigFault::NoCapacity));
        }
        if config.namespace.is_some() {
            return Err(StorageError::Config(ConfigFault::Namespace));
        }
        let in_memory = config.connection_string == ":memory:";
        if !in_memory {
            let path = std::path::Path::new(&config.connection_string);
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                continuum_host::confine::open_root(parent)
                    .map_err(|source| StorageError::Io { path: parent.to_path_buf(), source })?;
            }
        }
        let url = if in_memory {
            format!("sqlite:file%3Acontinuum-{}?mode=memory&cache=shared", uuid::Uuid::new_v4())
        } else {
            format!(
                "sqlite://{}?mode=rwc",
                config.connection_string.replace('\\', "/")
            )
        };
        let mut options = ConnectOptions::new(url.clone());
        options
            .max_connections(
                config
                    .max_connections
                    .try_into()
                    .map_err(|_| StorageError::Config(ConfigFault::CapacityRange))?,
            )
            .connect_timeout(std::time::Duration::from_millis(config.timeout_ms))
            .acquire_timeout(std::time::Duration::from_millis(config.timeout_ms))
            .sqlx_logging(false)
            .map_sqlx_sqlite_opts(|o| {
                o.foreign_keys(true)
                    .journal_mode(sea_orm::sqlx::sqlite::SqliteJournalMode::Wal)
                    .optimize_on_close(true, None)
            });
        let db = Database::connect(options).await?;
        let schema_db = super::relational_evolution::schema_connection(&url).await?;
        let initialized = async {
            use super::relational_schema::DeclaredTable;
            use sea_orm::EntityName;
            use super::relational_evolution::{admit_table, verify_references};
            let transaction = schema_db.begin().await?;
            admit_table(&transaction, super::relational_sequence::Entity.table_name(), DeclaredTable::from_entity(super::relational_sequence::Entity)).await?;
            let mut native = DeclaredTable::from_entity(super::relational_native_revision::Entity);
            native.indexes.push(super::relational_native_revision::revision_index());
            admit_table(&transaction, super::relational_native_revision::Entity.table_name(), native).await?;
            admit_table(&transaction, super::relational_declaration::Entity.table_name(), DeclaredTable::from_entity(super::relational_declaration::Entity)).await?;
            verify_references(&transaction).await?;
            transaction.commit().await?;
            Ok::<(), super::schema_error::SchemaError>(())
        }.await;
        let closed = schema_db.close().await;
        initialized?;
        closed?;
        let catalog_read = db.begin().await?;
        let schemas = super::relational_declaration::restore(&catalog_read).await?;
        use super::relational_sequence::{Column, Entity};
        use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
        let mut revisions = HashMap::new();
        // Transfer pages bound temporary materialization, never the number of heads.
        let mut after: Option<(String, String)> = None;
        loop {
            let mut query = Entity::find()
                .order_by_asc(Column::Domain)
                .order_by_asc(Column::Collection)
                .limit(64);
            if let Some((domain, collection)) = &after {
                query = query.filter(
                    Condition::any()
                        .add(Column::Domain.gt(domain.as_str()))
                        .add(
                            Condition::all()
                                .add(Column::Domain.eq(domain.as_str()))
                                .add(Column::Collection.gt(collection.as_str())),
                        ),
                );
            }
            let heads = query.all(&catalog_read).await?;
            if heads.is_empty() {
                break;
            }
            after = heads
                .last()
                .map(|head| (head.domain.clone(), head.collection.clone()));
            for head in heads {
                let domain = match head.domain.as_str() {
                    "append" => super::relational_sequence::Domain::Append,
                    "native" => super::relational_sequence::Domain::Native,
                    _ => return Err(StorageError::Corrupt { what: "a persisted revision names an unknown domain" }),
                };
                // A head is written only by a commit, so it is positive (the u64 conversion refuses the rest).
                let high = u64::try_from(head.high)
                    .ok()
                    .filter(|high| *high > 0)
                    .ok_or(StorageError::Corrupt { what: "a persisted revision is not positive" })?;
                revisions.insert(
                    (
                        domain,
                        super::adapter::naming::to_table_name(&head.collection),
                    ),
                    tokio::sync::watch::channel(high).0,
                );
            }
        }
        catalog_read.commit().await?;
        Ok(Self {
            db,
            in_memory,
            schema_url: url,
            schemas: Arc::new(parking_lot::RwLock::new(schemas)),
            schema_gate: Arc::new(tokio::sync::Mutex::new(())),
            writer_gate: Arc::new(tokio::sync::Mutex::new(())),
            revisions: Arc::new(parking_lot::Mutex::new(revisions)),
        })
    }

    pub async fn ensure(&self, schema: &CollectionSchema) -> Result<(), StorageError> {
        let _guard = self.schema_gate.lock().await;
        if let Some(existing) = self.schemas.read().get(&schema.collection) {
            if super::entity::schemas_equivalent(existing.as_ref(), schema) {
                return Ok(());
            }
        }
        let declared = super::relational_schema::compile(schema)?;
        let _write = self.writer_gate.lock().await;
        // SQLite DROP TABLE executes FK actions even during a rebuild. Use a
        // short-lived, pinned schema connection; normal read/write connections
        // keep FK enforcement enabled. Validate all references before commit.
        let schema_db = super::relational_evolution::schema_connection(&self.schema_url).await?;
        let result = async {
            let transaction = schema_db.begin().await?;
            if super::relational_declaration::admit(&transaction, schema).await? {
                transaction.execute(DbBackend::Sqlite.build(&declared.create)).await?;
                for index in declared.indexes {
                    transaction.execute(DbBackend::Sqlite.build(&index)).await?;
                }
                super::relational_declaration::record(&transaction, schema).await?;
            }
            super::relational_evolution::verify_references(&transaction).await?;
            transaction.commit().await?;
            Ok::<(), StorageError>(())
        }
        .await;
        let closed = schema_db.close().await;
        result?;
        closed?;
        self.schemas.write().insert(schema.collection.clone(), Arc::new(schema.clone()));
        Ok(())
    }

    pub fn schema(&self, collection: &str) -> Result<Arc<CollectionSchema>, StorageError> {
        self.schemas
            .read()
            .get(collection)
            .cloned()
            .ok_or_else(|| StorageError::Unadmitted { collection: collection.to_owned() })
    }

    pub fn collections(&self) -> Vec<String> {
        let mut collections: Vec<_> = self.schemas.read().keys().cloned().collect();
        collections.sort();
        collections
    }

    pub fn revision(
        &self,
        domain: super::relational_sequence::Domain,
        collection: &str,
    ) -> tokio::sync::watch::Receiver<u64> {
        self.revisions
            .lock()
            .entry((domain, super::adapter::naming::to_table_name(collection)))
            .or_insert_with(|| tokio::sync::watch::channel(0).0)
            .subscribe()
    }

    pub fn committed(
        &self,
        domain: super::relational_sequence::Domain,
        collection: &str,
        sequence: u64,
    ) {
        self.revisions
            .lock()
            .entry((domain, super::adapter::naming::to_table_name(collection)))
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

    pub async fn query(&self, query: &StorageQuery) -> Result<Vec<DataRecord>, StorageError> {
        let schema = self.schema(&query.collection)?;
        let statement = super::relational_query::select(
            &schema,
            query,
            super::relational_query::Selection::Page,
        )?;
        let started = std::time::Instant::now();
        let rows = self.db.query_all(DbBackend::Sqlite.build(&statement)).await?;
        let database_ms = started.elapsed().as_millis() as u64;
        // The connection has returned to the pool before any JSON body is parsed.
        let decode_started = std::time::Instant::now();
        let decoder = super::relational_codec::RowDecoder::new(schema.clone(), &[]);
        let count = rows.len();
        let result = rows.into_iter().map(|row| decoder.decode(row)).collect();
        continuum_base::probe!(class = "orm.relational.query", collection = %query.collection,
            rows = count, database_ms, materialization_ms = decode_started.elapsed().as_millis() as u64,
            "projected database retrieval and schema decoding measured separately");
        result
    }

    pub async fn count(&self, query: &StorageQuery) -> Result<usize, StorageError> {
        let schema = self.schema(&query.collection)?;
        let statement = super::relational_query::select(
            &schema,
            query,
            super::relational_query::Selection::Count,
        )?;
        let row = self
            .db
            .query_one(DbBackend::Sqlite.build(&statement))
            .await?
            .ok_or(StorageError::Corrupt { what: "a COUNT returned no row" })?;
        let count: i64 = row.try_get("", "count")?;
        usize::try_from(count).map_err(|_| StorageError::Overflow { what: "a row count" })
    }

    pub async fn insert<C: ConnectionTrait>(
        &self,
        connection: &C,
        record: &DataRecord,
    ) -> Result<(), StorageError> {
        let insert = self.insert_statement(record)?;
        connection.execute(DbBackend::Sqlite.build(&insert)).await?;
        Ok(())
    }

    pub fn insert_statement(
        &self,
        record: &DataRecord,
    ) -> Result<sea_orm::sea_query::InsertStatement, StorageError> {
        self.insert_statement_with_binary(record, std::collections::BTreeMap::new())
    }

    pub(super) fn insert_statement_with_binary(
        &self,
        record: &DataRecord,
        mut payloads: std::collections::BTreeMap<String, super::binary::BinaryValue>,
    ) -> Result<sea_orm::sea_query::InsertStatement, StorageError> {
        let schema = self.schema(&record.collection)?;
        let fields = record.data.as_object().ok_or_else(|| StorageError::Malformed {
            collection: record.collection.clone(),
            expected: super::storage_error::Shape::Object,
        })?;
        let mut values = Vec::new();
        let mut columns = Vec::new();
        for field in &schema.fields {
            if let Some(payload) = payloads.remove(&field.name) {
                if field.field_type != super::types::FieldType::Binary {
                    return Err(StorageError::NotBinary { collection: record.collection.clone(), field: field.name.clone() });
                }
                if fields.contains_key(&field.name) {
                    return Err(StorageError::TwoRepresentations {
                        collection: record.collection.clone(),
                        field: field.name.clone(),
                    });
                }
                let (descriptor, bytes) = payload.into_parts();
                columns.push(Alias::new(super::adapter::naming::to_snake_case(
                    &field.name,
                )));
                values.push(Expr::val(bytes).into());
                columns.push(Alias::new(super::binary::descriptor_column(&field.name)));
                values.push(Expr::val(descriptor.encode()?).into());
                continue;
            }
            use super::relational_codec::{encode, encode_integer, encode_text};
            let value = match field.name.as_str() {
                "id" => encode_text(field, &record.id)?,
                "createdAt" => encode_text(field, &record.metadata.created_at)?,
                "updatedAt" => encode_text(field, &record.metadata.updated_at)?,
                "version" => encode_integer(field, i64::from(record.metadata.version))?,
                name => {
                    let value = match fields.get(name) {
                        Some(value) => value,
                        None => match &field.default_value {
                            Some(value) => value,
                            None if field.nullable => &serde_json::Value::Null,
                            None => {
                                return Err(StorageError::Required { collection: record.collection.clone(), field: name.to_owned() })
                            }
                        },
                    };
                    encode(field, value)?
                }
            };
            columns.push(Alias::new(super::adapter::naming::to_snake_case(
                &field.name,
            )));
            values.push(Expr::val(value).into());
        }
        if let Some(field) = payloads.into_keys().next() {
            return Err(StorageError::Undeclared { collection: record.collection.clone(), field });
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
            .values(values)?;
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
        store.ensure(&schema).await?;
        let record = DataRecord {
            id: "one".into(),
            collection: "projected".into(),
            data: serde_json::json!({"body":{"exact":9007199254740993u64}}),
            metadata: RecordMetadata::default(),
        };
        store.insert(&store.db, &record).await?;
        let rows = store
            .query(&QueryBuilder::new("projected").limit(1).build())
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
                &QueryBuilder::new("projected")
                    .select(vec!["id".into()])
                    .limit(1)
                    .build(),
            )
            .await?;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            store.count(&QueryBuilder::new("projected").build()).await?,
            1
        );
        assert!(store
            .query(&QueryBuilder::new("projected").limit(1).build())
            .await
            .is_err());
        Ok(())
    }
}
