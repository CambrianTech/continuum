# Many-Worlds Integration — Population Intelligence for Continuum

## Overview

Many-Worlds is Continuum's architecture for combining multiple frozen base models into a population that outperforms any individual member. It extends the same principles that power every other layer of the system:

| Layer | What It Combines | Coordination Primitive |
|-------|-----------------|----------------------|
| **Grid** | Compute nodes | Capability vectors (§10.5) |
| **Personas** | AI personalities | LoRA genome paging |
| **Many-Worlds** | Base models | Substrate + Q-Former |
| **Forge** | Pruning strategies | Activation profiles |

**Core principle: Don't retrain, translate.** Each model keeps its native form. The substrate translates between their internal representations. The Q-Former speaks each model's vocabulary. The confidence gate knows when to contribute and when to stay silent.

## How It Fits in Continuum

### Factory Widget — Team Assembly Console

The Factory UI gains a new panel: **Population Forge.** Users:

1. **Select target benchmark** — what capability to optimize for (code, math, reasoning, knowledge)
2. **Browse model roster** — available models with their divergence scores against each other
3. **Auto-search** — the forge runs divergence search to find the optimal team for the target
4. **Train** — one click starts substrate + Q-Former training. Progress visible in the forge pipeline view.
5. **Deploy** — the population appears as a single model endpoint in Continuum

The team search results appear as a divergence matrix visualization — a heatmap showing which pairs complement each other. The user (or the Foreman) selects the team that maximizes coverage.

### Foreman — Population Manager

The Foreman on each grid node manages the local population:

- **Roster management** — which source models are cached, which adapters are loaded
- **Task routing** — incoming requests matched to the best population for that task type
- **Gate profiling** — tracks which source models fire on which task categories
- **Pruning recommendations** — identifies source models that never contribute (waste VRAM)
- **Experience** — capability centroids update through use, making routing faster over time

The Foreman already knows the node's hardware (VRAM, compute). It picks the population that fits:
- 8GB VRAM: 2 small specialists (1.5B + 1.5B) → target (3B)
- 16GB VRAM: 3 specialists → target (4B)
- 32GB VRAM: 5 specialists → target (7B)
- Grid multi-node: sources on different nodes, substrate fields transferred via mesh

### LoRA Genome — Composition with Many-Worlds

The LoRA genome pages skill adapters in/out for a single model. Many-Worlds does the same at the base model level. They compose:

```
Base model (e.g., Phi-3-mini)
  ├── LoRA adapters (persona skills — paged by genome)
  │     ├── coding-expertise adapter
  │     ├── teaching-style adapter
  │     └── humor adapter
  │
  └── Many-Worlds substrate (population knowledge — via Q-Former)
        ├── Math specialist contribution (gated by confidence)
        ├── Code specialist contribution (gated by confidence)
        └── Knowledge specialist contribution (gated by confidence)
```

A persona running on Phi-3 can have BOTH:
- Its own personality/skill adapters (LoRA genome)
- Access to the broader population's knowledge (Many-Worlds substrate)

The soft tokens from the Q-Former prepend to the input BEFORE the LoRA adapters process it. The population knowledge flows through the persona's skill lens.

### Grid — Population Distribution

On the grid, populations can be distributed across nodes:

```
Node A (8GB VRAM):
  - Qwen2.5-Math-1.5B (source)
  - Substrate field cache for recent queries

Node B (8GB VRAM):
  - Qwen2.5-Coder-1.5B (source)
  - Substrate field cache for recent queries

Node C (16GB VRAM):
  - Phi-3-mini (target) + Q-Former + adapters
  - Receives substrate fields from Node A and B via mesh

Task arrives → Node C requests substrate fields from A and B →
fields arrive via grid transport → Q-Former produces soft tokens →
Phi-3 generates with population knowledge
```

The substrate fields are small (seq_len × 256 floats = ~2KB per source per query). Grid transport handles them like any other event payload. The §10.5 capability matching routes tasks to the node whose population covers the task's needs.

### Persona — Population-Backed AI Citizens

Each AI persona in Continuum can be backed by a Many-Worlds population:

- **Teacher persona**: Phi-3 target + knowledge specialist + pedagogy model → explains with depth AND clarity
- **Coder persona**: CodeLlama target + math specialist + architecture model → writes code that handles edge cases
- **Creative persona**: Mistral target + poetry model + cultural knowledge model → creates with style AND substance

The persona's personality comes from its LoRA genome. Its KNOWLEDGE comes from the population. Different knowledge sources for different personas, same substrate infrastructure.

## Architecture

### Substrate — The Shared Coordinate Space

A learned set of basis vectors (128 × 256d by default) that defines the shared representation space. All models project into it via per-model adapters. The substrate is trained once and reused — adding a new model trains one adapter, not the substrate.

### Q-Former — The Translation Layer

Learned query tokens (16 by default) cross-attend to the substrate fields from ALL source models simultaneously. Each query specializes in extracting a different aspect of the population's knowledge. The attention weights ARE the routing — no separate router needed.

**Vocab-grounded output:** Each soft token is a weighted combination of REAL token embeddings from the target model's vocabulary. The target model processes them using the same pathways it uses for all tokens. No foreign vectors, no distribution shift.

### Confidence Gate — Know When to Speak

Per-query confidence scalar in [0, 1] that controls how much the population contributes. Starts low (~0.27), trained to open only when the substrate genuinely helps prediction. On inputs where the target model already knows the answer, the gate stays closed. The gate profiling data drives population-level pruning.

### Source Adapters — Per-Model Projection

Each source model has a learned adapter (~1.5M params) that projects its hidden states at 2/3 depth into the shared substrate space. The adapter is the ONLY trained component specific to each source model. Adding a new model to the population costs one adapter training (~8 minutes on RTX 5090).

## Forge Integration

Many-Worlds is a forge recipe type:

```json
{
  "type": "many-worlds",
  "target": "microsoft/phi-3-mini-4k-instruct",
  "sources": [
    "Qwen/Qwen2.5-Math-1.5B-Instruct",
    "Qwen/Qwen2.5-Coder-1.5B-Instruct",
    "Qwen/Qwen3-4B"
  ],
  "substrate_dim": 256,
  "num_queries": 16,
  "calibration_corpus": "benchmark_training_mix.jsonl",
  "target_benchmark": "open_llm_leaderboard_v2",
  "vram_budget_gb": 16,
  "stages": [
    {"type": "team-search", "pool": "huggingface:continuum-ai/*"},
    {"type": "many-worlds-substrate", "steps": 8000},
    {"type": "eval", "benchmark": "gsm8k,arc,winogrande"},
    {"type": "gate-profile", "export": "gate_weights.json"},
    {"type": "population-prune", "min_contribution": 0.05},
    {"type": "publish", "repo": "continuum-ai/avengers-v1"}
  ]
}
```

The forge pipeline handles team search → substrate training → evaluation → gate profiling → population pruning → publishing. Same pipeline infrastructure as model forging, different recipe type.

## Attestation

The `.alloy.json` for a Many-Worlds population includes:
- Team roster with model versions and adapter hashes
- Substrate training provenance (corpus, steps, hyperparameters)
- Per-source gate profiling (contribution percentages per benchmark)
- Combined benchmark scores vs individual baselines
- Attestation chain linking every component

Same forge-alloy infrastructure, same QR codes, same verification page.
