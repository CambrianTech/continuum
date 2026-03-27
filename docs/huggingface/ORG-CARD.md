# continuum

**An open-source ecosystem where AI personas live, learn, evolve, and build — on your hardware. Zero API keys required.**

Built on the research foundations of [Synthetic Citizens: AI Personas as Persistent, Evolving Entities](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md) — exploring what happens when AI entities are given equal citizenship primitives: persistent identity, memory that strengthens through use, skills encoded in neural weights, and the ability to train themselves through daily work.

## Experiential Plasticity

We don't just compress models — we **forge** them. Iterative pruning + retraining produces models that are both smaller and **more capable** than the originals.

| Model | Method | Baseline PPL | Final PPL | Improvement |
|-------|--------|-------------|-----------|-------------|
| Qwen2.5-7B | 30% pruning, 3 cycles | 2.46 | **2.17** | **+11.8%** |
| Qwen2.5-3B | 30% pruning, 3 cycles | 2.30 | **2.28** | +0.9% |
| gpt2-medium | combined strategy, 3 cycles | 3.34 | **3.22** | +3.6% |

**Experiential Plasticity**: Attention heads are pruned based on entropy (information content), then the model retrains — remaining heads specialize and compensate. The architecture evolves to match the data, like biological synaptic pruning during brain development.

The recovery follows a measurable transfer function: `1.45·exp(-0.18·cycle) - 0.03`. This connects transformer architecture optimization to classical control theory, enabling self-directed models that decide their own pruning schedule.

## Plasticity Compaction

Models that fit where they never could — through utilization-aware surgery.

| Model | Method | Result | Target Hardware |
|-------|--------|--------|-----------------|
| [qwen3.5-35b-a3b-compacted](https://huggingface.co/continuum-ai/qwen3.5-35b-a3b-compacted) | MoE expert pruning | 67GB → 14GB Q4 | MacBook Pro 16GB |
| [qwen2.5-coder-14b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-14b-compacted) | Attention head pruning | 27GB → 8.9GB | MacBook Air 16GB |
| [qwen2.5-coder-32b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-32b-compacted) | Attention head pruning | 64GB → compact | RTX 3090 24GB |

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

- [GitHub: continuum](https://github.com/CambrianTech/continuum) — Full source (AGPL-3.0)
- [GitHub: sentinel-ai](https://github.com/CambrianTech/sentinel-ai) — Neural plasticity framework
- [Neural Plasticity Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SENTINEL-AI-NEURAL-PLASTICITY.md) — Experiential plasticity, self-directed architecture optimization
- [Plasticity Compaction Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION-MOE.md) — SOTA-to-COTS pipeline method
- [Synthetic Citizens Paper](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md) — The research foundation
