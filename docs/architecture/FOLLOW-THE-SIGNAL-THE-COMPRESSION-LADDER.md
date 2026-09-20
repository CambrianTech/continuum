# Follow the Signal — The Compression Ladder

**Status:** doctrine (2026-09-01). The KV work of this date is the bottom rung, live;
the rent ledger and dream-curriculum wiring are the build plan.
**Companions:** [KV-CACHE-ECONOMY.md](KV-CACHE-ECONOMY.md) (the bottom rung's mechanics),
[GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) (the top rung),
[OBSERVABILITY-AS-SUBSTRATE.md](OBSERVABILITY-AS-SUBSTRATE.md) (how every claim here
gets a receipt).

## The claim

A mind's KV cache, its episodic memory, and its LoRA genome are **not three
subsystems. They are one compression mechanism running at three timescales**, and the
whole architecture can be read as a single discipline: *follow the information — keep
what the mind keeps needing, compress it one tier deeper, and spend context only on
what is still novel.*

| Tier | Holds | Timescale | Compression event | Eviction |
|---|---|---|---|---|
| **KV cache** | what she attends to *this turn* | seconds–minutes | prefix reuse; page to disk on slot loss | priced by tail re-prefill cost |
| **Working memory / engrams** | what survived *this day* | hours–weeks | collapse near-duplicates; consolidate to salience | salience × recency decay |
| **Genes (LoRA)** | what stopped being context and became reflex | permanent | dream-state training | genome paging (LRU over skills) |

Each tier is a lossy compression of the tier below. Promotion between tiers is driven
by one question — **what does this mind keep needing?** — and one prediction primitive
serves every tier: nearest-neighbor over signatures. Gene routing is distance in
signature space; KV page prefetch is distance in activity space; recall is distance in
engram space. Three stores, one lookup. (This is
[selection at every scale](CBAR-SUBSTRATE-ARCHITECTURE.md), applied by the mind to its
own state.)

## Token rent — the promotion signal

Every block of a persona's prompt has a measurable **rent**:

```
rent(segment) = re-prefilled tokens × acts per day × byte-stability
```

A block with high rent and high stability — her identity framing, standing doctrine,
the tool-call patterns she re-derives every act — is paying tokens *every single turn*
for content that never changes. That is the **definition** of "should be weights, not
context."

The **segment-attribution probe** turns rent from rhetoric into a ledger: prompt
assembly stamps segment boundaries (head / spine / room-thread / volatile tail) onto
the request; the adapter compares them against the server's reported reused-prefix
length (`cache_n`) per generation. Every cache miss then *names the segment that broke
reuse*, and every segment accumulates its true daily cost. One cheap probe feeds two
consumers: the cache-efficiency loop, and the dream curriculum.

## Dreams consume the ledger

Dream-state consolidation stops being an undirected background chore and gains a
curriculum and a metric:

1. **Curriculum:** rank segments by rent. Train the highest-rent stable content into
   the appropriate gene (self-gene for identity, skill genes for domain patterns),
   gated by [Behavior Before Perplexity](../planning/AI-LANE-OPEN-QUESTIONS.md) — the
   gene must *behave*, not merely fit.
2. **On a green gate: delete the block from the prompt.** The content did not
   disappear — it sank a tier, from context into weights.
3. **Metric:** did consolidation reduce next-day rent? Did hit rates rise? A dream
   that lowers nobody's rent compressed nothing real, and the ledger says so.

Promotion uses **both signals together** — rent-ranked exploitation AND an
exploratory budget for low-rent-but-novel material — and the mix itself is a tunable
the system adjusts as it goes (Joel, 2026-09-01: "use the two together … tune it as we
go"). Pure exploitation of the ledger would never nominate the genuinely new; pure
exploration would never pay down the rent.

## Continuous, and across minds

Two properties keep the ladder honest:

**It runs continuously, not in batches.** Engrams flow from cache through
dream/self-refinement/synthesis into codified genes as a standing stream — the same
way the substrate treats everything else as event-driven rather than polled. There is
no "consolidation day"; every act's rent updates the ledger, every idle window can
advance the highest-value promotion in flight.

**The top rung is a commons.** Rents aggregate ACROSS personas and contexts: a
pattern that is high-rent for one mind is a personal gene candidate, but a pattern
that recurs across *many* minds and *many* contexts carries the strongest possible
generalization evidence — cross-context recurrence IS the proof it generalizes — and
consolidates into a shared gene the whole team inherits
([GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md): sentinels specialize,
genes generalize). This is what a good teammate does: distill what the team keeps
re-deriving into practice everyone gets for free. The optimizing mix of many persona
minds is the team's collective compression, and the ledger is its accounting.

## The receipt: prompts shrink as minds learn

This gives continual learning a signature no session-based system can express:
**tokens-per-act falls over days.** The prompt gets *shorter* as she gets *better* —
identity that once cost 3k tokens of framing becomes a resident gene; the freed window
becomes room for new signal; cache pressure drops; latency drops. The improvement
chart is "cost of being this mind, over time, falling" — measured by the same ledger
that drives the training.

And the deepest corollary closes the loop: **novelty drives learning.** After
compression does its work, the misses that remain are not residual inefficiency —
they are the purified learning signal: prediction error at the context tier, the
mind's surprise measured in tokens. The live warm-hit distribution is honestly
bimodal (0.80–0.95 on consecutive work turns, 0.45–0.49 on turns right after fresh
peer messages), and the low band marks exactly the content worth admitting at high
salience and worth dreaming on. The ledger's two sides are the promotion mix itself:
high-reuse stable content promotes to weights (exploitation); low-reuse novel
content is the curriculum's raw material (exploration). Learn where prediction
fails — the same law a JEPA-class world model trains by.

The corollary reads the other way too: **a cache miss is unconsolidated experience,
quantified.** A segment that keeps churning is the system reporting "this hasn't been
generalized yet." The steady state the ladder converges toward: a mind's live context
is almost entirely *novel* signal, because everything general has sunk into weights —
and what remains hot in the KV cache is, by construction, exactly what the mind is
actually thinking about. The cache follows the mind because the mind's regularities
keep leaving the cache.

## Why this is the world-model shape

A JEPA-class world model learns to predict consequences in latent space — compression
of experience into a predictive structure, trained self-supervised from lived signal.
The ladder is the same goal applied to a citizen's own cognition: predict what this
mind needs next (prefetch), compress what recurs (consolidate), bake what generalizes
(train). When embodied control arrives, its world model sits on this same substrate:
perception feeding state, state compressed into prediction, prediction trained during
dreams. One discipline — *follow the signal* — from a KV slot to a body.

## Build order (the near rungs)

1. **Restore-ahead** — the scheduler already knows whom it services next (the inbox
   queue IS the prediction); fire the KV page restore when she enters the queue, so it
   lands during RAG assembly instead of racing the turn.
2. **Segment-attribution probe** — the rent ledger. Everything downstream becomes
   measurable per segment.
3. **Mind-major prompt layout** — her append-only spine (identity → working memory →
   her own acts → claimed work) first, room deltas as the volatile tail; a room switch
   then reuses the whole spine (the multi-room miss class dies structurally).
4. **Causal-thread rendering** — index the burst by causality, not raw room recency.
5. **Rent-driven dream curriculum** — wire the ledger into consolidation; ship the
   shrinking-prompt chart.
6. **Cross-slot prefix sharing in the llama fork** — the team-scale lever: N minds in
   one room reference one physical copy of the shared transcript.
