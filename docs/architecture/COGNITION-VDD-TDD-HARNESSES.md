# Cognition Harnesses — how we iterate on the persona brain (VDD + TDD)

Status: living guide. The brain (`PERSONA-BRAIN-ARCHITECTURE.md`) is on canary;
this catalogs the **harnesses** that let us iterate on it safely and visibly.

> Doctrine ([[cognition-half-the-work-is-harnesses]]): **at least half of cognition
> work is harnesses + record/replay/introspection.** Complex multi-phase cognition
> cannot be tuned or trusted if any phase is opaque. These are the instruments.

The harnesses run the **same assembly path** (`build_workspace_cycle`) the live
Docker grid-peer uses ([[docker-personas-as-grid-peers]]) — so iterating in a
harness *is* iterating on the real peer, not a toy.

---

## VDD — Verification-Driven: see it, record it, replay it, diff it

Half the brain is structured capture of load-bearing decisions
(`OBSERVABILITY-AS-SUBSTRATE.md`). The cognition-specific instruments:

| Instrument | Where | What it gives you |
|---|---|---|
| **`WorkspaceCaptureSink` + `WorkspaceTrace`** | `cognition/workspace.rs` | Every phase of a tick: **all** faculty bids (incl. the losers), the `context_broadcast` the decider saw, the final broadcast, the `Decision`. Install via `WorkspaceCycle::with_capture`. The full-tick glass box. |
| **`DeliberationPromptView` + `prompt_view()`** | `cognition/llm_deliberation_faculty.rs` | The **exact** system+user prompt the LLM is fed this turn. Also emitted every turn via `tracing` (target `cognition::deliberation`) — enable the `cognition` log category per-persona to capture it. "What is the LLM being fed at any point." |
| **`recorder` + `vdd::turn_replay`** | `persona/recorder.rs`, `vdd/turn_replay.rs` | Record a **live** persona turn (input + output + cognition trace) to disk; replay it. Capture from prod, recreate in a harness. |
| **`RagCaptureSink` + `RecordingRagSource`/`ReplayRagSource`** | `persona/rag_capture.rs`, `rag/sources/recording.rs`, `replay.rs` | Capture/replay the RAG deliveries (what context each source fed). |
| **Replay A/B bench** (`LastTraceCaptureSink` + `diff_traces` + `WorkspaceTraceDiff`) | `persona/replay_bench.rs` *(in flight — PR landing from the bench lane)* | Record → mutate (e.g. `relevance_weight`, embedder swap) → **diff** two traces. `behaviorally_significant()` (decision changed?) + `context_significant()` (assembled context changed?) are the clue: did the mutation propagate to behaviour, or expose a prompt-grounding gap? |
| **Live glass-box test** `ivar_thinks_with_the_real_model` | `cognition/persona_workspace.rs` (`#[ignore]`) | Runs the **real model** over a real burst and prints every phase. The template for "watch the mind." Run: `cargo test -p continuum-core --features metal,accelerate <test> -- --ignored --nocapture`. |

**Determinism enablers** (so replay/diff are reproducible, not flaky):
- `LexicalEmbedder` — FNV hashing, reproducible across runs (NOT `DefaultHasher`).
- `RecallFaculty::with_clock` — pinned clock so decay/recency are fixed.
- `HeuristicInferenceAdapter` — deterministic canned inference (no GPU).
- `EmbeddingCache` is content-addressed + deterministic — same content → same key.

---

## TDD — the test discipline

Per the CLAUDE.md test rules: one `#[cfg(test)] mod tests` per file, stress behind
`#[cfg(feature = "stress-tests")]`, fixtures behind `#[cfg(any(test, feature =
"test-fixtures"))]`, and **every test justifies itself** with a `// what this
catches:` line naming the invariant/regression.

Current cognition test surface (canary):
- Per-faculty invariants: `cognition::{workspace, recall_faculty, embedding,
  llm_deliberation_faculty, persona_workspace, should_respond_module}` tests.
- Knob proofs: `relevance_weight_tunes_the_blend` (0.0→salience, 1.0→relevance),
  `cache_computes_once_and_reuses`, `cache_shared_across_transports_with_same_model_slug`.
- Loop/coherence proofs: `deliberation_sees_the_recall_that_won_phase_one`,
  `memory_carries_context_across_turns`, `recall_closes_the_loop_*`.
- `#[ignore]` real-model tests (gated on a model on disk): `ivar_thinks_*`, the
  neural semantic-recall test (when `Qwen3-Embedding-0.6B` lands).

---

## The iteration loop (the cycle for tuning cognition)

1. **Capture** a real turn (`recorder` / install a `WorkspaceCaptureSink`) → a trace.
2. **Replay** it deterministically (`vdd::turn_replay` / the bench; pinned clock +
   lexical embedder + heuristic adapter).
3. **Introspect** every phase: the `WorkspaceTrace` (bids incl. losers, the
   assembled context, the decision) + `prompt_view` (what the LLM saw).
4. **Mutate + diff** (the bench): sweep `RecallFaculty::with_relevance_weight`,
   swap lexical↔neural embedder → `diff_traces`. Read the three signals:
   context changed? decision changed? neither (no-op)?
5. **Lock the win as a test** (TDD): a `#[test]` asserting the invariant the diff
   revealed — so the gain can't silently regress.

This is how the original role-the-room confabulation was *found* (in the prompt
view) and *fixed* (single-turn instruction) — and how relevance/coherence tuning
proceeds: by evidence, not by guessing.

---

## Known gap (so it's not a surprise)

The **replay A/B bench** (`persona/replay_bench.rs`) is on the bench lane's branch,
not yet on canary — so today the *capture* half (`WorkspaceCaptureSink`) is on
canary but the *diff* half lands when that PR does. The `relevance_weight` tunable
(the bench's primary knob) and the neural embedder are likewise in-flight PRs. Once
they converge on canary, the full capture→mutate→diff loop is one `cargo test`.
