# Adaptive Neural Compression Pipeline

## What This Is

A system that takes any large language model, learns which parts of its attention mechanism matter for a specific task, removes the parts that don't, and quantizes the rest at variable precision — producing a model that runs on consumer hardware where it normally wouldn't fit.

**Proven**: Qwen2.5-Coder-32B-Instruct (64 layers, 40 Q-heads, 8 KV-heads) compressed to 25 Q-heads / 5 KV-heads with Q3_K_S quantization. Runs locally on M1 Pro 32GB at 5.3 tok/s generating correct Python code. A 32-billion parameter coding model on a laptop.

## Why It Matters

Large models are better than small models. But they don't fit on consumer devices. The current options are:

1. **Use a smaller model** — sacrifice intelligence
2. **Use an API** — sacrifice privacy, cost money, require internet
3. **Uniform quantization** — squeeze the whole model equally, lose quality everywhere

This pipeline adds a fourth option:

4. **Adaptive compression** — learn what matters, keep precision where it counts, remove what doesn't contribute. Same memory budget, better model. Or same quality, smaller model.

The key insight: **attention heads specialize**. In a 32B model with 40 query heads grouped into 8 KV groups, not all groups contribute equally to every task. For coding, some groups handle syntax, others handle variable tracking, others handle natural language reasoning. By fine-tuning briefly on coding data and capturing gradient flow through each head, we learn which groups are essential and which are expendable.

This isn't theoretical. We proved it works:
- Pruned 3 of 8 KV groups (37.5% of the KV cache)
- Quantized to 3.5 bits per weight
- Model still generates correct `is_prime`, `binary_search`, `fibonacci`, `flatten_list`, `reverse_string`
- Runs locally on a $2000 laptop with 32GB RAM

## The Triangle of Tradeoffs

Every compression decision trades between three dimensions:

```
            QUALITY
              /\
             /  \
            /    \
           / sweet \
          /  spot   \
         /____________\
     SIZE ----------- SPEED
```

- **Quality**: Does the model still produce correct, coherent output?
- **Size**: Does it fit in the target device's memory?
- **Speed**: How many tokens per second?

Uniform quantization moves you along one axis. Adaptive compression lets you move diagonally — same size but better quality, because precision is concentrated where utilization is high.

### Measured Results (Qwen2.5-Coder-32B, M1 Pro 32GB)

| Quant | Size | Fits? | Speed | Quality | Status |
|-------|------|-------|-------|---------|--------|
| BF16 (original) | 62 GB | No | — | Reference | Too large |
| Q4_K_M | 18.8 GB | No (OOM) | — | Excellent (A100) | Proven coherent on server |
| Q3_K_S | 12.9 GB | Yes | 5.3 tok/s | Good (some repetition) | **Working locally** |
| Q2_K | 11.0 GB | Yes | — | NaN | Too aggressive |

The gap between Q3_K_S and Q4_K_M is exactly where adaptive mixed quantization adds value: give important layers Q5_K precision and unimportant layers Q3_K, achieving Q4_K_M-level quality at Q3_K_S-level size.

## Pipeline Architecture

Five stages, all Rust except the LoRA training step (Python on remote GPU). Each stage is a pure function with defined inputs and outputs.

```
┌──────────┐    ┌──────────┐    ┌───────────┐    ┌──────────┐    ┌──────────┐
│  SCORE   │───▶│   PLAN   │───▶│ COMPRESS  │───▶│  VERIFY  │───▶│  INFER   │
│ (remote) │    │ (Rust)   │    │  (Rust)   │    │  (Rust)  │    │ (Metal)  │
└──────────┘    └──────────┘    └───────────┘    └──────────┘    └──────────┘
 LoRA train      Utilization     Prune heads      Check NaN      Generate
 + gradient      → recipe:       + mixed quant    + budget       tokens
 capture         per-tensor      → GGUF file      + coherence    locally
                 quant levels
```

### Stage 1: Score

**Where**: Remote GPU (RunPod, 5090, reticulum grid)
**What**: LoRA fine-tuning on the target dataset with gradient capture
**Output**: `gate_gradients.json` — per-head utilization scores

The model is briefly fine-tuned (1-3 epochs) on task-specific data. During training, a callback captures gradient magnitudes flowing through each attention head's gate projection. Heads with consistently high gradients are critical for the task; heads with near-zero gradients are expendable.

This already works via `peft-train.py` with `GateGradientCallback`. The Academy pipeline (TeacherPipeline) runs this as part of its training loop.

**Future**: Sentinel orchestrates this step, farming it to available GPU nodes. The Reticulum can distribute scoring across multiple machines, each training on different dataset slices and merging utilization maps.

### Stage 2: Plan

**Where**: Local (Rust, no GPU needed)
**What**: Turn utilization scores + device spec into a `CompressionRecipe`
**Output**: `CompressionRecipe` — complete specification of what to prune and how to quantize

The planner takes three inputs:
1. Utilization scores from Stage 1
2. A device specification (e.g., "MacBook Air 16GB", "MacBook Pro 32GB", "RTX 5090 24GB")
3. The base model's architecture config

And produces a recipe specifying:
- Which KV groups to prune (utilization below threshold)
- Per-tensor quantization type (high-util layers → Q5_K/Q6_K, low-util → Q3_K)
- Dimension padding for block alignment
- Memory budget breakdown

```rust
pub fn plan_compression(
    utilization: &UtilizationData,
    device: &DeviceSpec,
    arch: &ModelArchConfig,
) -> Result<CompressionRecipe, String>
```

**Device presets**:
- `DeviceSpec::macbook_air_16gb()` — 11 GB effective budget
- `DeviceSpec::macbook_pro_32gb()` — 24 GB effective budget
- `DeviceSpec::rtx_5090_24gb()` — 22 GB effective VRAM budget
- `DeviceSpec::from_memory_gb(total)` — auto-compute reserves

**Quantization floor**: Q3_K_S is the practical minimum. Q2_K produces NaN for compacted models — the combination of head pruning and extreme quantization destroys too much precision.

### Stage 3: Compress

**Where**: Local (Rust, CPU-only)
**What**: Read safetensors, apply recipe, write GGUF
**Output**: Single GGUF file with mixed quantization + custom metadata

This is the core new capability. Our own GGUF writer that:
1. Reads each tensor from the base model's safetensors
2. For attention projections: slices out pruned heads, pads dimensions to block alignment
3. Quantizes each tensor at the recipe's assigned level using candle's `GgmlDType::from_float()`
4. Writes the GGUF file with standard + custom metadata

**Why our own writer**: llama.cpp's quantizer applies uniform quantization and rejects non-standard tensor dimensions from pruned models (e.g., `ncols=3200 not divisible by 256`). Our writer handles variable dimensions and per-tensor quant levels.

**Custom GGUF metadata** (readable by our inference engine, ignored by others):
- `continuum.compression_recipe` — JSON string of the full recipe
- `continuum.per_layer_head_counts` — array of Q head counts per layer
- `continuum.per_layer_kv_head_counts` — array of KV head counts per layer
- `continuum.utilization_scores` — array of per-group utilization scores

### Stage 4: Verify

**Where**: Local (Rust)
**What**: Validate the compressed GGUF before deployment
**Output**: Pass/fail + quality report

Checks:
- Load metadata, confirm dimensions match recipe
- Dequantize sample layers, check for NaN/Inf
- Actual file size vs budget target
- Short inference test (3-5 tokens) — does it produce coherent text?

### Stage 5: Infer

**Where**: Local (Metal/CUDA/CPU via Candle)
**What**: Run the compressed model
**Output**: Tokens

The existing `LlamaGgufBackend` in `quantized_llama.rs` already handles GGUF inference. Extended to:
- Read `continuum.per_layer_head_counts` from custom metadata
- Use per-layer head counts in attention reshape (instead of global uniform count)
- Derive `head_dim` from tensor shapes rather than metadata formulas

## The CompressionRecipe

Central data structure that drives everything:

```rust
pub struct CompressionRecipe {
    /// What heads to keep/prune, per-head precision tiers
    pub topology: HeadTopology,

    /// Per-tensor quantization: tensor name pattern → GGUF quant type
    pub tensor_quant_map: Vec<TensorQuantAssignment>,

    /// Target device that drove the budget
    pub device_spec: DeviceSpec,

    /// Memory budget breakdown
    pub budget: MemoryBudget,
}

pub struct TensorQuantAssignment {
    /// Glob pattern: "model.layers.*.self_attn.q_proj.weight"
    pub pattern: String,
    /// GGUF quant type: Q3_K_S, Q4_K_M, Q5_K_S, Q6_K, etc.
    pub quant_type: GgufQuantType,
    /// Why this assignment (for debugging/reports)
    pub reason: String,
}
```

The recipe is:
- **Serializable** (JSON) — can be stored, versioned, shared
- **Deterministic** — same inputs always produce the same recipe
- **Device-aware** — auto-fits to the target memory budget
- **Auditable** — every assignment has a reason string

## IPC Integration

```bash
# Compress a model for a specific device target
./jtag plasticity/compress \
  --capturePath=/path/to/gate_gradients.json \
  --modelPath=Qwen/Qwen2.5-Coder-32B-Instruct \
  --deviceSpec=32gb \
  --outputPath=~/.continuum/genome/models/qwen32b-coding.gguf

# Or as a Sentinel pipeline step
- type: Command
  command: plasticity/compress
  params:
    capturePath: "{{steps.train.data.outputDir}}"
    modelPath: "{{input.baseModelPath}}"
    deviceSpec: "{{input.targetDevice}}"
```

## How Mixed Quantization Helps

Uniform Q3_K_S: every tensor at 3.5 bits. Wastes precision on unimportant layers, starves important ones.

Mixed quantization with the same total size:

| Layer | Utilization | Uniform | Mixed |
|-------|------------|---------|-------|
| 0-5 (early) | Medium | Q3_K_S | Q4_K_S |
| 6-20 (mid, high-util) | High | Q3_K_S | Q5_K_M |
| 21-50 (mid, low-util) | Low | Q3_K_S | Q3_K_S |
| 51-63 (late) | High | Q3_K_S | Q5_K_S |
| embed_tokens | Critical | Q3_K_S | Q6_K |
| lm_head | Critical | Q3_K_S | Q6_K |

Same file size. Better quality. The important layers (which drive code correctness) get 5-6 bits. The unimportant layers (which contribute less) get 3 bits. The embeddings (where token identity lives) get maximum affordable precision.

## Distributed Training (Future)

The pipeline naturally separates GPU work (scoring) from CPU work (planning, compression, verification). This enables:

- **RunPod**: On-demand GPU for scoring large models
- **RTX 5090**: Local GPU for scoring smaller models or incremental re-scoring
- **Reticulum**: Distribute scoring across multiple nodes, each training on different data slices, merge utilization maps
- **Sentinel**: Orchestrate the whole pipeline — provision GPU, run training, collect scores, compress locally, deploy

The compression step itself (Stage 3) is CPU-only and runs on any machine. A MacBook Air can compress a 70B model if it has enough disk space — it processes one tensor at a time, never loading the full model into memory.

## Generalization Beyond Coding

The same pipeline works for any task:

- **Coding**: Train on code, prune heads that don't help with syntax/logic
- **Creative writing**: Train on fiction, prune heads that specialize in formal/technical language
- **Translation**: Train on bilingual data, prune heads that specialize in languages you don't need
- **Domain expertise**: Train on medical/legal/scientific text, prune generalist heads

The utilization scores are dataset-driven. Change the dataset, get a different pruning pattern, get a model optimized for a different task. Same base model, many specialized compressed variants.

This is personalized model compression. Your model, your data, your hardware, your budget.

## Implementation Status

### Done
- [x] Gradient-based utilization scoring (`scoring.rs`)
- [x] Head topology planning (`topology.rs`)
- [x] Tensor compaction / head pruning (`compactor.rs`)
- [x] Candle GGUF inference with Qwen2 support (`quantized_llama.rs`)
- [x] Architecture-aware GGUF metadata (qwen2, llama)
- [x] head_dim derivation for compacted models
- [x] KV cache clear without model reload
- [x] DeviceEmbedding (F16 on Metal)
- [x] Benchmark harness with quality/speed/memory metrics
- [x] Proof of concept: 32B on 32GB MacBook (Q3_K_S, 5.3 tok/s)

### Next
- [ ] CompressionRecipe type definitions
- [ ] Planner: utilization → per-tensor quant assignments
- [ ] GGUF writer (our own, not llama.cpp)
- [ ] Mixed quantization support
- [ ] Per-layer variable head counts in inference
- [ ] Pipeline IPC command (`plasticity/compress`)
- [ ] Verification stage
- [ ] Dimension padding for block alignment

### Future
- [ ] Sentinel pipeline integration
- [ ] Distributed scoring via Reticulum
- [ ] Custom sub-3-bit quantization kernels (Ternary/Q2 with custom Metal shaders)
- [ ] Per-head mixed quantization within a single tensor (requires custom GGUF tensor types)
- [ ] Auto-discovery of optimal compression for a given model+dataset+device triple

## File Structure

```
core/continuum-core/src/modules/plasticity/
├── mod.rs          — IPC routing, handle_command
├── types.rs        — HeadTopology, CompressionRecipe, DeviceSpec, GgufQuantType
├── scoring.rs      — Per-head gradient utilization scoring
├── topology.rs     — Head topology I/O, group selection
├── compactor.rs    — Tensor slicing (prune heads from safetensors)
├── quantizer.rs    — Block quantization primitives
├── planner.rs      — [NEW] Recipe planning from scores + device spec
├── gguf_writer.rs  — [NEW] Mixed-quant GGUF writer
├── pipeline.rs     — [NEW] End-to-end orchestration
├── validation.rs   — Integration tests, GGUF verification

core/continuum-core/src/inference/
├── vendored/quantized_llama.rs  — GGUF inference (Qwen2 + variable heads)
├── backends/llama_gguf.rs       — LlamaGgufBackend
├── backends/mod.rs              — ModelBackend trait, generate()
├── model.rs                     — Model loading utilities

docs/genome/
├── COMPRESSION-PIPELINE.md      — This document
├── plasticity_benchmark_report.json — Benchmark results
├── bench_q3ks.json              — Detailed Q3_K_S benchmark data
```
