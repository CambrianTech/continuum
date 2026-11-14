//! The vector seam's failures, typed end to end; text appears only at the command boundary.
use super::{RecordId, UnitVectorError};
use crate::orm::binary::{ByteOrder, ElementType};
use crate::sdk_codegen::CommandError;

/// Why a payload cannot enter, or query, the resident matrix.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub(crate) enum VectorIndexError {
    /// The producer did not certify L2 normalization; search never normalizes for it.
    #[error("vector from {producer} is not certified unit-length; its producer must normalize at write")]
    NotUnit { producer: String },
    /// Only native f32 rows are resident today (F16 joins with the Metal kernel).
    #[error("vector element type {0:?} is not resident-searchable (f32 only)")]
    Element(ElementType),
    /// Rows are copied as raw bytes, so they must already be in the host's byte order.
    #[error("vector byte order {0:?} is not the host's")]
    ByteOrder(ByteOrder),
    /// A vector is rank one: shape `[dimension]`.
    #[error("vector must be rank one, found rank {0}")]
    Rank(usize),
    /// Every row and every query in one matrix share one dimension.
    #[error("vector dimension {found} does not match the matrix dimension {expected}")]
    Dimension { expected: usize, found: usize },
    /// A page's byte count disagrees with its row count and descriptor.
    #[error("vector page holds {found} bytes, its rows need {expected}")]
    PageLength { expected: usize, found: usize },
    /// The declared dimension does not fit this host's address space.
    #[error("vector dimension {0} exceeds this host's address space")]
    DimensionOverflow(u64),
    /// A page's rows times its dimension does not fit this host's address space.
    #[error("a page of {rows} rows of dimension {dimension} exceeds this host's address space")]
    PageOverflow { rows: usize, dimension: usize },
}

/// A vector command's failure at the store seam.
#[derive(Debug, thiserror::Error)]
pub(crate) enum VectorStoreError {
    #[error(transparent)]
    Index(#[from] VectorIndexError),
    #[error(transparent)]
    Unit(#[from] UnitVectorError),
    /// The collection policy refused the collection (a protected or malformed name).
    #[error("{0}")]
    Refused(String),
    /// The storage owner's interface reports its faults as text; they stay its own.
    #[error("vector storage: {0}")]
    Store(String),
    /// The store's page size is zero, so no page could ever be read.
    #[error("the store's page size is zero")]
    PageSize,
    /// A ranked row has no identity in the matrix that ranked it.
    #[error("ranked row {0} has no identity in its matrix")]
    RowIdentity(usize),
    /// The collection's cache cell holds a value of another type.
    #[error("vector cache cell {scope}/{key} holds another type")]
    CacheType { scope: String, key: String },
    /// A ranked record disappeared before its hydration read.
    #[error("vector result {id}: {fault}")]
    Hydration { id: RecordId, fault: String },
}

/// A vector the caller supplied is invalid input; a store or matrix fault is the node's.
impl From<VectorStoreError> for CommandError {
    fn from(error: VectorStoreError) -> Self {
        match error {
            VectorStoreError::Unit(_)
            | VectorStoreError::Refused(_)
            | VectorStoreError::Index(VectorIndexError::Dimension { .. }) => {
                CommandError::Invalid(error.to_string())
            }
            VectorStoreError::Index(_)
            | VectorStoreError::Store(_)
            | VectorStoreError::RowIdentity(_)
            | VectorStoreError::PageSize
            | VectorStoreError::CacheType { .. }
            | VectorStoreError::Hydration { .. } => CommandError::Internal(error.to_string()),
        }
    }
}
