use super::*;

#[async_trait]
impl StorageAdapter for PostgresAdapter {
    fn name(&self) -> &'static str {
        "postgres"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_transactions: true,
            supports_indexing: true,
            supports_full_text_search: true,
            supports_vector_search: false, // pgvector would be a separate feature
            supports_joins: true,
            supports_batch: true,
            max_record_size: 1_073_741_824, // 1GB
        }
    }

    async fn initialize(&mut self, config: AdapterConfig) -> Result<(), String> {
        use crate::clog_info;

        if let Some(ns) = &config.namespace {
            self.schema = ns.clone();
        }

        let mut pg_config = Config::new();
        pg_config.url = Some(config.connection_string.clone());
        pg_config.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });
        // Apply max_connections and wait timeout to deadpool.
        // wait timeout = 10s prevents pool.get() from blocking forever when all
        // connections are in use. Without this, pool exhaustion causes rayon thread
        // starvation and cascading IPC death.
        pg_config.pool = Some(deadpool_postgres::PoolConfig {
            max_size: config.max_connections,
            timeouts: deadpool_postgres::Timeouts {
                wait: Some(std::time::Duration::from_secs(10)),
                create: Some(std::time::Duration::from_secs(10)),
                recycle: Some(std::time::Duration::from_secs(5)),
            },
            ..Default::default()
        });

        let pool = pg_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| format!("Failed to create Postgres pool: {}", e))?;

        // Verify connectivity
        let client = pool
            .get()
            .await
            .map_err(|e| format!("Failed to connect to Postgres: {}", e))?;

        // Ensure schema exists
        client
            .execute(&format!("CREATE SCHEMA IF NOT EXISTS {}", self.schema), &[])
            .await
            .map_err(|e| format!("Failed to create schema: {}", e))?;

        clog_info!(
            "PostgresAdapter initialized: pool_size={}, schema={}",
            config.max_connections,
            self.schema
        );

        self.pool = Some(pool);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), String> {
        // Drop the pool - all connections will be closed
        self.pool = None;
        Ok(())
    }

    async fn create(&self, record: DataRecord) -> StorageResult<DataRecord> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };
        self.create_on(&client, record).await
    }

    async fn read(&self, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };
        self.read_on(&client, collection, id).await
    }

    async fn query(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let bare_table = naming::to_table_name(&query.collection);
        let table = self.table_ref(&query.collection);

        // Schema evolution: ensure table + columns exist before SELECT.
        // Without this, querying a stale table with new entity fields fails with
        // "column X does not exist". Build a dummy data object from the SELECT
        // projection so ensure_table_exists_pg can ALTER TABLE ADD COLUMN.
        if let Some(ref cols) = query.select {
            let mut dummy = serde_json::Map::new();
            for col in cols {
                dummy.insert(col.clone(), Value::Null);
            }
            let schema_changed = match self
                .ensure_table_exists_cached(&client, &table, &bare_table, &Value::Object(dummy))
                .await
            {
                Ok(c) => c,
                Err(e) => return StorageResult::err(e),
            };
            if schema_changed {
                self.invalidate_column_cache(&bare_table).await;
            }
        }

        let col_types = self.cached_column_types(&client, &bare_table).await;
        let (where_clause, where_params) = build_where_clause(&query.filter, 0, &col_types);
        let order_clause = build_order_clause(&query.sort);

        let select_clause = build_select_clause(&query.select);
        let mut sql = format!("SELECT {} FROM {}", select_clause, table);
        if !where_clause.is_empty() {
            sql.push(' ');
            sql.push_str(&where_clause);
        }
        if !order_clause.is_empty() {
            sql.push(' ');
            sql.push_str(&order_clause);
        }
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let params_ref: Vec<&(dyn ToSql + Sync)> = where_params
            .iter()
            .map(|b| &**b as &(dyn ToSql + Sync))
            .collect();

        let rows = match client.query(&sql, &params_ref).await {
            Ok(r) => r,
            Err(e) => {
                let msg = format_pg_error(&e);
                if msg.contains("does not exist") {
                    return StorageResult::ok(Vec::new());
                }
                return StorageResult::err(format!("Query failed: {}", msg));
            }
        };

        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            match row_to_record(row, &query.collection, row.columns()) {
                Ok(record) => records.push(record),
                Err(e) => return StorageResult::err(format!("Row conversion failed: {}", e)),
            }
        }

        StorageResult::ok(records)
    }

    async fn query_with_join(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(&query.collection);

        // If no joins specified, delegate to simple query
        let joins = match &query.joins {
            Some(j) if !j.is_empty() => j,
            _ => return self.query(query).await,
        };

        // Build JOIN SQL — apply column projection to main table if specified
        let main_select = match &query.select {
            Some(cols) if !cols.is_empty() => {
                let mut parts = vec![
                    format!("{}.id", table),
                    format!("{}.created_at", table),
                    format!("{}.updated_at", table),
                    format!("{}.version", table),
                ];
                for col in cols {
                    let snake = naming::to_snake_case(col);
                    if snake != "id"
                        && snake != "created_at"
                        && snake != "updated_at"
                        && snake != "version"
                    {
                        parts.push(format!("{}.{}", table, snake));
                    }
                }
                parts.join(", ")
            }
            _ => format!("{}.*", table),
        };
        let mut select_parts = vec![main_select];
        let mut join_clauses = Vec::new();

        for join in joins {
            let join_table = self.table_ref(&join.collection);
            let join_type_sql = match join.join_type {
                super::super::query::JoinType::Left => "LEFT JOIN",
                super::super::query::JoinType::Inner => "INNER JOIN",
            };

            let local_col = naming::to_snake_case(&join.local_field);
            let foreign_col = naming::to_snake_case(&join.foreign_field);

            // Select specific fields or all
            if let Some(fields) = &join.select {
                for field in fields {
                    let pg_col = naming::to_snake_case(field);
                    select_parts.push(format!(
                        "{}.{} AS {}__{}",
                        join_table, pg_col, join.alias, pg_col
                    ));
                }
            } else {
                select_parts.push(format!("{}.*", join_table));
            }

            join_clauses.push(format!(
                "{} {} ON {}.{} = {}.{}",
                join_type_sql, join_table, table, local_col, join_table, foreign_col
            ));
        }

        let bare_table = naming::to_table_name(&query.collection);
        let col_types = self.cached_column_types(&client, &bare_table).await;
        let (where_clause, where_params) = build_where_clause(&query.filter, 0, &col_types);
        let order_clause = build_order_clause(&query.sort);

        let mut sql = format!(
            "SELECT {} FROM {} {}",
            select_parts.join(", "),
            table,
            join_clauses.join(" ")
        );

        if !where_clause.is_empty() {
            sql.push(' ');
            sql.push_str(&where_clause);
        }
        if !order_clause.is_empty() {
            sql.push(' ');
            sql.push_str(&order_clause);
        }
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        let params_ref: Vec<&(dyn ToSql + Sync)> = where_params
            .iter()
            .map(|b| &**b as &(dyn ToSql + Sync))
            .collect();

        let rows = match client.query(&sql, &params_ref).await {
            Ok(r) => r,
            Err(e) => return StorageResult::err(format!("Join query failed: {}", e)),
        };

        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            match row_to_record(row, &query.collection, row.columns()) {
                Ok(record) => records.push(record),
                Err(e) => return StorageResult::err(format!("Row conversion failed: {}", e)),
            }
        }

        StorageResult::ok(records)
    }

    async fn count(&self, query: StorageQuery) -> StorageResult<usize> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let bare_table = naming::to_table_name(&query.collection);
        let table = self.table_ref(&query.collection);
        let col_types = self.cached_column_types(&client, &bare_table).await;
        let (where_clause, where_params) = build_where_clause(&query.filter, 0, &col_types);

        let mut sql = format!("SELECT COUNT(*) FROM {}", table);
        if !where_clause.is_empty() {
            sql.push(' ');
            sql.push_str(&where_clause);
        }

        let params_ref: Vec<&(dyn ToSql + Sync)> = where_params
            .iter()
            .map(|b| &**b as &(dyn ToSql + Sync))
            .collect();

        match client.query_one(&sql, &params_ref).await {
            Ok(row) => {
                let count: i64 = row.get(0);
                StorageResult::ok(count as usize)
            }
            Err(e) => {
                let msg = format_pg_error(&e);
                if msg.contains("does not exist") {
                    return StorageResult::ok(0);
                }
                StorageResult::err(format!("Count failed: {}", msg))
            }
        }
    }

    async fn update(
        &self,
        collection: &str,
        id: &UUID,
        data: Value,
        increment_version: bool,
    ) -> StorageResult<DataRecord> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };
        self.update_on(&client, collection, id, data, increment_version).await
    }

    async fn delete(&self, collection: &str, id: &UUID) -> StorageResult<bool> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };
        self.delete_on(&client, collection, id).await
    }

    async fn batch(&self, operations: Vec<BatchOperation>) -> StorageResult<Vec<Value>> {
        // ONE checked-out connection for the whole batch. Dispatching through
        // `self.create(...)` per operation (as this did before) takes a DIFFERENT
        // pooled connection each time, so a transaction opened on any one of them
        // could not cover the others — which is why the batch could half-apply.
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        // `mut` because `transaction()` borrows the client mutably.
        let mut client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        // ── Schema readiness happens BEFORE the transaction opens ──────────
        //
        // Both remaining review findings come from DDL running INSIDE the batch:
        // postgres aborts a transaction on the first error and rejects every
        // later statement, so update's missing-column ALTER retry could never
        // succeed in there; and the column caches are published when DDL RUNS,
        // so a rolled-back batch left them asserting columns the database no
        // longer had. Ensuring here — on the pooled client, in autocommit, before
        // any transaction exists — removes both: nothing inside the transaction
        // issues DDL, so nothing inside it can abort on DDL or be rolled back
        // underneath a cache that already published. Astra's steer, and it beats
        // per-op savepoints because it deletes the failure rather than recovering
        // from it.
        for op in &operations {
            let Some(data) = op.data.as_ref() else { continue };
            if !matches!(
                op.operation_type,
                BatchOperationType::Create | BatchOperationType::Update
            ) {
                continue;
            }
            let bare_table = naming::to_table_name(&op.collection);
            let qualified_table = self.table_ref(&op.collection);
            match self
                .ensure_table_exists_cached(&client, &qualified_table, &bare_table, data)
                .await
            {
                Ok(changed) => {
                    if changed {
                        self.invalidate_column_cache(&bare_table).await;
                    }
                }
                // Fail BEFORE opening the transaction: a batch that cannot have
                // its schema is not a batch that should half-open one.
                Err(e) => {
                    return StorageResult::err(format!(
                        "Batch schema readiness failed for '{}': {}",
                        op.collection, e
                    ))
                }
            }
        }

        // A REAL `tokio_postgres::Transaction`, not a raw BEGIN. Its `Drop` impl
        // issues the rollback, which is the whole point: an error path I write can
        // only cover the failures I anticipated, but a CANCELLED future never
        // reaches any of my code at all. With a raw BEGIN, a cancelled batch
        // returned a connection to the pool with an ACTIVE transaction on it and
        // poisoned the next borrower. The guard cannot be forgotten because it is
        // not a code path (Astra, 2026-09-08: "prefer the existing transaction
        // guard").
        let tx = match client.transaction().await {
            Ok(tx) => tx,
            Err(e) => return StorageResult::err(format!("Failed to open batch transaction: {}", e)),
        };

        let mut results = Vec::with_capacity(operations.len());
        for (index, op) in operations.into_iter().enumerate() {
            let outcome: Result<Value, String> = match op.operation_type {
                BatchOperationType::Create => match (op.id, op.data) {
                    (Some(id), Some(data)) => {
                        let record = DataRecord {
                            id,
                            collection: op.collection,
                            data,
                            metadata: RecordMetadata::default(),
                        };
                        let r = self.create_on(&tx, record).await;
                        if r.success {
                            Ok(json!({"success": true}))
                        } else {
                            Err(r.error.unwrap_or_else(|| "create failed".to_string())) // the adapter reported failure without detail; the operation still failed
                        }
                    }
                    _ => Err("create requires both id and data".to_string()),
                },
                BatchOperationType::Read => match op.id {
                    Some(id) => {
                        let r = self.read_on(&tx, &op.collection, &id).await;
                        if r.success {
                            Ok(json!({"success": true, "data": r.data}))
                        } else {
                            Err(r.error.unwrap_or_else(|| "read failed".to_string())) // the adapter reported failure without detail; the operation still failed
                        }
                    }
                    None => Err("read requires an id".to_string()),
                },
                BatchOperationType::Update => match (op.id, op.data) {
                    (Some(id), Some(data)) => {
                        let r = self.update_on(&tx, &op.collection, &id, data, true).await;
                        if r.success {
                            Ok(json!({"success": true}))
                        } else {
                            Err(r.error.unwrap_or_else(|| "update failed".to_string())) // the adapter reported failure without detail; the operation still failed
                        }
                    }
                    _ => Err("update requires both id and data".to_string()),
                },
                BatchOperationType::Delete => match op.id {
                    Some(id) => {
                        let r = self.delete_on(&tx, &op.collection, &id).await;
                        if r.success {
                            Ok(json!({"success": true}))
                        } else {
                            Err(r.error.unwrap_or_else(|| "delete failed".to_string())) // the adapter reported failure without detail; the operation still failed
                        }
                    }
                    None => Err("delete requires an id".to_string()),
                },
            };

            match outcome {
                Ok(value) => results.push(value),
                Err(e) => {
                    // No explicit ROLLBACK: returning here drops `tx`, and its Drop
                    // rolls back. Writing the rollback by hand would be a second
                    // path that has to stay correct — and would still miss the
                    // cancellation case the guard covers for free.
                    return StorageResult::err(format!(
                        "Batch operation {} failed, entire batch rolled back: {}",
                        index, e
                    ));
                }
            }
        }

        // Committing CONSUMES the transaction, so nothing can be applied after
        // this point, and a failure to commit drops it — rolled back, not
        // half-applied.
        if let Err(e) = tx.commit().await {
            return StorageResult::err(format!("Failed to commit batch: {}", e));
        }
        StorageResult::ok(results)
    }

    async fn ensure_schema(&self, schema: CollectionSchema) -> StorageResult<bool> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(&schema.collection);

        let mut columns = vec![
            "id TEXT PRIMARY KEY".to_string(),
            "created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()".to_string(),
            "updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()".to_string(),
            "version BIGINT NOT NULL DEFAULT 1".to_string(),
        ];

        for field in &schema.fields {
            let col_name = naming::to_snake_case(&field.name);
            // Skip BaseEntity columns — adapter hardcodes them above.
            // See sqlite.rs::do_ensure_schema for the rationale.
            if super::super::entity::is_base_entity_column(&col_name) {
                continue;
            }
            let col_type = pg_type_from_field_type(&field.field_type);

            let mut col_def = format!("{} {}", col_name, col_type);
            if !field.nullable {
                col_def.push_str(" NOT NULL");
            }
            if field.unique {
                col_def.push_str(" UNIQUE");
            }
            columns.push(col_def);
        }

        // Foreign keys — same shape as sqlite.rs::do_ensure_schema.
        // Per [[no-fallbacks-ever]] + Joel 2026-06-03 ("provide a
        // relational db this time"): emit FOREIGN KEY constraints
        // inside CREATE TABLE so the DB enforces referential
        // integrity, not application code.
        for field in &schema.fields {
            if let Some(fk) = &field.foreign_key {
                let col_name = naming::to_snake_case(&field.name);
                let target_table = self.table_ref(&fk.collection);
                let target_col = naming::to_snake_case(&fk.field);
                columns.push(format!(
                    "FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {} ON UPDATE {}",
                    col_name,
                    target_table,
                    target_col,
                    pg_cascade_rule_sql(fk.on_delete),
                    pg_cascade_rule_sql(fk.on_update),
                ));
            }
        }

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} ({})",
            table,
            columns.join(", ")
        );

        if let Err(e) = client.execute(&sql, &[]).await {
            // Concurrent DDL race: table may already exist
            let is_race = e.as_db_error().map_or(false, |db| {
                matches!(
                    db.code(),
                    &tokio_postgres::error::SqlState::DUPLICATE_TABLE
                        | &tokio_postgres::error::SqlState::UNIQUE_VIOLATION
                )
            });
            if !is_race {
                return StorageResult::err(format!("Create table failed: {}", format_pg_error(&e)));
            }
        }

        // Create indexes
        for field in &schema.fields {
            if field.indexed {
                let col_name = naming::to_snake_case(&field.name);
                let idx_name = format!("idx_{}_{}", table, col_name);
                let idx_sql = format!(
                    "CREATE INDEX IF NOT EXISTS {} ON {} ({})",
                    idx_name, table, col_name
                );
                if let Err(e) = client.execute(&idx_sql, &[]).await {
                    return StorageResult::err(format!(
                        "Create index failed: {}",
                        format_pg_error(&e)
                    ));
                }
            }
        }

        // Create composite indexes
        for index in &schema.indexes {
            let cols: Vec<String> = index
                .fields
                .iter()
                .map(|f| naming::to_snake_case(f))
                .collect();
            let unique = if index.unique { "UNIQUE " } else { "" };
            let idx_sql = format!(
                "CREATE {}INDEX IF NOT EXISTS {} ON {} ({})",
                unique,
                naming::to_snake_case(&index.name),
                table,
                cols.join(", ")
            );
            if let Err(e) = client.execute(&idx_sql, &[]).await {
                return StorageResult::err(format!(
                    "Create composite index failed: {}",
                    format_pg_error(&e)
                ));
            }
        }

        // Phase 2 Step 4: seed the ensured_columns_cache (m5's QW#1) with
        // the DECLARED column set from the schema. Subsequent writes skip
        // the ensure_table_exists_cached probe entirely — zero runtime
        // data-shape inference on the hot write path. Previously the cache
        // was populated lazily on first write and grew column-by-column;
        // now it starts authoritative.
        //
        // Merge semantics (per m5-test review of PR #904): extend, don't
        // replace. If ensure_table_exists_cached populated the entry first
        // (theoretical ordering window — ensure_schema typically runs before
        // any write, but nothing enforces that), we union our declared set
        // into what's already known rather than dropping it. Matches the
        // `or_insert_with + insert` pattern at ensure_table_exists_cached.
        //
        // Bare-table name matches what ensure_table_exists_cached uses as
        // the cache key (the unqualified collection → snake_case conversion).
        let bare_table = naming::to_table_name(&schema.collection);
        {
            let mut cache = self.ensured_columns_cache.write().await;
            let entry = cache.entry(bare_table).or_insert_with(HashSet::new);
            entry.insert("id".to_string());
            entry.insert("created_at".to_string());
            entry.insert("updated_at".to_string());
            entry.insert("version".to_string());
            for field in &schema.fields {
                entry.insert(naming::to_snake_case(&field.name));
            }
        }

        StorageResult::ok(true)
    }

    async fn list_collections(&self) -> StorageResult<Vec<String>> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let sql = format!(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = '{}' AND table_type = 'BASE TABLE'",
            self.schema
        );

        let rows = match client.query(&sql, &[]).await {
            Ok(r) => r,
            Err(e) => return StorageResult::err(format!("Query failed: {}", e)),
        };

        let tables: Vec<String> = rows.iter().map(|r| r.get::<_, String>(0)).collect();
        StorageResult::ok(tables)
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

    async fn truncate(&self, collection: &str) -> StorageResult<bool> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(collection);
        let sql = format!("TRUNCATE TABLE {} CASCADE", table);

        match client.execute(&sql, &[]).await {
            Ok(_) => StorageResult::ok(true),
            Err(e) => StorageResult::err(format!("Truncate failed: {}", e)),
        }
    }

    async fn clear_all(&self) -> StorageResult<ClearAllResult> {
        let tables_result = self.list_collections().await;
        let tables = match tables_result.data {
            Some(t) => t,
            None => return StorageResult::err(tables_result.error.unwrap_or_default()),
        };

        let mut cleared = Vec::new();
        for table in &tables {
            if self.truncate(table).await.success {
                cleared.push(table.clone());
            }
        }

        StorageResult::ok(ClearAllResult {
            tables_cleared: cleared,
            records_deleted: 0,
        })
    }

    async fn cleanup(&self) -> Result<(), String> {
        let pool = self.pool()?;
        let client = pool.get().await.map_err(|e| format!("Pool error: {}", e))?;

        // VACUUM FULL requires exclusive lock, use regular VACUUM + ANALYZE
        client
            .execute("VACUUM", &[])
            .await
            .map_err(|e| format!("Vacuum failed: {}", e))?;

        client
            .execute("ANALYZE", &[])
            .await
            .map_err(|e| format!("Analyze failed: {}", e))?;

        Ok(())
    }
}
