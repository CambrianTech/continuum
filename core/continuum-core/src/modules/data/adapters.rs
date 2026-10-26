//! Backend resolution binds adapter pools and vector cache identity.
use super::*;

impl DataState {
    /// Get or create adapter for the given caller handle.
    ///
    /// Two-step resolution:
    ///   1. `resolve_handle(handle)` → opaque backend connection string
    ///   2. Route connection string to concrete adapter (SQLite only). Adapters are cached keyed by connection string so two
    ///      handles resolving to the same backend share one pool.
    pub(crate) async fn get_adapter(
        &self,
        handle: &str,
    ) -> Result<Arc<dyn StorageAdapter>, String> {
        self.adapter_with_identity(handle).await.map(|(_, adapter)| adapter)
    }

    /// Resolve once before I/O: the returned identity owns both pool and corpus.
    pub(super) async fn adapter_with_identity(
        &self,
        handle: &str,
    ) -> Result<(String, Arc<dyn StorageAdapter>), String> {
        let connection_string = self.resolve_handle(handle)?;
        let adapter = self.adapter_for_backend(handle, connection_string.clone()).await?;
        Ok((connection_string, adapter))
    }

    pub(super) async fn adapter_for_backend(
        &self,
        handle: &str,
        connection_string: String,
    ) -> Result<Arc<dyn StorageAdapter>, String> {

        reject_unsupported_backend(&connection_string)?;

        // Check cache (keyed by resolved connection string, not by handle —
        // different handles can point to the same backend).
        if let Some(adapter) = self.adapters.get(&connection_string) {
            return Ok(adapter.clone());
        }

        let _guard = self.init_lock.lock().await;

        if let Some(adapter) = self.adapters.get(&connection_string) {
            return Ok(adapter.clone());
        }

        // Scale pool size based on role. Main DB (full pool) vs per-persona
        // (small pool). Detection is post-resolution, on the connection
        // string, since that's where the backend shape is visible.
        let is_main_db = connection_string.contains("database/main.db")
            || connection_string.contains("database\\main.db");
        let max_connections = if is_main_db { 20 } else { 4 };

        let config = AdapterConfig {
            connection_string: connection_string.clone(),
            namespace: None,
            timeout_ms: 30_000,
            max_connections,
        };

        let adapter: Arc<dyn StorageAdapter> = {
            log_info!(
                "data",
                "get_adapter",
                "Creating SqliteAdapter for handle='{}' → {}",
                handle,
                connection_string
            );
            let mut sqlite = SqliteAdapter::new();
            sqlite.initialize(config).await?;
            ensure_declared_indexes(&sqlite, handle).await;
            Arc::new(sqlite)
        };

        self.adapters.insert(connection_string, adapter.clone());
        Ok(adapter)
    }
}

/// Apply every TS-declared entity's indexes over the tables this store already has.
/// Those collections are written row by row (`data/create` creates the table from the
/// row), and since the core went headless nothing calls `data/ensure-schema` for them,
/// so their declared indexes never reached the database: `chat_messages` scanned
/// 103,319 rows per room page (27-29 ms; with the declared index, under 1 ms, measured
/// on a copy of the M5's main.db 2026-10-26). Rust-authored entities are ensured by
/// their own stores. Once per store open; `IF NOT EXISTS` makes a repeat a lookup.
///
/// An index is performance, never correctness: one the database refuses is a loud
/// probe naming the entity, the index and the error, counted in the store's report,
/// and the store opens without it (Cormac's review of #5466).
async fn ensure_declared_indexes(sqlite: &SqliteAdapter, handle: &str) {
    let schemas = crate::entity_schemas::get_entity_schemas()
        .values()
        .map(crate::entity_schemas::to_collection_schema)
        .collect();
    let pass = sqlite.ensure_declared_indexes(schemas).await;
    for failure in &pass.failed {
        continuum_base::probe!(
            class = "orm.declared_index.failed",
            handle = handle,
            entity = failure.collection.as_str(),
            index = failure.index.as_str(),
            error = failure.error.as_str(),
            "a declared index could not be built; the store serves without it"
        );
        crate::log_warn!(
            "data",
            "declared_index",
            "index {} on {} (handle '{}') failed, store open without it: {}",
            failure.index,
            failure.collection,
            handle,
            failure.error
        );
    }
    continuum_base::probe!(
        class = "orm.declared_indexes.report",
        handle = handle,
        declared = pass.declared as u64,
        applied = pass.ensured as u64,
        awaiting_columns = pass.awaiting_columns as u64,
        failed = pass.failed.len() as u64,
        "declared entity indexes at store open"
    );
}

/// SQLite through the kernel ORM is the only store. A `postgres://` string
/// (e.g. a stale `DATABASE_URL`) fails loudly here, never falls back to a local file.
pub(super) fn reject_unsupported_backend(connection_string: &str) -> Result<(), String> {
    if connection_string.starts_with("postgres://") || connection_string.starts_with("postgresql://") {
        return Err(format!(
            "Postgres is no longer supported: '{connection_string}'. Continuum stores data in SQLite only; unset DATABASE_URL or point it at a SQLite file path."
        ));
    }
    Ok(())
}

/// Other modules reach named adapters through this trait, never by naming `DataModule` (card
/// 39f08cda): the same resolution and cache as [`DataState::get_adapter`].
#[async_trait::async_trait]
impl crate::runtime::StorageProvider for DataState {
    async fn adapter(&self, handle: &str) -> Result<Arc<dyn StorageAdapter>, String> {
        self.get_adapter(handle).await
    }
}
