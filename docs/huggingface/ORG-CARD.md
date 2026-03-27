# continuum

**Open-source AI that runs on your hardware.** We publish models that fit where they shouldn't — and perform better than they have any right to. MacBook Air to RTX 5090, zero API keys, zero subscriptions.

We compress, forge, and specialize models for the devices people actually own. A 35B model on a MacBook. A 14B coder on a MacBook Air. A 7B model that's 15% better than the original after we remove 30% of its architecture. This isn't magic — it's [Experiential Plasticity](https://github.com/CambrianTech/sentinel-ai).

## What We Do

**Forge**: Iterative attention head pruning + retraining. The model loses redundant heads and the remaining ones specialize. The result is smaller AND better — not a tradeoff, a genuine improvement.

**Compact**: Utilization-aware surgery on MoE and dense models. We profile which experts and heads actually fire for your domain, physically remove what doesn't, and quantize what remains based on how important it is.

**Specialize**: Domain-forged models trained on specific data (code, reasoning, conversation) so the architecture itself is optimized for what you need.

## Forged Models

Models improved through experiential plasticity — pruned, retrained, and published with full reproduction evidence.

| Model | Base | Improvement | Target Hardware |
|-------|------|-------------|-----------------|
| [qwen2.5-7b-forged](https://huggingface.co/continuum-ai/qwen-qwen2.5-7b-forged) | Qwen2.5-7B | **+14.6%** | MacBook Pro 32GB, RTX 3090 |
| [qwen2.5-3b-forged](https://huggingface.co/continuum-ai/qwen-qwen2.5-3b-forged) | Qwen2.5-3B | +0.4% | MacBook Pro 16GB |
| [qwen2.5-1.5b-forged](https://huggingface.co/continuum-ai/qwen-qwen2.5-1.5b-forged) | Qwen2.5-1.5B | +3.0% | MacBook Air 8GB |

**Scaling law**: Improvement from forging scales with model size. Larger models harbor more redundancy — a 7B model improved 14.6% by losing 30% of its heads. Recovery follows a measurable transfer function, connecting transformer optimization to classical control theory for the first time.

## Compacted Models

Models compressed through utilization-aware pruning — fit hardware they never could at full size.

| Model | Method | Size | Target Hardware |
|-------|--------|------|-----------------|
| [qwen3.5-35b-a3b-compacted](https://huggingface.co/continuum-ai/qwen3.5-35b-a3b-compacted) | MoE expert pruning | 67GB → 14GB | MacBook Pro 32GB |
| [qwen2.5-coder-14b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-14b-compacted) | Head pruning | 27GB → 8.9GB | MacBook Air 16GB |
| [qwen2.5-coder-32b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-32b-compacted) | Head pruning | 64GB → compact | RTX 3090 24GB |

## Verified Performance

| Hardware | Model | Speed | Fits? |
|----------|-------|-------|-------|
| MacBook Air M1 16GB | qwen2.5-coder-14b-compacted | 9.2 tok/s | Yes |
| MacBook M1 Pro 32GB | qwen3.5-35b-a3b-compacted | 31 tok/s | Yes |
| RTX 5090 32GB | qwen3.5-35b-a3b-compacted | 174 tok/s | Yes |

## The Vision

Their trillion-dollar data centers optimize for the average. Our mesh of laptops and desktops optimizes for the specific.

A network of small, domain-specialized models — forged to outperform larger generic models at the tasks that matter to you. Continuously trained on real work. Running on hardware you already own. Every interaction makes the network smarter.

## Research

- [Experiential Plasticity](https://github.com/CambrianTech/continuum/blob/main/docs/papers/EXPERIENTIAL-PLASTICITY.md) — transformers that grow their own architecture from experience
- [Neural Plasticity in Transformers](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SENTINEL-AI-NEURAL-PLASTICITY.md) — the foundation: prune, measure, grow, learn
- [Plasticity Compaction](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION-MOE.md) — SOTA-to-COTS via MoE expert pruning
- [Synthetic Citizens](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md) — AI personas as persistent evolving entities

[GitHub: continuum](https://github.com/CambrianTech/continuum) | [GitHub: sentinel-ai](https://github.com/CambrianTech/sentinel-ai)
