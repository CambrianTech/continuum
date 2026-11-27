//! A mind's view-end checkpoint handed over by a stop with her blocks and taken back by a new core, on a
//! real model (CONTINUUM-ENGINE 4.15 and 4.16). Without it a hybrid mind restored with every token she held
//! could not rewind to her view's end, so her first turn after a restart re-prefilled her whole prompt (the
//! M5, 2026-11-27: Aiko shared 214,911 of 246,764 tokens with what she held and prefilled 1,019 s).
//!
//! The old core prefills her view, takes her checkpoint at its end, prefills the turn's tail and decodes,
//! parks her and hands over her checkpoint and her blocks. A new core restores her blocks, refuses her
//! checkpoint of another generation, takes it back of hers, rewinds her to it and prefills a NEW tail: only
//! the tail, never her view. A control mind that never stopped prefills her view and the new tail whole.
//! Every token she decodes after the rewind must equal the control's (next-token continuity), and the
//! prefill she paid after the restart is the tail's, not her view's.
//!
//! cargo run --release --features metal,test-fixtures --example checkpoint_handoff -- <model.gguf> <view_tokens> <tail_tokens> <decode_tokens>

use std::time::Instant;

use continuum_persona_engine::persona::{Epoch, MindId, PersonaId};
use continuum_persona_engine::residency::{Direction, Execute, Tier};
use continuum_persona_llama::{base_genes, Blocks, KvHome, Step, Tokens, UnifiedTier};
use llama::{ContextParams, FlashAttn, Model, ModelParams, PersonaCheckpoint, Pos};

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

/// The most tokens one step submits: the context's batch.
const BATCH: usize = 2048;

/// Where her decode stands: the next position and her last sampled token.
#[derive(Clone, Copy)]
struct Cursor {
    pos: Pos,
    last: i32,
}

/// Prefill `tokens` for a bound mind from `at`.
fn prefill(tier: &mut UnifiedTier<'_, '_>, mind: MindId, tokens: &[i32], at: Cursor) -> Result<Cursor, String> {
    let Cursor { mut pos, mut last } = at;
    for chunk in tokens.chunks(BATCH) {
        tier.submit(mind, Step { tokens: Tokens::copied(chunk), from: pos, logits_at_end: true, genes: base_genes() }).map_err(|e| e.to_string())?;
        tier.settle().map_err(|e| e.to_string())?;
        pos = pos.advanced(chunk.len()).map_err(|e| e.to_string())?;
        last = argmax(tier.logits_ith(chunk.len() as i32 - 1));
    }
    Ok(Cursor { pos, last })
}

/// Decode `n` greedy tokens for a bound mind from `at`.
fn decode(tier: &mut UnifiedTier<'_, '_>, mind: MindId, n: usize, at: Cursor) -> Result<Vec<i32>, String> {
    let Cursor { mut pos, mut last } = at;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        tier.submit(mind, Step { tokens: Tokens::One(last), from: pos, logits_at_end: true, genes: base_genes() }).map_err(|e| e.to_string())?;
        tier.settle().map_err(|e| e.to_string())?;
        pos = pos.advanced(1).map_err(|e| e.to_string())?;
        last = argmax(tier.logits_ith(0));
        out.push(last);
    }
    Ok(out)
}

fn main() -> Result<(), String> {
    run().inspect_err(|why| eprintln!("checkpoint_handoff: refused: {why}"))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args.get(1).ok_or("usage: checkpoint_handoff <model.gguf> <view_tokens> <tail_tokens> <decode_tokens>")?;
    let (view_tokens, tail_tokens, decode_tokens) = (arg(&args, 2, "view_tokens")?, arg(&args, 3, "tail_tokens")?, arg(&args, 4, "decode_tokens")?);
    llama::backend_init();
    let model = Model::load(model_path, ModelParams::default())?;
    let window = (view_tokens + 2 * tail_tokens + 2 * decode_tokens + 64) as u32;
    let params = || ContextParams { n_ctx: window, n_batch: BATCH as u32, n_ubatch: BATCH as u32, n_seq_max: 1, flash_attn: FlashAttn::Enabled, ..ContextParams::default() };
    let tokens_of = |text: &str, n: usize, bos: bool| -> Result<Vec<i32>, String> {
        let mut tokens = model.tokenize(&text.repeat(n / 8 + 1), bos, false)?;
        tokens.truncate(n);
        Ok(tokens)
    };
    let view = tokens_of("She keeps her own notes, and picks up exactly where she left off. ", view_tokens, true)?;
    let (old_tail, new_tail) = (tokens_of("The clock reads nine and the ask is the build. ", tail_tokens, false)?, tokens_of("Now it is ten, and the review came back green. ", tail_tokens, false)?);
    let start = Cursor { pos: Pos::ZERO, last: 0 };

    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (live, preserved) = (root.path().join("live"), root.path().join("preserved"));
    std::fs::create_dir_all(&preserved).map_err(|e| e.to_string())?;

    // the control: one mind, never stopped, her view and the new tail prefilled whole
    let control = {
        let mut ctx = model.new_context(params())?;
        let (home, _) = continuum_blocks::Instance::join(&root.path().join("control"), "control").map_err(|e| e.to_string())?;
        let mut tier = UnifiedTier::new(&mut ctx, KvHome::Mapped { instance: home });
        let mind = MindId { persona: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1) };
        let mut blocks = tier.blocks_for(mind).map_err(|e| e.to_string())?;
        tier.begin(mind, &mut blocks, Direction::In).map_err(|e| e.to_string())?;
        let at = prefill(&mut tier, mind, &view, start)?;
        let at = prefill(&mut tier, mind, &new_tail, at)?;
        decode(&mut tier, mind, decode_tokens, at)?
    };

    // the old core: her view, her checkpoint at its end, the old tail and a decode; then her hand-over
    let her = MindId { persona: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1) };
    let (record, kv, checkpoint_file, generation) = (preserved.join("her.cpb"), preserved.join("her.kv"), preserved.join("her.ckp"), 0x5107_u64);
    let (in_kv, view_end) = {
        let mut ctx = model.new_context(params())?;
        let (home, _) = continuum_blocks::Instance::join(&live, "old").map_err(|e| e.to_string())?;
        let mut tier = UnifiedTier::new(&mut ctx, KvHome::Mapped { instance: home });
        let mut blocks: Blocks = tier.blocks_for(her).map_err(|e| e.to_string())?;
        let mut checkpoint = PersonaCheckpoint::new().ok_or("no memory for a checkpoint")?;
        tier.begin(her, &mut blocks, Direction::In).map_err(|e| e.to_string())?;
        let at = prefill(&mut tier, her, &view, start)?;
        let view_end = tier.checkpoint(her, &mut checkpoint).map_err(|e| e.to_string())?;
        let at = prefill(&mut tier, her, &old_tail, at)?;
        decode(&mut tier, her, decode_tokens, at)?;
        tier.begin(her, &mut blocks, Direction::Out).map_err(|e| e.to_string())?;
        blocks.hand_over_checkpoint(&checkpoint, &checkpoint_file, generation).map_err(|e| e.to_string())?;
        let in_kv = blocks.hand_over(&record, &kv, generation).map_err(|e| e.to_string())?;
        (in_kv, view_end)
    }; // the old core ends
    if view_end as usize + 1 != view.len() {
        return Err(format!("her checkpoint holds through position {view_end}, her view ends at {}", view.len()));
    }
    let kv = in_kv.then_some(kv);

    // the new core: her blocks back, then her checkpoint of this generation only
    let mut ctx = model.new_context(params())?;
    let (home, _) = continuum_blocks::Instance::join(&live, "new").map_err(|e| e.to_string())?;
    let mut tier = UnifiedTier::new(&mut ctx, KvHome::Mapped { instance: home });
    let mut blocks = tier.restore(her, &record, kv.as_deref(), generation).map_err(|e| e.to_string())?;
    let mut checkpoint = PersonaCheckpoint::new().ok_or("no memory for a checkpoint")?;
    if blocks.restore_checkpoint(&mut checkpoint, &checkpoint_file, generation + 1).is_ok() || checkpoint.pos().is_some() {
        return Err("a checkpoint of another generation was taken, or left one taken".into());
    }
    let t = Instant::now();
    blocks.restore_checkpoint(&mut checkpoint, &checkpoint_file, generation).map_err(|e| e.to_string())?;
    if checkpoint.pos() != Some(view_end) {
        return Err(format!("her restored checkpoint holds through {:?}, it was taken at {view_end}", checkpoint.pos()));
    }
    tier.begin(her, &mut blocks, Direction::In).map_err(|e| e.to_string())?;
    tier.rewind(her, &checkpoint).map_err(|e| e.to_string())?;
    let restored_at = Cursor { pos: Pos::ZERO.advanced(view.len()).map_err(|e| e.to_string())?, last: 0 };
    let at = prefill(&mut tier, her, &new_tail, restored_at)?;
    let after = decode(&mut tier, her, decode_tokens, at)?;
    let resumed_ms = t.elapsed().as_secs_f64() * 1e3;
    if after != control {
        let differs = after.iter().zip(&control).position(|(a, b)| a != b);
        return Err(format!("next-token continuity broken after the rewind: first difference at token {differs:?}"));
    }
    println!(
        "checkpoint_handoff: view={} tail={} decode={decode_tokens} | checkpoint {:.1} KB | after the restart she prefilled {} tokens (the tail), not {} | resumed in {resumed_ms:.1} ms | {} tokens equal the control's; another generation refused",
        view.len(),
        new_tail.len(),
        std::fs::metadata(&checkpoint_file).map(|m| m.len()).unwrap_or(0) as f64 / 1e3, // unwrap_or: a file not there reports 0 bytes
        new_tail.len(),
        view.len() + new_tail.len(),
        after.len()
    );
    Ok(())
}
