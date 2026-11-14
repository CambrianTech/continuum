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
    /// No row with this id.
    #[error("{collection} has no row {id}")]
    NotFound { collection: String, id: UUID },
    /// The write's record is not the shape its collection declares.
    #[error("{collection}: the record is not {expected}")]
    Malformed { collection: String, expected: Shape },
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
