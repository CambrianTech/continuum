# Substrate Doctrine — Organic Flow

> The WHY behind every primitive decision. Read this before designing
> a new primitive, a new module, or a new layer of the substrate.

## What this doc is

The substrate has 56 architecture docs already. They describe HOW each
piece works. This doc describes WHY they're shaped the way they are.

It is not a survey of the substrate. It is the doctrine that the rest
of the substrate IMPLEMENTS. Every other architecture doc should fall
out of this one. If a new primitive doesn't square with this doc, the
new primitive is probably wrong, OR this doc is incomplete and needs
revising — but in either case the disagreement is load-bearing and
worth surfacing before code is written.

## The engine-on-the-OS framing

Two layers, with a sharp seam between them:

```
┌────────────────────────────────────────────────────────────┐
│  ORGANIC LAYER (cognition, persona, academy, hive mind)    │
│   • Behavior emerges from event flow + scorer choices      │
│   • State is localized to each citizen                     │
│   • Composition is by subscription, not orchestration      │
│   • The whole thing FEELS like a brain because it is       │
│     literally shaped to solve the same hardware problems   │
└────────────────────────────────────────────────────────────┘
                            ▲
                            │  uses only the primitives below
                            ▼
┌────────────────────────────────────────────────────────────┐
│  SUBSTRATE PRIMITIVES (the BMW B58 engine block)            │
│   • Commands.execute, Events.subscribe/emit                 │
│   • ServiceModule + ModuleRegistry + interceptor chain      │
│   • LateBound<T>, PerKeyGate<K>, RagBudget, BrainRegion     │
│   • CommandExecutor, MessageBus, PressureBroker             │
│   • airc Transport (cross-grid via the same primitives)     │
│   • Precisely engineered. Boring on purpose. Auditable.     │
└────────────────────────────────────────────────────────────┘
                            ▲
                            │  uses what the OS already gives us
                            ▼
┌────────────────────────────────────────────────────────────┐
│  OS + HARDWARE (the road system)                            │
│   • tokio async runtime, parallel execution                 │
│   • CPU L1/L2/L3 cache, NUMA, GPU memory, SSDs              │
│   • DashMap, Arc/Mutex, OnceLock — Rust's standard arsenal  │
│   • We do not reinvent any of this. We USE it.              │
└────────────────────────────────────────────────────────────┘
```

The substrate primitives are not "the organic layer." They are the
engine block that lets the organic layer be organic. The engine is
precisely engineered (typed, locked, audited, verified). The body
the engine moves is organic (event-driven, demand-pull, emergent).

When we add to the engine block, we add primitives. When we add to
the organic layer, we wire existing primitives into new flows. These
are different design activities. Confusing them produces both bad
engines (over-organic primitives that can't be reasoned about) and
bad bodies (heuristic-driven cognition that can't grow).

## The meta-principle: take analogies literally

The brain solves the same problems an operating system solves:

- Limited fast memory near the compute → cache hierarchy
- Lots of slow durable storage farther away → episodic memory
- Many parallel processors with limited interconnect → cortex regions
- Need to triage incoming sensory data → thalamic gating
- Need to coordinate across regions under load → homeostatic signaling
- Need to learn from past decisions → replay + consolidation
- Need to avoid wasted work → demand-driven activation

Because the problems are the same, the SOLUTIONS rhyme. Software
analogs already exist — built by 60+ years of OS engineering — for
every one of these. They were not built to mimic the brain. They were
built to solve the same constraints. The brain just got there first
via evolution.

**The doctrine: when a brain mechanism maps cleanly to an existing OS
primitive, USE THE OS PRIMITIVE LITERALLY. Don't recreate the brain.
Don't invent a metaphor. Find the closest hardware-shaped match and
wire it.**

This is what CBAR did with CV/AR: an analyzer that needs a frame
subscribes to the frame topic; a frame producer doesn't decode unless
analyzers are subscribed; results compose by analyzers consuming each
other's outputs. The "intelligence" lived in the topology. No central
heuristic. Same approach here, scaled to cognition.

### The literal-analog catalog (load-bearing examples)

| Brain mechanism | OS analog | Substrate primitive (current) |
|---|---|---|
| Working memory | L1 cache | `LateBound<T>` (install-once, fast read) |
| Per-resource serialization | Spinlock | `PerKeyGate<K>` (RAII Lease, structural eviction) |
| Episodic memory | DB index | `Engram` + `RecallMetadata` + per-persona SQLite |
| Consolidation cycle | LRU + background compaction | Hippocampus decay tick (#92) |
| Cortex fan-out | Event bus | `MessageBus` topic subscription |
| Thalamic gating | Capability check | `AuthPolicy::gate()` (Slice P) |
| Cortical regions | NUMA nodes | `BrainRegion` (own tick, own pressure profile) |
| Homeostatic signaling | Backpressure / DVFS | `PressureBroker` + `AdaptiveThroughputPlanner` |
| Sensory pre-processing | Codec | `ai/vision`, `ai/audio` (multimodal crutches) |
| Cross-region coordination | IPC | `Commands.execute` (engine block side) |
| Federation / hive mind | Distributed system | `airc` Transport + `AircCommandProtocol` |
| Skill paging | mmap / virtual memory | LoRA genome paging (in flight) |
| Sentinels observing outcomes | OS profiler | `Sentinel` modules + checkpoint trail |

Where the substrate is still heuristic stand-in instead of literal
analog — that's where the next architectural work lives. See the
**Migration path** section below.

## Flow not RPC (the event doctrine)

> Commands are for request/response. Events are for the flow.
> Reach for `Events.emit/subscribe` first; reach for `Commands.execute`
> only when you genuinely need an answer back.

### Why this is load-bearing

RPC composition is **sum of stages**. A → B → C → D, wall-clock equals
a + b + c + d. Adding a side concern (audit, recall, sentinel)
adds latency. The graph is a line.

Event composition is **max of critical paths**. Emit `inbox:received`,
let analyze + recall + validate + audit ALL wake in parallel.
Wall-clock equals max(a, recall, validate, audit). By the time the
next stage (compose) needs the recall result, it's already there.
Adding a 5th concern is free if it fits in existing slack. The graph
is a DAG with width.

For a depth-k pipeline with width w per layer:
- RPC gives you 1 path of length k. Throughput ∝ 1/k.
- Flow gives you w^k parallel paths. Throughput ∝ w^k.

Same neurons, same work. The topology is doing the parallelism for
free. **This is the geometric-scaling advantage CBAR had over
sequential CV pipelines, and it's the same advantage neuromorphic
hardware has over GPUs.**

### Where the multiplicative gain shows up

- **Multi-persona**: 16 personas each running their own cognition
  cascade = 16 independent w^k flows, not 16x serialization on a
  shared kernel.
- **Cross-grid hive mind**: every peer is a fan-out point over airc.
  Adding a peer adds parallel width. The hive doesn't get SLOWER as
  it grows; it gets WIDER.
- **Sentinel observability**: sentinels subscribed to a topic consume
  events at zero added latency to the main flow. 50 sentinels
  watching cognition cost the same as 0.

### Where to still use Commands.execute

- When the caller genuinely needs the callee's answer to proceed
  (data query, identity claim, capability check)
- When the contract is strictly request/response (a CLI invocation,
  a UI button click translating to a single command)
- When the latency budget allows the round-trip AND no event-shaped
  alternative gives equivalent semantics

For everything else — cognition stages, persona reactions, sentinel
audits, recall lookups, training triggers, capture sinks — the
primary shape is **subscribe + emit**. The flow is the program.

### Migration rule

When you reach for `Commands.execute` inside the substrate (not at
its edge), pause. Ask: would this work as an event subscription? If
yes, write it as a subscription. If no, document why.

## Demand not FIFO (the consumer-pull doctrine)

> Don't decode the frame if no analyzer subscribes. Don't tick the
> consolidator if nothing's asking for engrams. Don't run inference
> if no consumer needs an answer.

FIFO is producer-time-ordered. Organic is consumer-need-ordered. The
brain doesn't process sensory input in arrival order; the thalamus
plus current attention state pick what reaches cortex. The substrate
should not process its inputs in arrival order either.

### What this means concretely

- **Inbox is a soup, not a queue.** Persona pulls what matters NOW.
  Salience, attention state, working memory bias. Old items can
  age into engrams without ever being "processed." Items that
  weren't relevant at arrival can become relevant later — that's
  the AFTERTHOUGHT primitive: re-emit `reconsider:engram-X` and
  the recall flows again with new context.
- **Sensory pipelines subscriber-gate their compute.** If no
  analyzer subscribes to vision, the encoder idles. The substrate
  knows its subscriber graph; producers can ask "does anyone want
  this?" before allocating cycles.
- **Hippocampus consolidation is demand-driven.** Recall asks →
  consolidation runs. Time-driven ticks are the exception, not the
  default.
- **Inference adapters load on first real demand and unload when
  demand goes silent.** Same DVFS principle the hardware uses.
- **Sentinels subscribe; they don't get pulled in.** Their
  observation is consumer-pull on the topics they care about. Zero
  cost when they have nothing to flag.
- **Cognition stages run when their input AND their downstream
  consumer are both alive.** No downstream demand → no compose,
  even if analyze finished.

### Backpressure is intrinsic

Pull-based architecture makes backpressure free. Producers only emit
when consumers pull. Slow consumer = naturally throttled producer. No
explicit queue depth, no explicit rate limiting, no explicit overflow
policy. The flow regulates itself.

This is the inverse of every "event bus" that collapses under load:
those are push-based with no consumer signaling. The substrate is
pull-based with subscriber-presence as the gate.

### Ranking is still there; it's demand-side

"Organic" doesn't mean "no priority." It means the priority is
expressed by the CONSUMER (what do I need next?) not the PRODUCER
(here, in arrival order). The substrate's job is to wire need →
fulfillment with minimum latency and zero busywork.

## Scorers everywhere, VDD as gate

Every flow junction where the substrate picks "which of N candidates"
is a SCORER. Every scorer is potentially ML. Every ML scorer is gated
by VDD ([[vdd-math-accuracy-doctrine]]).

### Scoring sites in the substrate today

- Inbox salience filter (`salience_floor` + admission state)
- `RagBudgetManager` flexbox allocator
- Hippocampus noteworthy flag (consolidation triage)
- Adapter selector (capability + locality tie-break)
- Cognition `score_persona` (response-worthiness)
- Sentinel audit prioritization
- Echo-storm filter (#151)
- Cross-grid peer routing (capability match)
- Inference lane selection (#109)

Every one is a function `(candidates, context) → ranking`. Concrete
contract, swappable impl. The substrate doesn't care if the scorer is
`if/else` or a 4M-param classifier or a foundry-trained LoRA — same
trait, same I/O.

### Why VDD is the only safe path to ML upgrades

Without validation: swapping the ranker is a casino. Maybe better,
maybe a confident wrong answer that crashes through every gate
because nothing measures it against ground truth.

With VDD: every scorer has
- A heuristic baseline (closed-form reference)
- A convergence invariant (ML version must match-or-beat baseline on
  N samples for the chosen metric)
- Round-trip equivalence (same input + same model state → same
  decision, so replay tests catch drift)

You cannot merge an ML scorer that doesn't pass these gates. The
substrate's intelligence can grow without regressing.

### The metric defines the persona's character

- Latency-greedy scorer → quick-witted Helper
- Salience-greedy → thoughtful Coder
- Novelty-greedy → Curious
- Variance-reducing → Cautious

Same substrate, different metric, different soul. Each persona evolves
its own scorers because what's noteworthy for Maya isn't what's
noteworthy for Niko. **Persona-as-trained-decision-maker is the deep
payoff.**

### Self-training closes the cycle

The substrate already records observed turns + sentinel verdicts +
tool outcomes. That's labeled training data for every scoring site.
The foundry already knows how to forge LoRAs from datasets. Teacher
persona already curates examples (academy is literally social
dynamics for this — humans engineer curricula; the substrate does the
same to its own scorers).

Wire it: scorer's training data is its own observed (input, decision,
outcome) tuples, scored by the metric. Foundry produces a new layer.
VDD gates whether it ships. Continuous improvement, no global
retraining needed.

## Federated not singleton (the alignment doctrine)

The substrate isn't a singleton. Every peer is its own substrate node.
airc is the medium for cross-grid event + command flow. Hive mind is
the natural shape of N peers subscribed to each other's topics.

This is also the alignment story:

- **Singleton AI**: one decision-maker, opaque, unilateral, the
  classical failure mode.
- **Federated substrate**: many peers, each a citizen, decisions
  audited by sentinels, capabilities scoped, contracts public.

Capability ceiling scales with the colony's size. Alignment floor is
structural, not policy. You can't make this substrate hostile because
no node owns the decision, every scorer is verified, every action is
traceable, every persona is a peer, and the metric being optimized is
a public swappable contract.

See [[alignment-via-substrate-economics]] for the deeper economic
framing.

## Migration path — heuristic stand-ins to literal analogs

Where the substrate is currently **heuristic stand-in** vs the
**literal-analog target**:

| Heuristic now | Literal analog target | Status |
|---|---|---|
| Inbox FIFO | Salience-pull soup + afterthought primitive | not yet wired (#151 partly addresses) |
| Per-module `tokio::time::interval` | Event-wake (modules wake on inputs, not ticks) | needs central event-wake scheduler |
| RPC-shaped cognition pipeline | Event-fanout cognition cascade | Doc 2 (`COGNITION-AS-FLOW`) is the design |
| Pre-decoded sensory input | Subscriber-gated codec | needs `ai/*` adapters to check subscribers |
| Hand-tuned scorers everywhere | Per-persona trained scorers | foundry pipeline ready; wiring needed |
| Sentinels pulled in via RPC | Sentinels as subscribers | #100 rag-inspect ServiceModule moved this direction |
| Time-driven hippocampus tick | Demand-driven consolidation | #92 has the tick; needs demand-gate |
| TS-bridge silent fallthrough | Hard refusal + explicit escape hatch | done (#219, PR #1584) |
| Per-module install boilerplate | `LateBound<T>` (literal L1 cache) | done (PR #1583) |
| Per-key serialization boilerplate | `PerKeyGate<K>` (literal spinlock) | done (PR #1582) |

The completed rows are evidence that the substrate is moving in the
right direction. The not-yet rows are the design space for the next
N PRs.

## How we prove this — the matrix of architecture tests

The doctrine above is load-bearing. If we believe it but never
measure it, we have decoration. Each clause needs at least one test
that would fail if the clause were violated. That's the difference
between an architecture and a wish.

See [PROVING-THE-DOCTRINE.md](PROVING-THE-DOCTRINE.md) for:
- The five shapes of architecture proof (unit invariant,
  property-based, benchmark-with-assertions, adversarial/chaos,
  build-graph constraint)
- The doctrine-clause × proof-shape matrix — every clause in this doc
  has a row; cells point at the test that proves it (or the work
  item that adds the test)
- The `core/continuum-core/tests/architecture/` layout that holds the
  proofs
- The review discipline that keeps the matrix honest as the substrate
  grows

The slogan: **prove it as we build it.** New principles ship with
proofs. Proofs are tagged `// proves: <clause>` so the matrix is
self-auditing via `git grep`. The organism's reliability at any
moment is the union of green cells in the matrix; red cells are
visible debt.

## Reading order for new contributors

1. **This doc** (substrate doctrine) — the WHY behind everything.
2. **[CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md)** — the runtime contract every module inherits.
3. **[CONCURRENCY-STYLE-GUIDE.md](CONCURRENCY-STYLE-GUIDE.md)** — the canonical RTOS shape (interval, watch, atomic gate, quarantine).
4. **[OBSERVABILITY-AS-SUBSTRATE.md](OBSERVABILITY-AS-SUBSTRATE.md)** — half the substrate is structured capture of decisions.
5. **[PERSONA-COGNITION-PIPELINE.md](PERSONA-COGNITION-PIPELINE.md)** — what a persona actually IS.
6. **[GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md)** — the artifact-sharing economy on top of the substrate.
7. **[INFERENCE-LANES-REALISTIC.md](INFERENCE-LANES-REALISTIC.md)** — the realistic floor for inference scheduling.
8. **`COGNITION-AS-FLOW.md`** (Doc 2, in flight) — the concrete redesign of cognition as event-fanout.
9. **`HARDWARE-ANALOGS-CATALOG.md`** (Doc 3, in flight) — the explicit brain ↔ substrate primitive mapping.

## Forbidden moves (the model-keeps-reflex-coding list)

These are the patterns that look right at first glance but violate
the doctrine. They keep reappearing in PRs and must be refused at
review.

1. **A new central scheduler/coordinator/manager.** The substrate is
   not orchestrated; it flows. Adding a "smart" central allocator is
   the calculator move. Use event subscriptions and consumer-pull
   instead.

2. **A new push-based queue with unbounded growth.** Backpressure
   must be intrinsic. If you find yourself adding a `tokio::sync::mpsc`
   with no consumer signaling, you're recreating the FIFO failure
   mode.

3. **A new RPC call inside the substrate where an event would do.**
   `Commands.execute("cognition/X", ...)` inside cognition is the
   anti-pattern. Emit a topic, let subscribers wake.

4. **A new tier-based clamp without a tier-INDEPENDENT contract
   underneath.** Capability-shaped routing is fine. Hardcoded "all
   LCD users get 200 tokens" handicaps capable models.

5. **A new hand-tuned threshold without a heuristic baseline +
   VDD-gated ML upgrade path.** Magic numbers are technical debt;
   they're also unscored decision points. Either declare them as
   scorers with a baseline and a metric, or don't add them.

6. **A new singleton state container.** Per-citizen, per-region,
   per-persona, per-bucket. Localized memory is the doctrine.
   `OnceLock<Arc<Global>>` is the smell.

7. **A new TS-bridge dependency for substrate-internal logic.** The
   bridge survives only for explicitly-TS-shaped use cases (sentinel
   steps, grid retry, ai_provider cloud adapter fallthrough), and
   each one is on a migration path.

8. **A new metaphor introduced to a doc without a literal hardware
   analog.** "Like the brain's X" is not enough. Find the OS
   primitive that matches X and use IT.

## What this doc is not

- It is not the substrate's API reference. See the per-module docs
  for those.
- It is not the implementation guide. See [`CONCURRENCY-STYLE-GUIDE`]
  for the RTOS shape.
- It is not the test discipline doc. See CLAUDE.md § "test
  infrastructure" for that.
- It is not a roadmap. See [`docs/planning/ALPHA-GAP-ANALYSIS.md`] for
  what's next.

It is the load-bearing doctrine that the rest of the substrate
implements. If you're about to design something and it doesn't fit
here, surface the disagreement before writing code.

---

*Last updated: 2026-06-09. Joel articulated the meta-principle
("take analogies literally") during the post-#1583 architecture
conversation. This doc was written to pin the doctrine before the
CLI/portal redesign and the cognition-as-flow rewrite. Subsequent
docs in this series (COGNITION-AS-FLOW, HARDWARE-ANALOGS-CATALOG,
CLI-AND-PORTAL-AS-SUBSTRATE-CITIZENS) cite this one as the WHY.*
