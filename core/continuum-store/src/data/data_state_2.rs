use super::*;

impl DataState {

    /// Backfill vectors - generate embeddings for records missing them
    ///
    /// Uses batch embedding generation for efficiency (10x faster than single).
    /// Processes in configurable batch sizes to manage memory.
    /// Generate embeddings for records in a collection that lack one, in batches.
    /// `text_field` names the field to embed; `filter` narrows the records;
    /// `model` is advisory (the gateway selects the served embed model).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn backfill_vectors(
        &self,
        handle: &str,
        collection: &str,
        text_field: &str,
        batch_size: usize,
        model: Option<String>,
        filter: Option<std::collections::HashMap<String, FieldFilter>>,
    ) -> Result<VectorBackfillStats, String> {
        require_public_collection(collection)?;
        require_field_name(text_field)?;
        require_query_fields(&filter, &None, &None)?;
        use std::time::Instant;
        let start = Instant::now();

        // Embedding is adapter-routed (llama-server /v1/embeddings, task #40) — the
        // SAME async neural-or-lexical embedder the live recall path uses. Build
        // it ONCE for the whole backfill. `params.model` is advisory only now;
        // the served embed model is selected by the gateway. Fail loud if no
        // embedder can be built (no in-process ONNX fallback).
        let embedder = continuum_embedding::module::build_adapter_embedder()
            .await
            .map_err(|e| format!("vector/backfill: cannot build embedder: {e}"))?;

        // `model` is advisory only now — the served embed model is chosen
        // by the gateway (task #40). Log when a caller requested a specific model
        // so the divergence from the old fastembed model-name semantics is visible.
        if let Some(requested) = model.as_deref() {
            log_info!(
                "data",
                "vector/backfill",
                "requested embed model '{}' is advisory; using gateway embedder '{}'",
                requested,
                embedder.id()
            );
        }

        let (scope, adapter) = self.adapter_with_identity(handle).await?;

        // Query all records from collection
        let query = StorageQuery {
            collection: collection.to_string(),
            filter: filter.clone(),
            ..Default::default()
        };
        let query_result = adapter.query(query).await;
        if !query_result.success {
            return Err(query_result
                .error
                .unwrap_or_else(|| "Query failed".to_string()));
        }

        let records = query_result.data.unwrap_or_default();
        let total = records.len();
        let mut processed = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;

        log_info!(
            "data",
            "vector/backfill",
            "Starting backfill for {} records in {}",
            total,
            collection
        );

        // Process in batches for memory efficiency
        for chunk in records.chunks(batch_size) {
            // Collect texts that need embeddings
            let mut texts_to_embed: Vec<(usize, &str)> = Vec::new();

            for (i, record) in chunk.iter().enumerate() {
                // Check if already has embedding
                if let Some(embedding) = record.data.get("embedding") {
                    if !embedding.is_null() {
                        skipped += 1;
                        continue;
                    }
                }

                // Extract text from specified field
                if let Some(text) = record.data.get(text_field) {
                    if let Some(text_str) = text.as_str() {
                        if !text_str.is_empty() {
                            texts_to_embed.push((i, text_str));
                        }
                    }
                }
            }

            if texts_to_embed.is_empty() {
                continue;
            }

            // Embed each text through the adapter-routed embedder (async,
            // content-addressed cache, neural-or-lexical — task #40). An empty
            // vector means the embedder produced no signal for that text; skip
            // it as a failure rather than writing zeros.
            for (idx, text) in texts_to_embed.iter() {
                let embedding = embedder.embed(text).await;
                if embedding.is_empty() {
                    log_error!(
                        "data",
                        "vector/backfill",
                        "embedder returned no signal for record in {}",
                        collection
                    );
                    failed += 1;
                    continue;
                }

                let record = &chunk[*idx];

                // Convert f32 to f64 for JSON
                let embedding_f64: Vec<f64> = embedding.iter().map(|&v| v as f64).collect();

                let update_data = json!({
                    "embedding": embedding_f64
                });

                let update_result = adapter
                    .update(collection, &record.id, update_data, false)
                    .await;

                if update_result.success {
                    processed += 1;
                } else {
                    failed += 1;
                }
            }
        }

        // Invalidate vector cache since we modified embeddings
        self.vector_cache.invalidate(&scope, collection);

        let total_ms = start.elapsed().as_millis();
        log_info!(
            "data",
            "vector/backfill",
            "Backfill complete for {}: total={}, processed={}, skipped={}, failed={} in {}ms",
            collection,
            total,
            processed,
            skipped,
            failed,
            total_ms
        );

        Ok(VectorBackfillStats {
            collection: collection.to_string(),
            total,
            processed,
            skipped,
            failed,
            elapsed_ms: total_ms as u64,
        })
    }

    // =========================================================================
    // Paginated Query Handlers
    // =========================================================================

    /// Open a paginated query.
    ///
    /// Returns BOTH the legacy `queryId` string (for back-compat) AND a
    /// kernel-typed [`HandleRef`] minted via [`CommandResponse::with_handle`]
    /// — see PR #1485/#1486 for the cell-shape/envelope substrate. The
    /// two share an underlying UUID; new callers thread the handle, old
    /// callers keep reading `response.data.queryId`. A follow-up will
    /// drop the legacy field once every consumer has migrated.
    ///
    /// The handle's `owner` is `"data"` and its `type_tag` is
    /// `"data::QueryCursor"`. `data/query-next` and `data/query-close`
    /// validate both fields when the caller threads a handle — passing
    /// a handle minted by a different module or for a different
    /// resource is a typed error rather than a silent misroute.
    ///
    /// Advantages over the TypeScript path:
    /// - No IPC overhead per page (state is Rust-side)
    /// - Cursor-based pagination using last ID (faster than OFFSET for large datasets)
    /// - DashMap for concurrent query state (lock-free reads)
    pub(super) async fn handle_query_open(&self, params: QueryOpenParams) -> Result<CommandResult, String> {
        require_public_collection(&params.collection)?;
        require_query_fields(&params.filter, &params.sort, &None)?;
        use std::time::Instant;
        let start = Instant::now();

        let adapter = self.get_adapter(&params.db_path).await?;

        // QW#2: skip the upfront SELECT COUNT(*) by default — it's a full
        // table scan that grows linearly with table size and most callers
        // only need has_more, which is the LIMIT N+1 probe in
        // handle_query_next. Caller opts in via count_exact: true when the
        // UI actually needs an exact "X of N" display.
        let total_count = if params.count_exact {
            let count_query = StorageQuery {
                collection: params.collection.clone(),
                filter: params.filter.clone(),
                ..Default::default()
            };
            adapter.count(count_query).await.data.unwrap_or(0) as u64
        } else {
            0
        };

        // Mint a UUID once. The same value lives in TWO places: the
        // DashMap key (a string for back-compat with the existing
        // storage shape) and the HandleRef.id (a typed Uuid for the
        // envelope). Identity is the same; only the wire shape differs.
        let cursor_id = uuid::Uuid::new_v4();
        let cursor_id_str = cursor_id.to_string();

        // has_more starts optimistic — the LIMIT N+1 probe on the first
        // query_next call is the authoritative signal. If the table is
        // empty, the caller sees an empty first page with has_more: false.
        let has_more = if params.count_exact {
            total_count > 0
        } else {
            true
        };

        // Create query state (the string form is the DashMap key, not
        // stored in the struct).
        let state = PaginatedQueryState {
            db_path: params.db_path.clone(),
            collection: params.collection.clone(),
            filter: params.filter,
            sort: params.sort,
            page_size: params.page_size,
            total_count,
            current_page: 0,
            cursor_id: None,
            has_more,
            created_at: Instant::now(),
        };

        self.paginated_queries.insert(
            cursor_id_str.clone(),
            Arc::new(tokio::sync::Mutex::new(state)),
        );

        let total_ms = start.elapsed().as_millis();
        log_info!(
            "data",
            "query-open",
            "Opened query {} for {} (total={}, pageSize={}) in {}ms",
            cursor_id_str,
            params.collection,
            total_count,
            params.page_size,
            total_ms
        );

        // Typed envelope: nested `data` preserves the legacy
        // `response.data.queryId` wire shape; the kernel-level `handle`
        // is the new canonical reference for the cursor.
        let response = QueryOpenResponseShape {
            data: QueryOpenInner {
                query_id: cursor_id_str,
                collection: params.collection,
                total_count,
                page_size: params.page_size,
                has_more,
            },
        };

        CommandResponse::ok(response)
            .with_handle(DATA_MODULE_OWNER, cursor_id, QUERY_CURSOR_TYPE_TAG)
            .into_command_result()
    }

    // The dual-shape (envelope handle OR legacy `queryId` string)
    // resolver previously lived here as a 35-line inline helper.
    // That logic moved into the substrate at
    // [`CommandRequest::handle_id_or_legacy`] (with owner/type
    // validation via [`HandleRef::expect_owned_by`]) so every future
    // migration of a stringly-typed id to a typed handle reaches
    // for the same primitive. `handle_query_next` / `handle_query_close`
    // call it directly with this module's owner + type tag constants.

    /// Get next page from paginated query.
    ///
    /// Cursor id is resolved by [`Self::resolve_query_cursor_id`] from
    /// either the typed envelope's `handle` (new canonical) or the
    /// legacy `queryId` field (back-compat).
    ///
    /// Uses keyset pagination (WHERE id > cursor) instead of OFFSET for performance.
    /// For sorted queries, combines sort column(s) with id for deterministic ordering.
    pub(super) async fn handle_query_next(
        &self,
        req: CommandRequest<QueryNextParams>,
    ) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let cursor_id = req.handle_id_or_legacy(
            DATA_MODULE_OWNER,
            QUERY_CURSOR_TYPE_TAG,
            "queryId",
            &req.params.query_id,
            "data/query-next",
        )?;

        // ── Acquire the per-cursor mutex ─────────────────────────────
        //
        // Clone the Arc<Mutex> handle OUT of the DashMap shard's lock
        // (cheap, no contention beyond the brief shard read), then
        // lock the per-cursor mutex for the full read-then-async-
        // then-write sequence below. The mutex is the substrate's
        // promise that concurrent next-calls on the SAME cursor
        // serialize — without it, every caller would read the same
        // pre-mutation `current_page` snapshot and produce duplicate
        // page reads (caught by the
        // `same_cursor_concurrent_next_does_not_corrupt_state` test).
        //
        // Concurrent next-calls on DIFFERENT cursors stay fully
        // parallel because each cursor has its OWN mutex; only same-
        // cursor activity is serialized, which is the minimum
        // required for cursor-state correctness.
        let state_lock = self
            .paginated_queries
            .get(&cursor_id)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| {
                format!(
                    "data/query-next: handle not found — cursor {} is unknown to this module. \
                     The handle may have been minted by a previous process instance, may have been \
                     closed via data/query-close, or may have been evicted by a future TTL policy.",
                    cursor_id
                )
            })?;
        let mut state = state_lock.lock().await;

        // Snapshot the read-only fields the adapter query needs into
        // locals. We keep the lock held across the .await so the
        // write at the bottom sees a consistent snapshot.
        let db_path = state.db_path.clone();
        let collection = state.collection.clone();
        let filter = state.filter.clone();
        let sort = state.sort.clone();
        let page_size = state.page_size;
        let total_count = state.total_count;
        let current_page = state.current_page;
        let has_more = state.has_more;

        if !has_more {
            return Ok(CommandResult::Json(json!({
                "success": true,
                "data": {
                    "items": [],
                    "pageNumber": current_page,
                    "hasMore": false,
                    "totalCount": total_count
                }
            })));
        }

        let adapter = self.get_adapter(&db_path).await?;

        // QW#2: fetch page_size + 1 rows. The extra row is the has_more
        // probe — if we got it back, there's at least one more page; we
        // drop it before returning the page. This replaces the prior
        // formula `offset + items_count < total_count`, which was both
        // wrong under concurrent inserts (total_count goes stale mid-iter)
        // and required the open-time COUNT(*) we just stopped paying for.
        let offset = current_page * page_size;
        let query = StorageQuery {
            collection: collection.clone(),
            filter: filter.clone(),
            sort: sort.clone(),
            limit: Some(page_size + 1),
            offset: Some(offset),
            ..Default::default()
        };

        let result = adapter.query(query).await;
        if !result.success {
            return Err(result.error.unwrap_or_else(|| "Query failed".to_string()));
        }

        let mut records = result.data.unwrap_or_default();
        let new_has_more = records.len() > page_size;
        if new_has_more {
            records.truncate(page_size);
        }
        let items_count = records.len();

        // Get last ID for cursor
        let new_cursor_id = records.last().map(|r| r.id.clone());

        // Update query state — `state` is still the locked
        // `MutexGuard` from the top of the function, so this write is
        // atomic with the read above. No second DashMap lookup needed;
        // the per-cursor mutex held the whole window.
        state.current_page += 1;
        state.cursor_id = new_cursor_id;
        state.has_more = new_has_more;
        drop(state);

        // Convert records to JSON
        let items: Vec<Value> = records
            .into_iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "data": r.data,
                    "metadata": {
                        "createdAt": r.metadata.created_at,
                        "updatedAt": r.metadata.updated_at,
                        "version": r.metadata.version
                    }
                })
            })
            .collect();

        let total_ms = start.elapsed().as_millis();
        log_info!(
            "data",
            "query-next",
            "Page {} for query {} ({} items, hasMore={}) in {}ms",
            current_page + 1,
            cursor_id,
            items_count,
            new_has_more,
            total_ms
        );

        // Wrap in StorageResult-style response for TypeScript compatibility
        Ok(CommandResult::Json(json!({
            "success": true,
            "data": {
                "items": items,
                "pageNumber": current_page + 1,
                "hasMore": new_has_more,
                "totalCount": total_count
            }
        })))
    }

    /// Close paginated query and free resources. Cursor id is resolved
    /// by [`Self::resolve_query_cursor_id`] from either the typed
    /// envelope's `handle` (new canonical) or the legacy `queryId`
    /// field (back-compat).
    pub(super) async fn handle_query_close(
        &self,
        req: CommandRequest<QueryCloseParams>,
    ) -> Result<CommandResult, String> {
        let cursor_id = req.handle_id_or_legacy(
            DATA_MODULE_OWNER,
            QUERY_CURSOR_TYPE_TAG,
            "queryId",
            &req.params.query_id,
            "data/query-close",
        )?;
        let removed = self.paginated_queries.remove(&cursor_id).is_some();

        log_info!(
            "data",
            "query-close",
            "Closed query {}: removed={}",
            cursor_id,
            removed
        );

        Ok(CommandResult::Json(json!({
            "success": removed,
            "queryId": cursor_id
        })))
    }

    // =========================================================================
    // Migration Handlers
    // =========================================================================

    /// Start a streaming migration between two adapters (async — returns immediately)
    /// Start a streaming migration between two adapter connection strings as a
    /// background task; returns the initial progress snapshot immediately. Single
    /// source for the typed `migration/start` command.
    pub(crate) async fn migration_start(
        &self,
        source: String,
        target: String,
        batch_size: usize,
        throttle_ms: u64,
        collections: Option<Vec<String>>,
    ) -> Result<MigrationProgress, String> {
        // Get or create adapters for source and target
        let source_adapter = self.get_adapter(&source).await?;
        let target_adapter = self.get_adapter(&target).await?;

        let config = MigrationConfig {
            batch_size,
            throttle_ms,
            collections,
        };

        let mut engine = MigrationEngine::new(source_adapter, target_adapter, config);
        let handle = engine.handle();

        // Store handle for status/pause/resume/verify (before spawning)
        *self.active_migration.lock().await = Some(handle.clone());

        // Spawn migration as background task — returns immediately
        tokio::spawn(async move {
            log_info!(
                "data",
                "migration/start",
                "Background migration started: {} -> {}",
                source,
                target
            );
            match engine.run().await {
                Ok(status) => {
                    log_info!("data", "migration/start", "Migration completed: {}", status);
                }
                Err(e) => {
                    log_error!("data", "migration/start", "Migration failed: {}", e);
                }
            }
        });

        Ok(handle.status())
    }

    /// Get migration progress (lock-free — reads atomic counters). Single source
    /// for the typed `migration/status` command.
    pub(crate) async fn migration_status(&self) -> Result<MigrationProgress, String> {
        let guard = self.active_migration.lock().await;
        match guard.as_ref() {
            Some(handle) => Ok(handle.status()),
            None => Err("No active migration".into()),
        }
    }

    /// Pause the active migration (atomic flag). Single source for `migration/pause`.
    pub(crate) async fn migration_pause(&self) -> Result<MigrationProgress, String> {
        let guard = self.active_migration.lock().await;
        match guard.as_ref() {
            Some(handle) => {
                handle.pause();
                log_info!("data", "migration/pause", "Migration paused");
                Ok(handle.status())
            }
            None => Err("No active migration".into()),
        }
    }

    /// Resume a paused migration (clears the pause flag). Single source for
    /// `migration/resume`.
    pub(crate) async fn migration_resume(&self) -> Result<MigrationProgress, String> {
        let guard = self.active_migration.lock().await;
        match guard.as_ref() {
            Some(handle) => {
                handle.resume();
                log_info!("data", "migration/resume", "Migration resumed");
                Ok(handle.status())
            }
            None => Err("No active migration".into()),
        }
    }

    /// Verify migration integrity (compare counts between source and target).
    /// Single source for the typed `migration/verify` command.
    pub(crate) async fn migration_verify(&self) -> Result<MigrationVerification, String> {
        let guard = self.active_migration.lock().await;
        match guard.as_ref() {
            Some(handle) => handle.verify().await,
            None => Err("No active migration".into()),
        }
    }

    /// Cutover: swap the active adapter for a new connection string, redirecting
    /// all subsequent operations to the new backend. Single source for
    /// `migration/cutover`.
    pub(crate) async fn migration_cutover(
        &self,
        current: String,
        target: String,
    ) -> Result<MigrationCutover, String> {
        // Store current for rollback
        *self.previous_connection.lock().await = Some(current.clone());

        // Remove old adapter from cache (forces re-creation on next access)
        self.adapters.remove(&current);

        // Pre-warm the target adapter
        let target_adapter = self.get_adapter(&target).await?;

        log_info!(
            "data",
            "migration/cutover",
            "Cutover: {} -> {}",
            current,
            target
        );

        Ok(MigrationCutover {
            previous: current,
            active: target,
            adapter: target_adapter.name().to_string(),
        })
    }

    /// Rollback: revert to the previously-active connection string recorded by the
    /// last cutover. Single source for the typed `migration/rollback` command.
    pub(crate) async fn migration_rollback(
        &self,
        current: String,
    ) -> Result<MigrationRollback, String> {
        // Snapshot the previous connection and release the lock before awaiting.
        let prev = {
            let guard = self.previous_connection.lock().await;
            guard.as_ref().cloned()
        };
        match prev {
            Some(prev) => {
                // Remove current adapter
                self.adapters.remove(&current);

                // Pre-warm previous adapter
                let adapter = self.get_adapter(&prev).await?;

                log_info!(
                    "data",
                    "migration/rollback",
                    "Rolled back: {} -> {}",
                    current,
                    prev
                );

                Ok(MigrationRollback {
                    rolled_back_from: current,
                    rolled_back_to: prev,
                    adapter: adapter.name().to_string(),
                })
            }
            None => Err("No previous connection to rollback to".into()),
        }
    }

}

// ── SDK contract: data/list (sdk_codegen) ──────────────────────────
//
// The Rust-rooted contract for `data/list` — the persona/UI-facing collection
// read. Per the single-source principle the TYPE leads and the handler conforms:
// `data/list` is, by contract, "collection (+ optional filter/ordering/paging)
// → items + accurate total". The typed command in `commands/data/list.rs` drives
// [`DataState::list`], which honors exactly this shape. There is deliberately NO
// `db_path` here: a persona reading rooms or messages must never reason about a
// database handle — the shared "main" store is the default, and power callers
// target a specific store via the optional `handle`.
/// Sort direction for a `data/list` ordering clause.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../protocol/typescript/data/SortDir.ts")]
pub enum SortDir {
    Asc,
    Desc,
}

/// One ordering clause: a field + a direction.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/OrderByClause.ts"
)]
pub struct OrderByClause {
    pub field: String,
    pub direction: SortDir,
}
