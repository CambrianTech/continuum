//! The CPU implementation of [`ResidentMatrix`]: one contiguous row-major f32 buffer,
//! a packed sign sketch per row, a popcount prefilter, exact dot on the survivors and a
//! bounded top-k heap. No allocation proportional to the corpus happens per query: a
//! query allocates its own D floats, a (D + 1)-bucket histogram and a k-entry heap.
use super::calibrate::{self, CandidateNeed};
use super::{searchable_dimension, RecallTarget, ResidentMatrix, VectorIndexError};
use crate::orm::binary::{BinaryDescriptor, BinaryValue};
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::num::NonZeroUsize;

#[derive(Default)]
pub(crate) struct CpuMatrix {
    /// Set by the first row; every later row and query must match it.
    dimension: Option<usize>,
    /// Row-major unit vectors, `ids.len() * dimension` values.
    rows: Vec<f32>,
    /// Sign bits, `words(dimension)` words per row, low bit first (the `Bit` layout).
    sketches: Vec<u64>,
    ids: Vec<String>,
    /// Measured on the current rows; `None` means every row is a candidate.
    need: Option<CandidateNeed>,
}

impl ResidentMatrix for CpuMatrix {
    fn append_rows(&mut self, descriptor: &BinaryDescriptor, rows: &[u8], ids: Vec<String>) -> Result<(), VectorIndexError> {
        let dimension = searchable_dimension(descriptor)?;
        if let Some(expected) = self.dimension.filter(|expected| *expected != dimension) {
            return Err(VectorIndexError::Dimension { expected, found: dimension });
        }
        let overflow = VectorIndexError::PageOverflow { rows: ids.len(), dimension };
        let values = ids.len().checked_mul(dimension).ok_or_else(|| overflow.clone())?;
        let expected = values.checked_mul(std::mem::size_of::<f32>()).ok_or(overflow)?;
        if rows.len() != expected {
            return Err(VectorIndexError::PageLength { expected, found: rows.len() });
        }
        if ids.is_empty() {
            return Ok(());
        }
        self.dimension = Some(dimension);
        let start = self.rows.len();
        self.rows.resize(start + values, 0.0);
        // The rows arrive in the host's byte order (checked above): one byte copy, no decode.
        bytemuck::cast_slice_mut::<f32, u8>(&mut self.rows[start..]).copy_from_slice(rows);
        for row in self.rows[start..].chunks_exact(dimension) {
            sketch_into(row, &mut self.sketches);
        }
        self.ids.extend(ids);
        // A grown corpus is re-measured before the prefilter trusts a candidate count.
        self.need = None;
        Ok(())
    }

    fn calibrate(&mut self, target: RecallTarget) {
        self.need = match self.dimension {
            Some(dimension) if self.ids.len() > 1 => Some(calibrate::measure(&self.rows, &self.sketches, dimension, target)),
            _ => None,
        };
    }

    fn search(&self, query: &BinaryValue, k: NonZeroUsize) -> Result<Vec<(usize, f32)>, VectorIndexError> {
        let found = searchable_dimension(query.descriptor())?;
        let Some(dimension) = self.dimension else { return Ok(Vec::new()) };
        if found != dimension {
            return Err(VectorIndexError::Dimension { expected: dimension, found });
        }
        let mut values = vec![0.0f32; dimension];
        bytemuck::cast_slice_mut::<f32, u8>(&mut values).copy_from_slice(query.bytes());
        let rows = self.ids.len();
        let mut top = TopK::new(k.get().min(rows));
        let candidates = self.candidates(top.capacity());
        if candidates >= rows {
            for (row, stored) in self.rows.chunks_exact(dimension).enumerate() {
                top.offer(row, dot(&values, stored));
            }
            return Ok(top.into_ranked());
        }
        let words = words(dimension);
        let mut sketch = Vec::with_capacity(words);
        sketch_into(&values, &mut sketch);
        let radius = self.radius(&sketch, candidates, dimension);
        let stored = self.rows.chunks_exact(dimension).zip(self.sketches.chunks_exact(words));
        for (row, (stored, bits)) in stored.enumerate() {
            if hamming(&sketch, bits) <= radius {
                top.offer(row, dot(&values, stored));
            }
        }
        Ok(top.into_ranked())
    }

    fn len(&self) -> usize {
        self.ids.len()
    }

    fn id(&self, row: usize) -> Option<&str> {
        self.ids.get(row).map(String::as_str)
    }
}

impl CpuMatrix {
    /// Rows the prefilter admits for `k`: the measured need, or every row when unmeasured.
    pub(super) fn candidates(&self, k: usize) -> usize {
        self.need.as_ref().map_or(self.ids.len(), |need| need.candidates(k)).min(self.ids.len())
    }

    /// The smallest Hamming radius whose ball holds at least `candidates` rows. Ties at the
    /// radius are all kept, so a true neighbor at the boundary is never cut by order.
    fn radius(&self, sketch: &[u64], candidates: usize, dimension: usize) -> usize {
        let mut histogram = vec![0usize; dimension + 1];
        for bits in self.sketches.chunks_exact(sketch.len()) {
            if let Some(bucket) = histogram.get_mut(hamming(sketch, bits)) {
                *bucket += 1;
            }
        }
        let mut seen = 0usize;
        for (distance, count) in histogram.iter().enumerate() {
            seen += count;
            if seen >= candidates {
                return distance;
            }
        }
        dimension
    }
}

/// Words of sign sketch per row.
pub(super) fn words(dimension: usize) -> usize {
    dimension.div_ceil(64)
}

/// Appends a row's sign bits, low bit first, `words(row.len())` words.
pub(super) fn sketch_into(row: &[f32], out: &mut Vec<u64>) {
    for block in row.chunks(64) {
        let mut word = 0u64;
        for (bit, value) in block.iter().enumerate() {
            word |= u64::from(*value > 0.0) << bit;
        }
        out.push(word);
    }
}

/// Sign disagreements between two sketches; padding bits are zero in both.
pub(super) fn hamming(a: &[u64], b: &[u64]) -> usize {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones() as usize).sum()
}

/// The whole score: unit rows and a unit query, so cosine IS the dot product.
/// Eight independent lanes let the compiler vectorize without reassociating one sum.
pub(super) fn dot(a: &[f32], b: &[f32]) -> f32 {
    let (blocks_a, tail_a) = a.as_chunks::<8>();
    let (blocks_b, tail_b) = b.as_chunks::<8>();
    let mut lanes = [0.0f32; 8];
    for (x, y) in blocks_a.iter().zip(blocks_b) {
        for ((lane, xi), yi) in lanes.iter_mut().zip(x).zip(y) {
            *lane += xi * yi;
        }
    }
    let tail: f32 = tail_a.iter().zip(tail_b).map(|(x, y)| x * y).sum();
    lanes.iter().sum::<f32>() + tail
}

/// A scored row. Greater is better: a higher score, then the earlier row, so equal scores
/// keep stable corpus order (signed zeros compare equal). Scores are finite by `offer`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ranked {
    pub(super) score: f32,
    pub(super) row: usize,
}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.score == other.score {
            other.row.cmp(&self.row)
        } else {
            self.score.total_cmp(&other.score)
        }
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Ranked {}

/// The k best offers, held in a min-heap of size k: each offer costs O(log k).
pub(super) struct TopK {
    capacity: usize,
    heap: BinaryHeap<Reverse<Ranked>>,
}

impl TopK {
    pub(super) fn new(capacity: usize) -> Self {
        Self { capacity, heap: BinaryHeap::with_capacity(capacity) }
    }

    pub(super) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(super) fn offer(&mut self, row: usize, score: f32) {
        if !score.is_finite() {
            return;
        }
        let offered = Ranked { score, row };
        if self.heap.len() < self.capacity {
            self.heap.push(Reverse(offered));
        } else if let Some(mut worst) = self.heap.peek_mut() {
            // Replacing the top in place sifts once, instead of a pop and a push.
            if offered > worst.0 {
                *worst = Reverse(offered);
            }
        }
    }

    /// Best first.
    pub(super) fn into_ranked(self) -> Vec<(usize, f32)> {
        self.heap.into_sorted_vec().into_iter().map(|Reverse(ranked)| (ranked.row, ranked.score)).collect()
    }
}
