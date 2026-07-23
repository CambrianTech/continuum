# The Belief Justification Graph — AGM revision over provenance edges

**Status:** design (Joel, 2026-07-22: "are these axioms, or could we design some kind of
logical system here? … you could design it"). Successor to the shipped plastic-memory
stack (#221 slices 1–3: decay drain, supersession review, learn bridge, rotating
window). Companion: [DREAM-CONSOLIDATION.md](DREAM-CONSOLIDATION.md),
`docs/architecture/COGNITION-ALGORITHMS.md`.

## 1. What beliefs are (and aren't)

A persona's Semantic engrams are **defeasible beliefs, not axioms**: held with a
confidence (salience × rehearsal), revisable (supersession, model-judged), and mortal
(decay). The shipped stack is, in classical terms, **AGM belief revision with a
learned revision operator**:

| AGM operation | Continuum mechanism (shipped) |
|---|---|
| Expansion (add belief) | dream distillation → `admit_reflection` (content-hash dedup) |
| Contraction (retract) | salience decay drain + `demote_to_floor` (floored, never deleted) |
| Revision (replace on conflict) | supersession review — the distiller judges `SUPERSEDES: n` on the same generation |

The classical failure of symbolic AGM was the revision operator (hand-written logic
can't judge real-world conflict). The LLM **is** the revision operator here — that
inversion is the paper contribution.

## 1b. The attention framing (Joel: "it's like attention graphs")

The same relation at three timescales. Transformer attention is soft and ephemeral —
which tokens bear on this judgment, this forward pass, then gone. Spreading-activation
recall (COGNITION-ALGORITHMS' bounded hop-decay propagation) is attention over engrams
at TURN timescale. The justification graph is attention made DURABLE at lifetime
timescale: which experiences bore on this belief, kept as edges instead of vanishing
with the activation. Cascade revision is then simply re-attention after a weight
change — a superseded belief is an updated weight, and everything that attended to it
gets re-scored by the model. One mechanism, three timescales, the slowest one finally
inspectable.

## 2. The gap the graph closes

Today the review only sees beliefs that are (a) lexically related to new experience or
(b) picked up by the rotating window. Two blind spots:

1. **Derived error survives its root.** If belief B was consolidated partly *from*
   belief A (fact-of-facts), superseding A leaves B standing on a dead foundation
   until the rotating window happens to reach it.
2. **No epistemic provenance.** "Why do you believe X?" has no queryable answer,
   though the data mostly exists.

## 3. The design: edges, not a new store

No new database. The graph is **edges over existing engrams**, persisted beside them:

```
belief_edges (engrams.sqlite sidecar table, via the existing admission persistence sink)
  edge_id      UUID PK
  from_engram  UUID     -- the belief holding the edge
  to_engram    UUID     -- what it points at
  kind         TEXT     -- 'derived_from' | 'superseded_by' | 'contradicts'
  created_ms   INTEGER
  origin       TEXT     -- 'distiller' (model verdict) | 'consolidation' (mechanical provenance)
```

Edge sources — all already computed, currently discarded:

- **`derived_from`** — `DistilledFact.source_ids` (dream provenance, exists since
  slice 1). When a cluster contains *Semantic* sources (fact-of-facts consolidation),
  those edges are the dependency structure a TMS calls justifications.
- **`superseded_by`** — the supersession verdict already yields (old_id, new_id)
  pairs at the demotion site (`hippocampus.supersede` probe); record them.
- **`contradicts`** — optional distiller vocabulary extension (`CONTRADICTS: n` for
  "in tension, neither clearly wins"); admit the tension without forcing a winner.
  Deferred until a live case demands it — do not build speculatively.

## 4. Mechanics (all retrieval/bookkeeping — the model stays the logician)

1. **Record** (slice A): at the two existing sites (consolidation admit, supersession
   apply), write the edges. No behavior change; pure provenance capture.
2. **Cascade revision** (slice B): when belief A is superseded, enqueue every belief
   with `derived_from → A` into the next dream's review set (union with lexical +
   rotating picks, same cap). Dependency-directed doubt: the *model* re-judges the
   dependents against current understanding — the graph only ensures they get their
   day in court. One supersession propagates through everything built on the error,
   instead of waiting for the rotating window's token luck.
3. **Provenance query** (slice C): `cognition/belief-trace --persona-id X --engram-id Y`
   → the justification tree (belief → sources → their sources), plus supersession
   history. Internal access; the persona-facing form is her own metacognition surface
   later ("what I used to think and why I changed").

## 5. Doctrine guards (non-negotiable)

- **The graph never decides.** No consistency prover, no automatic retraction beyond
  the model's own verdicts. Edges select *what enters the review*; the LLM judges.
  ([[cognition-is-always-ml-never-heuristic]], [[no-hardcoded-heuristics-to-steer-cognition]])
- **History is sacred.** Episodics are never graph-revised — only conclusions
  (Semantic) are. Plasticity eats inference, never experience.
- **Floored, not erased.** Superseded beliefs keep their rows and their history edges;
  the graph makes forgetting *legible*, not destructive.
- **Zero hot-path cost.** Edge writes ride the dream pass (already off-path);
  queries are operator/introspection surfaces.

## 6. Surfaces it feeds

- **Brain view / hippocampus gauge** ([../design/LEARNING-VISIBILITY-WIDGETS.md](../design/LEARNING-VISIBILITY-WIDGETS.md)):
  click a belief → see the experiences that taught it and what replaced it. The
  belief-graph rendered live is the "watch her change her mind" moment of the demo.
- **The paper:** LLM-as-AGM-operator over a TMS-style justification graph, with the
  falsifiable benchmark curve as evidence that revision improves capability.
- **Forensics:** a `stale-derivation` signature (belief whose foundation is
  superseded but which hasn't been re-judged yet) becomes a countable, watchable queue.

## 7. Build order

Slice A (record edges — ~small, two sites) → Slice B (cascade enqueue — small, one
union in `dream_pass`) → Slice C (belief-trace command — small, read-only). Each
slice independently testable; A ships value alone (provenance for widgets) even
before B/C. Prereq: none — all hooks exist as of `#221` slice 2b.
