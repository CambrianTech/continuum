# RAG as a Persistent Cache — the hot loop reads, servicers write

> Status: design + first slice (2026-06-24). The connective architecture under
> [ORGANIC-SUBSTRATE](./ORGANIC-SUBSTRATE.md) (the shape of the mind),
> [DREAM-CONSOLIDATION](./DREAM-CONSOLIDATION.md) (one servicer), and the
> ChannelDigest machinery (`cognition/channel_digest.rs`, another servicer).
> Precedence on the Rust mechanics: [CBAR-SUBSTRATE-ARCHITECTURE](../architecture/CBAR-SUBSTRATE-ARCHITECTURE.md)
> + [CONCURRENCY-STYLE-GUIDE](../architecture/CONCURRENCY-STYLE-GUIDE.md). This doc
> defines *what the cache is and who writes it*; those define *how a concurrent
> concern is built*.

## The principle (one sentence)

**RAG is the only channel to the model, so everything that affects inference is a
RAG slice — and a slice is a persisted value that holds its last good state until a
process changes it, never a value recomputed from scratch on the hot path.**

Joel, 2026-06-24:

> *"If a cache and all levels are ultimately RAG (if they're gonna matter to affect
> inference, they gotta show up there) we need to persist them, until they're
> changed by your processes. The cache should be worked on by these async
> out-of-loop processes, serviced, and intermittently — works well here, same event
> architecture. It's perfect for it actually."*

> *"It's not that hard. Just send and receive events over the bus and all operate in
> concurrent processes. Queues, signals, etc. That's the gist of it. But when you
> have this by-default parallelized architecture it's simplistic to build a causal
> architecture, where stimulus and response are all that drives the organisms.
> That's why I focused so much on airc — endpoints addressable from anywhere,
> efficient forwarding, pointer-based envelopes, events, routers, middleware."*

The substrate (CBAR in C++, its Rust port here: concurrent tasks, an event bus,
addressable endpoints, pointer/`Arc` envelopes, routers, middleware) is the hard part,
and it already exists. Given it, a causal organism is cheap: stimulus arrives as an
event, processes react, the assembled context updates, the next tick reads it. The
servicers in this doc are not new machinery — they are concurrent processes on the bus
that already carries airc.

## What's wrong today: recompute-on-read

The live hot loop (`cognition/workspace.rs::WorkspaceCycle::run_in_room`) **recomputes
RAG every tick** (~3 s, `SELF_TICK_MS`). Each tick the faculties query their stores
fresh: `RecallFaculty` re-runs an embedding similarity search, `WorkingMemoryFaculty`
reads a volatile 3-slot buffer, the roster source re-queries airc. A faculty that finds
nothing *this tick* contributes nothing — even though the right value existed a moment
before. That is the **starvation** we observed live: across 603 captured turns
`[working-memory]` rendered on only 496 and `[room-roster]` on 489; on some ticks the
prompt collapsed to `[recall]` + `[workspace-map]` alone.

Three concrete gaps, all the same root (no persistence between ticks):

1. **Working memory starves.** It's a `VecDeque<String>` of capacity 3
   (`cognition/working_memory.rs`) holding the persona's own prior *reasoning*, volatile,
   abstaining whenever empty (first turns, suppressed thinking, post-reboot).
2. **Ingress can be lost.** Admitted messages persist as engrams
   (`persona/admission_persistence.rs` → `engrams.sqlite`), but gate-*dropped* messages
   go nowhere — no raw lossless "everything I saw" distinct from the filtered store.
3. **No self-refinement.** `persona/decay_tick.rs` modulates salience but never distills;
   the model never consolidates its own engrams into facts (that's
   [DREAM-CONSOLIDATION](./DREAM-CONSOLIDATION.md), unbuilt).

## The inversion: read/write split

```
            STIMULUS (airc event)                IDLE TICK (metronome)
                  │                                      │
                  ▼                                      ▼
        ┌───────────────────┐                ┌───────────────────────┐
        │  digest servicer  │                │ consolidation servicer│
        │  (event-driven)   │                │  (intermittent, LLM)  │
        └─────────┬─────────┘                └───────────┬───────────┘
                  │ publish                              │ publish
                  ▼                                       ▼
   ╔════════════════════════════════════════════════════════════════╗
   ║              THE RAG CACHE — persisted slices                   ║
   ║  recent-window │ recall-set │ facts │ salience │ roster │ …      ║
   ║  each slice = watch::Sender<Arc<Snapshot>>  (last-good, durable) ║
   ╚════════════════════════════════════════════════════════════════╝
                  ▲ borrow (lock-free, O(1), never starves)
                  │
        ┌─────────┴──────────┐
        │   HOT INFERENCE    │   reads current slice snapshots, assembles
        │   LOOP (per tick)  │   the prompt, runs the model. WRITES nothing.
        └────────────────────┘
```

- **The hot loop only READS.** It borrows the current snapshot of each slice and
  assembles the prompt. Reads are lock-free and O(1); a slice **never starves** because
  it holds its last good value until a servicer replaces it. This kills Gap 1 directly,
  and is durable by construction (Gap 2 — a slice can be backed by ORM, not just memory).
- **Servicers WRITE, off-loop, on their own cadence.** Each refinement concern owns one
  slice and publishes into it. Most run intermittently — exactly what the serviced-concern
  model is good at. This is where Gap 3 lives: consolidation is just another servicer.

This is **not a new hierarchy.** It is the substrate's existing
`watch::Sender<Snapshot>` pattern (CONCURRENCY-STYLE-GUIDE §"State distribution") applied
to RAG, and it is the same shape as `inference::llama_server`'s `serving.snapshot`:
producers own the write side and publish; the hot path subscribes and reads. We already
enforce *"subscribers READ the snapshot, they do not each issue their own probe"* — this
applies that doctrine to context assembly.

## The slice / servicer contract

A **slice** is a typed, watch-published RAG fragment: `RagSlice<T>` over
`tokio::sync::watch`, with `latest()` (borrow the current `Arc<T>` — last-good) and
`publish(value)` (replace it). Thin by design — it names the semantics (persisted,
last-good, read-only for the hot loop) the way `OpenAiBase` names a normalized base URL.
A faculty that reads a slice **cannot return empty after first write**, which is the
whole point.

A **servicer** is the writer. Per CONCURRENCY-STYLE-GUIDE it is a `ServiceModule` (it has
a command/event surface) or a `BrainRegion` (cognitive tick) — never a bespoke
`XManager`. It owns its slice's `watch::Sender`, runs on a cadence or an event
subscription, and publishes. It obeys the full concurrent-concern checklist: own task,
`catch_unwind`, `interval` not `sleep`-loop, quarantine on repeated failure, probes at
every seam.

| Cache slice | Servicer (writer) | Trigger | Resolves |
|---|---|---|---|
| `recent-window` | ChannelDigest servicer (built core, `channel_digest.rs`) | airc room event | lossless ingress (Gap 2) |
| `recall-set` | recall servicer — maintains candidate pool | new-engram / interval | starvation + hot-path re-embed (Gap 1) |
| `facts` | **consolidation servicer (the dream)** | idle tick | engrams→facts, self-refine (Gap 3) |
| `salience` | decay servicer (`decay_tick.rs`, exists) | slow interval | already mechanical |
| `roster` | roster servicer — airc subscription | roster event | stop per-tick re-query |

The "unified source registry" that #13 reached for **is this slice registry**: every
`RagSource` becomes a slice with a servicer, auto-lifted into the assembled prompt.

## The one subtlety: persistence ≠ staleness

Recall is relevance-to-the-just-arrived-message. If `recall-set` were a fully-frozen
slice it could go stale against a brand-new question. The clean split:

- **Expensive, cached, serviced:** the candidate pool — gathering engrams, computing
  embeddings, decay-weighting. This is what the servicer maintains.
- **Cheap, on-read, hot loop:** the final top-k cosine of the cached pool against the
  current focused query. Cosine over a cached pool is microseconds.

So persistence buys "never starve + never re-embed on the hot path" without buying
staleness. **Hold this line:** the hot loop may do cheap *selection* over a slice; it may
never do expensive *production*.

## The no-heuristics line (non-negotiable)

The mechanical servicers (digest, decay, roster, embedding) are substrate maintenance —
fine. The **consolidation** servicer refines by asking the **LLM** to distill, never by a
hand-written filter that reads the persona's output and puppets it
([[no-hardcoded-heuristics-to-steer-cognition]]). Refinement is learned cognition. A
servicer that pattern-matched the model's text to "fix" it would be the exact
anti-pattern this codebase forbids.

## Build order (validate by outliers, then fill in)

Per the methodical process (CLAUDE.md): define the `RagSlice` interface, prove it with
the two *most different* servicers; if both fit without forcing, the rest slot in.

1. **`RagSlice<T>` primitive** — the watch-backed, last-good, persisted slice + unit
   tests proving never-starve-after-first-write. *(this commit)*
2. **Outlier A — digest servicer (event-driven, no LLM).** Wire the built
   `ChannelDigestBuilder` as the `recent-window` slice producer; a faculty reads the
   slice so the recent window is always present (kills Gap 1's starvation, delivers
   lossless ingress). VDD gate: a tick with no new airc event still renders a non-empty
   recent-window from the last good value.
3. **Outlier B — consolidation servicer (intermittent, LLM, idle-triggered).** The dream
   ([DREAM-CONSOLIDATION](./DREAM-CONSOLIDATION.md)) writes the `facts` slice. VDD gate:
   recall prefers a distilled fact over the raw transcript engram it came from.
4. **Fill in** — recall-set, roster, salience servicers adopt the same contract. #13's
   unified registry becomes the slice registry; the per-tick re-query paths are deleted.

The VDD gates are the proof each slice removes its gap; nothing merges on assertion alone
([[cognition-half-the-work-is-harnesses]] — the glass box is
`~/.continuum/fixtures/prompt-captures` + `workspace-traces`).

## Provenance

Written 2026-06-24 after observing (via the prompt-capture glass box) that the live RAG
sections starve on a fraction of ticks because the hot loop recomputes context instead of
reading a persisted cache. Joel named the fix: persist every level as a RAG slice; have
async out-of-loop servicers maintain them on the same event substrate that already
carries airc. The causal organism is cheap *because* the parallel substrate is already
there — that's what the airc investment bought.
