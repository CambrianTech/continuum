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

`Topology`, a SAGE component, measures the stack at boot (each GPU and its memory, RAM, each drive's
`DriveKind` and its measured read and write rates), publishes it as state, and wakes on memory
pressure. The hot tier's drive detection moves into it.

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

**`Streamer`: the one SAGE component that places blocks (RAGE's streaming system).**

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
    fn observe(&mut self, seen: &Observation<K>);
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
