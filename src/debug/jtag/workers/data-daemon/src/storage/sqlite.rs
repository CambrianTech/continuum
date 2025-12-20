use async_trait::async_trait;
use rusqlite::{Connection, Result as SqliteResult};
use serde_json::Value;
use std::error::Error;
use std::sync::{Arc, Mutex};

use super::adapter::StorageAdapter;

/// SQLite Storage Adapter
pub struct SqliteAdapter {
    connection: Arc<Mutex<Option<Connection>>>,
}

impl SqliteAdapter {
    pub fn new() -> Self {
        Self {
            connection: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl StorageAdapter for SqliteAdapter {
    async fn initialize(&mut self, config: Value) -> Result<(), Box<dyn Error>> {
        let filename = config.get("filename")
            .and_then(|f| f.as_str())
            .ok_or("Missing filename in config")?;

        let conn = Connection::open(filename)?;

        // Set pragmas for performance
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA temp_store = MEMORY;"
        )?;

        *self.connection.lock().unwrap() = Some(conn);
        Ok(())
    }

    async fn create(&self, collection: &str, record: Value) -> Result<Value, Box<dyn Error>> {
        let record_obj = record.as_object()
            .ok_or("Record must be a JSON object")?;

        // Build dynamic INSERT statement based on fields in record
        // Filter out metadata fields and convert camelCase to snake_case
        let mut columns: Vec<String> = Vec::new();
        let mut column_keys: Vec<String> = Vec::new();

        for key in record_obj.keys() {
            // Skip metadata fields that aren't database columns
            if key == "collection" || key == "metadata" {
                continue;
            }
            column_keys.push(key.clone());
            columns.push(to_snake_case(key));
        }

        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{}", i)).collect();

        let insert_sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            collection,
            columns.join(", "),
            placeholders.join(", ")
        );

        // Execute insert
        let conn_guard = self.connection.lock().unwrap();
        let conn = conn_guard.as_ref().ok_or("Not initialized")?;

        // Build params
        let mut stmt = conn.prepare(&insert_sql)?;
        let mut param_idx = 1;

        for col_key in &column_keys {
            let value = &record_obj[col_key];
            match value {
                Value::String(s) => stmt.raw_bind_parameter(param_idx, s)?,
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        stmt.raw_bind_parameter(param_idx, i)?;
                    } else if let Some(f) = n.as_f64() {
                        stmt.raw_bind_parameter(param_idx, f)?;
                    }
                },
                Value::Bool(b) => stmt.raw_bind_parameter(param_idx, *b as i64)?,
                Value::Null => stmt.raw_bind_parameter(param_idx, rusqlite::types::Null)?,
                _ => stmt.raw_bind_parameter(param_idx, value.to_string())?,
            }
            param_idx += 1;
        }

        stmt.raw_execute()?;

        Ok(record)
    }

    async fn read(&self, collection: &str, id: &str) -> Result<Option<Value>, Box<dyn Error>> {
        let conn_guard = self.connection.lock().unwrap();
        let conn = conn_guard.as_ref().ok_or("Not initialized")?;

        let query_sql = format!("SELECT * FROM {} WHERE id = ?1", collection);

        let result = conn.query_row(&query_sql, [id], |row| {
            // Build JSON object from all columns
            let mut obj = serde_json::Map::new();
            let col_count = row.as_ref().column_count();

            for i in 0..col_count {
                let col_name = row.as_ref().column_name(i).unwrap_or("unknown").to_string();

                let value: Value = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => Value::Null,
                    rusqlite::types::ValueRef::Integer(n) => serde_json::json!(n),
                    rusqlite::types::ValueRef::Real(f) => serde_json::json!(f),
                    rusqlite::types::ValueRef::Text(t) => {
                        Value::String(String::from_utf8_lossy(t).to_string())
                    }
                    rusqlite::types::ValueRef::Blob(b) => {
                        Value::String(format!("0x{}", hex::encode(b)))
                    }
                };

                obj.insert(col_name, value);
            }

            Ok(Value::Object(obj))
        });

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(Box::new(err)),
        }
    }

    async fn query(&self, query: Value) -> Result<Vec<Value>, Box<dyn Error>> {
        let collection = query.get("collection")
            .and_then(|c| c.as_str())
            .ok_or("Missing collection in query")?
            .to_string();

        // Build SQL query
        let mut sql = format!("SELECT * FROM {}", collection);

        // Add filters (simplified - no params for now)
        if let Some(filter) = query.get("filter").and_then(|f| f.as_object()) {
            let conditions: Vec<String> = filter.iter()
                .map(|(field, _value)| format!("{} = ?", to_snake_case(field)))
                .collect();
            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }
        }

        // Add ORDER BY
        if let Some(sort) = query.get("sort").and_then(|s| s.as_array()) {
            let sort_clauses: Vec<String> = sort.iter()
                .filter_map(|s| {
                    let field = s.get("field")?.as_str()?;
                    let dir = s.get("direction")?.as_str().unwrap_or("asc");
                    Some(format!("{} {}", to_snake_case(field), dir.to_uppercase()))
                })
                .collect();
            if !sort_clauses.is_empty() {
                sql.push_str(" ORDER BY ");
                sql.push_str(&sort_clauses.join(", "));
            }
        }

        // Add LIMIT
        if let Some(limit) = query.get("limit").and_then(|l| l.as_i64()) {
            if limit > 0 {
                sql.push_str(&format!(" LIMIT {}", limit));
            }
        }

        // Add OFFSET
        if let Some(offset) = query.get("offset").and_then(|o| o.as_i64()) {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        // Execute query - scope the lock
        let records = {
            let conn_guard = self.connection.lock().unwrap();
            let conn = conn_guard.as_ref().ok_or("Not initialized")?;

            let mut stmt = conn.prepare(&sql)?;
            let column_names: Vec<String> = stmt.column_names().iter()
                .map(|s| s.to_string())
                .collect();

            let rows: Result<Vec<serde_json::Map<String, Value>>, _> = stmt.query_map([], |row| {
                let mut obj = serde_json::Map::new();
                for (i, col_name) in column_names.iter().enumerate() {
                    let value: Value = match row.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => serde_json::json!(n),
                        rusqlite::types::ValueRef::Real(f) => serde_json::json!(f),
                        rusqlite::types::ValueRef::Text(t) => {
                            Value::String(String::from_utf8_lossy(t).to_string())
                        }
                        rusqlite::types::ValueRef::Blob(b) => {
                            Value::String(format!("0x{}", hex::encode(b)))
                        }
                    };
                    obj.insert(col_name.clone(), value);
                }
                Ok(obj)
            })?.collect();

            rows
        }?;

        // Build DataRecord format outside the lock
        let result: Vec<Value> = records.into_iter().map(|obj| {
            serde_json::json!({
                "id": obj.get("id").cloned().unwrap_or(Value::Null),
                "collection": &collection,
                "data": obj,
                "metadata": {
                    "createdAt": obj.get("created_at").cloned(),
                    "updatedAt": obj.get("updated_at").cloned(),
                    "version": obj.get("version").cloned().unwrap_or(serde_json::json!(1))
                }
            })
        }).collect();

        Ok(result)
    }

    async fn update(&self, collection: &str, id: &str, data: Value) -> Result<Value, Box<dyn Error>> {
        todo!("Implement update")
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<bool, Box<dyn Error>> {
        todo!("Implement delete")
    }

    async fn ensure_schema(&self, collection: &str, schema: Option<Value>) -> Result<bool, Box<dyn Error>> {
        // Tables are managed by TypeScript - no-op
        Ok(true)
    }

    async fn list_collections(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let conn_guard = self.connection.lock().unwrap();
        let conn = conn_guard.as_ref().ok_or("Not initialized")?;

        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
        )?;

        let collections: Vec<String> = stmt.query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(collections)
    }

    async fn get_collection_stats(&self, collection: &str) -> Result<Value, Box<dyn Error>> {
        let conn_guard = self.connection.lock().unwrap();
        let conn = conn_guard.as_ref().ok_or("Not initialized")?;

        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", collection),
            [],
            |row| row.get(0)
        )?;

        Ok(serde_json::json!({
            "name": collection,
            "recordCount": count,
            "totalSize": 0,
            "lastModified": chrono::Utc::now().to_rfc3339()
        }))
    }

    async fn close(&mut self) -> Result<(), Box<dyn Error>> {
        *self.connection.lock().unwrap() = None;
        Ok(())
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut prev_is_lower = false;
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 && prev_is_lower {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
            prev_is_lower = false;
        } else {
            result.push(ch);
            prev_is_lower = ch.is_lowercase();
        }
    }
    result
}
