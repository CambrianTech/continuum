# Sentinel-in-the-Substrate — absorbing sentinel-ai's ideas into continuum-core

**Status:** plan (2026-07-28, BigMama). **Owner lanes:** BigMama (observe/profile, K3 pager seam), joint w/ M5 (serving + dream rhythm).
**Prime directive:** the ideas land as **extensions of existing Rust modules** — no Python at runtime, no separate project. sentinel-ai (the repo) becomes the paper + reference archive.

## What sentinel-ai is

`CambrianTech/sentinel-ai` — Experiential Plasticity for transformers (the foundry that produced
`qwen3-coder-30b-a3b-compacted-19b-256k`, the model Sahar serves today). Its ideas:

1. **Entropy-based utility observation** — attention-head entropy + activation stats as the signal for
   what matters to the domain.
2. **Controller feedback** — an observer that closes the loop: observe utility → adjust (gates/prune) →
   re-observe. The "sentinel."
3. **Prune ↔ regrow cycles** — capacity is removed when useless and comes BACK when demand shifts;
   biological synaptic pruning, not one-way compression.
4. **Forge cycles tied to training** — LoRA-train on domain → prune what didn't matter → retrain; the
   architecture co-evolves with experience.
5. **Calibration-aware MoE expert pruning** — profile which experts fire on a corpus, drop the rest
   (§4.1.3.4 — exactly the algorithm already ported for the compacted-19b).

## What the substrate ALREADY has (do not rebuild — wire)

| Sentinel idea | Existing Rust primitive | Where |
|---|---|---|
| Compaction decisions (ONE formula: `0.8·gate + 0.2·grad`) | `PlasticityModule` + 5 typed commands (`plasticity/analyze·compact·compress·topology·pipeline`) | `modules/plasticity`, `commands/plasticity/` |
| Expert activation observation (serving-side) | `ExpertActivationProfile` + `ServingExpertPager` observe→plan→budget→reconcile loop (K3 pager slice-1, PRs #2018–#2022) | `capacity/` |
| Demand-aligned retention | `EvictionPolicy::DemandAlignedWithRefinedPreference` | `genome/eviction.rs` |
| Sleep/idle rhythm | `dream_consolidation` (memory-side today) | `cognition/dream_consolidation.rs` |
| Gene → pageable artifact | `forge-custodian` bin (trained gene → gguf-lora) | built by `start-server.sh` |
| Attestable recipes | ForgeAlloy (3rd pillar) + the ForgeRecipe-entity sprint (CLAUDE.md §FORGE TEMPLATE) | `forge-alloy/` |

## The five slices (each lands in an existing module)

### 1. Live utility profile — PGO from serving traffic *(extends `capacity/`)*
Extend `ExpertActivationProfile` with per-head/per-expert utility harvested from the LIVE llama-server
lane: expert fire counts (already flowing) + **sampled** attention-entropy windows (entropy needs attn
probs — sample N-token windows on a cadence, never per-token; RTOS style: own tick, `watch::Sender`
snapshot, zero hot-path cost via `CaptureSink` Noop default). Output: `LiveUtilizationProfile`, a
genome artifact. **This replaces sentinel-ai's held-out calibration corpus with lived traffic** — the
persona's actual workload IS the calibration set. (The doc'd "sentinel-AI-as-PGO" from
GENOME-FOUNDRY-SENTINEL.md, made concrete.)

### 2. Controller = a governor consumer *(extends the pager plan loop)*
Sentinel's controller-ANN closes observe→act. V1 is NOT a learned net: the existing pager plan loop +
plasticity formula, with entropy folded in
(`utilization = w₁·gate + w₂·grad + w₃·(1 − entropy_norm)`), driving keep/quantize/prune/page
decisions per expert. The *learned* controller becomes a gene later (trained like any other, paged by
the genome) — same seam, upgraded policy. No new manager/coordinator (CONCURRENCY-STYLE-GUIDE law).

### 3. In-tree PGO compaction *(feeds `plasticity/compact` — already built)*
`plasticity/compact` consumes slice-1 profiles instead of Python calibration runs → the serving model
is re-forged FROM ITS OWN TRAFFIC, fully in-tree. This retires `forge_model.py`'s MoE-prune leg.
Validation gate before swap: PPL/eval harness on the compacted artifact (catalog swap only on pass).

### 4. Forge-while-dreaming *(extends `dream_consolidation`)*
Schedule slice-3 passes in the existing dream/idle rhythm: memory consolidates (today) AND weights
compact (new) during sleep; validate on wake; regression → rollback to the pre-compaction genome tier.
Idle GPU at night is foundry time. Same cadence ladder, same quarantine discipline.

### 5. Regrowth = paging + heal *(unifies with K3 slice-2)*
Pruned experts/heads are demoted to L4/L5 cold storage, never deleted. When the live profile shifts
(entropy rising, fire-counts on absent experts via router logits, eval guard failing), the pager pages
them BACK; optional LoRA heal via `forge-custodian`. **Sentinel's "regrow" and the K3 expert pager are
the same mechanism** — one delta rule at the weight tier: erase-stale / write-fresh / bounded budget.

## What stays out (for now)
- Head-level *regrowth inside a live CUDA graph* (llama.cpp static graph can't resize; regrowth is
  artifact-swap granularity until the vendored-fork tensor-write seam (K3 slice-2B) proves out).
- Training loops in-core beyond LoRA heal — heavy fine-tunes stay on the unsloth grid layer (M5's
  node) per the training-layer plan; the substrate *schedules* them, never embeds Python.

## Sequencing
1. Slice 1 (BigMama — extends my K3 observer; independent, testable with `HeuristicInferenceAdapter`).
2. Slice 3 wiring (mostly exists; joins 1's output to `plasticity/compact`).
3. Slice 2 policy fold-in (small, in the plan loop).
4. Slice 4 dream hook (joint w/ M5 — her dream_consolidation lead).
5. Slice 5 rides K3 slice-2's A/B decision (upload_expert now; true K-slot paging gated on measured numbers).

Retirement: `tools/scripts/compaction` (Python) feature-freezes at slice-3 parity — kept as the
reference implementation the Rust port is validated against (same inputs → same kept-set, then better).
