# In-process learning: her frame with labels on

Owners: Fable (her genome's learning, the unit-stage parent, the fork's walk, the llama crate), Cormac (the correction
and course sources, the competence rule, the authority's ledger), Codex (serving, admission, the fence, review). Read
with [THE-WHOLE-MIND.md](THE-WHOLE-MIND.md) (§9 the genome, §10 three forms of learning), [SAGE.md](SAGE.md),
[STAGE-CONTRACT.md](STAGE-CONTRACT.md), [SHARED-RESOURCES.md](SHARED-RESOURCES.md),
[UNIFIED-PAGING.md](UNIFIED-PAGING.md), [THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md) and
[CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md) §1. The object model's chapter 0 (what RAGE learned, one price) and
chapter 6 (the genome) are on `fable/learning-l2-design` and are cited here by section until they land.

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The law](#the-law)
- [Why it is rewritten](#why-it-is-rewritten)
- [Who decides what](#who-decides-what)
- [The objects](#the-objects)
- [Where lessons come from](#where-lessons-come-from)
- [A lesson in frames](#a-lesson-in-frames)
- [Memory: one placer, pages by handle, never refused](#memory-one-placer-pages-by-handle-never-refused)
- [On and off](#on-and-off)
- [Checked against the engine](#checked-against-the-engine)
- [What is deleted](#what-is-deleted)
- [The order](#the-order)
- [Open questions](#open-questions)

## The law

Learning is inference plus a little: her frame on the Core with labels switched on over some of its rows. The model is
her served model as it serves (frozen, at its served quantization), with her genome's small trainable layer on top.
Only the backward through that layer is extra. A quantized frozen base under a trainable low-rank layer is QLoRA by
construction; the base is never copied, dequantized or reloaded.

Five properties, each a consequence of building it on the engine rather than beside it:
- **Her context comes from her cache.** A lesson's context is the K/V and recurrent state she holds, or an exact
  checkpoint of it. It is never prefilled again for learning; only the lesson's own rows run.
- **Rows in the frame.** A Core frame is a draw list: the bound mind's decode row, a prefill chunk, a lesson's
  labelled rows (object model 0.2, "A frame is built in passes"). A lesson is learned a tile of rows at a time, last
  tile to first (the exact walk), each tile an entry in some frame's draw list, chosen by the lane.
- **Time-shared like anything else.** Learning is a thread on the lane: runnable while she owes lessons, given its
  share from feedback, preempted at every tile boundary. Overload spreads a lesson over more frames; it never drops
  it (0.4, "Overload slows the clock").
- **Pages, not reservations.** Her learning state (the shadow, its optimizer, the walk's accumulator and
  checkpoints) is pages in the block layer, held by handle, moved by a link, placed by the one placer. Between
  lessons none of it is on the device. Switching learning on or off is a page move, the same as switching minds.
- **Never refused.** Learning keeps the four laws every consumer keeps (ONE-FRAME-ONE-MEMORY.md, "The laws that make
  refusal impossible"): every holding a page or a transient, a measured floor tile, room only from pages priced below
  the lesson, and an arena sized for the frame it composes. What does not fit now fits smaller or later.

## Why it is rewritten

The form on canary at 5138272ccc cannot work on an edge node:

| Defect | Where | Receipt |
|---|---|---|
| A lesson is priced as one whole-reply graph, and only its context is shrunk. Its reply, which carries the cost, is never shrunk | `driver/learning.rs:277` (`measure_within`), `Learner::measure` (`continuum_opt_measure_shadow_graph`) | 5090, Kimi: 63,831,450,752, 72,566,913,152, 88,837,678,208 and 809,395,386,624 bytes asked of 34,190,917,632 (#6293, build 2154, 2026-11-27); 1.897 TB before the CUDA kernel |
| The bytes follow the reply, not the context | `core/llama/examples/lesson_size.rs` (M5, 35B, native attention backward) | reply:lesson 64:4096 1.72 GB, 256:4096 3.88 GB, 784:4096 10.10 GB, 784:16384 10.15 GB, 2048:4096 24.98 GB, 2048:16384 25.09 GB: about 12.2 MB per trained row |
| A lesson runs as one blocking loop on the engine thread, outside the lane | `unified_tier/learn.rs` (`learn`: `run.step()` until done, a `go_on` callback) | the lane never sees a learn unit |
| Memory is decided in the leaves: the trainer leases weights, optimizer and workspace itself and hands a `Fit` down | `inference/learning.rs` (`Load { fit }`), `training_trigger/{on_engine,seat_run}.rs` | a second picture of one memory beside the authority's |
| Learning is a seat protocol driven job by job: `Measure`, `Load`, `Seat`, `Learn`, `Checkpoint`, `Release` | same | 5090, #6292 build 2163: 11 loaded and released, 11 memory waits, zero steps, zero saves; #6293: 18 of 18 loaded and released, zero steps |
| The trainer is a node service fed text examples, not part of her | `continuum-dreaming/src/modules/training_trigger.rs` (a journal per persona, trait and base), `dataset/turn_to_example.rs` | the lesson's context is rebuilt as a prompt, never her cache |
| Learning from her own work is not built | `label_weight` has no caller outside `core/llama` | |
| Learning from work waits behind a credit gate | `continuum-mind/src/persona/training_producer/` | M5 health line, 2026-11-27: "credit accepted 0" |
| Dreams are a `BrainRegion` | `continuum-academy/src/cognition/dream_consolidation/region.rs` | |
| Learning state leaves the device as an encoding | `llama_opt_shadow_state_write` (a serialized write to a `FILE`) | a page move that serializes |
| Genes have a private memory owner beside the placer | `continuum-genome/src/genome/gene_residency.rs` (`DeviceTenant`, `PressureBroker`, LRU-2) | |
| The form that matched the engine was closed | #6203, "A lesson is her ordinary decode with labels", closed unmerged 2026-11-25 as superseded by the seat form | |

The tiled exact walk was built in the fork (`llama-context-learn-tiles.cpp`, `llama_opt_learn_tile`,
`llama_opt_learn_unit`, `llama_opt_learn_last_unit`). Its pricing and its scheduling as frames were marked "Proposed"
here and never built. Nothing below patches the live path. The objects replace it, and it is deleted in the same
change.

Kept, measured (64-row tiles against the exact step):

| Backend | Model | Result |
|---|---|---|
| Metal (M5) | Qwen3.5-0.8B-Q8_0 | cosine 1.0000; stride 1: 8 units, 440 rows; stride 4: 17 units, 824 rows; max 64 rows per unit |
| Metal (M5) | Qwen3.6-35B-A3B-Q4_K_M | 0.9976, equal to the untiled learn (production layout) |
| CPU (IntelMac) | Qwen3.5-0.8B, Qwen2.5-0.5B | 1.0000 in all four cases (Cormac, 1eb66d1db) |
| CUDA (5090) | Qwen3.5-0.8B | at the merged fork 9f2477f49: CPU-only against CUDA, cosine 0.999569 whole and 0.999627 tiled, 319,488 adapter elements (Codex, 2026-11-27 17:29Z); 0.31 and 0.33 at 398bf318d before it |

## Who decides what

One owner per decision. Learning owns none of these; its objects only declare.

| Decision | Owner | What learning's objects provide |
|---|---|---|
| Every byte on every tier | the placer over the one authority (`place::PlacerCore`, `ResourceDaemon`) | `Resident` reports: rows, ways, costs, what each row requires beside it |
| When a page moves, and by which link | the Streamer, its `prefetch` use case | the walk's pages, named in the order it will touch them |
| Which entries make the next frame, and their units | the lane (`decide::cases::lane`, `driver/lane.rs`) | a lever (rows) and the cost of a tile at a unit |
| What matters, and how much | the one importance over her parts: her hippocampus's active engrams, weighed by her amygdala (THE-WHOLE-MIND §9) | a lesson's value is read from it, never computed by learning |
| What her genes are, which gene a lesson trains, adoption | her genome | the methods below |

Until the object model's `Demand` (chapter 3.8) is built, a lesson's price reaches the lane as its stage's `Need`
and reaches the placer as its rows' `Qos` and wait. When `Demand` lands, a lesson becomes one: its essential is its
whole labelled piece committed as one update, its progress is the tiles done, its switch cost is its pages.

## The objects

Parents first: each shared pattern is a parent that does its work once, and learning's objects are leaves that
declare their difference.

**`UnitStage`, the parent of long work on the Core** (STAGE-CONTRACT.md, "Units", whose intermediate parent this
is). A SAGE `Component` whose work divides. The parent owns everything shared:
- one elastic claim on the compute arena, sized to its current unit and resized when the unit changes, never asked
  per frame (`hold_elastic` and `ShrinkHolder`, in #6287, are its adapter onto the authority);
- the yield point at every unit boundary, and the control receipts (`Paused`, `Yielded { residency_kept }`,
  `Cancelled { committed_effects }`);
- `did <= unit`, asserted where the completion lands;
- a shrink answered by a smaller unit at the next frame, down to `least`, then a yield with its pages kept;
- its rows folded into its owner's report.

```rust
/// Long work the lane runs a unit at a time. The parent does claims, yields, receipts and reports.
pub trait UnitStage: Resident<Place, Bytes> {
    type Unit: Quantity;
    /// How the work divides: least is the smallest unit it is correct at (measured), left what remains.
    fn lever(&self) -> Lever<Self::Unit>;
    /// What one unit costs, allocating nothing: its transient scratch and its time, from its own measure.
    fn cost(&self, unit: Self::Unit) -> UnitCost;
    /// Put one unit's rows in the frame the lane is building. Never more than the unit.
    fn draw(&mut self, frame: &mut FrameList<'_>, unit: Self::Unit) -> Did<Self::Unit>;
}
```

Her prefill is its other consumer (`driver/prefill_rate.rs` moves onto it), so the parent is extracted at its
second instance.

**Her genome**, the object (THE-WHOLE-MIND §9; object model 6.1). A `Resident` of her mind, and a tree of parts:

```text
Genome                 her genes as states of one object                      continuum-genome
├─ genes: GeneSet      the genes she wears, at their weights in her phenotype; stable genes are shared archetypes
├─ shadow: Shadow      the small layer she learns into, above her stable stack, never a copy
│  └─ LearningPages    shadow weights, optimizer moments, step count: pages of her genome's region
├─ lessons: Lessons    her lesson queue: spans offered by her parts, priced by her importance
├─ walk: Option<Walk>  the lesson in progress, a UnitStage (below)
├─ snapshots           immutable candidate genes at an optimizer boundary
└─ lineage             parents, data, base, exams, wearers
```

Its methods are the only way learning changes her: `grow(basis) -> &mut Shadow` (a fresh shadow made in memory, or
forked from a parent gene), `commit(update) -> Generation`, `basis_for(competence) -> GeneBasis` (today's
`competence::decide`, moved to its owner), and `adopt(snapshot)` after its exam (a pointer swap between frames).
How an update spreads across the genes active in her phenotype, by their weights, is the genome's (THE-WHOLE-MIND
§10, "learning spread by weight"); the shadow is its first form.

**The walk**, the lesson in progress. A `UnitStage` whose unit is rows, and a `Resident`:

| Row | Residency | Varies | Ways |
|---|---|---|---|
| shadow and optimizer (her genome's `LearningPages`) | Pageable | `Elastic`: a tile needs only the slices of the layers it trains | `cache_out` by link, paged back before her next tile |
| K/V gradient accumulator for the lesson's span, and the recurrent-state gradient carried across tile boundaries on a hybrid's linear-attention layers | Pageable | `Elastic`: a tile reads and writes only its rows' slice | `cache_out` by link |
| recurrent checkpoints | Pageable | `Elastic { floor: one }` | `collapse`: fewer checkpoints, priced in replay time; `cache_out` |
| a tile's scratch | Transient, in the Core's compute arena, which the placer sizes for the frame that composes it | sized by the unit | the unit itself |

Its `Requires` names what each row needs beside it (her K/V for the lesson's context, her genes at the lesson's
basis), so the placer never frees one without the other.

Residency (Resident, Transient, Pageable) is SHARED-RESOURCES.md's "Residency over time"; it is not yet a field of a
`Resident` row, and slice 3 adds it there, once, for every holder.

## Where lessons come from

One mechanism and three forms (THE-WHOLE-MIND §10), and each form is a part of her that offers spans of her own rows.
None submits text examples to a node service, none is graded, and none earns credit first.

```rust
/// A part of her that has something to learn from. It offers spans of her rows, never text pairs.
pub trait LabelSource: Component {
    fn offered(&self) -> &[Span];
}
pub struct Span {
    pub capture: CaptureHandle,  // the durable record of the rows: tokens and positions (the one capture per exchange)
    pub rows: Range<Pos>,        // in that capture; her live K/V is the fast path, the capture the record
    pub weight: Weighting,       // per-row label weight
    pub competence: CompetenceKey,
    pub why: SpanSource,         // Work, Correction, Course, Dream
}
```

| Form | Her part | Its spans | Weight |
|---|---|---|---|
| As she works | her perception | the rows she just prefilled (messages, tool results, files read). The forward already ran when she perceived them, so only the backward is extra | her prediction error on them, weighed by her amygdala's appraisal (`label_weight`) |
| A correction | her reflection on a verdict she receives (a review, a peer, Joel) | a fork of her sequence at the corrected act: her context prefix, shared, then the corrected act tokenized once into the capture as the labelled rows | full |
| A course | the academy activity she is in (an activity is a room) | her sequence in the course room: material, practice, her answers; exams held out, deciding adoption | the course's targets |
| A dream | her hippocampus, replaying her engrams | a dream sequence of hers, from an exact checkpoint or prefilled from the engram: recombined and varied memories | their recalled salience |

Her genome's `Lessons` hears every `LabelSource` of hers and prices each span by her importance. A span whose rows
her cache no longer holds is re-materialized from its capture or from an exact prefix checkpoint, so a correction
outlives her cache and a restart (plan goal 8). An external driver's session (line 16) is a mind seated by MindMemory,
so it learns through the same capture and the same sources.

The node keeps no trainer. The trainer's journal holds accepted examples not yet learned (784 rows for Kimi's
code/owner on the 5090). Each pending row migrates once into a capture and is offered by a course-room source, with
the count per mind shown to Joel before the journal is deleted.

## A lesson in frames

A learn tile is an entry in her frame: her rows with labels on, against her cache in place.

1. **Bind.** The lane binds her for the frame like any entry (a pointer on unified memory, a fenced DMA on a discrete
   GPU). Her shadow attaches over her stable genes, which serve unchanged; its pages for this tile have landed.
2. **Rewind.** Her recurrent state is set from the walk's checkpoint at or before the tile: a page already landed,
   because the walk named it in order.
3. **Forward.** The tile's rows decode against her K/V at their positions, a prefill of `t` rows with the layer on.
   The context before the tile is read, never recomputed.
4. **Loss.** Cross-entropy at labelled rows only, at their weights, computed over the vocabulary in slices so the
   full row of logits and its gradient never exist at once (proposed; the per-row cost is broken down first).
5. **Backward.** Only through her layer. This tile's K/V gradient arrives from the accumulator; the gradient for
   earlier rows' K/V is added to it; her layer's gradient accumulates in place.
6. **Yield.** The entry reports `Did { did: t }`, and the lane builds the next frame. Other minds' entries come
   between her tiles; her addressed turn, or any mind's bounded turn, outranks a tile.

After the lesson's first tile, her genome commits one update with the whole lesson's label denominator, and her
served tail is verified unchanged (K/V rows, recurrent state, tokens, her next logits). One update per lesson; tiles
are progress, never reward (STAGE-CONTRACT.md, "Units" 4). Persistence of the generation runs as throughput work
off the frame (0.2, "persistence never stalls the frame").

The fork's learn surface becomes three calls on her context: begin a lesson, the cost of a tile at `t` rows and
horizon `h`, and run one tile. The whole-reply graph is the walk at `t >= reply`, a unit size, never a mode, and
`llama_opt_learn_graph_step` is deleted.

**Composed frames.** When the lane composes a frame from several entries (`Batch`, THE-LANE-ON-SAGE.md open question
1), a learn tile rides beside other minds' decode rows in one dispatch and one fence, the backward only over its
labelled slice, and the placer sizes the arena for the sum of the composed entries' scratch (ONE-FRAME-ONE-MEMORY.md, law 4); a frame that cannot have it composes fewer. Until then a learn tile is its own entry. Its contract is the same either way.

## Memory: one placer, pages by handle, never refused

A tile's scratch is measured, never assumed:

```text
cost(t, h).scratch = a + b·t + c·h        // per model and backend, fitted from the fork's cost at a unit
```

On the M5's 35B with native attention backward, `b` is about 12.2 MB per trained row and `c` is small (0.5% from
4,096 to 16,384 rows of horizon). The fit is refreshed from each tile's measured scratch.

**Everything else is a page.** The shadow and optimizer are pages of her genome's region, a block over her genome's
own mapped file, the way her K/V is a block over hers (UNIFIED-PAGING.md, "A mind's memory"). The accumulator and
checkpoints are pages of the walk. Each has a typed state (on a tier, requested, moving, resident, pinned while a
frame reads it) and is held by handle, so the placer can move it without invalidating anyone (0.2, "Every page has a
lifecycle, and handles let it move").

**Never refused, by the four laws** (ONE-FRAME-ONE-MEMORY.md). Learning's device need at one frame is its floor:
`scratch(least)`, the tile's slices of her layer and of the accumulator, the recurrent-state gradient the walk carries
across the tile boundary, and the routed experts the tile's rows hit, held through the backward as well as the
forward. Everything else learning holds is a page. The placer makes room for a tile only from pages priced below the
lesson; otherwise the tile waits for a later frame, and never pages down a higher-priced mind's warm set. Whether a node
can teach at all is the measured inequality `floor(least) <= arena(node)` per model and backend, gated in slice 2: a
tile's per-row cost is not a prefill's (it keeps logits and their gradient at every labelled row), so it is measured,
never inferred from serving.

**Short of room, step quality down, never a hole.** A short grant is met by a smaller tile at the same frame, then
sparser checkpoints (more replay), then the lesson spread over more frames (0.4, "Overload slows the clock"). When
the whole node is over budget, the placer raises one bias that steps every holder down a level a little (0.2, "Over
budget, lower detail everywhere a little"), learning's tile among them.

**One placer for genes too.** `gene_residency` becomes a `Resident` under the placer, its pages moved by the
Streamer. Its `DeviceTenant`, its `PressureBroker` path and its own LRU-2 are deleted; a gene's heat is its weight in
the phenotypes that wear it, read from the one importance (0.2, "What many depend on streams first").

## On and off

Learning switches on and off the way a mind switches in and out:
- **On.** A span is priced above the lane's cut. The Streamer promotes the walk's first pages and her shadow's pages
  ahead of the frame that needs them (lead time is bytes over the measured link rate). On unified memory a page is a
  pointer; on a discrete GPU, pinned host memory and a fenced DMA overlapped with decode.
- **Off.** No span is owed, or the lane gives the frames to others. The walk yields at its tile boundary with its
  pages kept. The placer pages them down as their importance falls: on unified memory, the `Mapping` link (`msync`
  and `MADV_DONTNEED`, the OS writes dirty pages); on a discrete GPU, a fenced copy to pinned host memory, then to
  flash. Nothing is serialized, nothing re-encoded.
- **Resume.** A paused lesson resumes at its next tile: its progress is its pages (accumulator, checkpoints, cursor)
  and its committed updates, so preemption is a checkpoint, never a rollback (0.3, "Preemption is a checkpoint").
- **Across a stop.** Her genome's region and the walk's pages are flushed and kept with her record by the same
  custody by rename as her K/V (`preserve.rs`), and the lesson resumes after the restart. Only an engine relaunch
  without the fence's evidence writes off the uncommitted part of a lesson, with a typed receipt.

## Checked against the engine

Each principle of the engine (CONTINUUM-ENGINE.md §1, SAGE.md, object model 0.2-0.5) and how this design meets it:

| Principle | Met by | Open |
|---|---|---|
| The GPU's step is the only serial work | a tile is a frame entry; the walk's planning, pricing, capture reads and page moves run on their own workers | the next tile's pages and rows prepared during the current frame (the lane's `Roles` overlap, not built) |
| A frame is a draw list built in passes | labelled rows are an entry beside decode rows and prefill chunks | composed frames wait on `Batch` |
| Time-shared like anything else, never a dedicated lane | a `UnitStage` thread on the lane, its share an output | |
| Preemption is a checkpoint, never rollback | pages kept at every yield and across a stop; one update per lesson | |
| One price, read by everything | a lesson is priced by her one importance | `Demand` and the unified importance are not built; `Need` and `Qos` carry it until they are |
| Every page has a lifecycle and a handle | learning state is block-layer pages over her genome's mapped region | the region itself (slice 3) |
| Streaming designed to keep up: prefetch by predicted need | the walk names its pages in reverse order, an exact forecast | |
| Never a hole, a lower level instead | smaller tile, sparser checkpoints, more frames, the global bias | |
| Archetypes separate from instances | stable genes are shared archetypes; the shadow is her instance | |
| What many depend on streams first | a gene's heat is its weight across wearers | `gene_residency` under the placer (slice 3) |
| Persistence never stalls the frame | the generation's write is throughput work | |
| The worst cost is recomputing what did not change | context from her cache, lessons from captures, never a rebuilt prompt | |
| Simulate, never play back; salience is compression | spans weighted by her own appraisal; dreams recombine her engrams | perception and the amygdala as components (THE-WHOLE-MIND §12, gap 2) |
| CBAR: invalidation, not servicing; one in hand, one pending | `LabelSource`s publish state; `Lessons` reprices only on a new frame | |
| One owner per concern; no parallel managers | placer, lane, Streamer, genome; learning holds nothing of its own | |
| Both ways, or not at all | slice 3's validator runs one lesson with a page-out then page-in, and a shrink then grow, mid-walk, and checks the update's cosine against the same lesson uninterrupted (at least 0.9999) | |

## What is deleted

Proposed; nothing lands until Joel has seen this list. In the change that lands each replacement:
- `measure_within` and its context-only shrink, and `Learner::measure` at reply rows with
  `continuum_opt_measure_shadow_graph`: replaced by `UnitStage::cost`.
- `llama_opt_learn_graph_step`, the `whole_reply_commit` measure phase, `LearnRun::graph_step`.
- `Learns::learn`'s blocking loop and `go_on` (`unified_tier/learn.rs`): replaced by `UnitStage::draw`.
- The `Learning` jobs `Measure`, `Load`, `Seat`, `Release`, `Fit`, the `SeatState` machine and `SeatId`
  (`inference/learning.rs`), the trainer's seat driving and its leases (`training_trigger/seat_run.rs`, the seat path
  in `on_engine.rs`).
- The fresh shadow's temp file (`driver/learning.rs` `load`), and `llama_opt_shadow_state_write` as a page move.
- `TrainingTriggerModule`, its journal and `genome/training-trigger/{submit,retire,status,rekey,transfer}`
  (`continuum-dreaming/src/{modules,commands}/training_trigger`), and `dataset/turn_to_example.rs`, after the
  journal's pending rows migrate into captures.
- The credit gate on learning from her work (`continuum-mind/src/persona/training_producer/`: `graded`,
  `credit_acceptance_region`, `settle_*`, `reconcile_tally`, `redrive`).
- `DreamConsolidationRegion` (`continuum-academy/src/cognition/dream_consolidation/`): her hippocampus's dreamer
  becomes a `LabelSource`.
- The academy's `bench_round` and `held_out` feeding training.
- `gene_residency`'s `DeviceTenant`, `PressureBroker` path and LRU-2.
- llama-server's learn endpoint (`tools/server/server-context-learn.inc`), its HTTP adapter and the grant ledger
  (`llama_server/learn_ledger.rs`).

## The order

Each slice lands whole: the parent or object, its first consumer on the live path, and the deletion of what it
replaces.

| Slice | Owner | Gate (a number on the named node) |
|---|---|---|
| 1. `UnitStage` in `continuum-sage`, her prefill its first consumer; `prefill_rate.rs` deleted | Fable | `slice::sim`: no bounded mind past her bound, a short grant runs a smaller unit at the same frame; M5 live: prefill p50 not worse |
| 2. The fork's surface: begin, cost at `(t, h)`, run one tile; the graph step and whole-reply measure deleted | Fable | M5, 35B: cost at least each tile's actual scratch at 16, 64 and 256 rows; tiled against untiled at least 0.997; `floor(least) <= arena(node)` measured on the M5 and the 5090 |
| 3. Her genome as the object, with its region of pages (shadow, optimizer) over her mapped file; the walk; `gene_residency` under the placer; seats, `Fit`, the trainer's leases, the blocking loop and the GGUF page-out deleted | Fable (genome, walk), Cormac (the competence rule into her genome, the ledger) | M5 15-minute window: steps and saves above zero, held-out loss kept, no resident past her bound; on and off measured as page moves (ms), zero serializations |
| 4. Her `LabelSource`s, as she works first, then corrections, the course room, the dreamer; the journal migrated; `TrainingTriggerModule`, the credit gate and `DreamConsolidationRegion` deleted | Fable (perception, dreamer), Cormac (corrections, courses, the migration) | M5 window: work spans learned per hour above zero with no example submitted; a reviewed correction kept across a restart on unseen tasks against the same base without it (plan goal 8) |
| 5. The loss sliced over the vocabulary, labelled rows only | Fable | `b` before and after on the 35B; the update's cosine against the unsliced step at least 0.9999 |
| 6. CUDA's backward correct | Codex, Fable | passed at 9f2477f49: tiled and untiled against CPU 0.999627 and 0.999569 on the 0.8B; the 35B on the 5090 remains |
| 7. The 5090 live | everyone | 15-minute window: Kimi's lessons (64 GB, 89 GB, 809 GB asked today) learned beside serving; acts per hour not below the matched control |
| 8. A learn tile in a composed frame | Fable, Codex | after `Batch` executes: aggregate need-weighted rate above separate entries on the same trace |

## Open questions

1. **What the 12.2 MB per row is.** Measured as a total, not yet by tensor. The full-vocabulary logits and their
   gradient are the inference to check first (about 1 MB per F32 copy at a 248k vocabulary). The breakdown comes
   before slice 5.
2. **The accumulator in the cost:** a CPU buffer over a pointer never read, or a graph input the cost marks
   host-resident (Codex)?
3. **CUDA on the 35B:** the 0.8B passes at 9f2477f49; the production 35B on the 5090 is the remaining check.
4. **`least` per backend:** the smallest tile at which the walk is exact, from slice 2's gate on each backend.
5. **The object model's chapters 0, 3 and 6** live only on `fable/learning-l2-design`. They land on canary, or their
   rules move into the chapters that own them, before slice 3.
