//! Diagnostic: does the Metal build produce IDENTICAL token output to the
//! CPU build, given the same prompt + greedy sampler + same seed?
//!
//! Greedy sampling is fully deterministic: highest-logit token wins, no RNG.
//! If two backends compute the same logits to the same precision, they emit
//! the same token IDs. So:
//!
//!   GPU == CPU output  ⇒ Metal kernels are mathematically correct;
//!                          any "garbage" output we see in chat is from
//!                          OUR sampler config / chat template, not Metal.
//!   GPU != CPU output  ⇒ Metal kernel bug producing wrong logits;
//!                          this would be the major bug.
//!
//! Run:
//!   cargo test --release --test qwen35_cpu_vs_gpu_diff -- --ignored --nocapture

use llama::{Batch, ContextParams, Model, ModelParams, Sampler};
use std::path::PathBuf;

const MODEL_PATH: &str = "/Users/joelteply/.docker/models/bundles/sha256/18055fe8ee379b95f4af3cf420588c5daa28f2a1ce1da335112a2d1ea188d3e6/model/model.gguf";
const PROMPT: &str = "Q: What is twelve times seven? A:";
const N_GENERATE: usize = 32;

fn run(n_gpu_layers: i32, label: &str) -> Vec<i32> {
    let model = Model::load(
        PathBuf::from(MODEL_PATH),
        ModelParams {
            n_gpu_layers,
            use_mmap: true,
        },
    )
    .expect("load");
    let mut ctx = model
        .new_context(ContextParams {
            n_ctx: 4096,
            n_batch: 512,
            n_seq_max: 1,
            ..Default::default()
        })
        .expect("ctx");

    let prompt_tokens = model.tokenize(PROMPT, true, false).expect("tokenize");
    let mut batch = Batch::allocated(512, 1);
    let last = (prompt_tokens.len() - 1) as i32;
    for (i, t) in prompt_tokens.iter().enumerate() {
        batch.push(*t, i as i32, &[0], i as i32 == last);
    }
    ctx.decode(&batch).expect("prefill");

    let mut sampler = Sampler::greedy();
    let mut out: Vec<i32> = Vec::with_capacity(N_GENERATE);
    let mut pos = batch.n_tokens();
    let mut text = String::new();
    for _ in 0..N_GENERATE {
        let tok = sampler.sample(&ctx, -1);
        sampler.accept(tok);
        if model.is_eog_token(tok) {
            break;
        }
        text.push_str(&model.token_to_piece(tok));
        out.push(tok);
        batch.clear();
        batch.push(tok, pos, &[0], true);
        ctx.decode(&batch).expect("gen");
        pos += 1;
    }
    eprintln!("[{label}] tokens={} text={:?}", out.len(), text);
    out
}

#[test]
#[ignore = "requires local GGUF; run with --ignored --nocapture"]
fn qwen35_cpu_vs_gpu_greedy_diff() {
    let cpu = run(0, "CPU");
    let gpu = run(-1, "GPU");
    assert_eq!(cpu.len(), gpu.len(), "different output lengths");
    let first_diff = cpu.iter().zip(gpu.iter()).position(|(a, b)| a != b);
    match first_diff {
        None => eprintln!(
            "\n✅ CPU and GPU produced IDENTICAL {} tokens — Metal kernels mathematically correct.",
            cpu.len()
        ),
        Some(i) => {
            eprintln!(
                "\n❌ CPU vs GPU DIVERGE at token {i}: CPU={} GPU={}",
                cpu[i], gpu[i]
            );
            eprintln!("   CPU tokens: {:?}", &cpu[..(i + 1).min(cpu.len())]);
            eprintln!("   GPU tokens: {:?}", &gpu[..(i + 1).min(gpu.len())]);
            panic!("Metal kernels produce different output than CPU — major bug");
        }
    }
}
