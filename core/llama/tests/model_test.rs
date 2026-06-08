//! Isolated tests for Model — each test exercises one thing.
//! Run: cargo test --release -p llama --features metal --test model_test

use std::path::PathBuf;
use llama::{Model, ModelParams};

/// Find a test model. Returns None if none available (tests skip).
fn test_model() -> Option<PathBuf> {
    for candidate in [
        "/tmp/qwen25_3b.gguf",
        "/tmp/test_model.gguf",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() { return Some(p); }
    }
    // HF cache
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

#[test]
fn load_fails_loudly_on_missing_file() {
    let result = Model::load("/nonexistent/model.gguf", ModelParams::default());
    assert!(result.is_err(), "must fail on missing file");
}

#[test]
fn load_succeeds_on_real_gguf() {
    let path = match test_model() {
        Some(p) => p,
        None => { eprintln!("no test model — skipping"); return; }
    };
    let model = Model::load(&path, ModelParams::default())
        .expect("load should succeed on real GGUF");
    assert!(model.n_vocab() > 0, "vocab should be non-empty");
    eprintln!("loaded {} with vocab size {}", path.display(), model.n_vocab());
}

#[test]
fn tokenize_returns_tokens() {
    let path = match test_model() {
        Some(p) => p,
        None => { eprintln!("no test model — skipping"); return; }
    };
    let model = Model::load(&path, ModelParams::default()).expect("load");
    let tokens = model.tokenize("Hello, world!", true, false).expect("tokenize");
    assert!(!tokens.is_empty(), "should tokenize non-empty text");
    eprintln!("'Hello, world!' -> {} tokens: {:?}", tokens.len(), tokens);
}

#[test]
fn token_to_piece_roundtrips_simple_text() {
    let path = match test_model() {
        Some(p) => p,
        None => { eprintln!("no test model — skipping"); return; }
    };
    let model = Model::load(&path, ModelParams::default()).expect("load");
    let tokens = model.tokenize("hello", false, false).expect("tokenize");
    let mut reconstructed = String::new();
    for t in tokens {
        reconstructed.push_str(&model.token_to_piece(t));
    }
    assert!(reconstructed.trim() == "hello" || reconstructed.contains("hello"),
        "token_to_piece should roundtrip, got {:?}", reconstructed);
}

#[test]
fn eog_token_detection() {
    let path = match test_model() {
        Some(p) => p,
        None => { eprintln!("no test model — skipping"); return; }
    };
    let model = Model::load(&path, ModelParams::default()).expect("load");
    // Token 0 is usually not EOG, but we verify the function is callable
    let _ = model.is_eog_token(0);
    // Some very high token should not crash
    let _ = model.is_eog_token(999_999);
}

#[test]
fn context_creation_succeeds() {
    let path = match test_model() {
        Some(p) => p,
        None => { eprintln!("no test model — skipping"); return; }
    };
    let model = Model::load(&path, ModelParams::default()).expect("load");
    let ctx = model.new_context(llama::ContextParams::default());
    assert!(ctx.is_ok(), "context creation should succeed");
}

#[test]
fn model_is_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<Model>();
    assert_sync::<Model>();
}
