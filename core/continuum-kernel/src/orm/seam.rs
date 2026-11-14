//! The storage seam's vocabulary: what a `StorageAdapter` call names, reads, changes and returns.
//!
//! Joel, 2026-11-14: "Firm api boundaries and optional only if it truly is. Structs to group
//! parameters and avoid copy when ref. Think of mutability. Define this at function and schemas."
//! So at this seam:
//! - what travels together is one struct (`RowRef`, `FieldRef`, `Update`, `BinaryWrite`);
//! - a read borrows what it reads (`&StorageQuery`, `RowRef`); a write owns what it writes (records,
//!   data, batches, payloads), because the committing task outlives the caller and moves them into
//!   the statement, so a borrow would only force a copy; a schema is shared (`Arc`); only `close`
//!   takes `&mut self`;
//! - an outcome is a named state (`Deleted`, `Written`, `BinaryWritten`, `Versioning`), never a
//!   `bool`; a failure is a [`StorageError`](super::storage_error::StorageError) variant.
//!
//! Phase 2 (owner: the ORM, card 370dde3e's sibling for storage): `Collection` and `RowId(Uuid)`
//! newtypes replace the `&str` collection and `String` id, through `DataRecord` and the schemas.

use std::collections::BTreeMap;

use super::binary::{BinaryValue, BinaryWriteCondition};
use super::types::UUID;

/// One row: its collection and id, which every row operation names together.
#[derive(Debug, Clone, Copy)]
pub struct RowRef<'a> {
    pub collection: &'a str,
    pub id: &'a UUID,
}

/// One declared field of a collection.
#[derive(Debug, Clone, Copy)]
pub struct FieldRef<'a> {
    pub collection: &'a str,
    pub field: &'a str,
}

/// Whether an update moves the row's version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Versioning {
    Increment,
    Keep,
}

/// A change to one row's declared fields; the data is owned because the write consumes it.
#[derive(Debug, Clone)]
pub struct Update<'a> {
    pub row: RowRef<'a>,
    pub data: serde_json::Value,
    pub versioning: Versioning,
}

/// What a delete of one row found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deleted {
    Removed,
    Absent,
}

/// What a monotonic upsert did: wrote the record, or kept a row whose field was already greater.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    Written,
    Kept,
}

/// Native payloads for one row's declared binary fields, under a write condition. The payloads
/// are owned: the write moves their bytes into the statement, so borrowing would copy them.
#[derive(Debug)]
pub struct BinaryWrite<'a> {
    pub row: RowRef<'a>,
    pub payloads: BTreeMap<String, BinaryValue>,
    pub condition: BinaryWriteCondition,
}

/// What a binary write did under its condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryWritten {
    Written,
    Unchanged,
}
