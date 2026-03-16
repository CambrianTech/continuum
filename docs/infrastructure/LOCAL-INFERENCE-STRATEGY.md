# Local Inference Strategy: Independence Through Engineering

> **"We don't cripple. We figure it out."**

---

## The Stack (All Ours)

```
Layer 4: GRID — GPU contributors, routing, altcoin compensation
Layer 3: CONTINUUM — personas, sentinels, LoRA training flywheel
Layer 2: SENTINEL-AI — self-pruning/growing transformers (adapts to hardware)
Layer 1: CANDLE — Rust-native inference runtime, in-process
```

Every layer is controlled by the project. No OpenAI API dependency. No Ollama. No vLLM. No HuggingFace Inference Endpoints. The entire path from model architecture to distributed compute is ours.

---

## Current Architecture

### Dual Adapter Registration

Two Candle backends registered in the global adapter registry:

| Adapter ID | Backend | Format | LoRA | Use Case |
|-----------|---------|--------|------|----------|
| `candle-q` | GGUF quantized | `.gguf` | No | Large models, long context |
| `candle` | Safetensors BF16 | `.safetensors` | Yes | Persona-specific, fine-tuned |

### ModelSpec Routing

```
local/default         → candle-q (GGUF, best available model)
local/{persona}       → candle (safetensors + LoRA adapters)
explicit model name   → candle-q (GGUF, specific model)
```

### HTTP Endpoint

Anthropic-compat HTTP at `POST /v1/messages`. Clean pass-through proxy:
- No truncation — model's own definition is the limit
- SSE streaming with proper event framing
- `adapter.capabilities().max_context_window` queries actual loaded backend
- Currently bound to `127.0.0.1` (local only)

---

## Quantization Strategy

### The Rules

1. **Q4_K_M is the floor** for coding quality
2. **Q5_K_M is the sweet spot** (minimal quality loss)
3. **Model choice > quantization level** — a 32B Q4 destroys a 3B BF16 for coding
4. **NEVER artificially cap context** — the model defines its own limits
5. **Below Q4, coding accuracy drops off a cliff** — don't go there
6. **Above Q6, diminishing returns** vs the VRAM cost

### Hardware → Model Matrix

| Hardware | VRAM | Best Model | Quantization | Why |
|----------|------|-----------|-------------|-----|
| MacBook Air M1 (8GB) | ~5GB usable | 7B (sentinel-pruned to ~4B) | Q5_K_M | Shared VRAM, tight budget |
| MacBook Pro M1 (16GB) | ~12GB usable | 8B or 14B | Q5_K_M / BF16 | Room for model + LoRA |
| MacBook M4 Max (64GB) | ~48GB usable | 34B | Q5_K_M or BF16+LoRA | Enough for real models |
| RTX 5090 (32GB) | 32GB dedicated | 70B or 34B | Q4_K_M / Q6_K | Dedicated VRAM, fast |
| 5090 + CPU offload | 32GB + system | 70B | Q5_K_M | GPU+CPU split |

### Innovation at Bottlenecks (Not Crippling)

| Bottleneck | Wrong (Cripple) | Right (Innovate) |
|-----------|----------------|------------------|
| Context too long | Truncate to 4K | Paged attention, KV cache quantization |
| Model too big | Use tiny model | Sentinel pruning to fit, quality preserved |
| Prefill too slow | Limit input tokens | Fix chunked prefill, speculative decode |
| VRAM exhausted | Refuse request | Route to grid node with capacity |
| Quality too low | Accept mediocrity | Train LoRA on coding data, grow heads |

---

## Immediate Priorities

### 1. Fix GGUF Chunked Prefill (Critical)

**The problem**: Vendored `quantized_llama.rs` forward() assumes `seq_len=1` when KV cache has content. The attention mask shape is wrong for batched input. Result: token-by-token prefill. 131K tokens = 60+ minutes.

**The fix**: Mask calculation, not a fundamental limitation. Proper chunked prefill (256-512 token chunks) turns 60 minutes into 2-3 minutes.

**Location**: `src/workers/continuum-core/` vendored Candle quantized llama implementation.

### 2. Remote Inference (MacBook → 5090)

**The problem**: HTTP server bound to `127.0.0.1`. Can't reach it from another machine.

**The fix**:
- Bind to `0.0.0.0` (configurable)
- Add token-based auth (header check)
- On MacBook: `ANTHROPIC_BASE_URL=http://5090-ip:PORT` — Claude Code runs locally, inference runs on 5090

**This is the architecture for distributed coding**. Same Anthropic-compat API, different compute.

### 3. Support Better Models

Currently running Llama-3.2-3B — too small for real coding. Target models:
- **Qwen2.5-Coder-32B** — current SOTA open coding model
- **DeepSeek-Coder-V2** — strong alternative
- **CodeLlama-34B** — proven coding capability

The GGUF loader already reads model metadata for architecture/context length. Adding architectures = adding attention pattern implementations to the backend.

---

## Sentinel-AI Integration Path

> **Status**: Research prototype with real mathematical substance. Unvalidated end-to-end.
> **Stance**: Document the path. Do NOT depend on it until proven. Everything above works regardless.

### What Sentinel-AI Does

Biologically-inspired transformer with per-head sentinel gates (continuous 0-1):
- Gates near 0 → head contributes minimally (prune for speed)
- Gates near 1 → head fully active (keep for quality)
- ANN controller adjusts gates based on target compression ratio
- Dynamic pruning (cull) and growing (expand) during training

### Claimed Results (Unverified)

- 30-70% parameter reduction with 98% quality preservation
- Perplexity: 975→211 after 500 adaptive steps
- Cross-architecture: GPT-2, BLOOM, Llama, OPT, Pythia
- 2x inference speedup at 50% pruning

**No saved checkpoints, metrics CSVs, or experiment outputs exist in the repo.** The ideas are sound. The experiments need to be run.

### Integration Path (When Proven)

1. **Port sentinel gates to Candle** — `SentinelTransformerBackend` implementing `ModelBackend` trait
2. **Train sentinel-gated coding model** — Qwen2.5-Coder-7B base + sentinel gates + LoRA
3. **Export compression profiles**:
   - `sentinel-coder-7b-full.safetensors` (7B, for 5090)
   - `sentinel-coder-7b-pruned50.gguf` (3.5B equiv, for MacBook Air)
   - `sentinel-coder-7b-pruned30.gguf` (5B equiv, for MacBook Pro)
4. **Candle loads appropriate profile** based on `adapter.capabilities()` VRAM query
5. **LoRA stacks on top** of pruned model (persona-specific coding style)

### Validation Plan

Run on the 5090 with real hardware:
1. `pip install -r requirements.txt` at `/Users/joel/Development/cambrian/sentinel-ai`
2. Fix `EntropyPruningStrategy` (wire in real entropy code, not magnitude wrapper)
3. Load Qwen2.5-Coder-7B, add sentinel gates
4. Run adaptive training loop on coding data
5. Measure: perplexity, pass@1 on HumanEval, before/after pruning
6. Save actual numbers

If quality preserves at even 80% (not 98%), that's still a game-changer for local inference.

---

## Future Envelope-Pushing

Once the basics work (chunked prefill, remote inference, better models):

- **Speculative decoding** — 3B drafts, 34B verifies → 3-4x speedup
- **KV cache quantization** — separate from weight quant, saves VRAM for longer context
- **Paged attention** — vLLM-style dynamic memory, no pre-allocation
- **LoRA composition** — train coding-specific adapters per persona, stack them
- **Sentinel gate dynamic adjustment** — gates open/close at inference time based on available compute

---

## Grid Integration

The existing Grid architecture (`docs/grid/GRID-ARCHITECTURE.md`) extends naturally:

- **Wire protocol**: Anthropic-compat HTTP (already built)
- **Node capabilities**: VRAM, model, compression profile, utilization, throughput, latency
- **Routing**: Match requests to best available node
- **Compensation**: Proof-of-inference → altcoin (contributors earn for inference served)

The adapter registry pattern (`candle-q`, `candle`, cloud providers) extends to remote Candle nodes. A "grid adapter" is just another adapter that forwards requests over HTTP to the best available node.

```
MacBook Air → local 3.5B for quick tasks
            → remote 5090 34B for heavy coding
            → grid 3090 contributor for overflow
```

The key insight: **the model adapts to hardware** (sentinel-ai). The **routing adapts to availability** (grid). The **persona adapts to task** (LoRA). Three levels of adaptation, zero vendor lock-in.
