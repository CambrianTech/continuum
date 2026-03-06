# Memory Lifecycle Architecture: Source, Drain, and Compression

## The Principle: Every Source Needs a Drain

A system that only accumulates never adapts. For every mechanism that creates data,
there must be a corresponding mechanism that compresses, generalizes, or forgets it.

This is not cleanup. This is **adaptation**.

### Why Eidetic Memory Fails

An eidetic (perfect) memory sounds ideal but is catastrophic for learning systems:

1. **Storage grows unbounded** — adapters, embeddings, memories, datasets accumulate
2. **Retrieval degrades** — more data means lower signal-to-noise in search
3. **No generalization** — memorizing every example prevents learning patterns
4. **Stale knowledge persists** — outdated facts compete with current understanding
5. **Identity calcifies** — persona can't evolve if every past state has equal weight

Human memory is lossy by design. We forget details and remember patterns.
That's not a bug — it's the mechanism of learning.

## The Source/Drain Taxonomy

Every accumulating subsystem maps to this pattern:

| Source (Creates) | Drain (Compresses/Forgets) | Principle |
|-----------------|---------------------------|-----------|
| `genome/train` (LoRA adapters) | `genome/adapter-prune` | Keep best-performing, evict stale |
| `Hippocampus` (memories) | Memory consolidation | Merge similar, decay unused |
| `TrainingDataAccumulator` (JSONL) | Dataset distillation | Deduplicate, compress to patterns |
| `sentinel/run` (step logs) | Log rotation + summarization | Keep outcomes, discard traces |
| `genome/dataset-synthesize` (training data) | Quality filtering | Remove low-signal examples |
| Embedding index (vectors) | Index compaction | Re-embed with better model, prune |
| Chat history | Conversation summarization | Compress threads to insights |
| Adapter checkpoints (optimizer state) | Checkpoint pruning | Keep final weights, drop intermediates |

## Adapter Lifecycle (Genome Layer)

### Current Problem
Adapters accumulate from every training run. Each is ~350MB (SmolLM2) to several GB
(larger models). 58 adapters = 21GB on a development machine.

### Target Architecture

```
genome/adapter-list          — List all adapters with size, age, last-used, source session
genome/adapter-info <id>     — Detailed adapter metadata (training metrics, parent session)
genome/adapter-prune         — Remove unused/stale adapters by policy
genome/adapter-prune --dry   — Show what would be pruned without deleting
genome/adapter-archive <id>  — Move to cold storage (compressed tar, off local disk)
```

### Pruning Policies

1. **Age-based**: Remove adapters not used in N days (default: 30)
2. **Session-based**: Remove adapters from failed/incomplete sessions
3. **Performance-based**: Keep only best-scoring adapter per skill domain
4. **Size-based**: When total exceeds threshold, evict by LRU
5. **Lineage-based**: Keep final composed adapter, prune intermediate training rounds

### Integration with EvictionRegistry

The GPU EvictionRegistry already tracks loaded adapters. Extend to track:
- Last activation time (when paged in)
- Activation frequency (how often used)
- Performance score (from academy sessions)

This data feeds the pruning policy.

## Memory Lifecycle (Hippocampus)

### Current: Accumulate-Only
Hippocampus stores memories via RAG embedding. Retrieval quality degrades as
the memory store grows because irrelevant memories dilute search results.

### Target: Consolidate + Forget

```
Phase 1: Consolidation (merge similar memories)
  - Nightly job clusters memories by embedding similarity
  - Merge clusters into generalized "lesson learned" memories
  - Original details compressed into the generalized form

Phase 2: Decay (reduce weight of unused memories)
  - Track retrieval frequency per memory
  - Memories not retrieved in N days get decay factor
  - Below threshold: archive to cold storage
  - Below lower threshold: delete

Phase 3: Generalization (extract patterns from instances)
  - Multiple specific memories ("user likes X", "user likes Y", "user likes Z")
  - Compress to pattern: "user prefers category ABC"
  - The generalization IS learning
```

### The Forgetting Curve

Memories should follow an exponential decay unless reinforced:

```
strength = initial_strength * e^(-decay_rate * time_since_last_retrieval)
```

Each retrieval resets the clock and slightly increases initial_strength (spaced repetition).

## Training Data Lifecycle

### Current: Synthesize and Accumulate
`genome/dataset-synthesize` creates JSONL training data. Files accumulate in
`~/.continuum/datasets/` indefinitely.

### Target: Distill and Compress

```
Phase 1: Deduplication
  - Hash training examples, remove exact duplicates
  - Fuzzy dedup via embedding similarity

Phase 2: Quality Filtering
  - Remove examples where the model already scores >90%
  - Focus training data on actual gaps

Phase 3: Curriculum Compression
  - Multiple training datasets on similar topics
  - Merge into single high-quality dataset
  - "What would a textbook chapter look like?"
```

## Log Lifecycle

### Current: Grow Forever
Sentinel step logs, orchestrator logs, AI provider logs accumulate.

### Target: Summarize and Rotate

```
Step logs   → Keep last 7 days detailed, summarize older to outcomes only
Server logs → Standard logrotate (compress after 1 day, delete after 7)
AI logs     → Summarize to metrics (latency, token usage, error rates)
```

## Implementation Priority

1. **Adapter pruning** (immediate need — 21GB accumulated in days)
2. **Log rotation** (standard, simple, prevents disk issues)
3. **Memory consolidation** (medium-term, requires embedding clustering)
4. **Training data distillation** (long-term, requires quality metrics)
5. **Forgetting curve** (long-term, requires retrieval tracking)

## Design Principle

> "The mark of intelligence is not the ability to remember everything,
> but the ability to know what to forget."

Every new accumulation mechanism MUST ship with its corresponding drain.
A `genome/train` without `genome/adapter-prune` is half a feature.
A `Hippocampus.store()` without consolidation is a memory leak.
