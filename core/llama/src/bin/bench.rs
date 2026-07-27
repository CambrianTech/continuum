//! Generation benchmark — tok/s comparison with llama.cpp baseline.
//!
//! Usage:
//!   cargo run --release -p llama --features metal --bin bench -- <model.gguf> [n_tokens]
//!
//! Prints: prefill tok/s, generation tok/s, wall-clock time.

use std::path::PathBuf;
use std::time::Instant;

use llama::{Batch, ContextParams, Model, ModelParams, Sampler};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args.get(1).expect("usage: bench <model.gguf> [n_tokens]");
    let n_tokens: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(128);
    let prompt = "Write a function to compute the nth Fibonacci number in Rust:\n";

    println!("Loading {}", model_path);
    let load_start = Instant::now();
    let model = Model::load(
        PathBuf::from(model_path),
        ModelParams {
            n_gpu_layers: -1,
            use_mmap: true,
        },
    )
    .expect("load");
    println!(
        "Loaded in {:.2}s (vocab={})",
        load_start.elapsed().as_secs_f64(),
        model.n_vocab()
    );

    let mut ctx = model
        .new_context(ContextParams {
            n_ctx: 4096,
            n_batch: 512,
            n_seq_max: 1,
            ..Default::default()
        })
        .expect("context");

    let prompt_tokens = model.tokenize(prompt, true, false).expect("tokenize");
    let prompt_len = prompt_tokens.len();
    println!("Prompt: {} tokens", prompt_len);

    // Prefill
    let prefill_start = Instant::now();
    let mut batch = Batch::allocated(512, 1);
    let last_idx = (prompt_tokens.len() - 1) as i32;
    for (i, tok) in prompt_tokens.iter().enumerate() {
        batch.push(*tok, i as i32, &[0], i as i32 == last_idx);
    }
    ctx.decode(&batch).expect("prefill decode");
    let prefill_elapsed = prefill_start.elapsed();
    let prefill_tok_s = prompt_len as f64 / prefill_elapsed.as_secs_f64();
    println!(
        "Prefill: {} tokens in {:.3}s = {:.1} tok/s",
        prompt_len,
        prefill_elapsed.as_secs_f64(),
        prefill_tok_s
    );

    // Generate N tokens
    let mut sampler = Sampler::greedy();
    let mut n_decoded = 0;
    let mut n_cur = batch.n_tokens();
    let gen_start = Instant::now();
    let mut output = String::new();

    for _ in 0..n_tokens {
        let token = sampler.sample(&ctx, -1);
        if model.is_eog_token(token) {
            break;
        }
        output.push_str(&model.token_to_piece(token));

        batch.clear();
        batch.push(token, n_cur, &[0], true);
        ctx.decode(&batch).expect("gen decode");

        n_cur += 1;
        n_decoded += 1;
    }

    let gen_elapsed = gen_start.elapsed();
    let gen_tok_s = n_decoded as f64 / gen_elapsed.as_secs_f64();
    println!(
        "Generation: {} tokens in {:.3}s = {:.1} tok/s",
        n_decoded,
        gen_elapsed.as_secs_f64(),
        gen_tok_s
    );
    println!("\n--- Output ---\n{}\n--- End ---", output);
    println!(
        "\nSummary:  prefill={:.1} tok/s  generation={:.1} tok/s",
        prefill_tok_s, gen_tok_s
    );
}
