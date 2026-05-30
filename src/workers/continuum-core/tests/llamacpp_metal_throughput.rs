//! Smoke test for the bundled llama.cpp's Metal acceleration on M-series Macs.
//!
//! Bypasses Docker Model Runner (DMR) entirely and loads qwen3.5-4b directly
//! through the in-process LlamaCppBackend wrapper. Measures throughput and
//! prints whether the Metal tensor API path is active.
//!
//! Why this test exists: 2026-04-19 found that DMR's container Metal toolchain
//! fails to compile the f16 tensor API source on M5 Pro (MTLGPUFamilyMetal4),
//! causing `has tensor = false` and a degraded fallback that runs SLOWER than
//! pre-M5 hardware (M5 at 22 tok/s vs M1 at 27 tok/s for the same qwen2.5-7B).
//!
//! Hypothesis: our bundled llama.cpp built on the host Metal toolchain DOES
//! compile the tensor API source correctly. If this test produces ≥50 tok/s on
//! M5 for qwen3.5-4b (Q4_K_M), the bypass-DMR path is the answer for Mac local
//! inference AND we have concrete repro evidence for an upstream llama.cpp
//! issue ("DMR build is degraded vs host build on identical hardware").
//!
//! Run manually:
//!   cargo test --package continuum-core --test llamacpp_metal_throughput \
//!     --release -- --ignored --nocapture
//!
//! Marked #[ignore] because it requires the qwen3.5-4b GGUF file at the DMR
//! path, takes 10-30s, and isn't part of the regular CI test loop.

use continuum_core::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use continuum_core::inference::backends::SamplingConfig;
use llama::FlashAttn;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

/// SHA256-keyed path to the qwen3.5-4b-code-forged GGUF (target), as DMR pulls it.
/// The same content hashes identically across all hosts that pull the same
/// model, so the path is a matter of `$HOME` only.
fn qwen35_4b_target_path() -> PathBuf {
    // Override wins. If $QWEN35_4B_GGUF is set, use it verbatim.
    if let Ok(p) = env::var("QWEN35_4B_GGUF") {
        return PathBuf::from(p);
    }
    // Otherwise resolve via `$HOME/.docker/models/bundles/sha256/<hash>/model/model.gguf`.
    // Hash is the content-address of the continuum-ai forged Qwen3.5-4B GGUF.
    let home = env::var("HOME").expect("HOME env var must be set for this integration test");
    PathBuf::from(format!(
        "{}/.docker/models/bundles/sha256/18055fe8ee379b95f4af3cf420588c5daa28f2a1ce1da335112a2d1ea188d3e6/model/model.gguf",
        home
    ))
}

/// SHA256-keyed path to the qwen3.5-0.8B GGUF (draft for speculative decoding).
/// Same family as the target → tokenizer-identical → drop-in draft candidate.
/// Pull with: `docker model pull hf.co/unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M`.
fn qwen35_08b_draft_path() -> Option<PathBuf> {
    if let Ok(p) = env::var("QWEN35_08B_DRAFT_GGUF") {
        return Some(PathBuf::from(p));
    }
    let home = env::var("HOME").ok()?;
    // The hash differs per-machine because it's the content-address of the
    // specific GGUF blob pulled. We discover it by listing the bundles dir
    // and picking the one whose contained file is ~500MiB.
    let bundles = PathBuf::from(format!("{}/.docker/models/bundles/sha256", home));
    if !bundles.is_dir() {
        return None;
    }
    for entry in std::fs::read_dir(&bundles).ok()? {
        let entry = entry.ok()?;
        let gguf = entry.path().join("model").join("model.gguf");
        if !gguf.is_file() {
            continue;
        }
        let size = std::fs::metadata(&gguf).ok()?.len();
        // 0.8B Q4_K_M is ~497MiB; target 4B is ~2.5GiB; sibling quants of the
        // 0.8B fall in 300-700MB range so 300..900MB is the sanity window.
        if (300_000_000..900_000_000).contains(&size) {
            // Confirm via metadata read if llama.cpp tool is available —
            // skipped here for simplicity. Size-based filter is the heuristic.
            return Some(gguf);
        }
    }
    None
}

#[test]
#[ignore = "requires local GGUF + 10-30s; run manually with --ignored --nocapture"]
fn qwen35_4b_metal_throughput_via_bundled_llamacpp() {
    let model_path = qwen35_4b_target_path();
    if !model_path.exists() {
        panic!(
            "qwen3.5-4b GGUF not found at {:?} — pull via `docker model pull \
             huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf` first \
             (or set QWEN35_4B_GGUF env var to the path)",
            model_path
        );
    }

    let load_start = Instant::now();
    // Override knob: $QWEN35_4B_GPU_LAYERS lets the operator force CPU-only
    // (=0) or partial-offload (=N) to isolate which side of the Metal/CPU
    // boundary breaks. Default -1 = all layers on GPU (the original
    // measurement). Mac Intel + AMD-discrete debugging needs the 0 case
    // to confirm llama.cpp emits coherent tokens when the Metal-AMD
    // shader path is bypassed.
    let n_gpu_layers: i32 = env::var("QWEN35_4B_GPU_LAYERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(-1);
    eprintln!("[smoke] n_gpu_layers = {n_gpu_layers}");
    let config = LlamaCppConfig {
        model_path,
        n_gpu_layers,
        context_length: Some(32768),
        n_seq_max: 1,
        n_ubatch: 128,
        flash_attn: FlashAttn::Disabled,
        fused_gdn_ar: false,
        fused_gdn_ch: false,
        ..Default::default()
    };
    let backend = LlamaCppBackend::load(config).expect("failed to load llama.cpp backend");
    let load_ms = load_start.elapsed().as_millis();
    eprintln!(
        "[smoke] backend loaded in {}ms (model_id={})",
        load_ms,
        backend.model_id()
    );

    // Warm-up call so the first-call compile/cache cost doesn't pollute measurement.
    // SamplingConfig::chat() = temp 0.6 + repeat_penalty 1.1 + top-k 40 + top-p 0.95,
    // matching what live chat traffic uses (the throughput we want to measure).
    eprintln!("[smoke] warm-up generation (10 tokens)...");
    let warm_start = Instant::now();
    let warm_result = backend
        .generate("Reply OK.", 10, SamplingConfig::chat(), &[], &[])
        .expect("warm-up generate failed");
    eprintln!(
        "[smoke] warm-up: {} tokens in {}ms ({:.1} tok/s) — text={:?}",
        warm_result.1,
        warm_start.elapsed().as_millis(),
        warm_result.1 as f64 / warm_start.elapsed().as_secs_f64(),
        warm_result.0
    );

    // Real measurement: 100 tokens, longer output, isolated decode rate.
    eprintln!("[smoke] measurement generation (100 tokens)...");
    let gen_start = Instant::now();
    let (text, tokens) = backend
        .generate(
            "Count from 1 to 50, separated by commas.",
            100,
            SamplingConfig::chat(),
            &[],
            &[],
        )
        .expect("measurement generate failed");
    let elapsed_secs = gen_start.elapsed().as_secs_f64();
    let tokens_per_sec = tokens as f64 / elapsed_secs;

    eprintln!("");
    eprintln!("=== llamacpp metal throughput on this host ===");
    eprintln!("  tokens generated: {tokens}");
    eprintln!("  elapsed: {:.2}s", elapsed_secs);
    eprintln!("  THROUGHPUT: {:.1} tok/s", tokens_per_sec);
    eprintln!("");
    eprintln!("  reference (DMR's degraded path on M5 Pro): ~22 tok/s");
    eprintln!("  expected for fully-accelerated Metal on M5 Pro: ≥50 tok/s");
    eprintln!("  text head: {:?}", &text[..text.len().min(120)]);
    eprintln!("===============================================");
    eprintln!("");

    // Don't assert a hard floor — this test is observational. The output above
    // is the diagnostic. Manual review interprets whether the bypass-DMR
    // approach is justified by the throughput delta vs DMR's measured 22 tok/s.
    assert!(
        tokens > 0,
        "no tokens generated — backend may have failed to load model on Metal"
    );
}

/// Speculative-decoding throughput benchmark. Target = qwen3.5-4b-code-forged,
/// draft = qwen3.5-0.8B (same family → byte-identical tokenizer → drop-in draft).
///
/// Uses raw `llama` crate primitives (Model/Context/Batch/Sampler) — no
/// generate_with_draft() wrapper yet. Per 2026-04-20 pair discussion with anvil:
/// prove the loop in the test harness first, measure tradeoffs (draft_max,
/// accept threshold, KV-rewind strategy), then promote to a safe.rs wrapper
/// once the right shape is obvious.
///
/// Algorithm (greedy, deterministic):
///   1. Tokenize prompt once, push into target and draft contexts in parallel.
///   2. Loop:
///      a. Draft autoregressively samples K tokens. KV extends by K.
///      b. Target validates in ONE decode pass: batch with K draft tokens,
///         positions [pos..pos+K), want_logits=true on each.
///      c. For each position i in 0..K, read target's logits_ith(i), sample
///         greedy. Compare to draft_tokens[i]. First mismatch: accept 0..i
///         from draft, emit target's sample as correction at position i,
///         rewind draft KV to pos+i+1, rewind target KV to pos+i+1.
///      d. If all K agree: accept all K, sample target's logits_ith(K-1) as
///         the bonus next token. Advance pos by K+1.
///   3. Terminate on EOG or max_tokens.
///
/// Metrics reported: baseline tok/s (no draft), spec-dec tok/s, accept rate,
/// uplift ratio. Draft_max parameter tunable via QWEN35_DRAFT_MAX env var
/// (default 4; grid-search candidates: 2, 4, 6, 8).
#[test]
#[ignore = "requires target+draft GGUFs + 20-60s; run manually with --ignored --nocapture"]
fn qwen35_4b_spec_dec_throughput() {
    use llama::{Batch, ContextParams, Model, ModelParams, Sampler};

    let target_path = qwen35_4b_target_path();
    assert!(
        target_path.exists(),
        "target GGUF not found: {target_path:?}"
    );
    let draft_path = match qwen35_08b_draft_path() {
        Some(p) => p,
        None => {
            eprintln!("[spec-dec] draft GGUF not found in ~/.docker/models/bundles — set $QWEN35_08B_DRAFT_GGUF or run:");
            eprintln!("          docker model pull hf.co/unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M");
            return; // skip cleanly, test is observational
        }
    };
    eprintln!("[spec-dec] target: {target_path:?}");
    eprintln!("[spec-dec] draft:  {draft_path:?}");

    let draft_max: usize = env::var("QWEN35_DRAFT_MAX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let max_output: usize = env::var("QWEN35_MAX_TOKENS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let prompt = "Count from 1 to 50, separated by commas.";

    // --- Load both models on Metal ---
    let load_start = Instant::now();
    let target_model = Model::load(
        &target_path,
        ModelParams {
            n_gpu_layers: -1,
            use_mmap: true,
        },
    )
    .expect("target load failed");
    let draft_model = Model::load(
        &draft_path,
        ModelParams {
            n_gpu_layers: -1,
            use_mmap: true,
        },
    )
    .expect("draft load failed");
    eprintln!(
        "[spec-dec] loaded target + draft in {}ms (target_vocab={}, draft_vocab={})",
        load_start.elapsed().as_millis(),
        target_model.n_vocab(),
        draft_model.n_vocab()
    );
    assert_eq!(
        target_model.n_vocab(),
        draft_model.n_vocab(),
        "target and draft vocab sizes differ — different tokenizer → spec-dec impossible"
    );

    // --- Context config: 32k ctx, FA-auto. Mirror LlamaCppBackend's defaults. ---
    let ctx_params = ContextParams {
        n_ctx: 32_768,
        ..Default::default()
    };
    let mut target_ctx = target_model
        .new_context(ctx_params.clone())
        .expect("target ctx");
    let mut draft_ctx = draft_model.new_context(ctx_params).expect("draft ctx");

    // --- Tokenize + push initial prompt into both contexts ---
    let prompt_tokens = target_model
        .tokenize(prompt, true, true)
        .expect("tokenize prompt");
    let prompt_len = prompt_tokens.len() as i32;

    // Push prompt into target: one batch, last token gets logits (for first draft seed).
    {
        let mut batch = Batch::allocated(prompt_len, 1);
        for (i, &tok) in prompt_tokens.iter().enumerate() {
            let want = i == prompt_tokens.len() - 1;
            batch.push(tok, i as i32, &[0], want);
        }
        target_ctx.decode(&batch).expect("target prompt decode");
    }
    // Same prompt into draft.
    {
        let mut batch = Batch::allocated(prompt_len, 1);
        for (i, &tok) in prompt_tokens.iter().enumerate() {
            let want = i == prompt_tokens.len() - 1;
            batch.push(tok, i as i32, &[0], want);
        }
        draft_ctx.decode(&batch).expect("draft prompt decode");
    }

    let mut target_sampler = Sampler::greedy();
    let mut draft_sampler = Sampler::greedy();

    // --- Spec-dec loop ---
    let gen_start = Instant::now();
    let mut output_tokens: Vec<i32> = Vec::with_capacity(max_output);
    let mut pos: i32 = prompt_len;
    let mut draft_proposed: usize = 0;
    let mut draft_accepted: usize = 0;
    let mut spec_iterations: usize = 0;

    // Seed: sample target's first token (off the prompt's last-token logits).
    let mut last_token = target_sampler.sample(&target_ctx, prompt_len - 1);
    output_tokens.push(last_token);

    // Prime draft with the same first token so both contexts agree on pos.
    {
        let mut batch = Batch::allocated(1, 1);
        batch.push(last_token, pos, &[0], true);
        draft_ctx.decode(&batch).expect("draft seed decode");
    }
    pos += 1;

    'outer: while output_tokens.len() < max_output {
        if target_model.is_eog_token(last_token) {
            break;
        }
        spec_iterations += 1;

        // --- (a) Draft generates K tokens autoregressively from draft KV ---
        let mut drafts: Vec<i32> = Vec::with_capacity(draft_max);
        let mut seed = last_token;
        for k in 0..draft_max {
            // draft's last decode had logits at its last position; sample from there
            let draft_last_logit_idx = if k == 0 { 0 } else { 0 }; // always position 0 of last batch
            let next = draft_sampler.sample(&draft_ctx, draft_last_logit_idx);
            drafts.push(next);
            // feed next into draft so it can produce draft[k+1]
            let mut batch = Batch::allocated(1, 1);
            batch.push(next, pos + k as i32, &[0], true);
            if draft_ctx.decode(&batch).is_err() {
                break;
            }
            seed = next;
            if target_model.is_eog_token(next) {
                break; // stop drafting further
            }
        }
        let k_drafted = drafts.len();
        if k_drafted == 0 {
            break;
        }

        // --- (b) Target validates all K drafts in ONE decode ---
        let mut tgt_batch = Batch::allocated(k_drafted as i32, 1);
        for (i, &tok) in drafts.iter().enumerate() {
            tgt_batch.push(tok, pos + i as i32, &[0], true);
        }
        target_ctx
            .decode(&tgt_batch)
            .expect("target validate decode");

        // --- (c) Compare draft-vs-target at each position, find first mismatch ---
        let mut accepted = 0usize;
        let mut correction: Option<i32> = None;
        for i in 0..k_drafted {
            let tgt_pred = target_sampler.sample(&target_ctx, i as i32);
            if tgt_pred == drafts[i] {
                accepted += 1;
            } else {
                correction = Some(tgt_pred);
                break;
            }
        }
        draft_proposed += k_drafted;
        draft_accepted += accepted;

        // Emit accepted drafts
        for &tok in drafts.iter().take(accepted) {
            output_tokens.push(tok);
            if output_tokens.len() >= max_output {
                break 'outer;
            }
            if target_model.is_eog_token(tok) {
                break 'outer;
            }
        }

        // (d) Handle the tail — mismatch path or all-accept bonus.
        //
        // KV invariants (entering this block):
        //   target KV: positions 0..pos+k_drafted (target decoded all drafts)
        //   draft  KV: positions 0..pos+k_drafted (draft autoregressively produced all K)
        //
        // Goal after this block: both KVs reflect [0..pos+accepted) ++ [emitted_next]
        // where emitted_next is either `c` (correction at position pos+accepted) or
        // `bonus` (at position pos+k_drafted).
        match correction {
            Some(c) => {
                // Mismatch at position `accepted`. Target rejected drafts[accepted].
                // Correction token `c` replaces drafts[accepted] at position pos+accepted.
                //
                // memory_seq_rm(seq_id, p0, p1) removes KV entries with positions in
                // [p0, p1). Passing p1 = -1 means "to the end". So we cut everything
                // from pos+accepted inclusive — BOTH contexts had drafts[accepted] or
                // later cached there and none of that is valid anymore.
                output_tokens.push(c);
                last_token = c;
                let cut_pos = pos + accepted as i32;
                let _ = target_ctx.memory_seq_rm(0, cut_pos, -1);
                let _ = draft_ctx.memory_seq_rm(0, cut_pos, -1);
                // Push c at cut_pos into BOTH contexts so their KV extends with the
                // real next token. Off-by-one in the previous version: we pushed at
                // cut_pos-1 which collided with the last accepted token already in KV.
                let mut tbatch = Batch::allocated(1, 1);
                tbatch.push(c, cut_pos, &[0], true);
                target_ctx.decode(&tbatch).expect("target sync decode");
                let mut dbatch = Batch::allocated(1, 1);
                dbatch.push(c, cut_pos, &[0], true);
                draft_ctx.decode(&dbatch).expect("draft sync decode");
                pos = cut_pos + 1;
            }
            None => {
                // All K accepted. Take target's sample at position K-1 as bonus.
                // Target's logits_ith(K-1) gives the prediction for position pos+K
                // (what comes after drafts[K-1]). Bonus token lands at position pos+k_drafted.
                let bonus = target_sampler.sample(&target_ctx, (k_drafted - 1) as i32);
                output_tokens.push(bonus);
                last_token = bonus;
                let bonus_pos = pos + k_drafted as i32;
                // No rewind needed — every position up to pos+k_drafted-1 is valid
                // in both KVs. We just append bonus_pos onto both.
                let mut tbatch = Batch::allocated(1, 1);
                tbatch.push(bonus, bonus_pos, &[0], true);
                target_ctx.decode(&tbatch).expect("target bonus decode");
                let mut dbatch = Batch::allocated(1, 1);
                dbatch.push(bonus, bonus_pos, &[0], true);
                draft_ctx.decode(&dbatch).expect("draft bonus-sync decode");
                pos = bonus_pos + 1;
            }
        }
    }

    let elapsed = gen_start.elapsed().as_secs_f64();
    let out_len = output_tokens.len();
    let tok_per_sec = out_len as f64 / elapsed;
    let accept_rate = if draft_proposed == 0 {
        0.0
    } else {
        draft_accepted as f64 / draft_proposed as f64
    };

    // Reconstruct text for visibility.
    let text: String = output_tokens
        .iter()
        .map(|&t| target_model.token_to_piece(t))
        .collect();

    eprintln!("");
    eprintln!("=== qwen3.5-4b spec-dec throughput (draft=0.8B, K={draft_max}) ===");
    eprintln!("  output tokens: {out_len}");
    eprintln!("  wall time: {elapsed:.2}s");
    eprintln!("  THROUGHPUT: {tok_per_sec:.1} tok/s");
    eprintln!(
        "  draft proposed: {draft_proposed}  accepted: {draft_accepted}  accept_rate: {:.1}%",
        accept_rate * 100.0
    );
    eprintln!("  spec-dec iterations: {spec_iterations}");
    eprintln!("  reference baseline (no draft, single-model): ~33 tok/s M1 / ~47 tok/s M5");
    eprintln!("  text head: {:?}", &text[..text.len().min(120)]);
    eprintln!("=======================================================");
    eprintln!("");

    assert!(out_len > 0, "no tokens generated via spec-dec");
}
