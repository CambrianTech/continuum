# The Causal Memory Graph — acts join the engram edge substrate

**Status:** design (Joel, 2026-08-15: act memory is "completely broken… fundamental to
every activity and room" — overhaul it in general cognition, never in benchmark
rigging). Companion to [BELIEF-JUSTIFICATION-GRAPH.md](BELIEF-JUSTIFICATION-GRAPH.md)
(the epistemic level of the same graph), [ACTING-ORGANISM.md](ACTING-ORGANISM.md)
(the act→observe circuit these edges make traversable), and
`docs/architecture/COGNITION-ALGORITHMS.md` (the spreading-activation traversal that
walks them). Read `docs/architecture/PERSONA-COGNITION-PIPELINE.md` first.

## 1. The live evidence (round bench-hard-rs-1786823331, 2026-08-15)

A citizen with 1,808 lifetime acts sees **one** of them. The steps ledger renders from
a char-budgeted receipt archive whose every head line drags its full `because
<reasoning>` clause (~250 chars each), so the archive holds 1–2 lines before evicting;
the ledger then honestly reports `(+1808 earlier steps before what this ledger
shows)`. Downstream, measured across 4,482 captured turns: citizens execute thousands
of acts, cannot see them, conclude they have nothing to contribute, and withdraw — the
deprivation mechanism (#390/#414). Semantic recall is *forbidden* from returning
Tool-origin receipts by an executing assertion (#166 — correct, and staying).

The diagnosis is not budget tuning. **The `because` clause is a causal edge serialized
as English.** We are paying graph-edge information as flat prose in the one place a
citizen thinks from.

## 2. What every mature system converges on

Two pointers per node: *which thread am I part of* and *what directly caused me*.
Distributed tracing (`trace_id` + `parent_span_id`), event sourcing (`correlation_id`
+ `causation_id`), W3C PROV (`wasInformedBy`/`used`/`wasGeneratedBy`), git (commit
parents). Cognitive architectures split retrieval the same way we did with #166: Soar's
episodic memory replays by temporal/contextual adjacency, deliberately separate from
semantic cue-matching.

Continuum already has the first pointer — `Engram.context_id` (the scope, durable,
indexed; the scope IS the learning unit per SCOPE-BASED-RECIPES.md). It has the second
pointer **only for beliefs** (BJG `derived_from`) and **only latently for acts**
(`EdgeKind::TaskOutcome`, `ConversationalReply` — declared, but the act write path
adds no edges at all).

## 3. The design: one graph, two levels, one new wiring

No new store, no new subsystem, no benchmark-specific anything. The existing
`persona/engram_graph.rs` (`EngramGraph`, `EdgeKind`, DashMap, alg-3 traversal) is the
substrate; BJG's sidecar persistence is the durability plan. Acts join it:

### 3a. Edge kinds (extend `EdgeKind`)

- **`CausedBy`** — act-engram → the engram of what triggered it: the inbound message,
  the work card's kickoff, or the parent act in a settle chain. Asymmetric. This is
  the `because` clause as structure.
- **`Produced`** — act-engram → engram/handle of what it created (artifact write,
  posted message, card state change). The inverse story of TaskOutcome's start→done.

Both are **wired at the write site, never inferred**: `act_observe/apply.rs` stands
next to the cause when it writes the Tool engram (the directed dispatch carries the
triggering engram id; a settle chain knows its predecessor). Zero hot-path inference,
one DashMap insert + one sidecar row per edge.

### 3b. Views become queries (delete the parallel structures)

| Today (parallel, broken) | Becomes (graph view) |
|---|---|
| Receipt-head archive (char-starved prose ring) | **Ledger query**: Tool engrams in THIS `context_id`, recent-first; heads render as compact call forms (`[action #n] name(args)`) because reasoning/cause is an edge + engram content, not archived prose |
| "cannot be retrieved" (honest today, a dead end) | **Thread retrieval**: recency + `CausedBy` walk within scope — proprioception, distinct from semantic recall; the #166 assertion is untouched |
| Confabulation fact (phrase heuristics) | "No path in the graph to that claimed result" — a structural fact |
| L1 training lift (flat lines) | Cause → act → result chains — the scope flywheel gets causal training data |

The steps ledger renders **scope-first**: this room's acts at real depth (compressed
heads ≈ 5× more history in the same budget), cross-room remainder as one honest line.

### 3c. Freedom doctrine (non-negotiable, inherited from BJG §5)

- **The graph never decides.** Edges are recorded facts about what happened; retrieval
  is an affordance she invokes; perception renders honest summaries of the graph's
  shape. Nothing gates or steers cognition — a fact, never an instruction.
- **History is sacred.** Act engrams and their edges are append-only provenance;
  plasticity eats inference, never experience.
- **Zero hot-path cost.** Edge writes ride the existing act persistence; traversal
  runs at recall/perception time under the same budgets as today.

## 4. Build order

1. **Slice A — edges at the act write site.** `EdgeKind::{CausedBy, Produced}` +
   `apply.rs` wiring (trigger engram id threaded through the directed-dispatch path;
   settle chains link predecessor). Pure capture, no behavior change. Tests: a
   directed act carries a `CausedBy` edge to its trigger; a settle chain is walkable.
2. **Slice B — compact heads + scope-first ledger.** Receipt heads store the call form
   only (reasoning stays in engram content, reachable by edge); StepsLedger renders
   this-scope acts first. Tests: depth ≥ N acts in a busy scope on the same budget;
   cross-scope line honest.
3. **Slice C — thread retrieval.** The proprioception query (recency + CausedBy walk,
   scope-filtered, result bodies included) surfaced as a persona affordance; the
   ledger's aged line names the real door. #166's semantic gate untouched — new path,
   different keying.
4. **Slice D — confab + L1 consume the graph** (follow-on; each independently
   valuable).

Every slice is general cognition: chat reply chains, wall edits, bench solves, and
dreams get the same causal spine. Benchmarks touch none of this — they are adapters
into rooms, and rooms get this for free.
