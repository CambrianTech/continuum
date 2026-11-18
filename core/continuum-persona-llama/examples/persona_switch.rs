//! The persona engine on a real model: N minds take turns through `Roles` and the `UnifiedTier`, one
//! decoding at a time. Each turn stages the next mind (her copy-in), hands off, retires the previous
//! mind (her copy-out to RAM), then decodes. Reports the switch (handoff + copy-out + copy-in) and the
//! decode rate, separately and together.
//!
//! cargo run --release --features metal --example persona_switch -- <model.gguf> <minds> <prompt_tokens> <decode_tokens> <rounds>

use std::time::{Duration, Instant};

use continuum_persona_engine::persona::{Epoch, PersonaId};
use continuum_persona_engine::pipeline::{Identified, Roles};
use continuum_persona_engine::residency::{Direction, FenceTag, Tier};
use continuum_persona_llama::{Landed, Rebind, UnifiedTier};
use llama::{Batch, ContextParams, FlashAttn, Model, ModelParams};

/// One mind in the run: who she is, where her history ends, and her last sampled token.
struct Mind {
    id: PersonaId,
    epoch: Epoch,
    prompt: Vec<i32>,
    pos: i32,
    last: i32,
}

impl Identified for Mind {
    fn persona(&self) -> PersonaId {
        self.id
    }
    fn epoch(&self) -> Epoch {
        self.epoch
    }
}

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
    batch.push(token, 0, &[0], false);
    ctx.decode(&batch)?;
    let mind = FenceTag { persona: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1), direction: Direction::In };
    {
        let mut tier = UnifiedTier::new(ctx);
        if tier.begin(mind).is_ok() {
            return Err("adoption: a new mind was bound over sequence 0's state".into());
        }
    }
    if ctx.memory_seq_pos_max(0) != 0 {
        return Err("adoption: the refused move changed sequence 0's state".into());
    }
    ctx.memory_seq_rm(0, -1, -1);
    if UnifiedTier::new(ctx).begin(mind).is_err() {
        return Err("adoption: a new mind was refused an empty stream".into());
    }
    ctx.memory_seq_rm(0, -1, -1);
    println!("persona_switch: adoption: a new mind is refused over held state and bound on an empty stream");
    Ok(())
}

/// Codex on #6134 (de0e34a97): a refused move must not cost her the blocks she had. Epoch 1 decodes and
/// parks in RAM; another sequence's state lands on her stream; epoch 2's move in is refused there; once
/// the stream is clear epoch 1 binds back at the position she left (her byte-identical continuation
/// after a bind is the fork's own test, test-persona-blocks).
fn a_refused_newer_epoch_keeps_her_parked_blocks(model: &Model, ctx: &mut llama::Context<'_>) -> Result<(), String> {
    let tokens = model.tokenize("She keeps her own notes.", true, false)?;
    let her = PersonaId(uuid::Uuid::new_v4());
    let tag = |epoch, direction| FenceTag { persona: her, epoch: Epoch(epoch), direction };
    let mut tier = UnifiedTier::new(ctx);
    tier.begin(tag(1, Direction::In)).map_err(|e| e.to_string())?;
    let seq = tier.stream_of(her).ok_or("replay: she is not bound")?.seq();
    let mut batch = Batch::allocated(tokens.len() as i32, 1);
    for (i, &t) in tokens.iter().enumerate() {
        batch.push(t, i as i32, &[seq], false);
    }
    tier.context().decode(&batch)?;
    let held_to = tier.context().memory_seq_pos_max(seq);
    tier.begin(tag(1, Direction::Out)).map_err(|e| e.to_string())?;
    let first = *tokens.first().ok_or("replay: no tokens")?;
    let mut other = Batch::allocated(1, 1);
    other.push(first, 0, &[seq], false);
    tier.context().decode(&other)?;
    if tier.begin(tag(2, Direction::In)).is_ok() {
        return Err("replay: epoch 2 was bound over another sequence's state".into());
    }
    tier.context().memory_seq_rm(seq, -1, -1);
    tier.begin(tag(1, Direction::In)).map_err(|e| format!("replay: her parked epoch 1 was lost: {e}"))?;
    let back = tier.stream_of(her).ok_or("replay: she is not bound again")?.seq();
    if tier.context().memory_seq_pos_max(back) != held_to {
        return Err(format!("replay: epoch 1 came back holding {} instead of {held_to}", tier.context().memory_seq_pos_max(back)));
    }
    tier.begin(tag(1, Direction::Out)).map_err(|e| e.to_string())?;
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
    // two execution streams (active + next); each holds one mind's whole window
    let window = (prompt_tokens + decode_tokens * rounds + 64) as u32;
    let params = ContextParams { n_ctx: window * 2, n_batch: 2048, n_ubatch: 2048, n_seq_max: 2, flash_attn: FlashAttn::Enabled, ..ContextParams::default() };
    let mut ctx = model.new_context(params)?;
    adopted_context_is_refused(&model, &mut ctx)?;
    a_refused_newer_epoch_keeps_her_parked_blocks(&model, &mut ctx)?;
    let mut tier = UnifiedTier::new(&mut ctx);

    let mut parked: Vec<Mind> = (0..minds)
        .map(|i| {
            let text = format!("Mind {i} keeps her own notes. ").repeat(prompt_tokens / 6 + 1);
            let mut prompt = model.tokenize(&text, true, false)?;
            prompt.truncate(prompt_tokens);
            Ok(Mind { id: PersonaId(uuid::Uuid::new_v4()), epoch: Epoch(1), prompt, pos: 0, last: 0 })
        })
        .collect::<Result<_, String>>()?;

    let mut roles: Roles<Mind, Landed> = Roles::default();
    let (mut same_seq, mut other_seq) = (Vec::new(), Vec::new());
    let (mut switches, mut decode_time, mut decoded, mut prefill_time) = (Vec::new(), Duration::ZERO, 0usize, Duration::ZERO);
    let run = Instant::now();
    for turn in 0..minds * rounds {
        // the next mind in turn order (the scheduler's choice in the engine; round-robin here)
        let next = parked.remove(0);
        let t = Instant::now();
        let fence = tier.begin(FenceTag { persona: next.id, epoch: next.epoch, direction: Direction::In }).map_err(|e| e.to_string())?;
        let rebind = tier.last_rebind();
        roles.stage(next, fence).map_err(|r| format!("{:?}", r.reason))?;
        roles.handoff().map_err(|e| format!("{e:?}"))?;
        if let Some(out) = roles.awaiting_copy_out() {
            let fence = tier.begin(out).map_err(|e| e.to_string())?;
            roles.begin_retire(fence).map_err(|r| format!("{:?}", r.reason))?;
            parked.push(roles.release_retired().map_err(|e| format!("{e:?}"))?);
        }
        let switched = t.elapsed();
        if turn > 0 {
            switches.push(switched);
            match rebind {
                Some(Rebind::SameSequence) => same_seq.push(switched),
                Some(Rebind::OtherSequence) => other_seq.push(switched),
                None => {}
            }
        }

        let mind = roles.active_mut().ok_or("no active mind after handoff")?;
        let seq = tier.stream_of(mind.id).ok_or("the active mind is not bound")?.seq();
        let ctx = tier.context();
        if mind.pos == 0 {
            let t = Instant::now();
            let mut last_index = 0;
            for chunk in mind.prompt.chunks(2048) {
                let mut batch = Batch::allocated(chunk.len() as i32, 1);
                for (j, &tok) in chunk.iter().enumerate() {
                    batch.push(tok, mind.pos + j as i32, &[seq], j + 1 == chunk.len());
                }
                ctx.decode(&batch)?;
                mind.pos += chunk.len() as i32;
                last_index = chunk.len() as i32 - 1;
            }
            mind.last = argmax(ctx.logits_ith(last_index));
            prefill_time += t.elapsed();
        }
        let t = Instant::now();
        for _ in 0..decode_tokens {
            let mut batch = Batch::allocated(1, 1);
            batch.push(mind.last, mind.pos, &[seq], true);
            ctx.decode(&batch)?;
            mind.pos += 1;
            mind.last = argmax(ctx.logits_ith(0));
        }
        decode_time += t.elapsed();
        decoded += decode_tokens;
    }

    // a stale request for an older epoch of a held mind is refused and leaves her blocks: she still binds
    let probe = parked.first().ok_or("no parked mind to probe")?;
    let stale = tier.begin(FenceTag { persona: probe.id, epoch: Epoch(0), direction: Direction::In });
    let kept = tier.begin(FenceTag { persona: probe.id, epoch: probe.epoch, direction: Direction::In }).is_ok();
    println!("persona_switch: stale epoch refused={} her blocks still bind={kept}", stale.is_err());
    switches.sort();
    same_seq.sort();
    other_seq.sort();
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
    println!(
        "persona_switch: rebinds (binds of a mind saved before) same sequence n={} p50 {:.2} ms | other sequence (cells relabeled over her span) n={} p50 {:.2} ms; stage, handoff and copy-out run serially before her decode, not overlapped",
        same_seq.len(),
        at(&same_seq, 0.5),
        other_seq.len(),
        at(&other_seq, 0.5)
    );
    Ok(())
}
