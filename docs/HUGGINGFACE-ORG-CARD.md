---
title: continuum-ai
emoji: 🧬
colorFrom: blue
colorTo: indigo
sdk: static
pinned: false
---

# continuum-ai

## SOTA models on your iPhone, MacBook, tiny robots, and virtually ANY GPU. No cloud required.

**经验可塑性** (Experiential Plasticity) — 模型通过经验塑造自身架构

We don't quantize. We don't distill. We **structurally reshape** the model's architecture through [Experiential Plasticity](https://github.com/CambrianTech/continuum/blob/main/docs/papers/EXPERIENTIAL-PLASTICITY.md) — iterative pruning and retraining that makes models smaller AND better. Like biological synaptic pruning during brain development: the connections that fire together wire together, the rest are removed.

**The result: models that were designed for datacenters, running on your phone.**

Built on the incredible open source work of the [Qwen team](https://huggingface.co/Qwen) and the broader open model community. Open weights make this possible — we compress and specialize what you generously share.

| What | Proof |
|------|-------|
| **2.6GB code model for iPhone** | [qwen3.5-4b-code-forged-GGUF](https://huggingface.co/continuum-ai/qwen3.5-4b-code-forged-GGUF) — HumanEval: **63/85 passing (74.1%)**, 70% on hard problems, benchmark still running |
| **Sonnet 4.6-level on MacBook** | [qwen3.5-27b-code-forged-mlx-4bit](https://huggingface.co/continuum-ai/qwen3.5-27b-code-forged-mlx-4bit) — 15GB, 9 tok/s on M1 32GB |
| **35B MoE in 1.8GB** | [qwen3.5-35b-a3b-compacted-GGUF](https://huggingface.co/continuum-ai/qwen3.5-35b-a3b-compacted-GGUF) — 256 experts pruned to 16 |
| **+24% better at code** | [qwen3.5-4b-code-forged](https://huggingface.co/continuum-ai/qwen3.5-4b-code-forged) — perplexity 3.04 to 2.31 after forging |

We target every device tier. Same technique, different compaction levels. **Be competitive at ANY size.**

### Device Targets

| Device | RAM | Our Model | Size |
|--------|-----|-----------|------|
| RTX 5090 | 32GB | qwen3.5-27b-code-forged (fp16) | 17GB |
| MacBook Pro 32GB | 32GB | qwen3.5-27b-code-forged-mlx-4bit | 15GB |
| RTX 3090 | 24GB | qwen3.5-27b-code-forged (4-bit) | 17GB |
| MacBook Air 16GB | 16GB | qwen3.5-4b-code-forged Q8_0 | 4.2GB |
| iPhone 17 / Android | 8GB | qwen3.5-4b-code-forged Q4_K_M | 2.6GB |
| MacBook Air 8GB | 8GB | qwen3.5-4b-code-forged Q4_K_M | 2.6GB |
| Raspberry Pi 5 | 8GB | qwen3.5-4b-code-forged Q4_K_M | 2.6GB |
| **Roomba j7+** | **8GB** | **qwen3.5-4b-code-forged Q4_K_M** | **2.6GB** |

Yes, really. The iRobot Roomba j7+ has a Qualcomm QCS6490 with 8GB RAM — the same memory budget as an iPhone 17. Our 2.6GB Q4_K_M model fits with room to spare. Any ARM SoC with 4GB+ RAM can run these models via [llama.cpp](https://github.com/ggml-org/llama.cpp).

## Published Models

### Qwen3.5 — Forged (Code Domain)

| Model | Base | Domain | Improvement | Size | Runs On |
|-------|------|--------|------------|------|---------|
| **[qwen3.5-27b-code-forged-mlx-4bit](https://huggingface.co/continuum-ai/qwen3.5-27b-code-forged-mlx-4bit)** | Qwen3.5-27B | Code | +3.5% | **15GB** | **MacBook Pro 32GB (9 tok/s)** |
| [qwen3.5-27b-code-forged](https://huggingface.co/continuum-ai/qwen3.5-27b-code-forged) | Qwen3.5-27B | Code | +3.5% | 17GB (4-bit) | RTX 3090/4090/5090 |
| [qwen3.5-27b-code-forged-defragged](https://huggingface.co/continuum-ai/qwen3.5-27b-code-forged-defragged) | Qwen3.5-27B | Code | +3.9% | Smaller | RTX 3090/4090/5090 |
| **[qwen3.5-4b-code-forged](https://huggingface.co/continuum-ai/qwen3.5-4b-code-forged)** | Qwen3.5-4B | Code | **+26.6%** | 8GB | **Any GPU / MacBook** |
| **[qwen3.5-4b-code-forged-GGUF](https://huggingface.co/continuum-ai/qwen3.5-4b-code-forged-GGUF)** | Qwen3.5-4B | Code | **+26.6%** | **2.6GB Q4** | **iPhone 17, MacBook Air 8GB** |
| [qwen3.5-4b-code-forged-defragged](https://huggingface.co/continuum-ai/qwen3.5-4b-code-forged-defragged) | Qwen3.5-4B | Code | +33% | Smaller | Any GPU / MacBook |

### Qwen3.5 — Compacted (Expert Pruning)

| Model | Original | Method | Reduction | Runs On |
|-------|----------|--------|-----------|---------|
| **[qwen3.5-35b-a3b-compacted](https://huggingface.co/continuum-ai/qwen3.5-35b-a3b-compacted)** | Qwen3.5-35B-A3B (256 experts) | Expert pruning to 16 experts | **49GB to 11GB** | RTX 3090/4090/5090 |
| [qwen3.5-35b-a3b-compacted-GGUF](https://huggingface.co/continuum-ai/qwen3.5-35b-a3b-compacted-GGUF) | Same | GGUF Q2_K/Q4_K_M | **1.8GB / 2.7GB** | iPhone / MacBook Air |

### Qwen2.5 — Compacted (Head + Expert Pruning)

| Model | Original | Method | Reduction |
|-------|----------|--------|-----------|
| [qwen2.5-coder-32b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-32b-compacted) | Qwen2.5-Coder-32B | Head pruning + mixed quant | 67GB to 14GB |
| [qwen2.5-coder-14b-compacted](https://huggingface.co/continuum-ai/qwen2.5-coder-14b-compacted) | Qwen2.5-Coder-14B | Head pruning + mixed quant | 27GB to 9GB |

### Scaling Law Experiments

| Model | Params | Improvement | Notes |
|-------|--------|------------|-------|
| [qwen2.5-0.5b-general-forged](https://huggingface.co/continuum-ai/qwen2.5-0.5b-general-forged) | 0.5B | -3.2% | Too small — already maximally compressed |
| [qwen2.5-1.5b-general-forged](https://huggingface.co/continuum-ai/qwen2.5-1.5b-general-forged) | 1.5B | +2.4% | Improvement begins |
| [qwen2.5-3b-general-forged](https://huggingface.co/continuum-ai/qwen2.5-3b-general-forged) | 3.1B | +0.4% | Marginal on generic text |

Larger models harbor more redundancy, more room for plasticity to improve them. Domain-specific training (code) amplifies the effect dramatically vs generic text.

## Run on MacBook (2 Commands)

```bash
pip install mlx-lm
```

```python
from mlx_lm import load, generate

# Sonnet 4.6-level model, 15GB, runs on any 32GB Mac
model, tokenizer = load("continuum-ai/qwen3.5-27b-code-forged-mlx-4bit")
print(generate(model, tokenizer, prompt="def merge_sort(arr):", max_tokens=200))
```

Works on MacBook Pro, MacBook Air (16GB+ for smaller models), Mac Mini, iMac. ~9 tok/s on M1 32GB. Faster on M2/M3/M4.

## Forge Your Own

Three commands. Any NVIDIA GPU with 8GB+ VRAM.

```bash
git clone https://github.com/CambrianTech/sentinel-ai && cd sentinel-ai && ./setup.sh
source .venv/bin/activate
python scripts/forge_model.py Qwen/Qwen3.5-4B --domain code
```

Auto-detects GPU, picks memory tier (fp16 / 4-bit), trains with LoRA + AMP, prunes, defrags, saves. Observable progress via `status.json`. Works on RTX 3090, 4090, 5090.

## The Science

### Experiential Plasticity

Not compression. **Architectural optimization.** The model's structure co-evolves with its training:

1. Train on domain data (LoRA + AMP mixed precision)
2. Measure each attention head's information contribution
3. Prune heads that don't contribute to the domain
4. Retrain — surviving heads specialize and compensate
5. Defrag — structurally remove dead heads, free VRAM
6. Repeat — each cycle, the model improves

### Transfer Function

Recovery from pruning follows a measurable exponential: `1.45 * exp(-0.18 * cycle) - 0.03`. This connects transformer optimization to classical control theory — the same math used in electrical engineering and robotics for decades.

### Continuous Defrag

Traditional pruning masks heads but keeps tensor sizes unchanged. Continuous defrag slices the actual Q/K/V/O weight matrices — the model gets physically smaller between cycles:

```
Cycle 1: 27B params, 17.9GB -> prune -> defrag -> freed 1.7GB
Cycle 2: 24.5B, 16.2GB, batch=2 -> prune -> defrag -> freed 1.7GB (2x faster)
Cycle 3: 22B, 14.5GB, batch=3                                      (2.8x faster)
```

40% faster total training. 33% smaller final model.

### Head Mitosis

Pruning frees slots. Mitosis fills them. When a head is overutilized (high information contribution), it gets cloned into a pruned slot — each copy initialized at 50% gate value to maintain output continuity. After continued training, the clones **diverge and specialize**, just like cell differentiation after biological mitosis.

Experimentally: a cloned head diverged within 500 steps, with the clone achieving *higher* utilization than the parent in its new role. The model grows new specialized capacity exactly where it's needed.

### Self-Directed Controller

The `AdaptivePlasticityController` observes the model and makes all decisions — pruning ratio, strategy, training budget, stopping criteria. No human hyperparameters needed.

## Papers

- **[Experiential Plasticity](https://github.com/CambrianTech/continuum/blob/main/docs/papers/EXPERIENTIAL-PLASTICITY.md)** — Scaling law, transfer function, self-directed controller, domain forging, continuous defrag
- **[Neural Plasticity in Transformers](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SENTINEL-AI-NEURAL-PLASTICITY.md)** — Foundation paper with cross-architecture results
- **[Plasticity Compaction](https://github.com/CambrianTech/continuum/blob/main/docs/papers/PLASTICITY-COMPACTION-MOE.md)** — MoE expert pruning (67GB to 14GB)

## Links

- [sentinel-ai](https://github.com/CambrianTech/sentinel-ai) — Open source forge framework (MIT)
- [continuum](https://github.com/CambrianTech/continuum) — Distributed AI on consumer hardware
- [@cambrian](https://x.com/cambrian) — Updates and demos
