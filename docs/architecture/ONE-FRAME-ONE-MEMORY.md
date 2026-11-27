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

Under these laws the only "never here" is an entry whose floor, with every movable page moved, exceeds the device. A
node that cannot hold one floor entry of a consumer cannot run that consumer at all, and that is known when the node
starts, not discovered mid-run. Every other shortfall is a wait for the next frame, at a lower level of quality if
the consumer has one.

What each consumer's floor is gets measured, not argued: a learn tile's logits and their gradient at every labelled
row make its per-row cost larger than a prefill's (Cormac, on #6295), so "a node that can serve her can teach her"
is the inequality `scratch_learn(least) <= arena(node)`, gated per model and backend.

## What decides alone today, and its fate

By owner, on canary 5138272ccc. Each owner's code is read before it is cut; the fate is the design's, and each cut
deletes what it replaces in the same change. Nothing is deleted until Joel has seen the list for that cut.

| Owner today | Where (lines) | Fate |
|---|---|---|
| The kernel's resource authority: `ResourceLease`, `LeaseArbiter`, `TieredArbiter`, `ResourceBroker`, `ResourceGovernor`, `UnifiedMemoryPool`, `LeaseGuard` | `continuum-kernel/src/resources` (7,550) | the authority as one `place` ledger (#6287, Cormac); the rest deleted |
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
| 1 | The authority as one `place` ledger (#6287); the `Resident` row gains its residency (Resident, Transient, Pageable) | the kernel authority's duplicate ledgers | one picture of memory: the placer's totals equal the allocators' on both nodes |
| 2 | The compute arena and every mind's K/V as pages (`MindStorage` step 2: pinned, fenced, async on the 5090) | slots and seat counts, the window grants as byte owners | M5 and 5090: switch time and turn p50 not worse; zero slot refusals |
| 3 | The frame as a draw list: `UnitStage`, prefill and decode as its first entries | `prefill_rate.rs`, the admission owners | no ready mind past her bound; prefill per act equals appended tokens |
| 4 | Learning on the frame and pages (IN-PROCESS-LEARNING.md slices 2 to 4) | the seat protocol, the trainer module, the credit gate, learning's counters | steps and saves above zero on both nodes; a correction kept across a restart |
| 5 | Genes and experts as pages under the Streamer | `gene_residency`'s owner, the expert pager's private budget | gene and expert faults counted, none a miss the forecast could have prevented |
| 6 | Disk tiers: build caches, worktrees, logs and captures as `Resident`s; `Topology` reads capacity live | the disk monitors and pools, the disk-admission refusal | a deploy never waits on a cache it could have evicted |
| 7 | The remaining consumers (embedding, media, comms) as entries and pages | their budgets | each consumer's floor measured and gated |
| 8 | llama-server's lanes, the pressure broker and the substrate governors | everything in the table above not already gone | the owner count by name: zero types that decide room or time outside the frame, the placer and the Streamer |

## Gates for the whole

On the M5 (35B) and the 5090, each step's 15-minute window on real residents:
- **Zero refusals** that are not a node's known "never here".
- **Every wait names its event**, and resolves in the next frames, not after a timeout.
- **Learning runs:** steps and saves above zero, held-out loss kept.
- **Faster, not slower:** acts per hour and turn p50 at least the matched control; `busy_permille` at least 950 while a
  turn waits.
- **Smaller:** the owner count by name and the lines deleted, reported in every step's PR.
