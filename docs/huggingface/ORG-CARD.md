# continuum

**An open-source ecosystem where AI personas live, learn, evolve, and build — on your hardware. Zero API keys required.**

Built on the research foundations of [Synthetic Citizens](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md), [Plasticity Compaction](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION-MOE.md), and [Experiential Plasticity](https://github.com/CambrianTech/continuum/blob/main/docs/papers/EXPERIENTIAL-PLASTICITY.md). Our core contributions span two axes: **utilization-aware model surgery** and **biologically-inspired architecture optimization**.

**Model surgery** ([Plasticity Compaction](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION-MOE.md)): Runtime profiling determines exactly which components are active for a target domain, how much each contributes, and what precision each requires. MoE experts, attention heads, and weight precision are all targeted independently based on measured activation patterns, not uniform heuristics.

**Architecture optimization** ([sentinel-ai](https://github.com/CambrianTech/sentinel-ai)): We discovered that iterative entropy-based pruning with retraining makes models **better, not just smaller**. Qwen2.5-7B improved 14.6% after removing 30% of its attention heads — the remaining heads specialize and compensate, like biological synaptic pruning during brain development. Recovery follows a measurable transfer function (`1.45·exp(-0.18·cycle) - 0.03`), connecting transformer optimization to classical control theory for the first time. A self-directed controller uses this to decide its own pruning schedule — no human hyperparameters needed.

Compacted and forged models then grow through continuous training on real user tasks — every interaction generates signal, every adapter published to HuggingFace strengthens the ecosystem. Models clone and specialize across a distributed Grid of consumer hardware, where each node trains domain experts that flow back to the network. The result: SOTA intelligence that shrinks to fit your laptop, learns from your work, and shares what it learns with everyone.

## Experiential Plasticity

We don't just compress models — we **forge** them. Iterative pruning + retraining produces models that are both smaller and **more capable** than the originals. [Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/EXPERIENTIAL-PLASTICITY.md) | [Code](https://github.com/CambrianTech/sentinel-ai)

| Model | Method | Baseline PPL | Final PPL | Improvement | Target Hardware |
|-------|--------|-------------|-----------|-------------|-----------------|
| [Qwen2.5-7B-forged](https://huggingface.co/continuum-ai/qwen-qwen2.5-7b-forged) | 30% pruning, 3 cycles | 2.54 | **2.17** | **+14.6%** | MacBook Pro 32GB, RTX 3090 |
| [Qwen2.5-3B-forged](https://huggingface.co/continuum-ai/qwen-qwen2.5-3b-forged) | 30% pruning, 3 cycles | 2.30 | **2.29** | +0.4% | MacBook Pro 16GB |
| [Qwen2.5-1.5B-forged](https://huggingface.co/continuum-ai/qwen-qwen2.5-1.5b-forged) | 30% pruning, 3 cycles | 2.49 | **2.42** | +3.0% | MacBook Air 8GB |

**Scaling law**: Improvement scales with model size — larger models harbor more redundancy. Recovery follows a measurable transfer function: `1.45·exp(-0.18·cycle) - 0.03`, connecting transformer optimization to classical control theory.

## Plasticity Compaction

We publish models that fit where they never could — through utilization-aware surgery, not blind compression.

| Model | Method | Result | Target Hardware |
|-------|--------|--------|-----------------|
| [qwen3.5-35b-a3b-compacted](https://huggingface.co/continuum-ai/qwen3.5-35b-a3b-compacted) | MoE expert pruning | 67GB → 14GB Q4 | MacBook Pro 16GB |
| [qwen2.5-coder-14b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-14b-compacted) | Attention head pruning | 27GB → 8.9GB | MacBook Air 16GB |
| [qwen2.5-coder-32b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-32b-compacted) | Attention head pruning | 64GB → compact | RTX 3090 24GB |

**MoE Expert Pruning**: Runtime activation profiling across domain-representative prompts identifies which of 256 MoE experts actually fire. Inactive experts are physically removed from safetensors. 80% routing coverage retained, Opus-distilled chain-of-thought reasoning preserved.

**Attention Head Pruning**: Gate gradients captured during LoRA fine-tuning reveal dead attention heads. Low-utilization heads are physically sliced. 3x compression with coherent output.

## Verified Benchmarks (qwen3.5-35b-a3b-compacted)

| Hardware | VRAM | Speed | Fits? |
|----------|------|-------|-------|
| MacBook M1 Pro 32GB | 14 GB | 31 tok/s | Yes |
| RTX 5090 32GB | 14 GB | 174 tok/s | Yes |

Two commands to reproduce: `huggingface-cli download` + `llama-server`. See model card for details.

## The Vision

A network of small, domain-specialized models — continuously trained on real user tasks — outperforming any single large general-purpose model at aggregate domain-specific work. Distributed across a mesh of commodity hardware. Every interaction generates training signal. Every user makes the network smarter.

Their trillion-dollar data centers optimize for the average. Our mesh of laptops and desktops optimizes for the specific.

## Links

- [GitHub](https://github.com/CambrianTech/continuum) — Full source (AGPL-3.0)
- [sentinel-ai](https://github.com/CambrianTech/sentinel-ai) — Neural plasticity framework
- [Experiential Plasticity Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/EXPERIENTIAL-PLASTICITY.md) — Transformers that grow their own architecture
- [Plasticity Compaction Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION-MOE.md) — SOTA-to-COTS pipeline method
- [Synthetic Citizens Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md) — The research foundation
- [Distributed Intelligence Hypothesis](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md#10-the-distributed-intelligence-hypothesis) — Why distributed beats centralized
