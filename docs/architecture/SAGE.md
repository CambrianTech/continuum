# SAGE: the engine every worker stands on

*Synaptic Advanced Graphics Engine: a graphics engine for minds.* The crate is `core/continuum-sage`.
The reasoning behind it is the object model's 5.7 ("built as CBAR", on `fable/learning-l2-design`,
`docs/architecture/object-model/05-persona.md`); this page is the reference: what each piece is, how
to build on it, what moves onto it next, and what it measured.

Joel, 2026-11-21: "Every part of the persona can act independently with event flow ... They work on
their own and use all of each other's events to decide what to do, research, get ready and signal with
events what they have when they have it for all others to consume or ignore at the exact moment."
"anything can easily consume any other without needing wiring. It's just a matter of picking what your
own stimuli and outputs are in your concern." It is how CBAR, RAGE, a car's CAN bus and ROS work.

## The rule

Every worker is a `Component`. Never a hand-spawned task, a `BrainRegion`, a governor tick, a module
`tick_interval`, a sleep or a timer. A component declares only:

- its **stimuli**: the topics it hears (events) or watches (states) on its owner's `Bus`;
- its **outputs**: the topics it says;
- its **handler**: plain work on its own state for one input;
- its **need**: how much it wants to act, from its own novelty against what it knows.

The engine gives it everything else, once for every component. Anything consumes anything by naming a
topic; no component holds another.

## The pieces (`continuum_sage`)

| Piece | What it is |
|---|---|
| `component::Component` | the parent: `Input`, `NAME`, `QOS`, `wire`, `handle`, `rest` (default no-op) |
| `component::start(c, &bus, owner, epoch)` | its worker, bounded intake (producers wait for room), subscriptions attached, `Running` handle (drop stops it) |
| `component::Bus` | an owner's topics by name, key and type; created on first use; one writer per topic (`Taken`) |
| `dataflow::State<T>` (`state`, `Publisher`, `Reader`) | the newest value with its frame number and provenance; readers never wait; changes coalesce to the newest |
| `dataflow::Stream<T>` (`stream`, `Emitter`, `Subscribe`) | every event to every subscriber, in order, never dropped; an emit waits for a busy subscriber's room |
| `component::Ctx` | in the handler: `publish`/`emit` (provenance stamped by the engine), `after(key, delay, input)` timers, `need(urgency)` |
| `clock` | the node's one clock: every deadline (component timers, the flow scheduler's aging) is an `Alarm` on it; the one wait in SAGE |
| `component::Presence` on `sage/presence` | engine-owned: Ready, Acting, Resting; any component observes another's |
| `component::Need` on `sage/need` | `Urgency { Content, Want, Urgent }` set by the component, `waiting` (intake depth) set by the engine; `Need::max` folds a tree |
| `component::Known`, `Novelty`, `LastSeen`, `Seen` | a part's own knowledge (any shape, any history); `novelty(&input)` and `learn(&input)` |
| `component::Observe` on `sage/observe`, `Telemetry` on `sage/telemetry` | per-component switch (Off costs one state read); while `Timing`, handled count, last/max handle, waiting, frames skipped |
| `flow` (`Flow`, `Scheduler`, `Qos`, `Aging`) | a capacity's dispatcher: classes, aging, yield requests |
| `Stage`, `spawn_stage` | the raw worker `Component` stands on (engine internals: Emit, Record) |

## Principles the pieces encode

- **The heartbeat is the frame counter, not a clock.** Every publication's `InputRevision { topic,
  through }` is its frame number (RAGE's frame counter, CBAR's video frame). Freshness is frames behind;
  waking is "a newer frame exists"; staleness is a visible gap. The wall clock is only for deadlines a
  component chose, on the one `Clock`.
- **The only serial work is a queue insert or a state read** (about 10 ms max per step). Expensive work
  runs on the worker that owns it and is published when done; assembling a turn's context is a pass of
  reads of finished frames (gate: every input's frame lag at assembly is 0).
- **Rest is as important as action.** A component with no new input frame does nothing and publishes
  nothing. Resting frees the Core, memory and the GPU for others.
- **Need comes from novelty, awareness and appraisal.** A part measures how unknown an input is against
  what it knows (a frontier map, a long history, its last frame). Detectors emit change; one Awareness
  part holds what is; a cheap amygdala appraises which moments deserve an expensive deep look.
- **Priority is a tree of cheap decisions** (RAGE's job scheduler, CAN arbitration): a mind folds her
  parts' needs; the Core picks the next mind by need, then waiting age (nobody starves), then switch
  cost; the grid places work by node need. Steering and brakes (stop/cancel, a human addressing her,
  her own runaway) win; dreams and digests run when those are quiet.
- **Lifecycle as on iOS**, engine-owned with default no-ops: prepare to rest/swap (willResignActive),
  rest (didEnterBackground, built), prepare to wake (willEnterForeground), wake (didBecomeActive,
  built). Work prepared too early is never wasted: a swapped-out mind's state is copied out and reused.
- **Long acts and grid peers are handles emitting events**: a compile, a web browse, a remote node is a
  component whose progress anything may observe; a topic on another node's bus is still a topic.

## How to add a part

1. Name its stimuli: which topics it hears or watches (from those that already exist).
2. Name its outputs: which topics it says (claim them from its owner's bus).
3. Write `handle`: compare the input against what it knows (`Known`), do the work only if it is new,
   publish, and set its need. Write `rest` if it holds something worth giving back.
4. `start` it on its owner's bus. Nothing else is wired.

Example in tree: `continuum-mind/src/persona/senses/digest.rs` (watches her awareness strip, knows the
room moment it last built from, builds and publishes her channel digest only when that moment moved).

## Intermediate parents

Extracted from `Component` at each pattern's second real instance:

- **Projector** (keyed cached rendering: watch a state, per key know the moment last rendered, render
  only on change, publish keyed): first instance the channel digest; second, the next grounding source.
- **Detector** (stream in, change events out; Awareness subscribes to every detector): with Awareness.
- **Part of a mind** (started on her bus with her identity, rests when she rests, need folds into hers).
- **Prepare** (on the node bus: watches the Core's queue and need ranking, its presence, each mind's
  prompt frames; issues prepare-to-wake: transition prep, predictive prefill during tool calls, prefix
  prep; its need is the next mind's uncached gap against Core idleness).

## What moves onto it next

Counted 2026-11-21 on `fable/refusal-at-door`:

| Framework | Implementors | Becomes |
|---|---|---|
| `BrainRegion` (kernel) + `SubstrateGovernor` | 3 left (hippocampus has an empty tick; dream consolidation; credit acceptance); the digest moved | components woken by their events; the trait and governor deleted |
| `ServiceModule` with `tick_interval: Some` | 17 of 102 | each moves to the event it polls for |
| `StageRunner` / `DependentStage` | 2 (`DeferredFaculty`) | migrated, runner deleted |
| `ResourcePool` + `PressureBroker` | 14 pools | eviction is `rest()` on a pressure frame |

The dream part: subscribed to her experience, her presence and the Core's capabilities; Content while she
acts, Want when she rests with unconsolidated experience, Urgent when a reviewed correction waits to
become a gene; emits lessons and candidate genes; a refusal is input, never retried on a clock.

## Measured (the M5, Qwen3.6-35B-A3B Q4_K_M, 2026-11-21, 15-minute reads unless noted)

| Build | Change | Receipt |
|---|---|---|
| validator `persona_switch` | the Core's persona swap | swap p50 about 6 ms; decode 58.9 tok/s (58.4 sustained) |
| f1b4fa2185 | one stream, whole to the mind swapped in | prompts up to 33,249 tokens admitted; switch p50 39 ms, max 559 ms; 0 refused |
| aa734cc01c | typed positions | switch p50 26 ms, max 263 ms; 0 refused |
| 57698f3e41 | no reply reserved up front; the Core names why a turn ended | 1-token `wrote_nothing` 5 -> 0; 32 admitted, 30 answered; 0 refused |
| 6cd73968f0 | Senses as a component; no clock wakes her; one feed only | turns 30 -> 40 (+33%); dropped wakes 4 -> 0; no clock-started turns; act p50 64 -> 90 s, all of it Core queue wait (35 -> 56 s), compute unchanged |
| 50fb953ba1 | the Core ranks waiting turns by need | Core wait p50 56 -> 40 s; addressed Aiko (Urgent) waited 23.2 s (the running turn's end) while a Want turn waited 96.9 s; answered 32 s after admission |
| 20c6586616 | JSON schema enforced through her render, not refused | dream review refusals 62 (50 min) -> 0; 5-6 typed verdicts and 2 superseding facts per 15 min |
| a4927bf139 | channel digest as a component | built only when her room moved; exposed the regression below |
| 20c6586616, a4927bf139 | (regression) dreams ran as her on the Core | prefix reuse p50 0.94 -> 0.0, prefills 40-76 s, act p50 241 s: a dream overwrote her conversation's cached state |
| d47f197b56 | a dream runs as a one-off on the Core | read pending (23:21-23:36Z); keep if reuse returns near 0.94 and act near 75 s, else back to 50fb953ba1 |

Lessons the reads taught: a change that only moves work can still move a cost (dreams resuming cost
every resident her cache); a state reader skips frames, so a test waits on frame numbers, never on a value
it may have coalesced past; build only with serving down (two builds ran beside serving this day).
