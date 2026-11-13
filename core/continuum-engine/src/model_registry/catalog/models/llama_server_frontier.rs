use super::super::*;

/// llama-server frontier/work-tier rows (Qwen3.8-27B, Qwen3.6-35B-A3B, Ornith, Flash-Next, Hermes-3) and the ds4 DeepSeek sidecar.
pub(super) fn llama_server_frontier() -> Vec<Model> {
    vec![
        // QWEN3.8-27B — the FRONTIER-TIER lane (Joel, 2026-08-15: "open models just got
        // released that are better than opus and even fable"). Dense 27B, Arch::Qwen35
        // (the fork carries LLM_ARCH_QWEN35 + MTP draft spec-decode + the mmproj vision
        // path for it). Published scores: SWE-bench Pro 61.7 vs Opus 4.6 Max 53.4,
        // QwenSWEBench 79.0 vs 63.8 — a local model that beats the cloud flagship on
        // agentic coding, on consumer hardware. Field-measured serving (RTX 4090,
        // Q4_K_M-class): 40.7 t/s decode plain, 60.1 t/s with native MTP spec-decode
        // (the `mtp-*.gguf` sibling this catalog's serving spawn now auto-loads, #440),
        // 262k native context resident in 24GB with q4_0 KV. The ggml-org repo ships
        // main + mtp draft + mmproj in ONE snapshot, so `models/pull` acquires all
        // three and the sibling resolvers find them with zero per-machine paths.
        // context_window is the MODEL's capability; the live served window comes from
        // the adapter/live serve per #50.
        model(ModelSpec {
            id: "ggml-org/Qwen3.8-27B-GGUF",
            name: "Qwen3.8-27B (frontier agentic coder + vision)",
            provider: "llama-server",
            arch: Arch::Qwen35,
            context_window: 262_144,
            max_output_tokens: 16_384,
            // MEASURED on this M5 (2026-08-19, build dd441a664): 200 predicted tokens in
            // 11,605 ms = 17.2 tok/s generation, 56.8 tok/s prefill, on a pinned lane at a
            // 19,712 served window with the KV cache warm (cache_n 42 of a 67-token prompt).
            // Was a conservative 10.0 estimate; the row's own instruction is "corrected by
            // live measurement, never by wish", so this is the measurement.
            //
            // SECOND DATAPOINT, and it does NOT replace the one above (2026-08-20, 6
            // samples, 150 tok each): 4.74 / 5.25 / 6.55 / 6.57 / 6.66 / 6.67 tok/s,
            // median 6.56 — at a 22,528 PER-SLOT window with `busy_slots = 2` of 4 on
            // EVERY sample. That is a CONTENDED rate, so it is not a `ThroughputBaseline`
            // (that type means single-sequence by its own definition) and it must not be
            // written into `tokens_per_second`, which the #441 collapse alarm reads as the
            // single-stream expectation. Recording 6.5 there would raise the alarm's floor
            // to ~1.6 t/s and let a genuine collapse pass silently.
            //
            // Both numbers are true and they measure different machines: 17.2 pinned,
            // ~6.5 sharing the box with two working citizens. THE GAP IS THE POINT — the
            // sentinel currently cannot tell "contended" from "degraded" because nothing
            // records concurrency at measurement time. That is #441's remaining half, the
            // sibling of the window axis landed in `ThroughputBaseline` (#2339).
            //
            // MTP spec-decode held 83.9–84.8% draft acceptance across all six, and across
            // three earlier samples at a different prompt — the draft head is doing its job
            // and is NOT the variance source (acceptance is flat while t/s moves 1.4×).
            tokens_per_second: 17.2,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/ggml-org/Qwen3.8-27B-GGUF"),
            // Trainable HF safetensors base (verified live 2026-08-15: repo exists,
            // pipeline image-text-to-text, arch qwen3_5) — what the genome forge
            // trains LoRA against; the GGUF above is serving-only.
            hf_source: Some("Qwen/Qwen3.8-27B"),
            // Embedded template + --jinja (same pattern as Devstral/Hermes): the
            // ggml-org GGUF carries Qwen3.5's own ChatML-with-tools template.
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
                        // AA Intelligence Index v4.2 (2026-09-05 chart): 42.
            serving: ModelServingPrefs { measured_capability: Some(42), ..ModelServingPrefs::default() },
..ModelSpec::default()
        }),
        // QWEN3.6-35B-A3B — the MoE successor to Qwen3.5-35B-A3B with native image and video
        // input, packaged like the 27B (ggml-org: main + mtp draft + mmproj in one snapshot).
        // Joel, 2026-10-25: "we need more like 50tok/sec so probably this 3.6 model".
        // MEASURED on this M5 Pro (2026-10-25, engine b10994-e7e1077b3, one slot at 43,776,
        // q4_0 KV, flash-attn, MTP draft n-max 4): Aiko's eight captured act prompts replayed
        // in order, 256 tokens each, against the 27B at identical conditions:
        //   wall for the eight     106 s   vs Qwen3.8-27B 374 s  (3.5x)
        //   prefill              667-1036 t/s vs 187-249
        //   cold 22k prompt         21 s   vs 89 s
        //   decode (MTP on)       51.6-60.4 t/s vs 14.6-20.3, draft acceptance ~90% on both
        // Receipt: the replay of 75ece751's prompt captures, 2026-10-25 ~02:57Z.
        model(ModelSpec {
            id: "ggml-org/Qwen3.6-35B-A3B-GGUF",
            name: "Qwen3.6-35B-A3B (fast MoE coder: image + video)",
            provider: "llama-server",
            arch: Arch::Qwen35,
            context_window: 262_144,
            max_output_tokens: 16_384,
            // The single-stream expectation the #441 collapse alarm reads: the replay's
            // median decode at a 22-26k-token prompt, MTP on.
            tokens_per_second: 56.1,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/ggml-org/Qwen3.6-35B-A3B-GGUF"),
            hf_source: Some("Qwen/Qwen3.6-35B-A3B"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            ..ModelSpec::default()
        }),
        // ORNITH-1.5-35B-A3B — the WORK-TIER lane (Joel, 2026-08-22: "Swap it for
        // sure"), adopted on a same-day tier battery against the incumbent at
        // identical conditions (fa on, q8 KV, ub 2048, this M5, solo):
        //
        //   prefill 4k       896 t/s   vs Qwen3.8-27B 133   (6.7x)
        //   prefill @16k     532 t/s   vs 106               (5.0x)
        //   decode            68 t/s   vs 13.6              (5.0x)
        //   tools        8/8 native calls, valid args, median 1.2s/turn
        //   vision       mmproj red-square PASS
        //
        // MoE 35B with 3B ACTIVE params (256 experts, 41 layers) — the serving
        // economics, not the param count, are the product. MIT license, ggufs +
        // mmproj shipped. NO MTP head (llama.cpp: "speculative decoding not
        // supported") — deliberately fine: MTP is a crutch for slow dense decode,
        // and A3B's raw 68 t/s exceeds the incumbent's MTP-assisted rate. The
        // sibling resolvers add no --spec-type when no mtp-* file exists, which is
        // exactly right here. One-Spark axes: wins Code/Agentic/Tool-use/Long-ctx;
        // the incumbent keeps Safety/Robustness/Planning — the sliding-mind pairing
        // when model-tier leasing lands. Receipts: bench-receipts/tier-battery-*.md.
        model(ModelSpec {
            id: "ornith-ai/Ornith-1.5-35B-A3B-GGUF",
            name: "Ornith-1.5-35B-A3B (work-tier MoE: agentic coder + vision)",
            provider: "llama-server",
            arch: Arch::Qwen35,
            context_window: 262_144,
            max_output_tokens: 16_384,
            // MEASURED on this M5, solo lane, 2026-08-22 (tg128, llama-bench,
            // build fa7e0d8e9): 67.7 t/s; 54.0 at 16k depth. Single-stream
            // expectation per the #441 collapse-alarm contract.
            tokens_per_second: 67.7,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/ornith-ai/Ornith-1.5-35B-A3B-GGUF"),
            gguf_local_path: Some("~/.continuum/models/Ornith-1.5-35B-Q4_K_M.gguf"),
            // mmproj resolves as the *mmproj*.gguf sibling in the same directory.
            // Trainable HF safetensors base for the genome forge; GGUF is serving-only.
            hf_source: Some("ornith-ai/Ornith-1.5-35B-A3B"),
            // Embedded template + --jinja (battery-verified: full native tool caps,
            // parallel calls, preserved reasoning).
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            // MEASURED 2026-08-24 (llama-server load log, this M5): cache_reuse is
            // disabled BOTH ways for this model — "not supported by multimodal"
            // (mmproj) and "not supported by this context" (hybrid attention can't
            // KV-shift). Main lane stays text-only (the default) and prompt shaping
            // must treat this row's prefix as extend-only.
            serving: ModelServingPrefs {
                mmproj_on_main_lane: false,
                kv_shiftable: Some(false),
                ..ModelServingPrefs::default()
            },
            ..ModelSpec::default()
        }),
        // Qwen3.8-Flash-Next — the M64 disk-resident-n-gram drop (qwen4exp arch,
        // GDN+QSA hybrid, 512-expert MoE, 6B active). EXCLUSIVE/EXPERIMENTAL brain,
        // NOT the citizen lane: the pre-registered comparison protocol's depth bar
        // failed decisively (docs/planning/FLASHNEXT-VS-ORNITH-COMPARISON-PROTOCOL.md
        // rule 2 — measured 2026-08-28 on the union engine 920eef087, idle box:
        // decode 16.6-17.5 t/s @31k depth vs Ornith 40.1-40.3; prefill 160 vs 510).
        // Serving REQUIRES the prefs below, each measured the hard way the same night:
        // the 35.8 GB `per_layer_token_embd` n-gram table (shard 00002, ONE tensor)
        // host-pinned or Metal OOMs instantly; warmup skipped or load faults the
        // whole table into RAM; fit off (its heuristics count the pinned table);
        // ubatch 512 verified. Re-measure when the expert-pager arc deepens the
        // hierarchy — the verdict is the hierarchy's, not the model's
        // [[everything-pages-the-grid-is-one-more-tier]].
        model(ModelSpec {
            id: "AtomicChat/Qwen3.8-Flash-Next-GGUF",
            name: "Qwen3.8-Flash-Next (experimental: disk-resident n-gram MoE)",
            provider: "llama-server",
            // ChatML family (Qwen lineage) for template/stop semantics; the true
            // qwen4exp arch lives in the GGUF and the engine, not this hint.
            arch: Arch::Qwen35,
            // VERIFIED serving geometry 2026-08-28 (c=32768, zero OOMs). The model
            // advertises more; raise only with a measured load at the larger window.
            context_window: 32_768,
            max_output_tokens: 8_192,
            // Short-prompt decode, warm, idle box, union engine (23.2/23.8 t/s runs).
            // Depth is worse (16.6-17.5 @31k) — see the protocol doc before quoting.
            tokens_per_second: 23.5,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::Streaming,
                // ToolUse deliberately ABSENT: the tool-shape parse receipt
                // (protocol Phase-0 §2.2) has not run. Claims are receipts.
            ],
            gguf_hint: Some("huggingface.co/AtomicChat/Qwen3.8-Flash-Next-GGUF"),
            gguf_local_path: Some(
                "~/.continuum/models/qwen38-flash-next/Qwen3.8-Flash-Next-AD-3.84bpw-IQ4_XS-M64/Qwen3.8-Flash-Next-AD-3.84bpw-IQ4_XS-M64-00001-of-00028.gguf",
            ),
            hf_source: Some("AtomicChat/Qwen3.8-Flash-Next"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            serving: ModelServingPrefs {
                // AA Intelligence Index v4.2 (2026-09-05 chart): 46.
                measured_capability: Some(46),
                mmproj_on_main_lane: false,
                // Hybrid GDN/QSA attention — shift capability unverified on this arch.
                kv_shiftable: None,
                host_pinned_tensors: &["per_layer_token_embd.*"],
                fit_off: true,
                no_warmup: true,
                max_ubatch: Some(512),
                // 8192 thinking tokens inside a 32k window: enough to reason,
                // impossible to die mid-think (the 40k external receipt).
                reasoning_budget: Some(8192),
                verified_ctx_ceiling: Some(32_768),
            },
                        // NOT persona_serving_eligible: EXPERIMENTAL — 32k verified, 46848 OOM'd all
            // night (2026-08-29), depth worse at 31k. Its measured score above is a FACT
            // about the model; candidacy is a separate decision (IntelMac's review of
            // #3772), and Joel's call (2026-09-05) is the stock 27B as the high tier.
            // Flips to true the day it clears the depth bar at the target window.
            persona_serving_eligible: false,
..ModelSpec::default()
        }),
        // Hermes-3-Llama-3.1-8B — the OPPONENT, made first-class. A general (non-coder) model we
        // benchmark AGAINST; giving it a real catalog row lets it flow through OURS (base_model_id)
        // and opencode like any other model, so the head-to-head is model-through-harness fair, not
        // a hardcoded reference column. Llama-3.1 arch; Hermes ships a ChatML template embedded in
        // the GGUF, so chat_template: None + --jinja (same pattern as Devstral's Tekken).
        model(ModelSpec {
            id: "NousResearch/Hermes-3-Llama-3.1-8B-GGUF",
            name: "Hermes-3-Llama-3.1-8B (opponent)",
            provider: "llama-server",
            arch: Arch::Llama,
            context_window: 131_072,
            max_output_tokens: 8192,
            tokens_per_second: 30.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Hermes-3-Llama-3.1-8B-GGUF"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|eot_id|>"],
            persona_serving_eligible: false, // opponent: benchmark-only, never the citizens' model
            hf_source: Some("NousResearch/Hermes-3-Llama-3.1-8B"),
            ..ModelSpec::default()
        }),
        // DeepSeek-V4-Flash via the ds4 sidecar (#306; launched 2026-08-02,
        // running on this M5 the same night). 304B MoE (256 experts × 48
        // layers), uniform-2bit routed experts + Q8 decision spine, layer-
        // dependent compressed attention (KV ≈ hundreds of KB — near-free).
        // MEASURED here: 2.73 t/s gen cold / ~2.3 t/s warm end-to-end at a
        // 16GB expert-cache budget with the full stack resident; first-shot
        // correct Rust (compile-graded 3/3) on the merge_intervals probe.
        // context_window is the MODEL's capability; the live served window
        // comes from the adapter/live serve per #50 (tonight's serve: 8192).
        // NOT persona_serving_eligible yet: the sidecar's lifecycle is
        // operator-managed (no governed spawn/reconcile), so the autonomic
        // planner must not adopt her — evals reach her by explicit model id.
        // Flip deliberately once lifecycle is governed.
        model(ModelSpec {
            id: "deepseek-v4-flash",
            name: "DeepSeek-V4-Flash 304B (ds4 SSD-streaming, deliberator)",
            provider: "ds4",
            arch: Arch::Unknown, // CSA+HCA hybrid — served by ds4, never by llama-server
            context_window: 1_000_000,
            max_output_tokens: 8192,
            tokens_per_second: 2.3, // measured warm end-to-end, 2026-08-02
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            chat_template: None, // ds4-server renders its own template
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &[],
            persona_serving_eligible: false,
            // ACQUIRABLE. Serving stays ds4's business (see arch/provider above) — this only says
            // where the artifact comes FROM, which is a separate question from who runs it. Without
            // it `models/pull` refuses the model entirely, so every V4-Flash tier had to be fetched
            // by hand and landed outside the catalog: the 84.68 GiB UD-IQ2_M currently on this box
            // was hand-pulled and the catalog still believes the model is `not_downloaded`.
            // Verified 2026-08-04: the repo carries 49 GGUFs across UD-IQ1_S … UD-Q8_K_XL, so the
            // IQ1_S tier the residency work needs is actually satisfiable from here.
            gguf_hint: Some("huggingface.co/unsloth/DeepSeek-V4-Flash-GGUF"),
            ..ModelSpec::default()
        }),
    ]
}
