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
