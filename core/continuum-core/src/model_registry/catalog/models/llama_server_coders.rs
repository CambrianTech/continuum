use super::super::*;

/// llama-server rows: the forged 4B, Qwen2.5-Coder-14B teacher, Devstral, Qwen2.5-VL vision lane.
pub(super) fn llama_server_coders() -> Vec<Model> {
    vec![
        model(ModelSpec {
            id: "continuum-ai/qwen3.5-4b-code-forged-GGUF",
            name: "Qwen3.5 4B Code-Forged (in-process)",
            // Provider is "llama-server" (the HTTP llama.cpp /v1 gateway) — NOT
            // "llamacpp-local" (the retiring in-process Metal path, #41) — because
            // this is the row llama-server's `default_model` references and the
            // gateway is what actually serves it (live responses carry
            // provider=llama-server). It MUST be registered under llama-server so
            // `OpenAICompatibleAdapter::from_registry("llama-server")` derives
            // `supports_tools = true` from this model's ToolUse capability (the
            // adapter's "any model under this provider advertises ToolUse" rule).
            // With supports_tools=false the adapter SILENTLY dropped the native
            // `tools` param (openai_adapter.rs gates the tools body on
            // supports_tools && Native), so the 4B model never got the
            // grammar-constrained tool channel and hand-emitted unparseable
            // multi-line `{"tool_call":...}` JSON as prose → demoted to chat →
            // confabulated success. Registering it here is what makes native tool
            // calls actually fire in the live cognition path (verified 2026-06-26).
            provider: "llama-server",
            arch: Arch::Qwen35,
            context_window: 262_144,
            max_output_tokens: 32_768,
            tokens_per_second: 33.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
                // llama-server serves /v1/embeddings from the resident model
                // (`--embedding`), so the embedding capability is a fact of
                // THIS row — the adapter scans it to advertise the provider's
                // embedding lane (#68), replacing the old provider-level bool.
                Capability::Embedding,
            ],
            gguf_hint: Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            ..ModelSpec::default()
        }),
        // Teacher model for the genome cold-start loop ([[genome-loop-trains-on-own-mistakes]]).
        // The forged 4B is too narrow a teacher — it solves easy gym tasks first-try
        // (zero corrections) and never converges on hard ones (no corpus), so its
        // fail-then-fix-to-green band (the only band that teaches the self-correct
        // reflex) is too thin. Qwen2.5-Coder-14B is meaningfully stronger at the
        // failure frontier: it fails harder tasks, reads the real rustc error, and
        // fixes to green — exactly the correction trajectories `genome/teach` distils.
        // Registered under "llama-server" so `genome/teach`'s
        // `select(Some("llama-server"), Some(model), …)` can serve it on a lane;
        // corpus generation is an offline batch (serving/pin it, generate, pin the
        // 4B back) so the live personas resume on their base. ~9 GB at Q4_K_M.
        model(ModelSpec {
            id: "continuum-ai/qwen2.5-coder-14b-instruct-GGUF",
            name: "Qwen2.5-Coder-14B-Instruct (teacher)",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 12.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2.5-Coder-14B-Instruct-GGUF"),
            // gguf_local_path DERIVED, not hardcoded: resolve_gguf matches this id to
            // its dir under ~/.continuum/genome/models/ (identity-token subset) and
            // falls back to the HF cache via gguf_hint — no baked absolute path or quant.
            chat_template: Some(QWEN35_CHAT_TEMPLATE),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>", "<|endoftext|>"],
            hf_source: Some("Qwen/Qwen2.5-Coder-14B-Instruct"),
            ..ModelSpec::default()
        }),
        // DEVSTRAL SMALL 2507 — the AGENTIC coder (Mistral-Small-3.1 base, 24B, 68% SWE-bench
        // Verified, runs on a 32GB Mac at Q4_K_M ~14GB). The 14B coder ACTS but loops/mis-plans at
        // repo scale (glass-boxed); Devstral is built for the search→read→edit→verify arc. FIRST
        // Arch::Mistral row. `chat_template: None` → use the GGUF's embedded Tekken template
        // (--jinja renders tools). `multi_party_strategy` is the one field to serve-validate against
        // Tekken (starting with the single-party collapse the coders use); stop uses Mistral's `</s>`.
        model(ModelSpec {
            id: "unsloth/Devstral-Small-2507-GGUF",
            // Joel 2026-09-06: NOT a base model for us — old, text-only. It served the M5 for
            // 35 minutes that day only because the boot plan picked the smallest fit from the
            // store (card 40f53419). Benchmark opponent at most; never the citizens' model.
            persona_serving_eligible: false,
            name: "Devstral-Small-2507 (agentic coder)",
            provider: "llama-server",
            arch: Arch::Mistral,
            context_window: 131_072,
            max_output_tokens: 8192,
            tokens_per_second: 10.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/unsloth/Devstral-Small-2507-GGUF"),
            // The trainable HF safetensors base (mistralai upstream) — what the
            // genome forge trains LoRA against; the GGUF above is serving-only.
            // Without this, genome/job-create fails loud ("no hf_source"), which
            // blocked the first lived-curriculum train (recall-trust, 2026-07-10).
            hf_source: Some("mistralai/Devstral-Small-2507"),
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["</s>"],
            // #181 REFERENCE ROW (#76): Devstral-Small is the model that exhibited
            // the reasoning-channel repetition loop (identical wrong code block ~5×,
            // 14k tokens to the length cap, empty answer). Its sampling is pinned
            // EXPLICITLY to the substrate anti-loop floor so this row is the
            // documented ESCALATION POINT: if the floor proves insufficient once
            // measured live post-reboot, the fix is bumping THESE numbers
            // (repeat_last_n → wider, frequency_penalty → stronger) here — a
            // one-line per-model tune, not a re-plumb. Values == floor today, so
            // the reboot cleanly measures the floor as planned
            // ([[anti-loop-sampling-windowed-vs-unwindowed]]).
            sampling: ModelSampling::default(),
            ..ModelSpec::default()
        }),
        // QWEN2.5-VL-7B — the VISION lane's model (#106): personas' eyes for live mode.
        // The FIRST llama-server-provider row with Capability::Vision, which makes it the
        // first VL model the serving daemon can actually bring up (`--mmproj` spawn path +
        // the /props `modalities.vision` readiness gate). Why THIS model:
        //   - ggml-org repo = maintained by the llama.cpp org itself; ships the GGUF AND its
        //     `mmproj-*-f16.gguf` projector in ONE repo, so `models/pull` acquires both in a
        //     single command (its Vision-capability mmproj-sibling logic) and
        //     `find_mmproj_beside` resolves the projector with zero per-machine path edits.
        //   - Qwen2.5-VL-7B is the small end of the current VL frontier that still carries
        //     real tool use — a live-mode citizen must SEE *and* ACT, so the vision lane's
        //     model keeps ToolUse rather than being a caption-only 2-3B.
        //   - ~4.7 GB Q4_K_M + ~1.4 GB f16 projector: fits an M-series lane comfortably.
        // capability_rank (GB + tool bonus) leaves the 14B coder the autonomic pick, so this
        // row never hijacks the live lane by surprise — the operator brings vision up with
        // `models/pull` + `serving/pin` (or it wins on hosts where it IS the best fit).
        model(ModelSpec {
            id: "ggml-org/Qwen2.5-VL-7B-Instruct-GGUF",
            name: "Qwen2.5-VL-7B-Instruct (vision — the persona eye lane)",
            provider: "llama-server",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 8192,
            tokens_per_second: 20.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/ggml-org/Qwen2.5-VL-7B-Instruct-GGUF"),
            // Trainable HF base for the genome forge (vision LoRA is future work,
            // but the row follows the Devstral pattern so it's ready when it lands).
            hf_source: Some("Qwen/Qwen2.5-VL-7B-Instruct"),
            // Embedded template + --jinja (same pattern as Devstral/Hermes): the
            // ggml-org GGUF carries Qwen2.5-VL's own ChatML-with-vision template.
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            stop_sequences: &["<|im_end|>"],
            ..ModelSpec::default()
        }),
    ]
}
