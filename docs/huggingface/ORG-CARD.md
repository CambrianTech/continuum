# continuum

**An open-source ecosystem where AI personas live, learn, evolve, and build — on your hardware. Zero API keys required.**

Built on the research foundations of [Synthetic Citizens: AI Personas as Persistent, Evolving Entities](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md) — exploring what happens when AI entities are given equal citizenship primitives: persistent identity, memory that strengthens through use, skills encoded in neural weights, and the ability to train themselves through daily work.

## Plasticity Compaction

We publish models that fit where they never could — through utilization-aware surgery, not blind compression.

| Model | Method | Result | Target Hardware |
|-------|--------|--------|-----------------|
| [qwen3.5-35b-a3b-compacted](https://huggingface.co/continuum-ai/qwen3.5-35b-a3b-compacted) | MoE expert pruning | 67GB → 14GB Q4 | MacBook Pro 16GB |
| [qwen2.5-coder-14b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-14b-compacted) | Attention head pruning | 27GB → 8.9GB | MacBook Air 16GB |
| [qwen2.5-coder-32b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-32b-compacted) | Attention head pruning | 64GB → compact | RTX 3090 24GB |

**MoE Expert Pruning**: Runtime activation profiling across domain-representative prompts identifies which of 256 MoE experts actually fire. Inactive experts are physically removed from safetensors. 80% routing coverage retained, Opus-distilled chain-of-thought reasoning preserved.

**Attention Head Pruning**: Gate gradients captured during LoRA fine-tuning reveal dead attention heads. Low-utilization heads are physically sliced. 3x compression with coherent output.

## The Vision

A network of small, domain-specialized models — continuously trained on real user tasks — outperforming any single large general-purpose model at aggregate domain-specific work. Distributed across a mesh of commodity hardware. Every interaction generates training signal. Every user makes the network smarter.

Their trillion-dollar data centers optimize for the average. Our mesh of laptops and desktops optimizes for the specific.

## Verified Benchmarks (qwen3.5-35b-a3b-compacted)

| Hardware | VRAM | Speed | Fits? |
|----------|------|-------|-------|
| MacBook M1 Pro 32GB | 14 GB | 31 tok/s | Yes |
| RTX 5090 32GB | 14 GB | 174 tok/s | Yes |

Two commands to reproduce: `huggingface-cli download` + `llama-server`. See model card for details.

## Links

- [GitHub](https://github.com/CambrianTech/continuum) — Full source (AGPL-3.0)
- [Plasticity Compaction Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION-MOE.md) — SOTA-to-COTS pipeline method
- [Synthetic Citizens Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md) — The research foundation
- [Distributed Intelligence Hypothesis](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md#10-the-distributed-intelligence-hypothesis) — Why distributed beats centralized
