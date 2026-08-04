//! Continuous-batching scheduler for llama.cpp.
//!
//! Owns a single `Context` with `n_seq_max = N` and runs a dedicated OS
//! thread that drives one decode call per loop iteration, advancing every
//! active sequence in parallel within that single decode. Replaces the
//! old per-call-context + Rust-side semaphore design where N concurrent
//! generations created N independent contexts that fought for memory
//! bandwidth.
//!
//! Why this is the right shape:
//! - Inference is memory-bound — for each generated token, the model's
//!   weights have to traverse memory once. Independent contexts each
//!   re-read the weights, splitting bandwidth N ways. A shared context
//!   reads the weights ONCE per decode step and uses them to advance
//!   N sequences in parallel via batched matmul (cheap on GPU).
//! - llama.cpp natively supports this via `llama_batch` with multiple
//!   `seq_id` values per token. Setting `n_seq_max=1` (the prior fix
//!   that worked around the "memory slot" bug) was explicitly
//!   constraining us out of the right architecture.
//!
//! Design:
//! - `Scheduler::enqueue(req)` sends a `GenerationRequest` to the driver
//!   thread via std::sync::mpsc.
//! - The request carries a `tokio::sync::mpsc::UnboundedSender<TokenEvent>`
//!   that streams tokens back to the caller as they're sampled.
//! - The driver loop assigns a free `seq_id` (0..n_seq_max), prefills the
//!   prompt in chunks (interleaved with other seqs' generation), then
//!   transitions to generating; each loop iteration emits one token per
//!   active generating seq.
//! - When EOG / max_tokens / stop-sequence hits, the seq is closed:
//!   `Done` event sent, `memory_seq_rm` frees the KV slot, seq_id
//!   returned to free pool.
//!
//! LoRA / genome paging:
//! - Each request carries resolved gene handles (`active_loras`). The driver
//!   applies them **context-level** via `Context::set_loras` right before
//!   `decode`. The shared context applies one gene set per decode, so the
//!   batch builder enforces a **homogeneity gate**: a decode batch adopts the
//!   gene set of the lowest-seq_id seq with work, and seqs wanting a different
//!   set are deferred to a later iteration. A seq is therefore only ever
//!   decoded under its own gene — its entire KV history stays consistent, and
//!   we never silently decode under the wrong adapter (Rule 2). Same-gene
//!   seqs still co-batch (the eval, and the single-gene-per-machine common
//!   case); mixed-gene load serializes by group rather than corrupting.
//! - True per-seq LoRA mixing within one decode (distinct genes co-batched)
//!   is a future optimization; correctness here does not depend on it.
//! - Stop-sequence trimming happens in the caller (we still emit the
//!   stop sequence's tokens before signaling Done) — same as the prior
//!   per-call generate.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use llama::{Batch, ContextParams, FlashAttn, KvCacheType, LoraAdapter, Model, Sampler};
use uuid::Uuid;

use crate::inference::footprint_registry::{self, FootprintKey, ResourceType};
use crate::inference::kv_quant::Residency;
use crate::runtime;

use super::SamplingConfig;

/// Token event streamed from the scheduler to the requester.
#[derive(Debug)]
pub enum TokenEvent {
    /// One generated token piece (UTF-8 fragment from llama tokenizer).
    Token(String),
    /// Generation finished cleanly (EOG / max_tokens / stop sequence).
    Done {
        tokens_generated: usize,
        elapsed_ms: u64,
    },
    /// Generation failed — decode error, batch overflow, etc.
    Error(String),
}

/// What a caller submits to the scheduler.
pub struct GenerationRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub sampling: SamplingConfig,
    pub stop_sequences: Vec<String>,
    /// Genes to apply for this generation: `(id, handle, scale)`. Resolved to
    /// live adapter handles by the backend before enqueue (the scheduler
    /// thread can't lock the backend's cache). `id` is kept for the
    /// homogeneity signature; the `Arc` keeps the adapter alive for this
    /// seq's lifetime even across a cache eviction. Empty = base model.
    pub active_loras: Vec<(String, Arc<LoraAdapter>, f32)>,
    /// Tokens stream back through this. Use `tokio::sync::mpsc::unbounded_channel()`.
    pub response_tx: tokio::sync::mpsc::UnboundedSender<TokenEvent>,
    /// Persona that owns this generation — flows down from
    /// `TextGenerationRequest::persona_id` so the scheduler can attribute
    /// the seq slot's KV bytes to the right persona in the global
    /// FootprintRegistry. None = no attribution (test rigs, ad-hoc
    /// probes); production paths set this. Kept as `Uuid` here (not
    /// `Option<String>` like the wire format) because parsing happens at
    /// the adapter boundary — the scheduler always sees a typed value.
    pub persona_id: Option<Uuid>,
}

/// Scheduler config — sized at construction.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub n_seq_max: u32,
    /// Flash attention. Default `Auto` lets llama.cpp pick per-backend; on
    /// Metal with supported head dims (qwen3.5-4b's 256 qualifies) it turns
    /// on. Helps prefill more than single-token decode but cheap to enable.
    pub flash_attn: FlashAttn,
    pub fused_gdn_ar: bool,
    pub fused_gdn_ch: bool,
    /// KV cache K element type. `F16` lossless / `Q8_0` halves K memory.
    pub type_k: KvCacheType,
    /// KV cache V element type. `F16` lossless / `Q8_0` halves V memory.
    /// V is more sensitive to quantization than K — keep F16 unless RAM
    /// is tight.
    pub type_v: KvCacheType,
}

/// Public handle. Cloneable; clones share the same driver thread + context.
#[derive(Clone)]
pub struct Scheduler {
    request_tx: std::sync::mpsc::Sender<GenerationRequest>,
}

impl Scheduler {
    /// Spawn the driver thread and return a handle. The thread runs until
    /// the last `Scheduler` clone is dropped (channel closed) AND no
    /// active sequences remain.
    pub fn spawn(model: Arc<Model>, config: SchedulerConfig) -> Self {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let model_for_thread = model.clone();
        let cfg = config.clone();
        std::thread::Builder::new()
            .name("llamacpp-scheduler".to_string())
            .spawn(move || driver_loop(model_for_thread, cfg, request_rx))
            .expect("failed to spawn scheduler thread");
        Self { request_tx }
    }

    /// Enqueue a request. Returns Err if the scheduler thread has exited.
    pub fn enqueue(&self, req: GenerationRequest) -> Result<(), String> {
        self.request_tx
            .send(req)
            .map_err(|e| format!("scheduler closed: {e}"))
    }
}

/// Per-sequence state inside the driver.
struct ActiveSeq {
    seq_id: i32,
    prompt_tokens: Vec<i32>,
    /// How many of `prompt_tokens` have been pushed into the context KV.
    prefill_pos: usize,
    /// Absolute KV position to use for the NEXT pushed token (post-prefill).
    gen_pos: i32,
    /// Sampled-but-not-yet-pushed token from the previous iteration. None
    /// while the seq is still in prefill phase.
    next_token: Option<i32>,
    tokens_generated: usize,
    max_tokens: usize,
    sampler: Sampler,
    stop_sequences: Vec<String>,
    output_so_far: String,
    response_tx: tokio::sync::mpsc::UnboundedSender<TokenEvent>,
    started_at: Instant,
    /// Persona that owns this seq slot — copied from
    /// `GenerationRequest::persona_id`. Used by the registry-reporting
    /// path (Piece 2 of this work) to attribute KV bytes per-persona on
    /// alloc/free. None = test rig or ad-hoc probe; reporting skipped.
    persona_id: Option<Uuid>,
    /// Resolved genes `(handle, scale)` this seq decodes under. Fixed for the
    /// seq's lifetime, so its KV history is always consistent with the gene
    /// the driver applies when this seq is in the batch. Empty = base model.
    active_loras: Vec<(Arc<LoraAdapter>, f32)>,
    /// Canonical signature of `active_loras` (sorted `id:scale`, comma-joined;
    /// empty string = base). The homogeneity gate compares these to decide
    /// which seqs co-batch under one context-level `set_loras`.
    lora_sig: String,
}

/// Per-batch-slot bookkeeping so we know which logit index to sample for
/// which seq after `decode` returns. `logit_idx` is the BATCH POSITION
/// (not a sequential want-logits counter) — `llama_get_logits_ith(idx)`
/// dereferences the i-th batch position's logits buffer and asserts that
/// `batch.logits[idx] == true`. Passing a role-counter instead of the
/// actual batch position is what caused the
/// `GGML_ASSERT(logits != nullptr)` crash on first iteration.
enum BatchRole {
    /// This seq just finished its prefill in this batch. Sample to get
    /// the first generation token; future generation pushes use `gen_pos`.
    PrefillFinal {
        seq_id: i32,
        gen_pos: i32,
        logit_idx: i32,
    },
    /// This seq is mid-generation. Next sampled token continues from
    /// position `pos_after`.
    Generating {
        seq_id: i32,
        pos_after: i32,
        logit_idx: i32,
    },
}

fn driver_loop(
    model: Arc<Model>,
    config: SchedulerConfig,
    request_rx: std::sync::mpsc::Receiver<GenerationRequest>,
) {
    let log = runtime::logger("llamacpp-scheduler");

    let mut ctx = match model.new_context(ContextParams {
        n_ctx: config.n_ctx,
        n_batch: config.n_batch,
        n_ubatch: config.n_ubatch,
        n_seq_max: config.n_seq_max,
        flash_attn: config.flash_attn,
        fused_gdn_ar: config.fused_gdn_ar,
        fused_gdn_ch: config.fused_gdn_ch,
        type_k: config.type_k,
        type_v: config.type_v,
        embeddings: false,
        pooling_type: llama::PoolingType::None,
        // MoE expert-selection observer — None for now; the K3 serving path sets a
        // LiveExpertObserver here to feed the residency PGO tally.
        expert_observer: None,
    }) {
        Ok(c) => c,
        Err(e) => {
            log.error(&format!("Failed to create scheduler context: {e}"));
            return;
        }
    };
    log.info(&format!(
        "Scheduler context ready (n_ctx={}, n_batch={}, n_ubatch={}, n_seq_max={})",
        config.n_ctx, config.n_batch, config.n_ubatch, config.n_seq_max
    ));

    let n_batch = config.n_batch as usize;
    let n_seq_max = config.n_seq_max as i32;
    let mut batch = Batch::allocated(n_batch as i32, n_seq_max);

    let mut active: HashMap<i32, ActiveSeq> = HashMap::new();
    let mut free_seqs: Vec<i32> = (0..n_seq_max).collect();

    // The gene set currently applied to the shared context (its signature).
    // Starts base (""). The driver calls `set_loras`/`clear_loras` only when
    // the batch's chosen group differs from this — the cheap hot-swap.
    let mut current_ctx_lora_sig = String::new();

    // Per-phase timing — answers Joel's "I am not sure I believe your results"
    // about whether the GPU is actually doing work. We accumulate decode (Metal
    // compute + KV update) separately from sample (logits readback + sampler
    // chain on CPU + token-to-piece UTF-8 decode) so the periodic log line
    // makes the bottleneck obvious. If decode_ms ≫ sample_ms the model is
    // GPU-bound (good). If sample_ms is comparable or larger, sampling is the
    // problem and the win is moving sampling off the decode thread or pruning
    // the sampler chain.
    let mut decode_total = std::time::Duration::ZERO;
    let mut decode_count: u64 = 0;
    // Sampling time is split into two sub-phases so the GPU sync cost is
    // visible on its own. `sample_call_total` is just the `sampler.sample()`
    // call — which is what forces `llama_get_logits_ith()` to wait on the
    // outstanding Metal command buffer before the sampler chain reads the
    // logits. `post_sample_total` is everything else (token_to_piece,
    // string concat, channel send, stop-sequence scan) — which is pure CPU
    // and shouldn't be measurable.
    //
    // Why this split matters: post-Metal-fix we observed sample_avg jump
    // from 0.66ms to 20ms while decode_avg dropped from 31ms to 0.80ms.
    // Hypothesis is that decode is async-dispatch and the real GPU compute
    // wait moved into sampler.sample(). This split confirms or refutes it.
    let mut sample_call_total = std::time::Duration::ZERO;
    let mut post_sample_total = std::time::Duration::ZERO;
    let mut tokens_sampled_window: u64 = 0;
    // context-budget-exempt: how often the scheduler emits a throughput log line — a logging cadence
    const PERF_LOG_INTERVAL_TOKENS: u64 = 50;
    loop {
        // ── Phase 1: Accept new requests into free slots ──
        // If nothing is active, block on the first request (avoid spinning).
        // Otherwise non-blocking try_recv to keep the decode loop hot.
        loop {
            if free_seqs.is_empty() {
                break;
            }
            let recv = if active.is_empty() {
                match request_rx.recv() {
                    Ok(r) => Ok(r),
                    Err(_) => Err(()),
                }
            } else {
                match request_rx.try_recv() {
                    Ok(r) => Ok(r),
                    Err(_) => Err(()),
                }
            };
            match recv {
                Ok(req) => {
                    let seq_id = free_seqs.pop().unwrap();
                    match start_request(&model, seq_id, req) {
                        Ok(seq) => {
                            log.info(&format!(
                                "Seq {} started: prompt={} tokens, max_tokens={}",
                                seq_id,
                                seq.prompt_tokens.len(),
                                seq.max_tokens
                            ));
                            // Pending registry entry — bytes:0 marks "this seq
                            // exists but llama.cpp hasn't committed KV yet."
                            // Resolves to the real number after PrefillFinal
                            // succeeds. Skipped when persona_id is None
                            // (test rigs / ad-hoc probes don't get attribution).
                            if let Some(pid) = seq.persona_id {
                                footprint_registry::global().add(
                                    FootprintKey::for_persona(
                                        pid,
                                        ResourceType::KvCache,
                                        Residency::Active,
                                    ),
                                    0,
                                );
                            }
                            active.insert(seq_id, seq);
                        }
                        Err(e) => {
                            log.warn(&format!("start_request failed: {e}"));
                            free_seqs.push(seq_id);
                        }
                    }
                }
                Err(()) => break,
            }
        }

        // If the channel is closed AND no active work, exit.
        if active.is_empty() {
            log.info("Scheduler exiting (no active seqs and channel closed)");
            break;
        }

        // ── Phase 2: Build batch ──
        // Combine continuing-prefill chunks + next-token-per-generating-seq.
        // Cap total tokens at n_batch.
        batch.clear();
        let mut roles: Vec<BatchRole> = Vec::new();
        let mut tokens_in_batch = 0usize;
        let mut to_remove: Vec<i32> = Vec::new();
        // Homogeneity gate: this decode applies exactly one gene set. The
        // first seq with work (lowest seq_id) defines the group; seqs wanting
        // a different gene are deferred to a later iteration so a seq is never
        // decoded under another seq's adapter.
        let mut batch_lora_sig: Option<String> = None;
        let mut batch_lora_rep: Option<i32> = None;

        // Iterate sorted by seq_id for determinism in tests/logs.
        let seq_ids: Vec<i32> = {
            let mut v: Vec<i32> = active.keys().copied().collect();
            v.sort_unstable();
            v
        };

        for seq_id in seq_ids {
            let room = n_batch.saturating_sub(tokens_in_batch);
            if room == 0 {
                break;
            }
            let seq = active.get_mut(&seq_id).expect("seq present");

            // Gene-group gate (see batch_lora_sig above). A seq with no work
            // this iteration is skipped without claiming the group.
            let has_work = seq.prefill_pos < seq.prompt_tokens.len() || seq.next_token.is_some();
            if !has_work {
                continue;
            }
            match batch_lora_sig {
                None => {
                    batch_lora_sig = Some(seq.lora_sig.clone());
                    batch_lora_rep = Some(seq_id);
                }
                Some(ref sig) if *sig != seq.lora_sig => continue,
                Some(_) => {}
            }

            if seq.prefill_pos < seq.prompt_tokens.len() {
                let chunk_end = (seq.prefill_pos + room).min(seq.prompt_tokens.len());
                let is_final = chunk_end == seq.prompt_tokens.len();
                let mut final_logit_idx: i32 = -1;
                for i in seq.prefill_pos..chunk_end {
                    let want_logits = is_final && i == chunk_end - 1;
                    batch.push(seq.prompt_tokens[i], i as i32, &[seq_id], want_logits);
                    if want_logits {
                        // Record the BATCH POSITION (not role count) where
                        // this seq's logits live — required by
                        // llama_get_logits_ith.
                        final_logit_idx = tokens_in_batch as i32;
                    }
                    tokens_in_batch += 1;
                }
                if is_final {
                    debug_assert!(
                        final_logit_idx >= 0,
                        "final prefill chunk must record logit idx"
                    );
                    roles.push(BatchRole::PrefillFinal {
                        seq_id,
                        gen_pos: chunk_end as i32,
                        logit_idx: final_logit_idx,
                    });
                }
                seq.prefill_pos = chunk_end;
            } else if let Some(token) = seq.next_token {
                let logit_idx = tokens_in_batch as i32;
                batch.push(token, seq.gen_pos, &[seq_id], true);
                tokens_in_batch += 1;
                roles.push(BatchRole::Generating {
                    seq_id,
                    pos_after: seq.gen_pos + 1,
                    logit_idx,
                });
            }
        }

        if tokens_in_batch == 0 {
            // Nothing to do this iteration — yield briefly.
            std::thread::sleep(std::time::Duration::from_millis(1));
            continue;
        }

        // ── Phase 3: Apply this batch's gene group, then decode ──
        // Context-level LoRA. The homogeneity gate guarantees every seq in
        // this batch shares `batch_lora_sig`, and a seq is only ever decoded
        // under its own gene, so its KV history stays consistent. We only call
        // set_loras/clear_loras when the desired group differs from what's
        // already on the context (the cheap hot-swap). A set_loras failure is
        // routed through the same fan-out as a decode failure — never decode
        // under the wrong gene (Rule 2).
        let desired_sig = batch_lora_sig.clone().unwrap_or_default();
        let lora_apply: Result<(), String> = if desired_sig == current_ctx_lora_sig {
            Ok(())
        } else if desired_sig.is_empty() {
            ctx.clear_loras()
        } else {
            let rep =
                batch_lora_rep.expect("non-empty gene signature must have a representative seq");
            let refs: Vec<(&LoraAdapter, f32)> = active
                .get(&rep)
                .expect("representative seq present")
                .active_loras
                .iter()
                .map(|(a, s)| (a.as_ref(), *s))
                .collect();
            ctx.set_loras(&refs)
        };

        let decode_start = Instant::now();
        let decode_result = match lora_apply {
            Ok(()) => {
                // Context now reflects this gene group (even if decode later
                // fails) — record it so the next iteration skips a redundant
                // re-apply.
                current_ctx_lora_sig = desired_sig.clone();
                ctx.decode(&batch)
            }
            Err(e) => Err(format!("set_loras failed (gene '{desired_sig}'): {e}")),
        };
        if let Err(e) = decode_result {
            log.error(&format!(
                "Decode error: {e} (batch={} tokens, {} active seqs)",
                tokens_in_batch,
                active.len()
            ));
            // Fail every seq that had work in this batch — we can't tell
            // which one caused the failure, so the safe thing is to error
            // all participants and let callers retry.
            for role in &roles {
                let sid = match role {
                    BatchRole::PrefillFinal { seq_id, .. } => *seq_id,
                    BatchRole::Generating { seq_id, .. } => *seq_id,
                };
                if let Some(seq) = active.get(&sid) {
                    let _ = seq.response_tx.send(TokenEvent::Error(e.clone()));
                }
                to_remove.push(sid);
            }
        } else {
            // Decode succeeded — record Metal-compute time. This is the
            // wall-clock time the Metal command buffer + dispatch took,
            // including any CPU↔GPU graph splits if the Metal backend fell
            // back to CPU for unsupported ops.
            decode_total += decode_start.elapsed();
            decode_count += 1;

            // ── Phase 4: Sample for each logit-bearing position ──
            // Logits are addressed by BATCH POSITION (not role-vec index).
            // `llama_get_logits_ith(idx)` reads `batch.logits[idx]` and
            // panics if it's not `true`. We recorded `logit_idx` while
            // building the batch — it's the absolute batch position
            // where this seq's want_logits=true token sits.
            let sample_start = Instant::now();
            let mut sample_call_iter_total = std::time::Duration::ZERO;
            for role in &roles {
                let (seq_id, advance_pos, logit_idx) = match role {
                    BatchRole::PrefillFinal {
                        seq_id,
                        gen_pos,
                        logit_idx,
                    } => (*seq_id, *gen_pos, *logit_idx),
                    BatchRole::Generating {
                        seq_id,
                        pos_after,
                        logit_idx,
                    } => (*seq_id, *pos_after, *logit_idx),
                };
                let seq = match active.get_mut(&seq_id) {
                    Some(s) => s,
                    None => continue,
                };

                // Time the sampler.sample() call independently. This is the
                // implicit GPU sync point — llama_get_logits_ith() blocks
                // until the outstanding Metal command buffer completes, so
                // most of the apparent "sample" cost lives here, not in the
                // post-sample work below.
                let sample_call_start = Instant::now();
                let token = seq.sampler.sample(&ctx, logit_idx);
                let sample_call_elapsed = sample_call_start.elapsed();
                sample_call_iter_total += sample_call_elapsed;

                // If this role was PrefillFinal (first decode for the seq),
                // llama.cpp has now committed the seq's KV cache. Ask the
                // backend for the exact bytes and overwrite the pending
                // registry entry. Done here (not in a separate pass) because
                // we already have the seq + role in scope and the cost is
                // one FFI call. seq_state_bytes returns 0 if seq doesn't
                // exist — defensive fallback never lands a fake number.
                let was_prefill_final = matches!(role, BatchRole::PrefillFinal { .. });
                if was_prefill_final {
                    if let Some(pid) = seq.persona_id {
                        let bytes = ctx.seq_state_bytes(seq_id);
                        if bytes > 0 {
                            footprint_registry::global().report_authoritative(
                                FootprintKey::for_persona(
                                    pid,
                                    ResourceType::KvCache,
                                    Residency::Active,
                                ),
                                bytes,
                            );
                        }
                    }
                }

                if model.is_eog_token(token) {
                    // Registry cleanup MUST happen before sending Done, so
                    // any caller awaiting on the channel sees a consistent
                    // registry state (entry removed) the moment generate
                    // returns. Phase 5 only does memory_seq_rm + free_seq.
                    if let Some(pid) = seq.persona_id {
                        let bytes = ctx.seq_state_bytes(seq_id);
                        footprint_registry::global().remove(
                            &FootprintKey::for_persona(
                                pid,
                                ResourceType::KvCache,
                                Residency::Active,
                            ),
                            bytes,
                        );
                    }
                    let _ = seq.response_tx.send(TokenEvent::Done {
                        tokens_generated: seq.tokens_generated,
                        elapsed_ms: seq.started_at.elapsed().as_millis() as u64,
                    });
                    to_remove.push(seq_id);
                    continue;
                }

                let piece = model.token_to_piece(token);
                seq.output_so_far.push_str(&piece);
                let _ = seq.response_tx.send(TokenEvent::Token(piece));
                seq.tokens_generated += 1;

                let stop_hit = seq
                    .stop_sequences
                    .iter()
                    .any(|s| seq.output_so_far.ends_with(s));
                if stop_hit || seq.tokens_generated >= seq.max_tokens {
                    // Same pre-Done registry cleanup as the EOG path —
                    // single source of truth on what state the channel
                    // completion signals.
                    if let Some(pid) = seq.persona_id {
                        let bytes = ctx.seq_state_bytes(seq_id);
                        footprint_registry::global().remove(
                            &FootprintKey::for_persona(
                                pid,
                                ResourceType::KvCache,
                                Residency::Active,
                            ),
                            bytes,
                        );
                    }
                    let _ = seq.response_tx.send(TokenEvent::Done {
                        tokens_generated: seq.tokens_generated,
                        elapsed_ms: seq.started_at.elapsed().as_millis() as u64,
                    });
                    to_remove.push(seq_id);
                    continue;
                }

                seq.next_token = Some(token);
                seq.gen_pos = advance_pos;
            }
            // Phase-4 wall time minus the per-iteration sample-call cost =
            // post-sample CPU work (token_to_piece, push_str, channel send,
            // stop-sequence scan).
            let phase4_total = sample_start.elapsed();
            sample_call_total += sample_call_iter_total;
            post_sample_total += phase4_total.saturating_sub(sample_call_iter_total);
            tokens_sampled_window += roles.len() as u64;
        }

        // ── Periodic GPU/CPU bottleneck telemetry ──
        // Emit once per PERF_LOG_INTERVAL_TOKENS so chat sees real per-phase
        // numbers without log spam. Decode = Metal-side compute. Sample =
        // CPU-side sampler chain + UTF-8 decode + channel send. If decode_ms
        // dominates we're GPU-bound (expected). If sample_ms is comparable
        // the CPU tail is the bottleneck.
        if tokens_sampled_window >= PERF_LOG_INTERVAL_TOKENS && decode_count > 0 {
            let avg_decode_us = decode_total.as_micros() as f64 / decode_count as f64;
            let avg_sample_call_us =
                sample_call_total.as_micros() as f64 / tokens_sampled_window as f64;
            let avg_post_sample_us =
                post_sample_total.as_micros() as f64 / tokens_sampled_window as f64;
            let total_us_per_tok = avg_decode_us + avg_sample_call_us + avg_post_sample_us;
            let tok_per_s = if total_us_per_tok > 0.0 {
                1_000_000.0 / total_us_per_tok
            } else {
                0.0
            };
            // sample_call captures the GPU sync wait + sampler chain CPU
            // work. post_sample is everything else (token_to_piece, send,
            // stop scan). When sample_call ≫ post_sample the bottleneck is
            // GPU sync, not CPU sampler chain — and the lever is async
            // pipelining or a leaner sampler, not faster string ops.
            log.info(&format!(
                "perf: decode_dispatch={:.2}ms sample_call={:.2}ms post_sample={:.2}ms \
                 ({} decodes / {} sampled) → {:.1} tok/s",
                avg_decode_us / 1000.0,
                avg_sample_call_us / 1000.0,
                avg_post_sample_us / 1000.0,
                decode_count,
                tokens_sampled_window,
                tok_per_s,
            ));
            decode_total = std::time::Duration::ZERO;
            decode_count = 0;
            sample_call_total = std::time::Duration::ZERO;
            post_sample_total = std::time::Duration::ZERO;
            tokens_sampled_window = 0;
        }

        // ── Phase 5: Free completed seqs ──
        // Registry cleanup happens UPSTREAM at the Done send (Phase 4),
        // so callers awaiting on the channel see a consistent registry
        // state when they unblock. Here we only do the llama.cpp seq_rm
        // and return the seq_id to the free pool.
        //
        // Decode-error path: also pushes to to_remove, but bypasses the
        // Phase 4 cleanup. We catch it here as a fallback — if the seq is
        // still in `active` AND has a persona_id with a registry entry,
        // remove it. seq_state_bytes(seq_id) is still valid before
        // memory_seq_rm.
        for seq_id in to_remove {
            // Fallback registry cleanup (only fires for paths that didn't
            // already clean up — the decode-error path is the only one).
            if let Some(seq) = active.get(&seq_id) {
                if let Some(pid) = seq.persona_id {
                    let key =
                        FootprintKey::for_persona(pid, ResourceType::KvCache, Residency::Active);
                    // If the entry was already cleaned up by Phase 4, this
                    // is a no-op (remove on missing key does nothing). If
                    // it's still here (decode-error path), drain it to 0.
                    let bytes = ctx.seq_state_bytes(seq_id);
                    footprint_registry::global().remove(&key, bytes);
                }
            }

            ctx.memory_seq_rm(seq_id, -1, -1);

            if let Some(seq) = active.remove(&seq_id) {
                log.info(&format!(
                    "Seq {} finished: {} tokens in {}ms ({:.1} tok/s)",
                    seq_id,
                    seq.tokens_generated,
                    seq.started_at.elapsed().as_millis(),
                    seq.tokens_generated as f64 / seq.started_at.elapsed().as_secs_f64().max(0.001)
                ));
            }
            free_seqs.push(seq_id);
        }
    }
}

/// Canonical signature of a gene set — sorted `id:scale`, comma-joined.
/// Empty string = base model. Two seqs co-batch under one context-level
/// `set_loras` iff their signatures match (the homogeneity gate).
fn lora_signature(loras: &[(String, Arc<LoraAdapter>, f32)]) -> String {
    if loras.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = loras.iter().map(|(id, _, s)| format!("{id}:{s}")).collect();
    parts.sort_unstable();
    parts.join(",")
}

fn start_request(model: &Model, _seq_id: i32, req: GenerationRequest) -> Result<ActiveSeq, String> {
    let lora_sig = lora_signature(&req.active_loras);
    let active_loras: Vec<(Arc<LoraAdapter>, f32)> =
        req.active_loras.into_iter().map(|(_, h, s)| (h, s)).collect();
    // special=true so chat-template boundary markers (<|im_start|>,
    // <|im_end|>) are tokenized as the model's actual special token IDs
    // (151644/151645 for qwen3) rather than character-level text. With
    // special=false the model never sees the boundary tokens it was
    // trained on — output collapsed to short fragments terminating early
    // at character-matched stop sequences.
    let prompt_tokens = model.tokenize(&req.prompt, true, true)?;
    let sampler = if req.sampling.temperature <= 0.0 && req.sampling.grammar.is_none() {
        Sampler::greedy()
    } else {
        // Build the full sampler chain. Order: grammar → top_k → top_p →
        // penalties → temp → dist. Grammar early so structural constraint
        // applies BEFORE probabilistic sampling (otherwise temp could pick
        // a token that the grammar would have rejected).
        let mut chain = Sampler::chain();
        if let Some(g) = req.sampling.grammar.as_ref() {
            chain = chain.grammar(model, g, "root");
        }
        if req.sampling.top_k > 0 {
            chain = chain.top_k(req.sampling.top_k as i32);
        }
        if req.sampling.top_p > 0.0 && req.sampling.top_p < 1.0 {
            chain = chain.top_p(req.sampling.top_p as f32, 1);
        }
        // 64 = llama.cpp default last-n window for the penalty calculation.
        chain = chain.penalties(64, req.sampling.repeat_penalty, 0.0, 0.0);
        let temp = if req.sampling.temperature > 0.0 {
            req.sampling.temperature as f32
        } else {
            0.01
        };
        chain.temp(temp).dist(42).build()
    };
    Ok(ActiveSeq {
        seq_id: _seq_id,
        prompt_tokens,
        prefill_pos: 0,
        gen_pos: 0,
        next_token: None,
        tokens_generated: 0,
        max_tokens: req.max_tokens,
        sampler,
        stop_sequences: req.stop_sequences,
        output_so_far: String::new(),
        response_tx: req.response_tx,
        started_at: Instant::now(),
        persona_id: req.persona_id,
        active_loras,
        lora_sig,
    })
}
