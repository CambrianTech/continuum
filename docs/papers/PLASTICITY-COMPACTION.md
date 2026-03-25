# Utilization-Aware Head Pruning and Mixed Quantization for Device-Targeted Language Models

## Abstract

We present a plasticity-driven compaction pipeline that produces device-specific language model artifacts from a single training run. During LoRA fine-tuning, per-head gradient magnitudes are captured via an EMA-smoothed callback, producing a utilization map across all attention heads. Heads with low utilization are pruned entirely; remaining heads receive quantization precision (Q2K through BF16) proportional to their measured contribution. This produces mixed-quantization GGUF models optimized for specific memory budgets — targeting MacBook Air (11GB), MacBook Pro (16GB), and RTX 5090 (28GB) from one 27B parameter base model.

Unlike uniform quantization (which treats all heads equally) or magnitude pruning (which ignores task-specific usage), our approach uses actual training gradients to determine what matters for the specific domain being learned. A head that's critical for coding tasks may be irrelevant for creative writing — the utilization map captures this.

**Key result**: Qwen 2.5 Coder 14B compacted from 27GB to 8.9GB (67% reduction, 3x speedup) while maintaining coding capability. Published as `continuum-ai/qwen2.5-coder-14b-compacted` on HuggingFace.

## 1. Introduction

The deployment of large language models on consumer hardware is constrained by memory. A 27B parameter model in BF16 requires ~54GB — exceeding all but the highest-end GPUs. Existing approaches to this problem fall into two categories:

1. **Uniform quantization** (GPTQ, AWQ, GGUF): Apply the same precision to all weights. Simple but wasteful — critical attention heads get the same treatment as dead ones.

2. **Structured pruning** (magnitude-based, lottery ticket): Remove weights below a threshold. Ignores task-specific relevance — a head with small weights may be critical for the specific domain.

We propose a third approach: **utilization-aware compaction**, where training itself determines which heads matter. During LoRA fine-tuning on a target domain (e.g., coding), we capture per-head gradient magnitudes through the LoRA_B projection weights. These gradients reveal which heads are actively learning — and which are dead weight for this specific task.

The key insight is that LoRA training already touches every attention projection (Q, K, V, O). By instrumenting the training loop with a gradient callback, we get a utilization map at zero additional cost — no separate profiling pass, no calibration dataset, no additional forward passes.

## 2. Method

### 2.1 Gate Gradient Capture

During standard PEFT LoRA training, we attach a `GateGradientCallback` to the HuggingFace `SFTTrainer`. At each optimizer step (before `zero_grad`), the callback:

1. Walks the model's transformer layers
2. For each attention projection (Q, K, V, O), finds the LoRA_B weight gradient
3. Reshapes the gradient to per-head dimensions: `[n_heads, head_dim, rank]`
4. Computes the L2 norm per head: `magnitude = ||grad_head||`
5. Normalizes to [0, 1] range
6. Updates an EMA-smoothed score: `score = (1 - α) * score + α * magnitude`

The EMA smoothing (α = 0.1) prevents single-step outliers from dominating. After training completes, the callback writes `gate_gradients.json` containing:

```json
{
  "layer_scores": [[0.82, 0.03, 0.91, ...], ...],
  "num_steps": 4700,
  "model_name": "Qwen/Qwen2.5-Coder-14B-Instruct",
  "num_heads": 40,
  "num_kv_heads": 8
}
```

### 2.2 Utilization Scoring

The scoring engine reads the gate gradient data and classifies each head into action tiers:

| Utilization Score | Action | Precision |
|-------------------|--------|-----------|
| < 0.10 | Prune | Removed entirely |
| 0.10 – 0.30 | Heavy compress | Q2K or Q3K |
| 0.30 – 0.70 | Standard compress | Q4K or Q5K |
| 0.70 – 0.90 | Light compress | Q8_0 |
| > 0.90 | Full precision | BF16 (may benefit from higher-rank LoRA) |

GQA (Grouped Query Attention) constraints are enforced: KV heads cannot be pruned independently of their corresponding Q heads. Minimum head counts per layer prevent architectural collapse.

### 2.3 Device-Targeted Compaction

Given a memory budget (e.g., 16GB for MacBook Pro), the pipeline:

1. Starts with the utilization-scored topology
2. Iteratively adjusts precision tiers to fit the budget
3. Prioritizes keeping high-utilization heads at higher precision
4. Produces a mixed-quantization GGUF where each tensor gets independent precision

The same `gate_gradients.json` produces three different GGUF files by varying only the memory budget parameter.

### 2.4 Physical Head Pruning

Pruned heads are physically removed from the safetensors — not masked or zeroed. The compactor:

1. Loads each safetensor shard
2. For Q/K/V projections: slices out rows corresponding to pruned heads
3. For O projection: slices out columns
4. Writes compacted tensors to new safetensor files
5. Saves `head_topology.json` mapping original → compacted head indices

This produces a genuinely smaller model, not a sparse one.

## 3. Implementation

The pipeline is implemented in Rust (continuum-core) for performance:

- `scoring.rs`: Utilization scoring with configurable thresholds
- `compactor.rs`: Multi-shard safetensor head pruning
- `gguf_writer.rs`: Mixed-quantization GGUF export
- `pipeline.rs`: End-to-end orchestration
- `topology.rs`: Head topology serialization

The gate gradient callback is Python (integrated into `peft-train.py`), as it hooks into the HuggingFace Trainer callback system.

Total pipeline time for a 14B model: ~15 minutes on a single RTX 5090.

## 4. Results

### 4.1 Qwen 2.5 Coder 14B

| Metric | Original (BF16) | Compacted |
|--------|-----------------|-----------|
| Size | 27 GB | 8.9 GB |
| Reduction | — | 67% |
| Speedup | 1x | ~3x |
| Published | — | `continuum-ai/qwen2.5-coder-14b-compacted` |

### 4.2 Qwen 3.5 27B (In Progress)

| Target Device | Memory Budget | Expected Size | Quantization Mix |
|---------------|---------------|---------------|------------------|
| MacBook Air 16GB | 11 GB | ~11 GB | Q3_K_S dominant |
| MacBook Pro 32GB | 16 GB | ~16 GB | Q4_K_M dominant |
| RTX 5090 32GB | 28 GB | ~28 GB | Q8/BF16 mixed |

Training currently running on RTX 5090. Gate gradients accumulating from 50-epoch RealClassEval session. Results to follow.

## 5. Related Work

- **GPTQ** (Frantar et al., 2022): Post-training quantization using Hessian information. Uniform precision.
- **AWQ** (Lin et al., 2023): Activation-aware weight quantization. Per-channel but not per-head.
- **SparseGPT** (Frantar & Alistarh, 2023): Unstructured sparsity via optimal brain surgeon. Does not produce smaller models.
- **LLM-Pruner** (Ma et al., 2023): Structured pruning based on gradient information. Similar motivation but uses a separate profiling pass rather than piggybacking on LoRA training.
- **Wanda** (Sun et al., 2023): Pruning by weights and activations. Unstructured.

Our contribution: utilization-aware compaction that (a) requires no separate profiling pass, (b) produces genuinely smaller models via physical head removal, (c) targets specific device memory budgets with mixed quantization, and (d) integrates naturally into the LoRA fine-tuning workflow.

## 6. Conclusion

By instrumenting LoRA training with a gradient callback, we obtain a task-specific utilization map at zero additional cost. This map drives both structured pruning (physical head removal) and precision allocation (mixed quantization), producing device-targeted model artifacts from a single training run. The approach is particularly suited to the Continuum ecosystem where models are continuously fine-tuned for specific roles — each training session refines both the LoRA adapter AND the utilization map for future compaction.

## Acknowledgments

Built on the Continuum collaborative AI training system. Gate gradient capture integrated into the Academy training pipeline. Compaction engine implemented in Rust using the `safetensors` and `candle` crates.

## References

_[To be populated with full citations]_
