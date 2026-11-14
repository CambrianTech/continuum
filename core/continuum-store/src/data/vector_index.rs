//! The vector consumer on the native storage seam (CONTINUUM-ENGINE.md section 6).
//!
//! Contract (PR #5954, agreed with the storage owner):
//! - the producer normalizes ONCE at write and declares `unit = true` ([`unit_f32`]);
//! - search refuses a `unit = false` payload with a typed error and never normalizes a row,
//!   so every score is one dot product: no sqrt, no norms;
//! - one resident row-major matrix per collection, filled one native page at a time;
//! - a sign-bit sketch per row feeds a popcount prefilter whose candidate count is
//!   MEASURED against a recall requirement when the matrix is sealed ([`RecallTarget`]);
//! - exact dot on the survivors, top-k by a bounded heap.
//!
//! [`MatrixBuilder`] gathers pages; sealing it measures the prefilter and yields a
//! [`CpuMatrix`], so an unmeasured matrix cannot be searched. [`ResidentMatrix`] is the compute
//! seam: the Metal GEMV + fused top-k implements it over the same contiguous buffer.

mod calibrate;
mod cpu;
mod error;
mod unit;

#[cfg(test)]
mod tests;

use crate::orm::binary::{BinaryDescriptor, BinaryValue, ByteOrder, ElementType};
use std::num::NonZeroUsize;

pub(crate) use cpu::{CpuMatrix, MatrixBuilder};
pub(crate) use error::{VectorIndexError, VectorStoreError};
pub(crate) use unit::{unit_f32, UnitVectorError};

/// A stored record's identity, as the native page reports it.
pub(crate) type RecordId = uuid::Uuid;

/// The byte order rows are copied in without decoding; any other order is refused.
pub(crate) const NATIVE_ORDER: ByteOrder =
    if cfg!(target_endian = "little") { ByteOrder::Little } else { ByteOrder::Big };

/// The identity an embedder reports for the vectors it produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmbedderId(String);

impl EmbedderId {
    pub(crate) fn new(id: &str) -> Self {
        Self(id.to_owned())
    }
}

/// Who certified a vector's normalization. Rendered into the descriptor's producer field in
/// one place, so no label is typed at a call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Producer {
    /// The served embedder, writing rows through `vector/backfill`.
    Embedder(EmbedderId),
    /// A query vector arriving on the `vector/search` wire.
    WireQuery,
    /// A row vector arriving on the `vector/index` wire.
    IndexCommand,
}

impl Producer {
    fn identity(&self) -> String {
        match self {
            Self::Embedder(EmbedderId(id)) => id.clone(),
            Self::WireQuery => "vector/search".to_owned(),
            Self::IndexCommand => "vector/index".to_owned(),
        }
    }
}

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

/// One ranked row: its position in the matrix and its dot-product score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hit {
    pub(crate) row: usize,
    pub(crate) score: f32,
}

/// The compute seam every sealed backend implements over the same row-major unit matrix.
pub(crate) trait ResidentMatrix: Send + Sync {
    /// The `k` best rows by dot product, best first.
    fn search(&self, query: &BinaryValue, k: NonZeroUsize) -> Result<Vec<Hit>, VectorIndexError>;

    fn len(&self) -> usize;

    fn id(&self, row: usize) -> Option<RecordId>;
}

/// The rank-one dimension a descriptor declares, after the searchable-row checks.
fn searchable_dimension(descriptor: &BinaryDescriptor) -> Result<NonZeroUsize, VectorIndexError> {
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
    let dimension = usize::try_from(*dimension).map_err(|_| VectorIndexError::DimensionOverflow(*dimension))?;
    // No zero-dimension vector has unit length, whatever its descriptor claims.
    NonZeroUsize::new(dimension).ok_or_else(|| VectorIndexError::NotUnit { producer: descriptor.producer.clone() })
}
