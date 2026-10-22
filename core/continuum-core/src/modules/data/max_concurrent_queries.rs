use super::*;

/// Max concurrent query operations. Limits peak heap from personas × N queries
/// all materializing result sets simultaneously. Without this, RSS spikes
/// from concurrent JSON serialization of large result sets.
///
/// Set to 16 to match SQLite reader pool capacity (main DB has 20 readers).
/// Previous value of 4 caused head-of-line blocking: 20 readers sat idle
/// while the semaphore refused to let queries through, causing cascading
/// timeouts under normal 15-persona load.
pub(super) const MAX_CONCURRENT_QUERIES: usize = 16;

// Adapter builders interpolate field/table identifiers. The generic API accepts
// names, not SQL expressions: otherwise a public-table projection or filter
// could subquery a protected collection. Values remain ordinary bound data.
pub(super) fn require_identifier(name: &str) -> Result<(), String> {
    let mut bytes = name.bytes();
    if !bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("invalid storage identifier: expected letters, digits, and underscores".into());
    }
    Ok(())
}

pub(super) fn require_field_name(name: &str) -> Result<(), String> {
    for segment in name.split('.') {
        require_identifier(segment)?;
    }
    Ok(())
}

pub(super) fn require_query_fields(
    filter: &Option<HashMap<String, FieldFilter>>,
    sort: &Option<Vec<SortSpec>>,
    select: &Option<Vec<String>>,
) -> Result<(), String> {
    for name in filter.iter().flat_map(|fields| fields.keys()) {
        require_field_name(name)?;
    }
    for clause in sort.iter().flatten() {
        require_field_name(&clause.field)?;
    }
    for name in select.iter().flatten() {
        require_field_name(name)?;
    }
    Ok(())
}

pub(super) fn require_record_fields(data: &Value) -> Result<(), String> {
    if let Some(fields) = data.as_object() {
        for name in fields.keys() {
            require_identifier(name)?;
        }
    }
    Ok(())
}

/// Private substrate records are reachable only through their owning command.
/// Match adapter table naming so camelCase aliases cannot bypass the gate.
pub(super) fn require_public_collection(collection: &str) -> Result<(), String> {
    require_identifier(collection)?;
    let table = crate::orm::adapter::naming::to_table_name(collection);
    if let Some(owner) = crate::orm::entity::ProtectedCollection::find(&table) {
        return Err(format!(
            "protected collection: use {}",
            owner.owning_command
        ));
    }
    Ok(())
}

// ============================================================================
// Paginated Query State
// ============================================================================
/// Paginated query state - server-side cursor management
/// Advantage over TypeScript: no IPC per page, just in-memory state
#[derive(Debug)]
pub(super) struct PaginatedQueryState {
    // NOTE: query_id is NOT stored here - it's the DashMap key
    pub(super) db_path: String,
    pub(super) collection: String,
    pub(super) filter: Option<std::collections::HashMap<String, FieldFilter>>,
    pub(super) sort: Option<Vec<crate::orm::query::SortSpec>>,
    pub(super) page_size: usize,
    /// Exact row count from the open-time COUNT(*). Only populated when the
    /// caller passed `count_exact: true`; otherwise 0 means "not requested",
    /// which is the default behavior (QW#2 — skip the full-table scan when
    /// the caller only needs has_more, which is derived from LIMIT N+1).
    pub(super) total_count: u64,
    pub(super) current_page: usize,
    /// Cursor: last ID from previous page for efficient keyset pagination
    pub(super) cursor_id: Option<String>,
    pub(super) has_more: bool,
    /// Creation time for future TTL-based cleanup of stale queries
    #[allow(dead_code)]
    pub(super) created_at: std::time::Instant,
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
    pub(super) adapters: DashMap<String, Arc<dyn StorageAdapter>>,
    /// Mutex only used during adapter initialization (one-time setup)
    pub(super) init_lock: Mutex<()>,
    /// Each resolved backend/collection owns a detachable corpus cell.
    pub(super) vector_cache: crate::runtime::shared_compute::SharedCompute,
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
    pub(super) paginated_queries: DashMap<String, Arc<tokio::sync::Mutex<PaginatedQueryState>>>,
    /// Module context for inter-module communication (event bus, shared compute)
    /// Set during initialize(), used to publish data change events
    pub(super) context: RwLock<Option<Arc<ModuleContext>>>,
    /// Active migration handle for status/pause/verify (lightweight, lock-free)
    pub(super) active_migration: Mutex<Option<MigrationHandle>>,
    /// Pre-cutover connection string (for rollback)
    pub(super) previous_connection: Mutex<Option<String>>,
    /// Limits concurrent query/list operations to cap peak heap usage.
    /// 15 personas firing queries simultaneously can spike RSS by several GB
    /// from concurrent result set materialization + JSON serialization.
    pub(super) query_semaphore: Semaphore,
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

impl Default for DataState {
    fn default() -> Self {
        Self::new()
    }
}

impl DataState {
    pub fn new() -> Self {
        Self {
            adapters: DashMap::new(),
            init_lock: Mutex::new(()),
            vector_cache: crate::runtime::shared_compute::SharedCompute::new(),
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
    pub(super) fn publish_event(&self, collection: &str, action: &str, payload: serde_json::Value) {
        let ctx_guard = self.context.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ctx) = ctx_guard.as_ref() {
            let event_name = format!("data:{}:{}", collection, action);
            ctx.bus.publish_async_only(&event_name, payload);
        }
    }

    /// Log a slow query to the module's dedicated log file.
    /// Only logs if duration exceeds threshold (50ms).
    pub(super) fn log_slow_query(&self, operation: &str, collection: &str, duration_ms: u128) {
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
    /// - `"@node"` — machine-local SQLite ownership state under the native continuum home;
    ///   never follows the shared DATABASE_URL backend.
    /// - 36-char UUID shape — per-persona database (UUID-keyed variant).
    ///   Maps to `$HOME/.continuum/personas/<uuid>/longterm.db`. Kept
    ///   for back-compat with callers using UUIDs as identity.
    /// - Starts with `postgres://` / `postgresql://` / filesystem path —
    ///   legacy passthrough. Logged at WARN so remaining leak sites show
    ///   up in the next audit. Will be removed once every caller migrates.
    ///
    /// This keeps the abstraction enforced at the caller boundary: SQL,
    /// URLs, and filenames simply do not exist in the caller's language.
    /// `$HOME` remains the explicit home override; otherwise the native home
    /// directory is used (including Windows launches without a shell's HOME).
    pub(super) fn resolve_handle(&self, handle: &str) -> Result<String, String> {
        Self::resolve_node_or_backend(handle, crate::paths::continuum_home, ||
            Self::resolve_handle_with(handle, |key| std::env::var(key).ok(), dirs::home_dir))
    }

    pub(super) fn resolve_node_or_backend(
        handle: &str,
        node_home: impl FnOnce() -> Result<PathBuf, String>,
        backend: impl FnOnce() -> Result<String, String>,
    ) -> Result<String, String> {
        if handle == "@node" {
            // Local filesystem ownership never follows a remote DATABASE_URL.
            // The existing node-home policy also provides isolated test homes.
            return node_home()?.join("database/node.db")
                .into_os_string().into_string()
                .map_err(|_| "node database path is not valid UTF-8".to_string());
        }
        backend()
    }

    /// Keep environment access at the resolution boundary so all local handle
    /// families share one home policy, independently of the launching shell.
    pub(super) fn resolve_handle_with(
        handle: &str,
        env: impl Fn(&str) -> Option<String>,
        native_home: impl FnOnce() -> Option<PathBuf>,
    ) -> Result<String, String> {
        // Lazy: configured backends and legacy paths do not need a home lookup.
        let resolve_home = || {
            if let Some(home) = env("HOME") {
                return Ok(home);
            }
            native_home()
                .ok_or_else(|| format!("resolve_handle('{handle}'): home directory unavailable"))?
                .into_os_string()
                .into_string()
                .map_err(|_| {
                    format!("resolve_handle('{handle}'): home directory is not valid UTF-8")
                })
        };

        // Main DB sentinel — honors DATABASE_URL env, falls back to SQLite.
        if handle == "main" {
            if let Some(url) = env("DATABASE_URL") {
                if !url.is_empty() {
                    return Ok(url);
                }
            }
            let home = resolve_home()?;
            return Ok(format!("{}/.continuum/database/main.db", home));
        }

        // Per-CITIZEN slug-shape sentinel: @persona:<slug> / @agent:<slug> /
        // @human:<slug>. Slug matches the on-disk dir under
        // $HOME/.continuum/<bucket>/. First-class citizenship (Joel 2026-07-25):
        // an external coding agent and a human get the SAME per-citizen
        // longterm store a persona has, so the `/continuum:memory` skill gives
        // an agent durable, own-dir memory (the amnesia fix). One resolution
        // rule, three buckets. Mac Option B fix — the sentinel lets each side
        // resolve to its own filesystem view of the shared `~/.continuum` mount.
        for (sentinel, bucket) in [
            ("@persona:", "personas"),
            ("@agent:", "agents"),
            ("@human:", "humans"),
        ] {
            if let Some(slug) = handle.strip_prefix(sentinel) {
                if slug.is_empty() {
                    return Err(format!("resolve_handle('{sentinel}'): empty slug"));
                }
                // Defensive: slug must be a single path segment — no escapes.
                if slug.contains('/') || slug.contains('\\') || slug.contains("..") {
                    return Err(format!(
                        "resolve_handle('{sentinel}{slug}'): slug must be a single path segment"
                    ));
                }
                let home = resolve_home()?;
                return Ok(format!(
                    "{home}/.continuum/{bucket}/{slug}/data/longterm.db"
                ));
            }
        }

        // Telemetry SQLite sentinel.
        if handle == "@metrics" {
            let home = resolve_home()?;
            return Ok(format!("{}/.continuum/metrics/metrics.sqlite", home));
        }

        // Per-persona UUID shape: 8-4-4-4-12 hex chars with hyphens (36 total).
        // Safe to check without crate parsing — the shape is unambiguous.
        if is_uuid_shape(handle) {
            let home = resolve_home()?;
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


}

/// Check if a string matches the 36-char UUID shape `8-4-4-4-12` hex.
/// Intentionally simple — avoids pulling uuid crate just for a shape check.
/// `pub(crate)` so `commands/memory` derives the SAME per-persona handle
/// mapping `resolve_handle` uses (one shape check, one place).
pub(crate) fn is_uuid_shape(s: &str) -> bool {
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

/// Register the substrate's Rust-authored ORM entities into the global registry.
/// Idempotent; returns how many collections the registry resolves afterwards.
pub(crate) fn wire_substrate_orm_entities() -> Result<usize, String> {
    let registry = crate::orm::OrmEntityRegistry::global();
    crate::persona::register_substrate_orm_entities(registry)
        .map_err(|e| format!("substrate ORM entity registration conflict — boot refused: {e}"))?;
    Ok(crate::persona::SUBSTRATE_ORM_COLLECTIONS
        .iter()
        .filter(|c| registry.resolve(c).is_some())
        .count())
}

#[async_trait]
impl ServiceModule for DataModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "data",
            priority: ModulePriority::Normal,
            // `vector/*` fully migrated to typed self-routing commands
            // (commands/vector/*.rs) — no legacy arm remains, so the prefix is gone.
            command_prefixes: &["data/", "adapter/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // THE SUBSTRATE'S OWN ENTITIES (2026-09-14): `register_substrate_orm_entities`
        // existed with a doc comment saying "boot wires it" and NO caller — so every
        // `data/ensure-schema` for a Rust-authored collection (`staged_credit`, the
        // learning flywheel's per-persona credit) was refused as an unknown
        // collection, on every node, and the flywheel stayed dead for work turns
        // (training.credit.stage_failed ×2 per act turn, 0 staged). A conflict is a
        // boot failure, never a warning: two shapes for one collection is a bug.
        let registered = wire_substrate_orm_entities()?;
        continuum_base::probe!(
            class = "orm.substrate_entities.registered",
            registered = registered as u64,
            "Rust-authored ORM entities registered before the first ensure-schema"
        );
        // Store context for event publishing
        let ctx_arc = Arc::new(ModuleContext::new(
            ctx.registry.clone(),
            ctx.bus.clone(),
            ctx.compute.clone(),
            ctx.runtime.clone(),
        ));
        *self
            .state
            .context
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(ctx_arc);
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
        // DataModule owns the adapter pool, so it contributes the `data/*`
        // content commands, the `vector/*` embedding-search commands, the
        // `adapter/*` introspection commands, and the `migration/*` operator
        // commands — each family sharing this module's `Arc<DataState>`.
        let mut objects = crate::commands::data::command_objects(self.state.clone());
        objects.extend(crate::commands::vector::command_objects(self.state.clone()));
        objects.extend(crate::commands::adapter::command_objects(
            self.state.clone(),
        ));
        objects.extend(crate::commands::migration::command_objects(
            self.state.clone(),
        ));
        objects
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
pub(super) struct QueryParams {
    pub(super) db_path: String,
    pub(super) collection: String,
    #[serde(default)]
    pub(super) filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    pub(super) sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) offset: Option<usize>,
    #[serde(default)]
    pub(super) select: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryWithJoinParams {
    pub(super) db_path: String,
    pub(super) collection: String,
    #[serde(default)]
    pub(super) filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    pub(super) sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) offset: Option<usize>,
    #[serde(default)]
    pub(super) joins: Option<Vec<crate::orm::query::JoinSpec>>,
    #[serde(default)]
    pub(super) select: Option<Vec<String>>,
}

// Vector command params (collection/search/index/backfill/stats) moved to their
// typed command files under `commands/vector/*.rs`; the DataState methods they
// call take plain args (handle, collection, …). The `default_k`/`default_true`/
// `default_batch_size` serde helpers moved with them.

// ============================================================================
// Paginated Query Params
// ============================================================================
pub(super) fn default_page_size() -> usize {
    100
}

/// Open paginated query params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryOpenParams {
    pub(super) db_path: String,
    pub(super) collection: String,
    #[serde(default)]
    pub(super) filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    pub(super) sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default = "default_page_size")]
    pub(super) page_size: usize,
    /// Opt in to a SELECT COUNT(*) at open time so callers that show
    /// "X of N" displays get an exact total. Default false: skip the scan
    /// (a ~1M-row chat_messages table no longer pays a full-table cost on
    /// every scrollback open). When false the response carries
    /// `totalCount: 0` as the "not requested" sentinel — `hasMore` is
    /// always authoritative regardless and is derived from the LIMIT N+1
    /// probe in `handle_query_next`.
    #[serde(default)]
    pub(super) count_exact: bool,
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
pub(super) struct QueryNextParams {
    #[serde(default)]
    pub(super) query_id: Option<String>,
}

/// Close query params. Same dual-shape contract as
/// [`QueryNextParams`] — see its docs for the legacy/envelope handoff.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryCloseParams {
    #[serde(default)]
    pub(super) query_id: Option<String>,
}

/// The canonical type tag for cursor handles minted by `data/query-open`.
/// Lives here so cross-module callers can match on it without depending
/// on string magic.
pub(super) const QUERY_CURSOR_TYPE_TAG: &str = "data::QueryCursor";

/// The canonical owner string for handles this module mints. Matches
/// the module's `name` in `ModuleConfig`. Centralized so a future rename
/// of the module name is a single edit.
pub(super) const DATA_MODULE_OWNER: &str = "data";

/// Response payload shape for `data/query-open`. Lives in a typed struct
/// so the typed envelope can flatten it cleanly — the legacy wire shape
/// nests every field under a `data:` key, so we preserve that here.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryOpenResponseShape {
    /// Nested for back-compat with the pre-envelope wire shape that
    /// TS consumers currently parse as `response.data.queryId`. New
    /// consumers should read the kernel-level `handle` instead.
    pub(super) data: QueryOpenInner,
}

/// Inner payload — the historical fields the cursor returns at open
/// time. `query_id` stays for back-compat (it's the same UUID stringly
/// rendered as the `handle.id`); new consumers thread the handle.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueryOpenInner {
    pub(super) query_id: String,
    pub(super) collection: String,
    pub(super) total_count: u64,
    pub(super) page_size: usize,
    pub(super) has_more: bool,
}
