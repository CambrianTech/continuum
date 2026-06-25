//! DataModule — Storage and ORM operations via the StorageAdapter trait.
//!
//! Handles: data/* commands (create, read, update, delete, query, batch)
//! Also handles: vector/* commands (vector similarity search with in-memory caching)
//! Uses the ORM module's StorageAdapter trait for database-agnostic operations.
//!
//! CRITICAL: Database paths are ALWAYS passed by the caller (TypeScript handle layer).
//! NO defaults, NO environment variables, NO fallbacks. The caller owns the paths.

use crate::orm::{
    adapter::{AdapterConfig, StorageAdapter},
    migration::{MigrationConfig, MigrationEngine, MigrationHandle},
    postgres::PostgresAdapter,
    query::{FieldFilter, SortDirection, SortSpec, StorageQuery},
    sqlite::SqliteAdapter,
    types::{BatchOperation, DataRecord, RecordMetadata, StorageResult, UUID},
};
use crate::runtime::{
    CommandRequest, CommandResponse, CommandResult, HandleRef, ModuleConfig, ModuleContext,
    ModulePriority, ServiceModule,
};
use crate::{log_error, log_info};
use async_trait::async_trait;
use chrono;
use dashmap::DashMap;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::{Mutex, Semaphore};

/// Max concurrent query operations. Limits peak heap from personas × N queries
/// all materializing result sets simultaneously. Without this, RSS spikes
/// from concurrent JSON serialization of large result sets.
///
/// Set to 16 to match SQLite reader pool capacity (main DB has 20 readers).
/// Previous value of 4 caused head-of-line blocking: 20 readers sat idle
/// while the semaphore refused to let queries through, causing cascading
/// timeouts under normal 15-persona load.
const MAX_CONCURRENT_QUERIES: usize = 16;

// ============================================================================
// Vector Search Types and Cache
// ============================================================================

/// Cached vector for in-memory similarity search
struct CachedVector {
    id: String,
    embedding: Vec<f64>,
}

/// Collection vector cache with Arc for zero-copy sharing during concurrent searches
struct VectorCache {
    vectors: Arc<Vec<CachedVector>>,
}

/// Cache key: (db_path, collection)
type VectorCacheKey = (String, String);

// ============================================================================
// Paginated Query State
// ============================================================================

/// Paginated query state - server-side cursor management
/// Advantage over TypeScript: no IPC per page, just in-memory state
#[derive(Debug)]
struct PaginatedQueryState {
    // NOTE: query_id is NOT stored here - it's the DashMap key
    db_path: String,
    collection: String,
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
    sort: Option<Vec<crate::orm::query::SortSpec>>,
    page_size: usize,
    /// Exact row count from the open-time COUNT(*). Only populated when the
    /// caller passed `count_exact: true`; otherwise 0 means "not requested",
    /// which is the default behavior (QW#2 — skip the full-table scan when
    /// the caller only needs has_more, which is derived from LIMIT N+1).
    total_count: u64,
    current_page: usize,
    /// Cursor: last ID from previous page for efficient keyset pagination
    cursor_id: Option<String>,
    has_more: bool,
    /// Creation time for future TTL-based cleanup of stale queries
    #[allow(dead_code)]
    created_at: std::time::Instant,
}

/// DataState holds the storage substrate shared between the [`DataModule`]
/// (its `ServiceModule` shell) and the typed `data/*` commands in
/// `commands/data/*`. Both capture the same `Arc<DataState>` — the
/// CodeState/CodeModule convention — so a migrated command drives the exact
/// state the module owns. Database path comes from each request.
///
/// Adapter-agnostic: connection string determines which adapter is used.
/// - File paths or `:memory:` → SqliteAdapter (worker thread with mpsc)
/// - `postgres://` or `postgresql://` → PostgresAdapter (async connection pool)
///
/// NOTE: SqliteAdapter is internally thread-safe via mpsc channels.
/// PostgresAdapter is internally thread-safe via deadpool connection pool.
pub struct DataState {
    /// Adapter cache: connection_string -> initialized adapter (polymorphic)
    /// Lazy initialization per unique connection string
    adapters: DashMap<String, Arc<dyn StorageAdapter>>,
    /// Mutex only used during adapter initialization (one-time setup)
    init_lock: Mutex<()>,
    /// Vector cache: (db_path, collection) -> vectors
    /// Uses RwLock for concurrent reads (no mutex contention during searches)
    vector_cache: RwLock<HashMap<VectorCacheKey, VectorCache>>,
    /// Paginated query state: queryId -> per-cursor mutex.
    ///
    /// Server-side cursor management for efficient pagination. The
    /// per-cursor `tokio::sync::Mutex` serializes concurrent
    /// `query-next` / `query-close` calls on the SAME cursor — the
    /// read-then-async-then-write pattern in `handle_query_next` would
    /// otherwise race when N personas (or a retrying single persona)
    /// call next on the same handle concurrently, causing every
    /// caller to read the same page snapshot and produce duplicate
    /// page-1 reads.
    ///
    /// Per Joel 2026-05-30: "Each persona exists in its own threads."
    /// Independent cursors stay parallel (DashMap's per-shard locking
    /// preserves the lock-free read path for different cursor ids);
    /// only same-cursor concurrent activity is serialized, which is
    /// the minimum required for cursor-state correctness.
    paginated_queries: DashMap<String, Arc<tokio::sync::Mutex<PaginatedQueryState>>>,
    /// Module context for inter-module communication (event bus, shared compute)
    /// Set during initialize(), used to publish data change events
    context: RwLock<Option<Arc<ModuleContext>>>,
    /// Active migration handle for status/pause/verify (lightweight, lock-free)
    active_migration: Mutex<Option<MigrationHandle>>,
    /// Pre-cutover connection string (for rollback)
    previous_connection: Mutex<Option<String>>,
    /// Limits concurrent query/list operations to cap peak heap usage.
    /// 15 personas firing queries simultaneously can spike RSS by several GB
    /// from concurrent result set materialization + JSON serialization.
    query_semaphore: Semaphore,
}

/// `ServiceModule` shell over the shared [`DataState`]. The kernel registers
/// this; the typed `data/*` commands capture `self.state.clone()` via
/// [`DataModule::commands`]. All storage logic lives on `DataState`.
pub struct DataModule {
    pub(crate) state: Arc<DataState>,
}

impl DataModule {
    pub fn new() -> Self {
        Self {
            state: Arc::new(DataState::new()),
        }
    }
}

impl DataState {
    pub fn new() -> Self {
        Self {
            adapters: DashMap::new(),
            init_lock: Mutex::new(()),
            vector_cache: RwLock::new(HashMap::new()),
            paginated_queries: DashMap::new(),
            context: RwLock::new(None),
            active_migration: Mutex::new(None),
            previous_connection: Mutex::new(None),
            query_semaphore: Semaphore::new(MAX_CONCURRENT_QUERIES),
        }
    }

    /// Publish a data change event to the message bus.
    /// Events follow pattern: data:{collection}:{action}
    /// Actions: created, updated, deleted, batch
    fn publish_event(&self, collection: &str, action: &str, payload: serde_json::Value) {
        let ctx_guard = self.context.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ctx) = ctx_guard.as_ref() {
            let event_name = format!("data:{}:{}", collection, action);
            ctx.bus.publish_async_only(&event_name, payload);
        }
    }

    /// Log a slow query to the module's dedicated log file.
    /// Only logs if duration exceeds threshold (50ms).
    fn log_slow_query(&self, operation: &str, collection: &str, duration_ms: u128) {
        if duration_ms < 50 {
            return;
        }
        let ctx_guard = self.context.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ctx) = ctx_guard.as_ref() {
            let logger = ctx.logger("data");
            logger.timing_with_meta(
                operation,
                duration_ms as u64,
                &format!("collection={}", collection),
            );
        }
    }

    /// Resolve a caller-supplied HANDLE to the backend connection string.
    ///
    /// Callers pass opaque handles across the IPC boundary — they never
    /// construct URLs, filenames, or any other backend-specific identifier.
    /// This function is the ONLY place in the codebase that maps
    /// handle → connection string; downstream adapter selection in
    /// `get_adapter` then routes by the resolved string's shape.
    ///
    /// Resolution rules:
    /// - `"main"` — primary database. Uses `DATABASE_URL` env when set
    ///   (grid opt-in for Postgres / any future adapter); otherwise
    ///   defaults to a local SQLite file at
    ///   `$HOME/.continuum/database/main.db`.
    /// - `"@persona:<slug>"` — per-persona long-term memory DB. Resolves
    ///   to `$HOME/.continuum/personas/<slug>/data/longterm.db` on the
    ///   host. Mac Option B requires this — TS in container builds
    ///   container-rooted paths (`/root/...`) that the native core on
    ///   host can't open; the sentinel lets each side resolve to its
    ///   own filesystem view of the shared `~/.continuum` mount.
    /// - `"@metrics"` — telemetry SQLite. Resolves to
    ///   `$HOME/.continuum/metrics/metrics.sqlite` on the host. Same
    ///   Mac Option B rationale.
    /// - 36-char UUID shape — per-persona database (UUID-keyed variant).
    ///   Maps to `$HOME/.continuum/personas/<uuid>/longterm.db`. Kept
    ///   for back-compat with callers using UUIDs as identity.
    /// - Starts with `postgres://` / `postgresql://` / filesystem path —
    ///   legacy passthrough. Logged at WARN so remaining leak sites show
    ///   up in the next audit. Will be removed once every caller migrates.
    ///
    /// This keeps the abstraction enforced at the caller boundary: SQL,
    /// URLs, and filenames simply do not exist in the caller's language.
    fn resolve_handle(&self, handle: &str) -> Result<String, String> {
        // Main DB sentinel — honors DATABASE_URL env, falls back to SQLite.
        if handle == "main" {
            if let Ok(url) = std::env::var("DATABASE_URL") {
                if !url.is_empty() {
                    return Ok(url);
                }
            }
            let home = std::env::var("HOME")
                .map_err(|_| "resolve_handle('main'): HOME env not set".to_string())?;
            return Ok(format!("{}/.continuum/database/main.db", home));
        }

        // Per-persona slug-shape sentinel: @persona:<slug>
        // Slug matches the on-disk dir under $HOME/.continuum/personas/.
        // Mac Option B fix — TS in container would otherwise ship a
        // /root-rooted path the host-side native core can't open even
        // though the file is the same on both sides of the mount.
        if let Some(slug) = handle.strip_prefix("@persona:") {
            if slug.is_empty() {
                return Err("resolve_handle('@persona:'): empty slug".to_string());
            }
            // Defensive: slug must be a single path segment — no escapes.
            if slug.contains('/') || slug.contains('\\') || slug.contains("..") {
                return Err(format!(
                    "resolve_handle('@persona:{}'): slug must be a single path segment",
                    slug
                ));
            }
            let home = std::env::var("HOME")
                .map_err(|_| format!("resolve_handle('@persona:{}'): HOME env not set", slug))?;
            return Ok(format!(
                "{}/.continuum/personas/{}/data/longterm.db",
                home, slug
            ));
        }

        // Telemetry SQLite sentinel.
        if handle == "@metrics" {
            let home = std::env::var("HOME")
                .map_err(|_| "resolve_handle('@metrics'): HOME env not set".to_string())?;
            return Ok(format!("{}/.continuum/metrics/metrics.sqlite", home));
        }

        // Per-persona UUID shape: 8-4-4-4-12 hex chars with hyphens (36 total).
        // Safe to check without crate parsing — the shape is unambiguous.
        if is_uuid_shape(handle) {
            let home = std::env::var("HOME")
                .map_err(|_| format!("resolve_handle('{}'): HOME env not set", handle))?;
            return Ok(format!(
                "{}/.continuum/personas/{}/longterm.db",
                home, handle
            ));
        }

        // Legacy passthrough — log so we can hunt remaining callers.
        if handle.starts_with("postgres://")
            || handle.starts_with("postgresql://")
            || handle.starts_with('/')
            || handle.starts_with('.')
            || handle.contains(".db")
        {
            log_info!(
                "data",
                "resolve_handle",
                "LEGACY connection string at IPC boundary: {} — caller should pass a handle ('main', '@persona:<slug>', '@metrics', or persona UUID)",
                handle
            );
            return Ok(handle.to_string());
        }

        Err(format!(
            "Unknown database handle: '{}'. Valid handles are 'main', '@persona:<slug>', \
             '@metrics', or a persona UUID. Custom backends must be opened via data/open (future).",
            handle
        ))
    }

    /// Get or create adapter for the given caller handle.
    ///
    /// Two-step resolution:
    ///   1. `resolve_handle(handle)` → opaque backend connection string
    ///   2. Route connection string to concrete adapter (Postgres / SQLite /
    ///      future). Adapters are cached keyed by connection string so two
    ///      handles resolving to the same backend share one pool.
    async fn get_adapter(&self, handle: &str) -> Result<Arc<dyn StorageAdapter>, String> {
        let connection_string = self.resolve_handle(handle)?;

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
            || connection_string.contains("database\\main.db")
            || connection_string.starts_with("postgres://")
            || connection_string.starts_with("postgresql://");
        let max_connections = if is_main_db { 20 } else { 4 };

        let config = AdapterConfig {
            connection_string: connection_string.clone(),
            namespace: None,
            timeout_ms: 30_000,
            max_connections,
        };

        let adapter: Arc<dyn StorageAdapter> = if connection_string.starts_with("postgres://")
            || connection_string.starts_with("postgresql://")
        {
            log_info!(
                "data",
                "get_adapter",
                "Creating PostgresAdapter for handle='{}' (resolved)",
                handle
            );
            let mut pg = PostgresAdapter::new();
            pg.initialize(config).await?;
            Arc::new(pg)
        } else {
            log_info!(
                "data",
                "get_adapter",
                "Creating SqliteAdapter for handle='{}' → {}",
                handle,
                connection_string
            );
            let mut sqlite = SqliteAdapter::new();
            sqlite.initialize(config).await?;
            Arc::new(sqlite)
        };

        self.adapters.insert(connection_string, adapter.clone());
        Ok(adapter)
    }
}

/// Phase 1 typed-IPC: deserialize raw IPC params into the typed struct
/// the handler expects, uniformly logging the parse error with
/// command-name context. Used in `handle_command` dispatch arms;
/// CONSUMES `params` (no `.clone()`) — that was one of the per-call hot-path
/// wins flagged in `docs/architecture/ORM-IDEALISM-PLAN.md`.
///
/// Usage:
///     "data/read" => self.handle_read(deserialize_params!(command, params)?).await,
///
/// `command` is the outer variable being matched on. Returns
/// `Result<TypedParams, String>`; the `?` propagates the formatted error
/// string back to the IPC dispatch layer.
macro_rules! deserialize_params {
    ($command:expr, $params:expr) => {
        serde_json::from_value($params).map_err(|e| {
            log_error!("data", "handle_command", "{} parse error: {}", $command, e);
            format!("Invalid params for {}: {}", $command, e)
        })
    };
}

/// Check if a string matches the 36-char UUID shape `8-4-4-4-12` hex.
/// Intentionally simple — avoids pulling uuid crate just for a shape check.
fn is_uuid_shape(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    s.chars().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => c == '-',
        _ => c.is_ascii_hexdigit(),
    })
}

impl Default for DataModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for DataModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "data",
            priority: ModulePriority::Normal,
            command_prefixes: &["data/", "adapter/", "vector/", "migration/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Store context for event publishing
        let ctx_arc = Arc::new(ModuleContext::new(
            ctx.registry.clone(),
            ctx.bus.clone(),
            ctx.compute.clone(),
            ctx.runtime.clone(),
        ));
        *self.state.context.write().unwrap_or_else(|e| e.into_inner()) = Some(ctx_arc);
        log_info!("data", "init", "DataModule initialized with event bus");
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        // Legacy Registry-A entry point — delegates to the shared state's
        // in-process dispatch. As `data/*` arms migrate to typed commands in
        // `commands/data/*`, the executor's `route_object` path wins first and
        // this string match shrinks toward deletion (Wave Z).
        self.state.dispatch(command, params).await
    }

    async fn shutdown(&self) -> Result<(), String> {
        // Close all adapters - clear the DashMap
        // Adapters will clean up when their Arc refcount drops to zero
        self.state.adapters.clear();
        Ok(())
    }

    /// The migrated `data/*` commands as typed self-routing objects on the ONE
    /// registry. Each shares this module's `Arc<DataState>`; the executor routes
    /// their names straight here (winning over the legacy `data/` prefix arm),
    /// and their `CommandSpec` descriptors flow into `command_registry()` → the
    /// persona tool surface + grid ACL. See [`crate::commands::data`].
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::data::command_objects(self.state.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Command param structs for the remaining legacy arms (still dbPath-shaped).
// The migrated CRUD commands (data/create|read|update|delete) carry their own
// clean `handle`-based params in `commands/data/*.rs`.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryParams {
    db_path: String,
    collection: String,
    #[serde(default)]
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    select: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryWithJoinParams {
    db_path: String,
    collection: String,
    #[serde(default)]
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    joins: Option<Vec<crate::orm::query::JoinSpec>>,
    #[serde(default)]
    select: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CountParams {
    db_path: String,
    collection: String,
    #[serde(default)]
    filter: Option<serde_json::Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchParams {
    db_path: String,
    operations: Vec<BatchOperation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnsureSchemaParams {
    db_path: String,
    /// Phase 2: callers pass a collection NAME, not an inline CollectionSchema.
    /// Rust resolves the schema from entity_schemas.json (decorator-sourced
    /// at build time via generate-entity-schemas.ts). The language level
    /// never constructs SQL / fields / indexes on the wire.
    collection: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollectionParams {
    db_path: String,
    collection: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbPathOnly {
    db_path: String,
}

/// Vector search params (matches data-daemon API)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorSearchParams {
    db_path: String,
    collection: String,
    query_vector: Vec<f64>,
    #[serde(default = "default_k")]
    k: usize,
    #[serde(default)]
    threshold: f64,
    #[serde(default = "default_true")]
    include_data: bool,
}

fn default_k() -> usize {
    10
}
fn default_true() -> bool {
    true
}
fn default_batch_size() -> usize {
    100
}

/// Index vector params - store embedding for a record
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexVectorParams {
    db_path: String,
    collection: String,
    id: String,
    embedding: Vec<f64>,
}

/// Backfill vectors params - generate embeddings for existing records
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackfillVectorsParams {
    db_path: String,
    collection: String,
    text_field: String,
    #[serde(default = "default_batch_size")]
    batch_size: usize,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
}

/// Vector stats params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorStatsParams {
    db_path: String,
    collection: String,
}

// ============================================================================
// Paginated Query Params
// ============================================================================

fn default_page_size() -> usize {
    100
}

/// Open paginated query params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryOpenParams {
    db_path: String,
    collection: String,
    #[serde(default)]
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default = "default_page_size")]
    page_size: usize,
    /// Opt in to a SELECT COUNT(*) at open time so callers that show
    /// "X of N" displays get an exact total. Default false: skip the scan
    /// (a ~1M-row chat_messages table no longer pays a full-table cost on
    /// every scrollback open). When false the response carries
    /// `totalCount: 0` as the "not requested" sentinel — `hasMore` is
    /// always authoritative regardless and is derived from the LIMIT N+1
    /// probe in `handle_query_next`.
    #[serde(default)]
    count_exact: bool,
}

/// Get next page params.
///
/// The cursor id reaches this handler one of two ways:
/// - Legacy flat `queryId` string field on the params body (what TS
///   consumers send today and will keep sending through the migration
///   window).
/// - Kernel-level `handle: HandleRef` on the [`CommandRequest`]
///   envelope (the canonical post-PR #1486 shape — minted by
///   `data/query-open` via `CommandResponse::with_handle`).
///
/// `resolve_query_cursor_id` walks the envelope first, falls back to
/// the legacy field, and fails loud when neither is present so a
/// caller who simply forgot the cursor sees a typed error instead of
/// silently no-op'ing.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct QueryNextParams {
    #[serde(default)]
    query_id: Option<String>,
}

/// Close query params. Same dual-shape contract as
/// [`QueryNextParams`] — see its docs for the legacy/envelope handoff.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct QueryCloseParams {
    #[serde(default)]
    query_id: Option<String>,
}

/// The canonical type tag for cursor handles minted by `data/query-open`.
/// Lives here so cross-module callers can match on it without depending
/// on string magic.
const QUERY_CURSOR_TYPE_TAG: &str = "data::QueryCursor";

/// The canonical owner string for handles this module mints. Matches
/// the module's `name` in `ModuleConfig`. Centralized so a future rename
/// of the module name is a single edit.
const DATA_MODULE_OWNER: &str = "data";

/// Response payload shape for `data/query-open`. Lives in a typed struct
/// so the typed envelope can flatten it cleanly — the legacy wire shape
/// nests every field under a `data:` key, so we preserve that here.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct QueryOpenResponseShape {
    /// Nested for back-compat with the pre-envelope wire shape that
    /// TS consumers currently parse as `response.data.queryId`. New
    /// consumers should read the kernel-level `handle` instead.
    data: QueryOpenInner,
}

/// Inner payload — the historical fields the cursor returns at open
/// time. `query_id` stays for back-compat (it's the same UUID stringly
/// rendered as the `handle.id`); new consumers thread the handle.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct QueryOpenInner {
    query_id: String,
    collection: String,
    total_count: u64,
    page_size: usize,
    has_more: bool,
}

// ============================================================================
// Migration Params
// ============================================================================

/// Start migration params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MigrationStartParams {
    source: String,
    target: String,
    #[serde(default = "default_migration_batch")]
    batch_size: usize,
    #[serde(default = "default_migration_throttle")]
    throttle_ms: u64,
    #[serde(default)]
    collections: Option<Vec<String>>,
}

fn default_migration_batch() -> usize {
    500
}
fn default_migration_throttle() -> u64 {
    10
}

/// Cutover params - switch active connection
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MigrationCutoverParams {
    /// The db_path currently in use to swap out
    current: String,
    /// The new connection string to swap in
    target: String,
}

/// Rollback params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MigrationRollbackParams {
    /// The db_path that was swapped in (to remove)
    current: String,
}

impl DataState {
    /// In-process command dispatch — the legacy `match` over `data/*`,
    /// `adapter/*`, `vector/*`, `migration/*`. The [`DataModule`] shell's
    /// `handle_command` delegates here so dispatch logic lives with the state it
    /// touches. Each typed `data/*` command in `commands/data/*` calls the
    /// corresponding `handle_*` directly, bypassing this string match — this
    /// stays only until every arm is migrated and Registry A is retired.
    async fn dispatch(&self, command: &str, params: Value) -> Result<CommandResult, String> {
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
            "data/count" => {
                self.handle_count(deserialize_params!(command, params)?)
                    .await
            }
            "data/batch" => {
                self.handle_batch(deserialize_params!(command, params)?)
                    .await
            }
            "data/ensure-schema" => {
                self.handle_ensure_schema(deserialize_params!(command, params)?)
                    .await
            }
            "data/list-collections" => self.handle_list_collections(params).await,
            "data/collection-stats" => self.handle_collection_stats(params).await,
            "data/truncate" => self.handle_truncate(params).await,
            "data/clear-all" => self.handle_clear_all(params).await,

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

            "adapter/capabilities" => self.handle_capabilities(params).await,
            "adapter/info" => self.handle_info(params).await,

            // Vector search (migrated from data-daemon-worker)
            "vector/search" => self.handle_vector_search(params).await,
            "vector/index" => self.handle_index_vector(params).await,
            "vector/stats" => self.handle_vector_stats(params).await,
            "vector/invalidate-cache" => self.handle_invalidate_vector_cache(params).await,
            "vector/backfill" => self.handle_backfill_vectors(params).await,

            // Migration between adapters
            "migration/start" => self.handle_migration_start(params).await,
            "migration/status" => self.handle_migration_status(params).await,
            "migration/pause" => self.handle_migration_pause(params).await,
            "migration/resume" => self.handle_migration_resume(params).await,
            "migration/verify" => self.handle_migration_verify(params).await,
            "migration/cutover" => self.handle_migration_cutover(params).await,
            "migration/rollback" => self.handle_migration_rollback(params).await,

            _ => Err(format!("Unknown data command: {command}")),
        }
    }

    /// Phase 1 typed-IPC scaffold: takes already-deserialized `CreateParams`.
    /// Dispatch (handle_command) does the parse; handler body is pure logic.
    /// Follow this shape for QW#3's other hot-handler conversions.
    /// Create a record (single source for the typed `data/create` command).
    /// `handle` names the storage; the persona-facing command defaults it to
    /// "main". A missing `id` is minted. Publishes `<collection>:created` on
    /// success. The body is unchanged from the legacy `handle_create` arm.
    pub(crate) async fn create_record(
        &self,
        handle: &str,
        collection: String,
        id: Option<UUID>,
        data: Value,
    ) -> Result<StorageResult<DataRecord>, String> {
        let start = std::time::Instant::now();

        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let record = DataRecord {
            id: id.clone(),
            collection: collection.clone(),
            data,
            metadata: RecordMetadata::default(),
        };

        let adapter = self.get_adapter(handle).await?;
        let result = adapter.create(record).await;
        self.log_slow_query("create", &collection, start.elapsed().as_millis());

        if result.success {
            self.publish_event(
                &collection,
                "created",
                json!({ "id": id, "collection": collection }),
            );
        }

        Ok(result)
    }

    /// Read a record by id (single source for the typed `data/read` command).
    pub(crate) async fn read_record(
        &self,
        handle: &str,
        collection: &str,
        id: &UUID,
    ) -> Result<StorageResult<DataRecord>, String> {
        let start = std::time::Instant::now();
        let adapter = self.get_adapter(handle).await?;
        let result = adapter.read(collection, id).await;
        self.log_slow_query("read", collection, start.elapsed().as_millis());
        Ok(result)
    }

    /// Update a record by id (single source for the typed `data/update` command).
    /// Publishes `<collection>:updated` on success.
    pub(crate) async fn update_record(
        &self,
        handle: &str,
        collection: String,
        id: UUID,
        data: Value,
        increment_version: bool,
    ) -> Result<StorageResult<DataRecord>, String> {
        let adapter = self.get_adapter(handle).await?;
        let result = adapter
            .update(&collection, &id, data, increment_version)
            .await;

        if result.success {
            self.publish_event(
                &collection,
                "updated",
                json!({ "id": id, "collection": collection }),
            );
        }

        Ok(result)
    }

    /// Delete a record by id (single source for the typed `data/delete` command).
    /// Publishes `<collection>:deleted` on success.
    pub(crate) async fn delete_record(
        &self,
        handle: &str,
        collection: String,
        id: UUID,
    ) -> Result<StorageResult<bool>, String> {
        let adapter = self.get_adapter(handle).await?;
        let result = adapter.delete(&collection, &id).await;

        if result.success {
            self.publish_event(
                &collection,
                "deleted",
                json!({ "id": id, "collection": collection }),
            );
        }

        Ok(result)
    }

    async fn handle_query(&self, params: QueryParams) -> Result<CommandResult, String> {
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

    async fn handle_query_with_join(
        &self,
        params: QueryWithJoinParams,
    ) -> Result<CommandResult, String> {
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

    async fn handle_count(&self, params: CountParams) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter.map(|m| {
                m.into_iter()
                    .map(|(k, v)| (k, FieldFilter::Value(v)))
                    .collect()
            }),
            ..Default::default()
        };

        let adapter_start = Instant::now();
        let adapter = self.get_adapter(&params.db_path).await?;
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
                params.collection,
                total_ms,
                adapter_ms,
                count_ms,
                result.success
            );
        }

        CommandResult::json(&result)
    }

    async fn handle_batch(&self, params: BatchParams) -> Result<CommandResult, String> {
        let op_count = params.operations.len();

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.batch(params.operations).await;

        // Publish batch event on success
        if result.success {
            self.publish_event(
                "batch",
                "completed",
                json!({
                    "operationCount": op_count,
                    "successCount": result.data.as_ref().map(|d| d.len()).unwrap_or(0)
                }),
            );
        }

        CommandResult::json(&result)
    }

    /// Phase 2 Step 3: ensure_schema pivot through resolve().
    /// Typed dispatch via deserialize_params! macro (Phase 1 primitive, m5).
    /// Schema content sourced from entity_schemas.json (build-time codegen
    /// from TS decorators), resolved per collection by the entity_schemas
    /// module. Unknown collection → hard fail with rebuild hint.
    async fn handle_ensure_schema(
        &self,
        params: EnsureSchemaParams,
    ) -> Result<CommandResult, String> {
        // Resolution order per [[orm-everything-not-hand-edited-files]]:
        //   1. Rust-native registry (substrate entities authored Rust-first:
        //      hw_tiers, role_templates, identity pools, universes).
        //   2. entity_schemas.json (TS-decorator authored: chat, users,
        //      cognition, timeline — the existing pipeline).
        //   3. Error — collection unknown to either path.
        let collection_schema = if let Some(rust_schema) =
            crate::orm::OrmEntityRegistry::global().resolve(&params.collection)
        {
            rust_schema
        } else if let Some(entity) = crate::modules::entity_schemas::resolve(&params.collection) {
            crate::modules::entity_schemas::to_collection_schema(entity)
        } else {
            return Err(format!(
                "Unknown collection '{}' — not in the Rust ORM registry and not in \
                 entity_schemas.json. If this is a newly added TS-decorated entity, \
                 rebuild TS: `npm run build:ts`. If it's a Rust-native substrate entity, \
                 confirm OrmEntityRegistry::global().register::<YourEntity>() is called \
                 at boot.",
                params.collection
            ));
        };
        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.ensure_schema(collection_schema).await;

        CommandResult::json(&result)
    }

    async fn handle_list_collections(&self, params: Value) -> Result<CommandResult, String> {
        let params: DbPathOnly =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.list_collections().await;

        CommandResult::json(&result)
    }

    async fn handle_collection_stats(&self, params: Value) -> Result<CommandResult, String> {
        let params: CollectionParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.collection_stats(&params.collection).await;

        CommandResult::json(&result)
    }

    async fn handle_truncate(&self, params: Value) -> Result<CommandResult, String> {
        let params: CollectionParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.truncate(&params.collection).await;

        CommandResult::json(&result)
    }

    async fn handle_clear_all(&self, params: Value) -> Result<CommandResult, String> {
        let params: DbPathOnly =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.clear_all().await;

        CommandResult::json(&result)
    }

    async fn handle_capabilities(&self, params: Value) -> Result<CommandResult, String> {
        let params: DbPathOnly =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
        let caps = adapter.capabilities();

        Ok(CommandResult::Json(json!({
            "supportsTransactions": caps.supports_transactions,
            "supportsJoins": caps.supports_joins,
            "supportsIndexing": caps.supports_indexing,
            "supportsFullTextSearch": caps.supports_full_text_search,
            "supportsVectorSearch": caps.supports_vector_search,
            "supportsBatch": caps.supports_batch,
            "maxRecordSize": caps.max_record_size,
        })))
    }

    async fn handle_info(&self, params: Value) -> Result<CommandResult, String> {
        let params: DbPathOnly =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
        let caps = adapter.capabilities();

        Ok(CommandResult::Json(json!({
            "adapter": adapter.name(),
            "path": params.db_path,
            "capabilities": {
                "supportsTransactions": caps.supports_transactions,
                "supportsJoins": caps.supports_joins,
            }
        })))
    }

    // =========================================================================
    // Vector Search (migrated from data-daemon-worker)
    // =========================================================================

    /// Vector similarity search with in-memory caching
    ///
    /// OPTIMIZATION: Vectors are cached in memory per (dbPath, collection).
    /// First search loads from SQLite, subsequent searches are instant.
    ///
    /// Flow:
    /// 1. Check cache (RwLock read - concurrent, no blocking)
    /// 2. If miss, load from SQLite (serialized, but only once per collection)
    /// 3. Parallel rayon search against cached vectors
    async fn handle_vector_search(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let search_start = Instant::now();

        let params: VectorSearchParams = serde_json::from_value(params.clone()).map_err(|e| {
            log_error!(
                "data",
                "vector/search",
                "Parse error: {}, params: {}",
                e,
                params
            );
            format!("Invalid params: {e}")
        })?;

        let cache_key = (params.db_path.clone(), params.collection.clone());

        // Step 1: Try to get vectors from cache (RwLock read - concurrent)
        let cached_vectors: Option<Arc<Vec<CachedVector>>> = {
            let cache = self.vector_cache.read().unwrap_or_else(|e| e.into_inner());
            cache.get(&cache_key).map(|c| c.vectors.clone())
        };

        let corpus: Arc<Vec<CachedVector>> = if let Some(vectors) = cached_vectors {
            log_info!(
                "data",
                "vector/search",
                "Cache HIT for {} ({} vectors)",
                params.collection,
                vectors.len()
            );
            vectors
        } else {
            // Cache MISS - load from SQLite
            log_info!(
                "data",
                "vector/search",
                "Cache MISS for {} - loading from SQLite",
                params.collection
            );
            let load_start = Instant::now();

            // Get adapter and load vectors
            let adapter = self.get_adapter(&params.db_path).await?;

            // Query all records with embeddings
            let query = StorageQuery {
                collection: params.collection.clone(),
                ..Default::default()
            };

            let result = adapter.query(query).await;
            if !result.success {
                return Err(result.error.unwrap_or_else(|| "Query failed".to_string()));
            }

            // Extract vectors from records
            let mut vectors: Vec<CachedVector> = Vec::new();
            for record in result.data.unwrap_or_default() {
                if let Some(embedding) = record.data.get("embedding") {
                    let vec = Self::parse_embedding(embedding);
                    if !vec.is_empty() {
                        vectors.push(CachedVector {
                            id: record.id,
                            embedding: vec,
                        });
                    }
                }
            }

            let vectors_arc = Arc::new(vectors);
            let count = vectors_arc.len();

            // Store in cache
            {
                let mut cache = self.vector_cache.write().unwrap_or_else(|e| e.into_inner());
                cache.insert(
                    cache_key,
                    VectorCache {
                        vectors: vectors_arc.clone(),
                    },
                );
            }

            log_info!(
                "data",
                "vector/search",
                "Cached {} vectors for {} in {:?}",
                count,
                params.collection,
                load_start.elapsed()
            );
            vectors_arc
        };

        if corpus.is_empty() {
            return Ok(CommandResult::Json(json!({
                "results": [],
                "count": 0,
                "corpusSize": 0
            })));
        }

        let corpus_size = corpus.len();

        // Step 2: Parallel cosine similarity with rayon
        let query_vec = &params.query_vector;
        let threshold = params.threshold;

        let mut scored: Vec<(String, f64)> = corpus
            .par_iter()
            .filter_map(|cv| {
                let score = Self::cosine_similarity(query_vec, &cv.embedding);
                if score >= threshold {
                    Some((cv.id.clone(), score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_k: Vec<(String, f64)> = scored.into_iter().take(params.k).collect();
        let count = top_k.len();

        // Build results
        let results: Vec<Value> = if params.include_data {
            // Fetch full records for top-k (need another query)
            let adapter = self.get_adapter(&params.db_path).await?;
            let mut full_results = Vec::new();

            for (id, score) in &top_k {
                let result = adapter.read(&params.collection, id).await;
                if result.success {
                    if let Some(record) = result.data {
                        full_results.push(json!({
                            "id": id,
                            "score": score,
                            "distance": 1.0 - score,
                            "data": record.data
                        }));
                    }
                }
            }
            full_results
        } else {
            top_k
                .into_iter()
                .map(|(id, score)| {
                    json!({
                        "id": id,
                        "score": score,
                        "distance": 1.0 - score
                    })
                })
                .collect()
        };

        log_info!(
            "data",
            "vector/search",
            "Complete: {} results from {} vectors in {:?}",
            count,
            corpus_size,
            search_start.elapsed()
        );

        Ok(CommandResult::Json(json!({
            "results": results,
            "count": count,
            "corpusSize": corpus_size
        })))
    }

    /// Parse embedding from record data (supports BLOB and JSON array)
    fn parse_embedding(value: &Value) -> Vec<f64> {
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
    fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
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
    async fn handle_index_vector(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: IndexVectorParams = serde_json::from_value(params.clone()).map_err(|e| {
            log_error!(
                "data",
                "vector/index",
                "Parse error: {}, params: {}",
                e,
                params
            );
            format!("Invalid params: {e}")
        })?;

        let adapter = self.get_adapter(&params.db_path).await?;

        // Update the record's embedding field
        let update_data = json!({
            "embedding": params.embedding
        });

        let result = adapter
            .update(&params.collection, &params.id, update_data, false)
            .await;

        // Invalidate vector cache for this collection since we modified an embedding
        {
            let cache_key = (params.db_path.clone(), params.collection.clone());
            let mut cache = self.vector_cache.write().unwrap_or_else(|e| e.into_inner());
            cache.remove(&cache_key);
        }

        let total_ms = start.elapsed().as_millis();
        log_info!(
            "data",
            "vector/index",
            "Indexed vector for {} in {}ms, success={}",
            params.id,
            total_ms,
            result.success
        );

        CommandResult::json(&result)
    }

    /// Get vector index statistics for a collection
    async fn handle_vector_stats(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: VectorStatsParams = serde_json::from_value(params.clone()).map_err(|e| {
            log_error!(
                "data",
                "vector/stats",
                "Parse error: {}, params: {}",
                e,
                params
            );
            format!("Invalid params: {e}")
        })?;

        let adapter = self.get_adapter(&params.db_path).await?;

        // Get total record count
        let total_query = StorageQuery {
            collection: params.collection.clone(),
            ..Default::default()
        };
        let total_result = adapter.count(total_query).await;
        let total_records = total_result.data.unwrap_or(0);

        // Query to count records WITH embeddings
        // We need to query and check which have embedding field
        let query = StorageQuery {
            collection: params.collection.clone(),
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
        let cache_key = (params.db_path.clone(), params.collection.clone());
        let cached_count = {
            let cache = self.vector_cache.read().unwrap_or_else(|e| e.into_inner());
            cache.get(&cache_key).map(|c| c.vectors.len()).unwrap_or(0)
        };

        let total_ms = start.elapsed().as_millis();
        log_info!(
            "data",
            "vector/stats",
            "Stats for {} in {}ms: total={}, with_vectors={}, dims={}",
            params.collection,
            total_ms,
            total_records,
            records_with_vectors,
            vector_dimensions
        );

        // Wrap in StorageResult-style response for TypeScript compatibility
        Ok(CommandResult::Json(json!({
            "success": true,
            "data": {
                "collection": params.collection,
                "totalRecords": total_records,
                "recordsWithVectors": records_with_vectors,
                "vectorDimensions": vector_dimensions,
                "cachedVectors": cached_count,
                "lastUpdated": chrono::Utc::now().to_rfc3339()
            }
        })))
    }

    /// Invalidate vector cache for a collection
    /// Called when records are modified outside of vector/index
    async fn handle_invalidate_vector_cache(&self, params: Value) -> Result<CommandResult, String> {
        let params: CollectionParams = serde_json::from_value(params.clone()).map_err(|e| {
            log_error!(
                "data",
                "vector/invalidate-cache",
                "Parse error: {}, params: {}",
                e,
                params
            );
            format!("Invalid params: {e}")
        })?;

        let cache_key = (params.db_path.clone(), params.collection.clone());
        let removed = {
            let mut cache = self.vector_cache.write().unwrap_or_else(|e| e.into_inner());
            cache.remove(&cache_key).is_some()
        };

        log_info!(
            "data",
            "vector/invalidate-cache",
            "Invalidated cache for {}: removed={}",
            params.collection,
            removed
        );

        Ok(CommandResult::Json(json!({
            "success": true,
            "collection": params.collection,
            "cacheInvalidated": removed
        })))
    }

    /// Backfill vectors - generate embeddings for records missing them
    ///
    /// Uses batch embedding generation for efficiency (10x faster than single).
    /// Processes in configurable batch sizes to manage memory.
    async fn handle_backfill_vectors(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: BackfillVectorsParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!(
                    "data",
                    "vector/backfill",
                    "Parse error: {}, params: {}",
                    e,
                    params
                );
                format!("Invalid params: {e}")
            })?;

        let batch_size = params.batch_size;

        // Embedding is adapter-routed (unsloth /v1/embeddings, task #40) — the
        // SAME async neural-or-lexical embedder the live recall path uses. Build
        // it ONCE for the whole backfill. `params.model` is advisory only now;
        // the served embed model is selected by the gateway. Fail loud if no
        // embedder can be built (no in-process ONNX fallback).
        let embedder = crate::modules::embedding::build_adapter_embedder()
            .await
            .map_err(|e| format!("vector/backfill: cannot build embedder: {e}"))?;

        // `params.model` is advisory only now — the served embed model is chosen
        // by the gateway (task #40). Log when a caller requested a specific model
        // so the divergence from the old fastembed model-name semantics is visible.
        if let Some(requested) = params.model.as_deref() {
            log_info!(
                "data",
                "vector/backfill",
                "requested embed model '{}' is advisory; using gateway embedder '{}'",
                requested,
                embedder.id()
            );
        }

        let adapter = self.get_adapter(&params.db_path).await?;

        // Query all records from collection
        let query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter.clone(),
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
            params.collection
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
                if let Some(text) = record.data.get(&params.text_field) {
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
                        params.collection
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
                    .update(&params.collection, &record.id, update_data, false)
                    .await;

                if update_result.success {
                    processed += 1;
                } else {
                    failed += 1;
                }
            }
        }

        // Invalidate vector cache since we modified embeddings
        {
            let cache_key = (params.db_path.clone(), params.collection.clone());
            let mut cache = self.vector_cache.write().unwrap_or_else(|e| e.into_inner());
            cache.remove(&cache_key);
        }

        let total_ms = start.elapsed().as_millis();
        log_info!(
            "data",
            "vector/backfill",
            "Backfill complete for {}: total={}, processed={}, skipped={}, failed={} in {}ms",
            params.collection,
            total,
            processed,
            skipped,
            failed,
            total_ms
        );

        Ok(CommandResult::Json(json!({
            "success": true,
            "data": {
                "collection": params.collection,
                "total": total,
                "processed": processed,
                "skipped": skipped,
                "failed": failed,
                "elapsedMs": total_ms
            }
        })))
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
    async fn handle_query_open(&self, params: QueryOpenParams) -> Result<CommandResult, String> {
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

        self.paginated_queries
            .insert(cursor_id_str.clone(), Arc::new(tokio::sync::Mutex::new(state)));

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
    async fn handle_query_next(
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
    async fn handle_query_close(
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
    async fn handle_migration_start(&self, params: Value) -> Result<CommandResult, String> {
        let params: MigrationStartParams = serde_json::from_value(params.clone()).map_err(|e| {
            log_error!(
                "data",
                "migration/start",
                "Parse error: {}, params: {}",
                e,
                params
            );
            format!("Invalid params: {e}")
        })?;

        // Get or create adapters for source and target
        let source = self.get_adapter(&params.source).await?;
        let target = self.get_adapter(&params.target).await?;

        let config = MigrationConfig {
            batch_size: params.batch_size,
            throttle_ms: params.throttle_ms,
            collections: params.collections,
        };

        let mut engine = MigrationEngine::new(source, target, config);
        let handle = engine.handle();

        // Store handle for status/pause/resume/verify (before spawning)
        *self.active_migration.lock().await = Some(handle.clone());

        let src = params.source.clone();
        let tgt = params.target.clone();

        // Spawn migration as background task — returns immediately
        tokio::spawn(async move {
            log_info!(
                "data",
                "migration/start",
                "Background migration started: {} -> {}",
                src,
                tgt
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

        // Return handle status immediately
        Ok(CommandResult::Json(handle.status_json()))
    }

    /// Get migration progress (lock-free — reads atomic counters)
    async fn handle_migration_status(&self, _params: Value) -> Result<CommandResult, String> {
        let guard = self.active_migration.lock().await;
        match guard.as_ref() {
            Some(handle) => Ok(CommandResult::Json(handle.status_json())),
            None => Err("No active migration".into()),
        }
    }

    /// Pause active migration (atomic flag — returns immediately)
    async fn handle_migration_pause(&self, _params: Value) -> Result<CommandResult, String> {
        let guard = self.active_migration.lock().await;
        match guard.as_ref() {
            Some(handle) => {
                handle.pause();
                log_info!("data", "migration/pause", "Migration paused");
                Ok(CommandResult::Json(handle.status_json()))
            }
            None => Err("No active migration".into()),
        }
    }

    /// Resume paused migration (clears pause flag, migration loop re-checks)
    async fn handle_migration_resume(&self, _params: Value) -> Result<CommandResult, String> {
        let guard = self.active_migration.lock().await;
        match guard.as_ref() {
            Some(handle) => {
                handle.resume();
                log_info!("data", "migration/resume", "Migration resumed");
                Ok(CommandResult::Json(handle.status_json()))
            }
            None => Err("No active migration".into()),
        }
    }

    /// Verify migration integrity (compare counts between source and target)
    async fn handle_migration_verify(&self, _params: Value) -> Result<CommandResult, String> {
        let guard = self.active_migration.lock().await;
        match guard.as_ref() {
            Some(handle) => {
                let verify = handle.verify().await?;
                Ok(CommandResult::Json(verify))
            }
            None => Err("No active migration".into()),
        }
    }

    /// Cutover: swap the active adapter for a connection string
    /// After migration, this redirects all operations to the new backend
    async fn handle_migration_cutover(&self, params: Value) -> Result<CommandResult, String> {
        let params: MigrationCutoverParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!(
                    "data",
                    "migration/cutover",
                    "Parse error: {}, params: {}",
                    e,
                    params
                );
                format!("Invalid params: {e}")
            })?;

        // Store current for rollback
        *self.previous_connection.lock().await = Some(params.current.clone());

        // Remove old adapter from cache (forces re-creation on next access)
        self.adapters.remove(&params.current);

        // Pre-warm the target adapter
        let _target = self.get_adapter(&params.target).await?;

        log_info!(
            "data",
            "migration/cutover",
            "Cutover: {} -> {}",
            params.current,
            params.target
        );

        Ok(CommandResult::Json(json!({
            "previous": params.current,
            "active": params.target,
            "adapter": _target.name()
        })))
    }

    /// Rollback: revert to the previous connection string
    async fn handle_migration_rollback(&self, params: Value) -> Result<CommandResult, String> {
        let params: MigrationRollbackParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!(
                    "data",
                    "migration/rollback",
                    "Parse error: {}, params: {}",
                    e,
                    params
                );
                format!("Invalid params: {e}")
            })?;

        let previous = self.previous_connection.lock().await;
        match previous.as_ref() {
            Some(prev) => {
                // Remove current adapter
                self.adapters.remove(&params.current);

                // Pre-warm previous adapter
                let _adapter = self.get_adapter(prev).await?;

                log_info!(
                    "data",
                    "migration/rollback",
                    "Rolled back: {} -> {}",
                    params.current,
                    prev
                );

                Ok(CommandResult::Json(json!({
                    "rolledBackFrom": params.current,
                    "rolledBackTo": prev,
                    "adapter": _adapter.name()
                })))
            }
            None => Err("No previous connection to rollback to".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::types::CollectionSchema;

    /// Helper: per-test isolated SQLite file routed through resolve_handle's
    /// legacy passthrough. Tests still hit the abstraction (handle resolves
    /// the same way TS callers' would); the passthrough is documented as a
    /// migration target pending sentinel-handle adoption everywhere. When
    /// the passthrough is removed, migrate these to per-test HOME +
    /// "main" handle.
    ///
    /// Returns (TempDir guard, db_path String). Hold the guard for the
    /// duration of the test — drop deletes the tempdir.
    fn test_db_path(suffix: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir
            .path()
            .join(format!("{}.db", suffix))
            .to_string_lossy()
            .to_string();
        (dir, path)
    }

    /// Test seam: drive [`DataState::create_record`] through the SAME params
    /// deserialization the typed `data/create` command uses, returning the JSON
    /// envelope the legacy arm produced so the storage-layer tests below keep
    /// asserting one shape. (The command-object wiring itself is covered by
    /// `commands/data/mod.rs`'s tests; this exercises the data path.)
    async fn create_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::data::create::DataCreateParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = module
            .state
            .create_record(handle, p.collection, p.id, p.data)
            .await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    /// Test seam: the read counterpart of [`create_via_state`].
    async fn read_via_state(
        module: &DataModule,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        let p: crate::commands::data::read::DataReadParams =
            serde_json::from_value(params).map_err(|e| e.to_string())?;
        let handle = p.handle.as_deref().unwrap_or("main");
        let result = module
            .state
            .read_record(handle, &p.collection, &p.id)
            .await?;
        Ok(CommandResult::Json(
            serde_json::to_value(result).map_err(|e| e.to_string())?,
        ))
    }

    #[test]
    fn data_create_params_drop_the_db_path_leak_but_alias_legacy_callers() {
        // what this catches: the persona-facing `data/create` contract dropped the
        // required `dbPath` leak — `handle` is optional and resolves to "main"
        // downstream — while legacy callers passing `dbPath` keep working through
        // the serde alias. If the alias or the optionality regresses, either the
        // persona surface re-grows a storage-handle leak or chat/sentinel/self_task
        // (which still send `dbPath`) silently start writing to the wrong store.
        let clean: crate::commands::data::create::DataCreateParams =
            serde_json::from_value(json!({
                "collection": "test_users",
                "data": { "name": "Alice" }
            }))
            .expect("clean params deserialize");
        assert!(clean.handle.is_none(), "omitted handle stays None (→ main)");

        let legacy: crate::commands::data::create::DataCreateParams =
            serde_json::from_value(json!({
                "dbPath": "some/store.db",
                "collection": "test_users",
                "data": { "name": "Alice" }
            }))
            .expect("legacy dbPath params deserialize");
        assert_eq!(
            legacy.handle.as_deref(),
            Some("some/store.db"),
            "legacy dbPath aliases onto handle"
        );
    }

    #[tokio::test]
    async fn test_data_module_create_and_read() {
        let module = DataModule::new();
        let (_tmp, db_path) = test_db_path("create_and_read");

        // Create table first
        let schema = CollectionSchema {
            collection: "test_users".to_string(),
            fields: vec![crate::orm::types::SchemaField {
                name: "name".to_string(),
                field_type: crate::orm::types::FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            }],
            indexes: vec![],
        };

        // Tests bypass the IPC surface (which now requires a REGISTERED
        // entity collection per Phase 2 Step 3) — call the adapter directly
        // with a synthetic test CollectionSchema.
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        // Create with dbPath
        let create_result = create_via_state(
                &module,
                json!({
                    "dbPath": &db_path,
                    "collection": "test_users",
                    "data": { "name": "Alice" }
                }),
            )
            .await;

        assert!(
            create_result.is_ok(),
            "create_result failed: {:?}",
            create_result
        );

        if let Ok(CommandResult::Json(result)) = create_result {
            assert!(result["success"].as_bool().unwrap_or(false));
            let id = result["data"]["id"].as_str().unwrap();

            // Read with dbPath
            let read_result = read_via_state(
                    &module,
                    json!({
                        "dbPath": &db_path,
                        "collection": "test_users",
                        "id": id
                    }),
                )
                .await;

            assert!(read_result.is_ok());
            if let Ok(CommandResult::Json(read)) = read_result {
                assert!(read["success"].as_bool().unwrap_or(false));
                assert_eq!(read["data"]["data"]["name"], "Alice");
            }
        }
    }

    #[tokio::test]
    async fn test_vector_index_and_stats() {
        let module = DataModule::new();
        let (_tmp, db_path) = test_db_path("vector_index");

        // Create schema with embedding field
        let schema = CollectionSchema {
            collection: "test_vectors".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "content".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                },
                crate::orm::types::SchemaField {
                    name: "embedding".to_string(),
                    field_type: crate::orm::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                },
            ],
            indexes: vec![],
        };

        // Tests bypass the IPC surface (which now requires a REGISTERED
        // entity collection per Phase 2 Step 3) — call the adapter directly
        // with a synthetic test CollectionSchema.
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        // Create a record
        let create_result = create_via_state(
                &module,
                json!({
                    "dbPath": &db_path,
                    "collection": "test_vectors",
                    "data": { "content": "Hello world" }
                }),
            )
            .await;

        assert!(
            create_result.is_ok(),
            "create_result failed: {:?}",
            create_result
        );
        let record_id = if let Ok(CommandResult::Json(result)) = &create_result {
            result["data"]["id"].as_str().unwrap().to_string()
        } else {
            panic!("Create failed");
        };

        // Index a vector for this record
        let test_embedding: Vec<f64> = (0..384).map(|i| (i as f64) * 0.001).collect();
        let index_result = module
            .handle_command(
                "vector/index",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_vectors",
                    "id": record_id,
                    "embedding": test_embedding
                }),
            )
            .await;

        assert!(index_result.is_ok());
        if let Ok(CommandResult::Json(result)) = &index_result {
            assert!(result["success"].as_bool().unwrap_or(false));
        }

        // Get vector stats
        let stats_result = module
            .handle_command(
                "vector/stats",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_vectors"
                }),
            )
            .await;

        assert!(stats_result.is_ok());
        if let Ok(CommandResult::Json(result)) = stats_result {
            let stats = &result["data"];
            assert_eq!(stats["collection"], "test_vectors");
            assert_eq!(stats["totalRecords"], 1);
            assert_eq!(stats["recordsWithVectors"], 1);
            assert_eq!(stats["vectorDimensions"], 384);
        }
    }

    #[tokio::test]
    async fn test_vector_search_basic() {
        let module = DataModule::new();
        let (_tmp, db_path) = test_db_path("vector_search");

        // Create schema
        let schema = CollectionSchema {
            collection: "test_search".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "content".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                },
                crate::orm::types::SchemaField {
                    name: "embedding".to_string(),
                    field_type: crate::orm::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                },
            ],
            indexes: vec![],
        };

        // Tests bypass the IPC surface (which now requires a REGISTERED
        // entity collection per Phase 2 Step 3) — call the adapter directly
        // with a synthetic test CollectionSchema.
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        // Create records with embeddings
        let embeddings: Vec<Vec<f64>> = vec![
            (0..384).map(|i| (i as f64) * 0.001).collect(),
            (0..384).map(|i| (i as f64) * 0.002).collect(),
            (0..384).map(|i| (i as f64) * 0.003).collect(),
        ];

        for (idx, emb) in embeddings.iter().enumerate() {
            let _ = create_via_state(
                    &module,
                    json!({
                        "dbPath": &db_path,
                        "collection": "test_search",
                        "data": {
                            "content": format!("Document {}", idx),
                            "embedding": emb
                        }
                    }),
                )
                .await;
        }

        // Search for similar vectors
        let query_vector: Vec<f64> = (0..384).map(|i| (i as f64) * 0.001).collect();
        let search_result = module
            .handle_command(
                "vector/search",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_search",
                    "queryVector": query_vector,
                    "k": 3,
                    "threshold": 0.0,
                    "includeData": true
                }),
            )
            .await;

        assert!(search_result.is_ok());
        if let Ok(CommandResult::Json(result)) = search_result {
            let results = result["results"].as_array().unwrap();
            assert_eq!(results.len(), 3);
            // First result should be most similar (score close to 1.0)
            let first_score = results[0]["score"].as_f64().unwrap();
            assert!(
                first_score > 0.9,
                "Expected high similarity, got {}",
                first_score
            );
        }
    }

    #[tokio::test]
    async fn test_vector_cache_invalidation() {
        let module = DataModule::new();
        let (_tmp, db_path) = test_db_path("vector_cache");

        // Create schema
        let schema = CollectionSchema {
            collection: "test_cache".to_string(),
            fields: vec![crate::orm::types::SchemaField {
                name: "embedding".to_string(),
                field_type: crate::orm::types::FieldType::Json,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            }],
            indexes: vec![],
        };

        // Tests bypass the IPC surface (which now requires a REGISTERED
        // entity collection per Phase 2 Step 3) — call the adapter directly
        // with a synthetic test CollectionSchema.
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        // Create a record with embedding
        let _ = create_via_state(
                &module,
                json!({
                    "dbPath": &db_path,
                    "collection": "test_cache",
                    "data": {
                        "embedding": vec![1.0; 384]
                    }
                }),
            )
            .await;

        // First search populates cache
        let query: Vec<f64> = vec![1.0; 384];
        let _ = module
            .handle_command(
                "vector/search",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_cache",
                    "queryVector": query,
                    "k": 1
                }),
            )
            .await;

        // Verify cache has vectors via stats
        let stats_result = module
            .handle_command(
                "vector/stats",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_cache"
                }),
            )
            .await;

        if let Ok(CommandResult::Json(result)) = &stats_result {
            let stats = &result["data"];
            assert!(stats["cachedVectors"].as_u64().unwrap() > 0);
        }

        // Invalidate cache
        let invalidate_result = module
            .handle_command(
                "vector/invalidate-cache",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_cache"
                }),
            )
            .await;

        assert!(invalidate_result.is_ok());
        if let Ok(CommandResult::Json(result)) = invalidate_result {
            assert!(result["success"].as_bool().unwrap_or(false));
            assert!(result["cacheInvalidated"].as_bool().unwrap_or(false));
        }

        // Verify cache is empty
        let stats_after = module
            .handle_command(
                "vector/stats",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_cache"
                }),
            )
            .await;

        if let Ok(CommandResult::Json(result)) = stats_after {
            let stats = &result["data"];
            assert_eq!(stats["cachedVectors"].as_u64().unwrap(), 0);
        }
    }

    #[tokio::test]
    async fn test_paginated_query() {
        let module = DataModule::new();
        let (_tmp, db_path) = test_db_path("paginated");

        // Create schema
        let schema = CollectionSchema {
            collection: "test_paginated".to_string(),
            fields: vec![crate::orm::types::SchemaField {
                name: "name".to_string(),
                field_type: crate::orm::types::FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            }],
            indexes: vec![],
        };

        // Tests bypass the IPC surface (registered-entity-only post-Step 3).
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        // Create 25 records
        for i in 0..25 {
            let _ = create_via_state(
                    &module,
                    json!({
                        "dbPath": db_path,
                        "collection": "test_paginated",
                        "data": { "name": format!("Item {}", i) }
                    }),
                )
                .await;
        }

        // Open paginated query with page size 10. Default count_exact=false
        // means the response carries totalCount=0 (sentinel for "not
        // requested") and has_more starts optimistic; the LIMIT N+1 probe
        // in query-next is the authoritative has_more.
        let open_result = module
            .handle_command(
                "data/query-open",
                json!({
                    "dbPath": db_path,
                    "collection": "test_paginated",
                    "pageSize": 10
                }),
            )
            .await;

        assert!(open_result.is_ok(), "open_result failed: {:?}", open_result);
        let query_id = if let Ok(CommandResult::Json(result)) = &open_result {
            let data = &result["data"];
            assert_eq!(
                data["totalCount"], 0,
                "QW#2: count_exact=false skips COUNT(*); 0 is the sentinel"
            );
            assert_eq!(data["pageSize"], 10);
            assert!(
                data["hasMore"].as_bool().unwrap(),
                "QW#2: open is optimistic, query-next is authoritative"
            );
            data["queryId"].as_str().unwrap().to_string()
        } else {
            panic!("Expected JSON result");
        };

        // Get first page
        let page1 = module
            .handle_command("data/query-next", json!({ "queryId": query_id }))
            .await;

        assert!(page1.is_ok());
        if let Ok(CommandResult::Json(result)) = &page1 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 10);
            assert_eq!(data["pageNumber"], 1);
            assert!(data["hasMore"].as_bool().unwrap());
        }

        // Get second page
        let page2 = module
            .handle_command("data/query-next", json!({ "queryId": query_id }))
            .await;

        assert!(page2.is_ok());
        if let Ok(CommandResult::Json(result)) = &page2 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 10);
            assert_eq!(data["pageNumber"], 2);
            assert!(data["hasMore"].as_bool().unwrap());
        }

        // Get third page (should have 5 items)
        let page3 = module
            .handle_command("data/query-next", json!({ "queryId": query_id }))
            .await;

        assert!(page3.is_ok());
        if let Ok(CommandResult::Json(result)) = &page3 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 5);
            assert_eq!(data["pageNumber"], 3);
            assert!(!data["hasMore"].as_bool().unwrap()); // No more pages
        }

        // Close query
        let close_result = module
            .handle_command("data/query-close", json!({ "queryId": query_id }))
            .await;

        assert!(close_result.is_ok());
        if let Ok(CommandResult::Json(result)) = close_result {
            assert!(result["success"].as_bool().unwrap());
        }
    }

    /// QW#2 back-compat: callers that need an exact "X of N" total can opt
    /// in via count_exact: true. This restores the pre-QW#2 behavior — one
    /// COUNT(*) at open time, totalCount populated in the response.
    #[tokio::test]
    async fn test_paginated_query_count_exact() {
        let module = DataModule::new();
        let (_tmp, db_path) = test_db_path("count_exact");

        let schema = CollectionSchema {
            collection: "test_count_exact".to_string(),
            fields: vec![crate::orm::types::SchemaField {
                name: "name".to_string(),
                field_type: crate::orm::types::FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            }],
            indexes: vec![],
        };
        // Tests bypass the IPC surface (registered-entity-only post-Step 3).
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        for i in 0..7 {
            let _ = create_via_state(
                    &module,
                    json!({
                        "dbPath": db_path,
                        "collection": "test_count_exact",
                        "data": { "name": format!("Item {}", i) }
                    }),
                )
                .await;
        }

        let open_result = module
            .handle_command(
                "data/query-open",
                json!({
                    "dbPath": db_path,
                    "collection": "test_count_exact",
                    "pageSize": 10,
                    "countExact": true
                }),
            )
            .await;

        assert!(open_result.is_ok(), "open_result failed: {:?}", open_result);
        if let Ok(CommandResult::Json(result)) = open_result {
            let data = &result["data"];
            assert_eq!(
                data["totalCount"], 7,
                "count_exact=true should populate totalCount via COUNT(*)"
            );
            assert!(data["hasMore"].as_bool().unwrap());
        } else {
            panic!("Expected JSON result");
        }
    }

    #[tokio::test]
    #[ignore = "Requires libonnxruntime.dylib — run with ORT_DYLIB_PATH set"]
    async fn test_backfill_vectors() {
        let module = DataModule::new();
        let (_tmp, db_path) = test_db_path("backfill");

        // Create schema with content and embedding fields
        let schema = CollectionSchema {
            collection: "test_backfill".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "content".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                },
                crate::orm::types::SchemaField {
                    name: "embedding".to_string(),
                    field_type: crate::orm::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                },
            ],
            indexes: vec![],
        };

        // Tests bypass the IPC surface (which now requires a REGISTERED
        // entity collection per Phase 2 Step 3) — call the adapter directly
        // with a synthetic test CollectionSchema.
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        // Create records without embeddings
        for i in 0..5 {
            let _ = create_via_state(
                    &module,
                    json!({
                        "dbPath": &db_path,
                        "collection": "test_backfill",
                        "data": { "content": format!("Test content number {}", i) }
                    }),
                )
                .await;
        }

        // Run backfill
        let backfill_result = module
            .handle_command(
                "vector/backfill",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_backfill",
                    "textField": "content",
                    "batchSize": 10
                }),
            )
            .await;

        assert!(backfill_result.is_ok(), "Backfill should succeed");

        if let Ok(CommandResult::Json(result)) = backfill_result {
            assert!(result["success"].as_bool().unwrap_or(false));
            let data = &result["data"];
            assert_eq!(data["total"].as_u64().unwrap(), 5);
            assert_eq!(data["processed"].as_u64().unwrap(), 5);
            assert_eq!(data["failed"].as_u64().unwrap(), 0);
        }

        // Verify embeddings were added
        let stats_result = module
            .handle_command(
                "vector/stats",
                json!({
                    "dbPath": &db_path,
                    "collection": "test_backfill"
                }),
            )
            .await;

        assert!(stats_result.is_ok());
        if let Ok(CommandResult::Json(result)) = stats_result {
            let stats = &result["data"];
            assert_eq!(stats["recordsWithVectors"].as_u64().unwrap(), 5);
            assert!(stats["vectorDimensions"].as_u64().unwrap() > 0);
        }
    }

    #[test]
    fn test_cosine_similarity() {
        // Test identical vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = DataState::cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 0.001,
            "Identical vectors should have similarity 1.0"
        );

        // Test orthogonal vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = DataState::cosine_similarity(&a, &b);
        assert!(
            sim.abs() < 0.001,
            "Orthogonal vectors should have similarity 0.0"
        );

        // Test opposite vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = DataState::cosine_similarity(&a, &b);
        assert!(
            (sim + 1.0).abs() < 0.001,
            "Opposite vectors should have similarity -1.0"
        );

        // Test with 384-dimension vectors (typical embedding size)
        let a: Vec<f64> = (0..384).map(|i| (i as f64) * 0.01).collect();
        let b: Vec<f64> = (0..384).map(|i| (i as f64) * 0.01).collect();
        let sim = DataState::cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 0.001,
            "Identical 384-dim vectors should have similarity 1.0"
        );
    }

    // ====================================================================
    // HandleRef migration tests for data/query-open/next/close
    // ====================================================================
    //
    // The cursor surface migrated from a hand-rolled string queryId to
    // typed HandleRef minted via CommandResponse::with_handle. These
    // tests cover the migration's hard edges:
    //   - both wire shapes (envelope handle + legacy queryId) resolve
    //   - cross-module/cross-resource handles fail loud with named
    //     owner/type values, not silent misroutes
    //   - stale handles surface a typed "handle not found" error that
    //     names the cursor + suggests likely causes
    //   - the legacy field stays additive — old TS consumers see the
    //     same JSON shape they parse today, plus a new top-level
    //     `handle` field they can ignore

    /// Helper: stand up a fresh DataModule + a temp SQLite + the schema
    /// + N rows. Used by every cursor test below — keeps the cursor
    /// tests focused on the handle behavior, not on row setup.
    async fn setup_paginated_for_handle_tests(
        suffix: &str,
        rows: usize,
    ) -> (DataModule, tempfile::TempDir, String) {
        let module = DataModule::new();
        let (tmp, db_path) = test_db_path(suffix);

        let schema = CollectionSchema {
            collection: "test_handle_cursor".to_string(),
            fields: vec![crate::orm::types::SchemaField {
                name: "name".to_string(),
                field_type: crate::orm::types::FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            }],
            indexes: vec![],
        };
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;

        for i in 0..rows {
            let _ = create_via_state(
                    &module,
                    json!({
                        "dbPath": &db_path,
                        "collection": "test_handle_cursor",
                        "data": { "name": format!("Item {i}") }
                    }),
                )
                .await;
        }
        (module, tmp, db_path)
    }

    /// Helper: open a cursor + return the response JSON so each test
    /// can read the new `handle` field and the legacy `data.queryId`
    /// without re-implementing the open call.
    async fn open_cursor(module: &DataModule, db_path: &str, page_size: usize) -> Value {
        let result = module
            .handle_command(
                "data/query-open",
                json!({
                    "dbPath": db_path,
                    "collection": "test_handle_cursor",
                    "pageSize": page_size,
                }),
            )
            .await
            .expect("query-open must succeed");
        let CommandResult::Json(v) = result else {
            panic!("query-open must return CommandResult::Json")
        };
        v
    }

    #[tokio::test]
    async fn query_open_returns_handle_alongside_legacy_query_id() {
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("handle_open", 3).await;
        let response = open_cursor(&module, &db_path, 10).await;

        // Legacy shape: nested data.queryId still present so existing
        // TS consumers keep parsing the same fields.
        let legacy_id = response["data"]["queryId"]
            .as_str()
            .expect("legacy queryId must remain in the response shape during migration window");

        // New shape: kernel-level handle minted at top level with the
        // canonical owner + type tag from the data module's
        // QUERY_CURSOR_TYPE_TAG / DATA_MODULE_OWNER constants.
        let handle = &response["handle"];
        assert!(handle.is_object(), "handle must be present: {response}");
        assert_eq!(handle["owner"], "data");
        assert_eq!(handle["type_tag"], "data::QueryCursor");
        assert!(
            handle["created_at_ms"].as_u64().is_some(),
            "handle must carry a creation timestamp"
        );

        // Identity invariant: the two surfaces MUST address the same
        // cursor. Otherwise a caller threading the handle and a
        // caller threading the queryId would see different state.
        let handle_id = handle["id"]
            .as_str()
            .expect("handle.id must be the canonical UUID string");
        assert_eq!(
            legacy_id, handle_id,
            "legacy queryId and handle.id must be the SAME UUID — otherwise dual-shape callers diverge"
        );
        // Both fields are real UUIDs.
        uuid::Uuid::parse_str(handle_id).expect("handle.id must parse as a UUID");
    }

    #[tokio::test]
    async fn query_next_accepts_handle_in_envelope() {
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("handle_next", 5).await;
        let open = open_cursor(&module, &db_path, 3).await;
        let handle = open["handle"].clone();

        // New canonical shape: thread the handle via the envelope.
        let next = module
            .handle_command("data/query-next", json!({ "handle": handle }))
            .await
            .expect("query-next via handle must succeed");
        let CommandResult::Json(v) = next else {
            panic!("expected Json result")
        };
        assert_eq!(
            v["data"]["items"].as_array().unwrap().len(),
            3,
            "first page must contain pageSize items"
        );
        assert_eq!(v["data"]["pageNumber"], 1);
        assert_eq!(v["data"]["hasMore"], true);
    }

    #[tokio::test]
    async fn query_next_still_accepts_legacy_query_id_field() {
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("handle_legacy", 5).await;
        let open = open_cursor(&module, &db_path, 3).await;
        let legacy_id = open["data"]["queryId"].as_str().unwrap().to_string();

        // Existing TS callsites send {"queryId": "..."} flat — that path
        // must keep working through the migration window.
        let next = module
            .handle_command("data/query-next", json!({ "queryId": legacy_id }))
            .await
            .expect("query-next via legacy queryId must succeed");
        let CommandResult::Json(v) = next else {
            panic!("expected Json result")
        };
        assert_eq!(v["data"]["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn query_next_rejects_handle_with_wrong_owner() {
        // KINK: a handle minted by another module reaching this
        // module's handler is a routing bug — fail loud with the
        // mis-owned value named, NOT a silent lookup miss that would
        // look like "stale handle".
        let (module, _tmp, _db) = setup_paginated_for_handle_tests("handle_wrong_owner", 1).await;
        let bogus_handle = json!({
            "owner": "chat",
            "id": uuid::Uuid::new_v4().to_string(),
            "type_tag": "data::QueryCursor",
            "created_at_ms": 0_u64,
        });
        let err = module
            .handle_command("data/query-next", json!({ "handle": bogus_handle }))
            .await
            .expect_err("handle with non-data owner must surface a typed error");
        assert!(
            err.contains("handle owner mismatch"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("\"chat\"") && err.contains("\"data\""),
            "error must name both the offender and the expected owner: {err}"
        );
    }

    #[tokio::test]
    async fn query_next_rejects_handle_with_wrong_type_tag() {
        // KINK: even within the data module, multiple handle shapes
        // are possible in principle (a future data::Migration handle
        // alongside data::QueryCursor). Threading the wrong type tag
        // here must fail loud, not silently treat it as a cursor.
        let (module, _tmp, _db) = setup_paginated_for_handle_tests("handle_wrong_type", 1).await;
        let wrong_type = json!({
            "owner": "data",
            "id": uuid::Uuid::new_v4().to_string(),
            "type_tag": "data::Migration",
            "created_at_ms": 0_u64,
        });
        let err = module
            .handle_command("data/query-next", json!({ "handle": wrong_type }))
            .await
            .expect_err("wrong type_tag must surface a typed error");
        assert!(
            err.contains("handle type mismatch"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("data::Migration") && err.contains("data::QueryCursor"),
            "error must name both the offender and the expected type: {err}"
        );
    }

    #[tokio::test]
    async fn query_next_rejects_when_neither_handle_nor_query_id_provided() {
        // No handle, no queryId. The TS resolver previously deserialized
        // an empty `{}` into a `QueryNextParams` with an empty string;
        // here, BOTH fields are optional so the empty case is reachable.
        // It must surface a typed error rather than silently 404 with
        // an empty-string lookup.
        let (module, _tmp, _db) = setup_paginated_for_handle_tests("handle_neither", 1).await;
        let err = module
            .handle_command("data/query-next", json!({}))
            .await
            .expect_err("empty params must surface a typed error");
        assert!(
            err.contains("neither `handle`")
                && err.contains("nor `queryId`"),
            "error must name both supported shapes: {err}"
        );
    }

    #[tokio::test]
    async fn query_next_with_unknown_handle_returns_handle_not_found() {
        // Stale-handle path: a well-formed handle whose id was never
        // (or no longer) in the DashMap. Must surface a typed error
        // that names the cursor + suggests likely causes (TTL eviction,
        // already-closed, prior process instance).
        let (module, _tmp, _db) = setup_paginated_for_handle_tests("handle_unknown", 1).await;
        let stale_handle = json!({
            "owner": "data",
            "id": uuid::Uuid::new_v4().to_string(),
            "type_tag": "data::QueryCursor",
            "created_at_ms": 0_u64,
        });
        let err = module
            .handle_command("data/query-next", json!({ "handle": stale_handle }))
            .await
            .expect_err("stale handle must surface a typed error");
        assert!(
            err.contains("handle not found"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("query-close") || err.contains("evicted"),
            "error must hint at likely causes so the caller can self-diagnose: {err}"
        );
    }

    #[tokio::test]
    async fn query_close_accepts_handle_in_envelope() {
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("handle_close", 1).await;
        let open = open_cursor(&module, &db_path, 5).await;
        let handle = open["handle"].clone();

        let close = module
            .handle_command("data/query-close", json!({ "handle": handle }))
            .await
            .expect("close via handle must succeed");
        let CommandResult::Json(v) = close else {
            panic!("expected Json result")
        };
        assert_eq!(v["success"], true);

        // Subsequent next on the SAME handle must now fail loud — the
        // close actually freed the state, not just acked.
        let stale_handle = open["handle"].clone();
        let err = module
            .handle_command("data/query-next", json!({ "handle": stale_handle }))
            .await
            .expect_err("after-close lookup must fail loud");
        assert!(
            err.contains("handle not found"),
            "close + reuse must surface stale-handle error: {err}"
        );
    }

    #[tokio::test]
    async fn query_close_still_accepts_legacy_query_id_field() {
        let (module, _tmp, db_path) =
            setup_paginated_for_handle_tests("handle_close_legacy", 1).await;
        let open = open_cursor(&module, &db_path, 5).await;
        let legacy_id = open["data"]["queryId"].as_str().unwrap().to_string();

        let close = module
            .handle_command("data/query-close", json!({ "queryId": legacy_id }))
            .await
            .expect("legacy close must succeed");
        let CommandResult::Json(v) = close else {
            panic!("expected Json result")
        };
        assert_eq!(v["success"], true);
    }

    #[tokio::test]
    async fn full_round_trip_open_next_close_via_handles_only() {
        // End-to-end through the new canonical shape ONLY (no legacy
        // queryId reads). 12 rows, page size 5: page 1 → 5 items,
        // page 2 → 5 items, page 3 → 2 items + hasMore=false. The
        // handle stays valid across the entire cursor lifetime.
        let (module, _tmp, db_path) = setup_paginated_for_handle_tests("round_trip", 12).await;
        let open = open_cursor(&module, &db_path, 5).await;
        let handle = open["handle"].clone();

        // ── page 1 ───────────────────────────────────────────────────
        let p1 = module
            .handle_command("data/query-next", json!({ "handle": handle.clone() }))
            .await
            .expect("page 1 must succeed");
        let CommandResult::Json(p1) = p1 else {
            panic!("expected Json")
        };
        assert_eq!(p1["data"]["items"].as_array().unwrap().len(), 5);
        assert_eq!(p1["data"]["pageNumber"], 1);
        assert_eq!(p1["data"]["hasMore"], true);

        // ── page 2 ───────────────────────────────────────────────────
        let p2 = module
            .handle_command("data/query-next", json!({ "handle": handle.clone() }))
            .await
            .expect("page 2 must succeed");
        let CommandResult::Json(p2) = p2 else {
            panic!("expected Json")
        };
        assert_eq!(p2["data"]["items"].as_array().unwrap().len(), 5);
        assert_eq!(p2["data"]["pageNumber"], 2);
        assert_eq!(p2["data"]["hasMore"], true);

        // ── page 3: partial + terminal ───────────────────────────────
        let p3 = module
            .handle_command("data/query-next", json!({ "handle": handle.clone() }))
            .await
            .expect("page 3 must succeed");
        let CommandResult::Json(p3) = p3 else {
            panic!("expected Json")
        };
        assert_eq!(p3["data"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(p3["data"]["pageNumber"], 3);
        assert_eq!(p3["data"]["hasMore"], false);

        // ── close ────────────────────────────────────────────────────
        let close = module
            .handle_command("data/query-close", json!({ "handle": handle }))
            .await
            .expect("close must succeed");
        let CommandResult::Json(close) = close else {
            panic!("expected Json")
        };
        assert_eq!(close["success"], true);
    }

    // ════════════════════════════════════════════════════════════════
    // Concurrency stress tests for the query-cursor surface — gated
    // behind the `stress-tests` cargo feature. Default `cargo test`
    // skips compilation; periodic CI runs them via
    //     cargo test -p continuum-core --features stress-tests
    // See continuum-core/Cargo.toml § "stress-tests" for the doctrine.
    // ════════════════════════════════════════════════════════════════
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
    //
    // Per Joel 2026-05-30: "Each persona exists in its own threads."
    //
    // The DataModule is registered ONCE; every persona's thread calls
    // its `&self` handlers concurrently. The paginated-query state
    // map is a `DashMap` precisely so concurrent cursor activity
    // doesn't serialize at a module-level mutex. The tests below
    // pin the invariants the substrate is designed to uphold under
    // that load — they are not exercising rare paths, they are the
    // production scenario.
    //
    // Every test uses `flavor = "multi_thread", worker_threads = 4`
    // so tasks actually preempt each other on distinct OS threads.
    // Single-threaded tokio would silently serialize and pass even
    // if the substrate had a data race.

    /// Build a fresh `Arc<DataModule>` + tempdir + schema + N seeded
    /// rows for a concurrency test. Returns the Arc so callers can
    /// `.clone()` it into spawned tasks without lifetime gymnastics.
    /// The tempdir's lifetime extends past the test body when bound
    /// to a `let _tmp = ...` binding so the SQLite file stays alive
    /// for the duration of every spawned task.
    async fn setup_concurrent(
        suffix: &str,
        rows: usize,
    ) -> (Arc<DataModule>, tempfile::TempDir, String) {
        let module = Arc::new(DataModule::new());
        let (tmp, db_path) = test_db_path(suffix);
        let schema = CollectionSchema {
            collection: "test_handle_cursor".to_string(),
            fields: vec![crate::orm::types::SchemaField {
                name: "name".to_string(),
                field_type: crate::orm::types::FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            }],
            indexes: vec![],
        };
        let adapter = module.state.get_adapter(&db_path).await.unwrap();
        let _ = adapter.ensure_schema(schema).await;
        for i in 0..rows {
            let _ = create_via_state(
                    &module,
                    json!({
                        "dbPath": &db_path,
                        "collection": "test_handle_cursor",
                        "data": { "name": format!("Item {i}") }
                    }),
                )
                .await;
        }
        (module, tmp, db_path)
    }

    /// N personas open their own cursor at the same time. Every cursor
    /// must mint a DISTINCT HandleRef.id (UUID collision check), every
    /// cursor must be independently reachable via query-next, and
    /// closing one must NOT close any other.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn cursors_are_isolated_under_concurrent_open_and_next() {
        const PARALLEL: usize = 20;
        // 10 rows seeded → pageSize 3 means each cursor's first page
        // is a full 3-item page (3 + 3 + 3 + 1 = 4 pages total).
        let (module, _tmp, db_path) = setup_concurrent("conc_isolated", 10).await;

        // Phase 1: every persona opens its own cursor in parallel.
        let mut open_tasks = Vec::with_capacity(PARALLEL);
        for _ in 0..PARALLEL {
            let module = module.clone();
            let db_path = db_path.clone();
            open_tasks.push(tokio::spawn(async move {
                let result = module
                    .handle_command(
                        "data/query-open",
                        json!({
                            "dbPath": db_path,
                            "collection": "test_handle_cursor",
                            "pageSize": 3,
                        }),
                    )
                    .await
                    .expect("query-open must succeed");
                let CommandResult::Json(v) = result else {
                    panic!("expected Json")
                };
                v["handle"].clone()
            }));
        }
        let handles: Vec<Value> = futures::future::join_all(open_tasks)
            .await
            .into_iter()
            .map(|h| h.expect("task must not panic"))
            .collect();

        // Every minted cursor must have a distinct id.
        let mut ids: Vec<String> = handles
            .iter()
            .map(|h| h["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(
            ids.len(),
            before,
            "concurrent query-open MUST produce distinct cursor UUIDs ({} dups)",
            before - ids.len()
        );
        assert_eq!(ids.len(), PARALLEL);

        // Phase 2: every persona advances its OWN cursor in parallel.
        // Each cursor's first query-next must return a full page (3
        // items); page numbering must be per-cursor (always 1 for the
        // first call), not cross-contaminated.
        let mut next_tasks = Vec::with_capacity(PARALLEL);
        for handle in &handles {
            let module = module.clone();
            let handle = handle.clone();
            next_tasks.push(tokio::spawn(async move {
                let result = module
                    .handle_command("data/query-next", json!({ "handle": handle }))
                    .await
                    .expect("query-next must succeed");
                let CommandResult::Json(v) = result else {
                    panic!("expected Json")
                };
                (
                    v["data"]["items"].as_array().unwrap().len(),
                    v["data"]["pageNumber"].as_u64().unwrap(),
                )
            }));
        }
        let next_results: Vec<(usize, u64)> = futures::future::join_all(next_tasks)
            .await
            .into_iter()
            .map(|r| r.expect("task must not panic"))
            .collect();

        for (i, (items, page)) in next_results.iter().enumerate() {
            assert_eq!(
                *items, 3,
                "cursor {i}: first page must return pageSize items independently of sibling cursors"
            );
            assert_eq!(
                *page, 1,
                "cursor {i}: first call's pageNumber must be 1 — per-cursor state, not shared"
            );
        }

        // Phase 3: close half the cursors in parallel. The OTHER half
        // must still be usable — close MUST be per-cursor.
        let (to_close, to_keep): (Vec<_>, Vec<_>) = handles
            .iter()
            .enumerate()
            .partition(|(i, _)| i % 2 == 0);

        let mut close_tasks = Vec::with_capacity(to_close.len());
        for (_, handle) in &to_close {
            let module = module.clone();
            let handle = (*handle).clone();
            close_tasks.push(tokio::spawn(async move {
                module
                    .handle_command("data/query-close", json!({ "handle": handle }))
                    .await
            }));
        }
        for r in futures::future::join_all(close_tasks).await {
            r.unwrap().expect("close must succeed");
        }

        // Closed cursors fail loud on next.
        for (_, handle) in &to_close {
            let err = module
                .handle_command("data/query-next", json!({ "handle": (*handle).clone() }))
                .await
                .expect_err("closed cursor's next must Err");
            assert!(
                err.contains("handle not found"),
                "closed cursor must surface handle-not-found, got: {err}"
            );
        }

        // Kept cursors still serve their next page (page 2).
        for (i, handle) in &to_keep {
            let result = module
                .handle_command("data/query-next", json!({ "handle": (*handle).clone() }))
                .await
                .unwrap_or_else(|e| panic!("kept cursor {i} must still work: {e}"));
            let CommandResult::Json(v) = result else {
                panic!("expected Json")
            };
            assert_eq!(
                v["data"]["pageNumber"], 2,
                "kept cursor {i}: page 2 follows page 1 — closing sibling cursors did NOT touch this one's state"
            );
        }
    }

    /// Same cursor reached by N concurrent `query-next` calls (whether
    /// from one persona retrying or two callers sharing a handle): the
    /// substrate MUST serialize them via the per-cursor mutex so the
    /// cursor advances atomically. Each non-tail page must be served
    /// AT MOST ONCE.
    ///
    /// Originally caught a real substrate kink: without the per-cursor
    /// mutex, all N concurrent callers read the same `current_page`
    /// snapshot and all returned pageNumber=1. The fix wrapped each
    /// cursor's state in a `tokio::sync::Mutex` so the read-then-
    /// async-then-write window is atomic per cursor.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn same_cursor_concurrent_next_does_not_corrupt_state() {
        const PARALLEL: usize = 8;
        // 30 items at pageSize 5 = 6 pages. With the per-cursor mutex,
        // each non-tail page (1..=5) is served exactly once and page 6
        // is the terminal page (hasMore=false); any extra concurrent
        // calls after that observe the empty-tail response.
        let (module, _tmp, db_path) = setup_concurrent("conc_same_cursor", 30).await;

        let open = module
            .handle_command(
                "data/query-open",
                json!({
                    "dbPath": db_path,
                    "collection": "test_handle_cursor",
                    "pageSize": 5,
                }),
            )
            .await
            .expect("open must succeed");
        let CommandResult::Json(open) = open else {
            panic!("expected Json")
        };
        let handle = open["handle"].clone();

        // Fire PARALLEL concurrent next calls against the SAME handle.
        let mut tasks = Vec::with_capacity(PARALLEL);
        for _ in 0..PARALLEL {
            let module = module.clone();
            let handle = handle.clone();
            tasks.push(tokio::spawn(async move {
                module
                    .handle_command("data/query-next", json!({ "handle": handle }))
                    .await
            }));
        }
        let outcomes: Vec<Result<CommandResult, String>> = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|r| r.expect("task must not panic"))
            .collect();

        // No call should error from concurrency (DashMap's per-shard
        // locking handles the contention). After the cursor exhausts,
        // the substrate returns success with `hasMore=false` and an
        // empty items list — not an error.
        for (i, outcome) in outcomes.iter().enumerate() {
            assert!(
                outcome.is_ok(),
                "concurrent next call {i} must not Err: {:?}",
                outcome
            );
        }

        // The 6 valid pages + however many empty-tail responses fired
        // before the cursor exhausted. Page numbers must be monotone
        // when sorted; no duplicates of a non-tail page (each non-tail
        // page can only be served ONCE because the cursor advances).
        let mut page_numbers: Vec<u64> = outcomes
            .iter()
            .filter_map(|o| o.as_ref().ok())
            .filter_map(|r| match r {
                CommandResult::Json(v) => v["data"]["pageNumber"].as_u64(),
                _ => None,
            })
            .collect();
        page_numbers.sort();

        // Every served page number must be in [1, 6] (we have 30 items
        // at pageSize 5 → 6 real pages, all subsequent calls see page
        // 6 again because the cursor stays at exhausted).
        for &pn in &page_numbers {
            assert!(
                (1..=6).contains(&pn),
                "concurrent next produced an out-of-range pageNumber: {pn} (expected 1..=6)"
            );
        }

        // CRITICAL: each non-tail page (1..=5) must appear AT MOST
        // once — DashMap's `get_mut` serializes mutators, so the
        // cursor only advances through each page once. (Page 6 may
        // appear multiple times because once exhausted the cursor
        // stops advancing but keeps returning the empty-tail response
        // — that's the contract.)
        let mut non_tail_counts = std::collections::HashMap::new();
        for &pn in page_numbers.iter().filter(|&&pn| pn < 6) {
            *non_tail_counts.entry(pn).or_insert(0) += 1;
        }
        for (page, count) in non_tail_counts {
            assert_eq!(
                count, 1,
                "page {page} served {count} times — the cursor advanced through it MORE than once, indicating a lost serialization"
            );
        }
    }
    } // end mod stress

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
#[ts(export, export_to = "../../../protocol/typescript/data/OrderByClause.ts")]
pub struct OrderByClause {
    pub field: String,
    pub direction: SortDir,
}

/// Params for `data/list` — read records from a collection.
///
/// Persona/UI-facing contract: name the `collection` and (optionally) an
/// intuitive plain-JSON `filter`, ordering, and paging. There is deliberately
/// no database handle to reason about — the shared "main" store is the default.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/data/DataListParams.ts")]
pub struct DataListParams {
    /// The collection to read (e.g. "rooms", "users", "messages").
    pub collection: String,
    /// Optional field filter as plain JSON: `{"roomId": "general"}` for an
    /// equality match, `{"age": {"$gt": 18}}` for an operator match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub filter: Option<serde_json::Value>,
    /// Optional ordering clauses, applied in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sort: Option<Vec<OrderByClause>>,
    /// Max records to return. Omit for all matching (bounded by the store).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Records to skip before returning (paging alongside `limit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub offset: Option<u32>,
    /// Storage handle. Defaults to "main" (the shared DB). Power callers may pass
    /// "@persona:<slug>" or "@metrics" to target a specific store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

/// Result of `data/list` — the matching records + an accurate total.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/data/DataListResult.ts")]
pub struct DataListResult {
    /// The matching records (each carries id, collection, data, metadata).
    #[ts(type = "Array<unknown>")]
    pub items: Vec<serde_json::Value>,
    /// Total records matching the filter (SQL COUNT — independent of `limit`).
    pub total: u32,
}
