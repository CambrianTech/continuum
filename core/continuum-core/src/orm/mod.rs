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
pub mod entity;
pub mod migration;
pub mod postgres;
pub mod query;
pub mod sqlite;
pub mod store;
pub mod types;
pub mod vector;

#[cfg(test)]
mod derive_test;

pub use adapter::StorageAdapter;
pub use connection_manager::{ConnectionManager, ConnectionManagerConfig};
pub use entity::{
    base_entity_fields, is_base_entity_column, BaseEntity, OrmEntity, OrmEntityRegistry,
    RegistrationError,
};

// Re-export the derive macro at orm-module level so callers write
// `use continuum_core::orm::Entity;` consistently with the rest of
// the ORM surface, never reaching for the proc-macro crate by name.
// Per the [[organization-purity-as-we-migrate]] + E=mc² doctrine: one
// import path for the whole ORM, derive macros included.
pub use continuum_orm_derive::Entity;
pub use migration::{MigrationEngine, MigrationHandle};
pub use postgres::PostgresAdapter;
pub use query::{QueryBuilder, QueryOperator, SortDirection, StorageQuery};
pub use sqlite::SqliteAdapter;
pub use store::{OrmStore, OrmStoreError};
pub use types::{
    CascadeRule, CollectionSchema, DataRecord, FieldType, ForeignKeyRef, RecordMetadata,
    SchemaField, SchemaIndex, StorageResult,
};
pub use vector::{
    BackfillVectorsProgress, BackfillVectorsRequest, EmbeddingModel, GenerateEmbeddingRequest,
    GenerateEmbeddingResponse, IndexVectorRequest, VectorEmbedding, VectorIndexStats,
    VectorSearchAdapter, VectorSearchOptions, VectorSearchResponse, VectorSearchResult,
};
