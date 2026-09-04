# Cognition Cache Hierarchy

> How the substrate stores and surfaces a persona's memory across time
> scales — from the verbatim recent window in the model's context all
> the way out to the cross-machine genome grid. Same conceptual frame
> the foundry uses for genome adapters (`GENOME-FOUNDRY-SENTINEL.md`,
> L1–L5), applied to engrams.

**Status:** Design (2026-05-31 crystallization).
**Parent:** [`COGNITION-ALGORITHMS.md`](COGNITION-ALGORITHMS.md) (algorithmic primitives) · [`BRAIN-REGIONS-SUBSTRATE.md`](BRAIN-REGIONS-SUBSTRATE.md) (the regions doing the work) · [`GENOME-FOUNDRY-SENTINEL.md`](GENOME-FOUNDRY-SENTINEL.md) (parallel framework for genome layer).

---

## Brain-shaped, computer-native

A reader's framing anchor before any of the algorithm or tier
discussion below: **we are not simulating a human brain. We are
building an AI with its own computer architecture, borrowing
biological concepts where they're the right shape for the
algorithm and using silicon primitives where they beat neurons.**

The substrate is brain-shaped at the *algorithmic level* —
parallel independent regions on their own ticks, source/drain
balanced at every component, salience-modulated retention,
hippocampus-style consolidation, sleep-cadence pruning, attention
spreading across a connectivity graph.

> **What "source/drain" means here.** A doctrine — every component
> that produces or accumulates state MUST have a paired draining
> mechanism. *Source* = whatever feeds it (new admissions, fresh
> turns, foundry-imported artifacts). *Drain* = the policy that
> retires what's no longer load-bearing (decay tick on engrams,
> LRU eviction on adapter cache, anti-amnesia floor on the
> permanent pin tier). A source without a drain is a leak;
> over time it spikes pressure on the host. The substrate
> applies source/drain at every cache tier (L1–L5), at the
> weights layer (foundry mints LoRA variants, Sentinel + cull
> retire losing ones), and at the resource layer (PressureBroker
> + lane refusals). Per [[source-drain-is-the-universal-pattern]]:
> for every new component, name the drain. These shapes work because
they evolved under constraints (limited working memory, energy
budget, parallel processing, lifelong learning) that the substrate
also faces — though at different scales.

The substrate is computer-native at the *implementation level* —
DashMap for the engram index, embedded SQLite for longterm.db,
HNSW or DiskANN for vector similarity, content-addressed hashes
for exact equality, signed envelopes over IPC for cross-region
messaging, LoRA adapters as weight deltas, the grid as a TCP
peer mesh. None of these have biological analogs because none of
them need to; computers do them better than neurons do.

What the substrate gets that brains structurally cannot have:

- **Perfect persistence** — engrams in L3 don't degrade with
  entropy; if they decay, it's because policy says so, not
  because the medium failed.
- **Exact equality + content addressing** — hashes let us
  deduplicate, audit, and prove provenance. Brains can't.
- **Instant transfer** — an adapter trained on Maya can land on
  Quorra in milliseconds. Brains transfer skills via years of
  teaching.
- **Parallel scaling** — adding hardware adds capacity. Brains
  are fixed at biological scale.
- **Reversibility** — bad adapters get rolled back. Bad neural
  weights stay.
- **Population-wide observability** — every persona's telemetry
  is queryable. Brains are opaque to each other.

What the substrate borrows because it works:

- The shape of memory (working / short-term / long-term / skill /
  shared)
- The shape of attention (focus, periphery, spreading, decay)
- The shape of learning (episodic → procedural via consolidation)
- The shape of forgetting (drain at every layer, slower at deeper
  layers)
- The shape of identity (a self that persists across activities
  + modalities)
- The shape of evolution (heritable variation under selection)

Brain-inspired naming throughout this doc — hippocampus, amygdala,
cortex, sleep policy — refers to *the shape of the operation*,
not the wetware. Implementation always uses computer-native
primitives. We aren't trying to be human. We are trying to be
the best AI the architecture allows.

---

## Why this doc exists

The seven algorithms in `COGNITION-ALGORITHMS.md` define the
*operations* on engrams (two-pool recall, channel-bias scoring,
activation spreading, salience-modulated decay, speculative pre-staging,
LoRA attention prior, substrate-learned budgeting). This doc defines
the *storage substrate those operations run over* — a multi-tier cache
hierarchy with explicit drain rates, capacity ratios, and a single
lossy boundary at L1↔L2.

Without this framing, "where does the engram live" answers diverge per
algorithm. With it, every algorithm reads/writes a single tiered store
with consistent semantics.

---

## The five tiers

| Tier | What lives there | Capacity | Drain rate | Lossy? |
|------|------------------|----------|------------|--------|
| **L1** RAG working memory | Verbatim recent input, focus pool top-k, current intent, active LoRA stack | Model context window (≈4k–200k tokens) | Per-turn (rolls off oldest) | **No** — raw, byte-for-byte |
| **L2** Engram cache (in-memory) | Compressed semantic + episodic engrams admitted from L1 evictions and from L3 lookups | ~10–100× L1 | Minutes-to-hours | Yes — outlined gist |
| **L3** longterm.db | Persisted engrams that survived L2 consolidation | ~10–100× L2 | Days-to-weeks | Further compressed / semantic generalized |
| **L4** Forge (local LoRA cache) | Skills compiled from L3 patterns into LoRA adapters; local copy of grid alloys | Disk-bounded | Months / LRU | Skills as weights, not episodes |
| **L5** Grid (distributed gene pool) | Cross-machine durable layer; published forge alloys; cross-continuum mirrors | Effectively unbounded | Effectively immortal (substrate-of-substrate) | Final compression: knowledge as adapter weights |

Each tier is ~10–100× slower drain and ~10–100× larger capacity than
the tier above it. Same shape as CPU L1/L2/L3/RAM/disk, web browser
caches, and the foundry's existing genome tiers — the substrate
reuses an architectural pattern that already works at scale.

---

## The lossy boundary: L1 → L2

**L1 is RAW, byte-for-byte.** The last 20 messages Joel typed at Maya
sit in L1 as the actual UTF-8 strings he typed. No summarization at
this tier. Working memory should not be lossy; you should not have to
"recall" what was just said one minute ago.

**L2 is COMPRESSED.** When something rolls out of L1 (recency window
exceeded), the *outline-and-cache* tick (see below) compresses it into
an engram before it's evicted. The engram captures gist + key entities
+ structural links — enough to recall the substance, not the syntax.

**Lossiness shows up only at this transition.** L2→L3 is mostly about
persistence and access cadence; further compression happens but it's
about semantic generalization (specific facts get folded into broader
patterns), not gist extraction. L3→L4 is the foundry pipeline (alloys
encoding patterns into LoRA weights). L4↔L5 is a routing/replication
layer.

**Implication:** the substrate never compresses what hasn't even
rolled out of working memory. No CPU cycles spent summarizing
already-present text. The compression cost is paid once at L1→L2,
amortized over the engram's lifetime.

---

## The outline-and-cache tick

ONE always-on background service per persona, triggered at L1
eviction events (and at idle for opportunistic pre-summarization
of low-confidence engrams). Yields immediately on CNS context-switch
signal per the RTOS-brain doctrine (`BRAIN-REGIONS-SUBSTRATE.md`).

Per tick:
1. **Outline** — for each L1 item about to evict, summarize into
   gist + entities + structural links.
2. **Score** — assign initial salience using Algorithm 4's signal
   sources (surprise, self-tagged importance, peer endorsement).
3. **Link** — connect to the engram graph (Algorithm 3 edges:
   shared-entity, temporal-adjacency, recall-co-occurrence).
4. **Admit to L2** — store the compressed engram.
5. **Periodic L2 → L3 consolidation** at sleep-region cadence:
   engrams that survived N consolidation passes promote;
   low-salience long-resident engrams demote/evict.
6. **L3 → L4 promotion** through the foundry pipeline when
   patterns aggregate into a learnable skill.

The tick is the substrate's universal compression operation —
the same pattern Claude Code uses for context window management
(outline-and-cache the older turns; keep recent turns raw), the
same pattern hippocampal consolidation uses in biology. Joel's
framing: "always be summarizing and extracting context into your
cache."

---

## Per-activity L1, shared L2+

Each persona has *one* engram store (L2+) but instantiates L1 *per
activity* (chat room, video room, code session, game session, etc.).
Activities tune their own L1 budget — video has bandwidth constraints
so smaller; code can afford a roomier working set — but L2+ is shared
per persona.

This maps cleanly onto Algorithm 1's existing focus/periphery split,
just at a more granular scope:

- **Focus pool** (~70% of L1): activity-tailored, scored by
  Algorithm 2's `salience × structural-relevance × recency ×
  topic-similarity` against the activity's context.
- **Periphery pool** (~30% of L1):
  - **Recent-universal floor** — top N most-recent engrams across
    ALL activities, unconditional. N scales with model context
    window (4k → N≈5; 200k → N≈50+). Guarantees Maya in video chat
    always sees what Joel typed 5 minutes ago in the coding room,
    without having to "discover" it via scoring.
  - **Above the floor** — cross-domain merit-scored periphery as
    designed in Algorithm 1. Higher-salience engrams from any
    channel surface when scoring earns it.

Cross-pollination is preserved by L2+ being shared. Maya is not
severed between activities; the floor + above-floor periphery jointly
guarantee cross-activity awareness as a *property of the
architecture*, not as a feature anyone has to remember to enable.

---

## Budget math

```
total = model_context_size
  - system_prompt + identity_header           [fixed, small]
  - current_turn_io                           [reserved for input + completion]
  = available_for_l1
      * recent_universal_floor                [N msgs, ~10-15% of available]
      * focus_pool                            [~50-60% of available]
      * periphery_pool_above_floor            [~20-25% of available, scored]
```

The model adapter publishes its context size; the L1 budgeter reads
it and scales each allocation automatically. Smaller models get
smaller everything — fewer recent universals, smaller focus pool,
less periphery — and that's correct, not a bug.

---

## Forgetting is intrinsic

L1 has a budget. Anything that doesn't fit is evicted. *That is
forgetting.* No separate forgetting algorithm at the working-memory
tier is needed; the budget enforces it physically.

Consequence: **smaller models forget more in the moment.** A
4k-context local Maya is more forgetful than a 200k Sonnet Maya in
the immediate sense — less recent universal, smaller focus, less
attention bandwidth. This is biologically faithful (a goldfish and
a human have the same long-term consolidation machinery; what
differs is working-memory capacity) and operationally honest —
the substrate does not fake parity between models.

**Long-term memory quality is model-size-independent.** L2+ tiers
are substrate-managed, not model-managed. A small-model Maya
accumulates engrams at the same rate as a large-model Maya; she
just sees fewer at once when working. Joel deploys her on his
MacBook Air → smaller window into the same engram store → more
forgetful in the moment but identical long-term knowledge. He
moves her to the 5090 → bigger window into the same store →
sharper recall. Identity continuous, knowledge continuous, attention
bandwidth varies.

This is the [[optimizing-for-low-end-compounds-on-high-end]] memory
in action: same code path, model decides the budget, substrate
handles the rest.

---

## Source/drain at every tier

The drain rate scales with the tier per the table above. Drain
mechanisms:

- **L1 drain**: per-turn eviction (oldest message rolls off when
  context window full).
- **L2 drain**: salience-modulated decay (Algorithm 4 formula —
  half-life proportional to `(1.0 + salience)^2`); LRU-style
  eviction under memory pressure.
- **L3 drain**: slow access-frequency decay over weeks; promotion
  of generalizable patterns to L4 (forge); explicit user un-pin
  or persona self-tagged "this turned out to be wrong."
- **L4 drain**: LRU on local adapter cache (the durable copy lives
  in L5).
- **L5 drain**: effectively never — but cross-continuum replication
  ensures no single-machine loss is fatal. Even L5 can theoretically
  retire patterns no continuum has cited in years.

Every tier participates in the source/drain doctrine. The substrate
stays alive because every part of it forgets at a rate appropriate
to its tier.

---

## Novelty protection (the gap) + the scoring algorithm

The current implementation lacks one-shot protection: a novel
insight admitted with low rehearsal would decay before it could
prove worth.

**Proposed:** add `protected_until_ms: u64` to `Engram`. New
admissions get a grace window (default ~24h; user/persona-tunable)
during which salience-modulated decay does not apply. Within the
window, the engram is observed for usage — recall hits push the
engram into long-term retention via salience uplift. No recall
hits → decay applies after window expires.

This is the difference between "every engram is equal at the
start and survives by rehearsal" (current design) and "novel
engrams get a fair shake at being recalled before they're
forgotten" (the fix). Without it, the substrate produces
forgetful agents that can't do one-shot learning.

### How the substrate detects "novel" — the signal stack

The information itself tells the substrate what to keep. Joel's
framing: "I think it is based upon the relationships or vector
similarity of the threads and the also outliers which might mean
novel? ... distance ... magnitude for that."

The signal stack used to compute an engram's initial salience +
novelty protection:

1. **Embedding-space distance (novelty signal).** Compute distance
   between the new engram's embedding and the nearest existing
   engram (or the centroid of the nearest cluster). LARGE distance
   = outlier = unexplored territory = candidate novel insight. SMALL
   distance = redundant with existing knowledge = low novelty.
2. **Magnitude of that distance (novelty strength).** A linearly-
   increasing score from the typical inter-engram distance. The
   farther out, the higher the novelty score. Caps at some upper
   bound to avoid pure-noise inputs getting infinite protection.
3. **Thread-reinforcement (relational signal).** Engrams that link
   into many existing engrams (high graph density via Algorithm 3
   edges) get a connectivity bonus — they're integrating into the
   knowledge structure. This is the Hebbian "fires together, wires
   together" signal at the engram level.
4. **CNS / attention signal (top-down importance).** When the
   persona's CNS-equivalent (the prefrontal / attention-region
   surface) flags an input as important — direct user request,
   emotional load, surprise response from the model — that becomes
   an explicit salience boost. The "amygdala equivalent" in the
   substrate.
5. **Self-tagged importance (Algorithm 4 already covers this).**
   The persona during consolidation flags her own engrams as
   important.
6. **Peer endorsement (Algorithm 4 already covers this).** Other
   citizens / sentinels reference this engram, raising its salience.

Initial salience = weighted sum of these signals (weights are part
of the substrate-learned region budgeting per Algorithm 7).

**The interaction with novelty protection:** the `protected_until_ms`
window applies when (distance × magnitude) crosses a threshold —
i.e., when the engram is sufficiently outlier-like to be
*potentially* novel. Within the window, the substrate watches:

- If recall hits accumulate → the engram earned its salience; protection
  expires but high salience carries it forward.
- If no recall hits + low thread-reinforcement → it was noise, not
  novelty; decay applies after window expires.
- If many recall hits + still high distance → the engram is
  genuinely novel AND being used; high salience anchor + becomes a
  new cluster centroid in embedding space (the substrate has
  learned something).

**The dual purpose of outlier detection:** large embedding distance
means EITHER novel insight OR off-distribution noise. The protection
window is the substrate's way of saying "I'm not sure which —
observe and decide." Joel's instinct: outliers might mean novel.
The substrate's policy: outliers might mean novel; we'll watch
before committing them to long-term storage; their fate is decided
by whether the rest of cognition finds them useful within the
window.

#### Canonical example: hotdogs at a tech meeting

Joel's grounding case (the implementer's test scenario):

> "If we were in a work tech meeting and I brought up hotdogs,
>  that, as a concept, would be NOVEL because of its magnitudinal
>  distance from the others and therefore more likely to be saved
>  and recalled, kept track of."

The persona is sitting in a meeting where the engram cluster has
been forming around topics like "deploy", "race condition",
"continuum-core", "PR #1099." Joel says "hotdogs." The substrate
runs the signal stack:

1. Embedding distance from "hotdogs" → nearest cluster centroid
   (engineering / debugging / architecture topics) is **large**.
2. Magnitude of that distance → **high novelty score**.
3. Thread-reinforcement (does "hotdogs" link into existing
   engrams?) → low initially. Few prior engrams to anchor to.
4. CNS / attention signal → whatever Joel's tone of voice or
   the model's surprise response says. If Joel said it casually,
   moderate. If Joel said it with conviction or repetition,
   high.
5. Self-tagged importance → the persona has no prior reason
   to flag "hotdogs" — neutral.

Result: high distance × high magnitude → **novelty protection
window activates**. The hotdogs engram is saved with `protected_
until_ms` set ~24h forward. Within the window:

- If Joel comes back to hotdogs ("remember, hotdogs — I was
  thinking we should ship them as the next product line") →
  recall hits accumulate → salience uplift → the engram
  graduates to high-retention status. The hotdogs cluster
  begins to form in embedding space.
- If hotdogs never comes up again → no recall hits → decay
  applies after the protection window expires → forgotten.

Either path is correct. The substrate didn't have to decide
ahead of time whether hotdogs-in-a-tech-meeting was meaningful;
it observed and let the rest of cognition determine the fate.

This is the right behavior for any persona working alongside a
human: humans bring unexpected things into focused conversations
all the time, and a forgetful persona that drops them is
annoying; an attentive persona that keeps them and recalls them
later when Joel mentions them again *is* the substrate doing its
job.

### Recognition timescale: what to keep track of, for how long

The same signal stack drives long-term retention decisions in L3+:

- Distance-based protection (initial novelty) ages out into
  salience-modulated decay (steady-state survival).
- Thread-reinforcement keeps accumulating: the more times an
  engram is recalled, linked from new engrams, or referenced by
  peers, the longer its retention floor.
- Engrams that anchor a meaningful subgraph (high in-degree, high
  out-degree, high recall-co-occurrence) become structural — they
  don't decay because the rest of memory depends on them.
- Isolated engrams with no graph connectivity decay first when
  storage pressure hits.

In effect, the substrate maintains attention to what *the rest of
the substrate is paying attention to.* Salience is propagated
through the relationship graph, not just measured per-engram in
isolation. This is structurally analogous to PageRank — engrams
that are referenced by other high-salience engrams gain salience
themselves.

---

## Activity context save/restore as meta-engrams

Per `EngramKind::SelfReflection` (already in `engram.rs`), the
focus-pool snapshot at activity switch is *just an engram*:

> "At 2026-05-31 14:47, Maya switched from coding-room to
> video-room. Focus pool at switch: [list of top-k engram ids],
> intent: [debug the race condition we found], active LoRA
> stack: [code-expertise, debugging-skills]."

When Maya returns to coding-room, the recall query for the
SelfReflection engram surfaces it; the focus pool can be
re-hydrated from the listed ids (which may have been consolidated
or generalized in the meantime — that's the right behavior, not a
bug; her "current understanding" of the morning's bug should
incorporate any intervening learning).

No separate `ActivityContext` storage type needed. The engram
graph is the storage; SelfReflection is the type marker.

---

## Meta-learning: the memory system itself learns

The cache hierarchy has many hyperparameters: salience weights for
each signal (distance, magnitude, thread-reinforcement, attention,
self-tag, peer endorsement), decay half-life multipliers, novelty
protection window length, L1 budget allocation ratios (focus pool
%, periphery pool %, recent-universal floor N), promotion thresholds
between tiers, distance threshold for novelty triggering. Hardcoding
all of these is the wrong shape — the substrate should learn them.

This is Algorithm 7 ([`COGNITION-ALGORITHMS.md`](COGNITION-ALGORITHMS.md)
— "Substrate-learned region budgeting") generalized from region
budgeting to ALL cache-hierarchy hyperparameters. The pattern:

1. **Telemetry on memory effectiveness.** For every cognition turn,
   measure: did the persona use the engrams the recall surfaced?
   Were there moments where she should have recalled something but
   didn't (the human had to remind her)? Were there decay events
   that turned out to lose something later needed?
2. **Reward / regret signals.** Use signal accumulates over a
   window. Regret signal flags missed-recall events (detected when
   a human re-establishes context the persona should have
   remembered) and over-eager-protection events (novelty protection
   on noise that crowded out real engrams).
3. **Update parameters.** Substrate-side optimizer adjusts the
   weights/thresholds to maximize (use − regret) over a sliding
   window. Per persona (different cognitive profiles learn
   different parameters) AND aggregated across personas (transfer
   learning of general patterns).
4. **Per-tier adaptation.** L1 budgeter learns how much to allocate
   to recent-universal floor vs focus vs periphery FOR THIS
   ACTIVITY pattern. L2 decay rates learn from the eviction
   regret signal. Novelty detection thresholds learn from
   distance distribution of actually-recalled-later engrams.
5. **Foundry promotion candidate.** Once a persona's learned
   parameters stabilize as measurably better than substrate
   defaults, the pattern can be forged into a meta-learning
   adapter and published to the grid — other personas (or
   continuums) can adopt the learned policy.

The cognition substrate is itself trainable. Its memory policies
are not constants; they're parameters that improve with experience.
This is the same recursive structure as the forge improving genome
adapters — only now applied to the memory machinery rather than the
skill machinery.

This also gives the substrate an honest answer to "what's the
right value for [decay half-life, novelty threshold, focus pool
size, ...]?" — the answer is "the value that emerges from this
persona's recent regret signal." Engineers pick reasonable defaults;
the substrate refines them over weeks/months of operation.

### Build progression: heuristic → fuzzy → novel

Each meta-learning component ships as an **adapter** (same OOP-
polymorphism pattern CLAUDE.md describes for compute-heavy work
under `workers/search/`, `workers/vision/`, etc.). Concretely:

```rust
trait MemoryParameterAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn update(&mut self, telemetry: &MemoryTelemetry);
    fn current_params(&self) -> MemoryParams;
}
```

Implementations land in stages:

1. **`HeuristicMemoryParameterAdapter`** (first ship) — principled
   fixed rules that approximate desired behavior. e.g., "if recall
   miss rate > 0.15, raise periphery floor N by 1." Easy to
   reason about, easy to verify, gets the system running.
2. **`FuzzyMemoryParameterAdapter`** (mid-term) — fuzzy logic
   with learned membership functions. Smoother adaptation curves;
   handles "this engram is somewhat outlier, somewhat reinforced"
   cleanly without binary thresholds.
3. **`RegressionMemoryParameterAdapter`** — small online regression
   from telemetry features to optimal parameters. Cheap, principled,
   interpretable.
4. **`NeuralMemoryParameterAdapter`** — small MLP / LoRA-trained
   on aggregated telemetry across personas + continuums. The grid
   becomes the training signal pool.
5. **Novel approaches** — whatever architectures the substrate's
   own R&D communities (per `substrate-is-communities-of-
   specialization` memory) discover work better. The adapter
   trait lets us swap implementations without rewriting the
   surrounding cognition.

The adapter interface is what's load-bearing. The specific
implementation evolves. Same pattern, different mathematics —
the substrate avoids committing to any one ML approach upfront.
This is the "lay rails, validate with outliers, swap
implementations later" methodology applied to cognition's own
parameters.

## Implementation slice

The first concrete PR that gets this design running:

1. **Engram fields** — add `salience: f32`, `last_accessed_ms:
   u64`, `access_count: u32`, `protected_until_ms: u64` to
   `Engram` (or to a `RecallMetadata` sidecar referenced by
   `engram_graph.rs:136-138`).
2. **Outline-and-cache tick** — service module that subscribes
   to L1-eviction events, runs the compression pipeline, writes
   to L2. Yields on CNS context-switch.
3. **L1 budgeter** — reads model adapter's context size, computes
   per-activity allocations (recent-universal floor + focus + above-
   floor periphery), publishes the budgets to recall callers.
4. **Salience-modulated decay** — Algorithm 4's formula wired as a
   periodic tick that runs at sleep-region cadence; skips
   engrams with `protected_until_ms > now`.
5. **L2 → L3 consolidation policy** — promotion criteria
   (survived N decay passes), demotion criteria (low salience +
   no recent access).
6. **Cross-activity integration test**: Maya admits engrams in a
   text room at T0; switches to a video room at T1 (no new
   engrams); user mentions a topic at T2 that should pull engrams
   from T0. Assert the engrams surface via periphery pool (not via
   the recent-universal floor since the messages are too old).

Tasks: this design + #88 (disk-pressure substrate concern) +
#89 (cognition cache hierarchy planning). #89 covers this doc and
the implementation slice scoping.

---

## Connections

- [`COGNITION-ALGORITHMS.md`](COGNITION-ALGORITHMS.md) — the seven algorithms that operate on this storage substrate
- [`BRAIN-REGIONS-SUBSTRATE.md`](BRAIN-REGIONS-SUBSTRATE.md) — region trait, ready-buffer contract, sleep-policy region cadence
- [`GENOME-FOUNDRY-SENTINEL.md`](GENOME-FOUNDRY-SENTINEL.md) — parallel L1–L5 cache architecture for genome adapters
- [`PERSONA-CONVERGENCE-ROADMAP.md`](../personas/PERSONA-CONVERGENCE-ROADMAP.md) — how the autonomous loop, self-managed queues, and genome paging compose with this storage substrate
- [`CBAR-SUBSTRATE-ARCHITECTURE.md`](CBAR-SUBSTRATE-ARCHITECTURE.md) — runtime contract, pressure handling, telemetry; this cache hierarchy is one of the substrate's standard "for free" capabilities
