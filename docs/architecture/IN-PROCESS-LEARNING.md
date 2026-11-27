# In-process learning: her serving step with labels on

Owners: Fable (the unit-stage parent, the fork's walk, the llama crate, her Learning part), Cormac (lessons and what
is owed, the authority's ledger), Codex (serving, admission, the fence, review). Read with
[THE-WHOLE-MIND.md](THE-WHOLE-MIND.md) (her genome is part of her), [SAGE.md](SAGE.md),
[STAGE-CONTRACT.md](STAGE-CONTRACT.md) ("Units", "Control"), [SHARED-RESOURCES.md](SHARED-RESOURCES.md) (holders
are Residents), [THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md) and [UNIFIED-PAGING.md](UNIFIED-PAGING.md).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The law](#the-law)
- [Why it is rewritten](#why-it-is-rewritten)
- [Who decides what](#who-decides-what)
- [The objects](#the-objects)
- [One learn step](#one-learn-step)
- [Memory, through the one placer](#memory-through-the-one-placer)
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
  checkpoint of it. It is never prefilled again for learning. Only the lesson's own rows run, as a prefill of those
  rows.
- **Streamed.** A lesson is learned in tiles, last to first: the exact walk. Each tile is one step of hers on the
  Core, the same shape as a prefill step, chosen by the lane like any other step. Between tiles, other minds step.
- **Constant per step.** A step's device scratch is bounded by its tile, whatever the length of the lesson or her
  context. An 8 GB laptop and a 34 GB card run the same mechanism, at the tile sizes their placer grants.

**Learning decides nothing about memory or time.** It is a set of objects that declare what they hold, how each
holding can vary and what each way costs. The one placer decides every byte. The one lane decides every step. A
lesson costs time on the Core, never a refusal.

## Why it is rewritten

The form on canary at 5138272ccc cannot work on an edge node:

| Defect | Where | Receipt |
|---|---|---|
| A lesson is priced as one whole-reply graph, and only its context is ever shrunk. Its reply, which carries the cost, is never shrunk. | `driver/learning.rs:277` (`measure_within`), `llama::Learner::measure` (`continuum_opt_measure_shadow_graph`) | 5090, Kimi: 63,831,450,752, 72,566,913,152, 88,837,678,208 and 809,395,386,624 bytes asked of 34,190,917,632 (#6293, build 2154, 2026-11-27); 1.897 TB before the CUDA kernel |
| The bytes follow the reply, not the context | `core/llama/examples/lesson_size.rs` (M5, 35B, native attention backward) | reply:lesson 64:4096 1.72 GB, 256:4096 3.88 GB, 784:4096 10.10 GB, 784:16384 10.15 GB, 2048:4096 24.98 GB, 2048:16384 25.09 GB: about 12.2 MB per trained row |
| A lesson runs as one blocking loop on the engine thread, outside the lane | `continuum-persona-llama/src/unified_tier/learn.rs` (`learn`: `run.step()` until done, a `go_on` callback) | the lane never sees a learn unit, cannot size one or put a mind between two |
| Memory is decided in the leaves: the trainer leases weights, optimizer and workspace itself, and a `Fit` is handed down to the engine | `inference/learning.rs` (`Load { fit }`), `training_trigger/{on_engine,seat_run}.rs` | a second picture of one memory beside the authority's, the defect SHARED-RESOURCES.md names |
| Learning is a seat protocol the trainer drives job by job: `Measure`, `Load`, `Seat`, `Learn`, `Checkpoint`, `Release` | same | 5090, #6292 build 2163: 11 loaded and released, 11 memory waits, zero steps, zero saves; #6293: 18 of 18 loaded and released, zero steps |
| A fresh shadow goes through a temp file to load | `driver/learning.rs` `load` (`continuum-shadow-<seat>.gguf`) | an encoding and a disk round trip for bytes already in memory |

The tiled exact walk was built in the fork (`llama-context-learn-tiles.cpp`, `llama_opt_learn_tile`,
`llama_opt_learn_unit`, `llama_opt_learn_last_unit`). Its pricing and its scheduling as lane steps were marked
"Proposed" in this doc and never built, so the live path still prices the whole-reply graph. Nothing here patches that
path: the objects below replace it, and it is deleted in the same change.

Kept, measured (64-row tiles against the exact step):

| Backend | Model | Result |
|---|---|---|
| Metal (M5) | Qwen3.5-0.8B-Q8_0 | cosine 1.0000; stride 1: 8 units, 440 rows; stride 4: 17 units, 824 rows; max 64 rows per unit |
| Metal (M5) | Qwen3.6-35B-A3B-Q4_K_M | 0.9976, equal to the untiled learn (production layout) |
| CPU (IntelMac) | Qwen3.5-0.8B, Qwen2.5-0.5B | 1.0000 in all four cases (Cormac, 1eb66d1db) |
| CUDA (5090) | Qwen3.5-0.8B | open: whole-reply 0.31 and tiled 0.33 against CPU, while CPU-only on the same binary is 0.9999; CUDA's backward is wrong, not the tiling (Codex, 398bf318d) |

## Who decides what

One owner per decision, each already built or specified:

| Decision | Owner | Learning's part in it |
|---|---|---|
| Every byte, on every tier | the placer over the one authority (`place::PlacerCore`, `ResourceDaemon`; SHARED-RESOURCES.md) | its objects are `Resident`s: they report rows and answer `shrink` and `grow` |
| Which step runs on the Core, and its unit | the lane (`decide::cases::lane`, `driver/lane.rs`) | its stage declares a lever and a cost at a unit |
| When a block moves between tiers | the Streamer and its `prefetch` use case | its walk names the blocks it needs, in order |
| What a mind still owes to learn | the trainer (Cormac), as `learning/owed` | its stage hears it |
| What her genes are | her genome | its stage asks the genome to grow and commit |

Nothing in learning holds a lease, a budget, a `Fit` or a threshold. A number that sizes anything comes from a
report, a grant or a measurement, owned above.

## The objects

Contracts and parents first: each pattern is a parent that does the shared work once, and the learning objects are
leaves that declare only their difference.

**`UnitStage`, the parent of long work on the Core** (STAGE-CONTRACT.md, "Units": the intermediate parent "Long work as
a stage run with a unit", whose first consumer this is). It is a SAGE `Component` whose work divides. The parent owns
everything shared: the claim held per unit size and resized when the size changes, the yield point at the unit
boundary, control requests and their receipts (`Paused`, `Yielded { residency_kept }`, `Cancelled { committed_effects }`),
`did <= unit` asserted where the completion lands, and its rows folded into the Core's report. A leaf declares four
things:

```rust
/// Long work the lane steps a unit at a time. The parent does claims, yields, receipts and reports.
pub trait UnitStage: Resident<Place, Bytes> {
    /// Its work's quantity: rows for a lesson, rows for a prefill, bytes for a stream.
    type Unit: Quantity;
    /// How the work divides: least is the smallest unit it is correct at, measured; left is what remains.
    fn lever(&self) -> Lever<Self::Unit>;
    /// What one unit costs, allocating nothing: transient scratch and the time it takes, from its own measure.
    fn cost(&self, unit: Self::Unit) -> UnitCost;
    /// Run one unit on the Core the lane bound. Never longer than the unit.
    fn run(&mut self, core: &mut CoreStep<'_>, unit: Self::Unit) -> Did<Self::Unit>;
}
```

Her prefill is its second consumer (`driver/prefill_rate.rs` moves onto it), so the parent is extracted at its second
instance, as SAGE.md requires, and this design does not stand alone.

**Her genome**, the object (THE-WHOLE-MIND.md). It is a `Resident` of her mind: stable genes, the shadow (a small layer
above her stable stack, never a copy), the optimizer moments and step count, snapshots and lineage. Learning changes
it only through its methods: `grow(basis) -> &mut Shadow` and `commit(update) -> Generation`. Adoption promotes the
shadow to stable after an exam, a pointer swap. Its rows:

| Row | Residency | Varies | Ways |
|---|---|---|---|
| stable genes | Pageable | `Whole { dependents: 1 }` while she serves | `cache_out` to RAM and flash |
| shadow and optimizer | Pageable | `Whole { dependents: 0 }` | `cache_out`, paged back on her next tile (`llama_opt_shadow_state_write`, restore) |

**What is owed** (Cormac's, the trainer's one output). Latest-wins State, keyed by mind and competence, so her Learning
part never reaches back into the trainer:

```rust
pub struct Owed(BTreeMap<MindId, Vec<OwedLesson>>);
pub struct OwedLesson {
    pub competence: CompetenceKey,   // persona x trait x base: a mind learns several at once
    pub basis: GeneBasis,            // Mint, or Fork { parent }: the trainer's competence decision, never re-decided
    pub next: LessonHandle,          // { submission, position }: rows read through the journal's row boundary
    pub examples_owed: u32,
    pub since: Frame,
}
```

A lesson the trainer decided to reuse or defer is not owed at all. The trainer stays the journal's one owner and her
genome the weights' one owner: `learn/<mind>/did` carries `{ update, lesson, rows }` for each update her genome
committed, and the trainer advances its learned cursor by that update id, idempotent on replay and never ahead of the
genome.

**Her Learning**, the leaf. A `UnitStage` and a part of a mind: started on her bus with her identity, it rests when she
rests, and its need folds into hers. It hears `learning/owed` for her and its own completions, and reads a lesson's
rows by its handle (`pending_counts`, `lesson_at`), never her history. It says `learn/<mind>/did`. Its unit is rows. It
holds one object, the walk:

**The walk**, the lesson in progress. It is a `Resident`, and its holdings are blocks in the block layer:

| Row | Residency | Varies | Ways |
|---|---|---|---|
| K/V gradient accumulator for the lesson's span | Pageable | `Elastic`: only the next tile's slice must be on the device | `cache_out` (host to flash), measured |
| recurrent checkpoints | Pageable | `Elastic { floor: one }` | `collapse`: fewer checkpoints, priced in replay time; `cache_out` |
| the tile's scratch | Transient, in the Core's compute arena | sized by the unit | the unit itself |

So the placer, not the walk, trades checkpoint density against replay time, and keeps the accumulator on the tier the
next tile needs. The walk's `report` names its blocks in reverse order, so the Streamer's forecast is exact.

**The Core's compute arena** is the one transient arena (SHARED-RESOURCES.md, "Residency over time"): serving's
graphs and a learn tile alias it, never sum. A tile whose cost fits the arena serving already holds costs no extra
device memory. A larger unit is a `grow` of the arena, granted or not by the placer like any other.

## One learn step

A learn step is her serving step with labels on. It reads her cache in place and runs only the tile's rows:

1. **Bind.** The lane switches her in like any step (a pointer on unified memory, a fenced DMA on a discrete GPU).
   Her genome's shadow attaches at scale one over her stable genes, which serve unchanged.
2. **Rewind.** Her recurrent state is set from the walk's checkpoint at or before the tile: a block that has already
   landed, because the walk named it.
3. **Forward.** The tile's rows decode against her K/V at their positions: a prefill of `t` rows with the base
   frozen and the layer on. The context before the tile is read, never recomputed.
4. **Loss.** Cross-entropy only at labelled rows, weighted by `label_weight` (a reviewed correction at full weight,
   a perception at her relevance). It is computed over the vocabulary in slices, so the full row of logits and its
   gradient never exist at once (proposed in the fork; the per-row cost is broken down first, Open questions).
5. **Backward.** Only through her layer. The gradient for this tile's K/V rows arrives from the accumulator (the tiles
   after it already ran). The gradient for earlier rows' K/V is added into the accumulator. Her layer's gradient
   accumulates in place.
6. **Yield.** The step ends at the tile, the parent reports `Did { did: t, lasting }`, and the lane chooses the next
   step. Another mind may step before her next tile.

After the lesson's first tile, one optimizer update is applied with the whole lesson's label denominator. Her genome
commits it, and her served tail is restored and verified unchanged: K/V rows, recurrent state, tokens and her next
logits. One update per lesson; tiles never count as work (STAGE-CONTRACT.md, "Units" 4).

The fork's learn surface becomes three calls on her context: begin a lesson, the cost of a tile at `t` rows and
horizon `h`, and run one tile. The whole-reply graph is the walk at `t >= reply`, a unit size, never a mode.

**Later in the same step as serving.** Labelled rows are rows. Once the lane executes `Batch` (THE-LANE-ON-SAGE.md, open
question 1), a learn tile rides in the same composed step as other minds' decode rows: one dispatch, one fence, the
backward only over the labelled slice. The tile's contract is the same either way, so batching is a scheduling choice,
never a second learning path.

## Memory, through the one placer

A tile's transient scratch is measured, never assumed:

```text
cost(t, h).scratch = a + b·t + c·h        // per model and backend, fitted from the fork's cost at a unit
```

On the M5's 35B with native attention backward, `b` is about 12.2 MB per trained row and `c` is small (0.5% from
4,096 to 16,384 rows of horizon). The fit is refreshed from each tile's measured scratch.

How it fits is the placer's and the lane's, never the walk's:
- **Space.** The lane offers a unit inside the slack it gives her learning class. The parent holds one elastic claim
  on the compute arena sized to its current unit, resized when the unit changes, never asked per step
  (SHARED-RESOURCES.md, "A claim is held per period"; `hold_elastic` and `ShrinkHolder` are its adapter onto the
  authority, Cormac). A shrink is answered by a smaller unit at the next step, down to `least`, then `Yielded`. That is
  the parent's code, written once for every `UnitStage`.
- **Space between units.** The placer shrinks the walk's rows by their declared ways: the accumulator caches out,
  checkpoints collapse. It asks only what its flexbox gives the walk at its QoS, so a lesson never takes a mind's window.
- **Time.** Her learning's share of the Core is what the lane's choices add up to: the owed queue's growth against
  its service. Daydreams take idle slack, deep learning takes long quiet, and continual learning keeps the queue in
  check.
- **Refusal.** Only a layout the walk cannot run on (no attention layer, a shared cache on the stream) is typed, when
  her Learning part starts, before any step.

Target: `b` under 1 MB per row once the loss is sliced, so a 64-row tile on the 35B fits inside the compute arena
serving already holds on the 5090.

## Lifecycle and custody

- **Nothing is held while nothing is learned.** With no lesson owed, the walk does not exist and her genome's shadow
  and optimizer are cached out. There is no loaded state between lessons.
- **A fresh shadow is made in memory**, by her genome, never written to a temp file and loaded back.
- **What lasts is the committed update**: provisional per update id until her genome commits it, idempotent on
  replay, written off on restore, cancel, or an engine relaunch without the fence's evidence (STAGE-CONTRACT.md,
  "Control").
- **At a stop**, a lesson mid-walk is provisional and the walk is dropped, which returns its blocks. Her genome's
  committed state is preserved with her record, through the same custody by rename as her K/V (`preserve.rs`).
- **She talks while she learns.** Her addressed turn, or any mind's bounded turn, outranks a tile at the next step, by
  the lane's envelope.

## What is deleted

In the change that lands the objects, never later:
- `measure_within` and its context-only shrink (`driver/learning.rs`), and `Learner::measure` at reply rows with
  `continuum_opt_measure_shadow_graph`: replaced by `UnitStage::cost`.
- `llama_opt_learn_graph_step`, the `whole_reply_commit` measure phase and `LearnRun::graph_step`: the finest unit is a
  small tile.
- `Learns::learn`'s blocking loop and `go_on` (`unified_tier/learn.rs`): replaced by `UnitStage::run`, one tile per
  lane step.
- The `Learning` jobs `Measure`, `Load`, `Seat`, `Release`, `Fit`, the `SeatState` machine and `SeatId`
  (`inference/learning.rs`), and the trainer's seat driving and its leases (`training_trigger/seat_run.rs`, the seat
  path in `on_engine.rs`): replaced by `learning/owed`, her Learning part and the placer.
- The fresh shadow's temp file (`driver/learning.rs` `load`).
- llama-server's learn endpoint (`tools/server/server-context-learn.inc`), its HTTP adapter and the grant ledger
  (`llama_server/learn_ledger.rs`).

## The order

Each slice lands whole: the parent or object, its first consumer on the live path, and the deletion of what it
replaces.

| Slice | Owner | Gate (a number on the named node) |
|---|---|---|
| 1. `UnitStage` in `continuum-sage`, with her prefill as its first consumer; `prefill_rate.rs` deleted | Fable | `slice::sim`: no bounded mind past her bound; a short grant runs a smaller unit at the same step; M5 live: prefill p50 not worse |
| 2. The fork's surface: begin, cost at `(t, h)`, run one tile; the graph step and whole-reply measure deleted | Fable | M5, 35B: cost at least each tile's actual scratch at 16, 64 and 256 rows; tiled against untiled at least 0.997 |
| 3. Her genome as the object (`grow`, `commit`, cache out), her Learning part and the walk as Residents; seats, `Fit`, the trainer's leases and the blocking loop deleted | Fable (genome, part, walk), Cormac (`learning/owed`, the ledger) | M5 15-minute window: steps and saves above zero, held-out loss kept, no resident past her bound |
| 4. The loss sliced over the vocabulary, labelled rows only | Fable | `b` before and after on the 35B; the update's cosine against the unsliced step at least 0.9999 |
| 5. CUDA's backward correct | Codex, Fable | 5090, 0.8B: tiled and untiled against CPU at least 0.999 (today 0.33) |
| 6. The 5090 live | everyone | 15-minute window: Kimi's lessons (64 GB, 89 GB, 809 GB asked today) learned beside serving; acts per hour not below the matched control |
| 7. A tile in a composed batch | Fable, Codex | after `Batch` executes: aggregate need-weighted rate above switching on the same trace |

## Open questions

1. **What the 12.2 MB per row is.** Measured as a total, not yet by tensor. The full-vocabulary logits and their
   gradient are the inference to check first (about 1 MB per F32 copy at a 248k vocabulary). The breakdown comes
   before slice 4.
2. **The accumulator in the cost:** a CPU buffer over a pointer never read, or a graph input the cost marks
   host-resident (Codex)?
3. **CUDA's 0.33 on the 0.8B:** which op, per the bisect with op offload off.
4. **`least` per backend:** the smallest tile at which the walk is exact, from slice 2's gate on each backend.
