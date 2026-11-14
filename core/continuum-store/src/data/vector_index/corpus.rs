//! A collection's vectors: one sealed matrix per embedding descriptor, never one mixed matrix.
use super::{compatible, CpuMatrix, EmbeddingDescriptor, RecordId, ResidentMatrix, VectorIndexError};
use crate::orm::binary::BinaryValue;
use std::num::NonZeroUsize;

/// A ranked record and its dot-product score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RecordHit {
    pub(crate) id: RecordId,
    pub(crate) score: f32,
}

/// Every sealed matrix of one collection, keyed by the descriptor its rows were written with.
/// Rows from `vector/index` and `vector/backfill` share a matrix only when their descriptors
/// are identical; otherwise each producer's rows are their own matrix.
pub(crate) struct VectorCorpus {
    matrices: Vec<(EmbeddingDescriptor, CpuMatrix)>,
}

impl VectorCorpus {
    pub(crate) fn new(matrices: Vec<(EmbeddingDescriptor, CpuMatrix)>) -> Self {
        Self { matrices }
    }

    /// Rows across every matrix.
    pub(crate) fn len(&self) -> usize {
        self.matrices.iter().map(|(_, matrix)| matrix.len()).sum()
    }

    /// The `k` best records of the one matrix compatible with the query's descriptor, best
    /// first. A query whose descriptor matches no stored matrix is refused, naming what is
    /// stored; an empty collection has nothing to rank.
    pub(crate) fn search(&self, query: &BinaryValue, k: NonZeroUsize) -> Result<Vec<RecordHit>, VectorIndexError> {
        if self.matrices.is_empty() {
            return Ok(Vec::new());
        }
        let wanted = EmbeddingDescriptor::from_binary(query.descriptor())?;
        let Some((_, matrix)) = self.matrices.iter().find(|(rows, _)| compatible(&wanted, rows)) else {
            return Err(VectorIndexError::DescriptorMismatch {
                query: wanted,
                available: self.matrices.iter().map(|(rows, _)| rows.clone()).collect(),
            });
        };
        matrix
            .search(query, k)?
            .into_iter()
            .map(|hit| {
                let id = matrix.id(hit.row).ok_or(VectorIndexError::RowIdentity(hit.row))?;
                Ok(RecordHit { id, score: hit.score })
            })
            .collect()
    }

    /// Every record id, matrix by matrix in storage order.
    #[cfg(test)]
    pub(crate) fn record_ids(&self) -> Vec<RecordId> {
        self.matrices
            .iter()
            .flat_map(|(_, matrix)| (0..matrix.len()).filter_map(|row| matrix.id(row)))
            .collect()
    }

    /// The descriptors of the sealed matrices.
    #[cfg(test)]
    pub(crate) fn descriptors(&self) -> Vec<EmbeddingDescriptor> {
        self.matrices.iter().map(|(descriptor, _)| descriptor.clone()).collect()
    }
}
