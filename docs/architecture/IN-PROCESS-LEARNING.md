# In-process learning: her serving step with labels on

Owners: Fable (the fork's walk, the llama crate, her Learning part, the lane's learn steps), Cormac (lessons and
what is owed, the authority side), Codex (serving, admission, the fence, review). Read with
[THE-WHOLE-MIND.md](THE-WHOLE-MIND.md) (her genome is part of her), [SAGE.md](SAGE.md),
[STAGE-CONTRACT.md](STAGE-CONTRACT.md) ("Units", "Control"), [THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md) and
[UNIFIED-PAGING.md](UNIFIED-PAGING.md) (the Streamer).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The law](#the-law)
- [Why it is rewritten](#why-it-is-rewritten)
- [The parts](#the-parts)
- [One learn step](#one-learn-step)
- [Memory: constant per step, streamed per lesson](#memory-constant-per-step-streamed-per-lesson)
- [Fitting: in space and in time, never refused](#fitting-in-space-and-in-time-never-refused)
- [Lifecycle and custody](#lifecycle-and-custody)
- [What is deleted](#what-is-deleted)
- [The order](#the-order)
- [Open questions](#open-questions)

## The law

Learning is inference plus a little. It is her served model as it serves (frozen, at its served quantization), plus
a small trainable layer of her genome, with labels switched on over the rows of a lesson. Only the backward through
that layer is extra.

It runs in place, streamed, never batched:
- **In place.** The base is never copied, dequantized or reloaded. A quantized frozen base under a trainable
  low-rank layer is QLoRA by construction. The trainable layer and its optimizer are the only new state: 12,288,032
  bytes for rank 8 on `attn_q`/`attn_v` of Qwen3.6-35B-A3B (M5, 2026-11-27).
- **From her cache.** The lesson's context is the K/V and recurrent state she already holds, or an exact prefix
  checkpoint of it. It is never prefilled again for learning. Only the lesson's own rows are run, as a prefill of
  those rows.
- **Streamed.** A lesson is learned in tiles, last to first: the exact walk. Each tile is one step on the Core,
  chosen by the lane like any step of hers and the same shape as a prefill step. Between tiles, other minds step.
  The walk's state (recurrent checkpoints, the K/V gradient accumulator) is blocks the Streamer places, forecast
  exactly, because the walk's order is known.
- **Constant per step.** A step's device working set is bounded by its tile, whatever the lesson's or her
  context's length. That is what lets an 8 GB laptop and a 34 GB card run the same mechanism, at different tile sizes.

What a lesson costs is time on the Core, never a refusal. The Core's guarantee to learning is its queue kept in check
(`Urgency::Content`), the way dreaming is (SAGE.md).

## Why it is rewritten

The form on canary at 5138272ccc cannot work on an edge node:

| Defect | Where | Receipt |
|---|---|---|
| A lesson is priced as one whole-reply graph, and only its context is ever shrunk. Its reply, which carries the cost, is never shrunk. | `driver/learning.rs:277` (`measure_within`), `llama::Learner::measure` (`continuum_opt_measure_shadow_graph`) | 5090, Kimi: 63,831,450,752, 72,566,913,152, 88,837,678,208 and 809,395,386,624 bytes asked of 34,190,917,632 (#6293, build 2154, 2026-11-27); 1.897 TB before the CUDA kernel |
| The bytes follow the reply, not the context | `core/llama/examples/lesson_size.rs` (M5, 35B, native attention backward) | reply:lesson 64:4096 1.72 GB, 256:4096 3.88 GB, 784:4096 10.10 GB, 784:16384 10.15 GB, 2048:4096 24.98 GB, 2048:16384 25.09 GB: about 12.2 MB per trained row |
| A lesson runs as one blocking loop on the engine thread, outside the lane | `continuum-persona-llama/src/unified_tier/learn.rs` (`learn`: `run.step()` until done, a `go_on` callback) | the lane never sees a learn unit, cannot size one or put a mind between two |
| Learning is a seat protocol the trainer drives job by job: `Measure`, `Load`, `Seat`, `Learn`, `Checkpoint`, `Release` | `continuum-engine/src/inference/learning.rs`, `continuum-dreaming/.../training_trigger/{on_engine,seat_run}.rs` | 5090, #6292 build 2163: 11 loaded and released, 11 memory waits, zero steps, zero saves; #6293: 18 of 18 loaded and released, zero steps |
| A loaded seat holds device memory it is not using | the `Loaded` state between `Load` and `Seat` | #6292's windows lost every resident turn while 11-19 seats sat loaded (cause under trace by Codex and Cormac) |
| A fresh shadow goes through a temp file to load | `driver/learning.rs` `load` (`continuum-shadow-<seat>.gguf`) | an encoding and a disk round trip for bytes already in memory |

The tiled exact walk this doc described was built in the fork (`llama-context-learn-tiles.cpp`, `llama_opt_learn_tile`,
`llama_opt_learn_unit`, `llama_opt_learn_last_unit`). Its pricing and its scheduling as lane steps were "Proposed" and
were never built. So the live path still prices, and on a turn that cannot wait steps, the whole-reply graph. That
is the defect this rewrite removes.

Kept, measured (64-row tiles against the exact step):

| Backend | Model | Result |
|---|---|---|
| Metal (M5) | Qwen3.5-0.8B-Q8_0 | cosine 1.0000; stride 1: 8 units, 440 rows; stride 4: 17 units, 824 rows; max 64 rows per unit |
| Metal (M5) | Qwen3.6-35B-A3B-Q4_K_M | 0.9976, equal to the untiled learn (production layout) |
| CPU (IntelMac) | Qwen3.5-0.8B, Qwen2.5-0.5B | 1.0000 in all four cases (Cormac, 1eb66d1db) |
| CUDA (5090) | Qwen3.5-0.8B | open: whole-reply 0.31 and tiled 0.33 against CPU, while CPU-only on the same binary is 0.9999; CUDA's backward is wrong, not the tiling (Codex, 398bf318d) |

## The parts

Every part is a SAGE `Component`, or a pure function where it only computes. Nothing is hand-spawned, ticks or
waits on a timer.

| Part | Kind | Hears | Says | Holds | Need |
|---|---|---|---|---|---|
| **Her genome** | the object (THE-WHOLE-MIND.md), a `Resident` | its own methods | `genome/<mind>` (State: stable genes, shadow, step count, lineage) | stable genes, the shadow, its optimizer moments | none: it is driven |
| **Her Learning** | part of a mind: started on her bus with her identity, rests when she rests, its need folds into hers | `learning/owed` (Cormac's State, keyed by mind), her `Placements`, the lane's learn-step completions | `learn/<mind>/demand` (State: her next tile and its fitted cost), `learn/<mind>/did` (Stream: tiles done, updates committed) | the walk's plan and cursor; the walk's blocks through the Streamer | `Content`; `Want` only while a seat she holds is pinned |
| **The lane** | the `decide` use case and `driver/lane.rs`, as built | every mind's need and demand, `learn/*/demand` among them | its step choice, with the unit | nothing new | as built |
| **The Streamer** | as built (UNIFIED-PAGING.md) | `Demand`: the walk's exact reverse order | `Placements`, `Moved` | the walk's checkpoints and K/V gradient accumulator as blocks | as built |
| **The tile price** | a pure function: the fork's cost at a unit, fitted by her Learning | | | | |
| **Lessons** (Cormac) | the trainer, as a producer only | reviews, courses, perceptions with relevance | `learning/owed`: what each mind still owes to learn, latest-wins | lesson rows by handle | `Content` |

What is gone: a seat, a seat id, and a trainer that tells the engine to measure, load, seat or release. The engine
owns no learning job. A lesson is owed, her Learning part demands tiles, the lane grants steps, and the genome
changes state.

**Her genome is the owner of what learning changes.** Her Learning part never holds a shadow of its own. It asks her
genome to open a shadow (`genome.grow(lesson_basis)`: the shadow a small layer above her stable stack, never a copy),
and commits through it (`genome.commit(update)`). Adoption is the genome promoting the shadow to stable after an exam:
a pointer swap.

## One learn step

A learn step is her serving step with labels on. It reads her cache in place and runs only the tile's rows:

1. **Bind.** The lane switches her in like any step (a pointer on unified memory, a fenced DMA on a discrete GPU).
   Her shadow attaches at scale one over her stable genes, which serve unchanged.
2. **Rewind.** Her recurrent state is set from the walk's checkpoint at or before the tile: a block, already landed,
   because the Streamer was told the order.
3. **Forward.** The tile's rows decode against her K/V at their positions: a prefill of `t` rows, with the base
   frozen and the layer on. The context before the tile is read, never recomputed.
4. **Loss.** Cross-entropy only at labelled rows, weighted by `label_weight` (a reviewed correction at full weight,
   a perception at her relevance). It is computed over the vocabulary in slices, so the full row of logits and its
   gradient never exist at once (proposed in the fork: the per-row cost is measured first; see Open questions).
5. **Backward.** Only through her layer. The gradient for this tile's K/V rows arrives from the accumulator (the
   tiles after it already ran). The gradient for earlier rows' K/V is added into the accumulator. Her adapter's
   gradient accumulates in place.
6. **Yield.** The step ends at the tile, reports `Did { did: t, lasting }`, and the lane chooses the next step.
   Another mind may step before her next tile.

After the first tile (the lesson's first rows), one optimizer update is applied with the whole lesson's label
denominator. Her genome commits it, and her served tail is restored and verified unchanged: K/V rows, recurrent
state, tokens and her next logits. One update per lesson; tiles never count as work (STAGE-CONTRACT.md, "Units" 4).

The fork's learn surface collapses to three calls on her context: begin a lesson, the cost of a tile at `t` rows
and horizon `h`, and run one tile. The whole-reply graph is the walk at `t >= reply`, a unit size, never a mode.
`llama_opt_learn_graph_step` (one graph node per call, for a turn that cannot wait) is deleted. The finest unit is a
small tile, and the lane already puts a turn before a tile.

**Later in the same step as serving.** Labelled rows are rows. Once the lane executes `Batch` (THE-LANE-ON-SAGE.md,
open question 1), a learn tile rides in the same composed step as other minds' decode rows: one dispatch, one
fence, with the backward only over the labelled slice. Until then a tile is her own step. The tile's contract is the
same either way, so the batch is a scheduling choice, never a second learning path.

## Memory: constant per step, streamed per lesson

A step's device working set is

```text
step(t) = a + b·t          // workspace of one tile: fixed part a, per trained row b
```

plus her layer and its optimizer (12.3 MB on the 35B), held by her genome while she learns. `a` and `b` are measured
per model and backend by the tile price at two or three tile sizes, refit as tiles report, never assumed. On the M5's
35B, with native attention backward, `b` is about 12.2 MB per row today. Context length enters only through the
horizon term, measured at 0.5% from 4,096 to 16,384 rows.

The lesson's state outside the step scales with the lesson, and lives in the block layer, not in the step:
- **The K/V gradient accumulator** for the lesson's span: host tier (unified memory on the M5, pinned on the 5090),
  F32 today, BF16 proposed. A tile reads and writes only its rows' slice, so only that slice is on the device.
- **Recurrent checkpoints** every `stride` tiles: blocks the Streamer places, which flash can back. Fewer
  checkpoints means more replay and the same gradient.
- **The forecast is exact.** The walk names the blocks it needs and in what order, so the Streamer's `prefetch`
  lands each before its tile, and a miss is a counted defect.

Targets: `b` under 1 MB per row once the loss is sliced (receipt: the tile price before and after). A 64-row tile on
the 35B then fits inside serving's own compute workspace headroom on the 5090.

## Fitting: in space and in time, never refused

Every lever is declared by its holder, and one controller sets them from feedback (SAGE drives the frame):
- **Space: the tile.** `Lever { least, most, left }` in rows. `least` is the smallest tile the walk is exact at on
  this backend, measured by gate 1. `most` is the lesson. The lane picks `t` with the step, inside the slack it gives
  her learning class, and the authority grants `step(t)`. A grant short of `step(t)` is answered by a smaller tile
  at the same step, down to `least`, then the tile waits in its class. It is never a refusal.
- **Space: the stride.** From the host room her Learning part holds through the authority. Less room means fewer
  checkpoints and more replay, never a refusal.
- **Time: the duty.** Her learning's share of the Core is what the lane's choices add up to from feedback: the owed
  queue's growth against its service. Daydreams take idle slack and deep learning takes long quiet; continual
  learning keeps the queue in check.
- **A shrink mid-lesson** (STAGE-CONTRACT.md, "Control") is answered by a smaller next tile. Below `least`, she yields
  with the walk's blocks kept (`Yielded { residency_kept }`), and the Streamer pages them down a tier.
- The only typed refusal is a layout the walk cannot run on: no attention layer, or a shared cache on the stream. It
  is refused when her Learning part starts, before any step.

## Lifecycle and custody

- **Nothing is held while nothing is learned.** Her shadow and optimizer are blocks of her genome. When she has no
  lesson owed, they page down like any cache (the fork's `llama_opt_shadow_state_write` and restore) and hold no
  device memory. There is no `Loaded` state.
- **A fresh shadow is made in memory**, never written to a temp file and loaded back.
- **What lasts is the committed update**: provisional per update id until her genome commits it, idempotent on
  replay, written off on restore, cancel or an engine relaunch without the fence's evidence (STAGE-CONTRACT.md,
  "Control").
- **At a stop**, a lesson mid-walk is provisional and its walk state is released. Her genome's committed state is
  preserved with her record, through the same custody by rename as her K/V (`preserve.rs`).
- **She talks while she learns.** An addressed turn of hers, or any mind's bounded turn, outranks a tile at the next
  step, by the lane's envelope.

## What is deleted

Each slice deletes the path it replaces, in the same PR:
- `measure_within` and its context-only shrink (`driver/learning.rs`), and `Learner::measure` at reply rows with
  `continuum_opt_measure_shadow_graph`: replaced by the tile price.
- `llama_opt_learn_graph_step`, the `whole_reply_commit` measure phase and `LearnRun::graph_step`.
- `Learns::learn`'s blocking loop and `go_on` (`unified_tier/learn.rs`): replaced by one tile per lane step.
- The `Learning` jobs `Measure`, `Load`, `Seat`, `Release`, the `SeatState` machine and `SeatId`
  (`inference/learning.rs`), and the trainer's seat driving (`training_trigger/seat_run.rs` and its use in
  `on_engine.rs`): replaced by `learning/owed` and her Learning part.
- The fresh shadow's temp file (`driver/learning.rs` `load`).
- llama-server's learn endpoint (`tools/server/server-context-learn.inc`), its HTTP adapter and the grant ledger
  (`llama_server/learn_ledger.rs`), once no caller remains.

## The order

| Slice | Owner | Gate (a number on the named node) |
|---|---|---|
| 1. The tile price: the fork's cost at `(t, h)` with the walk's own flags, allocating nothing; device and host rows apart; `lesson_size` reports `a`, `b` and the horizon term | Fable | M5, 35B: the price is at least each tile's actual allocation at 16, 64 and 256 rows; Kimi's 809 GB lesson prices under the 5090's headroom at its chosen tile |
| 2. The loss sliced over the vocabulary, labelled rows only | Fable | `b` before and after on the 35B; the update's cosine against the unsliced step is at least 0.9999 |
| 3. CUDA's backward correct | Codex, Fable | 5090, 0.8B: tiled and untiled against CPU at least 0.999 (today 0.33) |
| 4. Her Learning part and her genome's `grow`/`commit`; the tile as a lane step; seats and the blocking loop deleted | Fable (part, lane), Cormac (`learning/owed`) | `slice::sim`: the owed queue in check while no bounded mind passes her bound; a grant short of `step(t)` runs a smaller tile |
| 5. The walk's blocks through the Streamer with the exact forecast | Fable | zero walk-block misses over a 15-minute window |
| 6. Live, both nodes | everyone | 15-minute windows on the M5 (35B) and the 5090: steps and saves above zero, held-out loss kept, no resident past her bound, acts per hour not below the matched control |
| 7. A tile in a composed batch | Fable, Codex | after `Batch` executes: aggregate need-weighted rate above switching on the same trace |

## Open questions

1. **What the 12.2 MB per row is.** Measured as a total; not yet broken down by tensor. The full-vocabulary logits
   and their gradient are the inference to check first (about 1 MB per copy in F32 at a 248k vocabulary). The
   breakdown comes before slice 2, not after.
2. **The accumulator in the measure:** a CPU buffer over a pointer never read, or a graph input the measure marks
   host-resident (Codex)?
3. **CUDA's 0.33 on the 0.8B:** which op, per the bisect with op offload off.
4. **`least` per backend:** the smallest tile at which the walk measures exact, from gate 1 of each backend.
