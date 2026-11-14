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
//!         ├── SeaOrmAdapter
//!         ├── MysqlAdapter (future)
//!         └── etc.
//! ```
//!
//! Key design principles:
//! - Database-agnostic: All adapters implement the same trait
//! - No SQL in business logic: Adapters translate queries to native format
//! - camelCase ↔ snake_case: Automatic field name conversion
//! - JSON hydration: Automatically parse JSON fields

pub mod list_result;
pub mod adapter;
pub mod connection_manager;
pub mod entity;
pub mod migration;
pub mod query;
pub mod record_decode;
mod relational_schema;
mod relational_declaration;
mod relational_structure;
mod relational_evolution;
#[cfg(test)]
mod relational_derived_tests;
pub mod schema_error;
mod relational_query;
mod relational_codec;
mod relational;
mod relational_join;
mod relational_write;
mod relational_sequence;
mod relational_native_revision;
mod relational_delete_graph;
mod relational_batch;
mod relational_transaction;
mod relational_adapter;
mod relational_maintenance;
pub mod store;
pub mod types;
pub mod vector;

pub(crate) mod gated_adapter; // test-only: the file is `#![cfg(test)]`

pub use adapter::StorageAdapter;
pub use connection_manager::{ConnectionManager, ConnectionManagerConfig};
pub use entity::{
    base_entity_fields, is_base_entity_column, BaseEntity, OrmEntity, OrmEntityRegistry,
    RegistrationError,
};

// Re-export the derive macro at orm-module level so callers write
// `use continuum_core::orm::Entity;` consistently with the rest of
// the ORM surface, never reaching for the proc-macro crate by name.
// Per the [[organization-purity-as-we-migrate]] + E=mcÂ² doctrine: one
// import path for the whole ORM, derive macros included.
pub use continuum_orm_derive::Entity;
pub use migration::{MigrationEngine, MigrationHandle};
pub use query::{QueryBuilder, QueryOperator, SortDirection, StorageQuery};
pub use relational_adapter::SeaOrmAdapter;
pub use store::{commit_appending, OrmStore, OrmStoreError};
pub use types::{
    CascadeRule, CollectionSchema, DataRecord, FieldType, ForeignKeyRef, RecordMetadata,
    SchemaField, SchemaIndex, SequenceSpec, Sequenced, StorageResult,
};
pub use vector::{
    BackfillVectorsProgress, BackfillVectorsRequest, EmbeddingModel, GenerateEmbeddingRequest,
    GenerateEmbeddingResponse, IndexVectorRequest, VectorEmbedding, VectorIndexStats,
    VectorSearchAdapter, VectorSearchOptions, VectorSearchResponse, VectorSearchResult,
};
pub mod client_read;

mod relational_array_stats;

pub mod binary;
mod relational_binary;
mod binary_snapshot;
mod migration_snapshot;
pub mod binary_delta;
pub use binary_snapshot::BinarySnapshot;

#[cfg(test)]
mod tests;
