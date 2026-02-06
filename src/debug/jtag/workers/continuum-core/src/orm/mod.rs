//! Rust ORM Module - Database-agnostic storage abstraction
//!
//! Architecture:
//! ```text
//! TypeScript (thin portability layer)
//!     ↓ single IPC call
//! Rust continuum-core
//!     ├── OrmModule (entity logic, query building)
//!     │       ↓ trait calls (no IPC)
//!     └── StorageAdapter trait implementations
//!         ├── SqliteAdapter
//!         ├── PostgresAdapter (future)
//!         ├── MysqlAdapter (future)
//!         └── etc.
//! ```
//!
//! Key design principles:
//! - Database-agnostic: All adapters implement the same trait
//! - No SQL in business logic: Adapters translate queries to native format
//! - camelCase ↔ snake_case: Automatic field name conversion
//! - JSON hydration: Automatically parse JSON fields

pub mod adapter;
pub mod connection_manager;
pub mod query;
pub mod sqlite;
pub mod types;
pub mod vector;

pub use adapter::StorageAdapter;
pub use connection_manager::{ConnectionManager, ConnectionManagerConfig};
pub use query::{QueryBuilder, StorageQuery, QueryOperator, SortDirection};
pub use sqlite::SqliteAdapter;
pub use types::{DataRecord, RecordMetadata, StorageResult, CollectionSchema, SchemaField, FieldType};
pub use vector::{
    VectorSearchAdapter, VectorSearchOptions, VectorSearchResponse, VectorSearchResult,
    VectorEmbedding, EmbeddingModel, GenerateEmbeddingRequest, GenerateEmbeddingResponse,
    IndexVectorRequest, BackfillVectorsRequest, BackfillVectorsProgress, VectorIndexStats,
};
