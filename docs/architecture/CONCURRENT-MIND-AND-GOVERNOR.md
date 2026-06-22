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

## 4. Slice plan

- **Slice 1 — `SubstrateGovernor` heartbeat. ✅ SHIPPED** (`runtime/substrate_governor.rs`,
  commit e5dd7a63d). Deterministic daemon, ticks regions per live persona with
  `catch_unwind`+timeout isolation, publishes a `watch` snapshot, observable via
  `governor/status`. No regions schedule inference yet → flood-safe.
- **Slice 2 — Recall as a pre-staging concern.** Make a recall `BrainRegion` emit
  into a ready-buffer; the workspace's recall path **consumes the snapshot**
  instead of computing inline. *The proof that faculties are first-class bus-wired
  concerns, not batch entries.* (No inference; still flood-safe.)
- **Slice 3 — `PersonaCognitionRegion` + `VolitionFaculty` (the demand brain).**
  The persona advances what *it* wants; `VolitionFaculty` is a **wake source**
  (self-initiate from interest), not a polled bid. Cognition pulse = event + drive,
  not the governor tick.
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

The bones exist. The build is **wiring them into a free, ungated, concurrent
mind** — slice by slice, each held to §1.
