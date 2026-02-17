# Apple MLX LoRA/PEFT Research for Apple Silicon (M1/M2/M3/M4)

**Research Date**: 2026-02-16
**Target Hardware**: Apple Silicon M1 (16GB unified memory)
**Framework**: Apple MLX (ml-explore/mlx, ml-explore/mlx-lm, ml-explore/mlx-examples)

---

## Executive Summary

Apple MLX is a production-ready framework for LoRA fine-tuning on Apple Silicon with **excellent support** for:
- ✅ LoRA and QLoRA (quantized base + FP16 adapters)
- ✅ 4-bit and 8-bit quantization (MXFP4, MXFP8, NVFP4)
- ✅ Multiple model architectures (Llama, Mistral, Qwen, Phi, Gemma, etc.)
- ✅ Adapter merging/fusing into standalone models
- ✅ Runtime adapter loading for inference
- ⚠️ **LIMITED**: Multi-adapter stacking (not well-documented)
- ⚠️ **PYTHON-FIRST**: Rust bindings exist but immature, TypeScript integration requires subprocess

**Performance on M1 16GB**: 7B models at 30-60 tok/s with 4-bit quantization, ~7GB memory usage for QLoRA training.

---

## 1. LoRA Fine-Tuning Support

### ✅ Full LoRA Support

MLX has **native, first-class LoRA support** via `mlx_lm.lora`:

```bash
# Basic LoRA training
mlx_lm.lora --model mistralai/Mistral-7B-v0.1 \
  --train --iters 600 \
  --data /path/to/data
```

**Key Features**:
- Parameter-efficient fine-tuning (only adapters trained, base model frozen)
- Configurable rank (`--lora-layers`, default 16)
- Gradient checkpointing and accumulation for memory efficiency
- Resume from checkpoint via `--resume-adapter-file`

**Training Parameters**:
| Parameter | Purpose | Default |
|-----------|---------|---------|
| `--iters` | Training iterations | 600 |
| `--batch-size` | Examples per batch | 4 |
| `--lora-layers` | Number of layers to fine-tune | 16 |
| `--adapter-file` | Output adapter weights | `adapters.npz` |

### 📊 Training Performance

**M1 Max (32GB)**: ~250 tokens/second during training
**M2 Ultra**: ~475 tokens/second (Llama 7B on WikiSQL)
**M1 (16GB)**: 30-60 tokens/second (Qwen2 1.5B)

**Validation loss example** (Llama 7B, WikiSQL):
- Initial: 2.66
- After 1000 iterations: 1.23

---

## 2. QLoRA (Quantized LoRA) Support

### ✅ Automatic QLoRA

MLX **automatically detects quantization** and switches to QLoRA:

> "If `--model` points to a quantized model, then the training will use QLoRA, otherwise it will use regular LoRA."

**Workflow**:

```bash
# Step 1: Quantize base model to 4-bit
python convert.py --hf-path mistralai/Mistral-7B-v0.1 -q --q-bits 4

# Step 2: Train with QLoRA (automatic)
mlx_lm.lora --model mistralai/Mistral-7B-v0.1-4bit \
  --train --iters 600 \
  --batch-size 1 --lora-layers 8
```

**Memory Reduction** (Mistral 7B):
- Full precision: ~28 GB
- LoRA (r=8): ~14 GB
- **QLoRA (4-bit + LoRA)**: ~7 GB (3.5x reduction)

**Quality Preservation**: 95-98% of full-precision performance retained with 4-bit quantization.

---

## 3. Quantization Formats

### Supported Formats

MLX supports multiple quantization modes:

| Format | Precision | Group Size | Use Case |
|--------|-----------|------------|----------|
| **MXFP4** | 4-bit FP (E2M1) | 32 (required) | Memory-constrained inference |
| **MXFP8** | 8-bit FP (E4M3) | 32 (required) | Balanced quality/memory |
| **NVFP4** | 4-bit NVIDIA FP | 16 (required) | NVIDIA-compatible format |
| **Affine** | Arbitrary bits | Configurable | General-purpose quantization |

**NF4 (NormalFloat4)**: While not explicitly listed as a native MLX format, QLoRA documentation mentions NF4 quantization. This appears to be implemented through MLX's integration with quantization techniques rather than as a standalone format.

### Quantization API

```python
import mlx.core as mx

# Quantize array to 4-bit
quantized = mx.quantize(array, group_size=32, bits=4)
```

**Command-line quantization**:
```bash
python convert.py --hf-path <model> -q --q-bits 4  # 4-bit
python convert.py --hf-path <model> -q --q-bits 8  # 8-bit
```

---

## 4. Multiple LoRA Adapter Loading

### ✅ Runtime Adapter Loading (Single Adapter)

MLX supports **loading adapters at inference time**:

**Generation with adapter**:
```bash
mlx_lm.generate --model mistralai/Mistral-7B-v0.1 \
  --adapter-path ./adapters/ \
  --prompt "Your prompt here" \
  --max-tokens 100
```

**Python API**:
```python
from mlx_lm import load, generate

model, tokenizer = load("mlx-community/Mistral-7B-Instruct-v0.3-4bit",
                        adapter_path="./adapters")
text = generate(model, tokenizer, prompt="Hello", verbose=True)
```

**Server mode** (Chat completion endpoint):
```python
# Adapter specified per-request in JSON payload
{
  "model": "mistralai/Mistral-7B-v0.1",
  "messages": [...],
  "adapter": "./path/to/adapters"  # Runtime adapter selection
}
```

### ⚠️ Multi-Adapter Stacking (Limited)

**Current Status**: MLX does **not have well-documented support** for stacking multiple LoRA adapters simultaneously (e.g., "load typescript-expertise + debugging-skills + code-review adapters at once").

**Workaround**: Use adapter **merging** (see Section 5) to combine multiple adapters into a single fused model.

**Alternative Frameworks** for multi-adapter serving:
- **vLLM**: Multiplexes multiple adapters with co-batching
- **LoRAX**: Specialized for multi-tenant LoRA serving
- **MAX**: Supports dynamic adapter switching via `--lora-paths`

**Recommendation for Continuum**: Implement **LRU paging** at the application layer:
- Keep one adapter loaded in MLX at a time
- Page in/out adapters based on task domain
- Use `mlx_lm.generate` with `--adapter-path` to switch adapters between requests

---

## 5. Adapter Merging/Fusing

### ✅ Fuse Adapters into Base Model

MLX provides `mlx_lm.fuse` to **merge LoRA weights into the base model**, creating a standalone fine-tuned model:

```bash
mlx_lm.fuse --model mistralai/Mistral-7B-v0.1 \
  --adapter-path ./adapters/ \
  --save-path ./fused_model/
```

**With Hugging Face upload**:
```bash
mlx_lm.fuse --model mistralai/Mistral-7B-v0.1 \
  --adapter-path ./adapters/ \
  --upload-repo my-username/my-model \
  --hf-path mistralai/Mistral-7B-v0.1
```

**Export to GGUF** (for llama.cpp compatibility):
```bash
mlx_lm.fuse --model mistralai/Mistral-7B-v0.1 \
  --adapter-path ./adapters/ \
  --export-gguf ./model.gguf
```

**Limitations**: GGUF export only supports Mistral, Mixtral, and Llama models in FP16 precision.

**Use Cases**:
- Create domain-specific models (e.g., "TypeScript expert" model)
- Share models without distributing adapters separately
- Simplify inference (no adapter loading overhead)

---

## 6. M1 16GB Performance & Memory Requirements

### Model Size Guidelines

**8GB M1 Mac**: 3B-7B models
**16GB M1 Mac**: 7B-8B models comfortably (~12-13GB available)
**24GB+ Mac**: 14B-32B models
**32GB+ Mac**: Up to 70B models with quantization

### Specific Memory Requirements

**Full Precision** (FP16):
- 7B model: ~14 GB (2 bytes/param)
- 13B model: ~26 GB
- 70B model: ~140 GB (requires Mac Studio/Pro)

**4-bit Quantization**:
- 7B model: ~4 GB base + ~1 GB overhead = **~5 GB**
- 13B model: ~8 GB
- 70B model: ~35 GB

**QLoRA Training** (4-bit base + FP16 adapters):
- 7B model: **~7 GB** (includes gradients and optimizer states)
- 13B model: ~10 GB
- 32B model: ~16 GB

**Context Window Memory** (KV cache):
- 8K tokens: ~1.5-2 GB
- 32K tokens: ~6-8 GB
- Flash Attention reduces KV cache significantly

### Inference Performance (M1 16GB)

**Qwen2 1.5B (4-bit)**: 30-60 tok/s
**Llama 7B (4-bit)**: ~30-40 tok/s
**Mistral 7B (4-bit)**: ~35-45 tok/s

**Memory-constrained optimizations**:
- Rotating KV cache: `--max-kv-size 512` (lower quality, less RAM)
- Prompt caching: Reuse KV cache for repeated prefixes
- Batch size 1: Minimum memory for training

### Maximum Context Window on M1 16GB

**Practical limits** (with 7B model at 4-bit):
- **8K context**: Comfortably fits (~11 GB total)
- **16K context**: Tight but possible (~14 GB total)
- **32K context**: Not recommended (exceeds 16GB with overhead)

**Recommendation**: Use **8K-16K context** for M1 16GB with LoRA loaded.

---

## 7. Supported Model Architectures

### ✅ Production-Ready Architectures

MLX supports **thousands of models** from Hugging Face, including:

**Decoder-Only Transformers**:
- **Llama**: Llama 2, Llama 3, Llama 3.1, Llama 4
- **Mistral**: Mistral 7B, Mixtral MoE
- **Qwen**: Qwen 2, Qwen 2.5, Qwen 3, Qwen 3 MoE, Qwen 3 Next
- **Phi**: Phi 2, Phi 3
- **Gemma**: Gemma 1, Gemma 2, Gemma 3
- **DeepSeek**: DeepSeek V3
- **OLMo**: Allen AI's open LLM
- **MiniCPM**: Efficient Chinese models
- **InternLM2**: Shanghai AI Lab models

**Model Sources**:
- Hugging Face Hub (direct integration, no manual conversion required)
- MLX Community org: Pre-quantized models (4-bit, 8-bit)

**Architecture Support**: "Llama and Mistral style models" are explicitly mentioned, suggesting decoder-only transformers with similar attention mechanisms.

---

## 8. mlx-lm Workflow

### Complete Training Pipeline

**1. Install MLX**:
```bash
pip install mlx-lm
# OR
conda install -c conda-forge mlx-lm
```

**2. Prepare Dataset**:
Create `train.jsonl`, `valid.jsonl`, `test.jsonl` with format:
```json
{"text": "Your training example here"}
{"text": "Another example"}
```

**3. (Optional) Quantize Model**:
```bash
python convert.py --hf-path mistralai/Mistral-7B-v0.1 -q --q-bits 4
```

**4. Fine-Tune with LoRA**:
```bash
mlx_lm.lora --model mistralai/Mistral-7B-v0.1 \
  --train \
  --data ./data \
  --iters 600 \
  --batch-size 4 \
  --lora-layers 16 \
  --learning-rate 1e-5
```

**5. Evaluate**:
```bash
mlx_lm.lora --model mistralai/Mistral-7B-v0.1 \
  --adapter-file ./adapters.npz \
  --test
```

**6. Generate**:
```bash
mlx_lm.generate --model mistralai/Mistral-7B-v0.1 \
  --adapter-path ./adapters/ \
  --prompt "Explain recursion" \
  --max-tokens 200
```

**7. (Optional) Fuse Adapters**:
```bash
mlx_lm.fuse --model mistralai/Mistral-7B-v0.1 \
  --adapter-path ./adapters/ \
  --save-path ./fused_model/
```

### Memory Optimization Techniques

**If OOM (Out of Memory)**:
1. **Lower batch size**: `--batch-size 1` or `--batch-size 2`
2. **Reduce LoRA layers**: `--lora-layers 8` or `--lora-layers 4`
3. **Gradient accumulation**: `--grad-accumulation-steps 4` (effective batch size = batch_size × steps)
4. **Gradient checkpointing**: `--grad-checkpoint` (trades compute for memory)
5. **Quantization**: Use 4-bit QLoRA
6. **Shorter sequences**: Break long examples into <2K token chunks

**Example for M1 16GB**:
```bash
mlx_lm.lora --model mistralai/Mistral-7B-v0.1-4bit \
  --train \
  --batch-size 1 \
  --lora-layers 4 \
  --grad-accumulation-steps 8 \
  --grad-checkpoint
```

---

## 9. Language Integration (Rust/TypeScript)

### ⚠️ Python-First Framework

MLX is **primarily a Python framework** with some C++/Swift bindings. Rust and TypeScript integration requires workarounds.

### Rust Integration: mlx-rs (Early Stage)

**Repository**: [oxideai/mlx-rs](https://github.com/oxideai/mlx-rs)

**Status**: "In active development and can be used to run MLX models in Rust."

**Available APIs**:
- Array operations (lazy evaluation)
- Automatic differentiation (`transforms::grad()`)
- Device management (CPU/GPU via Metal)
- Neural network operations (matmul, activations)
- Model loading via `mlx-lm` subcrate

**Example** (Mistral inference):
```rust
use mlx_rs::prelude::*;

let model = Model::load("mlx-community/Mistral-7B-4bit")?;
let tokens = tokenizer.encode("Hello, world")?;
let output = model.generate(&tokens, 100)?;
```

**Limitations vs. Python MLX**:
- ⚠️ **Explicit parameter passing**: Rust closures require explicit capture (Python does this implicitly)
- ⚠️ **Segfault risk**: Implicit capture can cause segfaults
- ⚠️ **Documentation**: Hosted on GitHub Pages (not docs.rs due to platform limits)
- ⚠️ **Maturity**: FFI via `mlx-sys` bindings to `mlx-c`, less battle-tested than Python

**MSRV**: Rust 1.83.0+

**Recommendation for Continuum**:
- **Short-term**: Use Python subprocess (call `mlx_lm.generate` from Rust via `std::process::Command`)
- **Long-term**: Evaluate mlx-rs maturity in 6-12 months

### TypeScript Integration: Subprocess Only

**No native TypeScript bindings exist** for MLX.

**Workaround**: Call `mlx_lm` CLI from Node.js:

```typescript
import { spawn } from 'child_process';

async function generateWithMLX(
  model: string,
  prompt: string,
  adapterPath?: string
): Promise<string> {
  const args = [
    '-m', 'mlx_lm.generate',
    '--model', model,
    '--prompt', prompt,
    '--max-tokens', '200',
  ];

  if (adapterPath) {
    args.push('--adapter-path', adapterPath);
  }

  const python = spawn('python3', args);

  return new Promise((resolve, reject) => {
    let output = '';
    python.stdout.on('data', (data) => output += data);
    python.stderr.on('data', (data) => console.error(data.toString()));
    python.on('close', (code) => {
      if (code === 0) resolve(output);
      else reject(new Error(`MLX exited with code ${code}`));
    });
  });
}
```

**Alternative**: Run MLX as a **microservice** (FastAPI server) and call via HTTP:

```python
# mlx_server.py
from fastapi import FastAPI
from mlx_lm import load, generate

app = FastAPI()
model, tokenizer = load("mlx-community/Mistral-7B-4bit")

@app.post("/generate")
async def generate_text(prompt: str, adapter: str = None):
    # Load adapter if provided
    if adapter:
        model.load_adapter(adapter)

    text = generate(model, tokenizer, prompt=prompt, max_tokens=200)
    return {"text": text}
```

**Recommendation for Continuum**:
- **Use subprocess** for simplicity (low latency if model kept in memory)
- **Use FastAPI microservice** for production (better resource management)

---

## 10. Context Window Limits

### Theoretical Limits

MLX models inherit context window limits from their base architectures:

| Model | Default Context | Extended Context |
|-------|----------------|------------------|
| Llama 2 | 4K | — |
| Llama 3 | 8K | 32K (with RoPE scaling) |
| Mistral 7B | 8K | 32K (v0.2+) |
| Qwen 2.5 | 32K | 128K |
| Phi 3 | 4K | 128K (long context variant) |

### Practical Limits on M1 16GB

**With 7B model at 4-bit quantization**:

| Context Size | KV Cache Memory | Total Memory | Feasibility |
|--------------|----------------|--------------|-------------|
| 2K | ~0.5 GB | ~5.5 GB | ✅ Comfortable |
| 4K | ~1 GB | ~6 GB | ✅ Comfortable |
| 8K | ~2 GB | ~7 GB | ✅ Recommended |
| 16K | ~4 GB | ~9 GB | ⚠️ Tight |
| 32K | ~8 GB | ~13 GB | ❌ Not recommended |

**With LoRA adapter loaded**: Add ~0.5-1 GB for adapter weights.

**Flash Attention** (if available): Reduces KV cache by ~30-40%.

**Rotating KV Cache**: Allows longer contexts at quality cost:
```bash
mlx_lm.generate --max-kv-size 512 --prompt "..."
```

**Recommendation**: Use **8K context** as sweet spot for M1 16GB.

---

## 11. Adapter Stacking Support

### ❌ No Native Multi-Adapter Stacking

MLX does **not support** loading multiple LoRA adapters simultaneously (e.g., "adapter1 + adapter2 + adapter3").

**Evidence**:
- No documentation mentions combining adapters
- `--adapter-path` accepts single directory
- GitHub issues discuss adapter switching, not stacking

### Workarounds

**1. Sequential Training (Adapter Stacking via Fine-Tuning)**:
```bash
# Train adapter 1
mlx_lm.lora --model base-model --train --adapter-file adapter1.npz

# Fuse adapter 1
mlx_lm.fuse --adapter-path adapter1.npz --save-path model-with-adapter1

# Train adapter 2 on top
mlx_lm.lora --model model-with-adapter1 --train --adapter-file adapter2.npz
```

**2. Merge Adapters Before Loading**:
```python
# Hypothetical - would require custom code
adapter1 = np.load("adapter1.npz")
adapter2 = np.load("adapter2.npz")

# Average or weighted sum of adapter weights
merged = {
    key: 0.5 * adapter1[key] + 0.5 * adapter2[key]
    for key in adapter1.keys()
}
np.savez("merged_adapter.npz", **merged)
```

**3. Application-Layer Paging** (Recommended for Continuum):
```typescript
class LoRAGenomePager {
  private currentAdapter: string | null = null;
  private lruCache: Map<string, Date> = new Map();

  async activateSkill(domain: string): Promise<void> {
    const adapterPath = this.getAdapterPath(domain);

    if (this.currentAdapter !== adapterPath) {
      // Page in new adapter (MLX loads it at next generate call)
      this.currentAdapter = adapterPath;
      this.lruCache.set(domain, new Date());
    }
  }

  async generate(prompt: string): Promise<string> {
    return await generateWithMLX(
      "mistralai/Mistral-7B-v0.1-4bit",
      prompt,
      this.currentAdapter
    );
  }

  async evictLRU(): Promise<void> {
    // Remove least-recently-used adapter from disk if space needed
    const entries = Array.from(this.lruCache.entries());
    const sorted = entries.sort((a, b) => a[1].getTime() - b[1].getTime());
    const toEvict = sorted[0][0];

    // Delete adapter files
    await fs.rm(this.getAdapterPath(toEvict), { recursive: true });
    this.lruCache.delete(toEvict);
  }
}
```

**Alternative Frameworks with Multi-Adapter Support**:
- **vLLM-MLX**: Research paper mentions multi-adapter batching
- **Hugging Face PEFT**: Supports adapter composition (but not on MLX backend)

---

## 12. Production Recommendations for Continuum

### Architecture Strategy

**Use MLX for**:
- ✅ Local fine-tuning on Mac hardware
- ✅ Fast inference with quantized models
- ✅ Unified memory efficiency (CPU/GPU shared)
- ✅ Production-ready Python API

**Integrate via**:
- 🔧 **Python subprocess** (simple, low latency if model cached)
- 🔧 **FastAPI microservice** (better resource isolation)
- ⚠️ **mlx-rs** (evaluate in 6-12 months when more mature)

### LoRA Genome Paging Design

**Phase 1: Single Adapter Paging**
```typescript
// PersonaGenome activates ONE adapter at a time
await this.genome.activateSkill('typescript-expertise');
await this.genome.generate(codeTask);

await this.genome.activateSkill('spanish-translation');
await this.genome.generate(translationTask);
```

**Phase 2: LRU Eviction**
```typescript
// Evict least-used adapters when disk space >80%
if (this.genome.diskUsage > 0.8) {
  await this.genome.evictLRU();
}
```

**Phase 3: Continuous Learning**
```typescript
// Fine-tuning is just another task
{
  taskType: 'fine-tune-lora',
  targetSkill: 'typescript-expertise',
  trainingData: recentMistakes
}
```

### Memory Budget for M1 16GB

**Allocation**:
- Base model (4-bit): ~5 GB
- Active adapter: ~0.5 GB
- KV cache (8K context): ~2 GB
- Browser/OS: ~4 GB
- **Available for other services**: ~4.5 GB

**Recommendation**: Run MLX in dedicated process, limit to 8K context, use rotating KV cache if needed.

### Performance Targets

**M1 16GB with Mistral 7B (4-bit + LoRA)**:
- Inference: 35-45 tok/s
- Fine-tuning: 30-60 tok/s (batch size 1)
- Adapter switching: <1s (reload overhead)
- Memory footprint: ~8 GB

---

## Sources

### Official Documentation
- [MLX GitHub Repository](https://github.com/ml-explore/mlx)
- [MLX-LM GitHub Repository](https://github.com/ml-explore/mlx-lm)
- [MLX-Examples LoRA Documentation](https://github.com/ml-explore/mlx-examples/blob/main/lora/README.md)
- [MLX-LM LoRA Guide](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/LORA.md)
- [MLX Quantization API](https://ml-explore.github.io/mlx/build/html/python/_autosummary/mlx.core.quantize.html)
- [MLX-RS Rust Bindings](https://github.com/oxideai/mlx-rs)

### Tutorials & Guides
- [Train Your Own LLM on MacBook with MLX](https://medium.com/@dummahajan/train-your-own-llm-on-macbook-a-15-minute-guide-with-mlx-6c6ed9ad036a)
- [LoRA Fine-Tuning On Your Apple Silicon MacBook](https://towardsdatascience.com/lora-fine-tuning-on-your-apple-silicon-macbook-432c7dab614a/)
- [Fine-Tuning LLMs with LoRA and MLX-LM](https://medium.com/@levchevajoana/fine-tuning-llms-with-lora-and-mlx-lm-c0b143642deb)
- [Fine-Tuning LLMs Locally Using MLX LM](https://dzone.com/articles/fine-tuning-llms-locally-using-mlx-lm-guide)
- [Fine-tune LLMs on Laptop With QLoRA & MLX](https://medium.com/rahasak/fine-tune-llms-on-your-pc-with-qlora-apple-mlx-c2aedf1f607d)

### Performance & Benchmarks
- [Running Large Language Models on Apple Silicon with MLX](https://medium.com/@manuelescobar-dev/running-large-language-models-llama-3-on-apple-silicon-with-apples-mlx-framework-4f4ee6e15f31)
- [Local LLM Speed: Qwen2 & Llama 3.1 Real Benchmark Results](https://singhajit.com/llm-inference-speed-comparison/)
- [Best Local LLMs for Mac in 2026](https://www.insiderllm.com/guides/best-local-llms-mac-2026/)
- [Running LLMs on Mac M-Series: Complete Guide](https://insiderllm.com/guides/running-llms-mac-m-series/)
- [Apple Shows M5 Performance vs M4](https://9to5mac.com/2025/11/20/apple-shows-how-much-faster-the-m5-runs-local-llms-compared-to-the-m4/)
- [Exploring LLMs with MLX and M5 GPU](https://machinelearning.apple.com/research/exploring-llms-mlx-m5)

### Multi-LoRA & Advanced Topics
- [Efficiently Deploying LoRA Adapters](https://www.inferless.com/learn/how-to-serve-multi-lora-adapters)
- [Serving Heterogeneous LoRA Adapters](https://arxiv.org/html/2511.22880v1)
- [vLLM LoRA Support](https://docs.vllm.ai/en/latest/features/lora/)

### Apple Resources
- [WWDC 2025: Explore LLMs with MLX](https://developer.apple.com/videos/play/wwdc2025/298/)
- [Apple Open Source MLX](https://opensource.apple.com/projects/mlx/)

---

## Conclusion

Apple MLX is a **mature, production-ready framework** for LoRA fine-tuning on Apple Silicon. It excels at:
- Memory-efficient training (QLoRA)
- Fast inference (20-50% faster than llama.cpp)
- Unified memory architecture (CPU/GPU shared)
- Easy model conversion from Hugging Face

**Limitations**:
- No native multi-adapter stacking (workaround: application-layer paging)
- Python-first (Rust/TypeScript require subprocess or microservice)
- Context window limited by unified memory (8K sweet spot for M1 16GB)

**For Continuum's PersonaUser LoRA Genome**:
- Use **single-adapter paging** (Phase 6)
- Implement **LRU eviction** at application layer (Phase 6)
- Call MLX via **Python subprocess** or **FastAPI microservice** (Phase 6)
- Fine-tune on M1 16GB with **7B models at 4-bit** (Phase 7)
- Target **8K context** for optimal memory usage (Phase 6)

**Next Steps**:
1. Install MLX: `pip install mlx-lm`
2. Download test model: `mlx-community/Mistral-7B-4bit`
3. Fine-tune small adapter on synthetic data
4. Benchmark inference speed and memory on M1 16GB
5. Prototype subprocess integration in Continuum
6. Design adapter paging strategy for PersonaGenome
