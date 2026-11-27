//! A mind handed over by a stop and taken back by a new core, on a real model (UNIFIED-PAGING slice 2,
//! custody by rename). A control mind decodes without stopping. The old core runs the same mind halfway,
//! parks her and hands her blocks over: her record written, her KV file synced, unmapped and renamed out
//! of its instance directory; then it ends. A new core joins the same root (reclaiming the old core's
//! directory, which no longer holds her file), refuses a record of another generation with her file left
//! where it lies, restores her (her file mapped in place, verified, then renamed into its own directory),
//! refuses a second restore from the path her file has left, and decodes on. Her every token after the
//! restore must equal the control's: next-token continuity (Codex, 2026-11-25). On a device that cannot
//! view host memory (CUDA: her KV in VRAM) her rows travel in her record and no KV file moves; the same
//! generation check and continuity hold.
//!
//! cargo run --release --features metal,test-fixtures --example persona_handoff -- <model.gguf> <prompt_tokens> <decode_tokens>

use std::path::Path;
use std::time::Instant;

use continuum_persona_engine::persona::{Epoch, MindId, PersonaId};
use continuum_persona_engine::residency::{Direction, Execute, Tier};
use continuum_persona_llama::{base_genes, Blocks, KvHome, Step, Tokens, UnifiedTier};
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

/// The most tokens one step submits: the context's batch.
const BATCH: usize = 2048;

/// Where her decode stands: the next position and her last sampled token.
struct Cursor {
    pos: Pos,
    last: i32,
}

/// One turn of hers: the prompt to prefill (empty to go on), where she stands, and how many tokens to decode.
struct Turn<'a> {
    prompt: &'a [i32],
    at: Cursor,
    decode: usize,
}

impl Turn<'_> {
    fn first(prompt: &[i32], decode: usize) -> Turn<'_> {
        Turn { prompt, at: Cursor { pos: Pos::ZERO, last: 0 }, decode }
    }
}

/// Bind `mind`'s blocks, prefill the turn's prompt when given, then decode its greedy tokens. Returns the
/// tokens decoded and where she stands, her blocks parked again (her decode landed, her save done).
fn run_turn(tier: &mut UnifiedTier<'_, '_>, mind: MindId, blocks: &mut Blocks, turn: Turn<'_>) -> Result<(Vec<i32>, Cursor), String> {
    tier.begin(mind, blocks, Direction::In).map_err(|e| e.to_string())?;
    let Turn { prompt, at: Cursor { mut pos, mut last }, decode: n } = turn;
    for chunk in prompt.chunks(BATCH) {
        tier.submit(mind, Step { tokens: Tokens::copied(chunk), from: pos, logits_at_end: true, genes: base_genes() }).map_err(|e| e.to_string())?;
        tier.settle().map_err(|e| e.to_string())?;
        pos = pos.advanced(chunk.len()).map_err(|e| e.to_string())?;
        last = argmax(tier.logits_ith(chunk.len() as i32 - 1));
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        tier.submit(mind, Step { tokens: Tokens::One(last), from: pos, logits_at_end: true, genes: base_genes() }).map_err(|e| e.to_string())?;
        tier.settle().map_err(|e| e.to_string())?;
        pos = pos.advanced(1).map_err(|e| e.to_string())?;
        last = argmax(tier.logits_ith(0));
        out.push(last);
    }
    tier.begin(mind, blocks, Direction::Out).map_err(|e| e.to_string())?;
    Ok((out, Cursor { pos, last }))
}

fn main() -> Result<(), String> {
    run().inspect_err(|why| eprintln!("persona_handoff: refused: {why}"))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args.get(1).ok_or("usage: persona_handoff <model.gguf> <prompt_tokens> <decode_tokens>")?;
    let (prompt_tokens, decode_tokens) = (arg(&args, 2, "prompt_tokens")?, arg(&args, 3, "decode_tokens")?);
    llama::backend_init();
    let model = Model::load(model_path, ModelParams::default())?;
    let window = (prompt_tokens + 2 * decode_tokens + 64) as u32;
    let params = || ContextParams { n_ctx: window, n_batch: BATCH as u32, n_ubatch: BATCH as u32, n_seq_max: 1, flash_attn: FlashAttn::Enabled, ..ContextParams::default() };
    let text = "She keeps her own notes, and picks up exactly where she left off. ".repeat(prompt_tokens / 12 + 1);
    let mut prompt = model.tokenize(&text, true, false)?;
    prompt.truncate(prompt_tokens);

    let root = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (live, preserved) = (root.path().join("live"), root.path().join("preserved"));
    std::fs::create_dir_all(&preserved).map_err(|e| e.to_string())?;

    // the control: one mind, never stopped
    let control = {
        let mut ctx = model.new_context(params())?;
        let (home, _) = continuum_blocks::Instance::join(&root.path().join("control"), "control").map_err(|e| e.to_string())?;
        let mut tier = UnifiedTier::new(&mut ctx, KvHome::Mapped { instance: home });
        let mind = MindId { persona: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1) };
        let mut blocks = tier.blocks_for(mind).map_err(|e| e.to_string())?;
        run_turn(&mut tier, mind, &mut blocks, Turn::first(&prompt, 2 * decode_tokens))?.0
    };

    // the old core: her first half, then her hand-over at its stop
    let her = MindId { persona: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1) };
    let (record, kv, generation) = (preserved.join("her.cpb"), preserved.join("her.kv"), 0x5107_u64);
    // Where her rows travel is the device's to say: a device that views host memory keeps them in her
    // mapped file, which moves by rename; one that cannot (CUDA: her KV in VRAM) writes them into her
    // record, and no KV file exists to move.
    let (first_half, at, handed_ms, in_kv) = {
        let mut ctx = model.new_context(params())?;
        let (home, _) = continuum_blocks::Instance::join(&live, "old").map_err(|e| e.to_string())?;
        let mut tier = UnifiedTier::new(&mut ctx, KvHome::Mapped { instance: home });
        let mut blocks = tier.blocks_for(her).map_err(|e| e.to_string())?;
        let (tokens, at) = run_turn(&mut tier, her, &mut blocks, Turn::first(&prompt, decode_tokens))?;
        let t = Instant::now();
        let in_kv = blocks.hand_over(&record, &kv, generation).map_err(|e| e.to_string())?;
        let handed_ms = t.elapsed().as_secs_f64() * 1e3;
        if in_kv != kv.exists() {
            return Err(format!("hand-over said her rows are in a KV file: {in_kv}, and a KV file at the preserved path: {}", kv.exists()));
        }
        (tokens, at, handed_ms, in_kv)
    }; // the old core ends: its context, its tier and its instance lease go
    let kv = in_kv.then_some(kv);
    if first_half != control[..decode_tokens] {
        return Err("her first half differs from the control's before any hand-over".into());
    }

    // the new core: joins the same root, so the old core's directory is reclaimed
    let mut ctx = model.new_context(params())?;
    let (home, reclaimed) = continuum_blocks::Instance::join(&live, "new").map_err(|e| e.to_string())?;
    if reclaimed.instances != 1 || reclaimed.bytes != 0 {
        return Err(format!("the old core's directory should be reclaimed empty, her file handed out of it: {reclaimed:?}"));
    }
    let mut tier = UnifiedTier::new(&mut ctx, KvHome::Mapped { instance: home });
    if tier.restore(her, &record, kv.as_deref(), generation + 1).is_ok() {
        return Err("a record of another generation was taken".into());
    }
    if kv.as_ref().is_some_and(|kv| !kv.exists()) {
        return Err("a refused restore moved her file off the manifest's path".into());
    }
    let t = Instant::now();
    let mut blocks = tier.restore(her, &record, kv.as_deref(), generation).map_err(|e| e.to_string())?;
    let restore_ms = t.elapsed().as_secs_f64() * 1e3;
    if let Some(kv) = &kv {
        if kv.exists() {
            return Err("her restored file is still at the manifest's path: a crash now would restore a changed file".into());
        }
        let stale = MindId { persona: her.persona, epoch: Epoch(1) };
        if tier.restore(stale, &record, Some(kv), generation).is_ok() {
            return Err("a second restore from the path her file left was taken".into());
        }
    }
    let (second_half, _) = run_turn(&mut tier, her, &mut blocks, Turn { prompt: &[], at, decode: decode_tokens })?;
    if second_half != control[decode_tokens..] {
        let differs = second_half.iter().zip(&control[decode_tokens..]).position(|(a, b)| a != b);
        return Err(format!("next-token continuity broken after the restore: first difference at token {differs:?}"));
    }
    let size = |path: &Path| std::fs::metadata(path).map(|m| m.len()).unwrap_or(0); // unwrap_or: a file not there is 0 bytes in the report
    let rows = match kv {
        Some(_) => format!("her rows in her KV file, {:.1} MB moved by rename; the path her file left refused", blocks_bytes(&live) as f64 / 1e6),
        None => "her rows in her record (device KV)".to_owned(),
    };
    println!(
        "persona_handoff: prompt={prompt_tokens} decode={decode_tokens}+{decode_tokens} | hand-over {handed_ms:.2} ms, restore {restore_ms:.2} ms | record {:.1} KB | {rows} | {} tokens after the restore equal the control's; another generation refused in place",
        size(&record) as f64 / 1e3,
        second_half.len()
    );
    Ok(())
}

/// The bytes of the KV files under `live` (the new core's directory now holds hers).
fn blocks_bytes(live: &Path) -> u64 {
    let Ok(dirs) = std::fs::read_dir(live) else { return 0 };
    dirs.flatten()
        .filter_map(|dir| std::fs::read_dir(dir.path()).ok())
        .flat_map(|files| files.flatten())
        .filter_map(|file| file.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}
