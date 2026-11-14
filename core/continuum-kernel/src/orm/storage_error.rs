//! Why a storage operation failed, as a value a caller matches (RTOS rule 11: types, not strings).
//!
//! Every `StorageAdapter` method returns `Result<T, StorageError>`. It replaces the Node-era receipt
//! `StorageResult { success, data, error: Option<String> }` on the in-process path, which every
//! caller flattened to `Result<T, String>` (76 `.into_result()` calls, 85 `.map_err(|e| e.to_string())`
//! in the relational layer on 2026-11-14), so no caller could tell a missing row from a dead database.
//! The schema, binary and database failures keep their own typed errors; this composes them.

use super::binary::BinaryDescriptorError;
use super::schema_error::SchemaError;
use super::types::UUID;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The database refused or failed the statement.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    /// The collection's declaration and its physical table disagree, or a declaration is missing.
    #[error(transparent)]
    Schema(#[from] SchemaError),
    /// A native payload or its descriptor is not a valid value.
    #[error(transparent)]
    Binary(#[from] BinaryDescriptorError),
    /// A stored value did not decode to its declared type.
    #[error(transparent)]
    Codec(#[from] serde_json::Error),
    /// No entity schema has been admitted for this collection.
    #[error("collection {collection} has no admitted entity schema")]
    Unadmitted { collection: String },
    /// Persisted state the store cannot read back as written (a contract of the store, broken).
    #[error("persisted storage state is unreadable: {what}")]
    Corrupt { what: &'static str },
    /// The database's directory could not be opened.
    #[error("database directory {path}: {source}")]
    Io { path: std::path::PathBuf, #[source] source: std::io::Error },
    /// The adapter's configuration cannot open a store.
    #[error("storage configuration: {0}")]
    Config(ConfigFault),
    /// A native row's id is not a canonical lowercase hyphenated UUID (page cursors order by its text).
    #[error("{id} is not a canonical lowercase hyphenated UUID")]
    NotCanonicalId { id: String },
    /// No row with this id.
    #[error("{collection} has no row {id}")]
    NotFound { collection: String, id: UUID },
    /// The write's record is not the shape its collection declares.
    #[error("{collection}: the record is not {expected}")]
    Malformed { collection: String, expected: Shape },
    /// A declared field the write left out, with no default and not nullable.
    #[error("{collection}.{field} is required")]
    Required { collection: String, field: String },
    /// A field written as a native payload that is not declared binary.
    #[error("{collection}.{field} is not a declared binary field")]
    NotBinary { collection: String, field: String },
    /// A binary field supplied both as JSON and as a native payload.
    #[error("{collection}.{field} was supplied through two representations")]
    TwoRepresentations { collection: String, field: String },
    /// The statement could not be built (a column/value mismatch in the builder).
    #[error(transparent)]
    Query(#[from] sea_orm::sea_query::error::Error),
    /// A field the operation names is not declared on the collection.
    #[error("{collection}.{field} is not declared")]
    Undeclared { collection: String, field: String },
    /// A collection's sequence cannot advance (exhausted, or its persisted head is invalid).
    #[error("{collection}: the sequence cannot advance")]
    SequenceExhausted { collection: String },
    /// A count or size does not fit the integer the database or this process uses.
    #[error("{what} exceeds its integer range")]
    Overflow { what: &'static str },
    /// This adapter does not provide the operation (a capability, not a fault).
    #[error("{adapter} does not provide {operation}")]
    Unsupported { adapter: &'static str, operation: &'static str },
}

/// The shape a write needed and did not get.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A JSON object of the declared fields.
    Object,
    /// Any data at all (a write that carried none).
    Present,
}

impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Shape::Object => "an object of its declared fields",
            Shape::Present => "present",
        })
    }
}

/// Why a configuration cannot open a store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConfigFault {
    #[error("a connection pool needs a positive capacity")]
    NoCapacity,
    #[error("SQLite namespaces need separate database owners")]
    Namespace,
    #[error("the connection count exceeds u32")]
    CapacityRange,
}
