//! In-process engine benchmark: the same three measurements as
//! `tools/bench/decode_ab/bench.py` (llama-server), taken through the real
//! continuous-batching `Scheduler` over libllama, so the two engines compare
//! on one model and one set of settings. Prints one JSON line.
//!
//! `--mode turns` adds the many-minds scenario: N minds on n_seq_max slots, each with a
//! growing context (base prompt, then per act a new tail + generated reply), acting
//! round-robin as slots free up. It reports per-act timings and the KV reuse actually
//! achieved, which for this scheduler is none (see run_turns).
//!
//! Why the public `Scheduler` API is enough: every measurement is a function of
//! when a request is submitted and when its tokens arrive, and `TokenEvent`s
//! stream back over a channel we can timestamp on receipt.
//!
//! Known fidelity limits, each from the scheduler's own behavior (not modified here):
//! - It has no ignore-EOS switch, so a run whose sequence ends early is rejected
//!   rather than reported (the rates would not be comparable).
//! - It has no prompt-prefix cache, so the "solo" and "contended" decode requests
//!   re-prefill their prompt; only the generation phase is timed for decode.
//! - Prefill time includes tokenizing the prompt on the scheduler thread.

use std::sync::Arc;
use std::time::{Duration, Instant};

use continuum_engine::inference::backends::llamacpp_scheduler::{
    GenerationRequest, Scheduler, SchedulerConfig, TokenEvent,
};
use continuum_engine::inference::backends::SamplingConfig;
use llama::{FlashAttn, KvCacheType, Model, ModelParams};
use serde_json::json;

/// Everything the flags set. Defaults are the production 27B lane's settings.
struct Args {
    model: String,
    n_seq_max: u32,
    per_seq_ctx: u32,
    ctx_tokens: usize,
    out_tokens: usize,
    ubatch: u32,
    kv: KvCacheType,
    kv_name: String,
    flash_attn: FlashAttn,
    n_gpu_layers: i32,
    turns: bool,
    /// How long after A starts generating B is submitted (bench.py sleeps 1 s, sized for a 27B).
    b_delay_ms: u64,
    minds: usize,
    base_tokens: usize,
    tail_tokens: usize,
    acts: usize,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        model: "/Users/joel/.continuum/genome/models/qwen3.8-27b-gguf/Qwen3.8-27B-Q4_K_M.gguf".into(),
        n_seq_max: 2,
        per_seq_ctx: 112_384,
        ctx_tokens: 81_392,
        out_tokens: 478,
        ubatch: 2048,
        kv: KvCacheType::Q4_0,
        kv_name: "q4_0".into(),
        flash_attn: FlashAttn::Enabled,
        n_gpu_layers: -1,
        turns: false,
        b_delay_ms: 1000,
        minds: 4,
        base_tokens: 20_000,
        tail_tokens: 1_500,
        acts: 3,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut val = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        let bad = |f: &str, v: &str| format!("bad value for {f}: {v}");
        match flag.as_str() {
            "--model" => a.model = val()?,
            "--n-seq-max" => { let v = val()?; a.n_seq_max = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--per-seq-ctx" => { let v = val()?; a.per_seq_ctx = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--ctx-tokens" => { let v = val()?; a.ctx_tokens = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--out-tokens" => { let v = val()?; a.out_tokens = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--ubatch" => { let v = val()?; a.ubatch = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--mode" => {
                let v = val()?;
                a.turns = match v.as_str() {
                    "single" => false,
                    "turns" => true,
                    _ => return Err(bad(&flag, &v)),
                };
            }
            "--b-delay-ms" => { let v = val()?; a.b_delay_ms = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--minds" => { let v = val()?; a.minds = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--base-tokens" => { let v = val()?; a.base_tokens = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--tail-tokens" => { let v = val()?; a.tail_tokens = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--acts" => { let v = val()?; a.acts = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--n-gpu-layers" => { let v = val()?; a.n_gpu_layers = v.parse().map_err(|_| bad(&flag, &v))?; }
            "--kv-type" => {
                let v = val()?;
                a.kv = match v.as_str() {
                    "f16" => KvCacheType::F16,
                    "q8_0" => KvCacheType::Q8_0,
                    "q4_0" => KvCacheType::Q4_0,
                    _ => return Err(bad(&flag, &v)),
                };
                a.kv_name = v;
            }
            "--flash-attn" => {
                let v = val()?;
                a.flash_attn = match v.as_str() {
                    "on" => FlashAttn::Enabled,
                    "off" => FlashAttn::Disabled,
                    "auto" => FlashAttn::Auto,
                    _ => return Err(bad(&flag, &v)),
                };
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if a.n_seq_max < 2 {
        return Err("--n-seq-max must be >= 2: the contended arm needs two sequences".into());
    }
    // The deepest context a run reaches decides whether the window is big enough.
    let deepest = if a.turns {
        a.base_tokens + a.acts * (a.tail_tokens + a.out_tokens)
    } else {
        a.ctx_tokens + a.out_tokens
    };
    if a.per_seq_ctx as usize <= deepest {
        return Err(format!("--per-seq-ctx {} cannot hold the deepest context, {deepest} tokens", a.per_seq_ctx));
    }
    if a.turns && (a.minds == 0 || a.acts == 0) {
        return Err("--minds and --acts must be >= 1".into());
    }
    Ok(a)
}

/// Deterministic ASCII pseudo-source: repeatable across runs and machines, and
/// ASCII so every token piece is whole UTF-8 (pieces concatenate losslessly).
fn synthetic_text(seed: u64, approx_chars: usize) -> String {
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        state >> 33
    };
    let mut out = format!("[{seed}]\n");
    let mut i = 0u64;
    while out.len() < approx_chars {
        let (a, b, c) = (next() % 997, next() % 65_521, next() % 13);
        out.push_str(&format!(
            "fn handler_{i}(x: u64) -> u64 {{ let y = x.wrapping_mul({a}) ^ {b}; if y % {c} == 0 {{ y >> {c} }} else {{ y + {a} }} }}\n"
        ));
        i += 1;
    }
    out
}

/// A prompt that tokenizes (with the scheduler's own flags) to exactly `target`
/// tokens: cut the token stream, detokenize by pieces, retokenize, and correct
/// the cut by the difference until the count matches.
fn prompt_of(model: &Model, target: usize, seed: u64) -> Result<String, String> {
    // Source text is ~3 chars/token at worst; 5x leaves room for the correction loop.
    let ids = model.tokenize(&synthetic_text(seed, target * 5), true, true)?;
    if ids.len() < target + 64 {
        return Err(format!("synthetic text gave {} tokens, need {}", ids.len(), target + 64));
    }
    let mut n = target;
    for _ in 0..8 {
        let text: String = ids[..n].iter().map(|t| model.token_to_piece(*t)).collect();
        let got = model.tokenize(&text, true, true)?.len();
        if got == target {
            return Ok(text);
        }
        n = (n as isize + target as isize - got as isize).clamp(1, ids.len() as isize) as usize;
    }
    Err(format!("could not cut a prompt of exactly {target} tokens"))
}

/// Arrival times of one request's tokens, stamped on receipt.
struct Trace {
    submitted: Instant,
    tokens: Vec<Instant>,
    /// The generated text, so a mind's next prompt can include her own reply.
    text: String,
}

impl Trace {
    fn first(&self) -> Result<Instant, String> {
        self.tokens.first().copied().ok_or_else(|| "request produced no tokens".to_string())
    }
    fn prefill_s(&self) -> Result<f64, String> {
        Ok(self.first()?.duration_since(self.submitted).as_secs_f64())
    }
    /// Generation rate after the first token (which the prefill step samples),
    /// matching llama-server's `predicted_per_second`, which excludes prompt time.
    fn decode_tps(&self) -> Result<f64, String> {
        let span = self.tokens.last().ok_or("no tokens")?.duration_since(self.first()?).as_secs_f64();
        if span <= 0.0 {
            return Err("decode span is zero".into());
        }
        Ok((self.tokens.len() - 1) as f64 / span)
    }
}

/// Submit one request; the returned handle yields its `Trace` once the scheduler
/// signals Done. Rejects a sequence that stopped before `want` tokens (EOG hit).
/// The second return value fires when the first token lands (the prefill is over).
fn submit(
    sched: &Scheduler,
    prompt: String,
    want: usize,
    strict: bool,
) -> Result<(Handle, std::sync::mpsc::Receiver<()>), String> {
    let (first_tx, first_rx) = std::sync::mpsc::channel::<()>();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let submitted = Instant::now();
    sched.enqueue(GenerationRequest {
        prompt,
        max_tokens: want,
        sampling: SamplingConfig {
            temperature: 0.0,
            repeat_penalty: 1.0,
            top_k: 0,
            top_p: 1.0,
            grammar: None,
        },
        stop_sequences: vec![],
        active_loras: vec![],
        response_tx: tx,
        persona_id: None,
    })?;
    let handle = std::thread::Builder::new()
        .name("inproc-bench-rx".into())
        .spawn(move || {
            let mut tokens = Vec::with_capacity(want);
            let mut text = String::new();
            loop {
                match rx.blocking_recv() {
                    Some(TokenEvent::Token(piece)) => {
                        text.push_str(&piece);
                        tokens.push(Instant::now());
                        if tokens.len() == 1 {
                            // A closed receiver only means nobody is waiting on it.
                            let _ = first_tx.send(());
                        }
                    }
                    Some(TokenEvent::Done { tokens_generated, .. }) => {
                        if strict && tokens_generated < want {
                            return Err(format!(
                                "sequence ended at {tokens_generated} of {want} tokens (EOG; the scheduler cannot ignore EOS)"
                            ));
                        }
                        return Ok(Trace { submitted, tokens, text });
                    }
                    Some(TokenEvent::Error(e)) => return Err(format!("scheduler error: {e}")),
                    None => return Err("scheduler dropped the request".into()),
                }
            }
        })
        .map_err(|e| format!("spawn receiver: {e}"))?;
    Ok((handle, first_rx))
}

type Handle = std::thread::JoinHandle<Result<Trace, String>>;

fn join(h: Handle) -> Result<Trace, String> {
    h.join().map_err(|_| "receiver thread panicked".to_string())?
}

fn run(a: &Args) -> Result<serde_json::Value, String> {
    let model = Arc::new(Model::load(&a.model, ModelParams { n_gpu_layers: a.n_gpu_layers, use_mmap: true })?);
    // n_ctx is the pool shared by all sequences, so scale the per-sequence window
    // exactly as the production backend does (llama_cpp_backend.rs scheduler()).
    let sched = Scheduler::spawn(
        model.clone(),
        SchedulerConfig {
            n_ctx: a.per_seq_ctx.saturating_mul(a.n_seq_max),
            n_batch: a.ubatch,
            n_ubatch: a.ubatch,
            n_seq_max: a.n_seq_max,
            flash_attn: a.flash_attn,
            fused_gdn_ar: true,
            fused_gdn_ch: true,
            type_k: a.kv,
            type_v: a.kv,
        },
    );
    // Unique per-process seeds so no two prompts share a prefix.
    let salt = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos() as u64;
    // Untimed warm-up: the first request pays one-off backend/kernel setup that
    // would otherwise land in the cold-prefill number.
    join(submit(&sched, prompt_of(&model, 256, salt.wrapping_add(2))?, 8, false)?.0)?;

    if a.turns {
        return run_turns(a, &model, &sched, salt);
    }
    let decode_prompt = prompt_of(&model, a.ctx_tokens, salt)?;
    let fresh_prompt = prompt_of(&model, a.ctx_tokens, salt.wrapping_add(1))?;

    // 1. Cold prefill: one fresh prompt, time to its first sampled token.
    let cold = join(submit(&sched, decode_prompt.clone(), 1, true)?.0)?;
    let cold_prefill_tps = a.ctx_tokens as f64 / cold.prefill_s()?;

    // 2. Solo decode: the same prompt alone, generating out_tokens.
    let solo = join(submit(&sched, decode_prompt.clone(), a.out_tokens, true)?.0)?;
    let solo_decode_tps = solo.decode_tps()?;

    // 3. Contended: A decodes while B prefills a fresh prompt. B is submitted one
    // second after A starts generating, as bench.py does; A's first token marks
    // the end of its own prefill (the server had A's prompt cached).
    let (a_handle, a_first) = submit(&sched, decode_prompt, a.out_tokens, true)?;
    a_first.recv().map_err(|_| "A ended before its first token".to_string())?;
    std::thread::sleep(Duration::from_millis(a.b_delay_ms));
    let (b_handle, _) = submit(&sched, fresh_prompt, 1, true)?;
    let b = join(b_handle)?;
    let a_trace = join(a_handle)?;

    let b_submit = b.submitted;
    let b_first = b.first()?;
    let window = b_first.duration_since(b_submit).as_secs_f64();
    let overlap_tokens = a_trace.tokens.iter().filter(|t| **t > b_submit && **t <= b_first).count();
    if a_trace.first()? > b_submit {
        return Err("A had not finished prefill when B was submitted; the arms overlapped wrongly".into());
    }
    Ok(json!({
        "engine": "inproc",
        "ubatch": a.ubatch,
        "ctx_tokens": a.ctx_tokens,
        "out_tokens": a.out_tokens,
        "cold_prefill_tps": cold_prefill_tps,
        "solo_decode_tps": solo_decode_tps,
        "solo_decode_uncached_n": a.ctx_tokens,
        "contended_decode_tps": a_trace.decode_tps()?,
        "contended_prefill_tps": a.ctx_tokens as f64 / window,
        // A's rate restricted to B's prefill window, and how many A tokens landed in it:
        // bench.py's definition averages A's whole request, which includes any
        // solo tail after B finishes, so the strict overlap number is reported too.
        "contended_decode_tps_overlap": overlap_tokens as f64 / window,
        "a_tokens_during_b_prefill": overlap_tokens,
        "model": a.model,
        "kv": a.kv_name,
        "n_seq_max": a.n_seq_max,
        "per_seq_ctx": a.per_seq_ctx,
        "at_epoch_s": salt / 1_000_000_000,
    }))
}

/// One finished act of one mind.
struct ActRecord {
    mind: usize,
    act: usize,
    wait_s: f64,
    prompt_tokens: usize,
    prefill_tokens: usize,
    prefill_s: f64,
    decode_s: f64,
    out_tokens: usize,
    act_s: f64,
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// N minds on the scheduler's slots. Each mind's context grows by her own reply plus a
/// fresh tail per act; she acts again only after her previous act finished, and minds
/// are served round-robin as slots free up. At most `n_seq_max` requests are in flight,
/// so `wait_s` (ready to submitted) is time spent queued behind busy slots; the
/// scheduler itself exposes no admission event, and with a free slot it admits at once.
///
/// KV reuse: the scheduler takes a prompt STRING, tokenizes all of it and prefills from
/// position 0 (start_request, llamacpp_scheduler.rs), and frees the sequence's KV on
/// completion (`memory_seq_rm(seq_id, -1, -1)` in phase 5). So a mind's next act always
/// recomputes her whole context: `prefill_tokens` below is the full prompt, measured
/// as the work submitted, not assumed.
fn run_turns(a: &Args, model: &Arc<Model>, sched: &Scheduler, salt: u64) -> Result<serde_json::Value, String> {
    let slots = a.n_seq_max as usize;
    let mut contexts: Vec<String> = (0..a.minds)
        .map(|m| prompt_of(model, a.base_tokens, salt.wrapping_add(100 + m as u64)))
        .collect::<Result<_, _>>()?;
    let mut acts_done = vec![0usize; a.minds];
    let mut ready_at = vec![Instant::now(); a.minds];
    let mut queue: std::collections::VecDeque<usize> = (0..a.minds).collect();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<(usize, Result<Trace, String>, f64, usize)>();
    let mut in_flight = 0usize;
    let mut records: Vec<ActRecord> = Vec::new();
    let total = a.minds * a.acts;
    let mut tail_seed = salt.wrapping_add(10_000);

    while records.len() < total {
        while in_flight < slots {
            let Some(m) = queue.pop_front() else { break };
            let prompt = contexts[m].clone();
            // Counted with the scheduler's own tokenizer flags, so it is what gets prefilled.
            let prompt_tokens = model.tokenize(&prompt, true, true)?.len();
            let wait_s = ready_at[m].elapsed().as_secs_f64();
            // Not strict: an EOG before out_tokens ends that act short and is reported.
            let (handle, _) = submit(sched, prompt, a.out_tokens, false)?;
            let tx = done_tx.clone();
            std::thread::spawn(move || {
                // The receiver only disappears if the run already failed.
                let _ = tx.send((m, join(handle), wait_s, prompt_tokens));
            });
            in_flight += 1;
        }
        let (m, trace, wait_s, prompt_tokens) =
            done_rx.recv().map_err(|_| "all act receivers vanished".to_string())?;
        in_flight -= 1;
        let trace = trace?;
        let last = *trace.tokens.last().ok_or("act produced no tokens")?;
        records.push(ActRecord {
            mind: m,
            act: acts_done[m],
            wait_s,
            prompt_tokens,
            prefill_tokens: prompt_tokens,
            prefill_s: trace.prefill_s()?,
            decode_s: last.duration_since(trace.first()?).as_secs_f64(),
            out_tokens: trace.tokens.len(),
            act_s: wait_s + last.duration_since(trace.submitted).as_secs_f64(),
        });
        acts_done[m] += 1;
        if acts_done[m] < a.acts {
            tail_seed = tail_seed.wrapping_add(1);
            let tail = prompt_of(model, a.tail_tokens, tail_seed)?;
            contexts[m].push_str(&trace.text);
            contexts[m].push('\n');
            contexts[m].push_str(&tail);
            ready_at[m] = Instant::now();
            queue.push_back(m);
        }
    }

    for r in &records {
        println!(
            "{}",
            json!({
                "engine": "inproc", "kind": "act", "mind": r.mind, "act": r.act,
                "wait_s": r.wait_s, "prompt_tokens": r.prompt_tokens,
                "prefill_tokens": r.prefill_tokens, "prefill_s": r.prefill_s,
                "decode_s": r.decode_s, "out_tokens": r.out_tokens, "act_s": r.act_s,
            })
        );
    }
    let mut act_s: Vec<f64> = records.iter().map(|r| r.act_s).collect();
    let mut waits: Vec<f64> = records.iter().map(|r| r.wait_s).collect();
    act_s.sort_by(f64::total_cmp);
    waits.sort_by(f64::total_cmp);
    let prompt_sum: usize = records.iter().map(|r| r.prompt_tokens).sum();
    let computed_sum: usize = records.iter().map(|r| r.prefill_tokens).sum();
    Ok(json!({
        "engine": "inproc", "kind": "summary", "mode": "turns",
        "minds": a.minds, "n_seq_max": a.n_seq_max, "acts_per_mind": a.acts,
        "base_tokens": a.base_tokens, "tail_tokens": a.tail_tokens, "out_tokens": a.out_tokens,
        "ubatch": a.ubatch, "kv": a.kv_name, "model": a.model,
        "act_s_median": percentile(&act_s, 0.5), "act_s_p90": percentile(&act_s, 0.9),
        "wait_s_median": percentile(&waits, 0.5),
        "reuse_ratio": 1.0 - computed_sum as f64 / prompt_sum as f64,
        "kv_kept_between_acts": false,
    }))
}

fn main() {
    let result = parse_args().and_then(|a| run(&a));
    match result {
        Ok(v) => {
            println!("{v}");
            // Leave without running exit handlers: the scheduler thread still owns the Metal
            // context, and process teardown beneath it crashes (rc 138/139) after the result
            // is out. The result is already on stdout (line-flushed), so nothing is lost.
            use std::io::Write;
            let _ = std::io::stdout().flush();
            // SAFETY: _exit takes no pointers and has no preconditions; it only skips destructors.
            unsafe { libc::_exit(0) }
        }
        Err(e) => {
            eprintln!("inproc_bench: {e}");
            std::process::exit(1);
        }
    }
}
