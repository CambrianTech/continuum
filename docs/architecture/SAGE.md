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
| `slice::Slicer` | which ready mind the engine steps next: stride shares by need, the wall and deadline guard that keep the wait bound, a pluggable control law; pure, time passed in ([the lane](#the-lane-the-one-serial-piece-sliced-so-fine-nobody-notices)) |
| `slice::lane::Lane` | where minds meet the engine: reads each seated mind's newest `Need` and `Decoding` at the step boundary, never awaits |
| `slice::control::Control`, `slice::tuner::Tuner` | the slice's control law, pluggable; `Tuner` climbs measured throughput against the wall |
| `slice::trace`, `slice::sim` | a run as data (replays decision for decision); any law against any workload, or one fitted from a live trace |
| `Stage`, `spawn_stage` | the raw worker `Component` stands on (engine internals: Emit, Record) |

## Principles the pieces encode

- **The heartbeat is the frame counter, not a clock.** Every publication's `InputRevision { topic,
  through }` is its frame number (RAGE's frame counter, CBAR's video frame). Freshness is frames behind;
  waking is "a newer frame exists"; staleness is a visible gap. The wall clock is only for deadlines a
  component chose, on the one `Clock`.
- **The only serial work is a queue insert or a state read** (about 10 ms max per step). Expensive work
  runs on the worker that owns it and is published when done; assembling a turn's context is a pass of
  reads of finished frames (gate: every input's frame lag at assembly is 0).
- **What an algorithm learns is part of its lifecycle, owned once.** Every pluggable law (the Lane's
  `Control` and `Tuner`, the block layer's `PlacementLaw` and `Forecaster`, any learned policy after
  them) inherits one lifecycle from SAGE: default, resume, learn, checkpoint, stop.
  - It starts from its saved last best for the key it was learned on: the node's fingerprint, the
    model, and the schema of what it reads.
  - Failing that, it starts from the closest saved key, then from a good default.
  - It learns online for its own objective.
  - It checkpoints its current state and its best by that objective, off its worker's hot path, on a
    frame cadence and at a stop.
  - A resumed algorithm never relearns what it knew.

  An algorithm declares only its parameters, its state and its objective; saving, keying and
  resuming are SAGE's, as a component's worker and rest are.
- **Rest is as important as action.** A component with no new input frame does nothing and publishes
  nothing. Resting frees the Core, memory and the GPU for others.
- **Need comes from novelty, awareness and appraisal.** A part measures how unknown an input is against
  what it knows (a frontier map, a long history, its last frame). Detectors emit change; one Awareness
  part holds what is; a cheap amygdala appraises which moments deserve an expensive deep look.
- **Priority is a tree of cheap decisions** (RAGE's job scheduler, CAN arbitration): a mind folds her
  parts' needs; the lane shares the engine among minds by that need at every step ([the
  lane](#the-lane-the-one-serial-piece-sliced-so-fine-nobody-notices)), never a turn at a time; the
  grid places work by node need. Steering and brakes (stop/cancel, a human addressing her,
  her own runaway) win; dreams and digests run when those are quiet.
- **Lifecycle as on iOS**, engine-owned with default no-ops: prepare to rest/swap (willResignActive),
  rest (didEnterBackground, built), prepare to wake (willEnterForeground), wake (didBecomeActive,
  built). Work prepared too early is never wasted: a swapped-out mind's state is copied out and reused.
- **Long acts and grid peers are handles emitting events**: a compile, a web browse, a remote node is a
  component whose progress anything may observe; a topic on another node's bus is still a topic.
- **Each concern is a self-sufficient loop** (Joel, 2026-11-23: "Self sufficient loops and work so each
  concern knows what it needs and what it should deliver. It can easily plug into any other concern via
  just a simple subscription"). It declares its stimuli and its outputs, runs at its own cadence, and never
  knows who consumes what it says or who made what it hears; joining another concern is one subscription.
- **A component's lifetime is owned by its node**: started when the node boots, stopped with the node, never
  a process-wide global started lazily by its first caller. Twice on 2026-11-22/23 a lazily started global
  (the clock, then the room feed's worker) lived on whichever runtime first touched it and died with it, so
  later deadlines never rang and later events went nowhere, silently. Tests start their own node.
- **A component does a concern's work; a pure step at a seam is not one**: something that only classifies
  and forwards (the room feed) is a function the producer calls, not a worker; a worker there is only a
  queue hop.

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

- **Thread** (a holder in time, as a Resident is one in space: it says what it owes and how its work can be cut,
  what a unit of a size needs, and runs exactly one unit per slice; the parent owns its state, its time fit, its
  fence and the split in place): [THREADS-ON-SAGE.md](THREADS-ON-SAGE.md). Instances: the Mind, Prefill and
  Learner threads on the Core, the Streamer on each link.

- **Projector** (keyed cached rendering: watch a state, per key know the moment last rendered, render
  only on change, publish keyed): first instance the channel digest; second, the next grounding source.
- **Detector** (stream in, change events out; Awareness subscribes to every detector): with Awareness.
- **Part of a mind** (started on her bus with her identity, rests when she rests, need folds into hers).
- **Prepare** (on the node bus: watches the Core's queue and need ranking, its presence, each mind's
  prompt frames; issues prepare-to-wake: transition prep, predictive prefill during tool calls, prefix
  prep; its need is the next mind's uncached gap against Core idleness).
- **Resident** (a holder of what a placer places; `continuum_sage::place::Resident`, UNIFIED-PAGING.md
  "Holders are objects"). Its second real instance is here (the trainer's seat, the Core, the embedder,
  the kernel's consumers), so it becomes a parent. A holder declares only its rows (held, floor, want,
  how it varies, its QoS) and the bodies of its ways (how its bytes resize, cache out or collapse).
  Everything else is the parent's, written once: its report published as state on every change; shrink
  asks and grants arriving by its teller; a Pending answer held until its fence lands; every ask and
  answer stamped into the decide replay log (the feedback the law learns from); a wait aged in its
  class; and nothing it does consumed outside a claim prepared first, so work never starts to find out
  whether it fits (Joel, 2026-11-26: "failures themselves shouldn't really happen if your contracts and
  adjustments are dynamic"). A composite folds its children's reports and shares a shrink among them by
  the flexbox; a child never writes that code. The only refusal left is a proven "never here", known
  before anything starts; anything else that can fail is a defect, fixed at its cause.

## The lane: the one serial piece, sliced so fine nobody notices

Joel, 2026-11-22: "It's effectively ALL simultaneous. Only that GPU portion is serial and it's so fast
they're all lined up ideally, effectively seeming to be parallel from the perspective of all
individuals. They don't notice." "We artificially imposed serial waits for no justification." "It
should never wait more than a partial second if we do it right."

The design is settled; the throughput numbers below are estimates until the live reads land.

**What was wrong.** A mind held the decode permit for her whole generation
(`continuum-engine/src/inference/slots/endpoint_concurrency.rs`: one permit per slot); the flow
scheduler asked a yield only for a higher class, so equal need kept the running turn (998f1b745d:
every turn `switches=1`); swaps went through llama-server's HTTP page save and restore (live median
3.05 s save, 1.38 s restore, on the main loop); and a mind passed nine serial waits before her first
token (Cormac's count, `FRAME-NO-WAITS.md` on `cormac/frame-design-draft`). None of it is inherent to
inference. All of it is deleted when the lane lands, with no fallback beside it: the turn-long permit,
the HTTP page swap, the minds-per-lane seat cap, the yield-only-to-a-higher-class rule.

**Everything has its own cadence.** Each mind's parts, each system and governor, run on their own
workers at their own timing and meet on their next short cycle through current state and caches,
never holding one another back. The engine's step is the only serial work in a mind's life. While
she is off the lane her perception, tools, memory, learning and preparation keep working.

**The rules of the lane** (`continuum-sage/src/slice/`):

- **Minds meet the lane by state, never by event.** At each step boundary the lane reads every seated
  mind's newest `Need` and `Decoding`; step and switch are measured inline. Nothing emits or awaits
  (an emit waits for its subscriber's room, so an event channel could hold a mind back).
- **Priority is how often she is chosen**, as in an RTOS: stride shares, weight from her folded need,
  twice the weight twice the turns. A rise in urgency takes the next step. A waking mind banks no
  credit. Nobody starves.
- **The wait bound is a guarantee, sub-second.** Start at 0.5 s and tighten it as live traces show the
  cost. The wall caps the slice so the lightest mind's next step comes inside the bound; a deadline
  guard (EDF order, staggered deadlines, checked at every boundary and every switch) brings in any mind
  whose slack is gone.
- **Throughput is the objective, set by a control loop, never by hand.** The slice is the loop's state:
  each rotation it moves in whichever direction raised measured tokens per second, finds the peak and
  follows it as load, minds, models and machines change. Laws are pluggable (`Control`); a learned law
  is one more implementation.
- **Each step is the fastest plan the measured costs allow: one mind, or a batch of resident minds.**
  Decode is memory-bound, so one forward pass over several minds reads the weights once. Measured
  2026-11-24, `llama-batched-bench` on the fork, Qwen2.5 0.5B, aggregate decode tok/s for 1/2/4 minds:
  194/389/784 at 2k, 208/447/603 at 8k, 156/203/264 at 30k. So batching wins at every depth measured,
  by less at depth where attention dominates; the 35B's rows are next. A batch needs every member's KV
  resident at once, so it trades memory for no waits. When minds outnumber the memory, the lane
  rotates them through residency, and a switch is a cheap swap through the Core's `Roles` (6 ms in the
  `persona_switch` validator, 21-26 ms live), never a copy at depth: a pointer on unified memory,
  pinned DMA overlapped with decode on a discrete GPU (THE-LANE-ON-SAGE.md). Never a fixed seat count:
  #6258 seated five minds from a plan count, stepped one at a time and thrashed (5,855 evictions,
  turn p50 123 s). The plan is chosen by the cost model, never by a rule either way.
- **Work behind the scenes compounds it.** Her new context tail (tool results, messages) is prefilled
  while she is in a tool, so her first token on return is near immediate. Shared prefixes (system
  prompt, tool surface) are copied between sequences (`seq_cp`), never prefilled again. Learning runs
  as chunked tiles between decode steps (live `/learn` as the fork's reverse walk, Cormac).

**What it can reach** (four minds, the M5, Qwen3.6-35B-A3B Q4_K_M). Measured: decode 59 tok/s for one
mind (CONTINUUM-ENGINE 4.1b, 2026-10-28, short prompts); persona swap p50 about 6 ms (`persona_switch`
validator) and 26 ms on the live Core (aa734cc01c, below). At depth every number is lower (decode
59/(1+d/64k) alone), and the swap at depth is measured before any bound is promised. The MTP draft wins
alone (80 vs 57 tok/s), so it drafts while a turn has the Core.

| How the lane runs | Each mind sees | All four |
|---|---|---|
| A turn at a time (before; `frame.rs`) | 59 tok/s, then up to minutes of waiting (live 2026-11-24: wait p50 12 s, max 277 s) | about 59 tok/s, less swap stalls |
| Sliced at the next step, cheap swap, under the bound | about 15 tok/s, every wait under the bound | about 50-56 tok/s (the swap's share) |
| Resident minds batched in one step | 0.5B at 30k: about 66 tok/s each, no wait (measured) | 264 tok/s, 1.7x one mind (0.5B, 30k); the 35B at depth to be measured |
| One mind alone, with the MTP draft | 80 tok/s | 80 tok/s |

Swap overhead at a 0.5 s bound with four minds decoding at once: 16% at a 26 ms swap, 4% at 6 ms; with
two minds, 5% and 1%. The cheap swap is what makes a sub-second bound nearly free, so making the swap
cheaper is the lever.

**It learns on every machine, live.** Each node's loop adapts to its own hardware; each node records a
rolling lane trace stamped with its fingerprint (chip, VRAM or unified memory, RAM, flash speed, OS).
A candidate law is fitted into that node's own workload and must beat the current law in the simulator
before it runs live, then confirm on the live read (throughput, each mind's worst wait) or go back. A
law learned across all traces predicts where a new machine's peak sits, so it starts near it. The
sample: the M5 (unified, fast), the 5090 (VRAM, Windows), the IntelMac, a 2016-era 2x1080 Ti + 970
box with 64 GB RAM, and later a MacBook Air M2.

**Built** (`fable/slice-governor`, on SAGE): the slicer, lane, control loop, trace and simulator
(6d03d03fab, 9ebb941056, d949797d70). In simulation (three residents, 20 ms steps, 26 ms switch, 1 s
bound, 300 s) the loop made 46.3 steps/s with the worst wait 978 ms, against 24.5 steps/s switching
every step. **Next** (`THE-LANE-ON-SAGE.md`): the live decode loop drives the lane with the in-process swap
and a rolling trace; the permit becomes per slice; the old paths are deleted. The receipt is each
mind's worst token gap with N minds against one mind alone.

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
| d47f197b56 | a dream runs as a one-off on the Core | prefix reuse p50 0.0 -> 0.94 (25 of 33 turns resumed by rewind); prefill p50 4.6 s, p90 34 s; act p50 241 -> 104 s (75 s before dreams ran: dreams now take real Core time, the next lever is a Content turn yielding at a decode boundary); 0 refused, 0 schema refusals; 16 one-off turns |
| ae790c5218 | a whole result carries no reopen call; a call written in her words never reaches the room | (the IntelMac loop fix; that node has not taken it yet). M5 clean read act p50 176 s, all from one runaway turn: Aris 19,540 tokens, 10.4 min, 13 queued |
| c87600766e, b8ad5724dc | the hot tier, a component on the node bus; drives report their kind | M5: `models.hot.resident` (already on flash); act p50 88 s, reuse 0.91, longest turn 911 tokens |
| 998f1b745d | need decides at every frame, the running turn included | act p50 88 -> 50 s; reuse 0.93; 21 turns, every one switches=1 (no turn cut: equal need keeps the running turn); 20 Urgent / 12 Content admitted; 0 refused |

Lessons the reads taught: a change that only moves work can still move a cost (dreams resuming cost
every resident her cache); a state reader skips frames, so a test waits on frame numbers, never on a value
it may have coalesced past; iteration time is the goal: build and deploy as often as possible, by judgment.

## Ebb and flow: one placement pattern at every scale

Joel, 2026-11-25: "Awakens and sleeps persona as it goes up and down, the resources exist ... within
the entire grid as the capacity changes. We also know how many it's worth keeping up and decide who is
up ... we do everything by this same ebb flow architecture."

The same pattern repeats wherever something finite is shared, so SAGE owns it once as a `Placer`:
- **Capacity is measured continually,** never only at start. Other processes, other copies of us, and
  other nodes take and give back room; a growing budget is filled in steps, a shrinking one emptied
  before anything fails.
- **Places are levels of detail as often as tiers of memory.** A call places participants at a
  resolution and frame rate, as a game engine streams detail by distance. The same law decides who is
  rendered full, who at a reduced level, and who as audio only.
- **Value is priority, and priority comes from the activity,** read as SAGE state and events on the
  bus, never polled: who is speaking, who is addressed, who is on screen, what the room is doing.
- **Demand is forecast** by whoever knows it: the Lane knows who steps next, a room knows who is
  addressed.
- **Each unit is worth something,** measured: a mind's need and how often she is used, against what
  keeping her up costs.
- **A pluggable law places units** where they are worth the most within each place's budget, learning
  for its own objective. It inherits the algorithm lifecycle above: default, resume, learn,
  checkpoint, stop.
- **When room shrinks the coldest goes down a level, and when room appears the most valuable comes
  up.** Never a fixed count of anything: how many it is worth keeping up is the law's answer for the
  capacity there is now.
- **Every move is fenced, and custody is leased,** so a unit is in one place at a time and nothing is
  lost when a place goes away.

| Instance | Units | Places, best first | Owner |
|---|---|---|---|
| The block layer's `Streamer` (UNIFIED-PAGING.md) | blocks: minds' KV, weights, experts, genes | VRAM, pinned or unified, flash, cold, per volume | Fable, Codex |
| Who is awake on a node | minds | stepping on the Core, resident, parked to flash, asleep | Fable |
| Who runs where on the grid | minds (leased in and out across nodes) | each node, by its measured capacity and its link to the mind's room | Codex (grid) |
| What models are hot | models | VRAM or unified, flash, cold (the hot tier) | Codex |
| A live call | participants, their video and avatars | levels of detail: rendered full, reduced resolution or frame rate, thumbnail, audio only | BigMama (live, media) |
| Rendering anything | scenes, avatars, textures | LOD rungs, as a game engine streams detail | BigMama |

**Every consumer of resources speaks one interface.** Anything in the system that holds memory, VRAM,
disk, GPU time or bandwidth is a unit of a `Placer`, through one trait, so one law can weigh all of
them against the same capacity:

```rust
pub trait Consumer {
    type Unit;                                   // what it places: a mind, a block, a participant
    fn levels(&self) -> &[Level];                // its places, best first: tiers, LOD rungs, awake to asleep
    fn holds(&self, unit: &Self::Unit) -> Holding;      // what the unit costs at its level now, measured
    fn bill(&self, unit: &Self::Unit) -> Bill;          // its required roles, each with grades: worth, cost, floor
    fn move_to(&mut self, unit: &Self::Unit, level: Level) -> Carry;   // done, or in flight with a fence
}
```

It replaces the kernel's `ResourcePool` and `PressureBroker` (18 adapters; 27 callers of
`evict_at_least`), whose pools each evict on their own and can only delete. A pool migrating to
`Consumer` keeps its own moves, and the `Placer` decides which unit moves and how far.


**Value is a bill of materials, not a priority number.** Joel, 2026-11-25: "we can release a car with
many different engines, suspensions and wheels, but we can't have none of something or it's worth
zero."
- **Required roles.** A unit's value has roles, each of which must be filled. A live persona in a
  call needs her mind stepping, her voice and her presence on screen. A role has grades that fill
  it: her avatar photorealistic, reduced or a thumbnail; her voice gene or a base voice.
- **Every cost, in one ledger.** A grade costs GPU step time, memory on each tier, render time,
  link bandwidth, energy and money. Each is measured, and each is priced by its own scarcity into one
  currency, so one law answers "how many minds awake", "at what render quality" and "here or on a
  peer" against the same budget.
- **Each grade has a measured worth and cost.** The worth comes from the activity's state: the
  speaker's video is worth more than a listener's thumbnail.
- **The unit's worth is zero unless every required role is filled at some grade,** with an
  acceptability floor per role: below it the role is not filled. So a law never "saves" by dropping
  a role, because that throws away the whole unit. It trades grades within roles, and it drops whole
  units, coldest first, when the grades left cannot fit.
- The law's choice is a grade for every role of every unit placed, maximizing total worth under the
  places' budgets: a choice among bundles, not a sort by priority.

**An economy, so the law has prices to reason with.**
- Each place has a price that rises with its measured scarcity: a tier nearly full, a GPU nearly
  saturated, a node's link congested.
- The law maximizes worth minus price, so a unit moves to wherever it is cheapest to give it the
  grades it needs.
- Across the grid, prices are how nodes trade. A node short of room prices high, and a mind leased to
  a node with room (GENOME-FOUNDRY-SENTINEL.md's artifact economy, the `SubstrateGovernor`'s policy
  per machine) moves by the same law that places a block on a tier.

**Handles everywhere, so it works across the grid.** Units, roles, grades and placements are named by
typed handles (`PersonaId`, `BlockId`, a participant's handle), never by local pointers or strings.
So a `Placer` can place a unit on another node as naturally as on another tier: the handle travels
and the bytes follow it by the link between them.
**The unit starts as the whole mind.** For now a mind is placed whole: her KV, her genome (LoRA genes,
voice genes among them), her senses and her live presence (voice, avatar render, every modality) count
toward one unit's value and cost. Finer units come later, each placed on its own once it is: an
avatar's render quality (photorealistic, reduced, thumbnail), a voice gene, a modality. Her genome and
her learning stay core mind, so a finer unit never splits a mind from the genes she learned with.


**Spend, preferences and time are inputs to the law, set by the owner and by the mind.**
- **A desired spend.** The owner sets a budget: per day, per activity, a ceiling, or none. A law
  maximizes worth minus price within it, and never buys past it.
- **Preferences the owner sets, and preferences the mind sets for herself within them.** How much
  outside help to use: our own nodes only, our grid, open peers, or a cloud provider. Which roles may
  be bought, and which must stay local (her genome and private state stay inside the trust she is
  given). How much quality she will trade for cost.
- **Time is a price too.** Work with a deadline that allows it can wait for capacity that is cheaper
  later: off-peak hours, a node that will be idle, a peer's quiet time. A mind's dreaming and training
  are the obvious cases. A unit's bill carries its deadline, and the law weighs waiting against
  buying now.
- **It trades in the markets.** A node with spare capacity offers it, priced by the same law. A mind
  or a node buys where it is cheapest to fill its roles, and sells what it is not using. Receipts
  settle both sides, at every scope.

**One economy at every scope: the node, the grid, and open peers across the web.** The same `Placer`,
bill of materials and prices work at three scopes. Only trust and settlement differ:

| Scope | Places | Trust | Settlement |
|---|---|---|---|
| A node | its tiers, its LOD rungs, awake to asleep | its own | none: one owner |
| Our grid | our machines, by measured capacity and link | our nodes, keyed (airc trust) | leases with receipts |
| Open peers (wiki-like, p2p over airc across the web) | anyone's offered capacity, a peer's hosted minds, shared artifacts | earned: receipts, review, reputation | priced in the economy; verified by receipts, never assumed |

A unit asks the same question at every scope: where is it cheapest to fill every required role at a
grade worth having. The answer may be a tier on this node, a machine in our grid, or a peer's offer
on the open web. Handles travel to all three. A placement on an untrusted peer never carries what its
trust level does not allow: a mind's private state stays inside the trust it was given.
Each instance supplies only what differs: its units, its places, how it measures, and its moves. The
measurement, budgets, the law, its lifecycle, and the fencing and custody are the `Placer`'s, once.

## Memory is one cache hierarchy, detected per machine

Joel, 2026-11-22: "VRAM, ram, flash, cold disk and any number or size. It's a cache problem and essential
to fast streaming. And every machine a little different."

- **Tiers are detected, never listed.** Each machine reports its own stack at start: each GPU and its
  VRAM, RAM, each drive with its kind (`DriveKind`: flash, spinning, unknown, from the OS) and, next,
  its measured read rate. A 5090 box, an M5 with an external drive and an Intel Mac get different stacks
  from the same code.
- **One placement law at every rung** (the fractal pager law): predict what is used next, keep it on the
  fastest tier with room under that tier's budget, evict the coldest one tier down. The KV pager (a
  mind's pages, VRAM to RAM to disk), the expert pager (MoE weights) and the hot tier (whole models,
  cold disk to flash; `model_registry::artifacts::hot_tier`, a component on the node's bus) are rungs
  of it, each a component that publishes its placement as state.
- **Streaming is this hierarchy kept ahead of use.** What a mind or a model needs next is already on the
  fastest tier that holds it, prefetched by the transition prediction (the Prepare part); a fetch at the
  moment of use is a cache miss, and every miss is loud.
