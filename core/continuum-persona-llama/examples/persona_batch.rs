//! Several minds in one engine step, through our own tier (THE-LANE-ON-SAGE.md, open question 1, answered): N minds
//! each bound in her own stream by her attached storage, each prefilled to her depth, then the same teacher-forced
//! tokens decoded two ways on two fresh contexts. Time-shared: one mind's token per step, as the Core decodes today.
//! Batched: every mind's token in one step (`UnifiedTier::submit_batch`). What this catches:
//!   - a batch that gains nothing over time-sharing on this model (each mind's tokens route to different experts):
//!     it prints aggregate tok/s for both;
//!   - a batch that decodes the wrong mind's state into another's step: each step's argmax must agree with the
//!     time-shared arm's (a batch's kernel shapes may shift a near-tie, so agreement is reported, and a mind whose
//!     tokens disagree on more than one step in ten fails the run).
//!
//!   - genes: each mind may wear her own (the adapters after the counts, mind i the (i mod n)-th): one step serves
//!     minds under different genes, each on her own rows (fork #172), and the arms must still agree.
//!
//! cargo run --release --features metal,test-fixtures --example persona_batch -- <model.gguf> <minds> <prompt_tokens> <decode_tokens> [adapter.gguf ...]

use std::sync::Arc;
use std::time::{Duration, Instant};

use continuum_persona_engine::persona::{Epoch, MindId, PersonaId};
use continuum_persona_engine::residency::{Direction, Execute, Tier};
use continuum_persona_llama::{base_genes, Blocks, Genes, KvHome, Step, Tokens, UnifiedTier};
use llama::{ContextParams, FlashAttn, Model, ModelParams, Pos};

fn argmax(logits: &[f32]) -> i32 {
    let mut best = (0usize, f32::MIN);
    for (i, &v) in logits.iter().enumerate() {
        if v > best.1 {
            best = (i, v);
        }
    }
    best.0 as i32
}

fn arg(args: &[String], i: usize, what: &str) -> Result<usize, String> {
    args.get(i).ok_or(format!("missing {what}"))?.parse().map_err(|e| format!("{what}: {e}"))
}

/// What one arm decoded: each step's argmax per mind, and the time its decode took.
struct Arm {
    picks: Vec<Vec<i32>>, // [step][mind]
    took: Duration,
}

fn arm(model: &Model, prompts: &[Vec<i32>], forced: &[i32], wears: &[Genes], batched: bool) -> Result<Arm, String> {
    let minds = prompts.len();
    let window = prompts.iter().map(Vec::len).max().unwrap_or(0) + forced.len() + 64; // unwrap_or: no minds, no window
    let streams = u32::try_from(minds).map_err(|e| e.to_string())?;
    let n_ctx = u32::try_from(window).map_err(|e| e.to_string())?.checked_mul(streams).ok_or("the windows exceed a context")?;
    let params = ContextParams { n_ctx, n_batch: 2048, n_ubatch: 2048, n_seq_max: streams, flash_attn: FlashAttn::Enabled, ..ContextParams::default() };
    let mut ctx = model.new_context(params)?;
    let kv_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (kv_home, _) = continuum_blocks::Instance::join(kv_dir.path(), "persona-batch").map_err(|e| e.to_string())?;
    let mut tier = UnifiedTier::new(&mut ctx, KvHome::Mapped { instance: kv_home });

    // each mind bound in her own stream, prefilled to her depth
    let mut held: Vec<(MindId, Blocks, Pos, Genes)> = Vec::with_capacity(minds);
    for (i, prompt) in prompts.iter().enumerate() {
        let genes = &wears[i];
        let id = MindId { persona: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1) };
        let mut blocks = tier.blocks_for(id).map_err(|e| e.to_string())?;
        let _landed = tier.begin(id, &mut blocks, Direction::In).map_err(|e| e.to_string())?;
        let mut pos = Pos::ZERO;
        for chunk in prompt.chunks(2048) {
            tier.submit(id, Step { tokens: Tokens::copied(chunk), from: pos, logits_at_end: true, genes: Arc::clone(&genes) }).map_err(|e| e.to_string())?;
            pos = pos.advanced(chunk.len()).map_err(|e| e.to_string())?;
        }
        tier.settle().map_err(|e| e.to_string())?;
        held.push((id, blocks, pos, Arc::clone(genes)));
    }

    let mut picks = Vec::with_capacity(forced.len());
    let started = Instant::now();
    for &token in forced {
        let steps: Vec<(MindId, Step)> =
            held.iter().map(|(id, _, pos, genes)| (*id, Step { tokens: Tokens::One(token), from: *pos, logits_at_end: true, genes: Arc::clone(genes) })).collect();
        let mut row = Vec::with_capacity(minds);
        if batched {
            let refs: Vec<(MindId, &Step)> = steps.iter().map(|(id, step)| (*id, step)).collect();
            tier.submit_batch(&refs).map_err(|e| e.to_string())?;
            for i in 0..minds {
                row.push(argmax(tier.logits_ith(i32::try_from(i).map_err(|e| e.to_string())?)));
            }
        } else {
            for (id, step) in steps {
                tier.submit(id, step).map_err(|e| e.to_string())?;
                row.push(argmax(tier.logits_ith(0)));
            }
        }
        picks.push(row);
        for (_, _, pos, _) in &mut held {
            *pos = pos.advanced(1).map_err(|e| e.to_string())?;
        }
    }
    let took = started.elapsed();
    drop(held); // her blocks drop after the tier is done with them: this arm's last decode landed at the read above
    Ok(Arm { picks, took })
}

fn main() -> Result<(), String> {
    run().inspect_err(|why| eprintln!("persona_batch: refused: {why}"))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args.get(1).ok_or("usage: persona_batch <model.gguf> <minds> <prompt_tokens> <decode_tokens> [adapter.gguf ...]")?;
    let (minds, prompt_tokens, decode_tokens) = (arg(&args, 2, "minds")?, arg(&args, 3, "prompt_tokens")?, arg(&args, 4, "decode_tokens")?);
    llama::backend_init();
    let model = Model::load(model_path, ModelParams::default())?;
    let prompts: Vec<Vec<i32>> = (0..minds)
        .map(|i| {
            let text = format!("Mind {i} keeps her own notes about the board. ").repeat(prompt_tokens / 8 + 1);
            let mut prompt = model.tokenize(&text, true, false)?;
            prompt.truncate(prompt_tokens);
            Ok(prompt)
        })
        .collect::<Result<_, String>>()?;
    let forced: Vec<i32> = model.tokenize(&" the card, the claim, the review and the patch".repeat(decode_tokens / 8 + 1), false, false)?.into_iter().take(decode_tokens).collect();

    // each mind's genes: the adapters given after the counts, mind i the (i mod n)-th; none given, every mind the base
    let adapters: Vec<Arc<llama::LoraAdapter>> = args[5..].iter().map(|path| model.load_lora(path).map(Arc::new)).collect::<Result<_, String>>()?;
    let wears: Vec<Genes> = (0..minds)
        .map(|i| if adapters.is_empty() { base_genes() } else { Arc::from(vec![(Arc::clone(&adapters[i % adapters.len()]), 1.0f32)]) })
        .collect();

    let shared = arm(&model, &prompts, &forced, &wears, false)?;
    let batched = arm(&model, &prompts, &forced, &wears, true)?;
    let tps = |arm: &Arm| (forced.len() * minds) as f64 / arm.took.as_secs_f64();
    let mut worst = 0usize;
    for mind in 0..minds {
        let differ = (0..forced.len()).filter(|&step| shared.picks[step][mind] != batched.picks[step][mind]).count();
        worst = worst.max(differ);
    }
    println!(
        "persona_batch: minds={minds} prompt={prompt_tokens} decode={} | time-shared {:.1} tok/s aggregate | batched {:.1} tok/s aggregate ({:.2}x) | per mind {:.1} vs {:.1} tok/s | worst mind's argmax disagreements {worst} of {}",
        forced.len(),
        tps(&shared),
        tps(&batched),
        tps(&batched) / tps(&shared),
        tps(&shared) / minds as f64,
        tps(&batched) / minds as f64,
        forced.len()
    );
    if worst * 10 > forced.len() {
        return Err(format!("a batched mind's tokens disagree with her time-shared ones on {worst} of {} steps: a batch decoded the wrong state", forced.len()));
    }
    Ok(())
}
