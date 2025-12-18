/// Database Adapter Trait - Universal interface for all data sources
///
/// All adapters (SQLite, Postgres, MySQL, REST, GraphQL) implement this trait.
/// This allows the worker to route queries to different backends without knowing
/// the implementation details.

use serde_json::Value;

// ============================================================================
// Core Trait - All Adapters Must Implement This
// ============================================================================

/// DatabaseAdapter trait - the contract all adapters must fulfill
pub trait DatabaseAdapter: Send + Sync {
    /// Execute a query (SELECT) and return rows
    fn execute_query(
        &self,
        sql: String,
        params: Vec<Value>,
    ) -> Result<Vec<Value>, String>;

    /// Execute a statement (INSERT/UPDATE/DELETE) and return changes
    fn execute_statement(
        &self,
        sql: String,
        params: Vec<Value>,
    ) -> Result<ExecuteResult, String>;

    /// Get adapter type (for debugging/logging)
    fn adapter_type(&self) -> &str;

    /// Health check - verify connection is alive
    fn ping(&self) -> Result<(), String>;
}

/// Result from execute_statement (INSERT/UPDATE/DELETE)
#[derive(Debug, Clone)]
pub struct ExecuteResult {
    pub changes: usize,
    pub last_insert_id: Option<i64>,
}

// ============================================================================
// Adapter Registry - Routes queries to correct adapter
// ============================================================================

use std::collections::HashMap;
use std::sync::Arc;

/// AdapterRegistry manages multiple database adapters
pub struct AdapterRegistry {
    adapters: HashMap<String, Arc<dyn DatabaseAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Register an adapter with a handle name
    pub fn register(&mut self, handle: String, adapter: Arc<dyn DatabaseAdapter>) {
        self.adapters.insert(handle, adapter);
    }

    /// Get adapter by handle (or default)
    pub fn get(&self, handle: Option<&str>) -> Result<Arc<dyn DatabaseAdapter>, String> {
        let handle = handle.unwrap_or("default");
        self.adapters
            .get(handle)
            .cloned()
            .ok_or_else(|| format!("No adapter registered for handle: {}", handle))
    }

    /// List all registered handles
    pub fn list_handles(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }
}

// ============================================================================
// Example Usage (commented out - shows how adapters implement the trait)
// ============================================================================

/*
// SQLite Adapter
pub struct SqliteAdapter {
    pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

impl DatabaseAdapter for SqliteAdapter {
    fn execute_query(&self, sql: String, params: Vec<Value>) -> Result<Vec<Value>, String> {
        // SQLite-specific implementation using rusqlite
    }

    fn execute_statement(&self, sql: String, params: Vec<Value>) -> Result<ExecuteResult, String> {
        // SQLite-specific implementation
    }

    fn adapter_type(&self) -> &str {
        "sqlite"
    }

    fn ping(&self) -> Result<(), String> {
        // Check if connection pool is healthy
    }
}

// Postgres Adapter
pub struct PostgresAdapter {
    pool: deadpool_postgres::Pool,
}

impl DatabaseAdapter for PostgresAdapter {
    fn execute_query(&self, sql: String, params: Vec<Value>) -> Result<Vec<Value>, String> {
        // Postgres-specific implementation using tokio-postgres
    }

    fn execute_statement(&self, sql: String, params: Vec<Value>) -> Result<ExecuteResult, String> {
        // Postgres-specific implementation
    }

    fn adapter_type(&self) -> &str {
        "postgres"
    }

    fn ping(&self) -> Result<(), String> {
        // Check if Postgres connection is alive
    }
}

// REST API Adapter
pub struct RestAdapter {
    base_url: String,
    client: reqwest::Client,
}

impl DatabaseAdapter for RestAdapter {
    fn execute_query(&self, sql: String, params: Vec<Value>) -> Result<Vec<Value>, String> {
        // Interpret "sql" as REST endpoint
        // "GET /users?status=active" → HTTP GET request
        // Returns JSON response as rows
    }

    fn execute_statement(&self, sql: String, params: Vec<Value>) -> Result<ExecuteResult, String> {
        // "POST /users" → HTTP POST request
        // Returns { changes: 1 }
    }

    fn adapter_type(&self) -> &str {
        "rest"
    }

    fn ping(&self) -> Result<(), String> {
        // HTTP GET to health endpoint
    }
}

// Usage in worker
let mut registry = AdapterRegistry::new();

// Register SQLite adapter
let sqlite = Arc::new(SqliteAdapter::new(".continuum/jtag/data/database.sqlite")?);
registry.register("default".to_string(), sqlite.clone());
registry.register("main".to_string(), sqlite);

// Register Postgres adapter
let postgres = Arc::new(PostgresAdapter::new("postgres://localhost/continuum")?);
registry.register("postgres-main".to_string(), postgres);

// Register REST API adapter
let github = Arc::new(RestAdapter::new("https://api.github.com")?);
registry.register("api-github".to_string(), github);

// Route queries based on handle
let adapter = registry.get(Some("postgres-main"))?;
let rows = adapter.execute_query("SELECT * FROM users".to_string(), vec![])?;
*/
