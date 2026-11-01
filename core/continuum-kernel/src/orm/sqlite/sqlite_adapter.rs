use super::*;

// ─── Async Trait Implementation ──────────────────────────────────────────────
#[async_trait]
impl StorageAdapter for SqliteAdapter {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_transactions: true,
            supports_indexing: true,
            supports_full_text_search: false,
            supports_vector_search: false,
            supports_joins: true,
            supports_batch: true,
            max_record_size: 1_000_000_000,
        }
    }

    async fn initialize(&mut self, config: AdapterConfig) -> Result<(), String> {
        let path = config.connection_string.clone();
        let reader_count = config.max_connections.saturating_sub(1).max(1);

        // Open writer connection (read-write + create)
        let writer_flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let writer_conn = open_connection(&path, writer_flags)?;
        self.writer = Some(Arc::new(Mutex::new(writer_conn)));

        // Open reader pool (read-only connections)
        let reader_flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        for _ in 0..reader_count {
            let reader_conn = open_connection(&path, reader_flags)?;
            self.readers.push(Arc::new(Mutex::new(reader_conn)));
        }

        clog_info!(
            "SQLite adapter initialized: 1 writer + {} readers for {}",
            reader_count,
            path
        );
        Ok(())
    }

    async fn close(&mut self) -> Result<(), String> {
        // Drop all connection Arcs — connections close when last reference drops
        self.writer.take();
        self.readers.clear();
        Ok(())
    }

    // ─── READ operations → reader pool (concurrent via spawn_blocking) ──────

    async fn read(&self, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
        let conn = match self.get_reader() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let collection = collection.to_string();
        let id = id.clone();
        let pressure = self.last_pressure_check.clone();
        let types = self.declared_types(&collection);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            do_read(&conn, &collection, &id, types.as_deref())
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn query(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        if let Some(dummy) = projection_dummy(&query.select) {
            let writer = match self.get_writer() {
                Ok(c) => c,
                Err(e) => return StorageResult::err(e),
            };
            let table = naming::to_table_name(&query.collection);
            let types = self.declared_types(&query.collection);
            let ensure_result = tokio::task::spawn_blocking(move || {
                let conn = writer.lock().unwrap();
                ensure_table_exists(&conn, &table, &dummy)?;
                evolve_table_schema(&conn, &table, &dummy, types.as_deref());
                Ok::<(), String>(())
            })
            .await
            .unwrap_or_else(|e| Err(format!("spawn_blocking failed: {}", e)));
            if let Err(e) = ensure_result {
                return StorageResult::err(e);
            }
        }

        let conn = match self.get_reader() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let pressure = self.last_pressure_check.clone();
        let collection_name = query.collection.clone();
        let types = self.declared_types(&query.collection);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            let start = std::time::Instant::now();
            let result = do_query(&conn, query, types.as_deref());
            if start.elapsed().as_millis() > 100 {
                clog_warn!(
                    "SLOW query on {}: {}ms",
                    collection_name,
                    start.elapsed().as_millis()
                );
            }
            result
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn query_with_join(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        self.query(query).await
    }

    async fn count(&self, query: StorageQuery) -> StorageResult<usize> {
        let conn = match self.get_reader() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let pressure = self.last_pressure_check.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            do_count(&conn, query)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn list_collections(&self) -> StorageResult<Vec<String>> {
        let conn = match self.get_reader() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_list_collections(&conn)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn collection_stats(&self, collection: &str) -> StorageResult<CollectionStats> {
        let count_result = self
            .count(StorageQuery {
                collection: collection.to_string(),
                ..Default::default()
            })
            .await;

        let record_count = count_result.data.unwrap_or(0);

        StorageResult::ok(CollectionStats {
            name: collection.to_string(),
            record_count,
            total_size: 0,
            last_modified: chrono::Utc::now().to_rfc3339(),
            schema: None,
            indices: None,
        })
    }

    // ─── WRITE operations → dedicated writer (serialized via Mutex) ─────────

    async fn create(&self, record: DataRecord) -> StorageResult<DataRecord> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let pressure = self.last_pressure_check.clone();
        let types = self.declared_types(&record.collection);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            do_create(&conn, record, types.as_deref())
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn update(
        &self,
        collection: &str,
        id: &UUID,
        data: Value,
        increment_version: bool,
    ) -> StorageResult<DataRecord> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let collection = collection.to_string();
        let id = id.clone();
        let pressure = self.last_pressure_check.clone();
        let types = self.declared_types(&collection);
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            do_update(&conn, &collection, &id, data, increment_version, types.as_deref())
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn delete(&self, collection: &str, id: &UUID) -> StorageResult<bool> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let collection = collection.to_string();
        let id = id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_delete(&conn, &collection, &id)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn upsert_monotonic(&self, record: DataRecord, field: &str) -> StorageResult<bool> {
        self.upsert_monotonic_on_writer(record, field).await
    }

    async fn delete_matching(&self, query: StorageQuery) -> StorageResult<usize> {
        self.delete_matching_on_writer(query).await
    }

    async fn batch(&self, operations: Vec<BatchOperation>) -> StorageResult<Vec<Value>> {
        // Every operation runs on the SINGLE writer connection inside one
        // transaction. Dispatching through `self.create(...)` per operation (as
        // this did before) autocommits each row independently, which is why the
        // batch could half-apply.
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let pressure = self.last_pressure_check.clone();
        let declared = self.declared.read().map(|d| d.clone()).unwrap_or_default(); // unwrap_or: a poisoned lock reads as "nothing declared" — the legacy sniff, never a refused batch
        let sequences = Arc::clone(&self.sequences);
        let appends: Vec<(usize, String)> = operations
            .iter()
            .enumerate()
            .filter(|(_, op)| matches!(op.operation_type, BatchOperationType::Append(_)))
            .map(|(index, op)| (index, op.collection.clone()))
            .collect();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            let result = do_batch(&conn, operations, &declared);
            // Published inside the committing write, after COMMIT: the revision has the
            // commit's custody, so a caller dropped at the await cannot strand the row.
            if let Some(values) = result.data.as_ref().filter(|_| result.success) {
                for (index, collection) in &appends {
                    if let Some(seq) = values
                        .get(*index)
                        .and_then(|v| v.get("seq"))
                        .and_then(Value::as_u64)
                    {
                        sequences.publish(collection, seq);
                    }
                }
            }
            result
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    fn sequence_revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64> {
        self.sequences.subscribe(collection)
    }

    async fn ensure_schema(&self, schema: CollectionSchema) -> StorageResult<bool> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        // Remember the declared column types: reads decode by them (row_to_record).
        let types: HashMap<String, FieldType> =
            schema.fields.iter().map(|f| (f.name.clone(), f.field_type.clone())).collect();
        if let Ok(mut declared) = self.declared.write() {
            declared.insert(schema.collection.clone(), Arc::new(types));
        }
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_ensure_schema(&conn, schema)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn truncate(&self, collection: &str) -> StorageResult<bool> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let collection = collection.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_truncate(&conn, &collection)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn clear_all(&self) -> StorageResult<ClearAllResult> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_clear_all(&conn)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn cleanup(&self) -> Result<(), String> {
        let conn = self.get_writer()?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_cleanup(&conn)
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {}", e))?
    }
}
