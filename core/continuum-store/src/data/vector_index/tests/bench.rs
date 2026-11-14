//! vector-bench: the engine doc's simulation-first receipt for the vector consumer.
//!
//! OLD is the path this branch deletes, replayed faithfully: each row's embedding stored as
//! JSON text, parsed into `Vec<f64>` on load, and every query a full f64 cosine (two norms
//! and a square root per row) collected into an (index, score) vector, then partially
//! selected. NEW is the resident unit matrix: raw f32 bytes appended on load, the measured
//! sign-sketch prefilter, one dot product per survivor, a bounded heap.
//!
//! Both run single-threaded, so wall time per query is its CPU time (the deployed OLD path
//! spread the same CPU over rayon threads). Bytes copied per query are computed from the
//! buffers each path allocates per query, not sampled. Recall@k is NEW's top-k against
//! OLD's f64 ranking (the exact answer). The default run is the 1k receipt with its gates;
//! `--features stress-tests` reports 1k / 10k / 100k (`-- --nocapture` to read it).
use super::super::cpu::MatrixBuilder;
use super::super::{unit_f32, Hit, Producer, RecallTarget, ResidentMatrix};
use super::synthetic;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

const DIMENSION: usize = 384;
const K: usize = 10;

struct Receipt {
    rows: usize,
    old_load: Duration,
    new_load: Duration,
    old_p50: Duration,
    old_p99: Duration,
    new_p50: Duration,
    new_p99: Duration,
    old_bytes_per_query: usize,
    new_bytes_per_query: usize,
    candidates: usize,
    recall: f64,
}

/// The deleted f64 cosine, kept only as this bench's baseline.
fn old_cosine(a: &[f64], b: &[f64]) -> f64 {
    let (mut dot, mut norm_a, mut norm_b) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denominator = (norm_a * norm_b).sqrt();
    if denominator == 0.0 { 0.0 } else { dot / denominator }
}

fn old_search(corpus: &[Vec<f64>], query: &[f64], k: usize) -> Vec<usize> {
    let mut scored: Vec<(usize, f64)> = corpus.iter().enumerate().map(|(row, values)| (row, old_cosine(query, values))).collect();
    let rank = |a: &(usize, f64), b: &(usize, f64)| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0));
    if k < scored.len() {
        scored.select_nth_unstable_by(k, rank);
        scored.truncate(k);
    }
    scored.sort_unstable_by(rank);
    scored.into_iter().map(|(row, _)| row).collect()
}

fn percentile(samples: &mut [Duration], fraction: f64) -> Duration {
    samples.sort_unstable();
    let index = ((samples.len() as f64 * fraction).ceil() as usize).saturating_sub(1).min(samples.len() - 1);
    samples[index]
}

fn run(rows: usize, queries: usize) -> Result<Receipt, Box<dyn std::error::Error>> {
    let mut stream = synthetic::Stream::new(rows as u64);
    let centers = synthetic::centers(&mut stream, (rows / 20).max(1), DIMENSION);
    let raw = synthetic::clustered(&mut stream, &centers, rows, 1.0);
    let probes = synthetic::clustered(&mut stream, &centers, queries, 1.0);

    // OLD load: the JSON text the store held, parsed number by number.
    let stored: Vec<String> = raw.iter().map(serde_json::to_string).collect::<Result<_, _>>()?;
    let started = Instant::now();
    let old_corpus: Vec<Vec<f64>> = stored.iter().map(|text| serde_json::from_str(text)).collect::<Result<_, _>>()?;
    let old_load = started.elapsed();

    // NEW load: producer-certified payloads (normalized once at write), appended as bytes.
    // The page arrives as `binary_page_into` lays it out: row-major bytes and parallel ids.
    let (descriptor, page, ids) = super::page_of(&raw, 0)?;
    let started = Instant::now();
    let mut builder = MatrixBuilder::default();
    builder.append_page(&descriptor, &page, &ids)?;
    let matrix = builder.seal(RecallTarget::STORE);
    let new_load = started.elapsed();

    let k = NonZeroUsize::new(K).ok_or("bench: k")?;
    let (mut old_times, mut new_times) = (Vec::with_capacity(queries), Vec::with_capacity(queries));
    let mut kept = 0usize;
    for probe in &probes {
        // OLD query: the wire's f64 numbers, scored against every row.
        let wire: Vec<f64> = probe.iter().map(|value| f64::from(*value)).collect();
        let started = Instant::now();
        let truth = old_search(&old_corpus, &wire, K);
        old_times.push(started.elapsed());
        // NEW query: normalized once by its producer (the wire boundary), then searched.
        let started = Instant::now();
        let query = unit_f32(wire.iter().map(|value| *value as f32), &Producer::WireQuery)?;
        let found = matrix.search(&query, k)?;
        new_times.push(started.elapsed());
        kept += truth.iter().filter(|row| found.iter().any(|hit| hit.row == **row)).count();
    }
    let candidates = matrix.candidates(k).map_or(rows, NonZeroUsize::get);
    Ok(Receipt {
        rows,
        old_load,
        new_load,
        old_p50: percentile(&mut old_times, 0.50),
        old_p99: percentile(&mut old_times, 0.99),
        new_p50: percentile(&mut new_times, 0.50),
        new_p99: percentile(&mut new_times, 0.99),
        // The wire query as f64, then one (index, f64) per row.
        old_bytes_per_query: DIMENSION * 8 + rows * std::mem::size_of::<(usize, f64)>(),
        // The query payload, its copy into the thread's reused scratch, and the k hits.
        new_bytes_per_query: DIMENSION * 4 * 2 + K * std::mem::size_of::<Hit>(),
        candidates,
        recall: kept as f64 / (probes.len() * K) as f64,
    })
}

fn report(receipt: &Receipt) {
    eprintln!(
        "vector-bench rows={} load old={:?} new={:?} | query p50 old={:?} new={:?} | p99 old={:?} new={:?} | \
         bytes/query old={} new={} | candidates@{K}={} | recall@{K}={:.4}",
        receipt.rows, receipt.old_load, receipt.new_load, receipt.old_p50, receipt.new_p50,
        receipt.old_p99, receipt.new_p99, receipt.old_bytes_per_query, receipt.new_bytes_per_query,
        receipt.candidates, receipt.recall,
    );
}

// what this catches: the resident path losing recall against the exact f64 ranking it replaces,
// or copying more per query than the path it deletes.
#[test]
fn vector_bench_1k() -> Result<(), Box<dyn std::error::Error>> {
    let receipt = run(1_000, 50)?;
    report(&receipt);
    assert!(receipt.recall >= 0.95, "recall@{K} {} vs the exact f64 ranking", receipt.recall);
    assert!(receipt.new_bytes_per_query < receipt.old_bytes_per_query);
    assert!(receipt.candidates < receipt.rows);
    Ok(())
}

#[cfg(feature = "stress-tests")]
mod stress {
    use super::*;

    // what this catches: nothing on its own; it is the 1k / 10k / 100k receipt for the PR body.
    #[test]
    fn vector_bench_scales() -> Result<(), Box<dyn std::error::Error>> {
        for rows in [1_000, 10_000, 100_000] {
            report(&run(rows, 200)?);
        }
        Ok(())
    }
}
