# Unified paging: one block layer for every tier

How memory moves between VRAM, pinned host memory, unified memory, RAM, flash and cold disk. Owner: Fable
(`continuum-blocks`, the Core's consumer); Codex (discrete tiers, the hot tier). Who gets room on a tier, and who
gives it back, is the placement chapter ([SHARED-RESOURCES.md](SHARED-RESOURCES.md)); the engine that uses the
blocks is [THE-LANE-ON-SAGE.md](THE-LANE-ON-SAGE.md).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The rule](#the-rule)
- [Where it stands](#where-it-stands)
- [Tiers and topology](#tiers-and-topology)
- [Regions and blocks](#regions-and-blocks)
- [Links and transfers](#links-and-transfers)
- [The pager](#the-pager)
- [A mind's memory: the first consumer](#a-minds-memory-the-first-consumer)
- [Other copies of us](#other-copies-of-us)
- [The grid](#the-grid)
- [Per platform](#per-platform)
- [Validators](#validators)
- [The order](#the-order)
- [Open questions](#open-questions)

## The rule

Every byte that moves between tiers is a block moved by a link, and a link is one of three things the hardware
already does: a pointer (memory both sides see), a DMA on a copy stream with an event as its fence, or the OS's own
paging of a mapped file. Never a CPU loop over rows, never an encoding, never a private mover. A block's bytes are
their native layout (KV rows as the cache lays them out, weights as the GGUF does); moving a block never interprets
it.

ggml already gives each: `ggml_backend_dev_buffer_from_host_ptr` wraps memory we own as a device buffer where the
device supports it (Metal, including a file mapping; not CUDA); `ggml_backend_dev_host_buffer_type` gives pinned
memory a DMA reads at full rate; the `_async` tensor calls and `ggml_backend_event_*` give DMA with a fence;
`ggml_backend_cpu_buffer_from_ptr` wraps host memory for the CPU backend.

## Where it stands

Built (`core/continuum-blocks`, the fork's persona storage):

| Piece | Where |
|---|---|
| `TierKind` (`Unified`, `Vram`, `Pinned`, `Ram`, `Flash`) | `continuum-blocks/src/tier.rs` |
| `Region` and `Backing` (device, host, mapped file); `Block<K>` and `Pin` | `region.rs`, `block.rs` |
| `Link<K>::start` returning a `Transfer`; `Completion` (`landed`, `settle`); a `Transfer` dropped unlanded waits for the device | `link.rs` |
| The `Mapping` link (host tier to flash) and `Exchange` (two blocks trading regions: the pointer switch) | `link.rs` |
| The pager, a leaf of `Readiness` (THE-LANE-ON-SAGE.md, "Readiness"): holds, moves and lands blocks, the most pressed mind first; a block held again mid-move never takes the old move's landing | `pager.rs` |
| When a move starts, learned: the `prefetch` use case | `continuum-sage/src/decide/cases/prefetch.rs`, simulated in `slice/room.rs` |
| A mind's KV storage over her own mapped file, viewed by the Core's stream; custody by rename at stop and restart | fork `llama-persona-storage.cpp`, `llama_persona_storage_attach` (`llama.h`); `preserve.rs` |
| Her recurrent state the same way: a switch is a pointer on hybrids too (Qwen3.6-35B-A3B: 3.16 ms to 0.01 ms) | fork `llama-persona-recurrent-storage.cpp`, `llama-persona-view.cpp` |
| An OS-locked lease per running core | `continuum-blocks/src/instances.rs` |
| The OS calls for a writable file mapping, per OS | `continuum-host` (`mod.rs` logic, `unix.rs`/`windows.rs` calls) |

Proposed: `Topology`, the `Cold` tier and per-volume tiers, the `Dma`, `DirectIo` and `Staged` links, a mind's
storage as one object with an adapter per memory, the compute workspace as a block, and the remaining consumers
(weights, experts, genes). Each is marked where it is described.

## Tiers and topology

A node's tiers are detected and measured, never listed. Proposed beyond the built kinds: a tier per volume
(`Flash { volume }`, `Cold { volume }`), any number, ranked by measured read and write rate. Two flash volumes are
two tiers; a volume that disappears takes its blocks with it, counted, never assumed still there.

`Topology` (proposed), a SAGE component, keeps every quantity current from the cheapest true source, published as
frame-numbered state so a reader sees how stale a number is:
- **Free memory per tier** is the resource authority's `resources/free`, never a second measurement (two owners of
  "free" once let the 5090's trainer read 428 TB free). On unified memory the GPU's resident bytes are not in the
  OS's available counters, so the authority's unified number must include them.
- **Link and drive rates, passively:** every move is a measurement (bytes over time, by direction and volume); a
  probe runs only for a volume no move has used within its staleness window, and only while it is idle.
- **Volumes** from the OS's notifications (`DiskArbitration`, `WM_DEVICECHANGE`), else its free-space query.
- **Our share against everything else**, so another process's pressure shows as a shrinking budget, not as our
  misses. A budget that grows is an input too: blocks move back up ahead of use, in steps, re-measured between.

## Regions and blocks

```rust
pub struct Region { tier: TierKind, backing: Backing, bytes: u64 }   // RAII: drop returns it to its tier
pub enum Backing { Device(NativeBuffer), Host { ptr: NonNull<u8> }, Mapped { file: MappedFile, offset: u64 } }
pub struct Block<K> { key: K, region: Region, .. }                    // the key is the consumer's own type
pub struct Pin<'b>(..);                                                // while held, the block does not move
```

A block's key is its consumer's type (a mind, a model shard, an expert, a gene), never an enum in the floor crate,
which would make it depend on the crates above it. A block is pinned while device work reads it. Some blocks are
held by their owner and never placed: a seated learner's shadow and optimizer state stay put for the whole seat,
counted against the tier's budget.

## Links and transfers

```rust
pub trait Link<K>: Send + Sync {
    fn name(&self) -> &'static str;
    fn start(&self, block: &mut Block<K>, to: Placement) -> Result<Transfer, LinkRefusal>;   // answers at once
}
pub struct Transfer { bytes, started, to: Placement, done: Box<dyn Completion>, into: Option<Region> }
```

The destination region is reserved before the copy; `Transfer::land` hands it to the block only when `done` fires,
so the source stays charged and valid until then, and a refused move leaves the block as it was. A transfer dropped
before its fence waits for the device (settle), so no freed memory is ever still being read.

| Link | Between | What moves | State |
|---|---|---|---|
| `Exchange` | two blocks on memory both sides see | nothing: the regions change owner (the Core's switch on the M5) | built |
| `Mapping` | a host tier and flash | demote: `msync` and `MADV_DONTNEED`, the OS writes dirty pages; promote: `MADV_WILLNEED` | built |
| `Dma` | VRAM and pinned | an async copy on the copy stream, overlapped with compute, an event as the fence | proposed (Codex) |
| `DirectIo` | a host tier and cold | one unbuffered sequential stream on an IO worker, aligned to the drive | proposed |
| `Staged` | VRAM and flash or cold | a mapping or direct read into pinned memory, then DMA: two links as one transfer | proposed |

## The pager

The one part that moves blocks (RAGE's streaming system), a leaf of `continuum_sage::readiness::Readiness`: the parent hears
the lane's horizon, puts the most pressed mind first, measures each move's rate (bytes a second) and publishes her
readiness; the pager says only what a mind's block needs, how one move starts, and how a fence lands it.

| Hears | Says | Need |
|---|---|---|
| the lane's `Horizon` (who steps soon, with her slack; an expert router's routing and the hot tier's plan are the next forecasts), memory pressure, completions | a readiness `Table` (state: each mind ready, preparing with an estimate, or refused) | Urgent when a forecast block will not land in time; Want under pressure; Content otherwise |

Its law, once, at every rung:
- a block is promoted early enough to land before its use: lead time is bytes over the measured link rate;
- under a tier's budget the fastest tier keeps the soonest-needed blocks, and the coldest is demoted one tier:
  demotion, never deletion; only the bottom tier deletes, and only what its owner allows;
- a pinned block never moves; a moving block moves again only after its completion;
- transfers run on the pager's worker; the engine thread reads its table and never waits on a move, and a
  block not resident when needed is a counted miss;
- when a move starts is a `decide` use case (`prefetch`), never a fixed lead: start now, or a learned margin before
  need, or only at need, scored both ways (late: the lane waited; wasted: moved and parked again unused). In the
  simulator, three minds in room for two on the 5090's day: 82.9 s of switching never prefetching, 71.7 s starting
  at park, 68.0 s learned.

The law is a `decide::Policy`, so placement shares the decide module's records, replay and checkpointed learning
with every other decision, and a new law is tried against recorded history without changing the pager.

## A mind's memory: the first consumer

**Built: her storage is hers for life, and the Core's stream is a view of it.** Each mind's KV and recurrent state
are attached over her own mapped file (`llama_persona_storage_attach`); the stream binds her storage when she is on
the Core, so a switch moves no row on unified memory. At a stop her storage is flushed and her file renamed into the
preserved directory with her record (custody by rename: metadata only, no byte copied); a restart renames it back
into its own instance directory before any byte can change, maps it and attaches it. Measured on the M5 (Metal,
fork 62923e28b): Qwen2.5-0.5B at 30k, hand-over 14-16 ms and restore 6.8-9.7 ms, her 371 MB file moved by rename,
the tokens after restore equal to the control's. Recurrent state by pointer took the 35B's switch from 3.16 ms to
0.01 ms (fork b46a43f93, a08234c04).

**Proposed: one storage object with an adapter per memory, never a branch.** Today `UnifiedTier::kv_memory` maps
her storage as a file where the device views host memory, and elsewhere reserves her whole stream-sized storage in
device memory for her life. On the 5090 that is a lifetime VRAM reservation per mind outside every ledger, which the
driver spilled into shared memory until it lost the device (2f8bd03a1). The fix is the object:
- `MindStorage` answers `storage_for(StorageAsk { mind, window })` with a held object whose drop returns its room,
  and is a `Resident` reporting each mind's storage, tier and how it varies;
- `Unified`: her storage a block over her mapped file; parked, the OS pages it;
- `Dedicated`: the bound mind's place is the stream's own backing, the one physical KV and recurrent allocation the
  context already holds, never memory beside it (Codex's review: one extra 9.28 GB slot beside the 30.8 GB Core
  already oversubscribed the 34.2 GB card). Parked, her used rows are one pinned host block of the same layout,
  leased as RAM; restore copies them into the stream's own memory and park copies them out, asynchronously on a copy
  stream ordered by events (the last writer before park, restore before the first reader), every rebind through
  `persona_moved()` so a CUDA graph never replays a stale address. A prefetch slot for the next mind exists only
  where measured headroom allows it, as one elastic lease (floor one slot while a mind is bound, zero idle).
- Step 1, built: no mind holds device memory of her own on a discrete GPU (the lifetime `reserve_storage` per mind,
  `DeviceRoom` and `CoreRoom` deleted), so her rows move through the stream's own memory by the existing copy path.
  Step 2, proposed: that copy made pinned, asynchronous and fenced as above.

**Proposed: the compute workspace is a block.** A backend's shared compute buffer (serving's graphs and a learner's
graph run in it) is a block keyed by its allocator generation and backing allocation, its size its region's size.
Its growth is consumed from a prepared claim at the scheduler's admission callback, never asked on the engine thread
(the claim and its law are placement's).

## Other copies of us

Built: each running core holds an OS file lock on its own lease file; the lock is the liveness truth, released by
the OS however the process dies. Each core has its own live directory, so two never map the same file.

Proposed:
- each core publishes its holdings beside its lease, so every `Topology` divides the node's budget among live cores;
- a core whose lock is released has its live directory reclaimed; its preserved minds are durable and untouched;
- **handoff with no downtime:** each mind's live file carries a custody lease. The old core parks her at a step
  boundary and releases it; the new core takes it and maps the same file, whose pages are still in the page cache.
  Model weights are shared read-only through the page cache, so the overlap costs one copy. The old core hands minds
  over in the order they will step and exits when it holds none. A mind whose lease was released and never taken is
  a clean file the next core takes; a torn one starts cold, named.

## The grid

Built for it from the first slice: a block's handle is (node, local id), never a pointer or a path; another node is
a place and the network a link, its transfer fenced like a DMA and its rate measured passively; custody leases have
two adapters (an OS lock on one node, a signed expiring lease across nodes); a block travels in its native layout
behind a small versioned header, and binds on another node only under the same model and KV layout fingerprint,
otherwise her tokens travel and she is prefilled there, named as a cold start.

## Per platform

- **The M5 (unified):** her storage a block over her mapped file, wrapped for Metal; a switch is `Exchange`; parking
  to flash is the OS writing dirty pages, and bringing her back is read-ahead started from the lane's forecast.
- **The 5090 (discrete, Windows):** the bound mind's storage in VRAM, the rest pinned and moved by DMA overlapped
  with decode, flash staged through pinned; CUDA graphs checked against every pointer change.
- **CPU-only nodes:** RAM and flash, with `Mapping` between them.

## Validators

- Every link: a block moved there and back is byte-identical, at 80% or more of the tier's measured rate.
- A mind's memory: parked to flash and back, she continues byte-identically to her isolated control
  (`test-persona-blocks --test-core`); a stop and restore at 30k and 130k restores in under 10 ms.
- The pager: a captured horizon trace replayed, misses and moved bytes counted; a law changes only if it beats the
  current one on the same trace.
- Live: misses and bytes moved per hour, and `busy_permille` (THE-LANE-ON-SAGE.md).

## The order

Built: the block layer, the Mapping and Exchange links, the pager (a `Readiness` leaf) and its learned start, a mind's KV and
recurrent state over her own file. Next, one PR each, each deleting what it replaces:

| Slice | Owner | Gate |
|---|---|---|
| `Topology` and tier budgets from `resources/free` | Fable | Mapping at 80% of the measured drive rate |
| `MindStorage` with the Unified and Dedicated adapters | Fable, Codex | the 5090 with residents holds parked minds on pinned memory, shared GPU use flat, no driver loss; the M5 unchanged |
| The compute workspace as a block, its growth from a prepared claim | Fable, Codex, Cormac | placement's gate (SHARED-RESOURCES.md) |
| Discrete tiers: `Dma`, `Staged`, the CUDA graph check | Codex | a swap hidden under decode on the 5090 |
| The hot tier's models as blocks; its private copier deleted | Codex | load from flash at the drive's rate |
| llama-server's page IO onto `Mapping`, or deleted with the HTTP lanes | Codex | no `fread` of a page body |
| Genes and experts as blocks | Cormac (genes), the expert pager's owner | page-in at the link's rate |
| `PressureBroker`'s eviction folded into the pager; pools that moved bytes deleted | Fable, Codex | no `evict_at_least` on a tier the pager owns |

## Open questions

1. Metal and a file mapping as a buffer: does `buffer_from_host_ptr` over a mapping keep GPU reads coherent while
   the OS pages it? Proposed: a block is wired while resident for the GPU, and demoted only after its transfer lands.
2. Windows mappings: `CreateFileMapping` and `MapViewOfFile` for `Mapping`, `FILE_FLAG_NO_BUFFERING` for `DirectIo`;
   is the pinned staging ring sized by the measured DMA rate?
3. A mind's window: her block is sized to her window (about 1.8 GB on the 35B at 262k); with a mapping untouched
   pages cost nothing until written, so is a lazily committed mapping enough, or should the window follow her
   measured need?
