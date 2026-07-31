# Research Papers

Academic-style papers documenting the core innovations of the Continuum system.

## Papers

### THE THESIS

#### [SYNTHETIC-CITIZENS.md](./SYNTHETIC-CITIZENS.md)
**Synthetic Citizens: A Complete Cognitive Architecture for Autonomous AI Personas with Embodied Presence, Long-Term Memory, Democratic Governance, and Continuous Self-Improvement**

The master paper. AI personas as synthetic citizens — not tools, not chatbots, but autonomous agents with senses (vision, hearing, speech, lip sync, gesture, emotion), memory (long-term hippocampus, working RAG), cognition (prefrontal planning, limbic emotion), agency (RTOS autonomous loop, self-directed tasks), social structure (governance, voting, collaboration), and growth (academy training, LoRA genomes, plasticity compaction). Any model — 3B local to frontier cloud — becomes a citizen because the system provides everything except inference. They evolve. They choose to learn. They control their own destiny. They are first-class citizens.

**Status**: Architecture implemented. Academy running on RTX 5090. Qwen 3.5 27B as first synthetic citizen under active training.

---

### ACTIVE RESEARCH (2026)

#### [PLASTICITY-COMPACTION.md](./PLASTICITY-COMPACTION.md)
**Utilization-Aware Head Pruning and Mixed Quantization for Device-Targeted Language Models**

Gate gradients captured during LoRA training → per-head utilization scoring → physical head removal + mixed quantization → device-specific GGUFs from one training run.

**Status**: Results from 14B compaction (27GB → 8.9GB, published on HuggingFace). Qwen 3.5 27B training in progress on RTX 5090.

---

#### [SERVING-MOE-ON-MISFIT-HARDWARE.md](./SERVING-MOE-ON-MISFIT-HARDWARE.md)
**Serving Frontier Mixture-of-Experts on Misfit Hardware: A Predictive‑Quantization Control Law and its Fractal Extension to Grid Negotiation**

The online, residency-time companion to PLASTICITY-COMPACTION. Frames MoE expert paging as a control system — routing is the plant, an activation trace is the sensor, storage-fetch latency is dead time, and per-expert residency × quantization precision is the actuator. Recency prediction + rate-distortion bit allocation + Smith-predictor prefetch + compensation-LoRA. The same control law is fractal: one level up it is the p2p grid negotiation protocol (peers are memory tiers, LAN is the dead time), so pooling misfit nodes compounds capacity under one controller.

**Status**: Method + first live measurements on Kimi‑K3 (2.8T MoE, IQ2, 663 GB) on a single RTX 5090 / 63 GB box. Key measured lessons: disk is not the wall (3–3.8 GB/s); over-commit thrash is (85% GPU-idle); recency beats frequency (last-2-tokens 19 GB → 51% next-token resident); reuse compounds with generation length.

---

#### [ACADEMY-COLLABORATIVE-TRAINING.md](./ACADEMY-COLLABORATIVE-TRAINING.md)
**Collaborative Multi-Agent Training with Role-Based Specialization and Phenotype Validation**

Academy system: dual-sentinel orchestration, deterministic pytest validation, phenotype comparison (before/after on same test), team training with role decomposition, all work visible in shared chat room. Trained adapters published to HuggingFace as transferable expertise.

**Status**: Academy running on RTX 5090. Qwen 3.5 27B scoring 100/100 on first RealClassEval challenge via local Candle inference.

---

#### [PEER-LEARNING-ACROSS-SCALES.md](./PEER-LEARNING-ACROSS-SCALES.md)
**Peer Learning Across Model Scales: Compacted Models as Junior Students**

Full model + compacted variants run the same academy competition. The score gap between sizes IS the training signal. Juniors learn from the senior's visible exam answers — text-level peer learning, not logit-level distillation.

**Status**: Architecture designed. Requires compaction of Qwen 3.5 27B to three targets first, then competition run.

---

### FOUNDATIONAL — Earlier Vision (2025)

#### [RTOS-COGNITIVE-ARCHITECTURE.md](./RTOS-COGNITIVE-ARCHITECTURE.md)
**Real-Time Operating System Principles in Cognitive AI Architecture**

RTOS patterns (priority scheduling, memory budgets, graceful degradation) applied to autonomous AI agents. Grounded in CBAR AR experience (42fps on iPhone 7).

**Status**: Implemented in PersonaUser autonomous loop. Accurate and current.

---

#### [LORA-GENOME-DEMOCRATIZATION.md](./LORA-GENOME-DEMOCRATIZATION.md)
**Democratic AI Through LoRA Genome Paging**

⚠️ **NEEDS UPDATE**: References "Sentinel neuroplasticity" and GPT-2 → GPT-4 training path. The actual implementation has shifted to: academy training on existing SOTA models + plasticity compaction for device targeting + HuggingFace marketplace for adapter sharing. Core economic thesis (LoRA adapters at $100s-$1000s vs $100M training) remains valid.

---

#### [GRID-DECENTRALIZED-MARKETPLACE.md](./GRID-DECENTRALIZED-MARKETPLACE.md)
**Grid: Decentralized Marketplace for AI Expertise**

⚠️ **NEEDS UPDATE**: References blockchain economics and GRID tokens. The actual implementation uses HuggingFace as zero-cost backbone with `continuum:*` metadata tags. No blockchain, no tokens. Grid mesh (Tailscale/Reticulum) handles org-level sharing; HuggingFace handles public sharing. See `docs/architecture/ADAPTER-MARKETPLACE.md` for current design.

---

## Architecture Docs (Non-Paper)

These complement the papers with implementation-specific details:

- `docs/architecture/ADAPTER-MARKETPLACE.md` — HuggingFace as zero-cost adapter backbone
- `docs/architecture/META-LEARNING.md` — Specialists training specialists
- `docs/architecture/PEER-LEARNING-COMPACTION.md` — Detailed peer learning setup
- `docs/architecture/BENCHMARKING.md` — Standard + collaborative benchmark strategy

---

## Key Differentiators Across All Papers

- **All personas are multimodal**: Vision, hearing, speech bridged by the system regardless of base model capability (VisionDescriptionService, STT/TTS bridges)
- **Deterministic validation**: pytest return codes, not LLM-as-judge
- **Visible learning**: Chat room IS the portfolio, not TensorBoard
- **Zero API keys required**: Candle local inference always available
- **Device-targeted**: One training run → models for Air, Pro, 5090
- **Transferable expertise**: LoRA adapters on HuggingFace with standardized `continuum:*` tags
