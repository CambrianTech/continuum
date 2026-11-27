# Compacted Model Integration Phases

How the compressed Qwen2.5-Coder-32B-Compacted gets wired into Continuum
as a usable local coding agent, and how the compression pipeline becomes
a self-service tool for users.

## Phase 1: Local Coding Agent (Immediate)

**Goal**: Helper AI persona uses the compacted 32B as its coding backend instead of cloud API.

### Tasks
- [ ] Wire `CandleAdapter` to auto-discover GGUF models in `~/.continuum/genome/models/`
- [ ] Add model selection config: persona can specify `localModel: "continuum-ai/qwen2.5-coder-32b-compacted"`
- [ ] Auto-download from HuggingFace on first use (no key needed for public models)
- [ ] Integrate with existing RAG pipeline — system prompt + codebase context → local inference
- [ ] Chat formatting: wrap prompts in Qwen2 `<|im_start|>` template
- [ ] Test: Helper AI answers coding questions using local 32B instead of cloud API
- [ ] Benchmark: compare local 32B quality vs cloud Sonnet/GPT-4 on same coding tasks

### What This Proves
Users install Continuum, it downloads the model, and their coding agent works offline.
Zero API keys, zero cloud costs, zero data leaving their machine.

## Phase 2: Quality Iteration (Next)

**Goal**: Improve output quality through mixed quantization and prompt engineering.

### Tasks
- [ ] Build mixed-quant GGUF using the planner + writer (high-util layers → Q5_K, low → Q3_K)
- [ ] Benchmark mixed vs uniform Q3_K_S on the 5-prompt suite
- [ ] Tune generation parameters (temperature, top_p, repetition_penalty) to reduce Q3_K_S repetition
- [ ] Test with longer prompts (full function specs, multi-file context)
- [ ] Publish improved model to HuggingFace, update model card with new benchmarks
- [ ] Try Qwen2.5-Coder-14B compacted for 16GB MacBook Air target

### What This Proves
The compression pipeline produces models worth using daily, not just demos.

## Phase 3: Self-Service Compression (Pipeline)

**Goal**: Users run `./jtag plasticity/compress` to make their own models.

### Tasks
- [ ] End-to-end test: capture → plan → compress → verify → infer
- [ ] Compute adapter: RunPod (training), local (compression), Reticulum (future)
- [ ] Support Llama, Mistral, Gemma architectures (not just Qwen2)
- [ ] Auto-detect architecture from config.json
- [ ] Derive head_dim from tensor shapes (no hardcoded fallbacks)
- [ ] Pad dimensions for Q4_K_M/Q4_K_S block alignment
- [ ] HuggingFace publish command: `./jtag genome/publish --repo=my-org/my-model`
- [ ] Model card auto-generation with benchmark results

### What This Proves
Anyone can compress any model for any device. Continuum is a platform, not a product.

## Phase 4: Continuous Learning Loop (Academy Integration)

**Goal**: Personas improve their own models through usage.

### Tasks
- [ ] Academy pipeline: persona uses model → captures mistakes → generates training data
- [ ] Re-score utilization on new data (coding tasks the persona actually does)
- [ ] Re-compress with updated scores (heads that matter for THIS user's tasks)
- [ ] A/B test: old model vs new model on the persona's actual workload
- [ ] Auto-deploy if quality improves, rollback if not
- [ ] Sentinel orchestrates the full loop as a scheduled pipeline

### What This Proves
The AI optimizes itself. The model gets better at what YOU use it for, not what
a generic benchmark measures. Personalized compression.

## Phase 5: Ecosystem (Distribution)

**Goal**: Community of compressed models on HuggingFace.

### Tasks
- [ ] "Made with Continuum" badge/tag on HuggingFace
- [ ] Model leaderboard: compare compressed variants by quality/speed/size
- [ ] User-submitted models (compressed with their own datasets)
- [ ] Voice models (TTS compression using same pipeline)
- [ ] Agent templates (persona + model + tools bundled together)
- [ ] Reticulum compute marketplace: rent GPU time for training, pay with coin

### What This Proves
Continuum is the platform that a community builds on. The Linux of local AI.

## Current Status

| Phase | Status | Key Deliverable |
|-------|--------|----------------|
| 1 | **In Progress** | Local coding agent on 32GB MacBook |
| 2 | Ready to Start | Mixed-quant GGUF, quality improvement |
| 3 | Foundation Built | planner.rs, gguf_writer.rs, pipeline.rs |
| 4 | Designed | Academy + Sentinel integration points |
| 5 | Vision | HuggingFace ecosystem |

## Architecture Alignment

All phases build on the same Rust infrastructure:
- `plasticity/` module: scoring, topology, compactor, planner, gguf_writer, pipeline
- `inference/` module: Candle GGUF backend with Qwen2 + variable head support
- `python_adapter`: unified subprocess wrapper for training (the only Python)
- Command generator: `plasticity/compress` IPC command (generated, proper pattern)
- 142 Rust tests across the plasticity module
