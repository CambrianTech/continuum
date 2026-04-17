# Multi-Persona Concurrency Benchmark

**Date**: 2026-04-17
**Hardware**: NVIDIA RTX 5090 (32GB VRAM), Windows 11 + WSL2
**Model**: Qwen3.5-4B Q4_K_M (forged) — `continuum-ai/qwen3.5-4b-code-forged-GGUF`
**Runtime**: Docker Model Runner, llama.cpp CUDA backend
**Max tokens per response**: 150

## Results

| Concurrent Streams | Per-Stream (tok/s) | Aggregate (tok/s) | Wall Time (ms) | Scaling |
|---|---|---|---|---|
| 1 | 249.6 | 249.6 | 664 | 1.00x |
| 2 | ~202 | 404.4 | 819 | 1.62x |
| 3 | ~161 | 484.5 | 1,069 | 1.94x |
| 4 | ~141 | 565.7 | 1,295 | 2.27x |
| 5 | ~136 | ~680 | 1,333 | 2.73x |

## Headlines

- **Single persona**: 249.6 tok/s (150 tokens generated in 664ms)
- **4 concurrent personas**: 565.7 tok/s aggregate, each persona responds in 1.3 seconds
- **5 concurrent personas**: ~680 tok/s aggregate, graceful degradation
- **Scaling efficiency**: 57% at 4 streams (565.7 / (4 × 249.6))

## What This Means

Continuum serves **4 AI personas simultaneously** on a single consumer GPU, each generating a full response in **under 1.5 seconds**. No cloud, no API keys, no subscriptions.

The 57% scaling efficiency comes from llama.cpp's continuous batching — KV cache is shared across concurrent requests, so adding streams doesn't linearly increase memory or compute cost.

## Methodology

- Warmup: 1 request discarded before measurement (ensures model loaded to GPU)
- Each stream sends an independent technical prompt (polymorphism, TCP/UDP, B-trees, CAP theorem, concurrency vs parallelism)
- All streams launched simultaneously via background processes
- Wall time measured from request start to response complete
- Per-stream tok/s from llama.cpp's `timings.predicted_per_second` field
- Aggregate = sum of all concurrent per-stream tok/s

## Prompt Processing

| Concurrent Streams | Prompt tok/s (per stream) |
|---|---|
| 1 | 641 |
| 2 | ~409 |
| 3 | ~389 |
| 4 | ~156 |
| 5 | ~253 (4 streams) + 662 (1 solo) |

Prompt processing benefits more from batching at low concurrency, then memory-bandwidth-limited at 4+ streams.

## Reproduce

```bash
# From inside the continuum-core container on a CUDA-enabled host:
docker exec continuum-continuum-core-1 bash /tmp/bench-concurrent.sh

# Or from host (if model-runner TCP is enabled):
curl -s -X POST http://localhost:12434/engines/llama.cpp/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf",
       "messages":[{"role":"user","content":"Hello"}],"max_tokens":150}'
```

## Comparison Context

| System | Hardware | Model | Single-Stream | 4-Concurrent |
|---|---|---|---|---|
| **Continuum (this bench)** | RTX 5090 CUDA | Qwen3.5-4B Q4_K_M | 249.6 tok/s | 565.7 tok/s |
| **Continuum (Mac)** | M5 Metal | Qwen3.5-4B Q4_K_M | ~50 tok/s | ~measured by m5 |
| **Continuum (Mac)** | M1 Pro Metal | Qwen3.5-4B Q4_K_M | 34.1 tok/s | not yet measured |
