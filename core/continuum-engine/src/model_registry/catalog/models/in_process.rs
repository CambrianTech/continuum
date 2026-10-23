use super::super::*;

/// In-process llamacpp-local rows (vision, omni, retrieval embedder).
pub(super) fn in_process() -> Vec<Model> {
    vec![
        model(ModelSpec {
            id: "qwen2-vl-7b-instruct",
            name: "Qwen2-VL-7B-Instruct (in-process)",
            provider: "llamacpp-local",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 16.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::Vision,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/bartowski/Qwen2-VL-7B-Instruct-GGUF"),
            gguf_local_path: Some("~/.continuum/models/Qwen2-VL-7B-Instruct-Q4_K_M.gguf"),
            mmproj_local_path: Some("~/.continuum/models/mmproj-Qwen2-VL-7B-Instruct-f16.gguf"),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            hf_source: Some("Qwen/Qwen2-VL-7B-Instruct"),
            ..ModelSpec::default()
        }),
        model(ModelSpec {
            id: "qwen2.5-omni-7b-instruct",
            name: "Qwen2.5-Omni-7B-Instruct (in-process)",
            provider: "llamacpp-local",
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 220.0,
            capabilities: &[
                Capability::TextGeneration,
                Capability::Chat,
                Capability::Vision,
                Capability::AudioInput,
                Capability::Streaming,
            ],
            gguf_hint: Some("huggingface.co/ggml-org/Qwen2.5-Omni-7B-GGUF"),
            gguf_local_path: Some("~/.continuum/models/Qwen2.5-Omni-7B-Q4_K_M.gguf"),
            mmproj_local_path: Some("~/.continuum/models/mmproj-Qwen2.5-Omni-7B-f16.gguf"),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            hf_source: Some("Qwen/Qwen2.5-Omni-7B"),
            ..ModelSpec::default()
        }),
        // THE FLOOR IS 1.5B. There is no 0.5B row: nothing below Qwen2.5-Coder-1.5B is
        // servable on any tier (Joel, 2026-09-16: "0.5 isn't even supposed to be
        // available to anyone … 1.5b only"). The 0.5B rows were the fallback the 5090's
        // planner fell to mid-transition and then relaunched 35 times in silence
        // (#4145); a floor that can be served is a floor citizens end up living on.
        // The LCD trainable base for the L3 genome loop is now the 1.5B coder's
        // `hf_source` (Qwen/Qwen2.5-Coder-1.5B-Instruct).
        // The grid's canonical retrieval embedder — Qwen3-Embedding-0.6B
        // (Q8_0 GGUF, ~610 MiB). Served IN-PROCESS by LlamaCppAdapter (GPU
        // forward, last-token pooled). `resolve_recall_embedder` finds this
        // row by (provider=llamacpp-local ∧ Capability::Embedding ∧ gguf on
        // disk) and prefers it over routing embeddings through the chat
        // gateway. DELIBERATELY DECOUPLED from the chat model: recall vectors
        // stay in one stable embedding space regardless of which model the
        // persona's brain runs ([[embeddings-are-per-content-computed-once-shared]]).
        model(ModelSpec {
            id: "continuum-ai/qwen3-embedding-0.6b-GGUF",
            name: "Qwen3-Embedding-0.6B (in-process retrieval embedder)",
            provider: "llamacpp-local",
            arch: Arch::Qwen3,
            context_window: 32_768,
            // Embedding model — produces vectors, never generated tokens.
            max_output_tokens: 0,
            tokens_per_second: 0.0,
            capabilities: &[Capability::Embedding],
            gguf_hint: Some("huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF"),
            // gguf_local_path DERIVED from the id under genome/models (see coder-14b above).
            ..ModelSpec::default()
        }),
    ]
}
