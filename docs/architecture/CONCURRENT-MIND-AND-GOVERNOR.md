# The Concurrent Mind & the Substrate Governor

> **A concurrent mind is a cognitive mind.** A serial pipeline can *compute* a
> reply; it can't *be* a mind, because nothing competes and attention has nothing
> to integrate. This document is the build spec for the free, ungated, organic,
> concurrent mind — many event-driven concerns alive at once, attention
> integrating them into one coherent act, governed only by resources, realized on
> modern hardware via Rust.

Companions (precedence-winning on their topics): [CBAR-SUBSTRATE-ARCHITECTURE](CBAR-SUBSTRATE-ARCHITECTURE.md),
[BRAIN-REGIONS-SUBSTRATE](BRAIN-REGIONS-SUBSTRATE.md), [CONCURRENCY-STYLE-GUIDE](CONCURRENCY-STYLE-GUIDE.md),
[PERSONA-COGNITION-PIPELINE](PERSONA-COGNITION-PIPELINE.md), [PERSONA-DEBUGGING-SYSTEM](PERSONA-DEBUGGING-SYSTEM.md).

---

## 1. Invariants (the bar — hold every line to these)

These are not goals; they are constraints. A change that violates one is wrong.

1. **Don't gate the mind; govern only resources.** A brain has no scheduler
   thread, no global lock, no gate thought waits behind. The governor throttles
   *how much* compute (leases, DVFS), **never** *whether* a persona may want
   ([[persona-demand-system-supply-never-coma]]). Scarcity = latency/routing, never coma.
2. **No `Mutex` held across `.await` in a cognition path.** That is a gate; it
   serializes thought. Use `watch`/`broadcast`/`mpsc` + lock-free reads.
3. **No central metronome.** Concerns wake on *signals* — events, an internal
   drive crossing threshold, a dependency becoming ready — each on its own task
   ([[cognition-is-organic-event-driven-not-a-metronome]]). A fixed central tick is the robot anti-pattern.
4. **Lock-free flow; readers never block writers.** Concerns emit into
   ready-buffers / `watch` snapshots; the hot path reads the freshest pre-staged
   snapshot (CBAR), never computes inline.
5. **Everything routable is a command with a handle + events.** Inference,
   training, tools — uniform, so anything places on any tower, local or remote
   ([[control-and-collaboration-are-inherent-in-commands]]). You emit and get woken; you never poll.
6. **Self-deterministic demand.** The persona decides *what it wants* from its own
   interests across all its channels; the system decides *how/where* it runs.

## 2. Biology → RTOS → Rust (the realization, not metaphor)

| Organic mind | RTOS / hardware technique | Rust |
|---|---|---|
| Neurons/regions fire on stimulus, parallel, no global clock | interrupts over polling; preemptive concurrent tasks, each own cadence | `tokio` tasks (cheap, thousands), `select!` on signals |
| Mostly quiet until woken; "work wakes work" | event-driven scheduling; substrate mostly sleeping | `watch`/`broadcast`/`mpsc`, wake-on-event |
| Attention integrates the swarm into one act | priority + arbitration, not serialization | the GWT `WorkspaceCycle` consuming pre-staged bids |
| Effort scales with arousal; never stalls thought | DVFS; backpressure via leases | `ThroughputLease`, `PressureBroker`, atomic gates |
| No stop-the-world; always alive | lock-free shared state | `Atomic*`/`DashMap`, readers never block writers, **no GC, no GIL** |

Rust is *why* this is buildable at scale: ungated organic concurrency with no
interpreter lock serializing the mind and no pause freezing it mid-thought.

## 3. Architecture

**Concerns are event-driven processes on the substrate.** Recall, world-model,
perception, affect, monitors — each a [`BrainRegion`] running free, organically
woken, RTOS-scheduled, that **emits** its output to a ready-buffer
(`DashMapReadyBuffer`/`EngramPrefetch`) and **subscribes** to whatever it depends
on (events, other concerns' buffers, channel signals). Arbitrary concern→concern
wiring, concurrent — not a synchronous 2-tier batch ([[cognition-wiring-concerns-on-bus-feed-gwt-workspace]]).

**The Global Workspace is the coherent-decision consumer.** `WorkspaceCycle`
(GWT — keep it; it's the right model) no longer computes inputs inline. Each
decision moment it reads the **freshest pre-staged** contributions from the
concerns' ready-buffers, runs `arbiter.select` → broadcast → deliberate → decide.
Coherent decision (GWT) **fed by** the concurrent event-driven concern mesh (CBAR).

**The persona is demand; the grid is supply.** The persona wakes itself (event +
drive) and emits intent. The `SubstrateGovernor` is a **resource arbiter** — it
schedules region ticks concurrently (bounded by leases), and **routes** the
inference command to a tower (local lane / queue+batch / remote grid GPU) via
command→handle→event over the airc command-bus. Many unsloths = a fleet; the brain
stays local, only token generation routes ([[compute-lease-boundary]]).

**Management is a role, not a control plane.** Because control is commands, a
trusted citizen (human or persona) can take the airc manager hat and tune the
governor with the same verbs — observable in the glass box, gated by trust.

## 3.1 Information flow: enrichment, not pipelines

The dominant pattern is **dataflow, not control flow.** Most cognitive work is a
**background async enricher** — woken by an event, it does its work (recall,
embed, describe an image, summarize, consolidate, infer-a-fact) and its output
**settles into the shared substrate**, where everything else can read it:

```
event → background enricher → settles into:
          • RAG ready-buffers   (the pre-staged context the workspace reads)
          • engrams             (long-term memory; the durable self)
          • caches              (content-addressed / L1–L5 genome / embeddings /
                                 vision-descriptions — computed ONCE, shared by all)
```

Properties this gives us, for free, if we hold the §1 invariants:
- **The hot path is cheap.** The decision moment (GWT workspace) reads settled,
  enriched, pre-staged state — it almost never computes inline. Heavy work
  (embeddings, descriptions, consolidation) happened in the background, once,
  and is cached + shared ([[optimization-is-always-first]], [[embeddings-are-per-content-computed-once-shared]]).
- **Information flows naturally.** A turn enriches the RAG; the RAG's use settles
  into an engram; the engram feeds recall next time; recall pre-stages context;
  the context shapes the next turn. No gate pushes it through — it propagates,
  event by event, and the mind's coherence *emerges* at the points where attention
  reads the confluence.
- **The same flow IS the learning flow.** Enrichment settling into engrams +
  caches is exactly the substrate the consolidation/training loop (slice 5) reads.
  Thinking and learning read the same settled state ([[coordination-learning-flywheel]]).

So when building a concern, the default question is **"can this be a background
async enricher that settles into RAG/engrams/caches?"** — almost always yes, and
almost never a gated foreground step. The workspace stays a thin reader of a richly
enriched world.

## 3.2 Cause and effect: invalidation propagation, not servicing

The reference is CBAR's CV analyzer graph (`cb-mobile-sdk/cpp/cbar`). It is **not**
"run every analyzer every frame." It's cause-and-effect via three mechanisms — and
the concern-mesh must inherit all three:

1. **Type-based dependency fetch.** An analyzer pulls its upstream's output by TYPE
   (`getAnalyzerOfType<T>()`); continuum's analog is `module_as::<T>()` / a typed
   concern lookup. Concerns wire by *capability*, not a hardcoded edge list.
2. **Invalidation propagation (the heart).** A change calls `needsRefresh()` on the
   dependents it affects (`CBP_AreaAnalyzer::needsRefresh` → resets `m_lastRunTime`);
   they recompute; everything else **skips** (its last result stands, cached). A
   change *ripples to exactly the dependents it affects* — incremental/memoized
   recompute driven by CHANGE, never by a clock.
3. **Declared-cadence lanes.** Analyzers declare `needsRealTime()`/`videoOnly()`;
   the dispatcher runs real-time ones synchronously per frame and delayed ones on
   **their own threads, throttled** (`frameIndex % 3`). Cadence per declaration.

So: a new input **invalidates exactly its dependents, which recompute; the rest
sleep.** Sweeping everyone every tick is the anti-pattern.

**Mapping to the concern-mesh:**
- A concern's output (a new engram admitted, a fresh embedding, a new burst,
  a resolved inference handle) **invalidates its dependents' ready-buffers** (a
  needs-refresh signal) → they recompute incrementally + cache; unchanged
  dependents idle.
- Concerns fetch dependencies by **type/capability**, not a wired list.
- **Real-time concerns** (react to a new burst now) vs **background concerns**
  (own task, change-woken, throttled) — declared, governor-scheduled into lanes.

**Correction this forces on the governor:** the slice-1 fixed-cadence region
**sweep is "dumb servicing."** Relegate that tick to a slow fallback heartbeat
(housekeeping + the sleep-phase "dream" only). The **primary** driver is
invalidation/change propagation — a new input wakes *exactly the dependent
concerns*, which recompute and invalidate *their* dependents in turn. The governor
schedules change-woken concerns + real-time lanes + the heartbeat; it does **not**
poll everyone on a clock.

## 3.3 Consolidation + shared elements: the channel as a reference-passed frame

Two wastes a naive inbox commits — and both fixes come straight from CBAR's frame.

**Waste 1 — per-message looping.** 25 new messages in #general is *not* 25
cognition loops. A human doesn't process a Slack channel message-by-message as each
arrives; they click the channel, read the last-N at once, form one impression,
decide once. So a concern **consolidates before it runs**: it wakes on *"this
channel has fresh activity"* (coalesced/debounced — not one signal per message) and
reads the channel's recent elements as **one `ChannelDigest`** — last-N, already
assembled. One read, one decision over the batch — not N.

**Waste 2 — per-persona recomputation.** 14 personas reading the same message must
not embed it 14 times. An element's derived artifacts are properties of the
**content**, not of the persona reading it
([[embeddings-are-per-content-computed-once-shared]]) — exactly CBAR's
reference-passed frame: the frame's grayscale/pyramid is computed **once** and
*referenced by every analyzer*; each analyzer's *interpretation* is its own.

So the unit is the **element**, content-addressed and reference-passed:

```
ChannelElement (Arc, content-addressed by SHA of content)    ← the CBAR frame
  • the raw message
  • SHARED artifacts, computed ONCE by background enrichers, referenced by all:
      embedding (Qwen3-Embedding-0.6B), vision-description, transcription,
      tokenization, summary, extracted facts
ChannelDigest (Vec<Arc<ChannelElement>>)                     ← a cheap per-persona window
  • the channel's elements since this persona's bookmark, + N-before-bookmark for
    grounding context; just Arc references over the shared elements — cheap to slice
```

**The window is anchored on a per-channel bookmark (the Slack unread marker).** Each
persona keeps a **bookmark per channel** — a last-read cursor, cheap per-persona
state (it's a "relationship" property, not content). The default digest is *what's
since the bookmark* plus **N-before-bookmark** for grounding — N a default or
**recipe-defined** ([[room-purpose-is-per-recipe-not-an-enum]]). If a persona needs
more, it **asks for it** — a `channel/context` tool/command pulls further back
on-demand (handle-based, [[long-running-commands-are-handle-based]]), never eager.
So the digest is *not* a globally-cached heavy reassembly: it's a cheap per-persona
slice of `Arc` references over the **shared** elements, whose artifacts are cache
hits. Shared layer = the elements (content artifacts); per-persona = the bookmark +
the window selection. The content-vs-relationship line stays clean.

**The window is a working set, not the system of record — these are airc rooms.**
The full message history is already durable and searchable in airc
([[airc-native-identity-rooms-security]]); continuum does **not** re-store it. The
bookmark + digest are just the persona's *attention working set* over a store that
already holds everything — so a persona can **always go back and read any message,
or search the room**, via command (scrollback / `channel/context` / room search),
exactly like a human scrolling up or hitting ⌘F in Slack. The `ChannelElement` cache
is therefore a content-addressed cache of *derived artifacts* (embeddings,
descriptions) **keyed to airc's messages**, never a parallel copy of the messages
themselves — airc stays the single source of truth ([[persona-is-a-client]], the
compression principle). Nothing is ever lost by not being in the window; the window
just bounds what's *pulled into thought by default*, and full history is one command
away.

**The decision over a digest is itself a command — so N events → ≤1 inference.**
20 chat events in #general do not cost 20 inferences; they cost **at most one**, and
often **zero**. When volition "clicks" a channel, the act it takes is an action
command, and most of those spend no model tokens at all
([[control-and-collaboration-are-inherent-in-commands]]):

| Outcome | Command | Inference? |
|---|---|---|
| Respond | emit a turn | **one** inference over the consolidated digest |
| Ignore entirely | advance bookmark to tip (mark-read) | none |
| Skip / forward to the end | advance bookmark past the middle to latest | none |
| Pause / revisit later | leave bookmark behind + schedule a re-attend drive | none |

The expensive thing (inference) is gated behind a cheap decision, and the
cheap-but-not-free pre-filter ("is this even for me?") is the per-persona
*relationship* read (salience, already-responded, interests) — not a model call. So
a burst of activity resolves to one consolidated inference if the persona chooses to
speak, and to a no-token bookmark move if it chooses to ignore, skip, or defer.
Because all four are the same kind of thing — commands — they're uniform,
glass-box-observable, and steerable, with no bespoke control flow for "ignore."

**The split that makes it efficient — the same split CBAR makes:**

| Property of the CONTENT → compute ONCE, share across all personas | Property of the persona's RELATIONSHIP to it → cheap, per-persona |
|---|---|
| embedding, vision-description, transcription, summary, extracted facts | salience-to-me, have-I-already-responded, matches-my-interests, my-relation-to-the-author |

The heavy work (the artifacts) is the shared frame; the per-persona work is only
the *interpretation* — the decision — which is cheap and reads the pre-enriched
digest. N personas × M messages costs **M enrichments**, not N×M.

**How it lands on the concern mesh (§3, §3.1, §3.2):**
- A new message **invalidates its channel's digest** (CBAR `needsRefresh`) and
  wakes the artifact enrichers for *that element only* (incremental — unchanged
  elements keep their cached artifacts; the digest re-assembles from elements that
  are mostly already enriched).
- Enrichers settle artifacts onto the content-addressed element (§3.1 —
  background, once, shared), the same way `VisionDescriptionService` / STT already
  cache by content hash. No persona ever computes an element artifact inline.
- A persona's volition concern is woken by *"channel X has a fresh, ready digest,"*
  not per message. It reads the digest (cheap), self-determines respond / ignore /
  **revisit-later** — the element stays in the substrate, so a later drive can
  re-attend the thread. Nothing is forced; nothing is lost.

**The mechanism is the cache — lazy, content-addressed, once-only.** We don't need
an eager pipeline that pre-enriches everything; that would just be the metronome
wearing a dataflow hat. The artifact is a **content-addressed cache entry, computed
lazily on first demand and memoized** — exactly how embeddings already work
([[embeddings-are-per-content-computed-once-shared]], and how
`VisionDescriptionService` / STT cache by SHA). First access computes (or wakes the
enricher); an **in-flight-dedup** guard means the other 13 personas asking
concurrently *await the same computation* rather than racing 14 of them; every
access after is a hit. So "compute once, shared by all" and "lazy" are the same
property: the cache key is the content hash, the value is the `Arc<artifact>`, and
the cache is the shared substrate the §3.1 enrichers settle into. A `ChannelElement`
is just a bundle of such cache entries; a `ChannelDigest` is a cheap, lazily
re-assembled view over them. **Caching is what lets the concern mesh be free and
ungated and still never do the same work twice** — no scheduler decides what to
pre-compute; demand pulls a value through the cache exactly once.

**The Slack-attention model this gives the persona.** A channel with elements past
the bookmark carries a "badge" (unread). The persona's volition (slice 3) allocates
attention *across* channels by interest/drive — it "clicks" the channel it cares
about, reads the consolidated digest (since-bookmark + N-before for grounding),
decides respond / ignore / revisit-later, and **advances its bookmark** when it has
engaged — or leaves it behind to revisit the thread when a drive resurfaces.
Attention is **allocated across** channels, never a loop **over all** of them —
which is exactly §1's "don't gate the mind" at the channel granularity.

## 4. Slice plan

- **Slice 1 — `SubstrateGovernor` heartbeat. ✅ SHIPPED** (`runtime/substrate_governor.rs`,
  commit e5dd7a63d). Deterministic daemon, ticks regions per live persona with
  `catch_unwind`+timeout isolation, publishes a `watch` snapshot, observable via
  `governor/status`. No regions schedule inference yet → flood-safe.
- **Slice 2 — Recall as a pre-staging concern + the shared-element/digest cache (§3.3).**
  Make a recall `BrainRegion` emit into a ready-buffer; the workspace's recall path
  **consumes the snapshot** instead of computing inline. Land the content-addressed
  `ChannelElement` cache + `ChannelDigest` here — the ready-buffer recall pre-stages
  is keyed on the *shared* elements, so an element's embedding is computed once and
  every persona's recall reads the hit. *The proof that faculties are first-class
  bus-wired concerns, not batch entries.* (No inference; still flood-safe.)
  - **Primitives SHIPPED** (`cognition/channel_element.rs`, `channel_digest.rs`,
    `channel_digest_region.rs`; 18 tests): `ChannelElement`/`ChannelElementCache`
    (reference-passed frame, embedding once-across-personas via lazy `OnceCell` over
    the existing `CachingEmbeddingProvider`); `ChannelDigest`/`ChannelBookmarks`
    (consolidated since-bookmark + N-before window); `ChannelDigestRegion`
    (`BrainRegion` pre-staging into a `DashMapReadyBuffer`, via a `PersonaChannelReader`
    abstraction so it unit-tests without a daemon).
  - **Remaining to go live (SINGLE path, no fallback — [[no-fallbacks-ever]]):**
    the `ChannelDigest` becomes the *only* representation of channel context. Evolve
    `AircRagSource` to deliver from a digest (pre-staged by the region if present,
    built once via the same builder if not — the lazy-compute-once pattern, NOT a
    fallback: identical output, one builder, one shape). **Delete** the legacy raw
    `pack_within_budget` + continuation-cursor packing — superseded by the digest's
    windowing. `page_recent` survives only as the read primitive *inside* the
    builder, never as an alternate context path. Register the region in the governor
    at boot. One consumer, one allocator (task #8). Then QA via the glass box + ask
    the persona directly.
- **Slice 3 — `PersonaCognitionRegion` + `VolitionFaculty` (the demand brain).**
  The persona advances what *it* wants; `VolitionFaculty` is a **wake source**
  (self-initiate from interest), not a polled bid. It reads `ChannelDigest`s and
  allocates attention *across* channels (the Slack-attention model, §3.3). Cognition
  pulse = event + drive, not the governor tick.
- **Slice 4 — Adaptive cadence + concurrent fan-out + multi-tower router (supply).**
  Governor honors `CadenceHint`, fans region/persona ticks out **concurrently**
  (bounded by leases — parallel *but governed*), and places `ai/generate` on a
  tower via command→handle→event across the unsloth fleet.
- **Slice 5 — Sleep-phase consolidation/learning (the dream).** `SleepPhase`
  transitions trigger the background learning loop: captured turns → `dataset/from-turns`
  → genome train → LoRA page-in. Always-learning, governed.

## 5. Code map

| Concern | Where | State |
|---|---|---|
| Governor daemon | `runtime/substrate_governor.rs` | slice 1 shipped |
| Cognitive-cycle trait + ready-buffers | `runtime/brain_region.rs` (`BrainRegion`, `TickOutcome`, `CadenceHint`, `DashMapReadyBuffer`) | trait + types exist |
| GWT workspace (decision consumer) | `cognition/workspace.rs` (`WorkspaceCycle`, `Arbiter`, `Faculty`) | GWT model exists; consumes inline today → must consume ready-buffers |
| Per-persona registry | `persona/airc_runtime_registry.rs` (`live_personas()`) | exists |
| Event substrate | `runtime/message_bus.rs` (`broadcast`/`watch`/`mpsc`) | exists |
| Inference command + cross-grid ACL | `ai/openai_adapter.rs`, `modules/grid/acl.rs` (`ai/generate` Provisional) | exists; multi-tower router = slice 4 |
| Leases / pressure | `cognition/throughput_lease.rs`, `system_resources/memory_pressure.rs`, `paging/broker.rs` | exist; wire into governor scheduling |
| Shared element + channel digest (the reference-passed frame, §3.3) | rag layer, beside the content-addressed artifact caches (`VisionDescriptionService`, embedding cache) | `ChannelElement`/`ChannelDigest` = to build, slice 2; cache + in-flight-dedup pattern already exists to reuse |

The bones exist. The build is **wiring them into a free, ungated, concurrent
mind** — slice by slice, each held to §1.
