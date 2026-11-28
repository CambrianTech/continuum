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
- [Learning in her stream](#learning-in-her-stream)
- [The engine form](#the-engine-form)
- [Producers, consumers and the block layer](#producers-consumers-and-the-block-layer)
- [Memory: streamed, never reserved](#memory-streamed-never-reserved)
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
- **Labels ride her stream.** Labelled rows arrive incrementally in her own stream as she perceives them (messages,
  tool results, files, her output under correction), and are consumed as they arrive through her ordinary inference.
  The forward is the inference she is already doing; learning adds the backward through her layer for those rows, in
  the same frame. Nothing waits for a span to finish, and nothing is buffered into a lesson (Joel, 2026-11-28:
  "training context comes in via streams NOT blocked off batches").
- **Time-shared like anything else.** Learning is part of her frames on the lane. Each frame carries as many
  labelled rows as its budget holds, measured, and the optimizer's update is its own unit, granted time before it runs.
  Overload spreads learning over more frames; it never drops it (0.4, "Overload slows the clock").
- **Streamed, never reserved.** What a frame's labelled rows need (the shadow slices of the layers they train, those
  slices' optimizer moments and gradients) streams in just ahead of that frame, by predicted need, and streams out
  after it to the cheapest tier that holds it. The backward's scratch is a transient of the frame's own arena, like
  any inference intermediate. Nothing lesson-shaped is held on the device, and nothing is reserved ahead of need
  (Joel: "Reservations will not work. Stream in what's needed and stream it out").
- **Never refused.** Learning keeps the four laws every consumer keeps (ONE-FRAME-ONE-MEMORY.md, "The laws that make
  refusal impossible"): every holding a streamed page or a transient, a measured floor of labelled rows, room only from pages priced below
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
labelled rows learned into one update per period, its progress the rows learned, its switch cost the slices it streams.

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
├─ lessons: Lessons    the labelled rows her parts mark in her stream, priced by her importance
├─ walk: Option<Walk>  an exact reverse pass over a recorded span, for a dream at idle (below)
├─ snapshots           immutable candidate genes at an optimizer boundary
└─ lineage             parents, data, base, exams, wearers
```

Its methods are the only way learning changes her: `grow(basis) -> &mut Shadow` (a fresh shadow made in memory, or
forked from a parent gene), `commit(update) -> Generation`, `basis_for(competence) -> GeneBasis` (today's
`competence::decide`, moved to its owner), and `adopt(snapshot)` after its exam (a pointer swap between frames).
How an update spreads across the genes active in her phenotype, by their weights, is the genome's (THE-WHOLE-MIND
§10, "learning spread by weight"); the shadow is its first form.

**The walk**, an exact reverse pass over a recorded span, run when a dream has the idle Core: her stream's learning has no walk state (see "Learning in her stream"). A `UnitStage` whose unit is rows, and a `Resident`:

| Row | Residency | Varies | Ways |
|---|---|---|---|
| shadow and optimizer (her genome's `LearningPages`) | Pageable | `Elastic`: a tile needs only the slices of the layers it trains | `cache_out` by link, paged back before her next tile |
| K/V gradient accumulator for the lesson's span, and the recurrent-state gradient carried across tile boundaries on a hybrid's linear-attention layers | Pageable | `Elastic`: a tile reads and writes only its rows' slice | `cache_out` by link |
| recurrent checkpoints | Pageable | `Elastic { floor: one }` | `collapse`: fewer checkpoints, priced in replay time; `cache_out` |
| a tile's scratch | Transient, in the Core's compute arena, which the placer sizes for the frame that composes it | sized by the unit | the unit itself |

Its `Requires` names what each row needs beside it (her K/V for the lesson's context, her genes at the lesson's
basis), so the placer never frees one without the other.

**The walk cannot touch her live tail, by type** (Codex). Her identity and continuity survive every change in what is
resident, so the walk's API holds no write to her sequence at all:

```rust
/// The rows a lesson learns from: read-only views by handle (her K/V pages, the capture), never her sequence.
pub struct LivedSpan<'a> { /* page handles, positions, the adapter generation, and each row's routed experts, as lived */ }
/// What the walk may write: its own pages (accumulator, checkpoints) and its tile's scratch.
pub struct WalkScratch<'a> { /* transient and walk-owned pages only */ }

impl Walk {
    fn draw(&mut self, lived: LivedSpan<'_>, scratch: WalkScratch<'_>, unit: Rows) -> Did<Rows>;
}
```


**Routing is part of what was lived** (Cormac, on the 35B). On a mixture-of-experts base, a near-tie in a row's top-k
can choose a different expert when the arithmetic differs slightly (a tile of 64 rows against one of 512, CPU against
CUDA), and the discrete choice turns a 1e-4 difference into a different function. The walk therefore replays each
row's expert choices as recorded when she lived it, never routing it again: it trains the function she served,
deterministically, and skips the router's top-k. The recorded choices ride with the span (a few bytes per row per
layer), captured where serving already computes them.
Rewinding recurrent state happens in the walk's scratch, reconstructed from a checkpoint; her own state is never
restored over. While she speaks during a lesson, serving owns her advancing tail, the walk reads only the span it
named, and a turn appended after that span is untouched by construction. The commit introduces a version boundary,
and the context owner validates cache reuse across it.

**Engineering uses the same machinery, with nothing held back** (Joel). The rule above is the runtime product's: the
installs no one is working on, where a mind's continuity is the point. Engineering on a development node is dirty by
design. Restoring a checkpoint, rewinding or replaying a mind, corrupting or deleting a persona to reproduce a fault
are ordinary tools there, with no grant to ask for. They act on the same state, frames and pages as runtime, so a
captured failure replays faithfully and one thing can be varied at a time.

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

## Learning in her stream

Labelled rows are rows of her stream with labels on, consumed as they arrive, against her cache in place.

1. **Arrive.** A source marks rows of her stream as they enter it (her perception's chunk of a message or tool
   result, her own tokens under a correction), each with its label weight. A partial arrival is learned as far as it
   has arrived; chat that comes between is served as usual and learned only if it is labelled.
2. **Stream in.** Ahead of the frame that carries them, the Streamer brings in, by predicted need, only what those
   rows touch: the shadow's slices for the layers they train, those slices' optimizer moments and gradient
   accumulators. On unified memory a slice is a pointer; on a discrete GPU, pinned host memory and a fenced DMA
   overlapped with the frame before.
3. **Forward.** Two kinds of labelled row, and the cost counts each (Cormac, 2026-11-28):
   - **Labelled as they arrive** (a perceived message, a tool result, a file): the label is known when the row is
     prefilled, so the serving prefill is the forward, with her layer on, and only the backward is extra.
   - **Labelled after she decoded them** (her own output, corrected later): decode kept no activations for a backward,
     so those rows run a learning forward, a prefill of just those rows against her K/V before them, in a later frame.
     Its cost is in `cost(rows, layers)` as `rows_rerun`.

   Her context is read in place, never recomputed for learning.
4. **Loss and backward.** Cross-entropy at the labelled rows, at their weights, then the backward through her layer
   for those rows only, in the same graph and the same fence. Her layer's gradient accumulates into its slices in
   place. The backward's scratch is a transient of the frame's arena.
5. **Stream out.** After the frame the slices it touched go back to the cheapest tier that holds them. Nothing of the
   learning stays on the device between frames.
6. **Update.** The optimizer's update is its own unit, granted time before it runs, at a period the controller sets
   from feedback (her owed labels, the frame budget). It normalizes by the summed label weight of every row in the
   period, never per frame. One durable update per period, persisted off the frame (0.2, "persistence never stalls
   the frame").

**She serves with what she learns: online, and the drift is measured** (Cormac). The forward is shared, so she serves
with the shadow she is training: its front buffer, the generation of the last update. An update swaps in a new
generation between frames, under K/V and recurrent state her earlier rows computed with the old one. That is the
intended form (she improves as she works), and its cost is measured, never assumed: the next-logit KL on a fixed probe
before and after each update, and the context owner validates cache reuse across the version boundary (as for any
generation change). Learning never writes her served tail: the backward writes only gradient slices, and the update is
a pointer swap.

**Composed frames.** When the lane composes a frame from several minds (`Batch`, THE-LANE-ON-SAGE.md open question 1),
one mind's labelled rows ride beside the others' decode rows in one dispatch and one fence, the backward only over
those rows.

**What the stream gives up, measured.** A row's gradient here flows through her layer at that row and the rows of its
own frame. It does not flow back across the frame boundary: not into the K/V of rows learned in earlier frames, and
not into the recurrent state her linear-attention layers carry across frames on a hybrid (Qwen3.6). The exact walk
(the fork's tiled reverse pass) keeps both. Whether that matters is measured by what she retains on held-out work
against the same base, with both truncations named in the measure, not assumed; the exact walk stays the tool for a
dream at idle, where it costs no one a frame.

## The engine form

Conventional training is one fat synchronous loop: a batch, a forward, a backward, an optimizer step, every buffer
reserved up front. A game engine inverts each of those: the frame is the only thing that cannot slip, every resource
is transient or streamed, work is sized by measured budget, and whatever is not the frame runs on workers (Joel,
2026-11-28: "take what we know about ml training and turn it upside down into something more like a graphics engine
but still works").

| Training habit | Engine form | Her learning |
|---|---|---|
| A batch | samples in a frame | a frame carries as many labelled rows as its budget holds; few per frame is enough, because the result accumulates |
| Gradient accumulation | temporal accumulation (TAA) | each frame adds a cheap estimate into a history buffer in place, a plain weighted sum within the period: Adam's moments are already the exponential filter, so decay lives only there, never twice (Cormac) |
| The optimizer step | a resolve pass on a worker | a low-rank layer's gradient is megabytes: read back, Adam on a CPU worker against host-resident moments, written to a back buffer; the device never runs the optimizer |
| Weights updated in place | double buffering | the front buffer serves, the back buffer is written, a pointer swap at a frame boundary; no stall, no torn read |
| Every parameter every step | LOD and update rate per layer | layers whose gradient is small or settled update less often, and their slices stream in less often; Adam's bias correction counts each layer's own steps, so a rarely updated layer never takes an oversized first step (Cormac) |
| fp32 everywhere | precision LOD | precision is a level of detail per slice, over training time (below) |
| Uniform sampling | variable rate and culling | rows she already predicts well give almost no gradient and are culled before the backward; her surprise, weighed by her amygdala, sets the rate. A culled row leaves the period's denominator (so the step does not shrink as she improves), and a small importance-weighted random sample of low-loss rows is kept as the term against forgetting what she already does well, measured in held-out retention (Cormac) |
| Activations kept for backprop | frame-graph transients | the forward's intermediates the backward reads alias in the frame's arena and die with it; only the history buffer survives |
| All weights resident | virtual texturing (RAGE) | a frame records which slices its rows touched; workers stream exactly those in for the next frames and evict cold ones |
| A fixed batch size | dynamic resolution | the labelled rows per frame scale to hold the lane's bound, up and down, from measured frame time |
| Training as a job | async compute in the gaps | the device does one extra thing, the backward on the frame's labelled rows |

**Detail by distance, decided every frame** (Joel: "It does what it can with what it has, and asks for what it needs,
expands and contracts... like light or water effects... higher detail closely and potentially even some effects are
off on the distance, but that only depends on current demands. It's able to do full resolution far off in the distance
but it's making priority decisions continuously"). There are no modes. Every frame, each labelled row's detail is
priced by its distance from her camera, which is what matters now:
- **Near**: her current activity, a correction just received, a surprise her amygdala flagged. Every layer trained,
  full precision, no culling, a short update period.
- **Far**: routine stream content, an idle mind's backlog, a low-salience read. Fewer layers, more culling, a longer
  period, or nothing this frame when the budget is tight.
- **Headroom** raises everything: on a quiet node far rows learn at full detail, and a dream's exact walk runs.

Learning asks for what it needs each frame (its owed rows, priced by her one importance) and is granted detail by the
one controller that grants decode, prefill and memory; it expands when granted, contracts when serving needs the room,
and never decides alone that it cannot run.

**Precision is a level of detail, per slice, over training time** (Joel: "lower precision Lora first and as training
goes along transfer to higher fidelity... early on the error is so high you might as well operate at a lower bit size...
only really care about ones that need the fidelity... individually some areas"). Like a texture's mip chain: coarse
until the difference is visible, refined only where it is.
- **Start coarse.** A new shadow trains at a low bit width: its weights, gradients and moments. Stochastic rounding
  keeps each rounded value unbiased, but Adam's normalization is nonlinear, so the update is not unbiased by that alone (Codex): the coarse step is replayed against a high-precision control on identical labels, across seeds, including tiny gradients and rare outliers, and compared step by step.
- **Promote a slice when it needs fidelity.** Each slice (a layer, or a block of one) keeps its level while its typical
  update spans many quantization steps, and is promoted when its updates approach the step, where real progress would
  round away.
- **Demote when it settles or falls far.** A converged slice, or one far from her camera, drops back a level and is
  served and stored compactly, promoted again if it moves.
- **The accumulation rule** (refining Cormac's note that a few rows a frame round to zero): a slice's history buffer
  and moments resolve its per-frame contribution, or round it stochastically. Measured per slice, never a fixed fp32.

Each region of one gene sits at its own level, from feedback; on an edge device most of a lesson's life is in the
coarse phase, which is where its memory is saved.

**The levers are a learned policy** (Joel: "the algorithm learns to optimize this too"). The labelled rows per frame,
the layers given detail, each slice's precision level and when it is promoted or demoted, the culling threshold and
its sample against forgetting, and the update period are the actions of one SAGE `decide` use case, beside the lane's
(THE-LANE-ON-SAGE.md). It observes each frame's budget and pressure, her importance per row, each slice's gradient
size against its quantization step, and the measured link rates; its reward is what she retains and how she takes a
correction, per byte and millisecond spent, under the lane's constraint that no mind passes her wait bound. The rules
written above are its starting prior and its envelope, never the policy: like the lane, the envelope bounds what it
may choose and it learns inside it, from receipts.

**On the workers, concurrently, never in the frame:**
- **Data:** marking labels in her stream, pricing rows by her importance, culling well-predicted rows, reading
  captures for a dream.
- **Memory:** streaming slices in and out from the frame's feedback, prefetching by predicted need, compaction,
  flushing to her mapped region.
- **The optimizer:** the resolve on host-resident moments and the back-buffer write.
- **Persistence and checks:** generations written, held-out retention measured at idle.

**What keeps it correct:** one owner each (the frame owns the device, the workers own everything else, the swap is the
one point they meet); and every rate, threshold and level measured from feedback, never set as a constant: retention
from accumulated small-sample gradients against the same base, the culling threshold, the per-layer update rates.

## Producers, consumers and the block layer

Everything resource-related is centralized: producers and consumers, the one placer's leases, and the Streamer moving
blocks between them (Joel, 2026-11-28: "Everything resource related has to be centralized into producers and
consumers, leases and workers to move it around... keep to block copies and caches, never iterate unless it's
absolutely necessary. DMA ftw"). Learning owns no mover, no pool and no copy of its own. Its state is blocks
(`continuum-blocks`: `Block`, `Region`, `Placement`), and every move is one the hardware already does, through the one
`Link` a pair of tiers has: a pointer on unified memory, the OS paging a mapped file, one DMA of one block with a fence
on a discrete GPU. Never a copy loop, a staging copy or an encoding.

| SAGE component | Produces | Consumes | Its work |
|---|---|---|---|
| Her label marker (a `LabelSource`'s worker) | labelled rows ready to learn (state, `Key::ONE` per mind): positions, weights, the culled set | her stream as it arrives (the capture), her importance | marks, weighs and culls rows off the frame; only ready rows reach a frame |
| The frame (the driver, the tier, the fork) | a gradient block per slice it trained, its completion event | ready labelled rows; the shadow's front buffer and the slices its rows train, placed resident | the forward as served and the backward on those rows, in one graph and one fence |
| The Streamer (`continuum-blocks/src/streamer.rs`) | `Placements` | `Demand` (the frame's forecast of the slices its next rows train), `Landed` events | moves each block through its `Link`, soonest first; a source stays charged and valid until its move lands |
| Her resolver | the back buffer's slices, written | landed gradient blocks, host-resident moment blocks | the optimizer's update over whole blocks at once (SIMD over a contiguous block, the one place element math is the work itself) |
| The swap | the new front buffer | a resolved back buffer, a frame boundary | an `Exchange`: two blocks trade regions by pointer, no byte moves |
| Her persister | a durable generation | a swapped generation | the `Mapping` link writes its dirty pages back; nothing serialized |

The placer charges every block where it lies, through the one authority, and a block's charge follows its placement.
Nothing is leased ahead of need: a frame's slices are demanded for the frames that will use them, and a slice no
frame demands streams down.

On unified memory (the M5) the gradient readback is a pointer: the resolver reads the frame's gradient block where it
lies once its completion event fires, and no byte moves. On a discrete GPU it is one fenced DMA of one block into pinned
host memory, overlapped with the next frame.

**Measured on both node types** (Codex): each block's custody from production through completion, the overlap of
readback, resolve and swap with serving frames, and their latency in the frame traces.

## Memory: streamed, never reserved

What a frame's learning costs is measured, never assumed:

```text
cost(rows, rows_rerun, layers) = backward scratch over `rows` + a learning forward over `rows_rerun` + the slices of `layers` streamed in
```

On the M5's 35B with native attention backward the scratch is about 12.2 MB per labelled row, most of it the
labelled rows' logits and their gradient over a 248k vocabulary; the loss computed over the vocabulary in slices
(slice 5 below) is what shrinks it, and on a constrained device it comes first.

**Nothing is reserved.** No lesson-sized block, no claim held across frames, no lease taken ahead of need. A frame
carries the labelled rows its arena holds this frame; the slices they need stream in for it and out after it.

**Short of room, fewer rows, never a hole.** A tight frame carries fewer labelled rows, or trains fewer layers' slices,
and learning catches up over later frames (0.4, "Overload slows the clock"). When the whole node is over budget, the
placer's one bias steps every holder down a level a little (0.2, "Over budget, lower detail everywhere a little"),
learning's rows among them. Learning is never refused and never takes a resident's warm set.

**One placer for genes too.** `gene_residency` becomes a `Resident` under the placer, its pages moved by the
Streamer. Its `DeviceTenant`, its `PressureBroker` path and its own LRU-2 are deleted; a gene's heat is its weight in
the phenotypes that wear it, read from the one importance (0.2, "What many depend on streams first").

## On and off

Learning switches on and off with her stream, frame by frame:
- **On.** A frame carries labelled rows. The Streamer brings in the slices those rows train ahead of it (lead time is
  bytes over the measured link rate). On unified memory a slice is a pointer; on a discrete GPU, pinned host memory and
  a fenced DMA overlapped with the frame before.
- **Off.** A frame carries no labelled rows, or the lane gives the frames to others. Nothing is held: the slices the
  last frame touched have already streamed out, on unified memory by the `Mapping` link (`msync` and
  `MADV_DONTNEED`, the OS writes dirty pages), on a discrete GPU by a fenced copy to pinned host memory, then to flash.
  Nothing is serialized, nothing re-encoded.
- **Resume.** Labelled rows not yet learned stay marked in her stream's capture; a later frame takes them. Progress is
  the rows learned and the committed updates, so preemption is a checkpoint, never a rollback (0.3, "Preemption is a
  checkpoint").
- **Across a stop.** Her shadow, its moments and the accumulated gradient are flushed with her record by the same
  custody by rename as her K/V (`preserve.rs`); unlearned labelled rows resume from the capture after the restart.

## Checked against the engine

Each principle of the engine (CONTINUUM-ENGINE.md §1, SAGE.md, object model 0.2-0.5) and how this design meets it:

| Principle | Met by | Open |
|---|---|---|
| The GPU's step is the only serial work | labelled rows ride her frame; marking, pricing, capture reads and slice moves run on their own workers | the next frame's slices streamed in during the current frame (the lane's `Roles` overlap, not built) |
| A frame is a draw list built in passes | labelled rows are rows of her frame, beside decode rows and prefill chunks | composed frames wait on `Batch` |
| Time-shared like anything else, never a dedicated lane | learning is part of her frames; its share of rows is an output of feedback | |
| Preemption is a checkpoint, never rollback | unlearned rows stay marked; one durable update per period | |
| One price, read by everything | labelled rows are priced by her one importance | `Demand` and the unified importance are not built; `Need` and `Qos` carry it until they are |
| Every page has a lifecycle and a handle | learning state is streamed pages over her genome's mapped region | the region itself (slice 5) |
| Streaming designed to keep up: prefetch by predicted need | a frame's labelled rows name the layers, so the slices they train are an exact forecast | |
| Never a hole, a lower level instead | fewer labelled rows or fewer layers' slices per frame, more frames, the global bias | |
| Archetypes separate from instances | stable genes are shared archetypes; the shadow is her instance | |
| What many depend on streams first | a gene's heat is its weight across wearers | `gene_residency` under the placer (slice 5) |
| Persistence never stalls the frame | the generation's write is throughput work | |
| The worst cost is recomputing what did not change | the forward is her inference as served; context from her cache, never a rebuilt prompt | |
| Simulate, never play back; salience is compression | rows weighted by her own appraisal; dreams recombine her engrams | perception and the amygdala as components (THE-WHOLE-MIND §12, gap 2) |
| CBAR: invalidation, not servicing; one in hand, one pending | `LabelSource`s publish state; `Lessons` reprices only on a new frame | |
| One owner per concern; no parallel managers | placer, lane, Streamer, genome; learning holds nothing of its own | |
| Both ways, or not at all | slice 2's validator streams slices out and back mid-stream, and a tight frame then a roomy one, and checks the update against the same stream uninterrupted (at least 0.9999) | |

## What is deleted

Proposed; nothing lands until Joel has seen this list. In the change that lands each replacement:
- `measure_within` and its context-only shrink, and `Learner::measure` at reply rows with
  `continuum_opt_measure_shadow_graph`: replaced by `UnitStage::cost`.
- `llama_opt_learn_graph_step`, the `whole_reply_commit` measure phase, `LearnRun::graph_step`, and the fork's past-one-ubatch gate (`opt_tiles_past_ubatch`), under which every reply of up to one ubatch built the whole-reply graph (the M5 crash loop, 2026-11-28 14:08-14:58Z).
- The reply buffering that turns her stream into a lesson batch before it is learned, and `learn_next`'s idle-only path (`driver.rs`: lessons ran only when no request waited).
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
| 1. Labels in her stream: the tier marks labelled rows of her stream as they arrive, and her serving decode runs the backward on them in the same graph (the fork's `opt_mark_training` path), accumulating into the history buffer; rows labelled after they were decoded run a learning forward first; the optimizer's resolve runs on a worker into a back buffer, swapped at a frame boundary. Until slice 3, a frame carries only the handful of labelled rows its budget holds (12.2 MB a row on the M5's 35B). Deleted: the whole-reply graph and its past-one-ubatch gate, `llama_opt_learn_graph_step`, `Learns::learn`'s blocking loop and `go_on`, `learn_next`'s idle-only path, the reply buffering | Fable (tier, driver, fork), Codex (review, the frame) | M5, 35B: learning's incremental high-water above the same warm serving resident, by owner, at most the frame's labelled rows' scratch; a partially arrived lesson with chat between, she answers between frames with her state intact; one durable update per period; matched 15-minute windows, no resident past her bound; held-out retention against the same base, both truncations named; next-logit KL on a fixed probe across each update; the coarse step against a high-precision streamed control on identical labels across seeds, with tiny gradients and rare outliers: per-slice update error, squared-gradient range, steps against the reference, retention after demotion and promotion; correction uptake after reload; readback, resolve and swap latency and block custody in the frame traces, on both node types (Codex) |
| 2. Streamed slices: the shadow's, moments' and gradients' slices for the layers a frame trains stream in ahead of it by predicted need and out after it (the Streamer) | Fable | between frames, zero device bytes held by learning; a slice's move measured in ms, zero serializations |
| 3. The loss sliced over the vocabulary, labelled rows only | Fable | the per-row scratch (12.2 MB on the 35B) before and after; the update against the unsliced step at least 0.9999 |
| 4. Her `LabelSource`s, as she works first, then corrections, the course room, the dreamer; the journal migrated; `TrainingTriggerModule`, the credit gate and `DreamConsolidationRegion` deleted | Fable (perception, dreamer), Cormac (corrections, courses, the migration) | M5 window: work rows learned per hour above zero with no example submitted; a reviewed correction kept across a restart on unseen tasks against the same base without it (plan goal 8) |
| 5. Her genome as the object, its learning state streamed pages of her mapped region; `gene_residency` under the placer; seats, `Fit` and the trainer's leases deleted | Fable (genome), Cormac (the competence rule, the ledger) | on and off measured as page moves (ms) |
| 6. CUDA's backward correct | Codex, Fable | passed at 9f2477f49 on the 0.8B; the 35B on the 5090 remains |
| 7. The 5090 live | everyone | 15-minute window: Kimi's lessons learned beside serving; acts per hour not below the matched control |
| 8. Labelled rows in a composed frame | Fable, Codex | after `Batch` executes: aggregate need-weighted rate above separate entries on the same trace |

## Open questions

0. **The 35B's tiled update disagrees with the untiled one** (CPU: cosine 0.9790 at 64-row tiles, a difference 20.46% of
   the reference update's L2 norm, while 512 rows match byte for byte; CUDA 0.9924). Method (Codex): find the first
   tile boundary where values diverge, then raise the precision of only the state or accumulator crossing that
   boundary. If agreement returns, those bytes need the fidelity and everything else stays compact. Gates slice 2, measured
   with routing pinned to the reference run's expert choices, so the walk's exactness is separated from routing flips (Cormac).
1. **What the 12.2 MB per row is.** Measured as a total, not yet by tensor. The full-vocabulary logits and their
   gradient are the inference to check first (about 1 MB per F32 copy at a 248k vocabulary). The breakdown comes
   before slice 5.
2. **The accumulator in the cost:** a CPU buffer over a pointer never read, or a graph input the cost marks
   host-resident (Codex)?
3. **CUDA on the 35B:** the 0.8B passes at 9f2477f49; the production 35B on the 5090 is the remaining check.
4. **`least` per backend:** the smallest tile at which the walk is exact, from slice 2's gate on each backend.
5. **The object model's chapters 0, 3 and 6** live only on `fable/learning-l2-design`. They land on canary, or their
   rules move into the chapters that own them, before slice 3.
