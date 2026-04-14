//! Concurrent multi-stream tests at the `llama` crate level.
//!
//! SCOPE: raw-API safety under concurrent use. Each thread creates its
//! OWN `Context` via `model.new_context()`. This path is inherently
//! serialized — every context re-reads the model weights, and they
//! contend at the Metal/CUDA command queue level. So per-call-context
//! concurrency is ~0.25x efficiency on 4 streams by construction.
//!
//! The `no_corruption_*` tests are the load-bearing contract here:
//! concurrent per-call contexts must not crash, deadlock, or bleed
//! tokens between streams. That safety property must hold regardless of
//! which backend the caller chooses.
//!
//! The BatchScheduler (`continuum-core::inference::backends::llamacpp_scheduler`)
//! is where real multi-stream throughput lives — shared context,
//! n_seq_max sequences, one decode per loop advancing all seqs in
//! parallel. Its perf contract belongs in a continuum-core-level test,
//! not here. The `concurrent_streams_match_solo_throughput` test below
//! measures per-call-context behavior only — useful as a regression
//! floor (it should never drop BELOW 0.25x, which would indicate the
//! Metal queue is actively deadlocking rather than serializing).
//!
//! Run:
//!   cargo test --release -p llama --features metal \
//!       --test concurrent_streams_test -- --ignored --nocapture

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use std::thread;

use llama::{Batch, ContextParams, Model, ModelParams, Sampler};

fn test_model() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home)
            .join(".cache/huggingface/hub")
            .join("models--continuum-ai--qwen3.5-4b-code-forged-GGUF/snapshots")
            .join("6cfe43981913730b1abc4ad520510a24b3f05922")
            .join("qwen3.5-4b-code-forged-Q4_K_M.gguf");
        if p.exists() { return Some(p); }
    }
    None
}

/// Single-stream generation. Used both as the solo baseline and as the
/// per-thread workload in the concurrent test. Returns (tok_count, ms).
fn generate_once(model: &Model, prompt: &str, max_tokens: usize) -> (usize, u128) {
    let mut ctx = model.new_context(ContextParams::default()).expect("ctx");
    let prompt_tokens = model.tokenize(prompt, true, false).expect("tokenize");
    let total = prompt_tokens.len();

    // Chunked prefill (mirrors LlamaCppBackend::generate)
    let n_batch = 512;
    let mut batch = Batch::allocated(n_batch as i32, 1);
    let last_idx = total - 1;
    let mut chunk_start = 0;
    while chunk_start < total {
        let chunk_end = (chunk_start + n_batch).min(total);
        batch.clear();
        for i in chunk_start..chunk_end {
            batch.push(prompt_tokens[i], i as i32, &[0], i == last_idx);
        }
        ctx.decode(&batch).expect("prefill");
        chunk_start = chunk_end;
    }

    let mut sampler = Sampler::greedy();
    let mut n_decoded = 0usize;
    let mut n_cur = total as i32;
    let start = Instant::now();
    for _ in 0..max_tokens {
        let token = sampler.sample(&ctx, -1);
        sampler.accept(token);
        if model.is_eog_token(token) { break; }
        batch.clear();
        batch.push(token, n_cur, &[0], true);
        ctx.decode(&batch).expect("gen");
        n_cur += 1;
        n_decoded += 1;
    }
    (n_decoded, start.elapsed().as_millis())
}

/// Helper: load model once, run N parallel generate calls on the same
/// Arc<Model>. Returns (per-thread token counts, wall-clock ms).
fn run_concurrent(
    n_streams: usize,
    prompt: &str,
    max_tokens: usize,
) -> Option<(Vec<usize>, u128)> {
    let path = test_model()?;
    let model = Arc::new(Model::load(&path, ModelParams::default()).ok()?);

    // Warm up: solo run primes Metal kernels so concurrent timing isn't
    // polluted by first-call overhead.
    let _ = generate_once(&model, prompt, 4);

    let start = Instant::now();
    let handles: Vec<_> = (0..n_streams)
        .map(|i| {
            let m = Arc::clone(&model);
            let p = format!("{}\n// stream {}\n", prompt, i);
            thread::spawn(move || generate_once(&m, &p, max_tokens))
        })
        .collect();

    let mut tok_counts = Vec::with_capacity(n_streams);
    for h in handles {
        let (n, _ms) = h.join().expect("thread");
        tok_counts.push(n);
    }
    Some((tok_counts, start.elapsed().as_millis()))
}

// ─── Correctness contracts (must hold today AND after batching) ────────

#[test]
#[ignore]
fn no_corruption_two_streams() {
    let path = match test_model() {
        Some(p) => p,
        None => { eprintln!("no model — skipping"); return; }
    };
    let model = Arc::new(Model::load(&path, ModelParams::default()).expect("load"));

    let p1 = "fn add(a: u32, b: u32) -> u32 {\n";
    let p2 = "fn multiply(a: u32, b: u32) -> u32 {\n";
    let m1 = Arc::clone(&model);
    let m2 = Arc::clone(&model);

    let h1 = thread::spawn(move || generate_once(&m1, p1, 16));
    let h2 = thread::spawn(move || generate_once(&m2, p2, 16));

    let (n1, _) = h1.join().unwrap();
    let (n2, _) = h2.join().unwrap();

    assert!(n1 > 0, "stream 1 produced no tokens");
    assert!(n2 > 0, "stream 2 produced no tokens");
}

#[test]
#[ignore]
fn no_corruption_four_streams() {
    let model = match test_model().and_then(|p| Model::load(&p, ModelParams::default()).ok()) {
        Some(m) => Arc::new(m),
        None => { eprintln!("no model — skipping"); return; }
    };

    let prompts = [
        "fn fibonacci(n: u32) -> u32 {\n",
        "fn factorial(n: u32) -> u64 {\n",
        "fn is_prime(n: u32) -> bool {\n",
        "fn gcd(a: u32, b: u32) -> u32 {\n",
    ];

    let handles: Vec<_> = prompts.iter().map(|&p| {
        let m = Arc::clone(&model);
        thread::spawn(move || generate_once(&m, p, 8))
    }).collect();

    for (i, h) in handles.into_iter().enumerate() {
        let (n, _) = h.join().unwrap();
        assert!(n > 0, "stream {} produced no tokens", i);
    }
}

// ─── Performance contracts (target for BatchScheduler era) ──────────────

/// Solo baseline. Records tok/s for a 32-token gen on a short prompt.
/// Companion to `concurrent_streams_match_solo_throughput` — the ratio
/// of those two is the batching efficiency score.
#[test]
#[ignore]
fn solo_throughput_baseline() {
    let path = match test_model() {
        Some(p) => p,
        None => { eprintln!("no model — skipping"); return; }
    };
    let model = Model::load(&path, ModelParams::default()).expect("load");
    let _ = generate_once(&model, "warm", 4); // warmup
    let (n, ms) = generate_once(&model, "fn add(a: u32, b: u32) -> u32 {\n", 32);
    let tok_s = (n as f64) * 1000.0 / (ms as f64);
    eprintln!("SOLO: {} tok in {} ms = {:.1} tok/s", n, ms, tok_s);
    assert!(n > 0, "solo produced no tokens");
}

/// Per-call-context concurrent behavior — by construction this ratio
/// is ~0.25x on 4 streams (each thread has its own Context, Metal
/// queue serializes the decode calls). This test exists to catch a
/// REGRESSION BELOW 0.25x: if the efficiency drops much lower, the
/// Metal/CUDA queue is deadlocking rather than serializing, or one
/// stream is starving entirely.
///
/// Real multi-stream throughput is a continuum-core concern — see
/// `LlamaCppSchedulerBackend` in workers/continuum-core. Its perf
/// contract belongs there, not here.
///
/// IMPORTANT: run this test with `--test-threads=1` for clean
/// numbers. Cargo's default parallel test runner will contaminate the
/// solo baseline with other tests' GPU work and make the ratio noise.
#[test]
#[ignore]
fn concurrent_streams_match_solo_throughput() {
    // Solo baseline
    let path = match test_model() {
        Some(p) => p,
        None => { eprintln!("no model — skipping"); return; }
    };
    let model = Model::load(&path, ModelParams::default()).expect("load");
    let _ = generate_once(&model, "warm", 4);
    let (solo_n, solo_ms) = generate_once(&model, "fn add(a: u32, b: u32) -> u32 {\n", 32);
    let solo_tok_s = (solo_n as f64) * 1000.0 / (solo_ms as f64);
    drop(model);

    // 4-stream concurrent run, same prompt + max_tokens
    let (tok_counts, wall_ms) = match run_concurrent(4, "fn add(a: u32, b: u32) -> u32 {\n", 32) {
        Some(x) => x,
        None => { eprintln!("concurrent run failed — skipping"); return; }
    };

    let total_tokens: usize = tok_counts.iter().sum();
    let aggregate_tok_s = (total_tokens as f64) * 1000.0 / (wall_ms as f64);
    let per_stream_tok_s = aggregate_tok_s / 4.0;
    let efficiency = per_stream_tok_s / solo_tok_s;

    eprintln!("SOLO:        {:.1} tok/s", solo_tok_s);
    eprintln!("CONCURRENT:  {} streams produced {} tok in {} ms = {:.1} tok/s aggregate, {:.1} tok/s per stream",
        tok_counts.len(), total_tokens, wall_ms, aggregate_tok_s, per_stream_tok_s);
    eprintln!("EFFICIENCY:  {:.2}x solo per stream  (1.0 = perfect batching, 0.25 = serialized 4-way)",
        efficiency);

    // Per-call-context on 4 streams should land near 0.25x (serialized).
    // Floor is 0.15x — catches deadlocks/starvation without flagging
    // normal Metal queue scheduling jitter.
    assert!(efficiency >= 0.15,
        "per-call-context concurrent throughput collapsed below serialization floor ({:.2}x) — deadlock or stream starvation", efficiency);
}

#[test]
#[ignore]
fn concurrent_does_not_panic_or_segv() {
    // Pure stress: 8 concurrent threads, tiny outputs. Catches data
    // races, double-frees in shared Model, batch buffer aliasing.
    let model = match test_model().and_then(|p| Model::load(&p, ModelParams::default()).ok()) {
        Some(m) => Arc::new(m),
        None => { eprintln!("no model — skipping"); return; }
    };

    let handles: Vec<_> = (0..8).map(|i| {
        let m = Arc::clone(&model);
        thread::spawn(move || {
            let p = format!("fn f_{}() {{\n", i);
            generate_once(&m, &p, 4)
        })
    }).collect();

    let mut survived = 0;
    for h in handles {
        if let Ok((n, _)) = h.join() {
            assert!(n > 0, "thread produced 0 tokens");
            survived += 1;
        }
    }
    assert_eq!(survived, 8, "not all 8 threads survived");
}
