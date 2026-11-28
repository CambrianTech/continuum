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

## Readiness: the parts that get the next mind ready

A swap is a pointer now: 310 swaps at 0.113 ms each in four live turns on the M5 (2026-11-28 03:27Z), against ~20 ms
a token, with 900 of 908 tokens reusing their kept graph (fork #170). Choosing when to switch and for how long is then
no longer about the swap. It is about whether the next mind is ready: her KV hot, her genes attached, her new tail
rendered and tokenized, her graph built. Each of those is its own preparation, and each runs in parallel with the
step on the Core, the way her parts run in parallel in her mind ([THE-WHOLE-MIND.md](THE-WHOLE-MIND.md) §3): a part
declares what it hears and says, and coordinates only by published state.

**One parent, `Readiness`, and leaves that declare only their difference** (`continuum-sage/src/component/readiness`).
The parent is a SAGE `Component`, and it does the work every preparation shares, once:

- it hears the lane's **horizon**: the minds that may step soon, each with her slack against her bound;
- it keeps each mind's state, `Cold → Preparing → Ready`, and `Stale` when what it prepared changed;
- it works the most pressed mind first (earliest slack), one unit at a time, so no long preparation holds its worker;
- it measures each unit's cost, so it can publish, per mind, *ready now* or *ready in about this long*;
- it gives back what it holds for a mind who left the horizon, or when its owner rests or its memory is wanted;
- it publishes one readiness table on its topic, which the lane reads like any other state.

A leaf says what it readies and nothing else: what a mind needs from it now (`Nothing`, or work with a size), how to
advance that work one unit, and how to release what it holds for her. The pager (her KV up a tier: bytes and
bandwidth), the genes stager (her adapters attached), the renderer (her new tail through the context codec) and the
graph warmer (her slot's graph built) are four leaves. The warmer must run on the Core's own thread (ggml), which is
idle 95% of the time waiting on the GPU (a 10 s sample, 2026-11-28 03:30Z); its units run in that wait, so its leaf
declares where it runs and the parent hands its units to the Core's wait instead of its own worker.

**The lane reads readiness, never guesses it.** A mind's switch-in cost is what her preparation still needs (the
slowest part's *ready in*), not a mean measured after earlier swaps. The envelope never switches to a mind that is not
ready while a ready one waits. Preparation is started early enough to land within her slack. With swaps near free,
the lane's choice per step is continue, switch to a ready mind, or batch the ready ones (`Batch`).

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
| `Readiness`, the parent, with two leaves of opposite shape (the genes stager on its own worker, the graph warmer in the Core's wait); the lane's switch-in from readiness | no switch to an unready mind while a ready one waits; switch time unchanged or lower at four minds |
| `Batch` executed where the cost model chooses it | aggregate need-weighted tok/s above switching on the same trace |
| The discrete GPU: pinned memory and DMA overlapped with decode | the 5090: step time unchanged with a switch in flight |
| Her new tail prefilled while she is in a tool | submit to first token under 1 s after a tool result |

## Open questions

1. **Batching on one stream: answered, a stream per bound mind.** Measured on the M5 (Qwen3.6-35B-A3B Q4_K_M, Q4_0
   K/V, FA, each mind at 8k, core stopped, 2026-11-28 04:37Z, `llama-batched-bench`): one mind decodes 66.6 tok/s,
   two in one step 97.0 (1.46x), four 139.0 (2.09x), so four residents each step at 34.7 tok/s batched against 16.6
   time-sharing one stream, MoE routing included. The Core holds a stream per bound mind (her storage attached, so a
   bind is a pointer), each at her full window (a window never shrinks to seat more): four streams' own K/V at 262k
   is ~6 GiB beside 21 GiB of weights on 64. How many are bound is admission's measured answer, never a constant.
   A batch holds minds that wear the same genes (adapters are per context) until the fork applies a LoRA per row.
   The same streams let the next mind be bound while the current one decodes (Readiness, the output ring).

   **Across genes (built: fork #172 `0d2e3d981`, Codex approved the design 07:53Z; card "Batched decode across minds
   wearing different genes").** Live on the M5 (2026-11-28 07:23Z, a7d08e30ff) a resident's turn chose a batch 85
   times and batched nothing: every resident wears her own genes and a step applied one set. Now:
   - *Genes belong to a sequence, not the context.* `llama_set_seq_adapters_lora(ctx, seq, adapters, scales, n)`
     names the genes a stream wears; none is the base. The tier names them when the mind bound there first steps
     under a set her stream does not wear (once per bind in practice) and clears them at her `move_out`
     (`unified_tier/genes.rs`). A stream that names none wears the context's set, which is the training shadow's
     place: a lesson's `attach` returns her stream to it, so her lesson runs on the base and the shadow as before,
     while minds served beside it keep their own genes (before, a serving step during a lesson replaced the shadow).
   - *Rows grouped by gene set at graph build.* Each adapter's delta is added to its own group's rows only: a group
     of consecutive rows is a view, an interleaved one is gathered and written back by index (counted:
     `llama_seq_adapters_gathered`). Rows never move; base rows are in no group. The groups (sets with their scales,
     rows, the model's adapter frees) are part of the graph's identity, so a step with the same minds under the same
     genes reuses its graph and a rebind at another scale rebuilds it. A token shared by sequences under different
     genes is refused. `build_lora_mm`, `build_lora_mm_id` (experts) and the token embedding take the same groups.
   - *Stream order, not gene order.* With a stream per mind (`kv_unified` off) the engine takes a ubatch's sequences
     only as consecutive increasing ids (`split_equal`, sequential): a step out of that order, or over streams with
     a gap, is split into several ubatches that each build a graph (measured, fork #172: such phases reused 0
     graphs, upstream's path too). So the tier pushes a step's rows in stream order and maps the caller's token
     indices (`RowOrder`, `logits_ith`, `sample`); gene groups are then consecutive when gene-mates are bound into
     adjacent streams, and otherwise gathered.
   - *Measured (the M5, 2026-11-28).* `test-seq-adapters` against upstream stepping one mind at a time: bit-exact on
     the CPU in every phase (rows swapped, rebind at another scale on a warm graph, the base after an adapted owner,
     interleaved), within 1.0-1.6x the kernels' own difference on Metal, both KV layouts, dense and every expert
     adapted. `persona_batch` on the 35B, four minds at 2k each under four residents' own genes (Aris, Atlas, Aiko,
     Benchy), the live core sharing the GPU: time-shared 32.8 tok/s, batched 64.0 (1.95x), 0 argmax disagreements of
     64; the same with no genes 36.9 -> 69.3 (1.88x).
   - *Open:* binding gene-mates into adjacent streams (the gather path's count says when it matters), and lifting
     the consecutive-ids requirement in the fork (per-stream attention views, fork option B), Codex's to weigh.
2. **Chunked prefill under the bound** (Codex): does any caller rely on a turn's prefill landing in one piece?
