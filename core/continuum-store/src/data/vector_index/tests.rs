#![cfg(test)]
//! The consumer's contract: certified unit rows, typed refusals, one dot product per score,
//! and a measured prefilter that keeps the exact ranking.
use super::cpu::{dot, CpuMatrix, MatrixBuilder};
use super::{unit_f32, Hit, Producer, RecallTarget, RecordId, ResidentMatrix, UnitVectorError, VectorIndexError, NATIVE_ORDER};
use crate::orm::binary::{BinaryDescriptor, BinaryValue, ElementType};
use std::num::NonZeroUsize;

mod bench;

/// Deterministic embedding-shaped data: rows cluster around centers, like documents on a
/// topic, so neighbors share most sign bits (the case the sketch prefilter is for).
pub(super) mod synthetic {
    /// splitmix64: a fixed stream on every host, no rand dependency.
    pub(crate) struct Stream(u64);

    impl Stream {
        pub(crate) fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// Uniform in [-1, 1).
        pub(crate) fn signed(&mut self) -> f32 {
            (self.next() >> 40) as f32 / (1u64 << 23) as f32 - 1.0
        }

        pub(crate) fn below(&mut self, bound: usize) -> usize {
            (self.next() % bound as u64) as usize
        }
    }

    /// `count` vectors of `dimension`, each a random center plus `noise`-scaled jitter.
    pub(crate) fn clustered(stream: &mut Stream, centers: &[Vec<f32>], count: usize, noise: f32) -> Vec<Vec<f32>> {
        (0..count)
            .map(|_| {
                let center = &centers[stream.below(centers.len())];
                center.iter().map(|value| value + noise * stream.signed()).collect()
            })
            .collect()
    }

    pub(crate) fn centers(stream: &mut Stream, count: usize, dimension: usize) -> Vec<Vec<f32>> {
        (0..count).map(|_| (0..dimension).map(|_| stream.signed()).collect()).collect()
    }
}

pub(super) fn unit(values: &[f32]) -> Result<BinaryValue, UnitVectorError> {
    unit_f32(values.iter().copied(), &Producer::IndexCommand)
}

/// The id the fixtures give row `row`.
pub(super) fn id_of(row: usize) -> RecordId {
    RecordId::from_u128(row as u128 + 1)
}

/// One native page of `rows`, as `binary_page_into` lays it out: row-major bytes, parallel ids.
pub(super) fn page_of(rows: &[Vec<f32>], first: usize) -> Result<(BinaryDescriptor, Vec<u8>, Vec<RecordId>), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    let mut descriptor = None;
    for values in rows {
        let (row_descriptor, row_bytes) = unit(values)?.into_parts();
        bytes.extend_from_slice(&row_bytes);
        descriptor = Some(row_descriptor);
    }
    let ids = (first..first + rows.len()).map(id_of).collect();
    Ok((descriptor.ok_or("test: a page has rows")?, bytes, ids))
}

/// The sealed matrix of `rows`, appended as one page.
pub(super) fn matrix_of(rows: &[Vec<f32>]) -> Result<CpuMatrix, Box<dyn std::error::Error>> {
    let (descriptor, bytes, ids) = page_of(rows, 0)?;
    let mut builder = MatrixBuilder::default();
    builder.append_page(&descriptor, &bytes, &ids)?;
    Ok(builder.seal(RecallTarget::STORE))
}

pub(super) fn floats(value: &BinaryValue) -> Vec<f32> {
    value.bytes().chunks_exact(4).map(|bytes| f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])).collect()
}

/// The rows exactly as the producer certifies them.
pub(super) fn normalized(rows: &[Vec<f32>]) -> Result<Vec<Vec<f32>>, UnitVectorError> {
    rows.iter().map(|row| unit(row).map(|value| floats(&value))).collect()
}

/// Every row scored, best first: the ranking the prefilter must reproduce.
pub(super) fn exact(normalized: &[Vec<f32>], query: &[f32], k: usize) -> Vec<usize> {
    let mut scored: Vec<Hit> = normalized.iter().enumerate().map(|(row, values)| Hit { row, score: dot(query, values) }).collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.row.cmp(&b.row)));
    scored.into_iter().take(k).map(|hit| hit.row).collect()
}

fn rows_of(hits: &[Hit]) -> Vec<usize> {
    hits.iter().map(|hit| hit.row).collect()
}

// what this catches: the producer's normalization drifting past f32's error bound. The bound:
// a sequential f32 sum of D squares has relative error <= D*eps, its reciprocal square root
// adds ~2*eps and each scaled component one more rounding, so | |v|^2 - 1 | <= (D + 8) * eps.
#[test]
fn producer_certifies_unit_length_within_the_f32_bound() -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = synthetic::Stream::new(7);
    for dimension in [1usize, 3, 8, 384, 1024, 4096] {
        for scale in [1e-15f32, 1e-3, 1.0, 1e3, 1e15] {
            let values: Vec<f32> = (0..dimension).map(|_| scale * stream.signed()).collect();
            if values.iter().all(|value| *value == 0.0) {
                continue;
            }
            let payload = unit(&values)?;
            let descriptor = payload.descriptor();
            assert!(descriptor.unit && descriptor.element_type == ElementType::F32 && descriptor.byte_order == NATIVE_ORDER);
            assert_eq!(descriptor.shape, vec![dimension as u64]);
            let squared: f64 = floats(&payload).iter().map(|value| f64::from(*value) * f64::from(*value)).sum();
            let bound = (dimension as f64 + 8.0) * f64::from(f32::EPSILON);
            assert!((squared - 1.0).abs() <= bound, "dimension {dimension} scale {scale}: |v|^2 = {squared}, bound {bound}");
        }
    }
    Ok(())
}

// what this catches: a vector with no direction stored as NaN instead of refused.
#[test]
fn producer_refuses_vectors_without_a_direction() {
    assert_eq!(unit(&[0.0; 384]), Err(UnitVectorError::Zero));
    assert_eq!(unit(&[]), Err(UnitVectorError::Empty));
    assert_eq!(unit(&[1.0, f32::NAN]), Err(UnitVectorError::NonFinite));
    assert_eq!(unit(&[f32::INFINITY, 1.0]), Err(UnitVectorError::NonFinite));
    assert_eq!(unit(&[f32::MAX, f32::MAX]), Err(UnitVectorError::NonFinite));
}

// what this catches: search normalizing (or silently accepting) an uncertified vector.
#[test]
fn uncertified_rows_and_queries_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let certified = unit(&[3.0, 4.0])?;
    let (descriptor, bytes) = certified.clone().into_parts();
    let raw = BinaryValue::new(BinaryDescriptor { unit: false, ..descriptor.clone() }, bytes.clone())?;
    let mut builder = MatrixBuilder::default();
    assert!(matches!(builder.append_page(raw.descriptor(), raw.bytes(), &[id_of(0)]), Err(VectorIndexError::NotUnit { .. })));
    builder.append_page(certified.descriptor(), certified.bytes(), &[id_of(1)])?;
    let wider = unit(&[1.0, 2.0, 3.0])?;
    assert!(matches!(builder.append_page(wider.descriptor(), wider.bytes(), &[id_of(2)]), Err(VectorIndexError::Dimension { .. })));
    // A page whose bytes disagree with its row count never enters the matrix.
    assert_eq!(
        builder.append_page(&descriptor, &bytes, &[id_of(3), id_of(4)]),
        Err(VectorIndexError::PageLength { expected: 16, found: 8 }),
    );
    let matrix = builder.seal(RecallTarget::STORE);
    assert_eq!(matrix.len(), 1);
    let one = NonZeroUsize::MIN;
    assert!(matches!(matrix.search(&raw, one), Err(VectorIndexError::NotUnit { .. })));
    assert_eq!(matrix.search(&wider, one), Err(VectorIndexError::Dimension { expected: 2, found: 3 }));
    let quantized = BinaryValue::new(BinaryDescriptor { element_type: ElementType::F16, ..descriptor }, vec![0; 4])?;
    assert_eq!(matrix.search(&quantized, one), Err(VectorIndexError::Element(ElementType::F16)));
    Ok(())
}

// what this catches: a corpus split across pages ranking differently from one page.
#[test]
fn pages_append_to_the_same_matrix_as_one_page() -> Result<(), Box<dyn std::error::Error>> {
    let rows = [vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0], vec![0.6, 0.8, 0.0]];
    let whole = matrix_of(&rows)?;
    let mut paged = MatrixBuilder::default();
    for (first, page) in [(0, &rows[..2]), (2, &rows[2..])] {
        let (descriptor, bytes, ids) = page_of(page, first)?;
        paged.append_page(&descriptor, &bytes, &ids)?;
    }
    let paged = paged.seal(RecallTarget::STORE);
    let query = unit(&[0.6, 0.8, 0.0])?;
    let k = NonZeroUsize::new(3).ok_or("test: k")?;
    assert_eq!(paged.search(&query, k)?, whole.search(&query, k)?);
    assert_eq!(whole.search(&query, k)?.first().map(|hit| hit.row), Some(2));
    assert_eq!(paged.id(2), Some(id_of(2)));
    Ok(())
}

// what this catches: ties reordering between calls, k past the corpus, or an empty matrix.
#[test]
fn equal_scores_keep_corpus_order_and_k_is_bounded_by_the_rows() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = matrix_of(&[vec![1.0, 1.0], vec![1.0, 1.0], vec![-1.0, 0.0]])?;
    let ranked = matrix.search(&unit(&[1.0, 1.0])?, NonZeroUsize::new(99).ok_or("test: k")?)?;
    assert_eq!(rows_of(&ranked), vec![0, 1, 2]);
    assert_eq!(ranked[0].score, ranked[1].score, "identical directions score identically");
    let empty = MatrixBuilder::default().seal(RecallTarget::STORE);
    assert_eq!(empty.search(&unit(&[1.0, 1.0])?, NonZeroUsize::MIN)?, Vec::new());
    assert_eq!(empty.candidates(NonZeroUsize::MIN), None);
    Ok(())
}

// what this catches: the prefilter dropping true neighbors, or not pruning at all. The
// candidate count is measured on corpus probes; held-out queries from the same topics
// must keep their top-10 (mean recall), while the prefilter scores a fraction of rows.
#[test]
fn measured_prefilter_keeps_the_exact_top_k() -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = synthetic::Stream::new(42);
    let centers = synthetic::centers(&mut stream, 50, 384);
    let rows = synthetic::clustered(&mut stream, &centers, 1000, 1.0);
    let queries = synthetic::clustered(&mut stream, &centers, 40, 1.0);
    let matrix = matrix_of(&rows)?;
    let k = NonZeroUsize::new(10).ok_or("test: k")?;
    let candidates = matrix.candidates(k).ok_or("test: clustered rows must be measured to prune")?;
    assert!(candidates.get() < rows.len() / 2, "clustered rows must prune: {candidates} of {}", rows.len());
    let resident = normalized(&rows)?;
    let mut kept = 0usize;
    for query in &queries {
        let query = unit(query)?;
        let truth = exact(&resident, &floats(&query), k.get());
        let found = rows_of(&matrix.search(&query, k)?);
        kept += truth.iter().filter(|row| found.contains(row)).count();
    }
    let recall = kept as f64 / (queries.len() * k.get()) as f64;
    assert!(recall >= 0.95, "recall@10 {recall} below the measured requirement's neighborhood");
    Ok(())
}
