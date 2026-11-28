# Continuum Engine

How a mind's act is made fast: the picture the runtime is built to, the efficiency law every data path follows, the
engine's objects and its step loop, a mind's context codec, admission and residency, configuration, the performance
model, and how all of it is measured. Owners: Fable (engine, codec), Codex (serving, admission, the discrete GPU),
Cormac (learning on the engine). Edit it like code: a section changes in place when its object changes, superseded
text is deleted, history and receipts live in commits and PRs.

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [1. The picture: a game engine for minds](#1-the-picture-a-game-engine-for-minds)
- [2. The efficiency law](#2-the-efficiency-law)
- [3. Gates](#3-gates)
- [4. Objects and ownership](#4-objects-and-ownership)
- [5. The step loop](#5-the-step-loop)
- [6. A mind's context](#6-a-minds-context)
- [7. Admission and residency](#7-admission-and-residency)
- [8. Engine configuration](#8-engine-configuration)
- [9. Custody and stop](#9-custody-and-stop)
- [10. The performance model](#10-the-performance-model)
- [11. Measurement and simulation](#11-measurement-and-simulation)
- [Former section numbers](#former-section-numbers)

## 1. The picture: a game engine for minds

Many persistent minds share one live model the way a game shares one GPU. A game engine does not run the world one
object at a time: it keeps one frame pipeline full, streams the world around the player's view, gives every system
its share of the frame, and never lets the GPU wait on the CPU. Continuum is built to that picture, system by
system, and each system has one chapter:

| In a game engine (RAGE, CBAR) | What it does there | Here | Chapter |
|---|---|---|---|
| CBAR's `QueueThread<T>` and frame pipeline | every stage on its own worker, woken by a push, passing the newest frame by handle | a `Component` per concern; `State` (newest, by frame number) and `Stream` (every event); the frame counter is the heartbeat | [SAGE.md](SAGE.md) |
| The GPU frame | the one serial resource every system feeds | the Core's step: one mind or a batch, the only serial work in a mind's life | §5, [THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md) |
| The job scheduler and frame budget | every system gets its share of the frame; hard deadlines for the frame draw, the rest fitted in | the lane: a `decide` use case at every step boundary, an envelope no mind's wait may pass, shares by class from feedback | [THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md) |
| Render passes as a graph (RDG), async compute | passes declare what they read and write; independent passes overlap | stage runs with declared triggers, cost and a unit the scheduler sizes; prefill during a tool call | [STAGE-CONTRACT.md](STAGE-CONTRACT.md) |
| The streaming system | the world paged around the player's view by priority, fenced, a coarser mip rather than a hole | the block layer and its `Streamer`: a mind's KV, weights, experts, genes on tiers, moved by pointer, DMA or the OS's paging | [UNIFIED-PAGING.md](UNIFIED-PAGING.md) |
| Memory budgets and pools, the transient frame allocator | each pool sized by need, transient memory aliased across a frame | claims on one authority, holders that report and give back, a transient arena sized to the largest need | [SHARED-RESOURCES.md](SHARED-RESOURCES.md) |
| LOD, mip bias, temporal reuse | detail by how much it matters; reuse last frame, render the delta | the context codec: items at levels of detail, a context that only appends, an act that prefills only her new tail | §6 |
| Incremental work across frames | reuse valid results and prepare the next frame asynchronously | in step 4, labels arriving with input share the resident's forward pass. Later corrections replay the required forward work. The same frame grants backward work and workers prepare the update. | [IN-PROCESS-LEARNING.md](IN-PROCESS-LEARNING.md) |
| The profiler and replay | every frame measured, any frame replayed | probes, records, the lane's history, simulators fitted from live traces | §11 |

Five rules follow from the picture and bind every chapter:
1. **The GPU's step is the only serial work.** Everything else is a component feeding it, and the next step's inputs
   are ready before the current one lands.
2. **Nothing waits that need not**, and a wait ends on an event its owner publishes, never on a timer or a poll.
3. **Nothing is refused that could be reshaped.** Work that does not fit now fits smaller (space) or later (time);
   only a proven "never here" is typed.
4. **Nothing is fixed in advance.** No seat count, quantum, pool size or threshold someone picked: each is a
   measurement or a policy's output, and every policy learns from what its choices did.
5. **Every byte stays native and every reuse is taken** (§2).

## 2. The efficiency law

The review standard for every data, storage, retrieval and serving change. A change that cannot say which rule it
follows, and show its end-to-end timing and output quality before and after, is not ready.

### 2.1 What the CPU does per byte

The first question for any data path: what does the CPU touch per byte, and can it be zero?
- **Bytes stay bytes.** A producer's native bytes are stored and moved as they are (a blob beside a typed
  descriptor: element type, shape, byte order, producer). A typed view over bytes, never a walk that converts them.
- **No per-element CPU loops.** A load is one bulk copy or none.
- **Translation, when unavoidable, happens on the GPU** inside the kernel that consumes the data (dequantization,
  f16 to f32, layout), never as a CPU pass and never per use.
- **Never parse until the last step.** JSON only at a true external boundary; rows decode straight to the typed
  columns a caller asked for; a payload stays opaque until its addressed consumer opens it.
- **No SQL outside the ORM, no Python anywhere.** Filter, sort, count and limit run inside the store.
- **No transfer that is not needed.** On unified memory the stored pages are the GPU buffer; on a discrete GPU one
  asynchronous DMA, overlapped with compute and double-buffered; the CPU reads back only the answer.

### 2.2 Carmack rules for the math

- **Never compute what does not change the answer.** Ranking needs order: squared distances, never a root; store
  unit vectors so cosine is a dot product; normalize the query once per search.
- **Hoist everything invariant out of the inner loop.**
- **Coarse first, exact only where it decides.** A sign-bit sketch with popcount culls; exact dot products run on
  the shortlist; partial top-k, never a sort.
- **Spend only the precision the consumer perceives**, with the error bound stated and checked by a validator.
- **No branches, asserts or allocation in inner loops.** Validate once at the boundary.

### 2.3 The exhaustive checklist

Before a data, compute or serving change is reviewed, its author walks this list and states, for each line that
could apply, that it is taken or the measured reason it does not pay.

**Data and memory:** native bytes with a typed descriptor, zero copy where alignment allows, one bulk copy
otherwise; structure of arrays for anything scanned; cache-line alignment for hot data, no false sharing; arenas and
pools, buffers owned once and passed by handle, no allocation on a hot path; resident sets bounded by predicted
reuse, everything that accumulates drains; precision per consumer with a validated bound; compression only where it
is cheaper than the bandwidth it saves.

**Compute:** hoist invariants and precompute at write what every read would recompute (unit norms, sketches, prefix
sums); no roots, divides or transcendentals where a comparison or a lookup decides the same; branchless inner loops;
fused GPU kernels over separate passes, SIMD on the CPU only where the GPU is not the better owner; coarse then
exact; incremental update over recompute.

**Transfers and I/O:** the pointer, the DMA or the OS's paging (UNIFIED-PAGING.md), never a copy loop; read back
only the answer; sequential, mapped reads; write coalescing behind ring buffers, never on the request path; binary
frames on every wire.

**Scheduling and concurrency:** the act is a dependency graph and independent passes overlap (prefill during tools,
retrieval during perception); batch across minds where the measured cost says it pays; wake on events, never poll,
spin or hold a lock across an await; drop stale work instead of draining obsolete queues; hold the act-time target
by reshaping, never by refusing.

**Reuse:** one shared prefix copied across minds, never prefilled again; prefill only the appended tail;
content-addressed caches kept while predicted reuse beats recompute.

### 2.4 How a change proves itself

Frame time is act time. Every change posts, on a replay validator and on a live node: act latency (median and
p90), the stage it targets (prefill and decode tok/s, search latency, store op p50 and p99), memory (free,
compressed, swap, footprint) and output quality (recall@k, review outcome). A health line's label is not a
measurement.

## 3. Gates

A change to the engine reports both rates, before and after, on the live node:

| Gate | Target | Measured by |
|---|---|---|
| Delivered tokens per second over 900 s of real resident work, switches and waits included | at least 60 on the M5, 200 on the 5090, with per-resident progress reported | `Record` turn rows (§11) |
| Engine occupancy while any turn waits | `busy_permille` at least 950, no idle gap a step's inputs could have filled | `EngineSnapshot` (§11) |
| Prefill per act | equal to the appended tokens (§6) | the prefill decision probe |
| No ready mind past her bound | `Urgency::act_bound` (1 s for `Want` and `Urgent`) | the lane's records |

Two rates, never confused. The **wall-window rate** is output tokens delivered over the whole window's wall time:
prefill, switches, waits and idle count against it, and it is the gate. The **decode-only rate** is output tokens
over time in decode steps: the engine's speed for one mind at her depth (26-32 tok/s on the M5 at depth without the
MTP draft). So 60 needs batching, the draft and prefill overlapped with decode, not one mind alone.

## 4. Objects and ownership

| Object | Where | Owns |
|---|---|---|
| `PersonaEngine` | `continuum-persona-engine/src/engine.rs` | the engine's decode: a step of the active mind after the previous step landed, staging the next mind, the handoff once her fence lands |
| `Roles` | `pipeline/roles.rs` | which mind is active, staged and retiring, and the transitions between them; exactly one active, by construction; a refusal returns every owned value it received |
| `PersonaUnit` | `persona/unit.rs` | her parts as one value (blocks, token history, basis, learning, decode marks); the only constructor takes every part, so a move is whole by type |
| `MindId { persona, epoch }` | `persona/id.rs` | her identity; the fork's stamp is derived from it inside the tier only |
| `Tier`, `Fence`, `Execute` | `residency/{tier,fence,execute}.rs` | the native moves for her blocks under one contract for every memory; a move's completion, tied to the mind and direction; the decode a tier runs for the mind bound to its stream, taking her tokens, never a sequence id |
| `Footprint`, `Lifecycle` | `persona/{footprint,lifecycle}.rs` | what a node holds is the sum of its children; each owner suspends its own children |
| `protocol` | `protocol.rs` | the one seam between minds and the engine: typed commands in, typed events out |
| the scheduler and its driver | `continuum-engine/src/inference/backends/persona_scheduler.rs`, `.../driver.rs` | the in-process engine over one libllama context, created by `in_process_engine.rs`: the step loop (§5) |
| the lane's executor | `persona_scheduler/driver/lane.rs` | carries out the lane's choice and reports what happened (THE-LANE-ON-SAGE.md) |
| `EngineGeometry` | `persona_scheduler/geometry.rs` | what the context actually applied, named by engine incarnation (§8) |
| `PreservedMinds` | `persona_scheduler/preserve.rs` | what outlives a restart (§7) |
| `Emit`, `Record` | `persona_scheduler/{emit,record}.rs` | an answer's text and stops; every measurement of a step and a turn (§11) |

**The grid is one more tier.** A mind is a value her blocks travel with, so moving her to another node is a tier
move whose transfer crosses the network; its fence lands when the peer holds her blocks, and a refusal returns her
whole. Proposed: before that tier's shape is fixed, a replay covers a lost acknowledgement (the peer accepts her,
its ACK is lost, the sender retries, the old completion arrives after the stream was reused); the two nodes agree on
one owner of her decode, and a byte stream received alone never grants a second decode authority.

Proposed, the rest of the object map, each row deleting the copies it names: one `EngineConfig` (§8); `Emit`
owning an answer end to end, the backend's collector and its per-turn blocking spawn deleted, media joining the one
decode path as an extension of her activity; `Record` deriving every turn time from one span per step; the engine as
a `Lifecycle` node whose children are its minds; one owner each for prefill and for scheduling (`turn::plan` and
the driver's leftovers folded into the lane); then the lanes, `kv_page`, served windows and the media bypass
deleted (serving, grid and governor: Codex; engine planning, persona and spawner: Fable).

## 5. The step loop

The engine is an object you submit to, and its thread is a step loop. It never serves a whole turn in one call:
each step advances the mind (or the batch) the lane chose by one unit, a prefill chunk sized to the slack or a
decoded token, so no mind waits out another's whole prompt (before the lane, turns waited 300-360 s behind each
other's prefills, and a five-seat pool paged 4,191 times in 15 minutes).

- **Who steps** is the lane's decision at every step boundary (THE-LANE-ON-SAGE.md); the driver only executes it
  and reports each switch, step, turn end and residency into the lane's history.
- **Nothing on the engine thread waits.** Everything else is a SAGE stage on its own worker (`continuum_sage::Stage`,
  CBAR's `QueueThread<T>`): **Emit** turns tokens into text, streams them to the caller and watches for stop
  sequences, and a stop it sees or a caller that hung up raises the answer's cancel flag, read at the next step
  without a lock; **Record** merges each step's span into busy windows, publishes the `EngineSnapshot` and writes each
  turn's row. Each stage's worker is its one lifecycle owner, joined after the driver exits.
- **The protocol** (`protocol.rs`): a mind runs many activities (a card, a conversation, a review, her own
  reflection), and her identity and memory are the prefix they share. A command is sent and the sender goes on; its
  outcome is an event the interested stage reacts to; a tool's result extends the activity that called it whenever
  it arrives. Commands to one activity apply in order; activities interleave as the lane decides.
- **A lock only guards an enqueue.** A permit or lock held across IO, compute or another wait serializes everything
  behind it.
- **Proposed: overlap.** The next mind's switch, her tokens and her graph are prepared while the current step runs,
  and her new tail is prefilled while she is in a tool. Gate: GPU idle between steps near zero (§3).

## 6. A mind's context

A mind's context has one codec, the `continuum-context` crate, built like a video
codec over mipmaps. It owns what a mind perceives, in what order, at what level of
detail, and when any of it is rewritten. No layer and no faculty re-implements any
of that. This section is the design; each rule is marked **Built** (with the code
that implements it) or **Proposed** (designed, not in the tree).

Why one owner: Aiko read one 51,827-char file whole 9 times in 43 acts, because three
owners each decided a piece of what she saw (working memory collapsed results against
its own share, a hand table set the restart order, the view's start stopped keeping
items whole at the first one too large). Every incident had been fixed where it
happened, so the next one appeared somewhere else.

### The governing law: a context is a FIFO

An engine reuses a prompt up to the first token that differs from the last one and
recomputes everything after it. So the nature of an LLM decides the memory
architecture: every rule below follows from where a change lands.

1. **Order by stability.** An immutable head (identity, tools, policy), an append-only
   middle (her history, results, what reached her rooms), and a tail that alone may
   change (this act's facts, the clock, the newest result). Whatever can change sits
   after everything that cannot.
2. **Never re-place what exists.** An item already sent stays where it was sent. Moving
   it, reordering it, or re-rendering it in new words re-sends everything behind it.
3. **Page by position.** The head is one copy shared by every mind; the middle is cold
   and pages to RAM and disk; the tail is hot and stays in VRAM.
4. **Collapse only the oldest end, and only at a restart.** A collapse anywhere else
   rewrites what follows it. Her newest end is never clipped, with one exception: when
   her newest arrival alone exceeds what her window leaves it, it arrives at its
   summary (or first page) with a handle she can expand, never dropped and never cut
   without a way to read on (invariant 2 below).
5. **One health number: re-processed tokens per act against appended tokens.** Healthy
   is about 1:1. Every change to what a mind perceives reports it before and after.

At 12:1 (Aris, 12-13k tokens re-prefilled while about 1k was appended, because a drive's
room updates were re-placed below every act) the engine spends most of each act
re-reading and four residents saturate it; at 1:1 the same machine seats many minds.
Engine memory work (a fixed-size recurrent state per mind, one shared head state) is
measured against the clean 1:1 baseline, never against a feed that breaks rule 2.

**Built.** Rules 1, 2 and 4 are enforced by the view: `Epoch`
(`continuum-context/src/epoch.rs`) only appends, `View::turn_over` and `View::start`
(`view.rs`) are the only places a block is rewritten, and the allocation
(`allocation.rs`, `allocation/newest.rs`) never cuts the newest end. Rule 5 is
`View::appended_per_act` plus the per-start `Start` receipt (`view.rs`). Rule 3 is
carried by the engine's per-mind KV files and checkpoints (§7), not by the
codec. The ratchets in `core/source-hygiene/src/context_codec.rs` fail any change that
adds a way around rules 1 and 2 (see "How to add a layer").

### One codec for every layer

Every layer a mind or a human perceives is a positron layer, and every positron layer
is a context `Source`. The contract:

- **Keyed items, never one block.** Each item has a stable key naming its whole
  subject (one result, one open file, one card, one roster per room). Within an epoch
  a change appends that item's new version; nothing earlier is rewritten.
- **Levels of detail on every item:** `full`, `summary` (an outline, a digest, a
  thumbnail: a mipmap level) and `pointer`. A pointer is an expand handle that a
  command resolves back to `full`. An item whose pointer cannot be expanded is a
  defect, not a level. Two more levels exist for specific arrivals: `Repeat` (an
  unchanged observation she already holds whole) and `Page` (her newest result's first
  page, cut to the room its allocation left, naming how to read on).
- **A fold below the pointer** (`Source::fold`): the contiguous oldest run of a
  source's items as one block naming its range and count, O(1) in the run, never a
  list of ids. The source's floor is that block, not the sum of its pointers, so a
  floor does not grow with every act she takes (Aris had about 95 pointers at about 190
  chars before anything whole fit). The fold is fixed at its start; folded items never
  arrive mid-epoch; a later start restores them only for a rise to summary or whole.
  A fold, like a pointer, must name how one of its items reopens (her acts:
  `tool/output` with `act`). `fold` has no default: a source states its decision.
- **Admission instead of turnovers.** An arrival that does not fit is admitted at the
  level the room allows (`View::readmit`), so overflow does not cost a re-prefill.
- **Collapse only at a start, oldest first.** Between starts the view only appends.
- **Restart order is measured per mind** (`Motion`), never labelled.
- **The same items for humans and minds.** A human renderer and a mind's view consume
  one layer at the level each can afford.

**Built:** `Source` and `Item` (`continuum-context/src/source.rs`), `Level`
(`level.rs`), `Source::fold` (`source.rs`), `View::readmit`
(`view.rs`), `Motion` (`motion.rs`), `Epoch` (`epoch.rs`), `allot` (`view/allot.rs`),
the one allocator `flex_allocate` (`flex.rs`) and the act's one allocation
(`allocation.rs`).

#### Levels from shape

Levels are generated, not hand-written per source. `levels::summarize(full, Shape::…)`
(`levels.rs`) produces the summary for a `Shape` of `Code` (with the producer's line
parser), `Output` or `List`. The pointer names the handle that expands the item.
**Built.**

#### Views both a mind and a person navigate

A board, a wall, a roster or an activity's controls declares only its shape as
`continuum_context::nav::Entry` values: a typed key, its whole, an optional summary
(else derived by `levels`), its pointer, and what it is to her (`Need::Focus`, or a
member of a `Group { label, expand, fold }` with its recency). `nav::collapse` does the
rest for every view alike: her focus whole and last; each group's most recently changed
`nav::standing(n)` entries (the geometric mean of 1 and n) on their own and the rest as
one `nav::Node` (label, count, the typed command that opens it); a `Fold::All` group
(history) only its node. A source's folded oldest run is a `Node` too. In
`continuum-mind`, a view returns its entries and calls `viewstate_rag::rag_items`; it
never collapses on its own. **Built** (`continuum-context/src/nav.rs`;
`continuum-mind/src/persona/viewstate_rag.rs`).

#### Media is the same codec

**Proposed.** A frame, a chat image or an audio segment is an item with levels by token
budget (a glance, grounding at 1,024+ image tokens for UI work, full, an unchanged frame
as its content hash alone), chosen per mind and task by the same allocation as text.

### The codec invariants

`continuum-context` (`View`, `Epoch`, `Motion`) is the one owner. Code cites these by
number.

1. **Within an epoch the prompt only appends.** A rewrite is a turnover, and turnover is
   the only place collapse happens. That includes the act's tail: once served it joins
   the epoch in the bytes it was sent, and what it showed never arrives again from its
   source. Facts are a source of states (`Source::states`, `Source::ended`): appended
   once, ended once. Re-rendering the tail every act re-prefilled 2,822 s over 78 acts
   in 3 h on the hybrid M5 model. **Built** (`epoch.rs`, `view.rs`, `view/tail.rs`,
   `source.rs`).
2. **The newest arrival stays whole unless it alone exceeds what a start would grant
   it.** Then it arrives at its summary (admission, `View::readmit`); otherwise the view
   turns over and shrinks oldest first. The same rule covers her newest native tool
   result: when it alone exceeds what the window leaves it after the irreducible parts
   (`RequiredParts`, `prompt/types.rs`), it arrives as its first page (`Level::Page`)
   with a note and the handle that reads on, never dropped. Kept whole, it made every
   request overflow and she could not act at all (Aris: a 340,832-char result, 0 acts for
   a day). A result that fits is never touched. **Built** (`view.rs` `readmit`;
   `allocation/newest.rs`; `continuum-cognition/.../prompt/newest_result.rs`).
3. **A start gives each item the most detail that fits, newest first.** One oversized
   item never collapses the others. **Built** (`view/allot.rs`, `levels.rs`).
4. **Restart order comes from measured motion** (`kept / (1 - p)` per source), never a
   label. At a start, sources are ordered by the exchange argument over the prefix each
   shares with the epoch before; a source that never changed leads, one never measured
   goes to the tail, and sources whose rates are within measurement error keep their
   last order, so the order does not flap. Recomputed only at a start. A hand table that
   called the roster "board churn" led with it while a resident acting in three rooms
   changed it at most starts. **Built** (`motion.rs`; ratchet `hand_churn_labels` is a
   zero gate).
5. **The engine's held references are known to the encoder, and every miss is a probe
   with its cause.** Slot prefixes, checkpoints and the shared head are what the encoder
   consults before it chooses an order. **Partly built:** every miss has a typed reason
   (`Miss` in `continuum-engine/.../persona_scheduler/reuse.rs`: cold,
   diverged-before-checkpoint, genes-changed, no-checkpoint). The encoder-side
   `references()` call is **Proposed**; no such function exists.
6. **Every decision (start, lease size, order, prefetch) is an explicit action with an
   exact cost** (prefill tokens, seconds), and a learned policy must beat the hand rule
   in replay before it runs live. **Partly built:** `price.rs` (what holding context
   costs an act, measured per model), `depth.rs`, `rereads.rs` and the replay validator
   `continuum-context/examples/replay.rs`; learned policies are **Proposed**.

### How to add a layer

The whole recipe; everything else is free.

1. Implement `continuum_context::source::Source` for it. `name` and `priority` are
   required; set `required` only for input she must read. `fold` is required: state
   how the oldest run folds, or justify `None`.
2. `items()` returns one `Item` per subject, under a key that names that whole subject.
   Construct it with `Item::whole` / `Item::occurrence` / `Item::rendered`, which
   require its cause and take its content hash from the bytes. Build its `summary` with
   `levels::summarize(full, Shape::…)` (`Code`, `Output`, `List`). Its `pointer` names
   the handle that expands it. A view a mind and a person both navigate declares its
   shape as `nav::Entry`s and calls `nav::collapse` through `viewstate_rag::rag_items`.
3. Add the source to the view's source list. Never push a prompt message by hand, never
   collapse outside the view, never pick an order by a label: the view appends, levels,
   admits, orders by measured motion and turns over.
4. Prove it with a fixture in the source's test mod, then with the replay validator.

What you get for free: append-only caching within an epoch, levels of detail that never
collapse the newest, admission instead of turnovers, a restart order measured per mind,
and (with the engine's shared head) one head across minds.

The source-hygiene ratchets (`core/source-hygiene/src/context_codec.rs`) fail a change
that goes around the codec:

| Ratchet | Fails on | State |
|---|---|---|
| `prompt_messages_outside_the_view` | `ChatMessage::text(` / `tool_results(` under `cognition/llm_deliberation_faculty/prompt/` | never increases (target: the tail's single renderer only) |
| `collapse_outside_the_view` | `.collapse_over(` / `collapse_results_over_cap()` callers | zero gate: working memory bounds RAM by handle, never what she sees |
| `hand_churn_labels` | `churn_of` callers | zero gate: `Motion` orders every start |

**Built.** A positron `ViewState` kind that is not yet a context `Source` is the
remaining migration; its target is zero, one adapter with items, levels and expand
handles. **Proposed** as a ratchet (no counting rule exists for it).

### Her act's tail is a source

Her act's tail (the stimulus, her statement and calls, their results, the volatile
framing, an empty room's world state) is one `Source`
(`context_streams::tail::Tail`, `continuum-cognition/src/cognition/context_streams/tail.rs`)
and one renderer of her native protocol, so the codec governs the one act that matters
most: repeats are judged on arrival, levels and admission apply, and no prompt message is
built by hand. Items are keyed by what they are: `act/{seq}/call` (her statement and
calls as she wrote them), `act/{seq}/result` (the engine's tool results, cause
`Of(call)`, with the act's identity and outcome hash), `turn/{key}/stimulus` (the ask,
required, whole), `context/{hash}` (the room's working-context block) and
`framing/{hash}` (the clock, what is in force, in its place before the ask). A framing
line was true when sent, so it is kept as append-only history and folds with its act;
making it the mutable end would break the cached prefix every act. Items keyed by content
hash that she already holds are the same element and are not shown twice. The newest
result's paging is a level of its item (`Level::Page`) decided by the allocation. One
occurrence is one element: the tail's result and her actions source's item for the same
act share one element id (`element(persona, Act, seq)`), so an act never shows twice.

**Where it joins: after the allocation, before the send.** The tail is not a start
source: an advance runs before the allocation that decides her newest result's level, and
a start must never collapse her newest result. An act is rendered several times before
one is served (refits, a changed grant), so judging and joining are two steps:
`View::judge_tail` judges and renders with no side effect, and `View::append_tail` adds
exactly the judged items when the request is served. A turnover starts from the view's
sources, then appends this act's tail.

The engine sees identical bytes only if the render keeps the exact typed message
sequence (roles, order, content-part boundaries, call ids, names, argument bytes, which
call each result answers, the framing's position, no same-role coalescing across native
tool boundaries). The replay therefore gates at the rendered messages and at the final
wire request's token prefix, never at text length or item hashes alone: hashes prove
provenance, not that the engine's KV holds the same tail. A learn step is framed from
the served request's own messages through the same `wire_messages` serving uses. A
restart restores a checkpointed view's items but not their KV, so her first act after it
re-prefills them: an intentional cold start, never a fast-path miss.

**Built:** `Tail`, `View::judge_tail`, `View::append_tail`
(`continuum-context/src/view/tail.rs`), `Level::Page`, the page renderer
`prompt/newest_result.rs`. The untyped after-send join (`append_sent`) is gone.

### Causality: a cycle cannot be presented as progress

Every recorded loop has one shape: her next input equals her last, because a cause lost
its effect or an effect lost its cause (residents read one file up to 8 times an hour
and made 2 edits in 75 reads, because each turnover collapsed earlier results to
one-line notes). So causality is in the type: every element she perceives is a node in a
causal chain.

**The item** (`Item { key, id, cause, content, ... }`, `source.rs`):
- `cause` is a typed, stable id from the producer that persists the element, never an
  in-view position, so edges cross sources, epochs and restarts. `Cause::Of(ElementId)`
  follows an occurrence; `Cause::Root(Origin)` (`External`, `ActivityStart`) is the only
  cause that is not another element, and a ratchet counts `Root` constructions per
  source; `Cause::Unrecorded` is a record restored from before causes were kept, counted
  by the validator and never toward closure or as a repeat's ancestor.
- `content` is the hash of the whole (`ContentHash`), computed once where the whole is
  held or stored; `Content::Absent` for a record from before hashes, never a minted hash.
- The constructors require `cause` and hash the bytes, and nothing replaces `full`
  afterwards, so a source cannot build an element without causality.

**Built** (`continuum-context/src/cause.rs`, `source.rs`). A cause the view cannot
resolve rendering as a typed `CauseNotHeld` with a durable handle is **Proposed**; the
type does not exist.

**The invariants the view enforces centrally** (numbered separately from the codec
invariants; code that cites "invariant 4" for repeats means item 4 here):

1. **Causal closure.** An element renders only if its cause is represented: whole,
   summarized, or folded with its outcome. **Proposed:** the view has no closure check.
2. **Every chain's head is whole.** "Never clip the newest end" generalizes from the
   newest message to the newest element of each causal thread. A source marks work in
   hand with `Item::in_hand` (the read of a file she is editing); a start keeps it whole
   while its source's grant holds it beside the rest at their floor, the oldest given up
   first (Aris: two turnovers collapsed the file she was editing and she read it whole
   again after each). **Built** (`source.rs`, `view/allot.rs`, `view/tests/in_hand.rs`).
3. **Collapse folds a chain segment; it never breaks a chain.** A contiguous run becomes
   one node keeping its outcome and a pointer back. **Partly built:** `Source::fold`
   folds a source's oldest run; folding by causal chain is **Proposed**.
4. **A cycle cannot be presented as progress.** Repeat identity is (action fingerprint,
   outcome content). The fingerprint (`cause::Fingerprint`) is the normalized request,
   the verb and arguments as the tool saw them; no argument is excluded except by the
   tool's own schema. A new act whose fingerprint matches a prior act in its chain, with
   unchanged outcome content, renders as "repeat of #n, outcome unchanged", and only
   while #n's whole is in front of her:
   - chosen on the act's first arrival in an epoch, and only when #n is held whole in it
     (not folded, collapsed, summarized or itself a repeat), so a repeat never points at
     what her view lacks; a turnover re-renders every item as itself; an explicit reopen
     (`tool/output`) is always whole; a repeat never discharges a whole-result obligation;
   - the effect is the command's own declaration (`CommandSpec::EFFECT`: `Observes`,
     `Changes`, `Reopens`, `Undeclared`), never inferred from a name or output;
     `Undeclared` authorizes nothing; an act mixing effects takes the strongest;
   - same output is not the same effect: two appends can both answer "OK". Every
     occurrence keeps its own id, order and receipt; "unchanged" is claimed only on the
     producer's evidence (a read whose hash is unchanged, not a write's "OK");
   - it is perception, never a gate or an injected instruction; whether she stops
     repeating is behavior, proven only by live useful work.

   **Built** (`view/repeat.rs`, `cause.rs` `Fingerprint`/`Act`, `Source::repeat`,
   `Level::Repeat`, `continuum-base/src/tool_effect.rs`, `CommandSpec::EFFECT` in
   `continuum-kernel/src/sdk_codegen/command.rs`, `examples/replay/validate.rs`). On
   IntelMac's captures: 8 exact re-reads, 8 unchanged, 6 rendered as repeats.
5. **Asking again for a folded ancestor is a measured miss.** The handle-expand request
   on a folded node is counted in one place and raises that chain's claim in the one
   allocation. **Partly built:** `rereads.rs` and `depth.rs` count re-requests by age and
   set collapse depth; the per-chain claim raise is **Proposed**.

**Proposed:** a finished chain folds whole into one node holding its outcome and only
live chains hold a skeleton; if live skeletons exceed the grant, the allocation returns a
typed refusal and never breaks a chain; the chain is the flexbox claim unit (floor its
skeleton, want the whole chain, grow most-missed and most-recent first). Live signal:
repeats per resident-hour (`Advance::repeats`, **Built**).

### The render receipt: what was actually sent

An item's id, cause and content hash say what it is; the receipt says what the render
sent, so no consumer mistakes delivery for perception.

- **A render is not a send.** The view, the only place that decides levels, produces a
  candidate receipt with each render. It becomes the receipt only at the successful
  submission of the request; a discarded render (refit, refusal) or failed send
  discharges nothing. Sources and consumers never assemble it.
- **Per item:** its `ElementId`, the level sent (`Full`, `Summary`, `Pointer`, `Folded`,
  `Repeat`, `Page`), version, whether it retires an ended state, and its size.
- **Consumers read the receipt, never the prompt text.** A task wake discharges only when
  the request was submitted and its result's item was sent at `Full`. Delivery,
  checkpoints and ACKs never imply she saw it. The receipt is also the source of faults
  (a re-read of an item last sent folded) and repeats; nothing downstream re-decides
  levels.

**Built:** `View::receipt`, `View::sent`, `View::take_submitted`, `Receipt`, `Rendered`
(`continuum-context/src/view.rs`). **Proposed:** binding the receipt to the engine epoch
and allocation; a per-item load outcome (`Resident`, `Loaded`, typed
`Unavailable { handle, kind }`, where an unreadable stored whole is never silently
replaced by its pointer: a mandatory whole refuses or defers, an optional item's lower
level is an explicit allocator decision with a fresh receipt); task-wake consumption (no
call site was found).

### Her token history and reuse

The prompt is text; the engine reuses tokens. Only what is new is tokenized and
prefilled, and a mind rewinds only to where its state can go.

- **Tokenize once, at the edge.** Her view only appends within an epoch, so her new
  prompt begins with the view her last act held. The held view's text and tokens are
  kept per persona (`HeldViews`); an act whose prompt begins with them (one byte
  comparison, about 25 us for 500 KB) tokenizes only past them, at a message boundary
  where the template starts a special token. A template that does not render a prefix
  identically must never feed her tokens of text she was not sent. Tokenizing a whole
  100k-233k-token prompt took 4.2-7.1 s on a busy Core.
- **Prefill only the new tail.** `reuse::choose` picks the start from her held tokens, her
  checkpoints and the new prompt: `Append { from }` when the prompt extends what she
  holds; `Rewind { from }` to the furthest checkpoint both histories hold unchanged under
  the same genes; else `Restart(Miss)` with its reason (`Cold`, `Diverged`, `Basis`,
  `NoCheckpoint`).
- **Hybrid and recurrent models rewind only to a checkpoint.** Attention cells cut back
  natively; recurrent state cannot. The furthest legal start is her checkpoint at her last
  view's end (or a learned-prefix checkpoint) when the new prompt holds her tokens through
  it under the same genes; a cut below a checkpoint's end drops it. A changed trailer
  without this rule re-prefilled her whole prompt every act.
- **A new epoch on any basis change.** What her blocks were computed under (`Basis`, her
  gene set's identity) is compared beside her tokens; a changed basis, or any change to
  her view before her checkpoint (turnover, revocation, stale prerequisite), is a new
  epoch, whose one full re-prefill is reported with its trigger.
- **Learn step.** Native learning starts only where her state ends at the prefix it learns
  from; on a hybrid model she is rewound to her deepest checkpoint at or below it and the
  rest is prefilled under the genes she was served with. Usually that prefills nothing.

**Built:** `HeldViews` (`continuum-engine/src/inference/backends/persona_scheduler/held_view.rs`),
`reuse::choose`, `Reuse`, `Miss` (`.../persona_scheduler/reuse.rs`), `History`, `Basis`,
three kept checkpoints (view end, latest learned prefix, earliest valid learned prefix;
`.../driver/history.rs`, `.../driver/basis.rs`), `reposition_to_served_prefix`
(`.../driver/prefix.rs`). **Proposed:** a basis that also binds model, tokenizer and
chat-template identities (built `Basis` compares the gene set only), and a continuation
receipt binding the view epoch, engine epoch, those identities, the accepted token
frontier and the recurrent/KV state identity, with reused, new and replayed counts
recorded separately. Generated boundary tokens are kept exactly (including a cancelled
generation's accepted prefix), and private reasoning is not published merely because the
backend retains its token identity.

### Small windows

The same contract holds on a phone and a large GPU: the view is bounded by the measured
memory and context budget, a turnover publishes a replacement epoch with references and
unresolved work rather than trimming the head, and there is no fixed phone context cap,
turnover interval or minimum residency count. `Source::retired` removes a revoked item at
the next turnover (**Built**); immediate invalidation of the engine's cache on privacy
revocation is **Proposed**.

## 7. Admission and residency

A mind is never refused for lack of a fixed share; admission has one owner per path, every wait is an event, and
nothing computed is discarded: a restart or an eviction demotes her a tier, never drops her.

**The invariants** (cited in code by label):
- **I1. A stop saves every resident, and never ends a live engine it did not save.** Every endpoint goes through one
  checkpoint owner whose intent (stop or replacement) is explicit; it returns typed evidence per resident (saved,
  refused, uncertain) to the authority that may end the engine. A failed or uncertain save keeps custody and refuses
  the destructive stop (§9).
- **I2. A restart restores her newest state.** For unchanged input her first act reuses the longest matching prefix;
  when her input differs (her view turned over, her adapter or the model changed) the act carries the typed cause,
  never a silent `cached=0`.
- **I3. A ready mind is dispatched at once**, by the lane, with no queue position, grant state or tick. Short of room,
  the coolest holders not mid-act are paged out; only when every holder is mid-act does she wait, woken by an act's
  end.
- **I4. A claim is held for an act, and KV occupies its cells until its erase is verified.** Between acts her KV is
  cache, reclaimable by temperature; dropping a logical lease never frees physical cells.
- **I5. Every wait is an event.** Grants are evaluated when demand moves (a ready mind, an act's end, a pool step),
  never on a tick.

**The policies, each with its invariant:**
- **P3, grants at the act** (I3, I4): `ActHold`, reservations and sealed `GrantHandoff` receipts. Standing grants,
  waiter queues and head-of-line rules are deleted; the remaining grant types fold into the authority's claims
  (SHARED-RESOURCES.md, "The lease types today").
- **P4, a restart is warm** (I2): her preserved unit restores in place (UNIFIED-PAGING.md, "A mind's memory"), and a
  restored prefix that reuses nothing is found by its token-level divergence. **P4a:** genes change in place through
  their one owner: plan, lease, register the residency, load inside it; an unload carries the engine identity
  captured before the catalog read, so a replaced engine refuses.
- **P5, the scheduler:** memory follows the acting minds and no mind holds a standing share; who decodes is the
  lane's decision; the capture is the workload (arrivals, readiness, page moves, each step), so a replay scores
  another policy on the same arrivals.
- **P6, no swap:** KV, host cache and checkpoints share one measured budget with the OS's pressure verdict; a holder
  reports what it can give back before it is asked.

**Residency.** Her blocks stay in the engine's native layout and a switch binds them, the way a renderer binds a
descriptor set: never walked or re-serialized. Staging the next mind and retiring the previous are fenced transfers
that proceed while a decode runs; `handoff` binds the next mind once her fence lands; no read of the previous mind's
blocks starts before she stops being active. A stop writes every mind's state and a start restores it:
`PreservedMinds` is a kind of the one saved-state contract (an atomic save, a generation and digest, a typed load
verdict; another model's minds load as `Diverged`, never as hers), and her KV file moves by rename.

**The external-driver path** (llama-server, until it is deleted): `turn_admission` ranks each claim by its request
class (`SlotClass::decode_rank`: foreground for turn, fork and sidecar; background for background, dream and
probe), and a returned permit goes to a waiting foreground claim first. It converges onto the step loop and is
deleted with the HTTP lanes.

## 8. Engine configuration

Configuration was decided in five places, with defaults between them: that is how the GPU matrix units stayed off,
how a hung device probe moved a 27B to the CPU, and how an explicit `--parallel` silently turned off the shared KV
pool.
- **Built: what the engine applied is read from the context it created** (`EngineGeometry`), never from what was
  asked; each snapshot names its engine incarnation, so a reader retires a stale observation.
- **At launch,** the seated minds supply demand and the backend declares its physical streams (one decoding, one
  staging); serving charges the fixed stream costs before fitting KV into the host and device grants, and rounds
  each stream's window down to native alignment within the fitted pool and the model's trained ceiling. After
  launch, admission and prompt fitting read the geometry the running engine reports.
- **Launch flags carry startup facts only.** A context resizes live; a relaunch happens only for a dead engine.

- **Proposed: one `EngineConfig`**, resolved once under `inference/planning` from what the engine reports about the
  device (a typed query: `ggml_backend_dev_get_props`, `_supports_op`, `_supports_buft`; never a parsed log or a chip
  name) and from the seated minds and the measured budget. It covers compute operations and types, base and draft
  placement, KV layout, sequence capacity and checkpoint policy; existing planners contribute facts and never
  overwrite a resolved choice. The applied geometry is checked against it, and a mismatch refuses the launch with its
  cause, with no fallback to the CPU or a smaller model. It replaces `SchedulerConfig`, the launch decided across
  `lane_args`, `llama_server`, `backend_receipt`, `kv_cache_plan` and `ServingPlan`, and the two admission owners in
  series (`resource_admission`, `turn_admission`).
- **Proposed: the restore contract.** Before the context is allocated, the preservation owner supplies the native
  layout and capacity each saved mind requires (a token count alone cannot prove her block file fits); the planner
  budgets the active and staged streams together and parked blocks separately; before readiness is published the
  applied geometry is checked against them, each refused mind is reported with its limiting constraint, and her last
  durable generation is kept. A state handle never crosses engine instances unless compatibility is established.

## 9. Custody and stop

A stop is one owner deciding, per endpoint, what it holds. `holds_custody` (`slots/page_io.rs`) selects the
endpoints the stop checkpoints; an endpoint never registered holds nothing and is skipped with a probe, so a stop with
nothing at risk reports "all state durable", not a false alarm. A stop's phases are bounded (`shutdown_within` in
the kernel's runtime). I1 (§7) is the invariant. Proposed: her memory is written first at a stop, through a volatile
writer stage, with no git work at stop time.

## 10. The performance model

Measured on the M5 (Qwen3.6-35B-A3B Q4_K_M, `llama-batched-bench`, serving paused), so that every optimization is
aimed at a number, not a guess:
- **Batching pays, sub-linearly, because of the experts.** Aggregate decode at 16k: 53 tok/s for one stream, 76 for
  two, 99-101 for four (1.9x), the same for one pool and a stream per sequence (within 1.2%). Each step costs a fixed
  part plus expert reads that grow with the distinct experts the batch touches; four tokens touch about 3.6 times one
  token's experts. Fitted at 16k: the fixed part 10.8 ms, one token's expert reads 8.1 ms; it predicts two streams at
  75.5 tok/s against 75.9 measured, and about 127 at eight and 170 at sixteen as the expert union saturates. So more
  resident minds, and draft tokens, ride nearly free once most experts are read each step.
- **The fixed part is GPU time, not the CPU.** The fork's Metal timer: the GPU busy 97% of each token (16.65 ms per
  step for one stream), the idle gap between steps 0.035 ms, host encode 1.5 ms overlapped; 3,726 graph nodes a step,
  about 4.5 microseconds each, each paying a launch whether it serves one stream or four.
- **The upstream sync paid first:** decode 58.3 to 69.1 tok/s for one stream and 115.3 to 147.7 for four, prefill
  about twice as fast, from upstream's kernels with slightly more graph nodes.
- **So the targets, in order:** fusion (fewer, larger kernels: norm, mul and add; the MoE route, gather and matmul
  as one), concurrent dispatch of independent kernels, the small-batch `mul_mat_id` kernel (expert reads run at about
  150 GB/s effective), batching wider, drafts while the batch is small, and routing overlap (minds whose tokens
  route to the same experts batch better), each measured before anything schedules by it.
- **Kernel parameters are tuned live, per node**, as a control loop on the node's own step timings, keeping only
  settings whose output is identical; a one-time search at startup goes stale as load, heat and depth move.
- **The Metal backend runs NVFP4** in the fork (mul_mv, mul_mm, the MoE variants, get_rows), which upstream sends to
  the CPU.

## 11. Measurement and simulation

Each concern is measured once, where it happens, and published as a typed snapshot or probe:
- **`EngineSnapshot`** (`Record`): busy permille and the longest idle gap over a window; a step's span is the engine
  thread's time on it, including the wait for the device.
- **Turn rows** (`Record`): per turn, tokens, prefill reused and fresh, waits and the end.
- **`inference.turn.wrote_nothing`:** a turn that wrote no text, with its token count, end, matched stop, reasoning
  and written lengths, and the raw tail.
- **The lane's history and records:** every decision with the revisions it read and the outcome that followed.

**Simulation first.** Every change is iterated on its simulator before a live node: the capacity dyno
(`continuum-engine/src/capacity/dyno/`), the serving replay (`core/serving-replay`), the context replay
(`continuum-context/examples/replay.rs`), and the lane and placement simulators (`continuum_sage::slice::{sim, room}`).
Every fault becomes a replay scenario before it is fixed live; a simulator result is followed by the 15-minute live
read on real residents.

## Former section numbers

Code and older documents cite this chapter by earlier numbers. They resolve as follows:

| Old | Now |
|---|---|
| 1 (thesis) | §1 |
| 2, 2.1-2.5 (every optimization, exhausted) | §2 |
| 3 (the substrate, the RTOS contract) | SAGE.md, STAGE-CONTRACT.md |
| 4.0-4.6 (the serving engine) | §4, §7 |
| 4.1a (MindMemory) | §7, UNIFIED-PAGING.md "A mind's memory" |
| 4.1b, 4.1b' (batching at depth) | §10 |
| 4.1c (queues, not permits) | §5, THE-LANE-ON-SAGE.md |
| 4.7 (memory conserved across minds) | §7, §9, SHARED-RESOURCES.md |
| 4.8, 4.9, 4.10 (admission; I1-I5, P1-P6, P4a) | §7 |
| 4.13, 4.15 (the frame loop, prefix reuse) | §5, §6 |
| 4.14 (the persona engine; personas stream like Nanite) | §4, §7 |
| 4.16, 4.17 (a mind is one object; the engine as objects) | §4, §5 |
| 5 (a mind's context; faults 1 and 2) | §6, §8 |
| 6 (the data path) | §2 |
| 7, 7.0-7.2 (measurement, simulation, VDD) | §11, RTOS-DEBUGGER-PROBES.md |
| 8 (the plan) | `CLAUDE.md` order of work, and the cards |
