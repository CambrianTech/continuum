# Shared Resources: one core, resource-kind adapters, domain bases

Card 13c81ced. Joel, 2026-11-10: "Shared residency on one machine is difficult but necessary."
"It's like a resource like any other, just need to do it elegantly, not hacked patches."
"You don't hard code for rust or anything."

## Why this exists

Several citizens share one machine: agents, residents, and humans. They also share its memory, its
cores, its devices, its files and the work itself. On the M5 on 2026-11-10, five problems of that
kind each cost an afternoon:

- **A resident's test run starved serving.** Her full-workspace test run ran at the engine's
  priority beside serving. Load reached 36, swap 4.5 GB, and decode fell from 56 to 3.4 tokens/s.
- **A shared build cache broke an agent's check.** Agents and residents build into one cache
  directory. A resident's build put another tree's crates into it, and an agent's type check
  failed on a module that exists.
- **A mind deadlocked on her own grant.** Aris waited, inside her own turn, for a reclaim of her
  grant that needed that turn to end.
- **The memory board blocked a tiny learning step.** The board counted shared memory twice, so a
  4 MB learning step waited on "0 available" while 3 GB was free.
- **The embedder's fixed lease failed most embeddings.** It leased a constant 1,792 MiB, and 91% of
  embeddings failed with 1.5 GB free.

Each problem was patched where it surfaced. The code holds about nine lease mechanisms, each with
its own holders, queues and expiry. None of them is visible to the other citizens, and two are
written for one toolchain. This design gives them one owner.

## The principle: resources flow, like data

Joel, 2026-11-10: "Flow, like our data flow but for all the resources. Don't just occupy. Go in, do
your work, go out." Every consumer of every resource acquires for one operation and releases at
its end: an act's seat, an embedding's context, a learning step's workspace, a page promoted to
the device, a cache entry's RAM. Nothing holds a standing reservation "just in case". A reservation
that outlives its work is occupation, and occupation is the defect this design exists to remove.
Each machine's capacity is then a measured throughput of operations per second, not a sum of
parked claims. Where a consumer needs a guarantee (recall must always be able to embed), it is a
priority on its next turn in the flow, never bytes held between turns.

## Every decision is a price

Joel, 2026-11-10: "It's all about cost. Some stuff loads so fast, especially if cached properly,
it is trivial time, especially compared to massive queue locks." Hold or release, keep or demote,
wait or proceed: each is decided by comparing measured costs, never by habit or by a fear of
reloading.
- **The cost of releasing** is the measured time to bring the thing back from where it goes:
  milliseconds for a context rebuilt or a page promoted from RAM, more from flash, and the
  prefill time when it must be recomputed.
- **The cost of holding** is what every other consumer waits behind it. On 2026-11-10 that was
  ten minutes for two residents queued behind idle seats, and an hour for all four behind a
  stuck growth hold.
- Release wins whenever bringing it back costs less than the expected wait it causes, which,
  with pages cached in a fast tier, is nearly always. Fast tiers make releasing cheap, and that
  is what lets everything flow.

## What the users are

Continuum's users mostly work in Python or Node, usually on web projects. Few work in Rust. Much
of the work is not code at all: documents, spreadsheets, media, CAD, research, and calls. Nothing
in the core or in a resource-kind adapter may name a language, a toolchain or a kind of work.

## The design: three layers, each holding only its own difference

### 1. The core

The core knows nothing about the work. It has six parts:

- **Resource:** a typed id and its kind.
- **Holder:** any citizen (an agent, a resident or a human) and a handle to her.
- **Mode:** shared (many readers), exclusive (one writer), or a quantity (bytes, slots, a share
  of CPU).
- **Outcome:**
  - granted;
  - queued, with a handle that wakes the holder when the resource is granted;
  - deferred, with its typed reason.

  A request is never refused silently. A wait never runs inside the holder's own turn, or her
  turn would wait on itself.
- **Expiry:** a time limit renewed by heartbeat, like a kanban claim. A dead holder's resources
  are released.
- **Visibility:** holders and queues are published as an airc view, like the kanban. Every citizen
  on every machine sees who holds what, and can queue, defer or negotiate. One machine and the
  grid use the same view.

### 2. Resource-kind adapters

Each kind of resource answers five questions: how much exists, how much is in use, how to acquire
it, how to release it, and how to tell that a holder is dead. Nothing above this layer knows how
any kind is measured.

| Adapter | Measures and acts on |
|---|---|
| Memory tiers | Device, host and flash memory. Each tier has a measured capacity and a measured cost to read back. A lease is charged to the tier its bytes occupy. |
| Files and directories | Documents, a media library, a tool's cache, a worktree. Reads are shared and writes are exclusive per file or region. |
| Devices | The display, the camera, the microphone, the GPU. |
| Processes | A process's share of CPU, its scheduling class and its lifetime. |
| Network endpoints | Ports, a share of bandwidth, an external API's rate limit or quota. |
| Seats | An account, or an app session only one holder can drive at a time. |

The differences between platforms and devices are adapters too. The memory adapter takes its
topology from the backend's own report:

- **Metal** reports whether memory is unified (`hasUnifiedMemory`).
- **NVIDIA** reports its card's dedicated framebuffer.
- **Every other backend** must report its own, as AMD APUs, ARM integrated GPUs and Vulkan will.

When the mode is unknown, it stays explicit: VRAM is left ungoverned and the gap is named at
every boot. The OS-specific code lives in `continuum-host`, where `mod.rs` holds the logic once and
each OS adapter holds only its calls.

### 3. Domain bases

A domain base holds once what a kind of work shares, built on the resource-kind adapters. Each
tool adapter holds only its difference from its base.

| Domain base | What it holds once | Tool adapters (each only its difference) |
|---|---|---|
| Code | A worktree, the tool's declared caches, a build or test run as a background process, results read back | Python (venv, pip/uv), Node (node_modules, npm/pnpm), Go, Rust |
| Web | A dev server as a held process, its port, a file watcher, a browser session using the display and GPU | vite, next, django/flask, Playwright |
| CAD / 3D | Large binary project files (a whole-file writer lock, no merging), heavy GPU and memory use, licence seats | Blender, FreeCAD, Fusion |
| Media | Large source files, long renders as background jobs, scratch disk | ffmpeg, DaVinci, audio tools |
| Documents | Shared reading, per-region or per-cell writing alongside others (a human co-editing), versions | Text, spreadsheets, office files |
| Calls and live | Camera, microphone, display, a real-time share of CPU and network | Our live calls, Zoom/Meet |

A new tool is its difference from the nearest base. A new domain is a new base on the same
resource-kind adapters. The core never changes for either.

## Residency over time: resident, transient, pageable

Joel, 2026-11-10: "Are there ways to stage some things in RAM, then into VRAM, only briefly for the
operation?" A memory lease also carries how long its bytes must stay resident, as a graphics engine
treats a frame's memory:

- **Resident:** must stay where it is while its holder runs. The base model's weights, and a mind's
  KV while she acts.
- **Transient:** needed only for one bounded operation of a second or less, never at the same time
  as other transient work in the same arena. Examples are an embedding context, a vision encoder
  pass, a learning step's gradients and a draft model's buffers. Transient leases alias one arena
  that is reserved once and reused one after another, like a frame graph's transient attachments.
  The arena is sized to the largest transient need, not to the sum of every consumer's standing
  floor. Today each side consumer holds its own floor (the embedder alone held 1,792 MiB), so the
  serving plan budgets around their sum.
- **Pageable:** may leave the fast tier between uses and come back: KV pages to RAM or disk,
  LoRA adapters, MoE experts, a small model's weights. Each page has a measured cost to read back,
  and the placement decides by value (how soon it is needed against what it costs to bring back).

The memory adapter differs by topology:

- **Dedicated VRAM (CUDA):** a transient operation stages its data in pinned host memory, uploads
  it, runs, and frees the device memory. PCIe moves about 25 GB/s, so 600 MB takes about 25 ms,
  well inside a sub-second operation.
- **Unified memory (Apple, AMD APUs):** nothing is copied. Transience matters for the working set
  and for wired pages, so the arena time-shares one region instead of holding several floors.

The core schedules transient operations into the gaps between the engine's decode steps, the same
slot learning in place uses (training shaped like inference). Where it can, the core then reports
the arena and the pages to the serving plan as reclaimable memory rather than permanently taken.

## Built on what exists: the pipeline flows both ways and carries more

Joel, 2026-11-10: "We already had this graphics pipeline. Just need it to flow both ways and for more
things." Nothing here is a new engine. Three existing pieces become the memory-tier adapter:

- **The paging engine:** `PagedResourcePool` (continuum-kernel `paging`, UNIFIED-PAGING.md). One
  primitive with single-flight loads, pins, eviction by priority, and stats for the pressure
  broker. It was designed for LoRA weights, KV pages, MoE experts, model weights, embedding vectors
  and recall results. In production only KV slots construct it.
- **The frame pipeline:** the engine as a cache, run like bgfx (ONE-ACT 5a and 5e). The core's
  encoders record each mind's work as it arrives. The engine runs double-buffered frames ordered by
  a sort key: warm decode, then temperature, then bounded cold-prefill chunks, then
  prefetch/write-back. Handles are released at the frame fence.
- **Grants as leases:** the window grant owner (`window_grant`, `yield_after`, handoff with
  terminal and physical custody receipts).

What changes:

| Direction | Today | Flowing both ways |
|---|---|---|
| A page under pressure | Dropped (evicted), then re-prefilled at minutes per act | Demoted one tier (device to host to flash) at its measured transfer cost, and promoted back by predicted need (warm-ahead) |
| Engine capacity | Sized at launch; grows only by relaunch, once per run; never gives memory back | `reconfigure` grows or shrinks live at a frame boundary (SERVING-ENGINE S2); a user's game or editor gets its memory back, and gets it again when it quits |
| Seats | Change hands only when an evaluation catches an idle holder | A lease per act, re-decided each tick by temperature and aging (S3, #5914) |
| Learning | A separate admission waiting on a multi-GB lease | Forward and backward share the frames: learning chunks run in the gaps between decode steps (fork #102), on the transient arena |

And what it carries. Each concern becomes an implementer of the same pool, holding only its key,
its loader and its tier costs:

| Concern | Residency | Today | As an implementer |
|---|---|---|---|
| KV pages (per mind) | Pageable | Uses the pool; evicts by drop | Demote and promote across tiers |
| LoRA adapters (genome) | Pageable | Its own registry | Pool implementer, with a flash tier |
| MoE experts | Pageable | Its own host cache lease | Pool implementer |
| Small-model weights (embedder, mmproj) | Pageable | Always resident | Promoted to the device for an operation, demoted after |
| Embedding context, vision pass, draft buffers | Transient | Each holds its own floor or allocation | One transient arena, used one after another between decodes |
| Learning gradients | Transient | A waiting lease of GBs | Chunks in the transient arena between decode steps |
| Recall caches, embedding vectors | Pageable (host and flash) | Hand-rolled caches | Pool implementer |

Build order (each slice proven by its validator first, then a live read):

1. **S2, `reconfigure`** in the fork (SERVING-ENGINE §5): the model never reloads, and grow and
   shrink are one path. This ends relaunches, the largest live harm (13 in four minutes on
   2026-11-10).
2. **Tiered demotion in `PagedResourcePool`:** evict means demote one tier, with the KV slots
   first, since they already implement the pool.
3. **The transient arena:** the embedder first. It is already per-request sized and charged per
   pool (#5910), so it moves from a standing floor to arena turns.
4. **Learning chunks in the frames:** fork #102 plus the arena.
5. **S3 priority and aging,** with #5914 as its first policy.
6. **LoRA, experts and small-model weights** migrate as pool implementers; each old copy is deleted.

## Cache tier levels

Joel, 2026-11-10: "It's gotta be a sophisticated cache tier levels." A cache hierarchy with
measured costs, not an evict-or-keep switch. It builds on vocabulary that already exists:
`TierRole` (continuum-genome `tier.rs`, GENOME-FOUNDRY-SENTINEL Part 2) names tiers by role, never
by ordinal, so the tier count follows the machine. `TierRole` moves down into the kernel's paging
engine as the one tier vocabulary every concern uses. Recall scoring keeps reading it from there.

### The roles

| Role | Holds | Dedicated VRAM (CUDA) | Unified memory (Apple, AMD APU) |
|---|---|---|---|
| Fast | Bytes the accelerator reads at full bandwidth | VRAM | The hot part of unified memory |
| Warm | Bytes one copy away from the accelerator | Pinned host RAM (PCIe) | Absent: the same pool as Fast, so the type has no Warm to evict into |
| Bench | Bytes the host reads at memory speed, possibly compressed | Host RAM | Host RAM (compressed pages) |
| Cold | Local flash | NVMe or SSD | NVMe or SSD |
| Peer | Another node's Bench or Cold, over the grid | Grid peer | Grid peer |
| Frozen | Archive, append-only with provenance | Archive | Archive |

Below every tier is **recompute**: a miss re-derives the page. For KV that means re-prefilling at
the measured prefill rate. Recompute is the cost every eviction is priced against.

Each node's governor builds its `Vec<TierConfig>` from measurement, never from a machine kind:
the backend's reported memory mode, each tier's capacity, and each tier's live read and write
bandwidth and latency, sampled from real transfers. A Mac has no Warm. An AMD APU reports shared
memory and gets the same shape. A 5090 box has all of them.

### What a page carries

- **A key** (typed, never a string) and **the concern** that owns it.
- **A level of detail per tier,** like mipmaps: a page can be held more compactly farther down.
  KV runs at the serving cache type in Fast, can be quantized further or compressed in Bench, and
  compressed on Cold. A LoRA adapter is f16 in Fast and compressed on Cold. Every level of detail
  is measured: bytes, encode and decode milliseconds, and the quality it costs where it is lossy
  (only by a gene's or a lesson's evaluation, never on her normal work).
- **A temperature:** recency, frequency and the scheduler's predicted next use (it knows which mind
  acts next).

### Policies (each per concern, declared, never hard-coded per tool)

- **Inclusive or exclusive.** KV pages are exclusive: one live copy, plus at most one write-back
  copy below for crash recovery. Weights are inclusive: memory-mapped on Cold always, with a Fast
  copy as a cache.
- **Admission.** A page is promoted only when its predicted reuse pays for the transfer (an
  ARC-style second-touch rule), so a one-shot result never evicts a mind's working set.
- **Eviction is demotion.** A page moves one tier down, chosen by expected loss per byte freed:
  the chance it is needed again before it could come back, times the measured time to promote it
  (or to recompute it, if the tier below would cost more than recomputing). This generalizes
  today's `cost_of_loss = tail_tokens / prefill_rate`.
- **Prefetch.** The scheduler promotes the next lease holder's pages during the current act (the
  warm-ahead path, generalized).
- **Write-back** runs off the frame, from bytes already in Bench. A Fast-tier write never waits on
  flash.
- **Deduplication** is by content address across minds and concerns. A shared prompt prefix or a
  shared adapter is one page with many references (`seq_cp` is a back-reference). The codec's
  dictionary and the page table are one table.
- **Lines.** For hybrid models, whose recurrent state cannot roll back, checkpoints are the cache
  lines: a page is valid only with its line, and the tiers keep them together.

### Coherency and custody

One owner holds each page at a time (custody, as the grant handoff proves today). Transfers happen
at the frame fence. A page is never read while it moves, and never freed while a lease pins it.
Every transfer reports its measured cost, which feeds back into `TierConfig`. The hierarchy tunes
itself to each machine and keeps tuning as load changes.

## The test that proves the interface

Two domain bases as far apart as they come must both fit without bending the core:

- **Code:** a resident runs `npm install`, then `npm run dev`, then a browser test, while serving
  runs. This covers a directory cache, a held process with a port, and the display and GPU.
- **Documents:** a resident and a human edit one file together. This covers exclusive writing per
  region, shared reading, and a human holder.

If either one needs a special case in the core, the core is wrong.

## Migration

Each existing mechanism moves down into the core and becomes an adapter. Its copy is deleted in
the same change. This is phase 5 of the plan, where patterns bubble up into one owner.

| Today | Becomes |
|---|---|
| Kanban `work/claim` (airc) | The model for the core's exclusive hold, expiry and visibility: first, since it is already an exclusive hold, expired by heartbeat and visible across machines |
| `CargoTargetGrant` (kernel confinement) | A Code-base directory resource that the tool declares. The Rust special case is deleted. |
| Two `LeaseRequest` structs (kernel resources, engine capacity) | The memory-tier adapter |
| `GrantLease` (context windows) | A quantity of tokens on the memory-tier adapter |
| `ThroughputLease` | A share on the process adapter |
| `SlotPermit`, `LanePermit` | A quantity of engine slots |
| Two `AdmissionGate`s (kernel, cognition) | Queued outcomes of the core |
| `LifecycleGate` (the engine's lifecycle) | An exclusive hold on the engine process |
| `PortLease` (in core and again in hands) | The network-endpoint adapter |

## What has already landed (interim, card 13c81ced)

- **A mind's child processes run below the engine that serves her** (`continuum-host`
  `process_class`). On Unix it uses the OS's lowest ordinary priority; on Windows, the idle
  priority class. Her work keeps every idle core and is never refused or capped.
- **A wait for a grant never runs inside the holder's own turn.** A reclaim that needs her turn
  to end now ends her turn instead (#5910).
- **Shared memory is one pool.** The GPU working-set hint bounds one GPU process, never the pool,
  and an unreported memory mode stays unknown (`fable/shared-memory-is-one-pool`).
- **An embedding context is charged to each pool its bytes occupy**, at rates measured on
  Metal and on the CPU (#5910).

## Owners

- Fable: this design and the core.
- Codex: the memory, slot and engine-lifecycle adapters, and review of the core.
- Cormac: learning admission.
- The airc owner: the visibility channel.
