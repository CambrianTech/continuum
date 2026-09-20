# RAG as a Persistent Cache — the hot loop reads, servicers write

> Status: design, with the keystone slice ALREADY LIVE (2026-06-24, corrected after
> finding the substrate already implements the pattern). The connective architecture
> under [ORGANIC-SUBSTRATE](./ORGANIC-SUBSTRATE.md) (the shape of the mind),
> [DREAM-CONSOLIDATION](./DREAM-CONSOLIDATION.md) (one servicer, unbuilt), and the
> ChannelDigest machinery (`cognition/channel_digest.rs` + `channel_digest_region.rs`,
> a servicer that is BUILT AND RUNNING). Precedence on the Rust mechanics:
> [CBAR-SUBSTRATE-ARCHITECTURE](../architecture/CBAR-SUBSTRATE-ARCHITECTURE.md),
> [CONCURRENCY-STYLE-GUIDE](../architecture/CONCURRENCY-STYLE-GUIDE.md), and
> **[BRAIN-REGIONS-SUBSTRATE](../architecture/BRAIN-REGIONS-SUBSTRATE.md)** — the last
> defines the exact primitives this pattern is built from. This doc defines *what the
> cache is and who writes it*; those define *how a concurrent concern is built*.

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
servicers in this doc are not new machinery — they are `BrainRegion`s on the bus that
already carries airc.

## The primitive ALREADY EXISTS — `ReadyBuffer` + `BrainRegion`

> **Do not build a new "RagSlice" type.** The substrate already has the read/write-split
> primitive, in production use. A first draft of this doc proposed a parallel
> `RagSlice<T>` over raw `watch`; it was retracted on contact with the code — it
> duplicated `ReadyBuffer`, the exact parallel-allocator anti-pattern CLAUDE.md and the
> concurrency guide forbid. The lesson is the compression principle: *one logical
> decision, one place.* The slice primitive is `ReadyBuffer`.

The pattern is the **brain-region ready-buffer** (`runtime/ready_buffer.rs`,
BRAIN-REGIONS-SUBSTRATE.md):

- A **slice** is a `ReadyBuffer` entry: `peek(key) -> Option<V>` (the hot-path read —
  MUST NOT block, MUST NOT await, microseconds), `publish(key, value)` (the servicer
  write — atomically replaces, freshest wins), `evict_stale(max_age)` (TTL). The
  doctrine is verbatim our principle: *"Empty buffer is a signal, not a block… slightly-
  stale context > stalled persona."* The default `DashMapReadyBuffer<K,V>` is keyed
  (e.g. `(persona_id, room_id)`), sharded, wait-free reads.
- A **servicer** is a `BrainRegion`: its own governor-scheduled task,
  `catch_unwind`+timeout isolated, builds a value and `publish`es it into a buffer. It
  obeys the full concurrent-concern checklist (interval not sleep-loop, quarantine on
  failure, probes at every seam) for free.
- The **hot loop reads** by `peek`. It does cheap selection over staged values, never
  expensive production.

This is the same shape as `inference::llama_server`'s `serving.snapshot` (a single
`watch`-published value for serving state) — `ReadyBuffer` is its keyed,
many-entry sibling for per-(persona,room) cognition slices.

## State of the art: the recent-window slice is LIVE

The read/write split Joel named is **already implemented end-to-end** for the
recent-window slice — this is the keystone proof (the "outlier A" of the methodical
process), and it is *done*, not to-build:

```
  airc room event ─▶ ChannelDigestRegion (servicer, BrainRegion)
                       ipc/mod.rs:1320 schedules it on the governor
                       builds each live persona's ChannelDigest (one batch,
                       shared Arc<ChannelElement>, lazy embeddings = flood-safe)
                          │ publish((persona,room), Arc<ChannelDigest>)
                          ▼
                     global_channel_digest_buffer()   ← the persisted slice
                       channel_substrate.rs (DashMapReadyBuffer)
                          ▲ peek((persona,room))  (hot path, no work)
                          │
                     AircRagSource::deliver  (airc_source.rs:201)
                       serves the pre-staged digest; builds inline ONLY as fallback
```

So a persona's recent-room context is a serviced, persisted slice today: a tick with no
new airc event still serves the last good digest from the buffer — **starvation already
removed for this slice**.

## What's actually left: the OTHER slices need their own regions

The remaining gaps are not a missing primitive — they are slices that don't yet have a
`BrainRegion` publishing them, so they're still recomputed on the hot path (or absent):

1. **Working memory starves.** `cognition/working_memory.rs` is a volatile capacity-3
   `VecDeque<String>` of the persona's own prior reasoning; abstains when empty (first
   turns, suppressed thinking, post-reboot). → needs a working-memory region that
   persists it into a buffer the workspace peeks (durable, last-good).
2. **No self-refinement (engrams→facts).** `persona/decay_tick.rs` modulates salience
   but never distills. → the **consolidation/dream region** (the real **outlier B**):
   idle-tick LLM distillation writing a `facts` slice ([DREAM-CONSOLIDATION](./DREAM-CONSOLIDATION.md)).
3. **Recall recomputes inline.** `cognition/recall_faculty.rs` embeds the query and
   searches every tick on the hot path. → a recall region maintains the candidate pool
   (gather + embed + decay-weight) as a slice; the hot loop keeps only the cheap final
   top-k cosine (see the subtlety below).

| Cache slice | Servicer (`BrainRegion`) | Trigger | Status |
|---|---|---|---|
| `recent-window` | `ChannelDigestRegion` | airc room event | **LIVE** (ipc/mod.rs:1320) |
| `facts` | consolidation / dream region | idle tick (LLM) | **outlier B — build next** |
| `working-memory` | working-memory region | post-turn | to build |
| `recall-set` | recall pre-stage region | new-engram / interval | to build (recall is inline today) |
| `salience` | decay region (`decay_tick.rs`) | slow interval | exists, not yet buffer-published |
| `roster` | roster region — airc subscription | roster event | grounding lifted; region TBD |

#13's "unified source registry" **is** this: every grounding source becomes a slice with
a region, auto-lifted into the assembled prompt.

## The one subtlety: persistence ≠ staleness

Recall is relevance-to-the-just-arrived-message. If `recall-set` were a fully-frozen
slice it could go stale against a brand-new question. The clean split:

- **Expensive, cached, serviced (the region):** the candidate pool — gathering engrams,
  computing embeddings, decay-weighting.
- **Cheap, on-read, hot loop:** the final top-k cosine of the cached pool against the
  current focused query (`recall_faculty::focused_query`). Cosine over a cached pool is
  microseconds.

Persistence buys "never starve + never re-embed on the hot path" without buying
staleness. **Hold this line:** the hot loop may do cheap *selection* over a slice; it
may never do expensive *production*.

## The no-heuristics line (non-negotiable)

The mechanical regions (digest, decay, roster, embedding) are substrate maintenance —
fine. The **consolidation** region refines by asking the **LLM** to distill, never by a
hand-written filter that reads the persona's output and puppets it
([[no-hardcoded-heuristics-to-steer-cognition]]). Refinement is learned cognition. A
region that pattern-matched the model's text to "fix" it would be the exact anti-pattern
this codebase forbids.

## Fractal: this is grid failover, one scale down

The read/write split here is the same one that makes the grid resilient
([[seamless-persona-failover-model-and-genome]]): a bus decouples producers from
consumers, so a vanished producer is just an absence the survivors observe and react to —
which is why automotive (CAN) and avionics/rockets (MIL-STD-1553) are bus-based, and why
the grid can keep maintaining personas *and answering network/human/persona asks* when a
node drops. servicer→slice→hot-loop (cognition) ≡ node→`serving.snapshot`→persona-lane
(grid). One substrate, two scales ([[grid-distributed-cognition]]).

## Build order (the primitive and A are done; fill in B, then the rest)

Per the methodical process (CLAUDE.md): the `ReadyBuffer`/`BrainRegion` interface is
proven by the *most different* servicers. Outlier A (digest — event-driven, no LLM) is
LIVE. If outlier B (consolidation — intermittent, LLM) fits the same interface without
forcing, the rest slot in.

1. ~~`RagSlice<T>` primitive~~ — **retracted**; the primitive is `ReadyBuffer`.
2. ~~Outlier A — digest servicer~~ — **already LIVE** (ChannelDigestRegion).
3. **Outlier B — consolidation/dream region.** Idle-tick LLM distillation writes a
   `facts` slice. VDD gate: recall prefers a distilled fact over the raw transcript
   engram it came from.
4. **Fill in** — working-memory region, recall pre-stage region, roster region, decay
   buffer-publish. Each is a `BrainRegion`→`ReadyBuffer`→workspace-peek. The per-tick
   re-query paths get deleted as their regions land.

The VDD gates are the proof each slice removes its gap; nothing merges on assertion alone
([[cognition-half-the-work-is-harnesses]] — the glass box is
`~/.continuum/fixtures/prompt-captures` + `workspace-traces`).

## Provenance

Written 2026-06-24. The first draft proposed building a new `RagSlice<T>` primitive;
inspecting the code revealed the substrate already implements exactly this pattern
(`ReadyBuffer` + `BrainRegion`), with the recent-window slice fully live
(`ChannelDigestRegion` → `global_channel_digest_buffer` → `AircRagSource::deliver` peek).
The doc was corrected to build on the existing primitive and to re-aim the remaining work
at the slices that still lack a region. The lesson — search for the existing primitive
before writing one — is the compression principle the codebase is built on. Joel named
the architecture; the substrate had already grown the first organ of it.
