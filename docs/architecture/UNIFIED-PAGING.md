# Unified paging: one block layer for every tier

Status: design, 2026-11-25. It replaces the 2026-04-18 version of this page, which designed
`PagedResourcePool` and a migration that did not happen for anything that moves large native memory.
Read with `THE-LANE-ON-SAGE.md` (the engine that uses it) and SAGE.md, "Memory is one cache
hierarchy, detected per machine" (the law it implements).

Joel, 2026-11-25: "do we have an object or even sage component for ram or vram to flash as a layer of
cache, and no encoding DMA. We use this a ton ... operate like rage would and sort of proactively
manage blocks of memory more generally then use that for the pieces."

## Why: five private movers and a law nobody implements

Counted in `core/` at canary 893ef7b9d (2026-11-25):

| What exists | What it does | What it does not do |
|---|---|---|
| `continuum-kernel` `paging`: `PagedResourcePool`, `PressureBroker` (44 files), 18 `ResourcePool` adapters, 27 `evict_at_least` callers, `FilesystemTierPool` | caches values in RAM; frees bytes under pressure | move bytes between tiers; its eviction deletes, it never demotes |
| `continuum-persona-engine` `residency::Tier` ("one contract for every residency tier: device VRAM, pinned host RAM, the unified pool, flash") | the right move contract, with typed fences | it has one adapter, `UnifiedTier` |
| the fork's per-mind KV storage and her file (#6264) | the Core's switch as a pointer; her file one write per layer | anything for anyone else |
| `model_registry::artifacts::hot_tier` (models, cold disk to flash), the expert pager, llama-server's page IO (`server-page-io.h`: `fread` into a vector, then a copy into KV; card 6af87187 maps it), 22 files using mmap | each moves its own bytes its own way | share a placement, a budget or a transfer |
| SAGE.md's fractal pager law | predict next use, keep it on the fastest tier with room, demote the coldest one tier down | it is written, not implemented once |

And the engine's own probe shows the cost: the M5's GPU is busy 558 permille with turns waiting
(THE-LANE-ON-SAGE.md, "Use every core").

## What ggml already gives us (read from the fork)

- `ggml_backend_dev_buffer_from_host_ptr(dev, ptr, size, max_tensor_size)` wraps memory we own as a
  device buffer where the device supports it. Metal does (`buffer_from_host_ptr = true`,
  `ggml_backend_metal_device_buffer_mapped`), and that memory may be a file mapping. CUDA does not.
- `ggml_backend_dev_host_buffer_type(dev)` gives pinned host memory: `ggml_backend_cuda_host_buffer_type`
  for CUDA, the memory a DMA reads at full rate.
- `ggml_backend_tensor_set_async` / `get_async` / `copy_async` and `ggml_backend_event_*` give DMA on a
  backend's stream, with a completion to fence on.
- `ggml_backend_cpu_buffer_from_ptr` wraps host memory for the CPU backend.

So every movement below is one of three things the hardware already does: a pointer, a DMA, or the
OS's own paging of a mapped file. Never a CPU loop over rows, never an encoding.

## The objects

One crate, `continuum-blocks`, below `continuum-mind` and `continuum-serving` and above
`continuum-sage` and `continuum-base`. It is named for its one job. The `llama` crate gains only FFI
wrappers for the ggml calls above. `continuum-persona-llama`, which depends on both, wires a block's
memory into a ggml buffer.

**`TierKind` and `Tier`: what memory a node has, detected, never listed.**

```rust
pub enum TierKind {
    Vram { device: DeviceId },     // discrete GPU memory
    Pinned { device: DeviceId },   // host memory the device DMAs at full rate
    Unified,                       // one memory for CPU and GPU (Apple silicon)
    Ram,                           // pageable host memory (CPU-only nodes)
    Flash { volume: VolumeId },    // a fast drive: file-backed mappings
    Cold { volume: VolumeId },     // a slow drive: sequential streams
}

pub struct TierRates { read_bps: u64, write_bps: u64, latency: Duration }   // measured, never constants

pub trait Tier: Send + Sync {                      // one adapter per kind holds only what differs
    fn kind(&self) -> TierKind;
    fn budget(&self) -> u64;                       // the resource authority's number for this tier (resources/free), never a second measurement
    fn rates(&self) -> TierRates;
    fn allocate(&self, bytes: u64, align: u64) -> Result<Region, TierRefusal>;
}
```

**Tiers are per volume, any number of them, ordered by measured rate.** A node's stack is its devices
plus every volume it can map, each measured, never assumed from its kind:
- Windows (the 5090): VRAM, pinned RAM, an NVMe volume (`Flash`), and often a spinning SATA disk
  (`Cold`).
- A Mac: unified memory, the internal flash, and any external volume over Thunderbolt or USB, which
  may be an SSD (`Flash`, slower than the internal one) or a hard disk (`Cold`).

Two `Flash` volumes are two tiers, ranked by their measured read and write rates. A volume that
appears or disappears (an external drive plugged in or out) is a `Topology` change. A block on a
volume that disappears is lost from that tier and counted, never assumed still there.

**`Topology` measures continually, never only at start.** We are not the only thing running: other
processes take RAM, VRAM and disk, a drive slows when it is hot or busy, and an external volume comes
and goes. A number measured once at boot is wrong within minutes, and a law placing blocks on it
stops working. So `Topology`, a SAGE component, keeps every quantity current, each from the cheapest
source that is true now:
- **Free memory per tier:** the resource authority's `resources/free`, which already follows the OS
  (free and compressed RAM, swap, pressure on macOS; device free memory from the backend for VRAM).
  `Topology` subscribes and never samples on its own.
- **Link and drive rates, passively:** every move a link makes is itself a measurement (bytes over
  measured time, by direction and volume), so the rates follow contention and thermal throttling at
  no cost. An active probe runs only for a volume no move has used within its staleness window, and
  only while the tier is idle.
- **Volumes:** mount and unmount, free space and its rate of change, from the OS's own notifications
  where it gives them (`DiskArbitration` on macOS, `WM_DEVICECHANGE` or volume notifications on
  Windows), and its free-space query otherwise.
- **Our own share against everything else:** what our blocks hold on each tier against what the tier
  has free, so pressure from another process shows up as a shrinking budget, not as our own misses.

Each publishes as frame-numbered state, so a law reads how stale a number is, never just the number.
A shrinking budget is an input like any other: the Streamer demotes to fit it before anything fails,
and a law that learned on a quiet machine adapts as the machine fills.

**And it grows back.** A budget that grows is an input too. When another process frees memory (someone
quits a game, a build finishes, our own old core exits after a handoff), the Streamer fills the room
back in. Blocks move up a tier ahead of their forecast use, more minds stay resident, and the Lane may
batch wider where the cost model says it pays. It fills in steps, re-measuring between them, so a
budget that shrinks again finds nothing stranded. We take what the machine offers and give it back
when the machine needs it, in both directions, continually. The hot tier's drive detection
moves into `Topology`.


**Other copies of us: in parallel, or gone.** Two cores on one machine (a development core beside a
running one, or two models served side by side) are each other's "everything else", and the
measurements above already see each other's memory. Copies of us also know each other, so they
cooperate instead of fighting over the same budget:
- **A lease per instance.** Each core holds an OS file lock on its own lease file in the node's
  shared blocks directory, for as long as it runs. The lock is the liveness truth, not a pid or a
  heartbeat: the OS releases it the moment the process dies, however it dies.
- **Its own live directory per instance** (`persona-blocks/live/<instance>/`). Two copies never map
  the same file, so a parallel copy can never write another's KV.
- **Shared holdings.** Each instance publishes what it holds on each tier beside its lease, so every
  `Topology` reads its peers' share as theirs and divides the node's budget between live instances.
  Each instance's law learns with its peers present, rather than both laws discovering each other's
  pressure as misses.
- **A copy going down is reclaimed, not leaked.** At start, and whenever a peer's lock is released,
  `Topology` finds live directories whose lease is no longer held. It returns their bytes to the
  node's budget and deletes their files. A dead instance's preserved minds are not touched, because
  they are durable state, not live memory.


**Zero-downtime handoff: a new core beside the old one, minds moved one at a time** (as an elastic
replacement on AWS starts the new instance before it stops the old). Because a mind's KV is already
a file the OS pages, moving her between two cores on one node copies nothing:
- **Custody per mind, not per instance.** Each mind's live file carries a custody lease, the same
  OS-locked lease as an instance's. Exactly one core holds it, and only the holder maps her file
  writable. Two cores can never both decode her.
- **The move.**
  - The old core parks her at a step boundary: her storage flushed, her header's clean mark
    written, her lease released.
  - The new core takes the lease and maps the same file. Her pages are still in the OS page cache,
    so nothing is read from the drive.
  - Her next step runs on the new core. Her wait is one park and one map, and her context is
    untouched.
- **Model weights are shared through the page cache.** Both cores map the same GGUF read-only, so
  the overlap costs one copy of the weights in memory, not two. Only what each core allocates for
  itself (its compute buffers, the stream's own KV) is extra. `Topology` counts the overlap, and the
  new core takes minds only as fast as the budget allows.
- **The old core drains, then exits.** It hands minds over in order of who is next to step (the
  Lane's forecast), so none waits behind the handoff. It takes no new mind once the new core holds
  its first lease, and exits when it holds none.
- **Either core going down mid-handoff loses nothing.** A mind whose lease the old core released
  and the new one never took is a released lease and a clean file, so whichever core runs next
  takes her. A mind still leased to a core that died has her lease released by the OS. Her file's
  clean mark then says whether her last park finished: a clean file is taken, a torn one starts her
  cold, named.

This extends `continuum-host`'s `core_handoff` (a core-only handoff that keeps the engine running)
to the case where the engine itself is replaced: the minds move, not the engine.

**`Region` and `Block`: the memory, and the thing consumers hold.**

```rust
pub struct Region { tier: TierKind, backing: Backing, bytes: u64 }   // RAII: Drop returns it to its tier
pub enum Backing {
    Device(NativeBuffer),                  // a ggml device buffer (VRAM)
    Host { ptr: NonNull<u8> },             // pinned, unified or pageable host memory
    Mapped { file: MappedFile, offset: u64 },  // a file-backed mapping: flash under the OS's pager
}

pub struct BlockId(u64);
pub enum BlockKind {                       // what the bytes are; never how they are encoded (they are not)
    MindKv { persona: PersonaId, epoch: Epoch },
    Weights { model: ModelId, shard: u32 },
    Expert { model: ModelId, layer: u32, expert: u32 },
    Gene { gene: GeneId },
    Embeddings { index: IndexId, shard: u32 },
    Training { seat: SeatId, part: TrainingPart },   // a seated learner's shadow, optimizer state, workspace: owner-held
}
pub struct BlockRef(Arc<BlockInner>);      // what a consumer holds
pub struct Pin<'b>(&'b BlockInner);        // while held, the block does not move (RAII)
pub struct InFlight<T> { value: T, done: Completion }   // device work still reading it: it moves after `done`
```

**Owner-held blocks.** Some blocks are leased and held by their owner, never placed: a seated
learner's shadow weights, optimizer state and workspace (`BlockKind::Training`) are `Pinned` for the
whole seat, because losing the optimizer mid-seat loses its moments. The Streamer counts them against
the tier's budget and never moves them; the trainer releases them on the engine's `Released`
(Cormac, 2026-11-25).

**One owner for "free".** A tier's `budget()` is the resource authority's number, which it says on
the node bus (`resources/free`, card 9d856843); `Topology` and the Streamer read it and never sample
memory themselves. Two owners of "what is free" is how the 5090 trainer once read 428 TB free
(Cormac).

**The OS calls live in `continuum-host`.** A new adapter module, `mapped_region`, holds a writable
file mapping and its calls:
- Unix: `mmap`, `msync`, `madvise`, plus `F_NOCACHE` on macOS and `O_DIRECT` on Linux.
- Windows: `CreateFileMapping`, `MapViewOfFile`, `FlushViewOfFile`, `PrefetchVirtualMemory`, and
  `FILE_FLAG_NO_BUFFERING`.

It sits beside `positioned` and `read_pin`: logic in `mod.rs`, the calls in `unix.rs` and
`windows.rs`. `continuum-blocks` holds only the logic, and never a `cfg(windows)`.

A block's bytes are its native layout: KV rows as the cache lays them out, weights as the GGUF lays
them out. Moving a block never interprets it.

**`Link`: how bytes go from one tier to another, one adapter per pair.**

```rust
pub trait Link: Send + Sync {
    fn pair(&self) -> (TierKind, TierKind);
    fn rates(&self) -> TierRates;                                   // measured on this node
    fn start(&self, from: &Region, to: &mut Region) -> Result<Transfer, LinkRefusal>;
}
pub struct Transfer { fence: Box<dyn residency::Fence> }   // the fence type that exists today
```

| Link | Between | What moves |
|---|---|---|
| `Retag` | `Unified` and `Unified` (a parked mind and the Core's stream on the M5) | nothing: the owner changes (the pointer switch, #6264) |
| `Dma` | `Vram` and `Pinned` | async copy on the device's copy stream, an event as the fence; overlapped with compute |
| `Mapping` | any host tier and `Flash` | the region is a file mapping: demote = `msync` plus `MADV_DONTNEED` (the OS writes dirty pages); promote = `MADV_WILLNEED` (the OS reads ahead); a page touched before it lands is a counted miss |
| `DirectIo` | any host tier and `Cold` | one unbuffered sequential stream (`F_NOCACHE`, `O_DIRECT`, `FILE_FLAG_NO_BUFFERING`) on an IO worker, aligned to the drive |
| `Staged` | `Vram` and `Flash` or `Cold` | `Mapping` or `DirectIo` into pinned memory, then `Dma`: two links as one transfer |

**`Streamer`: the one SAGE component that places blocks (RAGE's streaming system).** It is the first
instance of SAGE's `Placer` (SAGE.md, "Ebb and flow: one placement pattern at every scale"). Who is
awake on a node and who runs where on the grid are the same pattern, with minds as the units.

| Hears | Says | Need |
|---|---|---|
| `Demand` (each consumer's forecast: the Lane's `Forecast` of who steps next, the expert router's routing, the hot tier's model plan); `Topology`; memory pressure; `Completion`s | `Placement` (state: every block's tier and whether it is moving, frame-numbered); `Moved` (events) | Urgent when a forecast block will not land before it is needed; Want under pressure; Content otherwise |

The law, once, at every rung:
- Each block's score is its predicted time to next use, from `Demand`.
- A block is promoted early enough that it lands before use: lead time is bytes over the measured
  link rate, plus a margin learned from its misses.
- Under a tier's budget the fastest tier keeps the soonest-needed blocks. The coldest is demoted one
  tier down: demotion, never deletion. Only the bottom tier deletes, and only what its owner allows.
- A pinned block never moves. A block under `InFlight` moves only after its `Completion`.
- Transfers run on the Streamer's own workers. The engine thread never waits on one: it reads
  `Placement`, and a block not resident when needed is a loud, counted miss.

**The law is pluggable, as the Lane's is.** The Streamer's decisions come from a `PlacementLaw`, the
same shape as `continuum_sage::slice::Control`. The Streamer around it stays the same: budgets,
pins, `InFlight`, and the links. So a law can be:
- swapped live;
- compared in a simulator on a node's real load;
- replayed exactly against a captured trace;
- replaced by a learned law (reinforcement learning, a fitted model) with no change to anything
  around it.

```rust
pub trait PlacementLaw<K>: Send {
    fn name(&self) -> &'static str;
    /// Named, typed parameters, read and set by name (cv::Algorithm's get/set), so a tuner, a learned
    /// policy or an operator adjusts a law without knowing its type.
    fn params(&self) -> Vec<Param>;
    fn set(&mut self, name: &str, value: f64) -> Result<(), ParamRefusal>;
    /// Bidirectional: everything the Streamer saw, in order, including what came of the law's own
    /// decisions (a miss, a move's measured time): the reward signal a learned law trains on.
    fn observe(&mut self, seen: &Observation<K>);   // the Forecaster sees the same stream: both seams learn
    /// The moves it wants now, given each block's placement and each tier's budget.
    fn plan(&mut self, view: &View<K>) -> Vec<(DecisionId, Move<K>)>;
}

pub enum Observation<K> {
    Demand { key: K, at: Instant, bytes: u64 },       // a consumer's forecast of next use
    Used { key: K, at: Instant },                     // the block was read
    Moved { key: K, from: Placement, to: Placement, bytes: u64, took: Duration },
    Missed { key: K, needed_at: Instant, landed_at: Instant },   // the outcome that scores a law
    Refused { decision: DecisionId, why: Refusal },  // a move the Streamer would not make, and why
    Budget { tier: TierKind, bytes: u64 },            // the resource authority's number
    Pressure { tier: TierKind, level: Pressure },
}
pub enum Move<K> { Promote(K, TierKind), Demote(K), Hold(K) }
```

**Easy for any algorithm to control: four contracts.**

1. **The `View` is borrowed, never copied, and laid out for a model.** `plan(&mut self, view: &View<K>)`
   borrows the Streamer's own state for the call. Every quantity a law could weigh is a column, one
   row per block, in a struct of arrays, so a learned law reads `&[f32]` slices straight into a tensor
   and a hand-written law indexes rows:

   ```rust
   pub struct View<'s, K> {
       pub keys: &'s [K],
       pub blocks: &'s Columns,   // per block: bytes, tier, placement, pinned, in_flight,
                                  // since_used_s, forecast_next_use_s, forecast_error_s, misses
       pub tiers: &'s Columns,    // per tier: budget, resident, moving, read_bps, write_bps, pressure
       pub links: &'s Columns,    // per link: rate_bps, queued_bytes, last_took_s
       pub frame: u64,            // the frame this view was taken at
   }
   pub struct Columns { schema: &'static Schema, data: Vec<f32>, rows: usize }   // column-major
   pub struct Schema { version: u32, names: &'static [&'static str] }             // named, versioned
   ```

   Adding a measurement adds a named column and bumps the schema version. A law asks for a column by
   name and is told when one is missing; a trained model is tied to the schema version it was
   trained on, and is refused on any other.

2. **The Streamer enforces the invariants, not the law.** A plan is a set of requests. Before acting,
   the Streamer drops any move that would:
   - move a pinned or in-flight block;
   - exceed a tier's budget;
   - demote a block forecast to be needed sooner than the demotion takes.

   Each dropped move is an `Observation::Refused { decision, why }`, so a learned law learns from
   it. A bad or still-learning law can make the system slower, but never wrong.

3. **Every decision carries an id, and its outcome comes back on it.** `plan` returns
   `Vec<(DecisionId, Move<K>)>`. Every later observation the move causes (`Moved`, and `Used`/`Missed`
   for the block, or `Refused`) carries the same `DecisionId`, so credit assignment for reinforcement
   learning is a join on the id. Nobody infers it from timing.

4. **A run is training data as it stands.** The `Trace` stores each frame's `View` columns, the
   decisions and their outcomes, as typed binary (the columns' own `f32`s, the schema version in the
   header). It is never JSON: a learner, the simulator and replay all read the same bytes.

**Measured with the future in mind.** Some columns are recorded before any law uses them, because a
learned law may want them and they cannot be recovered later:
- each block's use history (inter-use gaps);
- its forecast error;
- each consumer's declared demand next to the actual use;
- per-link queue depth and achieved rate;
- tier pressure.

The cost is a few floats per block per frame, written off the engine thread.

Two more seams, where a learned model is most likely to win, each with one implementation now:
- `Forecaster`: a block's predicted time to next use, from its `Observation` history. The first
  implementation takes the consumer's declared `Demand` (the Lane knows who steps next) and corrects
  it by its measured error. A learned forecaster replaces it behind the same trait.
- `LinkModel`: a move's predicted time, from bytes and the link's measured rates. It is the same
  cost model the Lane's `Cost` component owns (THE-LANE-ON-SAGE.md), read from there, never a
  second one.

**No hand tuning: initialized from measurement, then learning for its own throughput.** A law never
ships with a tuned constant. It starts from a good default where nothing is measured yet (first boot, a
new model, a new drive), and from what this node measures as soon as it has it:
- link rates and switch and rebuild times from the cost model;
- forecast error from the consumer's declared demand;
- tier budgets from the resource authority.

From there it learns online, on its own objective. The objective is measured throughput (useful
tokens per second across all minds) under the hard constraints: the wait bound, no misses for
forecast blocks, and every invariant the Streamer enforces. Every parameter a law exposes is a
starting value the learning moves, never a setting someone picked. The Lane's `Tuner` (the slice's
control loop climbing measured throughput against the wall) is the same shape; both laws are judged
by the same number on the same node.

**What a law learned is kept, so it never relearns.** A law's state is saved and restored like any
other state on the node: its parameters, its forecaster, and the best-scoring parameters it has found.
- **Keyed by what it was learned on:** the node's fingerprint (devices, memory, each volume with its
  measured rates), the model, and the schema version of its `View`.
- **At start** a law loads its last best for that key. It falls back to the good default only for a
  key it has never seen (a new model, a new drive), and then to the closest key it has (the same
  node with another model), because what transfers is most of what it knew.
- **It keeps two states:** the current one it is exploring from, and the last best by measured
  throughput. It returns to the best when exploring loses. Saving is off the engine thread, on a
  frame cadence, and at a stop.
- **Stored as typed binary** (its parameters and the forecaster's tables as raw arrays, the schema
  version in the header), never JSON. A key from another node with the same fingerprint (two
  identical Macs) may seed this one, under the same schema check.

This is SAGE's algorithm lifecycle (SAGE.md, "What an algorithm learns is part of its lifecycle"): the
Streamer's laws, the Lane's `Tuner` and any learned law get default, resume, learn, checkpoint and stop from
it, once.

What ships with slice 3 is one law, `FractalPager` (the fractal pager law above), plus `Lru` as the
baseline every other law is compared against. `Lru` is never a production law, as `Fixed` is the
Lane's baseline. Laws are made by name from a registry of factories (`laws::make("fractal")`,
CLAUDE.md's polymorphism pattern), so a node's configuration or a trial picks one without code.

**Every measurement is easy to take.**
- Each `Observation` is also emitted on the engine's bus as a stream (`blocks/observed`), so any
  consumer subscribes without wiring: a probe, a dashboard, a trainer for a learned law.
- Each tier's and each link's state (bytes resident, bytes moving, measured rate, misses per window)
  is published as state (`blocks/tiers`, `blocks/links`).
- A run is a `Trace` (every observation and every plan, timed), as the Lane's is:
  - replayed under the same law, it reproduces every decision: the regression check;
  - fitted to a workload, it drives a simulator where any law, learned or not, is scored on misses,
    bytes moved and wait on the load a real node carried.

**What it replaces, and what stays:**
- `PressureBroker` and the `ResourcePool` adapters: each tier registers as a `ResourcePool`, so the
  broker keeps working during migration. Its `evict_at_least` becomes "demote this many bytes one
  tier down", and the broker's job folds into the Streamer once its pools have migrated.
- `PagedResourcePool` stays what it is: a keyed cache of small values in RAM (embeddings, adapter
  metadata). Large native memory goes through blocks.
- `residency::Tier` stays the Core's view of one mind's move. Its blocks become `BlockRef`s, and
  `UnifiedTier` asks the block layer for her storage instead of the fork reserving it privately.

**Corrected by building slice 1 (2026-11-25):**
- **A block's key is the consumer's own type** (`Block<K>`), not a `BlockKind` enum. An enum naming
  `PersonaId` or `ModelId` would make the floor crate depend on the crates above it. The kinds in the
  table above are the keys consumers bring.
- **A link carries one block to a placement** (`Link<K>::carry(&mut Block<K>, Placement)`); it does
  not copy one region into another. A mapped block demoted to `Flash` and promoted back keeps its
  region and address; only the OS's pages move. The pointer switch is `Exchange`, two blocks trading
  regions.
- **The completion is the block layer's own.** `residency::Fence` lives above it, in
  `continuum-persona-engine`, which adapts to it rather than the floor depending upward.
- **A carry may still be in flight** (Codex, on the 5090, 2026-11-25). A synchronous `carry` returning
  `Moved` cannot represent an outstanding DMA. `carry` returns `Carry::Done(Moved)` for a move that has
  landed (`Mapping`, `Exchange`), or `Carry::InFlight(Transfer)` for one that has not (`Dma`, `Staged`).
  A `Transfer` holds both regions and the block's pins until its event signals, and the Streamer
  publishes the new placement only then. Cancelling a transfer waits for its event and leaves the
  block where it was.

## Built for the grid from the first slice

Moving a mind or a block to another node is the same move as moving it to another tier, with a
network link between them. So that works later with no rework, everything built now keeps these:
- **Handles are grid-global.** A `PersonaId` is already a UUID. A block's handle is (node, local id),
  never a pointer or a path. A placement names a node beside its tier.
- **Another node is a place, and the network is a `Link`.** A transfer to a peer is a
  `Carry::InFlight` with a fence, as a DMA is. Its rate is measured passively from every transfer,
  as a drive's is. The law weighs a node's price and its link's rate against a local tier's, with no
  special case.
- **Custody leases have two adapters.** On one node, an OS file lock. Across nodes, a grid lease:
  signed, with an expiry, renewed by its holder. The same `Lease` contract either way, so exactly one
  holder decodes a mind anywhere on the grid.
- **A block's bytes travel as they are.** Her KV in its native layout behind a small typed,
  versioned header (owner, epoch, layout fingerprint, rows). The same bytes go by mapping, file or
  network, never re-encoded.
- **Compatibility is checked, never assumed.** Her KV binds on another node only under the same
  model and the same KV layout fingerprint (`kv_rows`). Otherwise her tokens travel (raw `i32`s) and
  she is prefilled there, named as a cold start, never bound to KV that is not hers.
- **Each node's `Topology` publishes a summary to the grid** (capacity, prices, link rates), as
  frame-numbered state over airc. That is the market every node's law reads.
- **Trust bounds what travels.** A placement on a node or peer never carries what its trust level
  does not allow (SAGE.md, "One economy at every scope").

## Per platform

- **The M5 (unified memory):**
  - A parked mind's KV is a `Unified` block backed by a mapping of her own file, wrapped for Metal by
    `buffer_from_host_ptr`.
  - A switch is `Retag` (a pointer). Parking her to flash is the OS writing her dirty pages back;
    bringing her back is read-ahead the Streamer starts from the Lane's forecast.
  - Her file is her memory: no save step, and nothing written that the OS does not page out itself.
- **The 5090 (discrete, Windows):**
  - The Core's stream and as many minds as the VRAM budget holds are `Vram` blocks, switched by
    pointer.
  - The rest are `Pinned`, moved by `Dma` on a copy stream overlapped with decode.
  - Flash is `Staged`: a mapping into pinned memory, then DMA.
  - Codex's CUDA graph check (#6264) applies to every `Vram` pointer switch.
- **CPU-only nodes:** `Ram` and `Flash`, with `Mapping` between them.

## Validators (seconds, any machine), then the live read

- **Every `Link`:** a block moved there and back is byte-identical; its throughput is reported against
  the tier's measured rate, and below 80% of it is a defect.
- **The KV consumer:** a mind parked to flash and back continues byte-identically to her isolated
  control (the A/B/A check of `test-persona-blocks --test-core`, extended to `Mapping`).
- **The Streamer:** replay a captured `Demand` trace; count misses (a block needed before it landed)
  and moved bytes. A law changes only if it beats the current one on the same trace.
- **Live:** misses per hour, bytes moved per hour, and `busy_permille` (THE-LANE-ON-SAGE.md, gate 950).

## The order, one PR each, each deleting what it replaces

| # | Slice | Owner | Gate |
|---|---|---|---|
| 1 | `continuum-blocks`: `TierKind`, `Region` (host or mapped), `Block<K>`, `Pin`; the `Mapping` link and `Exchange` (the pointer switch); `continuum-host::mapped_region`. **Built:** 64 MiB to flash and back byte-identical at the same address; a pinned block never moves; an exchange trades memory by pointer | Fable | link validators on the M5 |
| 1b | `Topology` and the tiers' budgets, once the resource authority says `resources/free` on the bus (card 9d856843, Cormac's delta 8). The drive's measured write rate checks the Mapping link: 64 MiB demoted at 1.06 GB/s on the M5 with serving beside, below what an SSD gives | Fable | Mapping at >= 80% of the measured drive rate |
| 2 | The first consumer: the Core's parked minds' KV as `MindKv` blocks. Deletes the fork's private storage reservation (#6264), her separate file write, and `preserve.rs`'s JSON token history (her tokens are a block beside her KV) | Fable | `--test-core` byte-identical through a flash round trip; stop and restore at 130k at the drive's rate |
| 3 | The `Streamer` with `PlacementLaw` (`FractalPager`, `Lru` as baseline), `Forecaster`, the law registry, `blocks/observed` and the `Trace`, simulator and replay; the Lane's `Forecast` as its first `Demand` | Fable | replay reproduces every decision; zero misses for forecast minds; `FractalPager` beats `Lru` on a fitted live trace |
| 4 | Discrete: the `Vram` and `Pinned` tiers, `Dma` and `Staged` links, the CUDA graph check | Codex | the 5090: a swap hidden under decode |
| 5 | The hot tier as `Weights` blocks; its private copier deleted | Codex (owns the catalog) | model load from flash at the drive's rate |
| 6 | llama-server page IO onto `Mapping`, or deleted with the HTTP lanes (the inference audit's line 2) | the card 6af87187 owner, with Codex | no `fread` of a page body |
| 7 | Genes (`Gene` blocks) and experts (`Expert` blocks) | Cormac (genes); experts with their pager's owner | gene page-in at the link's rate |
| 8 | `PressureBroker`'s eviction folds into the Streamer; pools that moved bytes are deleted | Fable, Codex | no `evict_at_least` left on a tier the Streamer owns |

## Slice 2, exactly: a parked mind's KV is a block backed by her own file

**The fork** (`src/llama-persona-storage.cpp`, its own file; one-line hooks):
- `llama_persona_storage_bytes(ctx)` and `llama_persona_storage_alignment(ctx)`: the bytes and
  alignment of one mind's KV storage for this context's layout. Every layer's K and V, each at an
  offset aligned for its buffer type.
- `llama_persona_storage_attach(persona, base, bytes)`: her storage tensors are built over memory the
  caller owns, through `ggml_backend_dev_buffer_from_host_ptr` on each KV device, in place of the
  fork's private reservation (#6264, deleted). It is refused as `unsupported` when a KV device cannot
  wrap host memory (CUDA, until slice 4). Then her storage stays the fork's own allocation, as on
  canary today: the discrete adapter is slice 4's.
- `llama_persona_write_file` no longer writes her attention rows when her storage is attached, because
  they are already in her file. Its record (format 2) says where her rows are: `in_storage` or
  `in_record`. `llama_persona_read_file` takes an `in_storage` record only into a persona whose storage
  is attached, and checks the storage's bytes against the record's before taking anything. Her KV file
  carries no header, so it is exactly her storage, and a torn write cannot leave a half-written mark.

**The Rust side:**
- `continuum-persona-llama`: a mind's `Blocks` holds a `Block<MindId>` for her KV:
  - `Region::mapped(Unified, <home>/persona-blocks/<persona>.<epoch>.kv, header + storage bytes)`;
  - created at her first park;
  - attached to her fork persona;
  - dropped with her.
- `preserve.rs`, **custody by rename** (corrected while building the second half, 2026-11-25):
  - A running mind's file lives in this core's instance directory (`live/<instance>/`). That directory
    is reclaimed by the next core to join once this one's lease is released, so a file left there at
    a stop would be deleted before the restart maps it. The stop moves her file out, and a restart
    moves it back in. A rename is metadata only, so no byte is copied either way.
  - The stop, per mind: flush her region (`msync`, then the file synced), then the fork writes her
    **record** to `<persona>.<generation>.cpb`. The record holds her identity, layout, row count, cell
    table and recurrent rows; her attention rows are not in it, because they are already in her
    file. Then her mapping is dropped and her file renamed to `<persona>.<generation>.kv` in the
    preserved directory, which is then synced. Her token history is written beside it as raw `i32`s
    (`<persona>.<generation>.tok`, one write of the bytes she holds, never an encoding).
  - The manifest names each mind's three files, generation and token count. It no longer carries her
    tokens (the 5.3 MB of JSON on the M5).
  - **Custody is per mind** (Codex, 2026-11-25). A core takes her custody lock
    (`<persona>.custody` in the preserved directory, an OS lock like an instance's lease) before
    it touches her files, and holds it from her restore through her stop's sync, unmap and rename.
    So two cores contending for one mind (a parallel copy, or a handoff) never both move her file:
    the second is refused while the first holds her.
  - **The clean mark is where her file is.** Under her custody, the restart first checks everything it
    can without moving anything: her record's generation (the manifest's), identity, layout and
    storage bytes, her `.kv`'s exact length and her `.tok`'s exact length (four bytes per token,
    the manifest's count). Only then does it rename her `.kv` from the manifest's path into its own
    instance directory, before any byte of it can change. Then it maps it, attaches it as her
    storage and reads her record's cells. No row is read until a page is touched. Once her file has
    left the manifest's path, a crash leaves her cold and named: only a file that a completed stop
    put at that path restores.
  - The manifest is published only once every mind's three files are durable (each synced, then the
    directory synced).
  - Where her storage is the fork's own memory (a device that cannot wrap host memory, until slice
    4), her record carries her rows, as the file does today. The record says which.

**The stream is a view; a mind's storage never changes hands** (corrected while building slice 2).
#6264's exchange swapped storage between the stream and the outgoing mind. With anonymous memory
that is invisible. With a file-backed storage it would carry her file to the next mind, who would
write her KV into it. So:
- every mind owns her storage for life, attached over her mapped file or, where a device cannot wrap
  host memory, reserved by the fork;
- binding her points the stream's layer tensors at her storage, and parking her points them back at
  the stream's own;
- the cells object still swaps as today (small, owned metadata).

No ownership moves, so `persona_resident` and the ownership swap are deleted. A clear zeroes whatever
the stream views. Storage is taken at a mind's first bind, never by a copy at her first save. The
stream's own KV allocation then serves only a mind without storage; its size at the window (about
1.8 GB on the 35B) is measured and its release is a follow-up.

**Crash safety:**
- A mapped file changes while she runs, so only a file a completed stop put at the manifest's path
  restores her (custody by rename, above).
- A crash while she runs, or during a stop before its manifest, leaves her cold and named, as today
  when a stop does not finish. A torn file is never bound.
- An old manifest (format 2: rows inside each record, tokens as JSON) is migrated, never dropped,
  because a named cold start loses her continuity (Codex). An old record is exactly an `in_record`
  record, the same read a device without attachable storage uses. So the restart copies her rows into
  her new mapped storage once, the manifest upgrade carries her tokens until that stop, and the
  stop writes the new format. The upgrade is deleted once no node holds a format-2 manifest.

**Gates added by the second half** (Codex):
- two cores contending for one mind's custody, on each side of each rename: one holds her, the
  other is refused, and nothing is lost;
- a handoff in the new format with next-token continuity: her first token after the restore is
  byte-identical to a control that never stopped;
- Windows: a rename while another mapping of her file is still held fails with its error carried
  to the stop's report, and leaves the manifest's last generation whole.

**Measured, the second half as built** (M5, Metal, 2026-11-25, fork 62923e28b and its cell-table commit):
- `persona_handoff` (control never stopped; the old core hands her over and ends; a new core joins the
  same root, refuses another generation in place, restores her, refuses the path her file left, and
  decodes on):
  - qwen2.5-0.5b, 1k prompt: hand-over 12.9 ms, restore 4.1 ms, record 18 KB, KV file 15.7 MB moved by
    rename, 32 of 32 tokens after the restore equal the control's.
  - qwen35moe (hybrid), 1k prompt: hand-over 12.8 ms, restore 3.2 ms, record 158 KB (her recurrent rows
    stay in it), 32 of 32 tokens equal.
  - qwen2.5-0.5b, 30k prompt: hand-over 14–16 ms, restore 6.8–9.7 ms over three runs, record 510 KB,
    KV file 371 MB moved by rename, 16 of 16 tokens equal.
- Restore's parts at 30k: map 0.1 ms; attach (Metal wrapping her 371 MB) 2.3 ms; her record 0.3 ms (2.3
  ms before the cell table moved in one transfer); the synced rename 3.4 ms.
- Not yet measured: 130k on the 35B (the live model; it shares the M5 with serving). Owner Fable; due at
  this change's first deploy on the M5, read from the live stop and restore probes.
- The rename's directory sync is on restore's path because a crash after an unsynced rename could put
  her changed file back at the manifest's name. Overlapping it with the attach and the record read is
  the next cut if 130k misses 10 ms.

**Deleted:**
- the fork's private storage reservation;
- the row copy in her file write and read;
- the manifest's token history as JSON;
- the per-generation rewrite of every mind's whole file at every stop (a stop now flushes only
  dirty pages).

**Gates:**
- `test-persona-blocks --test-core` with attached storage: A/B/A byte-identical to isolated
  controls;
- a mind parked to flash (demoted) and back, byte-identical;
- a stop and restore at 30k and 130k: restore in under 10 ms before her first touch, and her first
  token byte-identical to her control;
- a crash simulated after her file left the manifest's path (a restart that never stopped): refused,
  cold, named.

## Slice 2c: her recurrent state by pointer too (Joel, 2026-11-25: "make sure transfers are fast and lowest possible cpu cost")

**Found** (M5, Metal, `persona_switch`, 2026-11-25): on a hybrid model the Core's switch is a pointer for her
attention rows but a copy for her recurrent state. `persona_commit_save` reads her r and s rows out of the
stream (`llama-memory-recurrent.cpp:493`, one `tensor_get` per layer per snapshot group) and
`persona_commit_load` writes the next mind's back (`:572`). On Metal each is a CPU memcpy.

| model | her recurrent state | switch p50 | p90 |
|---|---|---|---|
| qwen2.5-0.5b (attention only) | none | 0.01 ms | |
| qwen35 0.8B (hybrid) | small | 0.85 ms | 1.43 ms |
| Qwen3.6-35B-A3B (the live model, hybrid) | 62.8 MiB (R 2.8, S 60.0) | 3.16 ms | 4.95 ms |

On the 35B a decode step is about 26 ms (39 tok/s), so a mind switching at every step spends about 12% of
the step copying 126 MiB with the CPU. Her rows also ride in her record at every stop (63 MiB written and
read where her attention rows move by rename). And the PLE rows (`p_l`) are not saved at all: a model with
them would lose that state at a switch.

**The design: the same object as her KV storage.** On the Core the recurrent memory has one cell
(`mem_size` 1), so each layer's r, s and p tensors are exactly her rows in every snapshot group. Her
storage is tensors of that exact shape, laid out after her KV in the same run of memory (her one mapped
file on Metal; her one VRAM reservation on CUDA):
- `llama_rs_storage` beside `llama_kv_storage`: r, s, p per layer, its buffers, and the memory viewing it;
- `llama_memory_recurrent::persona_view(storage)`: each layer's tensor data, buffer and extra point at
  hers, as `llama_kv_cache::persona_view` does; `nullptr` points them back. The context already drops its
  graphs after a move;
- save: the stream views its own memory again, her cell's record (position, rollback index, whether she
  holds state) is kept, her cell is emptied; no row moves. Load: the stream views her storage, her cell's
  record goes into cell 0; no row moves. A mind she follows with no state starts from the zeroed cell
  the recurrent memory already clears for a new sequence;
- the hybrid's storage bytes are its attention bytes, aligned, plus its recurrent bytes; attach and
  reserve place both, so one file (or one reservation) holds her whole state;
- her record: rows in storage means all her rows, attention and recurrent; the record keeps only her
  cells and her recurrent cell's record. `attached_bytes` names the whole run.

A mind without storage (several streams, an example's native home) keeps the row-copy move; it is not
the Core.

**Not in this cut:** a checkpoint (`persona_commit_checkpoint_rows`) still copies her rows out, once
per act rather than per step. The snapshot groups the memory already keeps (`n_rs_seq`) are the
in-place form of it; that is the next cut, measured by the same probe.

**Gates:**
- `persona_switch` on the 35B: switch p50 within 0.05 ms of the attention-only model's;
- `test-persona-blocks`: A/B/A byte-identical on a hybrid model, attached and reserved;
- `persona_handoff` on the hybrid: record without recurrent rows, 32 of 32 tokens equal to the control.

**Measured, as built** (M5, Metal, 2026-11-25, fork b46a43f93 and a08234c04; `persona_switch <model> 3 256 8 4`):

| model | switch p50 before | after | p90 after |
|---|---|---|---|
| qwen2.5-0.5b (attention only) | 0.01 ms | 0.01 ms | 0.01 ms |
| qwen35 0.8B (hybrid) | 0.85 ms | 0.01 ms | 0.02 ms |
| Qwen3.6-35B-A3B (hybrid, live model) | 3.16 ms | 0.01 ms | 0.02 ms |

- On the 35B, decode over those turns read 39.0 tok/s before and 49.3 after, at the same parameters. The node
  was serving residents during both runs, so that gap is not claimed for this change until the live read.
- `test-persona-blocks` and `--test-core 3`: byte-identical on qwen2.5-0.5b and qwen35-0.8b (24 recurrent
  layers); the hybrid's record is 0.1 KB over 23.3 MB of attached storage.
- `persona_handoff` on qwen35-0.8b, 1k: record 18 KB (her recurrent rows used to ride in it), file 36.0 MB
  moved by rename, restore 3.4 ms, 32 of 32 tokens equal to the control.
- Not measured here: the reserved (VRAM) path, and a stop from it, which now stages through the device's
  pinned host memory. Owner Codex on the 5090: `test-persona-blocks --test-core 3` and `persona_handoff`.

**Also cut in this change:** a mind's token history is written and read as its own bytes (`bytemuck`),
refused by the file's length before any byte is read; the per-token encode and decode loops are gone.

**Deleted:** the row copy of her recurrent state at every switch on the Core, the recurrent rows in an
attached mind's record, and the KV storage's private layout, reserve, attach and repoint code (now
`llama-persona-view`, shared by both memories).

## Questions for review

1. **Metal and a file mapping as a buffer** (Fable, slice 1): does `buffer_from_host_ptr` over a
   mapping keep GPU reads coherent while the OS pages it? Proposed: a block is pinned
   (`MADV_WILLNEED`, wired) while it is resident for the GPU, and demotion happens only after
   `InFlight` completes.
2. **Windows mappings** (Codex, slice 4): `CreateFileMapping` plus `MapViewOfFile` for `Mapping`;
   `FILE_FLAG_NO_BUFFERING` for `DirectIo`. Is the pinned staging ring sized by the measured DMA rate?
3. **Window size per mind** (everyone): a `MindKv` block is sized to her window (about 1.8 GB on the
   35B at 262k). With `Mapping`, untouched pages cost nothing until written, so is a lazily committed
   mapping enough, or should the window shrink to her measured need?
