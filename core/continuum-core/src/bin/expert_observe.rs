//! expert_observe — glass-box the LIVE MoE expert routing (#230 / #229).
//!
//! Runs a GGUF MoE through the core/llama FFI with a [`LiveExpertObserver`] attached (the
//! already-built `cb_eval` → `ffn_moe_topk` seam in `core/llama/src/safe.rs`), generates
//! N tokens to drive REAL routing, then dumps the affinity data — hot/cold expert
//! distribution + co-occurrence + prefetch prediction. That affinity is the INPUT to
//! expert prefetch (#227), grid placement (#180), compaction, and distillation (#233).
//!
//! WHY this and not the live llama-server path: expert affinity is MODEL-INTRINSIC (which
//! experts co-fire for given inputs), so an in-process FFI run gathers valid data without
//! touching the live-persona serving lane. (The live llama-server path would need a
//! separate fork patch to emit routing; this harness needs neither.)
//!
//! Usage:
//!   cargo run -p continuum-core --features metal,accelerate --bin expert_observe -- <model.gguf> [n_tokens] [prompt]

use std::path::PathBuf;
use std::sync::Arc;

use continuum_core::capacity::expert_observer::LiveExpertObserver;
use llama::{Batch, ContextParams, ExpertObserver, Model, ModelParams, Sampler};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args
        .get(1)
        .expect("usage: expert_observe <model.gguf> [n_tokens] [prompt]");
    let n_tokens: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(96);
    let prompt = args.get(3).map(|s| s.as_str()).unwrap_or(
        "Write a Rust function to reverse a string, then explain how it works step by step:\n",
    );

    // The observer is the sink: `cb_eval` calls `observe(layer, selected_experts, n_used)`
    // per MoE layer per token from inside the compute thread.
    let observer = LiveExpertObserver::new();

    let model = Model::load(
        PathBuf::from(model_path),
        ModelParams {
            n_gpu_layers: -1,
            use_mmap: true,
        },
    )
    .expect("load model");
    println!("Loaded {model_path} (vocab={})", model.n_vocab());

    let mut ctx = model
        .new_context(ContextParams {
            n_ctx: 4096,
            n_batch: 512,
            n_seq_max: 1,
            expert_observer: Some(observer.clone() as Arc<dyn ExpertObserver>),
            ..Default::default()
        })
        .expect("context");

    // Prefill.
    let prompt_tokens = model.tokenize(prompt, true, false).expect("tokenize");
    let mut batch = Batch::allocated(512, 1);
    let last = (prompt_tokens.len() - 1) as i32;
    for (i, tok) in prompt_tokens.iter().enumerate() {
        batch.push(*tok, i as i32, &[0], i as i32 == last);
    }
    ctx.decode(&batch).expect("prefill decode");

    // Generate — every decoded token drives the router → observer tallies real selections.
    let mut sampler = Sampler::greedy();
    let mut n_cur = batch.n_tokens();
    let mut n_decoded = 0usize;
    for _ in 0..n_tokens {
        let token = sampler.sample(&ctx, -1);
        if model.is_eog_token(token) {
            break;
        }
        batch.clear();
        batch.push(token, n_cur, &[0], true);
        ctx.decode(&batch).expect("gen decode");
        n_cur += 1;
        n_decoded += 1;
    }

    // Dump the affinity — the fuel for the whole optimization catalog.
    let total = observer.total_hits();
    let hits = observer.snapshot_hits();
    let (_seen, cooccur) = observer.snapshot_cooccurrence();
    let predicted = observer.predicted();

    println!("\n=== EXPERT AFFINITY (model-intrinsic, {n_decoded} tokens observed) ===");
    println!("total expert activations : {total}");
    println!("distinct experts fired   : {}", hits.len());
    println!("co-occurring pairs seen  : {}", cooccur.len());
    println!("prefetch candidates      : {} experts", predicted.len());

    let mut ranked: Vec<(String, u64)> = hits
        .iter()
        .map(|(k, v)| (format!("{k:?}"), *v))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    let show = ranked.len().min(24);
    println!("\ntop {show} hottest experts (of {} fired):", hits.len());
    for (id, h) in ranked.iter().take(show) {
        let pct = if total > 0 { 100.0 * *h as f64 / total as f64 } else { 0.0 };
        println!("  {id:<28} {h:>8}  ({pct:.2}%)");
    }
    // The tail: how concentrated is activation? (the paging headroom)
    if ranked.len() > show {
        let tail: u64 = ranked.iter().skip(show).map(|(_, h)| *h).sum();
        let tail_pct = if total > 0 { 100.0 * tail as f64 / total as f64 } else { 0.0 };
        println!(
            "  ... {} colder experts share the remaining {tail_pct:.2}% (the page-out tail)",
            ranked.len() - show
        );
    }
}
