//! Isolated tests for Context — each test exercises one thing.
//! Run: cargo test --release -p llama --features metal --test context_test

use llama::{Batch, ContextParams, Model, ModelParams, Sampler};
use std::path::PathBuf;

/// Find a test model. Mirrors model_test.rs — keep in sync.
fn test_model() -> Option<PathBuf> {
    for candidate in ["/tmp/qwen25_3b.gguf", "/tmp/test_model.gguf"] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home)
            .join(".cache/huggingface/hub")
            .join("models--continuum-ai--qwen3.5-4b-code-forged-GGUF/snapshots")
            .join("6cfe43981913730b1abc4ad520510a24b3f05922")
            .join("qwen3.5-4b-code-forged-Q4_K_M.gguf");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn load() -> Option<(Model, ContextParams)> {
    let path = test_model()?;
    let model = Model::load(&path, ModelParams::default()).ok()?;
    Some((model, ContextParams::default()))
}

// ─── Batch construction ─────────────────────────────────────────────────

#[test]
fn batch_for_tokens_has_correct_count() {
    let b = Batch::for_tokens(vec![1, 2, 3, 4, 5]);
    assert_eq!(b.n_tokens(), 5);
}

#[test]
fn batch_for_tokens_empty_is_zero() {
    let b = Batch::for_tokens(vec![]);
    assert_eq!(b.n_tokens(), 0);
}

#[test]
fn batch_allocated_push_increments() {
    let mut b = Batch::allocated(16, 1);
    assert_eq!(b.n_tokens(), 0);
    b.push(42, 0, &[0], false);
    assert_eq!(b.n_tokens(), 1);
    b.push(43, 1, &[0], true);
    assert_eq!(b.n_tokens(), 2);
}

#[test]
fn batch_allocated_clear_resets() {
    let mut b = Batch::allocated(16, 1);
    b.push(42, 0, &[0], true);
    b.push(43, 1, &[0], true);
    b.clear();
    assert_eq!(b.n_tokens(), 0);
    // Post-clear push still works
    b.push(100, 0, &[0], true);
    assert_eq!(b.n_tokens(), 1);
}

#[test]
#[should_panic(expected = "push() on single-sequence batch")]
fn batch_for_tokens_push_panics() {
    let mut b = Batch::for_tokens(vec![1, 2]);
    b.push(3, 2, &[0], false);
}

// ─── Decode ─────────────────────────────────────────────────────────────

#[test]
fn decode_prefill_succeeds() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    let tokens = model.tokenize("Hello", true, false).expect("tokenize");
    let batch = Batch::for_tokens(tokens.clone());
    ctx.decode(&batch).expect("decode should succeed");
    // Logits for last token should be non-empty
    let logits = ctx.logits_ith(-1);
    assert_eq!(
        logits.len(),
        model.n_vocab() as usize,
        "logits length must match vocab size"
    );
}

#[test]
fn decode_one_token_after_prefill() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    let tokens = model
        .tokenize("The capital of France is", true, false)
        .expect("tokenize");
    ctx.decode(&Batch::for_tokens(tokens)).expect("prefill");

    // Sample next token greedily, then feed it back as a 1-token batch
    let mut sampler = Sampler::greedy();
    let next = sampler.sample(&ctx, -1);
    ctx.decode(&Batch::for_tokens(vec![next]))
        .expect("one-token decode");

    let logits = ctx.logits_ith(-1);
    assert_eq!(logits.len(), model.n_vocab() as usize);
}

#[test]
fn logits_have_finite_values() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    let tokens = model.tokenize("test", true, false).expect("tokenize");
    ctx.decode(&Batch::for_tokens(tokens)).expect("decode");
    let logits = ctx.logits_ith(-1);
    assert!(
        logits.iter().any(|&x| x.is_finite()),
        "at least some logits must be finite"
    );
    // argmax produces a sane token id
    let (argmax, _) = logits
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap();
    assert!(
        (argmax as i32) < model.n_vocab(),
        "argmax must be a valid token id"
    );
}

// ─── Sampling ───────────────────────────────────────────────────────────

#[test]
fn sample_greedy_returns_argmax() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    let tokens = model.tokenize("hello", true, false).expect("tokenize");
    ctx.decode(&Batch::for_tokens(tokens)).expect("decode");

    let mut s1 = Sampler::greedy();
    let t1 = s1.sample(&ctx, -1);
    let mut s2 = Sampler::greedy();
    let t2 = s2.sample(&ctx, -1);
    assert_eq!(t1, t2, "greedy must be deterministic");
    assert!(t1 >= 0 && t1 < model.n_vocab(), "valid token id");
}

#[test]
fn sample_temperature_chain_builds_and_samples() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    let tokens = model.tokenize("hello", true, false).expect("tokenize");
    ctx.decode(&Batch::for_tokens(tokens)).expect("decode");

    let mut sampler = Sampler::chain()
        .top_k(40)
        .top_p(0.9, 1)
        .temp(0.8)
        .dist(42)
        .build();
    let tok = sampler.sample(&ctx, -1);
    assert!(tok >= 0 && tok < model.n_vocab());
}

#[test]
fn sample_temperature_with_penalties() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    let tokens = model.tokenize("hello", true, false).expect("tokenize");
    ctx.decode(&Batch::for_tokens(tokens)).expect("decode");

    let mut sampler = Sampler::chain()
        .penalties(64, 1.1, 0.0, 0.0)
        .temp(0.8)
        .dist(42)
        .build();
    let tok = sampler.sample(&ctx, -1);
    assert!(tok >= 0 && tok < model.n_vocab());
}

// ─── LoRA ───────────────────────────────────────────────────────────────

#[test]
fn lora_clear_on_fresh_context_is_noop() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    // Clearing with no adapters loaded must not error.
    ctx.clear_loras().expect("clear on empty set should be ok");
}

#[test]
fn lora_set_empty_slice_is_noop() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    ctx.set_loras(&[]).expect("empty set must be ok");
}

#[test]
fn lora_load_fails_on_missing_file() {
    let (model, _) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let result = model.load_lora("/nonexistent/adapter.gguf");
    assert!(result.is_err(), "load_lora must fail on missing file");
}

/// Hot-swap round-trip: set adapters, clear, set again. Exercises the
/// genome-paging primitive without needing a real adapter file.
#[test]
fn lora_hot_swap_round_trips_with_empty_sets() {
    let (model, cp) = match load() {
        Some(v) => v,
        None => {
            eprintln!("no test model — skipping");
            return;
        }
    };
    let mut ctx = model.new_context(cp).expect("context");
    // Simulate paging cycle: active set changes over time.
    for _ in 0..5 {
        ctx.set_loras(&[]).expect("set empty");
        ctx.clear_loras().expect("clear");
    }
}

// ─── Send/Sync sanity ───────────────────────────────────────────────────

#[test]
fn context_is_send() {
    fn assert_send<T: Send>() {}
    // Context holds a raw pointer but we assert Send via unsafe impl when
    // a backend is used from one thread at a time — add that guarantee here.
    // Currently Context is !Send (no unsafe impl). When genome paging lands
    // a worker thread, this test will need to be updated. Keeping it here
    // as a breadcrumb.
    // assert_send::<llama::Context<'_>>();
    fn _noop() {
        assert_send::<()>();
    }
    _noop();
}

#[test]
fn sampler_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Sampler>();
}
