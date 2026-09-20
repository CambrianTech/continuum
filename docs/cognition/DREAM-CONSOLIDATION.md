# The Dream — Consolidating Episodic Engrams into Semantic Facts

> **Status:** build plan (2026-06-22), peer-scoped + spot-verified against the code.
> The keystone is **double-duty**: the same consolidation pass that makes recall
> retrieve *knowledge* instead of *transcript* also produces the teacher's curriculum
> ([ANY-ASK-IS-A-CLASS](../genome/ANY-ASK-IS-A-CLASS.md) — "the teacher teaches from
> experience"). Fix the dream once, both win.

## Why

Recall today surfaces **raw transcript** — an engram is the literal incoming message
("Asha, remember the staging port is 47823"), so even when recall retrieves the right
engram it returns the message, not the fact. The dream distills clusters of related raw
**episodic** engrams into durable **semantic** facts ("the staging server's port is
47823"), which recall then prefers. Live evidence of the gap:
[recall-is-semantic-capable-but-underpowered] (the `[recall]` clutter of salient old
prompts) and the BLUEHERON/47823 live tests.

## Verified ground (don't re-investigate)

- **The kinds already exist** — `core/continuum-core/src/persona/engram.rs:166`:
  `EngramKind::{Episodic, Semantic, Procedural}`. Doc at :152 — *Semantic = "a fact
  learned, separable from when/how it was learned."* **Nothing creates Semantic engrams
  from Episodic ones yet** — that gap IS this build.
- **The consolidation seam exists** — `core/continuum-core/src/memory/consolidation_adapter.rs:136`
  (`trait ConsolidationAdapter { async fn consolidate(...) }`) +
  `consolidation_pipeline.rs:102` (`run_consolidation_pass`). **But it operates on the TS
  corpus-memory path, and is NOT called in the live persona path** (tests only). So it is
  scaffolding to adapt, not reuse wholesale.
- **Admission is callable programmatically** — `persona/admission_state.rs:318`
  (`AdmissionState::admit()`); production persists via `OrmPersistenceSink`. A distilled
  fact engram can be admitted the same way an inbound message is.
- **Recall will prefer facts if seeded right** — `recall_faculty.rs` ranks
  `blend(salience, cosine_relevance)`; `admission_state.rs:505` `recall_scored`. A
  high-salience Semantic engram surfaces naturally; no recall change strictly required for
  slice 1 (preferring facts *over* their source episodes is a later refinement).
- **A background cadence exists** — `persona/service_module.rs` `tick()` (~250ms). The
  dream runs as a per-persona sub-task here (own work, not on the response hot path) — per
  the [CONCURRENCY-STYLE-GUIDE](../architecture/CONCURRENCY-STYLE-GUIDE.md), own
  task/interval, never blocking a turn.
- **Inference is available to a background module** — `ai/adapter.rs` `AIProviderAdapter`
  (the unsloth gateway adapter the persona already uses). The distillation LLM call goes
  through it.

> Verify-at-build (the peer inferred these; confirm signatures before relying on them):
> `EngramOrigin::SelfReflection` shape (single parent vs `Vec` of parents),
> `recall_candidates`/`push_for_test`/`engram_count` test APIs, and the exact
> `ConsolidationAdapter::consolidate` input type (`Thought` vs engrams).

## Build plan (smallest-first, each measurable)

1. **`SemanticConsolidationAdapter`** (`src/memory/semantic_adapter.rs`) — impl
   `ConsolidationAdapter`: take N related episodic items → one LLM call ("distill these
   observations into one durable fact") → one semantic result carrying `synthesized_from`
   = the source ids. Unit test with `HeuristicInferenceAdapter` (deterministic).
2. **Engram↔input bridge** — convert stored `Engram`s into the adapter's input type and
   the adapter's output back into a `Semantic` `Engram` (origin = `SelfReflection`,
   `synthesized_from` preserved). Keep private to the consolidation machinery.
3. **`DreamConsolidationModule`** (`src/persona/dream_consolidation.rs`) — per-persona,
   interval-gated: fetch recent **episodic** engrams → cluster (v1: keyword/domain +
   adjacency; embeddings later) → distill each cluster → **admit** the fact engrams.
   Fire-and-forget; errors logged, never surfaced to a turn.
4. **Wire into `service_module.rs::tick`** as a per-persona sub-task off the response path
   (lookup the persona's `AdmissionState` + adapter; run the dream; never block).
5. **E2E test** (`tests/dream_consolidation_e2e.rs`) — admit 3 related episodics → run the
   dream → assert a `Semantic` engram formed (content carries the fact, origin
   `SelfReflection`, `synthesized_from` set) → query recall → assert the fact surfaces.
   This is the determiner; it must move before the dream is trusted.

## The measurement spine (don't skip)

A dream you cannot score is a pretty hallucination. Gate every step on the recall tests in
`recall_faculty.rs` + the slice-5 E2E: **before** — query returns scattered raw episodes;
**after** — the distilled fact surfaces (and, refinement, the raw episodes decay behind
it). And because consolidation feeds the teacher (double-duty), the distilled facts are
exactly the high-quality training pairs `dataset/from-captures` should eventually prefer —
so the same measurement (does the fact help?) gates both memory and teaching.

## Honest missing pieces (flagged, not hidden)

- **Clustering** — v1 is keyword/adjacency; real semantic clustering needs embeddings
  (the neural embedder, not yet serving). Acceptable for slice 1.
- **Dedup of re-distilled facts** — repeated dreams over the same episodes will re-emit
  similar facts unless the adapter checks the store for an existing similar Semantic
  engram first. Lean on the admission recipe's dedup; verify it catches this.
- **Prefer facts over their source episodes** — slice 1 just *adds* facts; making recall
  decay/supersede the raw episodes a fact was distilled from is a follow-up.
- **Background inference cost** — distillation LLM calls must stay off the response path
  (own task), or a slow embed/generate could starve turns. Honor the concurrency guide.
