//! Typed refusals. Every failure is a variant a caller can match; none is a default value.

use std::io;
use std::path::PathBuf;

use crate::id::ContentId;
use crate::requires::Requirement;
use crate::store::RefName;

/// What a store refused, and why.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{op} {path}: {source}")]
    Io {
        op: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    #[error("{op}: {source}")]
    Stream { op: &'static str, source: io::Error },
    #[error("no blob {0}")]
    NotFound(ContentId),
    #[error("bytes read as {expected} hash to {actual}")]
    DigestMismatch {
        expected: ContentId,
        actual: ContentId,
    },
    #[error("range {start}+{len} is outside blob {id} of {size} bytes")]
    RangeOutOfBounds {
        id: ContentId,
        start: u64,
        len: u64,
        size: u64,
    },
    #[error("ref {name} is {found:?}, expected {expected:?}")]
    RefConflict {
        name: RefName,
        expected: Option<ContentId>,
        found: Option<ContentId>,
    },
    #[error("ref {name} is unreadable: {reason}")]
    RefCorrupt { name: RefName, reason: String },
    #[error("invalid ref segment {0:?}: use [A-Za-z0-9._-], not starting with '.'")]
    InvalidRefName(String),
    #[error("store full: {needed} more bytes over a capacity of {capacity}")]
    Full { capacity: u64, needed: u64 },
    #[error("store state poisoned by a panicked writer")]
    Poisoned,
    /// Produced only by the `conformance` feature's `FaultStore`.
    #[error("injected fault at operation {at} ({op})")]
    Injected { op: &'static str, at: u64 },
}

/// What packing or loading refused, and why.
#[derive(Debug, thiserror::Error)]
pub enum PackError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("manifest is malformed: {0}")]
    Manifest(serde_json::Error),
    #[error("manifest has no RFC 8785 form: {0}")]
    Canonical(#[from] crate::canonical::CanonicalError),
    #[error("manifest envelope format {found} is newer than {known}")]
    UnknownFormat { found: u32, known: u32 },
    #[error("expected a {expected} package, found {found}")]
    KindMismatch {
        expected: &'static str,
        found: String,
    },
    #[error("{kind} schema {found} is newer than this build knows ({known})")]
    UnknownNewerSchema {
        kind: &'static str,
        found: u32,
        known: u32,
    },
    #[error("{kind} has no migration from schema {from}")]
    MissingMigration { kind: &'static str, from: u32 },
    #[error("{kind} migration from schema {from} failed: {reason}")]
    Migration {
        kind: &'static str,
        from: u32,
        reason: String,
    },
    #[error("requirement {part} is not met here: {requirement:?}")]
    UnmetRequirement {
        part: String,
        requirement: Requirement,
    },
    #[error("requirement {part} has the wrong shape: {found:?}")]
    RequirementShape { part: String, found: Requirement },
    #[error("part {part} is missing")]
    MissingPart { part: String },
    #[error("part {part} is not a {expected}")]
    WrongPart {
        part: String,
        expected: &'static str,
    },
    #[error("blob {part} is {found}, expected {expected}")]
    WrongMedia {
        part: String,
        expected: String,
        found: String,
    },
    #[error("part {0} is named twice")]
    DuplicatePart(String),
    #[error("field {part}: {source}")]
    Field {
        part: String,
        source: serde_json::Error,
    },
    #[error("ref {0} does not exist")]
    RefAbsent(RefName),
    #[error(
        "secret part {part}: sealing to an audience lands in slice 5; refusing to store plaintext"
    )]
    SealingUnavailable { part: String },
}
