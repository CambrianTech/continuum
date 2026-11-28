# One frame, one memory: the node as one engine

Every consumer on a node runs the same way, or nothing runs well. This chapter is the whole-node design: one frame for
time, one memory for space, one importance that prices both, every consumer mapped onto them, and every owner that
decides alone today, with its fate. Owners: Fable (the frame, pages, this chapter), Cormac (the authority on `place`,
the disk and host tiers), Codex (serving, admission, the discrete GPU, the grid). It binds
[CONTINUUM-ENGINE.md](CONTINUUM-ENGINE.md), [SAGE.md](SAGE.md), [STAGE-CONTRACT.md](STAGE-CONTRACT.md),
[SHARED-RESOURCES.md](SHARED-RESOURCES.md), [UNIFIED-PAGING.md](UNIFIED-PAGING.md),
[THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md) and [IN-PROCESS-LEARNING.md](IN-PROCESS-LEARNING.md) together; each keeps
its own mechanism, and this chapter says how they make one engine. The RAGE lessons it follows are the object model's
chapter 0 (on `fable/learning-l2-design`, to land with this).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [Why](#why)
- [The rule](#the-rule)
- [The four objects](#the-four-objects)
- [Every consumer, on the same four](#every-consumer-on-the-same-four)
- [The laws that make refusal impossible](#the-laws-that-make-refusal-impossible)
- [What decides alone today, and its fate](#what-decides-alone-today-and-its-fate)
- [The cutover](#the-cutover)
- [Gates for the whole](#gates-for-the-whole)

## Why

Core has 265 types that own memory or time (pools, brokers, monitors, governors, leases, permits, grants,
admissions, ledgers, budgets; counted by name on canary 5138272ccc). The largest owners total about 65,000 lines.
Each decides alone whether its work runs, so a run stalls at whichever one is wrong this time:
- the 5090's learning asked 809 GB of a 34 GB card, priced by one owner, while another held 11 to 19 loaded seats
  with zero steps;
- the M5's deploy waited 2,078 s on a disk-admission refusal (2026-11-27 16:50Z) beside 29 GB of worktree build
  caches no owner was asked to give back;
- six counters each held a private picture of one memory on 2026-11-26 (SHARED-RESOURCES.md, "One authority").

Fixing one owner moves the stall to the next. The design below has no second owner of anything.

## The rule

**Every consumer on a node is two declarations and nothing else:**
1. **Its time is entries in frames.** A frame is the Core's draw list: what runs in the next step on a device. A
   consumer that needs a device declares a lever (how its work divides), the cost of a unit, and how to draw one. The
   lane composes each frame.
2. **Its space is pages.** Everything it holds is a page: a block in the block layer, held by handle, on a tier, with
   a typed state. It reports its pages as a `Resident` (rows, ways, costs, requirements). The placer places every
   page on every tier; the Streamer moves them.

**One importance prices both.** What matters now, computed once over the part tree (attention, appraisal, the
activity's value curve, what depends on it), orders entries in the lane and pages in the placer. No consumer keeps
its own idea of what matters, its own budget, its own pressure signal or its own eviction.

A consumer never asks whether it may run, never holds memory it is not using, and never refuses itself. It declares;
the frame and the placer decide; it does what it is told at its next yield.

## The four objects

All four exist or are specified; this chapter adds no fifth.

| Object | What it is | Where | State |
|---|---|---|---|
| **The frame** | the lane's composed step: a draw list of entries (decode rows, prefill chunks, labelled rows, an embedding batch, a vision pass) on one device | `decide::cases::lane`, `driver/lane.rs`; the `UnitStage` parent (IN-PROCESS-LEARNING.md) | lane built; entries other than decode and prefill, and composition (`Batch`), not built |
| **The page** | a block: native bytes, a key of its consumer's type, a region on a tier, typed states (on a tier, requested, moving, resident, pinned), held by handle | `continuum-blocks` (`Block`, `Region`, `Link`, `Transfer`) | built; tiers past `Unified`/`Ram`/`Flash` and the `Dma`, `DirectIo`, `Staged` links proposed |
| **The placer** | the one ledger of every page on every tier, fed by `Resident` reports, sharing a place by the flexbox, asking shrink and grow | `continuum-sage/src/place`, over the kernel's authority | built as `place`; the authority on it in #6287; no holder is a `Resident` yet |
| **The importance** | one price per demand over the part tree | the object model's `Demand` (chapter 3.8); `Need` and `Qos` carry it today | `Demand` not built |

## Every consumer, on the same four

| Consumer | Its entries (time) | Its pages (space) | Its levels (quality ladder) | On and off |
|---|---|---|---|---|
| A mind serving | decode rows; prefill chunks sized to the slack | her K/V and recurrent state (`MindStorage`), her token history | her context codec's levels (CONTINUUM-ENGINE.md §6) | bind or park: a pointer, a fenced DMA, the OS's paging |
| Base weights | none (they are read by entries) | archetype pages, `Whole { dependents }`, shared by every mind on the base | a smaller base, never a missing one | promoted by demand, demoted only when no dependent runs |
| Experts (MoE) | none | expert pages, placed by measured routing (S8) | fewer resident experts, more faults priced | the Streamer, on routing forecasts |
| Genes | none (applied by entries at their weights) | gene pages, archetypes shared by every wearer | fewer genes at full weight | the Streamer, on the importance of the phenotypes that wear them |
| Learning | labelled-row tiles in her frames | her genome's learning pages, the walk's accumulator and checkpoints | smaller tile, sparser checkpoints, more frames | page moves (IN-PROCESS-LEARNING.md, "On and off") |
| The compute arena | the scratch of every entry, transient | one arena per device, sized by the placer to the largest frame it composes | smaller entries | grown and shrunk by the placer |
| Recall and embedding | an embedding batch on its device | the embedder's weights and window, the vector corpus | coarse then exact; fewer candidates | the embedder's window is elastic between recalls |
| Media (vision, speech, avatar) | passes with deadlines (a frame draw at 33 ms) | models, buffers, frames by handle | lower level of detail first, never a missed deadline | by the call's presence level |
| Captures, engrams, the store | none on the Core | flash and cold pages, page cache | headlines and handles she expands | pages fault in by handle |
| Builds and worktrees | CPU entries on the node's processes | build caches and worktrees as disk pages | a cache evicted to be rebuilt, priced in rebuild time | `Resident`s on the flash tier with an eviction way |
| A remote node | entries on its own frames | a tier behind a link (the grid) | a nearer, smaller placement | a lease, placed by the placement use case |
| The host's other processes | none of ours | the operator's own use of the device and memory (a game, the desktop, another app), read live by `Topology` | none: we shrink, never they | a row priced above every node consumer and never paged by us; when it grows, the placer shrinks ours by the same laws |

What is the same in every row: the entries are sized by the lane, the pages by the placer, both by one importance,
and turning a consumer up or down is the same move as turning it down or up (both ways, in one validator).

## The laws that make refusal impossible

Refusal is the one action that is never taken (0.3: rejection costs the work done plus the return trip). It is
impossible, not merely rare, when every consumer keeps four laws:

1. **Every holding is a page or a transient.** Nothing is reserved between uses. A page that is not being read is
   movable. A transient (an entry's scratch) exists only while its entry runs and aliases the one arena.
2. **Every consumer has a floor entry.** Its smallest correct unit (`least`: a decode row, a prefill chunk, a learn
   tile, one embedding) and that unit's scratch are measured, per model and backend. The floor is the consumer's whole
   device need at one frame, beside the pages that entry reads.
3. **Room comes only from what is priced below.** To admit an entry, the placer moves only pages whose importance is
   below the entry's. If that is not enough, the entry waits in its class for the next frame: time, never refusal,
   and never a higher-priced consumer's warm set (Cormac, on #6295: a background tile must never page down a mind's
   K/V and turn its admission into her switch cost).
4. **The arena is sized for the frame it composes.** A frame that composes several entries asks the placer for the
   sum of their scratch; one that cannot have it composes fewer entries. Sharing the arena is a choice per frame,
   never an assumption (Cormac, on #6295).

Under these laws the only "never here" is an entry whose floor, with every movable page moved, exceeds what the device
has for us. Usually that is known when the node starts. It can also arise later: the host's other processes can grow
and stay grown, so a floor that fit stops fitting (Codex, on #6295). Then the laws say exactly what happens:
- **The event is named.** `Topology` publishes `CapacityChanged { tier, ours }` whenever the room the host leaves us
  moves. A consumer whose floor no longer fits is `Infeasible { floor, ours }`, reported as itself on the node's health
  line and in its probes, never as a wait that looks like progress.
- **Service is bounded once it fits again.** When a `CapacityChanged` makes the floor fit, the entry is served within a
  bounded number of eligible frames, measured in the frame simulator (step 3's trace: external memory held above the
  floor, released by one unit, the frames counted until service).
- Every other shortfall is a wait for the next frame, at a lower level of quality if the consumer has one.

What each consumer's floor is gets measured, not argued: a learn tile's logits and their gradient at every labelled
row make its per-row cost larger than a prefill's (Cormac, on #6295), so "a node that can serve her can teach her"
is the inequality `scratch_learn(least) <= arena(node)`, gated per model and backend.

## What decides alone today, and its fate

By owner, on canary 5138272ccc. Each owner's code is read before it is cut; the fate is the design's, and each cut
deletes what it replaces in the same change. Nothing is deleted until Joel has seen the list for that cut.
A type with callers outside its step is deleted in the step its last caller moves, never earlier (Cormac).

| Owner today | Where (lines) | Fate |
|---|---|---|
| The kernel's resource authority: `ResourceLease`, `LeaseArbiter`, `TieredArbiter`, `ResourceBroker`, `ResourceGovernor`, `UnifiedMemoryPool`, `LeaseGuard` | `continuum-kernel/src/resources` (7,550) | the authority as one `place` ledger with `PlacerCore` the only record and the lease map a view over it (#6287, step 1, Cormac); each other type deleted in the step its last caller moves (`training_admission`, the capacity leases, the lanes) |
| The pressure broker and pools: `PressureBroker`, `ResourcePool`, `PagedResourcePool`, `FilesystemTierPool`, `PressureAlert` | `continuum-kernel/src/paging` (2,392), `continuum-core/src/modules/pressure_broker_module.rs` (485) | deleted: pressure is the placer's shortfall, pools are `Resident`s, moves are the Streamer's |
| The substrate governors: `SubstrateGovernor`, `LocalSubstrateGovernor`, `GovernorPolicy`, `PressureSignal` | `continuum-kernel/src/governor` (3,838), `continuum-mind/src/persona/substrate_governor.rs` (827) | deleted: policy is the placer's and the lane's `decide` use cases |
| Disk monitors and pools: `DiskPressureMonitor`, `MemoryPressureMonitor`, `SystemResourceMonitor`, `CargoTargetPool`, `DevWorktreePool`, `CitizenWorkspacePool`, `NvmeServingTierPool`, `RotationLogPool` | `continuum-kernel/src/system_resources` (13,221) | each pool a `Resident` on its disk tier with an eviction way priced in rebuild time; the monitors become `Topology`'s live reading (UNIFIED-PAGING.md); the disk-admission refusal that held the M5's deploy is deleted |
| The engine's capacity owners: `window_grant` (`GrantLease`, `GrantHandoff`, `GrantReservation`, `ActHold`), `HostCacheLeaseInputs`, `StickyLease`, `ExpertPager`, `RelaunchPager`, `GridCapacityLedger`, `EcacheBudget` | `continuum-engine/src/capacity` (19,375) | the engine's holders as `Resident`s; experts' pages under the Streamer; relaunch deleted (a context resizes live); the capacity simulator kept as the simulator |
| Slots and seat counts: `KvSlotPool`, `KvSlotLease`, `SlotPermit`, `DecodePermit`, `KvPageStorePool`, `EndpointAdmission` | `continuum-engine/src/inference/slots` (5,804) | deleted: her K/V is `MindStorage` pages, the lane decides who steps |
| Admission owners: `ResourceAdmissionGate`, `ServingLanePermit`, `TurnAdmission`, `AdmissionHold`, `CoordinatorResourcePool`, `DeviceTenant` | `continuum-engine/src/inference/resource_admission` (1,758), `turn_admission.rs` (997), `coordinator_pool.rs` (448) | deleted: admission is a frame entry and its pages |
| llama-server lanes and its learn ledger: `LaneAdmission`, `RegisteredGrant`, `CoveringLease`, `PoolLaunch` | `continuum-engine/src/inference/llama_server` (10,783) | deleted with llama-server's lanes; external drivers come in through the gateway onto the same frame (DRIVER-GATEWAY.md) |
| Lane plans: `LaneLeaseTerms`, `ThroughputLaneBudget`, `HostBudget` | `inference/single_purpose_lane`, `inference/planning` | deleted with the lane plans |
| Learning's owners: the seat protocol, `LeasedMemory`, `LearningMemory`, `TrainingTriggerModule`, the credit gate | IN-PROCESS-LEARNING.md, "What is deleted" | as listed there |
| Gene residency: `gene_residency` (`DeviceTenant`, LRU-2) | `continuum-genome/src/genome/gene_residency.rs` | a `Resident`; genes as pages under the Streamer |
| Persona allocation and grant issuance modules | `continuum-core/src/modules/{persona_allocator,grant_issuance}.rs` (106, 186) | read first; folded into the placer or deleted |
| Context and prompt budgets: `ContextBudget`, `PromptBudget`, `BudgetLayer`, `TurnBudget` | `continuum-prompt/src/context_budget.rs` (564), `continuum-cognition` | one owner: the context codec's flex (`continuum-context`), the others deleted (ONE-BUDGET.md) |
| Comms budgets: `CommsMemoryBudget`, `CommsGpuBudget`, `BufferLease` | `continuum-core/src/comms` | read first; buffers as pages or transients |
| Not memory or time owners (kept): the routing grant authority, card and wall ledgers, `ReviewBoundaryLedger`, the health ledger | various | out of scope: they own identity or records, not room or time |

The kept list is not an exception path, but it was sorted by name, not by reading: each type there names a record, not room or time, and the cut that reads one and finds otherwise moves it to the table above.

## The cutover

Ordered so each step makes the next one possible and the live nodes get faster at every step. Each lands whole: the
object, its first consumer on the live path, and the deletion of the owners it replaces.

| Step | What lands | Deletes | Gate |
|---|---|---|---|
| 0 | The durable store has one schema owner, changed only by migration ("The durable store", below). It runs first because the 5090 boots with zero residents today | the ORM's implicit evolution (`evolve_table_schema` and its error-text retries) | the validator below; live: the 5090's control opens its store after a review window and serves its residents |
| 1 | The authority as one `place` ledger, `PlacerCore` its only record (#6287); the `Resident` row gains its residency (Resident, Transient, Pageable) | the lease map as a second record (it becomes a view) | on both nodes, for each `Resident` the placer's total equals that holder's measured allocation, and the device's remainder is reported as unowned with its size; the unowned number falls at every step, to zero at step 7 |
| 2 | The compute arena and every mind's K/V as pages (`MindStorage` step 2: pinned, fenced, async on the 5090), with page custody carried over from the slot path (below) | slots and seat counts, the window grants as byte owners, once their custody guarantees live on page handles | Codex's custody case below, byte-identical and released exactly once; then M5 and 5090: switch time and turn p50 at most the matched control's; zero slot refusals |
| 3 | The frame as a draw list: `UnitStage`, prefill and decode as its first entries | `prefill_rate.rs`, the admission owners | in `slice::sim` first: no ready mind past her bound, and the capacity-change trace (external memory above a floor, released by one unit) served within its bound in eligible frames; live: prefill per act equals appended tokens |
| 4 | Learning on the frame and pages (IN-PROCESS-LEARNING.md slices 2 to 4) | the seat protocol, the trainer module, the credit gate, learning's counters | steps and saves above zero on both nodes; a correction kept across a restart |
| 5 | Genes and experts as pages under the Streamer | `gene_residency`'s owner, the expert pager's private budget | gene and expert faults counted, none a miss the forecast could have prevented |
| 6 | Disk and memory kept continuously ("Staying up for months", below): `Topology` live, every holder a reporting `Resident`, the placer acting ahead of full; airc's checkout holder in airc. Pulled forward: it runs beside steps 2 to 5, since the M5 filled on 2026-11-27 | the disk monitors and pools, the disk-admission refusal; their guards move first: `every_cache_class_has_a_decided_eviction_story` and `broker_relieve_actually_deletes_from_an_over_budget_pool` become placer tests (every disk-tier `Resident` declares an eviction way; a shortfall ends in real bytes deleted) | a deploy never waits on a cache it could have evicted; a full disk with nothing below the write's price waits on a named event ("disk full; nothing evictable below this write's price") shown on the node's health line |
| 7 | The remaining consumers (embedding, media, comms) as entries and pages | their budgets | each consumer's floor measured and gated |
| 8 | llama-server's lanes, the pressure broker and the substrate governors | everything in the table above not already gone | the owner count by name: zero types that decide room or time outside the frame, the placer and the Streamer |

**Page custody on the discrete GPU** (step 2; Codex's wording and case). The slot path carries physical custody that
must move to page handles before it is deleted: `slots/grant_handoff.rs` credits reclaimed bytes only after a confirmed
erase, `capacity/window_grant/lease.rs` holds through a caller's cancellation to a terminal receipt and fence, and
`llama_server/learn_ledger.rs` scopes a grant to an engine incarnation. On pages:
- Cancellation ends the caller's interest. The existing device completion owner keeps the source and destination
  ranges charged until all prior readers, writers and transfers have finished, or reuse is ordered after them.
- Reclaimed capacity is credited once.
- Each completion is matched to the engine incarnation and the operation that submitted it.

Acceptance case: delay mind A's GPU write, cancel A, then have mind B request the same range; B's bytes equal an
uninterrupted run's. Deliver a duplicate completion and a late completion after an engine restart; neither releases
B's range. Switch time and refusal counts cannot detect corruption, so this case gates step 2 before any timing does.

## Staying up for months: measurement and management are continuous

A node runs for months with no person or agent tending it, and so does the grid. Nothing here is a cleanup command,
a scheduled sweep or an operator's relief: every byte of disk and memory is held by a `Resident` that reports
continuously, and the placer keeps every tier healthy on the events those reports raise. On 2026-11-27 the M5 had
filled to 95% (deploys refused for over three hours) from three writers no owner was asked about: 185 GB of airc
card checkouts no path could retire, a 15 GB engine stderr no one rotated, and 27 GB of build cache left behind its
merged work.

**The parts, all SAGE components, none new in kind:**

| Part | Hears | Says | Decides |
|---|---|---|---|
| `Topology` | the OS's volume and memory-pressure notifications (`DiskArbitration`/FSEvents, `inotify`, `WM_DEVICECHANGE`, the kernel's memory-pressure source), every move's measured bytes and rate | each tier's capacity, what the host's other processes use, and its own rate of change, as frame-numbered state | nothing: it measures |
| Every holder, as a `Resident` | its own writes (a log line, a checkout, a cache artifact, a capture) | its rows on change: held, want, growth rate, ways and their measured costs | how to give back when asked, the cheapest of its ways |
| The placer | every report and every `Topology` change | grants, shrinks and cache-outs | which holder gives back what, by price |

**Ahead of the wall, never at it.** The placer acts on each tier's predicted time to full (capacity over the measured
growth of its holders), not on a percentage. A tier that would fill before its cheapest way could free it is relieved
now, oldest and lowest-priced first, so no writer ever meets a refusal. The 95% admission refusal is deleted with the
monitors.

**Every disk holder declares its ways, priced in time:**

| Holder | Residency | Ways (cost) |
|---|---|---|
| A card checkout (airc) | `Fixed` while its card is claimed or in review; then pageable | retire when the card is terminal on any node, proven clean and pushed, recovery ref in the mirror (cost: a re-clone if reopened) |
| Build caches (per worktree, per crate) | pageable | evict (cost: its measured rebuild time); a worktree's cache goes with its worktree |
| Service logs and stderr | elastic | copy-truncate past what the probes already hold (cost: none) |
| Probe and capture streams | durable | compact to their summaries and cache out to a colder volume (cost: a fetch) |
| K/V pages, genes, experts, weights | pageable | the Streamer's demotion one tier down (cost: the measured fetch) |
| Engrams and the store | durable | never evicted; cache out to a colder volume |

A holder that writes without reporting is the defect SHARED-RESOURCES.md names: unknown is never room. The guard
tests move first (`every_cache_class_has_a_decided_eviction_story`, `broker_relieve_actually_deletes_from_an_over_budget_pool`
become placer tests), and a new one is added: every writer under the node's state directories is a reporting holder.

**Memory the same way.** RAM is a tier like any disk: the same placer, the same `Topology`, the same holders (K/V
pages, the compute arena, caches, the embedder's window), the host's other processes priced above ours.

**The grid keeps itself healthy too.** A node publishes its tiers' state; a node that cannot fit its own work in time
offers it to a peer (the placement use case), and an idle node takes it. A mind's home stays where it is.

**airc does its half in airc.** airc runs on other people's machines with no continuum beside it, so its daemon owns
its own disk as a component: it hears every card's state from every peer, retires its terminal checkouts on that
event (not only on the node that closed the card), and reports its checkouts and mirrors as one holder. Continuum's
placer asks that holder to give back like any other.

**Gate:** a 30-day run on the M5 and the 5090 with no person or agent touching disk or memory: free space never
crosses its predicted-full threshold, no write is refused, no deploy waits, and the bytes held by each holder track
its measured need.

## The frame's contract and the block descriptor (steps 3 and 2)

Built from what exists, parents first (Joel, 2026-11-28: "well formed abstractions and object inheritance... recycle
reuse"). A result is valid because of the inputs, at their revisions, it was derived from; it is published once and
read by every consumer that needs it; only a change to one of those inputs makes anything wake, recompute or move
(Cormac's CBAR reading: selective invalidation). Nothing below is a new event system, handle type or mover.

| Piece | The existing parent it is | The leaves, their difference only |
|---|---|---|
| A published result and what made it valid | the dataflow's `Snapshot<T>` (`value`, `revision: InputRevision`, `provenance`), where accepting a revision and replacing the output are one critical section (`continuum-sage/src/dataflow/state.rs`, `runner.rs`) | the frame's entries, the Streamer's `Demand` and `Placements`, a lesson's labelled rows: each a `T` |
| Long work done a unit at a time | STAGE-CONTRACT.md's "Units": a lever, the cost of a unit, draw one unit, yield with receipts, `did <= unit` | decode (one row per bound mind), prefill (a chunk), and at step 4 labelled rows |
| Who holds memory | `place::Resident` under the one placer (`continuum-sage/src/place/resident.rs`) | each holder's rows: the frame's arena, a mind's K/V, the shadow's slices |
| A page, its move and its readiness | `continuum-blocks`: `Block<K>` (a stable key), `Region`, `Placement`, `Link` (pointer, OS paging, one fenced DMA), `Completion`, `Exchange` | one `Link` per pair of tiers per backend: the M5's pointer and `Mapping`, the 5090's pinned fenced DMA |
| A choice under a budget (prefetch lead, demand forecast, learning detail) | `decide`: `UseCase`, `Policy<U>`, `Algorithms<U>`, the envelope | one `Policy` per candidate, compared behind the same interface |
| The fork's side | its named-hook pattern: our own files, one-line hooks into upstream (`llama-graph-slots`, `llama-adapter-seq`, `llama-lived-routing`) | one parent per concept in our files, leaves for the backend's difference |

**The frame's contract** (step 3, Fable). A frame is a draw list built from entries, each a `Units` leaf. Two kinds of
publication, by what a late reader may miss (Codex):
- **Latest state** (`Snapshot`): demand and placements. A held snapshot stays valid, and a reader that wakes late reads
  the newest and may skip the frames between, which is right for a forecast or a placement.
- **Ordered receipts** (the dataflow's stream, `Event<T>` with its revision, and the topic's durable cursor for a reader
  that falls behind): every frame's **built** (its entries, which unit each, the `Placements` revision its pages were
  ready at, the lane decision that chose it), **landed** (the fence that made its outputs readable, each entry's
  `did`) and **gave** (what each entry produced: a mind's tokens, at step 4 a gradient block's readiness), and every
  correction. None is skipped, and each is read exactly once in order.

Each names the inputs it was derived from; a recomputation or wake with no changed input is the waste the replay
counts. Tested: a recorder delayed over several frames, and a subscription that wraps, both replay every receipt
exactly once in order, and two readers of one result share one computation.

**The block descriptor** (step 2, shared by Fable on the M5 and Codex on the 5090). A block keeps its stable key in
`Block<K>`; the `InputRevision` rides on the publication that carries it (`Snapshot<Placements<K>>`), never on the
block (Codex). Readiness is the `Completion` its `Link` returns, published by the Streamer when the fence lands. Both
backends implement only their `Link`: on the M5 a pointer and the `Mapping` link, so a reader reads the block where it
lies; on the 5090 one pinned, fenced DMA into the destination, overlapped with the frame before. Consumers keep no
copy. The shared replay case is the Streamer's own `a_block_held_again_mid_move_never_takes_the_old_moves_landing`:
the old move's fence completes, and the newer publication keeps its own bytes and readiness until its own move lands.

**What it deletes** (in the changes that land each piece, per the cutover's table): `prefill_rate.rs` and the
admission owners at step 3; the slot path's private moves at step 2; any consumer-side copy or cache of a block.

## Gates for the whole

On the M5 (35B) and the 5090, each step's 15-minute window on real residents, at its exact head against a matched
control. Every outcome below is measured at every step; what each step must show is in three kinds (Codex):

| Outcome | Must hold from step 0 | Must not regress from the matched control | Must be positive from |
|---|---|---|---|
| Refusals that are not a known "never here" or a reported `Infeasible` | zero | | |
| Waits that name their event | every one | | |
| Data integrity: a page's bytes after a move, a store after a migration | byte-identical | | |
| Acts per hour; turn p50 | | acts at least the control's; turn p50 at most the control's | |
| `busy_permille` while a turn waits | | not lower | at least 950 from step 3 |
| Unowned device memory (step 1's remainder) | reported | falls at every step | zero at step 7 |
| Learning steps and saves, held-out loss kept | measured and reported | | step 4 |
| A reviewed correction kept across a restart on unseen tasks | | | step 4 |
| Owner count by name; lines deleted | reported in every PR | falls at every step | |

## The durable store: one schema owner, changed only by migration

Cormac's design (on #6295, with Codex's three additions), grounded in canary 5138272cc.

**What broke.** On 2026-11-27 the 5090's control build 2135 failed five modules (gene-examiner, genome,
training-trigger, ports, recipe) on `training_trigger_submissions`, and the node booted with zero residents. The live
table had four columns 2135 does not declare: `carried_from`, `carried_offset`, `carried_to` (canary's own, #6283) and
`example_count` (written by the unmerged #6292's builds 2163 and 2164), and lacked two indexes canary declares. No one
decided either change; the ORM made them.

**Why.** The SQLite adapter evolves a table on write. When an insert or update fails and the error text contains "has
no column named" or "no such column", it runs `ALTER TABLE ... ADD COLUMN` and retries (`orm/sqlite/open_connection.rs`
`:305` and `:485`, via `evolve_table_schema` at `:217`; also `sqlite_adapter.rs:89` for a projection). So any build
that writes a field, merged or not, changes the store every later build opens; the decision is made by matching an
error's text; and nothing records which version of a collection's schema a store holds.

**The rule.**
1. **The entity declaration is the schema, and the store records its version.** Each collection's declared schema has
   a generation: a hash of its declared fields, their types and its declared indexes. The store records the generation
   it holds per collection, written by the migration that produced it.
2. **No implicit evolution.** A write that names an undeclared column is a typed error
   (`UndeclaredColumn { collection, column }`), never an `ALTER`. The three evolve sites and their error-text matching
   are deleted. A store changes only by a migration.
3. **A migration is explicit, typed and run at open.** Each names its collection, its from and to generations, and its
   kind. Additive (a nullable column, an index): applied at open, idempotent, verified by reading the store's columns
   and indexes back. Breaking (a rename, a type change, a required column, a removal): applied at open only with a data
   step that rewrites every affected row, verified row by row before the generation is written; it is the
   durable-data exception, reviewed by two.
4. **A binary meets an older or newer store safely.** Store older: the binary runs its migrations up to its own
   generation. Store newer by additive migrations only: the binary reads and writes what it declares and leaves the
   rest, so a rollback to a control build works across added columns. Store newer by a breaking migration: that
   collection alone is a typed "never here" at open, naming the generation it needs, and the node's other collections
   serve.
5. **Only a merged build migrates a shared store.** A branch build on a live node runs its migrations against an
   isolated snapshot of the store (Codex), so review windows exercise a candidate without advancing the store the next
   control opens. Without that, every review window on the 5090 changes the store, which is what #6292 did.
6. **A migration and its generation record commit together** in one transaction; a restart between them leaves
   neither or both (Codex).

Durable rows are not pages: they survive any page eviction, and R3 governs only where their bytes reside, never
whether a row exists (Codex).

| Where | Fate |
|---|---|
| `open_connection.rs` `evolve_table_schema` and its two retry sites, `sqlite_adapter.rs:89` | deleted; `UndeclaredColumn` in their place |
| `do_ensure_schema` (`CREATE TABLE IF NOT EXISTS`) | the additive migration's create step, recording the generation |
| `ensure_indexes` (skips an index whose columns are missing) | part of the additive migration; a skipped index is a migration not yet run, reported at open, never silent |
| `orm/migration.rs` (copies data between adapters) | kept, renamed so it is not confused with a schema migration |

**The 5090 now.** Before any code: run control at canary 5138272cc or newer, which declares `carried_*`. Then
`example_count` is the one undeclared column; under rule 4 it is an additive change from a newer writer and is left in
place. If canary's startup refuses it today, the repair is a durable-data-exception PR that declares it
(`Option<u64>`). No column is dropped on the live store, and transfer lineage is untouched.

**Gate.** A validator over a fixture store: an older binary opens a store a newer binary migrated additively, and
serves; a breaking migration refuses only its collection, by type; a write naming an undeclared column is
`UndeclaredColumn`, and the store is unchanged afterwards; a branch build migrates only its isolated snapshot; an old
writer updating rows of a newer store keeps `carried_*` and every newer value across its updates (Codex); a restart
between a migration and its generation record leaves both committed or neither. Live: the 5090's control opens its
store after a review window and serves its residents.

## How we fix it: the path everyone signs

This section is the plan of record. Fable, Codex and Cormac each sign it at one exact, immutable head, in a PR review
on #6295 and on airc, before any step's code starts; signatures are never written into the tree, because that would
change the head they sign (Codex). A change to the plan is a new head and new signatures. Joel approves each step's
deletion list before that step lands.

### Firm requirements

Every step, and every review of every step, checks all of these. A change that fails one is not merged, however much
it improves a number.

| # | Requirement | How a reviewer checks it |
|---|---|---|
| R1 | One owner per decision: the frame (time), the placer (space), the Streamer (moves), the importance (what matters), her genome (her genes) | the diff adds no type that decides room or time; the owner count by name falls |
| R2 | Nothing is refused for size or pressure; the only refusals are a node's known "never here" and a reported `Infeasible` after a `CapacityChanged`, served within its bound once it fits again | no new refusal variant for size or pressure; a shortfall becomes a wait for a named event |
| R3 | Nothing is reserved between uses; every holding is a page by handle or a transient | no allocation that outlives its use; every new holder reports as a `Resident` |
| R4 | Bytes move only by a link (pointer, fenced DMA, the OS's paging); never a copy loop, never an encoding | no serialization on a page move; transfers are `Link::start` |
| R5 | The GPU's step is the only serial work; every consumer yields at a unit it declared | no blocking loop on the engine thread; long work is a `UnitStage` |
| R6 | Room comes only from what is priced below | a holder is shrunk only by the placer, never by a peer |
| R7 | Both ways: every mechanism that turns something down ships with the one that turns it up, in one validator | the validator runs down then up and checks the result equals the uninterrupted run |
| R8 | No constant where a measurement exists; no threshold, quantum, seat count or pool size someone picked | every number cites its measurement or is a policy's output |
| R9 | Learning is her frame with labels on, and its three forms are parts of her mind | no node trainer, no text-example journal, no grading of her work |
| R10 | Each step lands whole: the object, its first live consumer, the deletion of what it replaces, and its gate on the M5 (35B) and the 5090 | the PR's Measured section has both nodes' 15-minute windows at its exact head |

### No patches

A patch is any change to an owner this chapter deletes or folds, other than its deletion. From the moment this plan is
signed:
- **No PR changes a doomed owner.** Its fix is the step that replaces it. A reviewer refuses such a PR by naming the
  step.
- **The live nodes run canary as it is** until a step lands. A stall on a doomed path is recorded as a receipt for the
  step that removes it, not fixed in place.
- **The one exception is durable data**: a change whose only purpose is that a resident's data, an owner's work or a
  durable record is not lost, named as such, reviewed by two of us. Anything else that looks urgent is a reason to
  land the step sooner.
- **Steps are not patches when they are whole.** A step that lands an object without deleting what it replaces is a
  second owner, which R1 forbids.

### Who builds what, in parallel

| Lane | Owner | Steps | Can start |
|---|---|---|---|
| The durable store | Cormac | 0 | now: the 5090 boots with zero residents |
| The authority and the disk tiers | Cormac | 1, then 6 | now (#6287 is step 1's kernel half) |
| The frame | Fable | 3: `UnitStage`, prefill and decode as entries, in `slice::sim` first | now, against the simulator |
| The fork's learn surface | Fable | IN-PROCESS-LEARNING slice 2: begin, cost at a unit, run one tile | now, on the M5 |
| Pages on the discrete GPU | Codex | 2: `MindStorage` step 2 (pinned, fenced, async), the compute arena as a block | after 1 |
| Learning | Fable (genome, walk, perception, dreamer), Cormac (corrections, courses, migration) | 4 | after 1, 2 and 3 |
| Genes and experts as pages | Fable, Codex | 5 | after 2 |
| The remaining consumers | each consumer's owner | 7 | after 2 and 3 |
| The last deletions | Codex (serving, llama-server), Cormac (kernel), Fable (engine) | 8 | as each step frees them |

CUDA's backward (IN-PROCESS-LEARNING slice 6) passed on the 5090 at the merged fork 9f2477f49: CPU-only against CUDA,
cosine 0.999569 whole and 0.999627 tiled on the Qwen3.5-0.8B fixture, 319,488 adapter elements, identical initial
bytes (Codex, 2026-11-27 17:29Z).

### The open PRs against the path

Proposed dispositions, sorted from titles, files and descriptions on 2026-11-27. No PR is closed from its summary:
each is closed only when its author and one reviewer agree after reading it (Codex). "Close" means closed with a
pointer to the step that replaces it, its receipts carried into that step.

| Disposition | PRs |
|---|---|
| **A step or a deletion the path needs** | #6287 (step 1), #6295 (this design), #6261 (deletes `lane_health`), #6267 (a K/V page restore maps its file: pages by link), #6293 (the fork pin: CUDA attention backward), #6230 (binary audio, base64 path deleted), #6252 (typed bytes, not JSON), #6265 (timers and sleeps become events), #6247 (global maps become owned registries), #6248 (spawns through `continuum-host`), #6294 (typed errors), #6204 (`Part`, the root of every component) |
| **Patches a doomed owner: close** | #6292 (the trainer's seat path; Cormac agreed), #6143 (`window_grant` reclaim), #6116 (fair-grant wait bound), #6232 (`admit_turn` parameters), #6176 (warm admission), #6212 (engine-memory owners into an allocator: superseded by steps 1 and 2), #6081 and #6068 (host pool and checkpoint store designs: superseded by step 2), #6067 (decode width owner: superseded by the frame), #6127, #6113, #6215 and #6177 (disk governor, tier pool counter, disk accounting, accumulation owner: superseded by step 6), #6117 (genes paged per turn: superseded by step 5), #6190 and #6249 (llama-server placement and its installer: deleted with llama-server's lanes) |
| **Durable-data exception: review as such** | #6109 (BigMama cannot import incoming chat: durable sequence heads), #6097 (an incomplete training drain destroying in-flight work at exit), #6107 (a resident's custody across admission close) |
| **Read before deciding** | #6095 (request failures retiring a serving lane: may be a doomed admission owner), #6125 (gene trials as saved state: may be her genome object's first piece), #6200 (mine: its object-model chapters 0, 3 and 6 land as their own docs PR; the rest closes) |
| **Close: the benchmark harness** | #6242 (a benchmark solver) |
| **Not memory or time: normal review** | #6288, #6281, #6270, #6279, #6273, #6268, #6256, #6250, #6245, #6235, #6231, #6227, #6224, #6220, #6217, #6197, #6167, #6165, #6160, #6122, #6105, #6094, #6090, #6082, #6066 |

### Agreement

Signatures are PR reviews on #6295 at one head, mirrored on airc. The head that all three approve is the plan of
record; this file names no head of its own.
