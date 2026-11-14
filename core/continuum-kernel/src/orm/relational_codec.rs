//! Decode only projected columns, using the entity's declared codecs.
//! QueryResult owns its SQLite row; decoding happens after the pooled connection returns.
use super::{
    adapter::naming,
    types::{CollectionSchema, DataRecord, FieldType, RecordMetadata, SchemaField},
};
use sea_orm::sqlx::{Row, TypeInfo, ValueRef};
use sea_orm::{sea_query::Value as SqlValue, QueryResult};
use serde_json::{Map, Number, Value};

use super::storage_error::{StorageError, StoredFault};

fn mismatch(field: &SchemaField) -> StorageError {
    StorageError::TypeMismatch { field: field.name.clone().into(), declared: field.field_type.clone() }
}

/// Bind producer-owned text directly. The SQL driver owns the one required copy;
/// callers need not allocate an intermediate JSON string to reach this codec.
pub(super) fn encode_text(field: &SchemaField, value: &str) -> Result<SqlValue, StorageError> {
    match field.field_type {
        FieldType::String | FieldType::Date | FieldType::Uuid => Ok(value.into()),
        FieldType::Json => Ok(serde_json::to_string(value)?.into()), // disk: a Json-declared field is stored as SQLite TEXT
        _ => Err(mismatch(field)),
    }
}

/// Metadata versions already have an exact integer representation.
pub(super) fn encode_integer(field: &SchemaField, value: i64) -> Result<SqlValue, StorageError> {
    match field.field_type {
        FieldType::Number => Ok(value.into()),
        FieldType::Json => Ok(serde_json::to_string(&value)?.into()), // disk: a Json-declared field is stored as SQLite TEXT
        _ => Err(mismatch(field)),
    }
}

pub(super) fn encode(field: &SchemaField, value: &Value) -> Result<SqlValue, StorageError> {
    if value.is_null() {
        return if field.nullable {
            Ok(SqlValue::String(None))
        } else {
            Err(StorageError::NotNullable { field: field.name.clone().into() })
        };
    }
    match field.field_type {
        FieldType::String | FieldType::Date | FieldType::Uuid => {
            encode_text(field, value.as_str().ok_or_else(|| mismatch(field))?)
        }
        FieldType::Boolean => value.as_bool().map(Into::into).ok_or_else(|| mismatch(field)),
        FieldType::Number if value.is_number() => super::relational_query::value(value),
        FieldType::Number => Err(mismatch(field)),
        FieldType::Binary => Err(StorageError::BinaryThroughJson { field: field.name.clone().into() }),
        FieldType::Json => Ok(serde_json::to_string(value)?.into()), // disk: a Json-declared field is stored as SQLite TEXT
    }
}

pub(super) fn scalar(
    row: &sea_orm::sqlx::sqlite::SqliteRow,
    index: usize,
    field: &SchemaField,
) -> Result<Value, StorageError> {
    let raw = row.try_get_raw(index)?;
    if raw.is_null() {
        return if field.nullable {
            Ok(Value::Null)
        } else {
            Err(StorageError::NotNullable { field: field.name.clone().into() })
        };
    }
    let invalid = |fault| StorageError::InvalidStored { field: field.name.clone().into(), fault };
    match field.field_type {
        FieldType::String | FieldType::Date | FieldType::Uuid => Ok(Value::String(row.try_get::<String, _>(index)?)),
        FieldType::Binary => Err(StorageError::BinaryThroughJson { field: field.name.clone().into() }),
        FieldType::Json => {
            let text: String = row.try_get(index)?;
            serde_json::from_str(&text).map_err(|source| StorageError::StoredJson { field: field.name.clone().into(), source })
        }
        FieldType::Boolean => match row.try_get::<i64, _>(index)? {
            0 => Ok(Value::Bool(false)),
            1 => Ok(Value::Bool(true)),
            other => Err(invalid(StoredFault::Boolean(other))),
        },
        FieldType::Number => match raw.type_info().name() {
            "INTEGER" => Ok(Value::Number(row.try_get::<i64, _>(index)?.into())),
            "REAL" => Number::from_f64(row.try_get(index)?).map(Value::Number).ok_or_else(|| invalid(StoredFault::NonFinite)),
            storage => Err(StorageError::StorageClass {
                field: field.name.clone().into(),
                declared: field.field_type.clone(),
                found: storage.to_owned().into(),
            }),
        },
    }
}

/// Compile physical field names once per query, not once per cell in every row.
/// The schema allocation stays shared; the plan stores only names and field ordinals.
pub(super) struct RowDecoder {
    schema: std::sync::Arc<CollectionSchema>,
    fields: std::collections::HashMap<String, usize>,
    joined_columns: std::collections::HashSet<String>,
}

impl RowDecoder {
    pub(super) fn new(schema: std::sync::Arc<CollectionSchema>, joined_columns: &[String]) -> Self {
        let fields = schema.fields.iter().enumerate()
            .map(|(index, field)| (naming::to_snake_case(&field.name), index)).collect();
        Self { schema, fields, joined_columns: joined_columns.iter().cloned().collect() }
    }

    pub(super) fn decode(&self, result: QueryResult) -> Result<DataRecord, StorageError> {
        let schema = &self.schema;
        let row = result.try_as_sqlite_row().ok_or(StorageError::Corrupt { what: "a result is not a SQLite row" })?;
        use sea_orm::sqlx::Column;
        let mut fields = Map::new();
        for (index, col) in row.columns().iter().enumerate() {
            if self.joined_columns.contains(col.name()) {
                continue;
            }
            let field = self.fields.get(col.name()).map(|index| &schema.fields[*index])
                .ok_or_else(|| StorageError::Undecodable { collection: schema.collection.clone().into(), column: col.name().to_owned().into() })?;
            fields.insert(field.name.clone(), scalar(row, index, field)?);
        }
        let metadata_string = |name: &'static str| -> Result<String, StorageError> {
            match fields.get(name) {
                Some(Value::String(s)) => Ok(s.clone()),
                _ => Err(StorageError::Projection { name }),
            }
        };
        // Entity decoding consumes data, including flattened BaseEntity fields. The
        // record envelope also exposes those small values for generic storage callers.
        let id = metadata_string("id")?;
        let created_at = metadata_string("createdAt")?;
        let updated_at = metadata_string("updatedAt")?;
        let version = fields
            .get("version")
            .and_then(|v| v.as_u64())
            .ok_or(StorageError::Projection { name: "version" })?;
        Ok(DataRecord {
            id,
            collection: schema.collection.clone(),
            data: Value::Object(fields),
            metadata: RecordMetadata {
                created_at,
                updated_at,
                version: version.try_into().map_err(|_| StorageError::Overflow { what: "a record version" })?,
                tags: None,
                schema: None,
                ttl: None,
            },
        })
    }
}
