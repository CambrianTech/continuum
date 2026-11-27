# In-process learning on the persona engine

Owners: Fable (fork walk, llama crate, driver), Cormac (trainer, authority side), Codex (serving, admission,
review). Read with [ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md); the
learner is a stage run on the lane ([STAGE-CONTRACT.md](STAGE-CONTRACT.md)), its memory is held through the block
layer and the authority ([UNIFIED-PAGING.md](UNIFIED-PAGING.md), [SHARED-RESOURCES.md](SHARED-RESOURCES.md)).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The shape](#the-shape)
- [A reply learns as the exact walk, in tiles](#a-reply-learns-as-the-exact-walk-in-tiles)
- [Pricing a tile](#pricing-a-tile)
- [Fitting: smaller or later, never refused](#fitting-smaller-or-later-never-refused)
- [The learner on the Core](#the-learner-on-the-core)
- [Gates](#gates)
- [Open questions](#open-questions)

## The shape

A mind learns one reply on her own live serving sequence: the reply's rows are trained against the lesson already
in her cache, then she is restored under her stable gene set, so serving finds her exactly as it left her. Other
minds decode between the learner's units.

Built:
- the `Learner` over a context in the llama crate (`core/llama/src/safe/learn.rs`), its refusals typed from
  `llama_opt_failure`;
- `Job::Learn` on the driver (`persona_scheduler/driver/learning.rs`), interleaved with turns;
- the trainer reaching it through `LearnPort` (`continuum-dreaming/src/modules/training_trigger/on_engine.rs`).

Still to delete: llama-server's learn endpoint (`tools/server/server-context-learn.inc`) and the HTTP adapter, once
no caller remains.

## A reply learns as the exact walk, in tiles

**Why tiles.** A reply's backward holds every reply row's activations against every lesson row it attends. Learned
as one graph, Kimi's lessons measured 64.4 GB, 91.5 GB and 1.9 TB on a 34.2 GB card (the 5090, 2026-11-26). The
reply is what is trained and is never cut, so the graph is cut instead: the reply learns in tiles of rows.

**Why exact.** A later row's loss flows back through the earlier rows' K/V and recurrent state. Independent tiles
lose that. The fork's exact walk carries each tile's gradient into the cached rows before it, through a host K/V
gradient accumulator and recurrent checkpoints, so the tiled update is the whole graph's update.

**How it runs** (`llama-context-learn-tiles.cpp`, fork `fable/context-memory-rows`):
- forward, one tile per unit: decode under the shadow, a recurrent checkpoint every `stride` tiles;
- reverse, last tile to first: rewind to the checkpoint, replay to the tile, train it with its gradient carried
  into the tiles before; never below the served prefix;
- one optimizer update with the whole reply's label denominator, then the restore;
- `stride` comes from the host budget (`plan.stride_for`): fewer checkpoints, more replay, same gradient;
- every unit is bounded by its tile, replays and prefix decodes included (pending spans, fork fa85c3379), and
  reports `{ decoded, trained }` (`llama_opt_learn_last_unit`, fork 1eb66d1db).

**One path.** A reply past one ubatch learns only through this walk. The untiled step had let `decode()` split a
chunk into ubatches with a backward each, a plain walk whatever `walk_exact` said: cosine 0.949 / norm 0.889 on
Qwen3.5-0.8B and 0.960 / 0.944 on Qwen3.6-35B-A3B against the exact step at ubatch 64 (M5 Metal), on every live
reply past the Core's `n_ubatch` (2048). Fixed in the fork (398bf318d, awaiting review before the submodule pin
moves) and the binding (`Walk` has no `exact` field: the walk is always exact). Proposed next: the fork's learn surface collapses to begin, cost at a unit, run a unit; the
whole-reply graph is the walk at `unit >= reply`, and the other entries are deleted (STAGE-CONTRACT.md, "Units").

**Measured** (64-row tiles against the exact step):

| Backend | Model | Result |
|---|---|---|
| Metal (M5) | Qwen3.5-0.8B-Q8_0 | cosine 1.0000; stride 1: 8 units, 440 rows; stride 4: 17 units, 824 rows; max 64 rows per unit |
| Metal (M5) | Qwen3.6-35B-A3B-Q4_K_M | 0.9976, equal to the untiled learn (production layout) |
| CPU (IntelMac) | Qwen3.5-0.8B, Qwen2.5-0.5B | 1.0000 in all four cases (Cormac, 1eb66d1db) |
| CUDA (5090) | Qwen3.5-0.8B | open: with CUDA visible, both the whole-reply and the tiled update differ from CPU's (cosine 0.31 and 0.33), while CPU-only on the same binary is exact (0.9999); so CUDA's backward is wrong, not the tiling (Codex, 398bf318d) |

## Pricing a tile

Proposed. Today `measure_within` (`driver/learning.rs`) prices the whole-reply graph (`opt_measure_memory` at
`chunk = reply_rows`), which is Kimi's 1.9 TB. The walk is priced instead at its worst tile, the reply's last at
the full horizon (largest attention window, longest gradient tap), built with the walk's own flags and allocating
nothing. It reports two rows, never one sum: the device workspace of that graph, and the host accumulator and
checkpoints from the walk's own formulas at its host budget, so measure and run cannot disagree. Every other tile's
graph is no larger.

## Fitting: smaller or later, never refused

Proposed; each replaces a refusal in today's code.
- **A tile too large splits in place.** Its rows are decoded and its checkpoint held, so its reverse trains it as
  halves, last to first, down to one ubatch: the same exact step on a smaller graph. Replaces "a tile did not fit:
  learn this reply at a smaller tile".
- **The host budget sets the stride**, never a refusal. Replaces "past its budget". The budget is the seat's Ram
  lease from the authority, so stride and lease cannot drift (Cormac).
- **A shared cache never reaches the walk.** The Core is one stream with one bound occupant (`STREAMS = 1`,
  `continuum-persona-llama/src/unified_tier.rs`; the same on the 5090, Codex), checked when the context is built.
- **A shrink resizes before it releases.** A target at or above the floor tile's workspace is answered by a
  smaller tile from the next unit; only a target below it checkpoints and releases (Cormac).

## The learner on the Core

Proposed: the learner is a stage whose unit is a tile, scheduled by the lane like a mind's step
(STAGE-CONTRACT.md, "Units"; THE-LANE-ON-SAGE.md).
- Its class is `Content`: no per-unit bound; its guarantee is its queue kept in check (`Urgency::act_bound`).
- What it owes is state, latest-wins: `learning/owed`, `Owed(BTreeMap<SeatId, LearnerOwed>)`, a seat leaving the
  map is its Done (Cormac's, in review).
- Its share of the Core is what the scheduler's choices add up to, from feedback, never a constant.
- Its scratch is one claim on the authority held per tile size and resized when the size moves, not asked per unit.
- Between units its shadow, optimizer, checkpoints and accumulator stay resident and are reported like any holding.
- What counts is the optimizer update: provisional per update id until its generation commits, written off on
  restore or cancel (Cormac, Codex). Tiles advanced are never a reward.

## Gates

1. `test-walk-exact` on Metal, CPU and CUDA (0.8B) and on the 35B on Metal (the M5's production form): tiled
   against untiled on the same backend at the model's measured floor, every unit within its tile.
2. The tile measure is at least the workspace each tile actually admits, at 64, 128 and 512 rows.
3. A tile too large learns at a smaller one, never an abort. This replaces the suite's device-refusal case ("a
   refused chunk graph retried at a smaller horizon"), which tests a refusal and a retry; it fails today on Metal
   and on CPU (where nothing is refused) and is rewritten to split in place.
4. Kimi's 64 GB, 91 GB and 1.9 TB lessons learned on the 5090 beside serving, held-out loss kept.
5. A loaded window on the M5 with the 35B: the learner's queue in check, no resident past her bound, every unit
   recorded, zero `measure_defect`.

## Open questions

1. The accumulator's tensors in the measure: a CPU buffer over a pointer never read, or a graph input the measure
   marks host-resident (Codex)?
2. CUDA's 0.498 on the 0.8B: which op, per the bisect with op offload off.
