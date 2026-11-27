# SAGE: the engine every worker stands on

`continuum-sage` is the substrate every concern in Continuum runs on: each concern is a component that declares what
it hears and what it says, and the engine gives it a worker, its subscriptions, provenance, timers on one clock,
presence and rest. It is a game engine's frame pipeline (CBAR, RAGE) for minds. Owner: Fable, with every owner of a
component.

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The rule](#the-rule)
- [The pieces](#the-pieces)
- [Principles the pieces encode](#principles-the-pieces-encode)
- [Rules every part keeps](#rules-every-part-keeps)
- [How to add a part](#how-to-add-a-part)
- [Intermediate parents](#intermediate-parents)
- [Ebb and flow: one placement pattern at every scale](#ebb-and-flow-one-placement-pattern-at-every-scale)
- [What still moves onto it](#what-still-moves-onto-it)

## The rule

Every worker is a SAGE `Component`. It declares only its stimuli (topics it hears or watches on its owner's `Bus`),
its outputs (topics it says), its handler and its need. Never a hand-spawned task, a `BrainRegion`, a governor tick,
a module `tick_interval`, a sleep or a timer: the heartbeat is the frame counter, and the only wait is an alarm on
the one `Clock`. Long work is a stage run on its component's worker ([STAGE-CONTRACT.md](STAGE-CONTRACT.md)); room
is a claim on the one authority ([SHARED-RESOURCES.md](SHARED-RESOURCES.md)); time on the engine is the lane's
decision ([THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md)).

## The pieces

All in `core/continuum-sage/src`.

| Piece | What it is |
|---|---|
| `component::Component` | the parent: `Input`, `NAME`, `QOS`, `wire`, `handle`, `rest` (default no-op) |
| `component::start(c, &bus, owner, epoch)` | its worker, bounded intake (producers wait for room), subscriptions attached, a `Running` handle (drop stops it) |
| `component::Bus` | an owner's topics by name, key and type; one writer per topic (`Taken`) |
| `dataflow::State<T>` (`state`, `Publisher`, `Reader`) | the newest value with its frame number and provenance; readers never wait; changes coalesce to the newest |
| `dataflow::Stream<T>` (`stream`, `Emitter`, `Subscribe`) | every event to every subscriber, in order, never dropped; an emit waits for a busy subscriber's room |
| `component::Ctx` | in the handler: `publish`, `emit` (provenance stamped by the engine), `after(key, delay, input)` timers, `need(urgency)` |
| `clock` | the node's one clock: every deadline is an `Alarm` on it |
| `component::Presence` on `sage/presence` | engine-owned: Ready, Acting, Resting |
| `component::Need` on `sage/need` | `Urgency { Content, Want, Urgent }` set by the component, `waiting` set by the engine; `Need::max` folds a tree; `Urgency::act_bound` is the wait guarantee |
| `component::Known`, `Novelty`, `LastSeen`, `Seen` | a part's own knowledge; `novelty(&input)` and `learn(&input)` |
| `component::Observe`, `Telemetry` | a per-component switch (Off costs one state read); while timing, handled count, last and max handle, waiting, frames skipped |
| `Qos` | Interactive, Work, Utility, Background: a component's class floor |
| `Stage`, `spawn_stage`, `Capacity`, `Overflow` | the raw queue-thread worker `Component` stands on (CBAR's `QueueThread`): reliable or keep-newest intake, a capacity its owner grants |
| `Outbox` | a stage's undelivered output, retained until its consumer has room |
| `dataflow` | the stage contract's types, the runner, publication and artifacts ([STAGE-CONTRACT.md](STAGE-CONTRACT.md)) |
| `place` | the placement ledger, the `Placer`, the flexbox, the `Resident` parent ([SHARED-RESOURCES.md](SHARED-RESOURCES.md)) |
| `decide` | one parent for every decision: use cases, policies, records, replay, the learned lifecycle (below; the lane's use case in [THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md)) |
| `slice::sim`, `slice::trace`, `slice::room` | the simulator: any law against any workload, or one fitted from a live trace |

`slice::{Slicer, Lane, Control, Tuner}` are an earlier lane the engine no longer uses; the live lane is
`decide::cases::lane`, and they are deleted (THE-LANE-ON-SAGE.md, "The order").

**`decide`, one parent for every decision.** A use case declares a typed situation, its actions, and the measured
outcome of the one taken; its envelope (what no policy overrides, deferring included) and its constraints (outcomes
no policy trades). A policy chooses inside the envelope by index; the resource's owner executes the choice and
reports what happened, which closes the record the policy learns from. Records name the input revisions they read,
never copies of owner payloads, so another policy can be replayed on exactly what was known. A decision allocates
nothing once its buffers reach their high-water mark. The design test: a new use case is added without editing any
policy, and a new policy is tried on recorded history without changing the producer or the executor.

## Principles the pieces encode

- **The heartbeat is the frame counter, not a clock.** Every publication's `InputRevision { topic, through }` is its
  frame number; freshness is frames behind; waking is "a newer frame exists". The wall clock is only for deadlines a
  component chose.
- **The only serial work is a queue insert or a state read.** Expensive work runs on the worker that owns it and is
  published when done; assembling a turn's context is a pass of reads of finished frames.
- **The engine's step is the only serial work in a mind's life.** Everything else (her perception, tools, memory,
  learning, preparation) runs at its own cadence on its own workers and meets on the next short cycle through state.
- **Each concern is a self-sufficient loop.** It declares its stimuli and outputs, never knows who consumes what it
  says or who made what it hears; joining another concern is one subscription.
- **Rest is as important as action.** A component with no new input frame does nothing and publishes nothing.
- **Need comes from novelty, awareness and appraisal**, and priority is a tree of cheap decisions: a mind folds her
  parts' needs, the lane shares the engine by need, the grid places work by node need. Steering and brakes (stop, a
  human addressing her, her own runaway) win; dreams and digests run when those are quiet.
- **What an algorithm learns is part of its lifecycle, owned once.** Every pluggable law (the lane's algorithms, the
  Streamer's prefetch, any learned policy) declares only its parameters, state and objective; SAGE gives it default,
  resume (its saved last best for the node's fingerprint, the model and its input schema, else the closest key),
  online learning, checkpoint off the hot path, and stop. A resumed algorithm never relearns what it knew.
- **Lifecycle as on iOS**, engine-owned with default no-ops: prepare to rest, rest, prepare to wake, wake. Work
  prepared early is never wasted.
- **Long acts and grid peers are handles emitting events**: a compile, a browse, a remote node is something whose
  progress anything may observe; a topic on another node's bus is still a topic.
- **A component's lifetime is its node's**: started at boot, stopped with the node, never a process-wide global
  started lazily by its first caller (twice on 2026-11-22/23 such a global died with the runtime that first touched
  it, and later deadlines and events went nowhere, silently). Tests start their own node.
- **A pure step at a seam is not a component.** Something that only classifies and forwards is a function the
  producer calls, not a worker; a worker there is only a queue hop.

## Rules every part keeps

- **Probes, not log lines, for anything a consumer reads.** `probe!`, `time_sync!`, `time_async!` reach the JSONL
  probe sink and its router by their `class` field; `tracing::info!(target = ...)` does not. Probe every meaningful
  seam and name the variables you would want at a breakpoint ([RTOS-DEBUGGER-PROBES.md](RTOS-DEBUGGER-PROBES.md)).
- **Policy is code or measurement, never an env var.** A threshold, interval or count is a measurement or a `const`
  in its owner; env vars are for deployment shape (sockets, paths, devices). The one operator-set exception is how
  much of a machine the substrate may use, read once through one typed owner, never per module.
- **No lock across an await**, no lock on a hot path: state moves through `State`, `Stream` or a component's intake.
- **Every mover has its mirror**, keyed on the same number: a measurement that can only rise, a page-out with no
  page-in, a claim that only expires is a slow outage. The test is a round trip: fire it, reverse the input, assert
  the state came back.
- **No parallel managers.** A new `XManager` or `YCoordinator` almost always duplicates an owner that exists; find it.
- **Failure is contained where work runs**: a stage run is quarantined after repeated panics and its last output
  kept (`dataflow::runner`), never allowed to take the node down; startup never `unwrap`s.
- Exceptions are narrow: a one-shot startup read of a static fact (CPU or GPU vendor, OS version), configuration
  read once before the runtime starts, `eprintln!` for a fatal error before a logger exists.

## How to add a part

1. Name its stimuli: which topics it hears or watches (from those that already exist).
2. Name its outputs: which topics it says (claim them from its owner's bus).
3. Write `handle`: compare the input against what it knows (`Known`), do the work only if it is new, publish, and
   set its need. Write `rest` if it holds something worth giving back.
4. `start` it on its owner's bus. Nothing else is wired.

Example: `continuum-mind/src/persona/senses/digest.rs` watches her awareness strip, knows the room moment it last
built from, and builds and publishes her channel digest only when that moment moved.

## Intermediate parents

Extracted from `Component` at each pattern's second real instance:
- **Resident** (built, `place/resident.rs`): a holder of what a placer places; it declares its rows and the bodies
  of its ways, and the parent does the rest ([SHARED-RESOURCES.md](SHARED-RESOURCES.md), "Room by priority").
- **Long work as a stage run with a unit**: a component whose work divides declares its lever, and the scheduler
  chooses how much each run does ([STAGE-CONTRACT.md](STAGE-CONTRACT.md#units-how-much-one-run-does)).
- **Projector** (proposed: keyed cached rendering, render only on change): first instance the channel digest;
  second, the next grounding source.
- **Detector** (proposed: stream in, change events out; Awareness subscribes to every detector).
- **Part of a mind** (proposed: started on her bus with her identity, rests when she rests, need folds into hers).
- **Prepare** (proposed: watches the Core's queue and need ranking and each mind's prompt frames, and issues
  prepare-to-wake: transition prep, prefill during tool calls, prefix prep).

## Ebb and flow: one placement pattern at every scale

Wherever something finite is shared, the pattern is the same, so SAGE owns it once (`place`): capacity measured
continually; places that are levels of detail as often as tiers of memory; value from the activity, read as state;
demand forecast by whoever knows it; a law that places units where they are worth the most within each place's
budget, the coldest going down a level when room shrinks and the most valuable coming up when it appears; every move
fenced and custody leased. Never a fixed count of anything.

| Instance | Units | Places, best first | Owner |
|---|---|---|---|
| The block layer's `Streamer` | blocks: minds' KV, weights, experts, genes | VRAM, pinned or unified, flash, cold | Fable, Codex |
| Who is awake on a node | minds | stepping on the Core, resident, parked to flash, asleep | Fable |
| Who runs where on the grid | minds leased across nodes | each node, by measured capacity and link | Codex |
| What models are hot | models | VRAM or unified, flash, cold | Codex |
| A live call, any rendering | participants, avatars, scenes | full, reduced resolution or rate, thumbnail, audio only | BigMama |

Proposed, the economy around it:
- **Value is a bill of materials, not a priority number.** A unit's worth has required roles (a live persona needs
  her mind stepping, her voice, her presence on screen), each filled at some grade, and is zero unless every role is
  filled above its floor. A law trades grades within roles and drops whole units, coldest first; it never saves by
  dropping a role.
- **Every cost in one ledger, priced by scarcity:** GPU time, memory per tier, render time, bandwidth, energy,
  money. A law maximizes worth minus price within the owner's spend, preferences and deadlines (time is a price:
  dreaming can wait for cheaper capacity).
- **One economy at every scope:** a node (its own trust, no settlement), our grid (keyed trust, leases with
  receipts), open peers (earned trust, priced and verified by receipts). A placement never carries what its trust
  level does not allow; a mind's genome and private state stay inside the trust she is given.

## What still moves onto it

Each row is deleted when its last instance moves:

| Framework | Instances left | Becomes |
|---|---|---|
| `BrainRegion` + `SubstrateGovernor` | 3 regions (dream consolidation, hippocampus, credit acceptance); the governor constructed at `continuum-core/src/ipc/start_server/persona_registry_modules.rs` | components woken by their events; the trait and governor deleted |
| `ServiceModule::tick_interval` | about 18 modules with a tick | each moves to the event it polls for |
| `ResourcePool` + `PressureBroker` | the kernel's pools | `Resident`s on the authority; eviction is a shrink answered by its holder |
| `dataflow::StageRunner` | recall (`deferred_faculty.rs`) | a stage run on its component's worker |
| `slice::{Slicer, Lane, Control, Tuner}` | none (unwired) | deleted |
