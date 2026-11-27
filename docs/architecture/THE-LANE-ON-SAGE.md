# The lane on SAGE: one Core, every mind served at the next step

Who steps on the engine next, and for how long. Owners: Fable (the lane, the Core), Codex (serving, the discrete
GPU), Cormac (learning's share). A swap's memory is the block layer's ([UNIFIED-PAGING.md](UNIFIED-PAGING.md)); what
the lane schedules is stage runs whose unit it chooses ([STAGE-CONTRACT.md](STAGE-CONTRACT.md)); the decision
interface is SAGE's `decide` ([SAGE.md](SAGE.md)).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The contract](#the-contract)
- [How the Core works](#how-the-core-works)
- [Where it stands](#where-it-stands)
- [The decision](#the-decision)
- [The algorithms](#the-algorithms)
- [Other use cases on the same interface](#other-use-cases-on-the-same-interface)
- [Use every core: the GPU never waits on the CPU](#use-every-core-the-gpu-never-waits-on-the-cpu)
- [How a change is judged](#how-a-change-is-judged)
- [The order](#the-order)
- [Open questions](#open-questions)

## The contract

- **Minds outnumber the Core.** A node serves a society (three to six residents is normal) on one engine; the
  steady state is many minds and one Core, so sharing it is the design, never an exception.
- **No ready mind waits past her bound.** `Urgency::act_bound`: one second for `Want` and `Urgent`. `Content` work
  (learning, dreaming) has no per-item bound; its guarantee is its queue kept in check.
- **Quality never yields to latency.** The served model serves every class; a slower class waits its turn, never a
  smaller model.
- **Callers carry no policy.** `ai/inference/*` names what to run, never slots, batching, quality or paging.
- **Nothing is fixed in advance.** No seat count, no pool sized from a plan, no quantum: who steps and for how long
  falls out of what continuing and switching are each worth, at this step, on this machine. A five-seat pool (#6258)
  paged 4,191 times in 15 minutes; the whole-turn rule it replaced gave a wait p50 of 12.1 s and a max of 277 s.

## How the Core works

The Core is modeled on a CPU core: minds are processes, and a mind's native state (KV rows, recurrent state) is her
register file.
- **One context, one stream, one bound occupant** (`continuum-persona-llama/src/unified_tier.rs`, `STREAMS = 1`).
  Every other mind's state stays in her own storage, as she left it.
- **A switch is a transfer**: a pointer on unified memory (0.01 ms on the 35B, KV and recurrent state), DMA
  overlapped with decode on a discrete GPU, one block to flash. Never a copy loop, never an encoding
  (UNIFIED-PAGING.md, "A mind's memory").
- **One mind or a batch per step, by measured cost.** Batched decode in one graph measured faster at every depth on
  the 0.5B (aggregate for 1/2/4 minds: 194/389/784 tok/s at 2k, 156/203/264 at 30k). A batch needs each member's
  state bound at once, which the one-stream Core does not yet do (open question 1).
- How many Cores a node has is its measured capability: one on the M5.

## Where it stands

Built:
- the pointer switch (#6264) and recurrent state by pointer;
- the lane in serving (#6280): `persona_scheduler/driver/lane.rs` is the executor, and the decision is
  `continuum_sage::decide::cases::lane`, an algorithm created by name from `cases::lane::algorithms()` (`baseline`,
  `need-first`, `cost-model`, `linear-ucb`), the serving default `cases::lane::SERVING` = `linear-ucb`; an unknown
  name refuses the engine's start;
- the lane's own history (`cases::lane_history`): the driver reports each switch-in, reuse, step (tokens and time,
  prefill and decode apart), turn end, residency and departure; what is not measured stays `Unmeasured`;
- learned state saved per served model at every stop and resumed, seeded from a measured step when none exists;
- the whole-turn rule (`frame.rs`) deleted.

Not built: the `Batch` action is chosen but not executed (`driver/lane.rs` maps it to no stepped mind); the next
step's inputs prepared while the current one runs (`Roles` overlap); the discrete GPU's DMA path; the learner and
prefill as stage runs with units; `persona-lane-bench`.

Delete: `continuum_sage::slice::{Slicer, Lane, Control, Tuner}`. The engine does not use them; the live lane is the
`decide` use case above, and two lanes is the defect. Its stride-share and deadline ideas live in the lane's
envelope; `slice::sim`, `slice::trace` and `slice::room` stay as the simulator.

## The decision

At every step boundary the lane chooses one action from an extensible set: `Continue`, `Switch(mind)`,
`Batch(minds)`, `Idle`. A mind's run length is an outcome of these choices, never a parameter.

- **The envelope** (never learned, never overridden): no ready mind past her deadline, earliest deadline first, an
  addressed mind's shortest; nobody starves; a mind whose predicted remainder is under one switch finishes first. A
  policy chooses only inside it, so a policy that explores or is wrong costs throughput, never a mind's wait.
- **What a policy estimates**, over the next window: continuing (her progress per engine-second in her phase, and
  what is left of her turn from her own history); switching to another (her progress plus the waiting it relieves,
  steepening near each deadline, less the switch's measured cost); batching (the measured aggregate rate at that
  width and depth, less the per-mind rate it dilutes).
- **A switch's cost is measured, per pair and node**: the pointer move, the graph rebuild for her next token
  (+0.1-1 ms), her cold pages at the measured fault rate, the warmth the previous mind loses, and any DMA not hidden
  under decode. So a deep cold mind is worth keeping longer once in, and a shallow warm one is cheap to rotate.
- **Shares are an output.** How much of the Core each class gets is what the choices add up to, from feedback
  (slack against the bound for bounded classes, queue growth for content classes), never a constant weight.
- **The unit is chosen with the step.** A prefill chunk or a learner's tile is sized to the slack it is given
  (STAGE-CONTRACT.md, "Units"); a 2,048-token chunk is about 3.2 s cold on the 35B, so at depth a chunk is smaller.

The XNU Clutch/Edge scheduler separates mechanism from policy the same way, but its quantum is a fixed table per
class; ours can be the decision itself, because we know what a kernel cannot: each switch's exact cost, each mind's
exact residency, her turn-length history, and batching as an action.

## The algorithms

Each is created by name, has typed parameters (refused out of range, never clamped), and saves its learned state as
bytes, through SAGE's algorithm lifecycle:
- **`baseline`, `need-first`:** the comparisons, never production on their own.
- **`cost-model`:** the running mind continues while her next step, or her whole predicted remainder, fits a
  `margin` share of the tightest waiter's slack (her bound, less her wait, less her measured switch cost); otherwise
  that waiter comes in. The margin is a control loop both ways (Rprop), resumed per model.
- **`linear-ucb`** (serving): `decide::Ridge` under `decide::LinearUcb`, a constrained upper-confidence bandit over
  14 features (times against the bound, phase, each candidate's predicted finishing rate) with two physical targets:
  the share of the stepped mind's remaining phase finished per second (maximized: least work left finishes first)
  and the worst wait against its bound (kept at or under one by its pessimistic prediction). No weight trades one
  against the other. In simulation (four minds, 300 s): 356.1 tok/s of Core, worst wait 1.04 of the bound, turn p50
  15.3 s, against 26.3 s for `need-first`; its prediction error on chosen actions fell from 0.014 to 0.001.

Two rewards were tried and dropped, and why stays: tokens per second rewarded switching to whoever prefilled (a
prefill token costs a fraction of a decode step); the Core's share not spent on transitions made a 1 ms switch look
free, so the learner rotated minds every step and turn p50 rose to 117 s against 17 s (M5, build 82750a4e27).

Policies are compared on the simulator fitted from a trace, then live inside the envelope; a replay never pretends
to know the outcome of an action not taken.

## Other use cases on the same interface

The lane is the first `decide` use case; each other one declares its situation, actions, outcome and envelope, and
gets the same records, replay and learned lifecycle:
- **prefetch** (built): when a parked mind's move starts (UNIFIED-PAGING.md, "The Streamer");
- **residency, placement, volume** (scaffolds, `Baseline` as policy): keep or demote a mind's state; run here, on
  another machine of mine, or on a leased peer, priced with transfer, restore, queueing, lost warmth and price; put a
  hot working set on the fastest volume with room, never a slower one, moving cold duplicates down instead;
- observed capacity is never committed capacity: an idle remote GPU is a candidate until a lease says what can be
  counted on and until when.

Cross-grid serving is built as an adapter: `continuum-engine/src/inference/airc_remote/adapter.rs` implements the
provider interface over airc, so a mind on the M5 is served by the 5090's warm model as if locally; where work runs
is the placement use case's decision.

## Use every core: the GPU never waits on the CPU

The only serial work is the GPU's step. The next step's inputs are ready before the current step lands, like a frame
pipeline that double-buffers: the next mind's switch and graph, her new tail tokenized, detokenizing and stop
sequences (Emit), probe rows (Record), prompt assembly, prefetch, embeddings and learning preparation, each on its
own worker, vectorized with the platform's libraries (BLAS: Accelerate, MKL, OpenBLAS; SIMD for per-element loops).
The engine thread only submits steps, reads results and switches.

Gate: `busy_permille >= 950` while any turn waits (`persona.engine.occupancy`; it read 558 on the M5, 2026-11-25).

## How a change is judged

By how long each resident waits between her own acts at the live roster, prefill included, and her turn time: never
one stream's decode rate, never one call's aggregate tokens per second. A change that lowers waits but raises turn
p50 (the rotating learner above) is worse. Each change carries the 15-minute live read beside its simulator result.

## The order

| Slice | Gate |
|---|---|
| `persona-lane-bench`: the real Core with N minds at 2k, 30k and 130k, mixed turns | reproduces the live shape; switch time by depth |
| Delete `slice::{Slicer, Lane, Control, Tuner}`; the lane's envelope is the only one | the lane's tests and the simulator unchanged |
| Steps and units sized to the slack; `Roles` overlap in the driver | worst wait within the bound at four minds; turns at least the baseline's |
| `Batch` executed where the cost model chooses it | aggregate need-weighted tok/s above switching on the same trace |
| The discrete GPU: pinned memory and DMA overlapped with decode | the 5090: step time unchanged with a switch in flight |
| Her new tail prefilled while she is in a tool | submit to first token under 1 s after a tool result |

## Open questions

1. **Batching on one stream.** A batch needs several minds' state bound at once; the Core is one stream with one
   bound occupant. Bind several minds' storage into a multi-sequence graph, or keep one stream and batch only minds
   whose state fits beside the occupant's?
2. **Chunked prefill under the bound** (Codex): does any caller rely on a turn's prefill landing in one piece?
