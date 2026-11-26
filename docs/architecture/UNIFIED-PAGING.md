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
  `Topology` subscribes and never samples on its own. On unified memory, GPU allocations do not show
  in the OS's free or available counters (measured elsewhere on Apple and Linux iGPUs; SPEED-BRAINSTORM,
  "Lessons: inference-optimisation-skill"), so the authority's unified number must include the
  driver's resident GPU bytes (`ioreg`/`vm_stat` on Apple), or the ledger's budget is a number that
  is not true.
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
// as built (continuum-blocks/src/link.rs, 2026-11-26); a pair's link is found through the node's Links table
pub trait Link<K>: Send + Sync {
    fn name(&self) -> &'static str;
    fn start(&self, block: &mut Block<K>, to: Placement) -> Result<Transfer, LinkRefusal>;  // answers at once
}
pub struct Transfer { bytes, started, to: Placement, done: Box<dyn Completion>, into: Option<Region> }
// into: the destination its tier reserved before the copy; Transfer::land gives it to the block only when
// done fires, so the source stays charged and valid until then, and a refused move leaves the block as it was
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
- **When a move starts is learned, never a fixed lead** (Joel: "If it's by a learning algorithm with feedback
  it'll figure it out"; "super important to use our extensible algorithms"). Each parked mind's move is a
  decision of the `prefetch` use case (`continuum-sage/src/decide/cases/prefetch.rs`): start now, her copy plus k
  spreads of the need prediction before her need, or only at need. Its outcome is measured both ways: late (the
  lane waited on her) and wasted (a move abandoned for a mind needed sooner, or landed and parked again unused).
  The stock `linear-ucb` learns it; `baseline` and `at-need` are registered beside it for comparison. In the
  simulator (`slice/room.rs`, the Streamer's law against a lane), three minds in room for two on the 5090's day:
  82.9 s of switching never prefetching, 71.7 s starting each move when parked, 68.0 s learned. Two minds the room
  holds switch by pointer (6.4 s, as unified); room for one copies at every switch (129 s).
- **The Dedicated block, per Codex's review (2026-11-26):** one device buffer per mind is compatible with
  `persona_view` and the listed layer order, with aligned backend offsets and one tensor descriptor per K, V and
  recurrent tensor. The stream assumes full-sized tensors today, so a window-sized binding must first constrain cell
  indexing, strides, child views and clears to the allocated extent before it is published, and a landing is never
  published while a prior decode still reads that buffer.

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

## Every byte the engine holds is a block: workspace, admission and reclaim

Joel, 2026-11-26: "The low level cache/memory subsystem needs to be a whole smart object and sage based
feature, not patched together. So there's no mystery and it works."

**What the night of 2026-11-26 showed.** The design above places blocks the Streamer moves. The bytes
that refused learning all night were bytes it does not see. Each refusal came from a different
counter's private picture of the same memory:

| Counter (2026-11-26, `fable/lesson-rows`) | Its picture | What it got wrong |
|---|---|---|
| `LearningMemory::workspace` (`continuum-engine/src/inference/learning/memory.rs`) | the seat's measured row against `required.max(held)` | refused every lesson smaller than what serving had grown the shared buffer to (MTL0 required 1.80 GB within held 4.62 GB, refused against 3.91 GB measured). Fixed in d119ff4dfb by checking growth only. |
| `lease_memory` (`continuum-genome/.../learner/seat.rs`) and `LearningMemory::from_rows` | the gross `SharedComputeRequired` sum | prices a seat at the whole workspace when only the growth past what serving holds is allocated (Codex, the 5090: 5.73 GB required, 3.28 GB held, priced at 5.73 GB) |
| `learning::leased`, `resource_admission/metrics.rs` | a third sum of the same rows | the same rule written again |
| `window_grant` + `GrantHandoff` (`continuum-engine/src/capacity/`) | a slot's erase as bytes returned | a parked mind's KV on CUDA lives on host (`UnifiedTier::kv_memory`, native backing), so evicting her frees no VRAM; the handoff receipt proves an erase, never a device release (Codex, 2026-11-26 07:20Z) |
| `training_demand::reservation` -> `working_set::with_training` -> `window_allocator::grow_shared_window` | demand as concurrent jobs | sums mutually exclusive queued seats as if they ran at once; a `TrainingGrowth::Reserved` plan is not a device release |
| ggml's scheduler buffers, grown by `ggml_backend_sched_alloc_graph_admitted` | its own `held` | the only number that is true, and it sits outside every counter above |

Six numbers for one memory, so every refusal needs a trace to explain, and a fix to one counter leaves
the other five wrong. That is the mystery. The cure is the same as for the movers: one owner.

**The compute workspace is a block.** A backend's shared compute buffer (the scheduler's per-buffer-type
allocation that serving's graphs and a learner's reply graph both run in) becomes
`BlockKind::Workspace { device, buffer_type }`, one per backend buffer type, owned by the Streamer like
any block:
- Its size is its `Region`'s size. `held` is never a separate number: it is the block's bytes.
- Growth is a request to the Streamer: `grow(workspace, to: u64) -> Result<Grant, Refusal>`. The
  Streamer prices exactly `to - held` on that block's tier, against that tier's budget, and makes room
  by its law (demoting the coldest blocks *on that tier*) before refusing.
- The scheduler's admission callback (`ReplyAdmission::workspace` today) becomes the one call site of
  `grow`. Within `held` there is nothing to ask. `LearningMemory`'s row matching and lease checks are
  deleted.
- Shrinking is a demotion like any other: when serving's graphs no longer need the high-water mark and
  the tier is under pressure, the law may shrink it, at a point where no graph is in flight
  (`InFlight` covers it).

**Admission is a reservation of blocks, priced where they will live.** A seat asks the Streamer for its
blocks: `Training` blocks (shadow, optimizer state) by kind and tier, and growth of each `Workspace`
it will run in. The Streamer answers with one typed `Reservation { per tier: bytes it will allocate }`
computed from the same block table it places by, or a typed `Refusal` naming the tier, the bytes short
and what it could not demote. Gross versus incremental stops being a question: the price is what the
Streamer will allocate, because it is the allocator's owner. `lease_memory`, `from_rows`, `leased` and
the metrics sum are deleted; Codex's typed incremental contract (2026-11-26 08:01Z) is this
`Reservation`, not a fourth counter.

**Reclaim is demotion, and the Streamer knows what a demotion frees.** Every block carries its tier, so
"evict her to make room for training" becomes "demote blocks on the tier that is short". On the 5090
a parked mind's KV is on `Pinned`, not `Vram`, so demoting it frees no VRAM and the Streamer never
offers it as VRAM. What it can free on `Vram` is listed: other `Workspace` growth past need, `Weights`
or `Expert` blocks the law can stream, a resident mind's `MindKv` on the stream. The answer is
visible in `Placement` before anyone asks. `GrantHandoff` stays for slot custody; it stops being
counted as bytes. `training_demand`'s reservation and `grow_shared_window`'s training path become
`Demand` the Streamer hears, one queued seat at a time (mutually exclusive seats are one demand, not a
sum).

**Waiting, not refusing.** A seat whose reservation cannot be met now is not refused. It is `Demand`
with a need and a forecast, and the Streamer answers `Granted` when its law has made room (a resting
mind demoted, a workspace shrunk, a model's experts streamed). A refusal is only for a reservation
that cannot be met on this node at all (the reply alone past every tier's budget), and it names that.
This is how refusals become impossible rather than rarer: the only "no" is a proven "never here".

**Why it works on every machine, or names the difference.** The M5 (unified) and the 5090 (discrete)
run the same `Reservation` and the same law. On unified memory a demoted resident frees the one tier
everyone shares; on the 5090 it frees `Pinned`, and the Streamer says so. The difference between
nodes is a row in `Placement`, never a mystery found by trace.

**Settled in review (Cormac, Codex, 2026-11-26 08:11Z and 08:19Z), before any code:**
- **One authority, no seventh counter.** The resource authority keeps leasing to everything outside
  the engine (builds, residents' tools). The Streamer holds one authority lease per tier for the
  engine's memory and sub-allocates every block inside it. The authority sees one holder; the Streamer's
  block table is the only picture of what is inside. Pressure from outside, our own builds included,
  arrives as that lease's budget shrinking, and a growing budget wakes waiting seats. Both launch and
  quit of another application are inputs.
- **The engine thread never waits on a demotion. Prepare, consume, commit.** A `Reservation` is one
  atomic claim across every tier. The Streamer prepares and pins it on its own workers, demoting as its
  law decides, and says `Granted` only when the room is free. Inside the graph, the scheduler's callback
  only consumes a prepared claim, answering at once. On allocator success it commits the actual size;
  on allocator failure or cancel the unused claim unwinds. A replacement buffer that overlaps the old
  one reserves its transient peak. With no prepared claim the step drains as `Unchanged`, and the seat
  stays `Demand` until `Granted`. `grow()` is therefore never a call on the engine thread; the callback
  is `consume`.
- **A workspace is identified by its storage, not its name.** `Workspace` blocks key on the allocator's
  context generation and backing allocation. Device and buffer type are descriptors, because two
  schedulers can share a device and type without sharing storage.
- **Bytes count as freed only when they are.** A source's bytes are credited to a tier only after a
  fenced move and the allocator's release. On unified memory pinned RAM and GPU mappings may alias, so
  the tier table is per physical memory. A smaller KV share, or a move from RAM to RAM, never pays for a
  device shortage.
- **"Never here" is a proven limit only.** A seat is refused only for a physical or semantic limit:
  the reply alone past what this seat could reserve on each tier with every demotable block demoted.
  The Streamer answers that query, and it is the `Fit` a lesson is sized within (2ae0feed16 sizes it from
  governed capacity until 3b lands, then from the Streamer). External pressure is always a wait, never
  a refusal.

**None of this is about memory, so it is SAGE's, not the Streamer's** (Joel, 2026-11-26: "Is it well
architected for more general use?"). Reservation, prepare/consume/commit, `Demand` that waits for
`Granted`, the proven "never here" bound, credit only after a fenced release, and a lease per place
from the authority apply to anything finite that is shared. SAGE.md's "Ebb and flow" lists the other
instances: who is awake on a node, which models are hot, who runs where on the grid, the levels of
detail in a live call. SAGE.md designs the `Placer` and `Consumer` for exactly this, but
`continuum-sage` holds no `Placer` yet (grep, 0e54175a38). Built inside the Streamer, the contract
would be memory-only, and each later instance would write it again. So the order is:
- **Slice 3a, the `Placer` in `continuum-sage`.** `Consumer` gains the reservation half:
  `prepare(units, place) -> Claim`, `consume(&Claim)` (answers now, never waits), `commit(actual)`,
  `unwind`. `Demand` and `Granted` are its bus topics, and `ceiling(unit) -> Bound` is the "never here"
  query. Its law is a `decide::Policy` use case, not a new `PlacementLaw` trait, so placement shares
  the decide module's replay, checkpoint and learned-policy lifecycle with every other decision. A
  committed claim keeps `decide::capacity`'s observed-versus-committed meaning, but `Lease` today is
  a rate (work a second), and a byte extent, an awake slot and a service rate differ in dimension and
  lifetime (Cormac, Codex, 2026-11-26). So `Lease` becomes `Lease<Q> { amount: Q, until, price }`,
  generic over a typed quantity (`Bytes`, `Slots`, `Rate`), so one kind can never be spent as
  another; the grid's rate is `Lease<Rate>`. Prepare, commit and unwind take a claim id and its
  owner's generation, so an owner that restarts cannot spend a stale claim, and a prepare across
  several places succeeds as one claim before `Granted`. Codex's host-wide arbitration across
  instances (`codex/placement-under-load`, folded at a92d5fc514) is the `Placer` at node scope,
  serializing prepares across instances; the Streamer supplies backing identity, aliasing, releases
  and native completion.
- **Validated by two outliers, as CLAUDE.md's method asks:** the Streamer's blocks (bytes on tiers,
  fenced DMA) and who is awake on a node (minds on levels from stepping to asleep, no bytes moved by
  the `Placer` itself). If both fit `Consumer` without forcing, the interface holds, and the hot tier,
  the grid and a live call's levels follow.
- **Slice 3b is the Streamer as its first `Consumer`.** It keeps only what is memory: `Workspace`
  storage identity, the links, and what aliases on unified memory.
- **The authority itself becomes the node's `Placer` in time.** Then builds and residents' tools are
  `Consumer`s, and the Streamer's single lease per tier is no longer needed. Until then it is the
  bridge that keeps one counter.

**Audit against the golden rules (Joel, 2026-11-26: "thread and memory based, low cpu iterations and
good object oriented compression into solid generalizations").** `continuum_sage::place` at
f8d0f28655 passes the thread and memory rules: one `&mut` owner and no lock; the consumer spends
from per-place atomics and never waits; typed places, quantities, owners and generations, never a
string; nothing scans data. Prepare allocates its claim once (not per step), and a grant pass is
O(waits × places), a few of each. It fails the compression rule: it was written before searching
for copies, and three exist.

| Copy | Where | What it has that `place` lacks | Its end |
|---|---|---|---|
| `ResourceLeaseLedger` (817 lines) | `continuum-kernel/src/resources/ledger.rs` | per-kind capacity from scans, physical-used samples, `set_backing` (a kind aliased onto another: unified memory), expiry and renew, reclaim selection; string consumer and lease ids | it becomes `place::Ledger<ResourceKind, Bytes>` at node scope. Backing aliasing becomes the per-physical-memory place rule (settled above), physical samples feed `Accounts::budget`, and expiry is a claim's `until` (`Lease<Q>`). Typed `Owner`s replace the strings. Its reclaim selection is the law. Owners: Cormac (the authority, delta 8) with Codex |
| `ThroughputLeaseRegistry` | `continuum-kernel/src/resources/throughput_lease.rs` | concurrency slots for transient jobs | `Ledger<_, Slots>` |
| `Flow` + `Scheduler` | `continuum-sage/src/{flow,scheduler}.rs` | QoS classes and aging (a waiter ages one class at a time), yield requests to the lowest class; but answers through a future (a promise) and has no user outside its own tests | `Scheduler`'s aging is the placer law's answer to multi-place starvation (Cormac, 09:24Z). `Flow`'s future wrapper is deleted. |

So slice 3a is not done until the kernel's ledger and registry are instances of `place::Ledger` and
their copies are deleted. Smaller findings on the same line: `LearningMemory::row`
(`continuum-engine/src/inference/learning/memory.rs:64`) matches a buffer by its name string on
every allocation callback on the engine thread; 3b deletes it, and until then it is bounded by a
seat's few allocations. The lane rebuilds its batch bound as a `NonZeroU64` at each pick: one add,
kept.

What it deletes, in the slice that builds it (slice 3b below): `LearningMemory`'s workspace and lease
matching, `lease_memory`, `from_rows`, `learning::leased`, the metrics sum, training's path through
`window_allocator`, and `GrantHandoff` as bytes.

### Room by priority: memory is a flexbox, only weights are fixed (Joel, 2026-11-26)

Joel: "some are variable and like the rag budget flexbox idea. The memory really needs to make room by
priority. Only some are fixed size."

**What the 5090 showed** (Codex, build 2167 at 4c3e3eb54, 15:41Z, Qwen3.8-27B, MiB): CUDA0 weights 17,402,
KV 8,704 (one stream sized for 262,144 tokens), recurrent 150, compute 3,124; CUDA_Host compute 1,184. A
learner's seat asked for about 12 MB and was short by 11.6 MB, 13 times. The card was full of bytes that are
almost all variable: the stream's KV is sized for a window no mind on that node uses, the compute
workspace for the largest prefill, and the embedder's window is reserved between recalls. None of them
could give a byte back, because every holder was one fixed number.

**Two halves exist and do not know about each other.** `continuum_context::flex` is the flexbox: claims with
a priority, a floor, a min and a max, a floor pass, a min pass and a grow pass. It shares one mind's prompt
tokens, with string ids, and nothing else. `continuum_sage::place` holds claims across places with floors,
waits and fenced releases, but every claim is one fixed amount. ONE-BUDGET.md's contract 3 says they are one
algorithm over any unit, and `cambrian-flex` was never built. So the join is the work:

- **A claim on a place is elastic.** `Claim` gains its shape: `Elastic { floor, want, priority }` in its
  quantity, where `floor` is what it cannot run below (a running turn's KV, the reply's workspace) and
  `want` what it would use. A fixed holding is `floor == want` (weights: `ResidentFloor.fixed` today). The
  board's ceiling becomes capacity less every floor, not every holding.
- **The law shares a place by the flexbox.** When a wait is short on a place, the placer runs the flex
  passes over every elastic holding and the wait, at their priorities: floors first, then mins, then grow.
  A holder whose grant falls below what it holds is sent `Shrink { claim, to }` on its asks state. That is a
  request, never a reclamation (ONE-BUDGET.md, "A shrinking grant is a request"). The wait is granted only
  from bytes released after a fence (`release` partial, Cormac's `shrink`). Priority is the holder's measured
  temperature: a mind on the Core, recall in a turn, a seat's lesson, an idle cache.
- **One solver, generic over the unit.** `flex_allocate` moves into `continuum-sage` (its `decide` law, like
  aging), over `Quantity` with typed owner ids. The prompt's tokens and the node's bytes call the same code,
  and the string `source_id` becomes the caller's typed key. That is ONE-BUDGET's `cambrian-flex`, built where
  the Placer is.
- **Each variable holder names its shrink.** The Core: KV past the occupant's measured window (the stream
  re-sized at a frame boundary, a fork verb this needs), and workspace past the next graph's need (the
  scheduler re-reserved smaller at a point with no graph in flight). The embedder: its window when no recall
  is in flight, taken back by a lease that waits. A seat: its lesson rows (Fit within the ceiling already
  sizes it). A turn's prompt: its own flex, unchanged.

**Never evict, never abort: resize or cache out** (Joel, 2026-11-26: "rather than evict or abort it resizes
by priority or caches out"). A `Shrink` is on one tier, and its holder answers it the cheapest way it has:
*resize* (a workspace re-reserved for a smaller batch, a lesson at fewer rows, a stream sized to its
occupant's window), *cache out* (its bytes demoted a tier: VRAM to pinned to flash, paged back on its next
touch with no recompute; the cached copy becomes its holding on the lower tier), or *collapse* (a cheaper
level: older context summarized behind a handle). Each elastic holding declares its ways with their measured
cost (the quality or speed a resize loses; a move's time and the tier it lands on), the law weighs them with
the priorities, and the way taken and what it cost is the receipt the policy learns from. A refusal stays
only for a proven "never here".

**Holders are objects: one interface, control down and feedback up, prioritized by QoS** (Joel, 2026-11-26:
"you do this with objects. The interface like the lifecycle one, allows feedback and control, both
directions. Point being the safe system, like rage did for the pov user seeing content, prioritized by
QOS, to meet the needs of the system. Later this works well with grid"). RAGE streams the world around
the player's point of view: what is in front of the camera is resident at full detail, what is behind
drops a level or pages out, and nothing the player looks at is ever missing. Here the point of view is the
system's need, in the QoS classes SAGE already schedules by (`Qos`: Interactive, Work, Utility,
Background): the turn someone waits on is in front of the camera, learning and idle caches behind it.

The interface has the shape of `persona::Lifecycle` (a trait with a typed report, composed over parts by
one declaration, every child's report folded into its parent's):

```rust
/// One holder of memory on a placer's places. Up: what it holds, needs and could give. Down: the room it
/// is granted, or asked to give back. A parent is a Resident of its children: its report folds theirs, and a
/// shrink it receives is shared among them by the same flexbox at their QoS.
pub trait Resident<P, Q> {
    /// Up, every place it holds on: held now (measured), floor (cannot run below), want (what its measured
    /// use would take), its QoS and how long it has waited, and its ways to give back with their costs.
    fn report(&self, into: &mut Report<P, Q>);
    /// Down: give back on `place` down to `to`, the cheapest way it has. Pending until its fence lands.
    fn shrink(&mut self, place: P, to: Q) -> impl Future<Output = Given<P, Q>> + Send;
    /// Down: room granted on `place` up to `to`: expand a collapsed level, page a cached copy back in.
    fn grow(&mut self, place: P, to: Q);
}
// a report row: { place, held, want, varies: Fixed { why } | Elastic { floor, ways: [Way] } | Whole { dependents, cost }, qos: Qos, waited }
// Way: Resize { loses } | CacheOut { to: P, takes } | Collapse { loses }  (each cost measured)
// Given: Released { way, freed, took } | Pending | Refused(why)  (Preservation's shape)
```

- *The controller knows what truly varies* (Joel: "We need the controller to know what can truly vary and
  what cannot. It can't be blind. A host of a base model can't just shrink typically"). Every row states
  its variability, typed, from the holder's own knowledge, never inferred by the controller:
  - `Fixed { why }`: it cannot run on less while it serves (a base model's weights under live minds; a
    turn's KV in flight; a writer's window). The controller never asks it, and the ceiling is short of it;
  - `Elastic { floor, ways }`: anything from `floor` up runs, each way to give back with its cost
    (a workspace re-reserved smaller, a stream sized to its occupant, a lesson at fewer rows);
  - `Whole { dependents, cost }`: all or nothing (a base model unloads only when no mind is on it and
    nothing in a higher class needs it; an expert set streams out as a set). Only a wait that outranks every
    dependent may ask it, and the answer is the whole holding moved or cached out, never a fraction.
  A holder that has not reported is `Fixed`: unknown is never room (fail closed, as the ledger does now: a
  holding not marked elastic keeps all it holds). Variability can change with state (the model with no
  mind on it becomes `Whole`), so it is re-reported on every change, like what is held.
- *Priority is QoS, then age.* The law's priority for a row is its class, aged by its wait within the
  class (Cormac's term, measured by `resources.ask.passed`), capped so an Interactive floor is never shrunk
  for anything below it. The same classes order the lane's steps, so memory and compute agree on what
  the system needs.
- *Safe by construction.* A holder never loses memory it did not give: nothing is taken, everything is
  asked and fenced; a holder that cannot give answers `Refused` with why, and the law asks the next.
- *Feedback both ways.* Every `report` is a measurement (held from the allocator, want from use); every
  `shrink` and `grow` answers with what was done, which way, and what it took. The law's replay log is
  those rows, and the policy trains on them.
- *Composes up to the grid.* A part (her KV, her genes) is a Resident; a mind is one over her parts; the
  Core over its minds and its workspace; a node over its engines, embedder and builds; the grid's placer
  over nodes, with a node's report its capacity and wants. One interface at every level: a grid shrink is
  a node moving work or caching it out, answered the same way.

**Everything feeds back** (Joel, 2026-11-26: "everything memory related needs to feed back"). No holder's
`floor`, `want` or `priority` is a constant someone picked; each is the last measurement of what it is, and
every decision writes the receipt the next one reads:
- *what is held*, measured from the allocator after every change (`core.held`; the embedder's lease; a seat's
  rows), never a plan or a catalog estimate;
- *what is used*, so `want` tracks use: a stream's KV against the occupant's measured window, a workspace
  against the largest graph it ran since the last reserve, an embed window against the recalls it served;
- *every grant, shrink, wait and refusal*, as an observation in the `decide` replay log (`resources.ask.passed`
  already counts a wait passed and its age): what was asked, what was short, what was shrunk, how long it
  took to land, and what the work it enabled earned (a lesson learned, a turn's latency);
- *the law learns from those receipts*. The flex passes are the envelope; priority and the shrink choice
  are a `decide::Policy` trained on the replay, the same lifecycle as the lane's switch policy.

**Settled in review (Cormac, 2026-11-26 15:55Z):**
- *One shrink path.* A kernel consumer receives `Shrink` as its existing `ResourceConsumer::reclaim`
  (`ReclaimRequest` -> `ReclaimOutcome`, deferred and partial outcomes included), settled through `shrink`
  and a fenced release. A Placer-native holder reads it from its asks state. Both doors reach one solver.
- *Priority ages.* Temperature alone ranks a learning seat below every serving mind forever (the 5090's
  13 waits). Priority = measured temperature + f(wait age), capped so a running turn's floor is never
  shrunk for a seat. The age term is learned from `resources.ask.passed`.
- *Fit is the seat's share, not the ceiling.* A lesson sized within the ceiling could claim serving's KV,
  and a shrink would then take a mind's window to train. The lesson is sized within what the flex passes
  grant the seat at its own priority, with its whole rows as its want. Cormac moves Fit with step 3.

Order: (1) the solver generic in `continuum-sage`, with `continuum-context`'s callers moved and `flex.rs`
deleted; (2) `Elastic` claims with `Shrink` on the placer; (3) the embedder's window as the first elastic
holder, since it frees memory at once without a fork verb; (4) the Core's workspace, then its KV. Each lands
with a validator on the 5090's split above. The gate is a seat granted on the 5090 while Kimi and Sahar serve.

### Placement while the machine is in use

The resource authority continuously observes the host. For each physical pool it publishes the time of
observation, the OS budget and available bytes, and the engine's attributed footprint where the backend
can measure it. The Streamer joins that observation to its live blocks, outstanding reservations and
transfers. OS available bytes already reflect resident allocations. Subtract only commitments that have
yet to land. Charge aliases once and expose unexplained differences between the allocator and OS readings.
An old observation can guide a forecast. The native allocation and its fence settle the actual result.
Pressure can change between prepare and consume, including when another Continuum instance makes a claim.
Slice 3b gives the resource authority one elected owner per host, held by a host-scoped OS lock.
Codex owns the host reservation endpoint. Other instances prepare through that owner's bus endpoint.
On owner loss, a successor reconciles live allocator receipts and outstanding lease identities before
granting competing claims. A peer offer remains provisional until that peer's authority
prepares it. Release a provisional claim on expiry or cancellation using its reservation identity.

SAGE considers feasible actions against the same history. A policy may keep a warm block, demote it,
place a future allocation on a host-capable backend, defer a seat, or use a compatible peer offer.
Each proposal names the bytes it would free on the short pool, the destination capacity it needs,
the transfer and restore time at recently observed rates, and the effect on the waiting lesson and
active turns. Reusing an existing allocation and leaving the current placement intact are candidates too.
Resource owners declare which backing can move and where computation can still execute. Free disk
can preserve an idle block. A CUDA kernel still needs accessible backing when the block is used again.
An authorized peer lease adds its price, expiry, queue delay and transfer cost to the same decision.
Only the resource owner executes the selected action. The policy records the outcome beside its
prediction so another algorithm can replay the decision using the same typed history.

When outside pressure consumes a prepared reservation's headroom, validate the claim at consumption.
If allocation fails, keep the original backing usable and release unused commitments. Observations
continue while a seat waits, while it runs, and after it finishes. Each change can alter placement.
Only a successful prepare covering the entire seat sends Granted to the trainer. Preserve the
original seat identity and its waiting age. A competing application
quitting, a transfer completing, and our own build ending can all change the preferred placement.
Re-evaluate on those events and as observations age. Include the cost of undoing a move before bringing
a block back. An executable path or an application name is unnecessary for these decisions.

**Acceptance under changing load.** Run the same resident workload through three exact 900-second
windows on each node. Establish the baseline, introduce a competing GPU and host-memory workload,
then release it while the same lesson remains outstanding. Capture observations around both transitions
inside the windows. Record build and artifact identity, request and lesson IDs, per-pool free bytes,
prepared and committed bytes, physical bytes released, destination and move time, serving latency,
and the interval from available capacity to grant. The proposed test command is
`placement-pressure --host-mib 1024 --device-mib 1024 --hold-seconds 900`. Its separate process
touches every host page and repeatedly executes a fixed GPU buffer pass using CUDA or Metal.
Report actual resident bytes, allocation failures, pass latency and release time. On unified memory
charge distinct physical backing once. These sizes belong to the repeatable fixture alone.
Verify that foreground work continues, that two
instances cannot spend the same headroom, and that the lesson produces one durable generation after
capacity returns. Restore and exercise that generation. Replay local GPU pressure with ample disk
space to detect false VRAM credits, and replay unified-memory pressure with aliased host and device
views to detect duplicate credits. Report model-output quality and the competing workload's latency
alongside throughput. These windows measure behavior; the control loop responds throughout each one.

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

### A mind's storage is one object with an adapter per memory, never a branch (2026-11-26, for review)

Joel: "you've got to nip unified vs dedicated memory with comprehensive objects not patches." The 5090's crash
at 2f8bd03a1 (Codex, 18:22Z) came from one branch: `UnifiedTier::kv_memory` mapped each mind's storage as a
file where the device views host memory, and elsewhere reserved her whole stream-sized storage in device memory
for her life. On unified memory a parked mind's file costs nothing (the OS pages it); on a discrete card the
same decision is a lifetime VRAM reservation per mind, outside every ledger, which the Windows driver spilled
into shared system memory until it lost the device. 76f45739f4 put a room check on that branch: a patch. The
branch is the defect.

- **`MindStorage`, the abstract home of a mind's KV.** One contract: `storage_for(StorageAsk { mind, window })`
  answers her storage (a held object whose drop returns its room), and the home is a [`Resident`] whose rows say
  what each mind's storage holds, on which tier, and how it varies. The tier asks it and never branches on the
  device.
- **Adapters, chosen once at start from the node's measured topology** (`continuum-host`, the per-OS files
  pattern): `Unified` (her storage a block over her mapped file; parked, the OS pages it; a cheap Elastic row
  whose way is cache-out to flash), `Dedicated` (her storage ONE device allocation leased on the ledger, sized to her
  window, never the stream; parked, cached out to pinned host memory by DMA overlapped with decode and back on her
  next step; an Elastic row with resize to her window and cache-out to pinned). A context that is not the Core's
  shape is not a third adapter: it is refused at start (row copy is deleted, below).
- **The levels do the work.** Each adapter's rows sit on the `Resizing`/cache-out levels, so an adapter writes
  only how its bytes move (map, lease, DMA); the floor, want, fences and receipts are the parent's.
- **A move is one DMA of one block, never iteration** (Joel: "The transfer itself should be DMA or block not
  iteration or any calculation or it'll be slow"). Today's fallback (`llama-kv-cache-persona.cpp:115`, "she moves
  by row copy") is synchronous `ggml_backend_tensor_get` per layer, into pageable `std::vector` buffers the
  driver stages through, of each layer's whole stream tensor (all its positions, used or not). The Dedicated
  adapter instead holds her storage as ONE device allocation (every layer's K and V for her window, viewed as the
  layers' tensors) and her parked copy as ONE pinned host block of the same layout: park is one device-to-host
  async copy, restore one host-to-device, each fenced by its own event (the Streamer's `Landed`). No per-layer
  calls, no staging, no `state_seq_get_data` (a serializer) on this path, only her window's bytes. Row copy is
  deleted, not kept as a fallback: a mind with no device room stays parked in pinned memory until the placer makes
  room.
- **Deleted with it:** the viewable branch in `unified_tier.rs` and `DeviceRoom` (its lease becomes the
  `Dedicated` adapter's).

Gate: the 5090 with Kimi and Sahar resumed holds each parked mind on pinned host memory (shared GPU usage flat at
the control's ~1.2 GB), the bound mind's storage leased on the board, and no driver loss; the M5 unchanged.

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
| 3a | SAGE's `Placer` in `continuum-sage`: `Consumer` with prepare/consume/commit/unwind, `Demand`/`Granted`, `ceiling`, its law as a `decide::Policy` use case, committed claims as `decide::capacity::Lease<Q>` over typed quantities, claims keyed by id and owner generation | Fable (Placer), Codex (node-scope arbitration on it) | two outlier consumers fit without forcing: a block consumer and the awake/asleep minds; sim replays a captured night's demand with zero waits that never clear |
| 3b | Workspace, admission and reclaim as blocks ("Every byte the engine holds is a block"): `Workspace` blocks and `grow` at the scheduler's admission callback; `Reservation` replaces every learning lease sum; training demand as `Demand`. Deletes the six counters listed there | Fable (blocks, Streamer), Codex (`Reservation` contract, the discrete tiers' reclaim), Cormac (the trainer as a consumer, outstanding demand) | the M5 and the 5090 each run 15 min on one head: zero refusals that are not "never here"; every wait names the tier and bytes it waits for; forward progress (a `Saved` generation linked to its lesson, serving restored after) on both nodes; time from the short tier freeing to `Granted`, p50 and max; fixtures for pressure onset and recovery, cancel, and a native allocation failure after a reservation |
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

## A long reply learns in tiles, never as one graph (2026-11-26, for review before code)

**What the 5090 showed** (Codex, 46cfb47915, 16:21-16:36Z): Kimi's lessons measured 64.4 GB, 91.5 GB and
1.9 TB on a 34.2 GB card, and a seat asked for 26.5 GB against a 13.1 GB ceiling. Cormac's reading, now
confirmed in the fork: `llama-context-learn.cpp` builds "one graph for the entire reply" (when the reply
has graph nodes, a step takes `end - cursor` rows and widens the batch to it), and `opt_measure_memory`
sizes that graph at `reply_rows`. A reply's backward holds every reply row's activations against every
lesson row it attends, so a long code reply cannot learn on any card. Collapsing the context cannot help:
the reply itself is what is trained and is never cut.

**The fix (Joel: stream in tiles, never shrink).** The reply learns in consecutive tiles of `tile` rows.
Each tile is one graph: its rows' forward and backward, attending the lesson before it from the cache as a
constant, its gradient accumulated into the shadow's accumulators; the optimizer commits once after the
last tile, with the label denominator of the whole reply, so the example and its loss are exactly the
reply's. Every label is trained; none is cut. The fork already walks chunks this way (`Walk`, `exact`,
`horizon`); the whole-reply graph replaced it for speed on short replies, and the tile restores it with the
tile sized by memory, not by the batch.

- **The tile is the seat's elastic dimension.** `measure` reports per tile rows: a seat's floor is one tile
  of the smallest size that learns (one ubatch), its want the whole reply as one tile. Its Resident row is
  `Elastic { floor, ways: Resize(tile) }`: a shrink asks for smaller tiles, a grow allows larger. The
  flexbox share at Background decides the tile, so the seat always fits the room it is given.
- **What it costs is measured, not guessed.** Smaller tiles add one forward per tile over the cached lesson
  (no re-prefill: the lesson's KV stays in place) and more kernel launches. Each Learned row says its tile and
  milliseconds per labelled token, the cost the policy weighs against waiting for room.
- **Exactness: the exact walk, never detached tiles** (Codex, 16:46Z, source review of 45e05f176). The
  whole-reply graph exists for correctness, not only speed: a later reply token's loss flows back through the
  earlier reply tokens' K/V and recurrent state, and independent tiles with detached caches lose that, even
  with one denominator and one update. The fork's exact walk (`llama-context-opt.cpp`, the xc path below the
  ordinary reply at :638) carries gradients into earlier cached K/V and recurrent state across chunks. The
  tiles reuse it across the reply, stopping at the served-prefix boundary. In an OS's terms this is paging
  with write-back, never dropping a dirty page; in RAGE's, the page cache streams what the frame needs and
  keeps every page the frame still depends on.

**How the engines we borrow from solve it** (Joel, 2026-11-26: "Look at how operating systems and gaming
engines work, particularly rage... Always streams, contracts, trees, caches"). An OS never needs a process's
whole address space resident: only its working set, demand paged, with dirty pages written back. RAGE never
needs the world in VRAM: fixed-size pages, a feedback pass naming what the view needs, a page cache filled by
priority, a lower mip for a frame rather than a hole. A lesson is the world, the tile is the page, the seat's
flexbox share is the page cache's size, and the exact walk is the write-back that keeps every gradient.

**The join: the exact walk made preemptible, one tile per call** (read from the fork, 2026-11-26). Today the
exact walk (`opt_walk::run_exact`) runs a whole window in one synchronous call over sequence 0 from position
0: a forward pass over every chunk under the shadow with recurrent checkpoints, then a reverse pass training
each chunk with its K/V gradient accumulated for the chunks before it. It can be stopped (`yield_point`) but
not resumed. The live reply (`llama-context-learn.cpp`) is resumable, one unit per call, so serving decodes
between, on a live serving sequence from its served prefix. The join is the walk as a state machine on the
live reply, the way an OS makes a long job preemptible at page boundaries:

- *Tiles.* The reply `[prefix, prefix + n_train)` is cut into tiles of `tile` rows (a whole number of
  ubatches), the tile given by the caller from the seat's share.
- *Forward, one tile per call.* Checkpoint the recurrent state at the tile's start (every `stride` tiles,
  the walk's host budget rule), decode the tile under the shadow, forward only.
- *Reverse, one tile per call, last to first.* Rewind to the tile's checkpoint, train it with the walk's
  graph flags (`grad_from` never below the served prefix: no gradient enters what was served), accumulate
  its K/V and state gradient for the tiles before it. The first tile's step ends the period: one optimizer
  update with the whole reply's denominator.
- *Restore.* The existing restoration: back to the served-prefix snapshot, the restore tokens decoded under
  her stable set, so serving finds her exactly as it left her.
- *Memory, as Residents.* Device: one tile's graph (the seat's elastic workspace). Host: the walk's K/V
  gradient accumulator, which grows with the reply (layers x reply x (k + v)); it is the seat's host row,
  sized by measurement and placed like any other, never assumed.
- *Preemption.* Every call is one tile, so a turn waiting is never more than one tile behind; a drain between
  calls takes the existing path (accumulated gradients cleared, moments and completed updates kept).

Order: (1) the fork's learn step walks the reply in tiles through the exact walk, the tile given by its
caller; `measure` sizes a tile, not the reply; (2) the driver picks the tile from the seat's share and
reports it; (3) the seat as a Resident with the tile as its resize. Gates: the live hybrid path's adapter
update matches the full graph's by cosine and norm (`tests/test-walk-exact.cpp`, extended: a label only in
the last tile, uneven labels, a partial final tile), one optimizer update, the restored serving state
unchanged; then Kimi's 64 GB, 91 GB and 1.9 TB lessons learned on the 5090 beside serving, held-out loss kept.

### Measuring a tile, not the reply (2026-11-26, for review before code)

**What is measured today** (Cormac and Codex, 22:14-22:16Z, at pin 32e155036):
- `measure_within` (`driver/learning.rs:277`) calls `opt_measure_memory` (`llama-context-opt-measure.cpp`) with `chunk = reply_rows` and `walk_active = false`.
- That is the whole-reply graph, which is honest for the path that runs today, and is Kimi's 1.9 TB.
- The fork's tile walk (`llama-context-learn-tiles.cpp`, fork head 17704d6a7, gate passing at 64- and 128-row tiles) has no measure of its own.

**The measure is the walk's worst tile, built as the walk builds it.**
- `llama_opt_measure_shadow_tile(ctx, params, tile, reply_rows, lesson_rows, horizon, callback)` builds one reverse-pass training graph and allocates nothing.
- It builds the graph the tile walk runs at its most expensive tile: the reply's last tile, at `n_past = lesson_rows - tile`.
- Its flags are the ones `opt_tiles_unit` sets: `walk_active` and `walk_exact`, `grad_from = max(prefix, n_past - horizon)`, and the K/V surrogate. The recurrent state surrogate is added when the memory has a recurrent part.
- The last tile is the worst on both terms that grow:
  - its attention window `n_past + tile` is the largest;
  - its tap span `n_past - grad_from` (the F32 cast of the cached rows that carry gradient) is the longest at the full horizon.
- Every other tile's graph is no larger, so this one bound covers the whole walk.
- The measure reports two rows, never one sum:
  - **Device:** that graph's workspace, by the same `ggml_opt_prepare_alloc_slices` pass `opt_measure_memory` uses.
  - **Host:** the walk's K/V gradient accumulator and its recurrent checkpoints. It uses the walk's own formulas (`llama_walk_acc::kv_bytes`, `state_bytes`, `plan.stride_for`, `plan.host_bytes`) at the walk's host budget, so the measure and the run cannot disagree.
- `opt_measure_memory` and the tile measure share one body. The tile measure is the same pass with the walk's flags and a positioned ubatch; it is not a second allocator walk.

**The accumulator in a graph that allocates nothing.**
- The graph reads `cparams.walk_acc_k/v[il]`, the host accumulator's tensors.
- The measure binds metadata tensors of the run's shape that sit in a host buffer. A leaf with no buffer would be scheduled into the compute buffer and overprice the device by the accumulator's size.
- Question 1 below is how to give those tensors a host buffer without allocating it.

**The driver.**
- `Walk` gains `tile: NonZeroU32` and becomes `exact = true` whenever tiles are used. Tiles are exact or not used: `opt_learn_tile` refuses otherwise.
- `measure_within` stops shrinking the lesson. The lesson's K/V are read from her cache as constants, so the lesson no longer prices the device beyond one window of attention.
- It shrinks the tile instead: halving from `min(reply, one tile past which Fit overshoots)` down to one ubatch, the floor.
- `Loaded` reports the tile beside `reply_rows` and `lesson_rows`. The Learn step calls `llama_opt_learn_tile(tile)` after begin.
- The seat's Resident row is `Elastic { floor: one ubatch tile, ways: Resize(tile) }`, as above.
- **A shrink resizes before it releases** (Cormac, 22:34Z).
  - A shrink whose target is at or above the floor tile's workspace is answered by a resize: she learns at the smaller tile from her next lesson boundary, and her lease shrinks to it. The holder answers `Partial` once the resize lands.
  - Only a target below the floor finishes, checkpoints and releases.
  - This needs the tile measure on a seated shadow without reloading it: a `Measure { tile }` verb beside Load. Cormac builds the trainer half (`Elastic.floor` = the floor tile's bytes).
- **The walk's host budget is the seat's Ram lease, never a constant** (Cormac). The host row is leased as Ram by `lease_memory_within`, and the walk reads its budget (`opt_walk_host_budget`, which sets the plan's stride) from that lease. The device workspace reads its admission view the same way, so the stride and the lease cannot drift.

**Never a refusal, never another path** (Joel, 2026-11-26: "Don't just abort, or give up, you optimize to fit without abandoning or defaulting").
- **A tile that does not fit at its full horizon splits in place.** Today `opt_tiles_unit` fails the step with "learn this reply at a smaller tile". Instead:
  - its rows are already decoded and its recurrent checkpoint is held, so its reverse trains it as sub-tiles of half its rows, last to first;
  - each sub-tile rewinds to the tile's checkpoint and decodes forward to its start, the walk's own rewind;
  - each sub-tile's gradient is accumulated into the rows before it, as the walk already does between tiles.
  - The gradient is the same exact step, and only the graph is smaller. Halving repeats down to one ubatch. The measure's floor is one ubatch tile, so admission never seats a tile that cannot reach it.
- **A shared cache is never sent to the whole-reply graph.** Today `opt_learn_tile` refuses `kv_unified && n_seq_max > 1` and the reply learns by its ordinary path (e4d811bf9). That ordinary path is the one that measured 1.9 TB. Instead:
  - the Core's layout, a stream per mind, is made the only one by construction, checked where the plan is chosen (Codex, 22:36Z);
  - so the layout the walk cannot read never reaches it, and the refusal and its fallback are both deleted.

### Learning is a thread on the Core: time divides as space does (2026-11-26, for review before code)

Joel: "Yeah or time divided. We never need to abort. If it doesn't fit now, it can in some duty cycle. The goal is everything fed back like sage does graphics. Learning for example is truly pwm like. It can all fit. You just need to have these levers and contracts under centralized intelligent control." Then: "It's how threads work on one core or many really. They're all given their time. This is an rtos."

**The scheduler already exists; learning becomes one of its threads.**
- The lane (THE-LANE-ON-SAGE.md) is the RTOS: one Core, one thread on it at a time, preempted at every step under the wait bound, chosen by its `decide` use case with history.
- A mind is a thread today. A learning seat becomes one more, with no mechanism of its own:
  - **Runnable** while it owes a lesson (its to-learn queue is not empty).
  - **Priority**: Background, with aging, so it is never starved. The learning cadence's guarantee is the queue kept in check, never a fixed rate.
  - **Time slice**: one tile, one unit of the walk (a forward tile or a reverse tile). A slice is never longer than the wait bound allows, because the tile is sized to it.
  - **Context**: what it keeps between slices, the walk's checkpoints, its host K/V-gradient accumulator, the reply's cursor and the optimizer. This is its memory as a resident, paged by the block layer like a mind's KV.
  - **Scratch**: the tile's graph workspace, held only while its slice runs, like a stack frame in use. The placer grants it for the step and takes it back when the step lands.
- **The lane's types need nothing new** (answering Cormac, 22:55Z). The Slicer is generic over its thread key `K`, and the engine's key becomes `enum Thread { Mind(PersonaId), Learner(SeatId) }`:
  - `Ready(Learner(seat))` and `Done` come from the trainer's typed `learning/owed` (pending examples, rows and oldest age per bucket, said on change). The lane never reads the journal.
  - `Need { urgency: Background }` with aging is its priority.
  - `Next::Switch(Learner(seat))` then `Continue` runs one tile per step, where a mind's step decodes. The driver calls the fork's one walk unit.
  - `Stepped(Learner(seat), took)` is the tile's measured time, which the slicer learns as that thread's step.
  - `Fetched` is its context paged back in.
- **Off gives back only the scratch** (Cormac). The shadow and its optimizer state (Kimi's: 36,176,000 B) are the thread's context and stay seated. They are cached out to pinned only when the lane parks the thread for long, priced like any park.
- **The ledger acts per period, never per step** (Cormac). The lane holds one elastic Vram lease for learning scratch, sized to the on-tile, and resizes it only when the tile or the share changes. A decode step is milliseconds, and an ask, grant and release through the authority's lock on every edge would cost more than it saves. Every grant, shrink and change is still a `decide` observation, recorded per period.
- **Priority is a share the central policy sets, and a threshold only where the work has one** (Joel: "Usually thread priority is exactly this. I get 10% or 50 etc. not hard coded just saying priority. In an rtos some things truly have thresholds like for instance a frame draw in a game engine. Others might have less of this or none more like cars at a traffic light. Regardless like in cbar or rockstars rage we can find time for everyone. You just must have more centralized logic and everyone comply").
  - **A threshold class** (`Want`, `Urgent`: a resident inside her act bound; a live call's frame) has a hard bound the scheduler never lets pass.
  - **A share class** (`Content`: learning, dreaming) has no per-item bound. Its guarantee is its queue kept in check, like cars at a light: each gets its turn, and none waits forever.
  - **The shares are the policy's output, not constants.** Today `slice::weight` fixes them at 1, 2 and 4 per urgency. They become the lane's decision per class, from each class's feedback: slack against the bound for a threshold class, the queue's growth for a share class. This is the decide framework's job, recorded per period.
- **Everyone complies: every slice is bounded and declared** (Codex's review at c6d40d7917).
  - A thread's unit may never run longer than the slack the scheduler grants it.
  - Two of the tile walk's units are not bounded by the tile: the whole prefix decoded before a run, and the reverse replay from a stride checkpoint before `train_chunk`. Each becomes a sequence of tile-sized units with a yield between, resumable at the existing chunk boundaries.
  - The commit is priced the same way. Each unit's cost, transfers on its critical path included, is measured, and the lane schedules it against the residents' remaining slack.
  - Codex's gates: a long unlabelled prefix, and a reverse stride above 1, each with a resident arriving just after dispatch. The timing receipt shows the actual overlap beside the isolated 236 ms pinned H2D.
- **One objective: time. Rejection is never an action** (Joel: "This is inherently how any resource works like timeshare vacation homes... it's another dimension. Its cost is again transfer time. Or time is our overall optimization here. We just know rejection is the costly one").
  - Every resource is shared in space and in time, and every way of sharing it is priced in time: the units a smaller tile adds, the transfer that swaps an occupant (the context switch, as a mind's KV moving), the wait.
  - `Ways`' `Cost::takes` is that price, measured.
  - Rejection discards the work done so far and everything spent to come back to it, so no controller has it among its actions. Only a broken contract is typed, and it is made impossible where the plan is chosen.
  - **The measure is power: useful work over time** (Joel: "Wattage work over time"). The controllers maximize the useful work the node delivers per second, each class's work under its own contract. A transfer is time spent moving, not working. A rejected unit is time spent for no work at all.
  - **Useful work is what lasts, never units advanced** (Codex, 23:06Z). Learning's work is durable optimizer updates and checkpoints saved, not tiles stepped. Serving's is resident turns completed within their bound. A controller rewarded per unit could raise its rate just by cutting the same lesson into more tiles. Each window's receipt reports tiles advanced apart from updates and checkpoints, beside resident turns and their waits against the bound.
- **`learning/owed` is state, not events** (Cormac, 22:59Z): `Owed(BTreeMap<SeatId, LearnerOwed>)` on one key, latest-wins, said on change. A seat leaving the map is its Done, so a dropped or coalesced say can never leave the lane holding a Ready it never sees retired.
  - `LearnerOwed { seat, mind, examples, next: Option<Rows { context, reply }>, since }`.
  - `next` is the lesson the engine sized in `Loaded` (`context = lesson_rows - reply_rows`), and is `None` until measured. Token counting has one owner, and the trainer never renders.
- The share of the Core that learning gets (Joel's PWM width) is nobody's setting. It is what the scheduler's choices add up to: wider while the queue grows and serving has slack, narrower under serving's demand.

**Space and time, under the same two controllers.**
- **The lane** decides when each thread runs (time).
- **The placer** shares each place's room among what runs and what stays resident (space).
- Each holder declares its levers and its contract in its `Report` rows: floor, QoS, the ways it gives back, what it requires beside it. Feedback comes up every period, and no holder decides alone that it cannot run.
- A tile too large for the room left at its slice splits in place (above). A queue the scheduler is not draining is its signal to give learning more slices, never a refusal.

**What this deletes.**
- Every refusal path in learning that means "not now": `measure_defect` for a lesson past the card, a seat `Refused` for no room, a tile that fails its step, and the shrink that finishes and releases.
- The learner's separate admission waits become the scheduler's runnable queue.
- A typed refusal remains only for a broken contract: a layout the walk cannot read, made impossible at plan selection.

**Gates.**
- The 5090's gate 5 below: zero `measure_defect`, every learning wait naming its tile.
- Plus: over a window with serving loaded, the learner's queue stays in check while no resident's wait passes its bound. Each period records the slices learning was given, and why.

**Gates, in order.**
1. The fork's `test-walk-exact`: the tile measure is at least the workspace each tile actually admits (`workspace_admission`'s bytes per tile). Run at 64, 128 and 512 rows, f16 and q8_0, flash on.
2. The same on a hybrid model: the 5090 serves Qwen3.8-27B, and the hybrid path has an open `k_idxs` abort.
3. The open device-refusal case: a refused tile names itself and the seat learns at a smaller tile, never an abort.
4. Kimi's 64 GB, 91 GB and 1.9 TB lessons measured, admitted and learned on the 5090 beside serving.
5. On the 5090 over a window, zero `measure_defect` (four today) and every learning wait naming the tile it waits for (Cormac). That proves the whole-reply path is gone from the live driver, not only from the fork's test.

**Questions for Codex.**
1. **The accumulator's tensors in a host buffer that is never allocated.** One option is a CPU buffer from `ggml_backend_cpu_buffer_from_ptr` over a pointer that is never read. Is that sound under ggml's allocator? Or should `walk_acc_rows` take a graph input that the measure marks host-resident?
2. **The Core's layout.** `opt_learn_tile` refuses `kv_unified && n_seq_max > 1`. Is every live serving context on the 5090 a stream per mind? If one shares a cache, the tile path is refused there and the reply would learn by the whole-reply graph again.

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
