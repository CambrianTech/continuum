//! The CPU implementation of [`ResidentMatrix`]: one contiguous row-major f32 buffer, a
//! packed sign sketch per row, a popcount prefilter, exact dot on the survivors and a bounded
//! top-k heap. A query reuses its thread's scratch (query floats, sketch, histogram, heap), so
//! after the first query on a thread nothing is allocated but the k hits it returns.
use super::calibrate::{self, CandidateNeed};
use super::{searchable_dimension, Hit, RecallTarget, RecordId, ResidentMatrix, VectorIndexError};
use crate::orm::binary::{BinaryDescriptor, BinaryValue};
use std::cell::RefCell;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::num::NonZeroUsize;

/// Rows of one dimension, laid out for the dot product and the prefilter.
pub(super) struct Rows {
    pub(super) dimension: NonZeroUsize,
    /// Row-major unit vectors, `ids.len() * dimension` values.
    pub(super) values: Vec<f32>,
    /// Sign bits, `words(dimension)` words per row, low bit first (the `Bit` layout).
    pub(super) sketches: Vec<u64>,
    pub(super) ids: Vec<RecordId>,
}

impl Rows {
    fn words(&self) -> usize {
        words(self.dimension)
    }
}

/// Gathers native pages; [`MatrixBuilder::seal`] measures the prefilter and yields the
/// searchable matrix, so no search ever runs on an unmeasured candidate count.
#[derive(Default)]
pub(crate) struct MatrixBuilder {
    rows: Option<Rows>,
}

impl MatrixBuilder {
    /// Appends one page: `ids.len()` rows laid out row-major in `bytes`, all described by
    /// `descriptor` (the per-row shape `[dimension]`). This consumes the storage owner's
    /// `binary_page_into` buffers directly: one byte copy per page, no decode.
    pub(crate) fn append_page(&mut self, descriptor: &BinaryDescriptor, bytes: &[u8], ids: &[RecordId]) -> Result<(), VectorIndexError> {
        let dimension = searchable_dimension(descriptor)?;
        if let Some(expected) = self.rows.as_ref().map(|rows| rows.dimension).filter(|expected| *expected != dimension) {
            return Err(VectorIndexError::Dimension { expected: expected.get(), found: dimension.get() });
        }
        let overflow = VectorIndexError::PageOverflow { rows: ids.len(), dimension: dimension.get() };
        let values = ids.len().checked_mul(dimension.get()).ok_or_else(|| overflow.clone())?;
        let expected = values.checked_mul(std::mem::size_of::<f32>()).ok_or(overflow)?;
        if bytes.len() != expected {
            return Err(VectorIndexError::PageLength { expected, found: bytes.len() });
        }
        if ids.is_empty() {
            return Ok(());
        }
        let rows = self.rows.get_or_insert_with(|| Rows { dimension, values: Vec::new(), sketches: Vec::new(), ids: Vec::new() });
        let start = rows.values.len();
        rows.values.resize(start + values, 0.0);
        // The rows arrive in the host's byte order (checked above): one byte copy, no decode.
        bytemuck::cast_slice_mut::<f32, u8>(&mut rows.values[start..]).copy_from_slice(bytes);
        for row in rows.values[start..].chunks_exact(dimension.get()) {
            sketch_into(row, &mut rows.sketches);
        }
        rows.ids.extend_from_slice(ids);
        Ok(())
    }

    /// Measures how many prefilter candidates meet `target` on these rows.
    pub(crate) fn seal(self, target: RecallTarget) -> CpuMatrix {
        CpuMatrix(match self.rows {
            None => Sealed::Empty,
            Some(rows) => {
                let need = calibrate::measure(&rows, target);
                Sealed::Resident { rows, need }
            }
        })
    }
}

/// A sealed, searchable matrix.
pub(crate) struct CpuMatrix(Sealed);

enum Sealed {
    /// No row holds a vector yet.
    Empty,
    Resident { rows: Rows, need: CandidateNeed },
}

impl ResidentMatrix for CpuMatrix {
    fn search(&self, query: &BinaryValue, k: NonZeroUsize) -> Result<Vec<Hit>, VectorIndexError> {
        let found = searchable_dimension(query.descriptor())?;
        let Sealed::Resident { rows, need } = &self.0 else { return Ok(Vec::new()) };
        if found != rows.dimension {
            return Err(VectorIndexError::Dimension { expected: rows.dimension.get(), found: found.get() });
        }
        let dimension = rows.dimension.get();
        let Some(k) = NonZeroUsize::new(k.get().min(rows.ids.len())) else { return Ok(Vec::new()) };
        SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            let Scratch { query: values, sketch, histogram, heap } = &mut *scratch;
            values.clear();
            values.resize(dimension, 0.0);
            bytemuck::cast_slice_mut::<f32, u8>(values).copy_from_slice(query.bytes());
            let mut top = TopK::new(k, heap);
            match need.candidates(k).filter(|candidates| candidates.get() < rows.ids.len()) {
                // The measured candidate set is the whole corpus (or too small to measure):
                // every row is scored exactly.
                None => {
                    for (row, stored) in rows.values.chunks_exact(dimension).enumerate() {
                        top.offer(row, dot(values, stored));
                    }
                }
                Some(candidates) => {
                    sketch.clear();
                    sketch_into(values, sketch);
                    let radius = radius(rows, sketch, candidates, histogram);
                    let stored = rows.values.chunks_exact(dimension).zip(rows.sketches.chunks_exact(rows.words()));
                    for (row, (stored, bits)) in stored.enumerate() {
                        if hamming(sketch, bits) <= radius {
                            top.offer(row, dot(values, stored));
                        }
                    }
                }
            }
            Ok(top.into_hits())
        })
    }

    fn len(&self) -> usize {
        match &self.0 {
            Sealed::Empty => 0,
            Sealed::Resident { rows, .. } => rows.ids.len(),
        }
    }

    fn id(&self, row: usize) -> Option<RecordId> {
        match &self.0 {
            Sealed::Empty => None,
            Sealed::Resident { rows, .. } => rows.ids.get(row).copied(),
        }
    }
}

impl CpuMatrix {
    /// Rows the prefilter admits for `k`; `None` when every row is scored.
    #[cfg(test)]
    pub(super) fn candidates(&self, k: NonZeroUsize) -> Option<NonZeroUsize> {
        match &self.0 {
            Sealed::Empty => None,
            Sealed::Resident { rows, need } => need.candidates(k).filter(|candidates| candidates.get() < rows.ids.len()),
        }
    }
}

/// A query's working buffers, kept per thread so repeated queries reuse their capacity.
#[derive(Default)]
struct Scratch {
    query: Vec<f32>,
    sketch: Vec<u64>,
    histogram: Vec<usize>,
    heap: Vec<Reverse<Ranked>>,
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default());
}

/// The smallest Hamming radius whose ball holds at least `candidates` rows. Ties at the
/// radius are all kept, so a true neighbor at the boundary is never cut by order.
fn radius(rows: &Rows, sketch: &[u64], candidates: NonZeroUsize, histogram: &mut Vec<usize>) -> usize {
    let dimension = rows.dimension.get();
    histogram.clear();
    histogram.resize(dimension + 1, 0);
    for bits in rows.sketches.chunks_exact(sketch.len()) {
        if let Some(bucket) = histogram.get_mut(hamming(sketch, bits)) {
            *bucket += 1;
        }
    }
    let mut seen = 0usize;
    for (distance, count) in histogram.iter().enumerate() {
        seen += count;
        if seen >= candidates.get() {
            return distance;
        }
    }
    dimension
}

/// Words of sign sketch per row.
pub(super) fn words(dimension: NonZeroUsize) -> usize {
    dimension.get().div_ceil(64)
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

/// A scored row. Greater is better: a higher score, then the earlier row, so equal scores keep
/// stable corpus order. Scores are finite (`offer` drops the rest), so `total_cmp` is a total
/// order that agrees with `<` on every value it sees.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ranked {
    pub(super) score: f32,
    pub(super) row: usize,
}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score.total_cmp(&other.score).then_with(|| other.row.cmp(&self.row))
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

/// The k best offers, held in a min-heap of size k over the thread's reused heap buffer.
struct TopK<'a> {
    capacity: NonZeroUsize,
    heap: BinaryHeap<Reverse<Ranked>>,
    home: &'a mut Vec<Reverse<Ranked>>,
}

impl<'a> TopK<'a> {
    fn new(capacity: NonZeroUsize, home: &'a mut Vec<Reverse<Ranked>>) -> Self {
        let mut buffer = std::mem::take(home);
        buffer.clear();
        Self { capacity, heap: BinaryHeap::from(buffer), home }
    }

    fn offer(&mut self, row: usize, score: f32) {
        if !score.is_finite() {
            return;
        }
        let offered = Ranked { score, row };
        if self.heap.len() < self.capacity.get() {
            self.heap.push(Reverse(offered));
        } else if let Some(mut worst) = self.heap.peek_mut() {
            // Replacing the top in place sifts once, instead of a pop and a push.
            if offered > worst.0 {
                *worst = Reverse(offered);
            }
        }
    }

    /// Best first; the heap's buffer goes back to the thread for the next query.
    fn into_hits(self) -> Vec<Hit> {
        let mut sorted = self.heap.into_sorted_vec();
        let hits = sorted.iter().map(|Reverse(ranked)| Hit { row: ranked.row, score: ranked.score }).collect();
        sorted.clear();
        *self.home = sorted;
        hits
    }
}
