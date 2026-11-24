//! The persona engine on a real model: N minds seated at once in the Core's pool (CONTINUUM-ENGINE
//! 4.3) through `PersonaEngine` over the `UnifiedTier`, one decoding at a time. Each mind is seated
//! once; a turn only makes the next mind active, so a switch moves nothing. Reports the switch and the
//! decode rate, separately and together.
//!
//! cargo run --release --features metal,test-fixtures --example persona_switch -- <model.gguf> <minds> <prompt_tokens> <decode_tokens> <rounds>

use std::sync::Arc;
use std::time::{Duration, Instant};

use continuum_persona_engine::engine::PersonaEngine;
use continuum_persona_engine::persona::{Epoch, MindId, PersonaId, PersonaParts, PersonaUnit};
use continuum_persona_engine::residency::{Direction, Execute, Tier};
use continuum_persona_llama::{base_genes, Blocks, Step, UnifiedTier};
use llama::{Batch, ContextParams, FlashAttn, KvLayout, Model, ModelParams, Pos};

/// Where her decode stands: the next position and her last sampled token.
#[derive(Default)]
struct Cursor {
    pos: Pos,
    last: i32,
}

/// One mind in the run: the unit that travels (CONTINUUM-ENGINE 4.17), carrying her blocks, her
/// prompt and where her decode stands.
type Mind = PersonaUnit<Blocks, Vec<i32>, (), (), Cursor>;

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

/// Codex on #6134: a tier wrapping a context already in use must not report a first bind over the
/// state another sequence holds there. One token is decoded into sequence 0 before the tier exists; a
/// new mind's move in must be refused with that state untouched, and succeed once the stream is empty.
fn adopted_context_is_refused(model: &Model, ctx: &mut llama::Context<'_>) -> Result<(), String> {
    let token = *model.tokenize("held", true, false)?.first().ok_or("no token for the probe")?;
    let mut batch = Batch::allocated(1, 1);
    batch.push(token, Pos::ZERO, &[0], false);
    ctx.decode(&batch)?;
    let mind = MindId { persona: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1) };
    {
        let mut tier = UnifiedTier::new(ctx);
        let mut blocks = tier.blocks_for(mind).map_err(|e| e.to_string())?;
        if tier.begin(mind, &mut blocks, Direction::In).is_ok() {
            return Err("adoption: a new mind was bound over sequence 0's state".into());
        }
    }
    if ctx.memory_seq_pos_max(0) != Some(Pos::ZERO) {
        return Err("adoption: the refused move changed sequence 0's state".into());
    }
    ctx.memory_seq_clear(0);
    {
        let mut tier = UnifiedTier::new(ctx);
        let mut blocks = tier.blocks_for(mind).map_err(|e| e.to_string())?;
        if tier.begin(mind, &mut blocks, Direction::In).is_err() {
            return Err("adoption: a new mind was refused an empty stream".into());
        }
    }
    ctx.memory_seq_clear(0);
    println!("persona_switch: adoption: a new mind is refused over held state and bound on an empty stream");
    Ok(())
}

/// Codex on #6134 (de0e34a97): a refused move must not cost her the blocks she had. Epoch 1 decodes and
/// is saved into the blocks her unit carries; another sequence's state lands on her stream (through the
/// raw context, as an adopted context would); epoch 2's fresh blocks are refused there; once the stream
/// is clear epoch 1's blocks bind back at the position she left (her byte-identical continuation after a
/// bind is the fork's own test, test-persona-blocks).
fn a_refused_newer_epoch_keeps_her_parked_blocks(model: &Model, ctx: &mut llama::Context<'_>) -> Result<(), String> {
    let tokens = model.tokenize("She keeps her own notes.", true, false)?;
    let her = PersonaId(uuid::Uuid::new_v4());
    let (first_gen, second_gen) = (MindId { persona: her, epoch: Epoch(1) }, MindId { persona: her, epoch: Epoch(2) });
    let mut tier = UnifiedTier::new(ctx);
    let mut kept = tier.blocks_for(first_gen).map_err(|e| e.to_string())?;
    tier.begin(first_gen, &mut kept, Direction::In).map_err(|e| e.to_string())?;
    let seq = tier.stream_of(first_gen).ok_or("replay: she is not bound")?.seq();
    tier.submit(first_gen, &Step { tokens: tokens.clone(), from: Pos::ZERO, logits_at_end: false, genes: base_genes() }).map_err(|e| e.to_string())?;
    tier.settle().map_err(|e| e.to_string())?;
    let held_to = tier.position_of(first_gen).ok_or("replay: she is not bound after her decode")?;
    tier.begin(first_gen, &mut kept, Direction::Out).map_err(|e| e.to_string())?;
    let first = *tokens.first().ok_or("replay: no tokens")?;
    let mut other = Batch::allocated(1, 1);
    other.push(first, Pos::ZERO, &[seq], false);
    tier.raw_context().decode(&other)?;
    let mut newer = tier.blocks_for(second_gen).map_err(|e| e.to_string())?;
    if tier.begin(second_gen, &mut newer, Direction::In).is_ok() {
        return Err("replay: epoch 2 was bound over another sequence's state".into());
    }
    tier.raw_context().memory_seq_clear(seq);
    tier.begin(first_gen, &mut kept, Direction::In).map_err(|e| format!("replay: her parked epoch 1 was lost: {e}"))?;
    let back = tier.position_of(first_gen).ok_or("replay: she is not bound again")?;
    if back != held_to {
        return Err(format!("replay: epoch 1 came back holding {back} instead of {held_to}"));
    }
    tier.begin(first_gen, &mut kept, Direction::Out).map_err(|e| e.to_string())?;
    println!("persona_switch: replay: a refused newer epoch kept her parked epoch 1, which bound back at position {held_to}");
    Ok(())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let model_path = args.get(1).ok_or("usage: persona_switch <model.gguf> <minds> <prompt_tokens> <decode_tokens> <rounds>")?;
    let (minds, prompt_tokens, decode_tokens, rounds) =
        (arg(&args, 2, "minds")?, arg(&args, 3, "prompt_tokens")?, arg(&args, 4, "decode_tokens")?, arg(&args, 5, "rounds")?);

    llama::backend_init();
    let model = Model::load(model_path, ModelParams::default())?;
    // the Core's pool: every mind seated in it at once, each in place
    let window = (prompt_tokens + decode_tokens * rounds + 64) as u32;
    let seats = minds as u32;
    let params = ContextParams { n_ctx: window * seats, n_batch: 2048, n_ubatch: 2048, n_seq_max: seats, kv_layout: KvLayout::Pool, flash_attn: FlashAttn::Enabled, ..ContextParams::default() };
    let mut ctx = model.new_context(params)?;
    adopted_context_is_refused(&model, &mut ctx)?;
    a_refused_newer_epoch_keeps_her_parked_blocks(&model, &mut ctx)?;
    let mut engine: PersonaEngine<Mind, UnifiedTier<'_, '_>> = PersonaEngine::new(UnifiedTier::new(&mut ctx));
    let genes = base_genes();

    let mut parked: Vec<Mind> = (0..minds)
        .map(|i| {
            let text = format!("Mind {i} keeps her own notes. ").repeat(prompt_tokens / 6 + 1);
            let mut prompt = model.tokenize(&text, true, false)?;
            prompt.truncate(prompt_tokens);
            let id = MindId { persona: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1) };
            let blocks = engine.blocks_for(id).map_err(|e| e.to_string())?;
            Ok(Mind::new(id.persona, id.epoch, PersonaParts { blocks, tokens: prompt, genome: (), learning: (), decode: Cursor::default() }))
        })
        .collect::<Result<_, String>>()?;

    // every mind seated once: a new mind claims an empty sequence, nothing copied
    let order: Vec<PersonaId> = parked.iter().map(|mind| mind.id()).collect();
    for mind in parked.drain(..) {
        engine.stage(mind).map_err(|r| r.reason.to_string())?;
        engine.seat().map_err(|e| e.to_string())?;
    }

    let (mut switches, mut decode_time, mut decoded, mut prefill_time) = (Vec::new(), Duration::ZERO, 0usize, Duration::ZERO);
    let run = Instant::now();
    for turn in 0..minds * rounds {
        // the next mind in turn order (the scheduler's choice in the engine; round-robin here)
        let next = order[turn % minds];
        let t = Instant::now();
        // seated, she is only made active: nothing moves
        engine.activate(next).map_err(|e| e.to_string())?;
        let switched = t.elapsed();
        if turn > 0 {
            switches.push(switched);
        }

        let (pos, prompt) = {
            let mind = engine.active_mut().ok_or("no active mind after handoff")?;
            let parts = mind.parts();
            (parts.decode.pos, if parts.decode.pos == Pos::ZERO { parts.tokens.clone() } else { Vec::new() })
        };
        let mut pos = pos;
        let mut last = engine.active_mut().map(|mind| mind.parts().decode.last).ok_or("no active mind")?;
        if !prompt.is_empty() {
            let t = Instant::now();
            for chunk in prompt.chunks(2048) {
                engine.submit(&Step { tokens: chunk.to_vec(), from: pos, logits_at_end: true, genes: Arc::clone(&genes) }).map_err(|e| e.to_string())?;
                pos = pos.advanced(chunk.len()).map_err(|e| e.to_string())?;
                last = argmax(engine.tier().logits_ith(chunk.len() as i32 - 1));
            }
            prefill_time += t.elapsed();
        }
        let t = Instant::now();
        for _ in 0..decode_tokens {
            engine.submit(&Step { tokens: vec![last], from: pos, logits_at_end: true, genes: Arc::clone(&genes) }).map_err(|e| e.to_string())?;
            pos = pos.advanced(1).map_err(|e| e.to_string())?;
            last = argmax(engine.tier().logits_ith(0));
        }
        decode_time += t.elapsed();
        decoded += decode_tokens;
        let mind = engine.active_mut().ok_or("no active mind after her decode")?;
        mind.parts_mut().decode = Cursor { pos, last };
    }

    switches.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let at = |v: &[Duration], q: f64| v.get(((v.len().max(1) as f64 - 1.0) * q) as usize).copied().map(ms).unwrap_or(0.0); // unwrap_or: an empty class prints 0
    let p = |q: f64| at(&switches, q);
    let decode_tps = decoded as f64 / decode_time.as_secs_f64();
    let switch_total: Duration = switches.iter().sum();
    let sustained = decoded as f64 / (decode_time + switch_total).as_secs_f64();
    println!(
        "persona_switch: minds={minds} prompt={prompt_tokens} decode/turn={decode_tokens} rounds={rounds} turns={} | switch p50 {:.2} ms p90 {:.2} ms max {:.2} ms | decode {decode_tps:.1} tok/s | sustained incl. switches {sustained:.1} tok/s | prefill {:.1} s total | wall {:.1} s",
        minds * rounds,
        p(0.5),
        p(0.9),
        p(1.0),
        prefill_time.as_secs_f64(),
        run.elapsed().as_secs_f64()
    );
    Ok(())
}
