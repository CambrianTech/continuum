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
use continuum_persona_llama::{Landed, UnifiedTier};
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
    let (mut switches, mut decode_time, mut decoded, mut prefill_time) = (Vec::new(), Duration::ZERO, 0usize, Duration::ZERO);
    let run = Instant::now();
    for turn in 0..minds * rounds {
        // the next mind in turn order (the scheduler's choice in the engine; round-robin here)
        let next = parked.remove(0);
        let t = Instant::now();
        let fence = tier.begin(FenceTag { persona: next.id, epoch: next.epoch, direction: Direction::In }).map_err(|e| e.to_string())?;
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

    switches.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let p = |q: f64| switches.get(((switches.len() as f64 - 1.0) * q) as usize).copied().map(ms).unwrap_or(0.0);
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
