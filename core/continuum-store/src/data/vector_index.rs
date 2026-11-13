//! The vector consumer on the native storage seam (CONTINUUM-ENGINE.md section 6).
//!
//! Contract (PR #5954, agreed with the storage owner):
//! - the producer normalizes ONCE at write and declares `unit = true` ([`unit_f32`]);
//! - search refuses a `unit = false` payload with a typed error and never normalizes a row,
//!   so every score is one dot product: no sqrt, no norms;
//! - one resident row-major matrix per collection, filled by appending native pages;
//! - a sign-bit sketch per row feeds a popcount prefilter whose candidate count is
//!   MEASURED against a recall requirement at build time ([`RecallTarget`]);
//! - exact dot on the survivors, top-k by a bounded heap.
//!
//! [`ResidentMatrix`] is the compute seam. [`CpuMatrix`] is its first implementation; the
//! Metal GEMV + fused top-k implements the same trait over the same contiguous buffer.

mod calibrate;
mod cpu;
mod unit;

#[cfg(test)]
mod tests;

use crate::orm::binary::{BinaryDescriptor, BinaryValue, ByteOrder, ElementType};
use std::fmt;
use std::num::NonZeroUsize;

pub(crate) use cpu::CpuMatrix;
pub(crate) use unit::unit_f32;

/// The byte order rows are copied in without decoding; any other order is refused.
pub(crate) const NATIVE_ORDER: ByteOrder =
    if cfg!(target_endian = "little") { ByteOrder::Little } else { ByteOrder::Big };

/// Why a payload cannot enter, or query, the resident matrix.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum VectorIndexError {
    /// The producer did not certify L2 normalization; search never normalizes for it.
    NotUnit { producer: String },
    /// Only native f32 rows are resident today (F16 joins with the Metal kernel).
    Element(ElementType),
    /// Rows are copied as raw bytes, so they must already be in the host's byte order.
    ByteOrder(ByteOrder),
    /// A vector is rank one: shape `[dimension]`.
    Rank(usize),
    /// Every row and every query in one matrix share one dimension.
    Dimension { expected: usize, found: usize },
    /// A bulk page's byte count disagrees with its row count and descriptor.
    PageLength { expected: usize, found: usize },
    /// The declared dimension does not fit this host's address space.
    DimensionOverflow(u64),
    /// A page's rows times its dimension does not fit this host's address space.
    PageOverflow { rows: usize, dimension: usize },
}

impl fmt::Display for VectorIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUnit { producer } => write!(f, "vector from {producer} is not certified unit-length; its producer must normalize at write"),
            Self::Element(element) => write!(f, "vector element type {element:?} is not resident-searchable (f32 only)"),
            Self::ByteOrder(order) => write!(f, "vector byte order {order:?} is not the host's"),
            Self::Rank(rank) => write!(f, "vector must be rank one, found rank {rank}"),
            Self::Dimension { expected, found } => write!(f, "vector dimension {found} does not match the matrix dimension {expected}"),
            Self::PageLength { expected, found } => write!(f, "vector page holds {found} bytes, its rows need {expected}"),
            Self::DimensionOverflow(dimension) => write!(f, "vector dimension {dimension} exceeds this host's address space"),
            Self::PageOverflow { rows, dimension } => write!(f, "a page of {rows} rows of dimension {dimension} exceeds this host's address space"),
        }
    }
}

impl std::error::Error for VectorIndexError {}

/// The fraction of calibration queries whose COMPLETE top-k must survive the prefilter.
/// A requirement, not a tuning knob: the candidate count is measured to meet it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RecallTarget(f32);

impl RecallTarget {
    /// The store's requirement for recall: a memory a mind should recall and the
    /// prefilter drops is a defect, so 99 of 100 queries keep their whole top-k.
    pub(crate) const STORE: Self = Self(0.99);

    pub(crate) fn fraction(self) -> f32 {
        self.0
    }
}

/// The compute seam every backend implements over the same row-major unit matrix.
pub(crate) trait ResidentMatrix: Send + Sync {
    /// Appends `ids.len()` rows laid out row-major in `rows`, all described by `descriptor`
    /// (the per-row shape `[dimension]`). This is the consumer of the storage owner's bulk
    /// `binary_page_into(collection, field, after, limit, &mut rows, &mut ids)`; until it
    /// lands, each `binary_page` record arrives as a one-row page.
    fn append_rows(&mut self, descriptor: &BinaryDescriptor, rows: &[u8], ids: Vec<String>) -> Result<(), VectorIndexError>;

    /// Measures how many prefilter candidates meet `target` on this corpus.
    fn calibrate(&mut self, target: RecallTarget);

    /// The `k` best rows by dot product, best first, as (row, score).
    fn search(&self, query: &BinaryValue, k: NonZeroUsize) -> Result<Vec<(usize, f32)>, VectorIndexError>;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn id(&self, row: usize) -> Option<&str>;
}

/// The rank-one dimension a descriptor declares, after the searchable-row checks.
fn searchable_dimension(descriptor: &BinaryDescriptor) -> Result<usize, VectorIndexError> {
    if descriptor.element_type != ElementType::F32 {
        return Err(VectorIndexError::Element(descriptor.element_type));
    }
    if descriptor.byte_order != NATIVE_ORDER {
        return Err(VectorIndexError::ByteOrder(descriptor.byte_order));
    }
    if !descriptor.unit {
        return Err(VectorIndexError::NotUnit { producer: descriptor.producer.clone() });
    }
    let [dimension] = descriptor.shape.as_slice() else {
        return Err(VectorIndexError::Rank(descriptor.shape.len()));
    };
    if *dimension == 0 {
        // No zero-dimension vector has unit length, whatever its descriptor claims.
        return Err(VectorIndexError::NotUnit { producer: descriptor.producer.clone() });
    }
    usize::try_from(*dimension).map_err(|_| VectorIndexError::DimensionOverflow(*dimension))
}
