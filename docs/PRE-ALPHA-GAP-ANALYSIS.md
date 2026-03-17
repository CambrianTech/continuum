# Pre-Alpha Gap Analysis

What needs to work for Continuum's first public release. Not feature-complete —
just enough that someone downloads it, sees it work, and wants more.

## Core Value Proposition

"Install Continuum. Get a local AI coding agent on your MacBook. No API keys,
no cloud, no data leaving your machine. It downloads its own model and works."

## Gap Status

### Local AI Inference (The Hook)

| Item | Status | Gap |
|------|--------|-----|
| Compacted 32B coding model on HuggingFace | DONE | Published: continuum-ai/qwen2.5-coder-32b-compacted |
| Auto-download model on first use | DONE | find_local_model() + HF fallback in CandleAdapter |
| GGUF inference on Metal (M1/M2/M3) | DONE | 5.3 tok/s, quantized_llama.rs with Qwen2 support |
| Qwen2 chat template formatting | GAP | Need `<\|im_start\|>` template in prompt builder |
| Model selection in persona config | GAP | Need `localModel` field in persona/AI provider config |
| Coding agent system prompt | GAP | Need coding-focused RAG system prompt for local model |
| 14B model for 16GB MacBook Air | GAP | Need to compress + publish smaller variant |
| Auto-detect device memory + pick model | GAP | 16GB → 14B, 32GB → 32B, auto-select |

### Compression Pipeline (The Differentiator)

| Item | Status | Gap |
|------|--------|-----|
| Gradient-based utilization scoring | DONE | scoring.rs, 40+ tests |
| Head topology planning | DONE | topology.rs |
| Tensor compaction (head pruning) | DONE | compactor.rs |
| Compression planner (recipe from scores) | DONE | planner.rs, 7 tests |
| GGUF writer (mixed quantization) | DONE | gguf_writer.rs, 2 tests |
| Pipeline orchestration | DONE | pipeline.rs, 4 tests |
| IPC command (plasticity/compress) | DONE | Generated + wired |
| Python subprocess adapter | DONE | python_adapter.rs, 4 tests |
| End-to-end test with real model | GAP | Need to run pipeline on actual safetensors |
| Mixed quantization benchmark | GAP | Compare uniform vs mixed quality |
| Dimension padding for Q4_K_M support | GAP | Unlock higher-quality quant levels |

### Persona System (The Experience)

| Item | Status | Gap |
|------|--------|-----|
| PersonaUser autonomous loop | DONE | Adaptive cadence, energy/mood |
| Persona inbox + priority queue | DONE | PersonaInbox with traffic management |
| Chat coordination | DONE | RTOS-style thought coordination |
| RAG pipeline | DONE | Codebase indexing, context injection |
| Tool execution | DONE | PersonaToolExecutor |
| Local model as persona backend | GAP | Wire CandleAdapter as AI provider option |
| Persona uses local 32B for coding | GAP | Phase 1 integration |
| Coding agent personality/prompt | GAP | System prompt optimized for code |

### Infrastructure (The Foundation)

| Item | Status | Gap |
|------|--------|-----|
| Commands.execute / Events system | DONE | Universal primitives |
| IPC (Rust ↔ TypeScript) | DONE | Unix socket, bidirectional |
| Data daemon (SQLite/Postgres) | DONE | Entity system |
| Sentinel pipeline engine | DONE | 10 step types, 103+ tests |
| Academy (training orchestration) | DONE | Teacher/student pipelines |
| LoRA fine-tuning | DONE | PEFT adapter, proven E2E |
| Genome/adapter management | DONE | AdapterStore, training memory guard |
| GPU memory management | DONE | Pressure tracking, eviction |
| npm start deployment | DONE | Build + deploy in one command |
| JTAG CLI | DONE | Full command discovery |

### Distribution (The Growth)

| Item | Status | Gap |
|------|--------|-----|
| HuggingFace org (continuum-ai) | DONE | https://huggingface.co/continuum-ai |
| First model published | DONE | qwen2.5-coder-32b-compacted |
| Model card with links to Continuum | DONE | Story, benchmarks, "Make Your Own" |
| Zero-key model download | DONE | Public models, no auth needed |
| Publish command (genome/publish) | GAP | Upload GGUF + model card from CLI |
| Multiple model sizes | GAP | 32B (32GB), 14B (16GB), 7B (8GB) |
| GitHub README showcasing local AI | GAP | Demo GIF, "try it in 2 minutes" |

### Compute Adapters (The Scale)

| Item | Status | Gap |
|------|--------|-----|
| RunPod adapter | PARTIAL | Shell scripts work, needs proper Rust adapter |
| Google Colab adapter | GAP | Free GPU option for users |
| Local GPU adapter | GAP | RTX 5090 / local CUDA |
| Reticulum (home GPU from anywhere) | GAP | Killer feature, Phase 5 |

## Priority for Pre-Alpha

**Must have** (blocks first impression):
1. Qwen2 chat template formatting
2. Model selection in persona config
3. Local model as persona AI provider
4. GitHub README with demo

**Should have** (makes it compelling):
5. 14B model for 16GB MacBook Air
6. Mixed quantization (quality improvement)
7. Auto-detect device memory + model selection
8. Publish command

**Nice to have** (builds ecosystem):
9. End-to-end pipeline test
10. Compute adapters
11. Multiple model variants
12. Reticulum

## What's Already Working

The hard stuff is done:
- 142 Rust tests in plasticity module
- 32B model running locally at 5.3 tok/s
- Model published on HuggingFace
- Compression pipeline (score → plan → compress → verify)
- Full IPC command system
- Persona autonomous loop

The gaps are mostly **wiring** — connecting pieces that individually work.
