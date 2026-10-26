use super::*;

// Migration command params now live with their typed ActionCommands in
// commands/migration/*. The `migration/*` output structs (MigrationProgress,
// MigrationVerification on the engine; MigrationCutover, MigrationRollback below)
// are the wire contracts.
impl DataState {
    /// In-process command dispatch — the legacy `match` over `data/*`,
    /// `adapter/*`, `vector/*`, `migration/*`. The [`DataModule`] shell's
    /// `handle_command` delegates here so dispatch logic lives with the state it
    /// touches. Each typed `data/*` command in `commands/data/*` calls the
    /// corresponding `handle_*` directly, bypassing this string match — this
    /// stays only until every arm is migrated and Registry A is retired.
    pub(crate) async fn dispatch(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        log_info!(
            "data",
            "handle_command",
            "Received: {} params: {}",
            command,
            params
        );
        match command {
            // data/create, data/read, data/update, data/delete are now typed,
            // self-routing commands (`commands/data/{create,read,update,delete}.rs`)
            // driving `DataState::{create,read,update,delete}_record`. The typed
            // object map wins over this match, so their legacy arms are gone.
            // `data/list` is now the typed, persona-facing command
            // (`commands/data/list.rs`) — routed via the typed object map, which
            // wins over this legacy arm. `data/query` keeps the explicit-`db_path`
            // `QueryParams` shape its internal Rust callers (chat, channel,
            // self-task) depend on.
            "data/query" => {
                self.handle_query(deserialize_params!(command, params)?)
                    .await
            }
            "data/queryWithJoin" => {
                self.handle_query_with_join(deserialize_params!(command, params)?)
                    .await
            }
            // data/count, data/list-collections, data/collection-stats, data/batch,
            // data/ensure-schema, data/truncate, data/clear-all are now typed
            // self-routing commands (commands/data/*.rs) driving the corresponding
            // DataState methods. The typed object map wins over this match, so their
            // legacy arms are gone.

            // Paginated queries - server-side cursor management
            "data/query-open" => {
                self.handle_query_open(deserialize_params!(command, params)?)
                    .await
            }
            "data/query-next" => {
                let req = CommandRequest::<QueryNextParams>::from_value(params)?;
                self.handle_query_next(req).await
            }
            "data/query-close" => {
                let req = CommandRequest::<QueryCloseParams>::from_value(params)?;
                self.handle_query_close(req).await
            }

            // adapter/info is now a typed self-routing command
            // (commands/adapter/info.rs) driving DataState::adapter_info; it
            // subsumes the old adapter/capabilities (capabilities are a field on
            // the AdapterInfo result now, not a parallel command).

            // vector/* are now typed self-routing commands (commands/vector/*.rs)
            // driving DataState::{vector_search,index_vector,vector_stats,
            // invalidate_vector_cache,backfill_vectors}. Each carries a real result
            // struct (VectorSearchResults / VectorStats / …) instead of an ad-hoc
            // json! blob, so the persona surface, codegen, and uu see the shape.

            // migration/* fully migrated to typed ActionCommands (commands/migration/*,
            // contributed via DataModule::commands()); no legacy arm remains.
            _ => Err(format!("Unknown data command: {command}")),
        }
    }

    /// Read a record by id (single source for the typed `data/read` command).
    pub(crate) async fn read_record(
        &self,
        handle: &str,
        collection: &str,
        id: &UUID,
    ) -> Result<StorageResult<DataRecord>, String> {
        require_public_collection(collection)?;
        let start = std::time::Instant::now();
        let adapter = self.get_adapter(handle).await?;
        let result = adapter.read(collection, id).await;
        self.log_slow_query("read", collection, start.elapsed().as_millis());
        Ok(result)
    }

    pub(super) async fn handle_query(&self, params: QueryParams) -> Result<CommandResult, String> {
        require_public_collection(&params.collection)?;
        require_query_fields(&params.filter, &params.sort, &params.select)?;
        // Limit concurrent queries to cap peak heap from 15 personas querying simultaneously.
        // Excess callers wait (not rejected) — bounded concurrency, not dropped work.
        let _permit = self
            .query_semaphore
            .acquire()
            .await
            .map_err(|_| "query semaphore closed")?;

        use std::time::Instant;
        let start = Instant::now();

        let query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter,
            sort: params.sort,
            limit: params.limit,
            offset: params.offset,
            select: params.select,
            ..Default::default()
        };

        // Log when column projection is active (visibility into optimization)
        if let Some(ref select) = query.select {
            log_info!(
                "data",
                "query",
                "Column projection active for {}: SELECT {} (instead of *)",
                params.collection,
                select.join(", ")
            );
        }

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.query(query).await;
        let total_ms = start.elapsed().as_millis();

        // Log slow queries to module log file
        self.log_slow_query("query", &params.collection, total_ms);

        CommandResult::json(&result)
    }

    /// Persona/UI-facing list: read a collection by an intuitive plain-JSON
    /// filter + optional ordering/paging, returning the matching records and an
    /// accurate `total` (SQL COUNT, not `items.len()`). The storage handle
    /// defaults to "main" (the shared DB) — callers never name a `db_path`;
    /// power callers may target a per-persona store via `handle`.
    ///
    /// This is the typed `data/list` command's body (`commands/data/list.rs`).
    /// The legacy `data/query` arm keeps the explicit-`db_path` `QueryParams`
    /// shape its internal Rust callers (chat, channel, self-task) still use.
    pub(crate) async fn list(&self, params: DataListParams) -> Result<DataListResult, String> {
        require_public_collection(&params.collection)?;
        // Bound peak heap when many personas list concurrently (same gate as
        // handle_query). Excess callers wait — bounded, not dropped.
        let _permit = self
            .query_semaphore
            .acquire()
            .await
            .map_err(|_| "query semaphore closed")?;

        let handle = params.handle.as_deref().unwrap_or("main");

        // Plain-JSON filter → typed FieldFilter map. FieldFilter is untagged, so
        // `{"roomId": "general"}` becomes an equality match and
        // `{"age": {"$gt": 18}}` an operator match — natural JSON, no special
        // syntax for the persona to learn. Bad filter shape fails loud.
        let filter = match params.filter {
            Some(v) => Some(
                serde_json::from_value::<HashMap<String, FieldFilter>>(v)
                    .map_err(|e| format!("data/list: invalid filter — {e}"))?,
            ),
            None => None,
        };

        let sort = params.sort.map(|clauses| {
            clauses
                .into_iter()
                .map(|c| SortSpec {
                    field: c.field,
                    direction: match c.direction {
                        SortDir::Asc => SortDirection::Asc,
                        SortDir::Desc => SortDirection::Desc,
                    },
                })
                .collect::<Vec<_>>()
        });

        require_query_fields(&filter, &sort, &None)?;
        let adapter = self.get_adapter(handle).await?;

        // Accurate total: SQL COUNT over the same filter, independent of paging.
        let count_res = adapter
            .count(StorageQuery {
                collection: params.collection.clone(),
                filter: filter.clone(),
                ..Default::default()
            })
            .await;
        if !count_res.success {
            return Err(format!(
                "data/list: count failed for '{}' — {}",
                params.collection,
                count_res.error.unwrap_or_else(|| "unknown error".into())
            ));
        }
        let total = count_res.data.unwrap_or(0) as u32;

        let res = adapter
            .query(StorageQuery {
                collection: params.collection.clone(),
                filter,
                sort,
                limit: params.limit.map(|n| n as usize),
                offset: params.offset.map(|n| n as usize),
                ..Default::default()
            })
            .await;
        if !res.success {
            return Err(format!(
                "data/list: query failed for '{}' — {}",
                params.collection,
                res.error.unwrap_or_else(|| "unknown error".into())
            ));
        }

        let records = res.data.unwrap_or_default();
        let mut items = Vec::with_capacity(records.len());
        for r in records {
            items.push(
                serde_json::to_value(r)
                    .map_err(|e| format!("data/list: serialize record — {e}"))?,
            );
        }

        Ok(DataListResult { items, total })
    }

    pub(super) async fn handle_query_with_join(
        &self,
        params: QueryWithJoinParams,
    ) -> Result<CommandResult, String> {
        require_public_collection(&params.collection)?;
        require_query_fields(&params.filter, &params.sort, &params.select)?;
        for join in params.joins.iter().flatten() {
            require_public_collection(&join.collection)?;
            require_identifier(&join.alias)?;
            require_field_name(&join.local_field)?;
            require_field_name(&join.foreign_field)?;
            require_query_fields(&None, &None, &join.select)?;
        }
        let _permit = self
            .query_semaphore
            .acquire()
            .await
            .map_err(|_| "query semaphore closed")?;

        let query = StorageQuery {
            collection: params.collection,
            filter: params.filter,
            sort: params.sort,
            limit: params.limit,
            offset: params.offset,
            joins: params.joins,
            select: params.select,
            ..Default::default()
        };

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.query_with_join(query).await;

        CommandResult::json(&result)
    }

    /// Count records in a collection (optionally filtered). Drives the typed
    /// `data/count` command. Returns the adapter's `StorageResult<usize>` — an
    /// accurate SQL COUNT, paging-independent.
    pub(crate) async fn count_records(
        &self,
        handle: &str,
        collection: String,
        filter: Option<serde_json::Map<String, Value>>,
    ) -> Result<StorageResult<usize>, String> {
        require_public_collection(&collection)?;
        for name in filter.iter().flat_map(|fields| fields.keys()) {
            require_field_name(name)?;
        }
        use std::time::Instant;
        let start = Instant::now();

        let query = StorageQuery {
            collection: collection.clone(),
            filter: filter.map(|m| {
                m.into_iter()
                    .map(|(k, v)| (k, FieldFilter::Value(v)))
                    .collect()
            }),
            ..Default::default()
        };

        let adapter_start = Instant::now();
        let adapter = self.get_adapter(handle).await?;
        let adapter_ms = adapter_start.elapsed().as_millis();

        let count_start = Instant::now();
        let result = adapter.count(query).await;
        let count_ms = count_start.elapsed().as_millis();

        let total_ms = start.elapsed().as_millis();
        if total_ms > 50 {
            log_info!(
                "data",
                "count",
                "TIMING: collection={}, total={}ms (adapter={}ms, count={}ms), success={}",
                collection,
                total_ms,
                adapter_ms,
                count_ms,
                result.success
            );
        }

        Ok(result)
    }

    /// Phase 2 Step 3: ensure_schema pivot through resolve().
    /// Typed dispatch via deserialize_params! macro (Phase 1 primitive, m5).
    /// Schema content sourced from entity_schemas.json (build-time codegen
    /// from TS decorators), resolved per collection by the entity_schemas
    /// module. Unknown collection → hard fail with rebuild hint.
    /// Ensure a collection's schema exists, resolving the schema by collection
    /// NAME (the wire never carries inline SQL/fields/indexes). Drives
    /// `data/ensure-schema`.
    pub(crate) async fn ensure_collection_schema(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<StorageResult<bool>, String> {
        require_public_collection(collection)?;
        // Resolution order per [[orm-everything-not-hand-edited-files]]:
        //   1. Rust-native registry (substrate entities authored Rust-first:
        //      hw_tiers, role_templates, identity pools, universes).
        //   2. entity_schemas.json (TS-decorator authored: chat, users,
        //      cognition, timeline — the existing pipeline).
        //   3. Error — collection unknown to either path.
        let collection_schema = if let Some(rust_schema) =
            crate::orm::OrmEntityRegistry::global().resolve(collection)
        {
            rust_schema
        } else if let Some(entity) = crate::entity_schemas::resolve(collection) {
            crate::entity_schemas::to_collection_schema(entity)
        } else {
            return Err(format!(
                "Unknown collection '{collection}' — not in the Rust ORM registry and not in \
                 entity_schemas.json. If this is a newly added TS-decorated entity, \
                 rebuild TS: `npm run build:ts`. If it's a Rust-native substrate entity, \
                 confirm OrmEntityRegistry::global().register::<YourEntity>() is called \
                 at boot."
            ));
        };
        let adapter = self.get_adapter(handle).await?;
        Ok(adapter.ensure_schema(collection_schema).await)
    }

    /// List the collection names present in a store. Drives `data/list-collections`.
    pub(crate) async fn list_collection_names(
        &self,
        handle: &str,
    ) -> Result<StorageResult<Vec<String>>, String> {
        let adapter = self.get_adapter(handle).await?;
        let mut result = adapter.list_collections().await;
        if let Some(collections) = result.data.as_mut() {
            collections.retain(|name| require_public_collection(name).is_ok());
        }
        Ok(result)
    }

    /// Statistics for one collection (record count, size, last-modified, schema,
    /// indices). Drives `data/collection-stats`.
    pub(crate) async fn collection_statistics(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<StorageResult<crate::orm::types::CollectionStats>, String> {
        require_public_collection(collection)?;
        let adapter = self.get_adapter(handle).await?;
        Ok(adapter.collection_stats(collection).await)
    }

    /// Adapter identity + full capability surface for a store. Drives the typed
    /// `adapter/info` command (which subsumes the old `adapter/capabilities` —
    /// capabilities are now a field on this one result, not a parallel command).
    pub(crate) async fn adapter_info(&self, handle: &str) -> Result<AdapterInfo, String> {
        let adapter = self.get_adapter(handle).await?;
        Ok(AdapterInfo {
            adapter: adapter.name().to_string(),
            handle: handle.to_string(),
            capabilities: adapter.capabilities(),
        })
    }

    // =========================================================================
    // Vector Search (migrated from data-daemon-worker)
    // =========================================================================

    /// Parse embedding from record data (supports BLOB and JSON array)
    pub(super) fn parse_embedding(value: &Value) -> Vec<f64> {
        match value {
            Value::Array(arr) => arr.iter().filter_map(|v| v.as_f64()).collect(),
            Value::String(s) => {
                // Try parsing as JSON array
                serde_json::from_str(s).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// Cosine similarity between two vectors
    /// Uses 4-way loop unrolling for SIMD-like performance
    pub(super) fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let len = a.len();
        let limit = len - (len % 4);

        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;

        // 4-way unrolled loop
        let mut i = 0;
        while i < limit {
            let a0 = a[i];
            let a1 = a[i + 1];
            let a2 = a[i + 2];
            let a3 = a[i + 3];
            let b0 = b[i];
            let b1 = b[i + 1];
            let b2 = b[i + 2];
            let b3 = b[i + 3];

            dot += a0 * b0 + a1 * b1 + a2 * b2 + a3 * b3;
            norm_a += a0 * a0 + a1 * a1 + a2 * a2 + a3 * a3;
            norm_b += b0 * b0 + b1 * b1 + b2 * b2 + b3 * b3;
            i += 4;
        }

        // Handle remainder
        while i < len {
            dot += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
            i += 1;
        }

        let denominator = (norm_a * norm_b).sqrt();
        if denominator == 0.0 {
            0.0
        } else {
            dot / denominator
        }
    }

    /// Index a vector - store embedding for a record
    /// Updates the record's 'embedding' field with the provided vector
    /// Store an embedding on a record (the record's `embedding` field) and drop
    /// the collection's cached vector set so the next search reloads it.
    pub(crate) async fn index_vector(
        &self,
        handle: &str,
        collection: &str,
        id: String,
        embedding: Vec<f64>,
    ) -> Result<StorageResult<DataRecord>, String> {
        require_public_collection(collection)?;
        use std::time::Instant;
        let start = Instant::now();

        let (scope, adapter) = self.adapter_with_identity(handle).await?;

        // Update the record's embedding field
        let update_data = json!({ "embedding": embedding });

        let result = adapter.update(collection, &id, update_data, false).await;

        // Invalidate vector cache for this collection since we modified an embedding
        self.vector_cache.invalidate(&scope, collection);

        let total_ms = start.elapsed().as_millis();
        log_info!(
            "data",
            "vector/index",
            "Indexed vector for {} in {}ms, success={}",
            id,
            total_ms,
            result.success
        );

        Ok(result)
    }

    /// Get vector index statistics for a collection.
    pub(crate) async fn vector_stats(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<VectorStats, String> {
        require_public_collection(collection)?;
        use std::time::Instant;
        let start = Instant::now();

        let (scope, adapter) = self.adapter_with_identity(handle).await?;

        // Get total record count
        let total_query = StorageQuery {
            collection: collection.to_string(),
            ..Default::default()
        };
        let total_result = adapter.count(total_query).await;
        let total_records = total_result.data.unwrap_or(0);

        // Query to count records WITH embeddings
        // We need to query and check which have embedding field
        let query = StorageQuery {
            collection: collection.to_string(),
            limit: Some(10000), // Reasonable limit
            ..Default::default()
        };
        let result = adapter.query(query).await;

        let mut records_with_vectors = 0;
        let mut vector_dimensions = 0;

        if let Some(records) = result.data {
            for record in &records {
                if let Some(embedding) = record.data.get("embedding") {
                    let vec = Self::parse_embedding(embedding);
                    if !vec.is_empty() {
                        records_with_vectors += 1;
                        if vector_dimensions == 0 {
                            vector_dimensions = vec.len();
                        }
                    }
                }
            }
        }

        // Check cache status
        let cached_count = self.vector_cache.get::<Vec<CachedVector>>(&scope, collection)
            .map_or(0, |vectors| vectors.len());

        let total_ms = start.elapsed().as_millis();
        log_info!(
            "data",
            "vector/stats",
            "Stats for {} in {}ms: total={}, with_vectors={}, dims={}",
            collection,
            total_ms,
            total_records,
            records_with_vectors,
            vector_dimensions
        );

        Ok(VectorStats {
            collection: collection.to_string(),
            total_records,
            records_with_vectors,
            vector_dimensions,
            cached_vectors: cached_count,
            last_updated: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Invalidate vector cache for a collection
    /// Called when records are modified outside of vector/index
    pub(crate) async fn invalidate_vector_cache(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<VectorCacheInvalidation, String> {
        require_public_collection(collection)?;
        let scope = self.resolve_handle(handle)?;
        let removed = self.vector_cache.invalidate(&scope, collection);

        log_info!(
            "data",
            "vector/invalidate-cache",
            "Invalidated cache for {}: removed={}",
            collection,
            removed
        );

        Ok(VectorCacheInvalidation {
            collection: collection.to_string(),
            cache_invalidated: removed,
        })
    }
}
