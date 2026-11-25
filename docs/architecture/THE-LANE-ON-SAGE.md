# The lane on SAGE: one Core, every mind served at the next step

Status: design for review (Joel, Codex, Cormac), 2026-11-24. No code before it is agreed.

The settled pieces already exist; this page wires them together and changes nothing about them:
- **the Core:** `continuum-persona-engine`'s `PersonaEngine` + `pipeline::Roles`, on
  `continuum-persona-llama`'s `UnifiedTier`;
- **the lane:** `continuum_sage::slice`, its `Lane`, `Slicer` and `Tuner`;
- **the rules:** SAGE.md, "The lane: the one serial piece, sliced so fine nobody notices".

## How it works (as before, and as it stays)

The Core is modeled on a CPU core. Minds are processes, and a mind's native state (her KV rows,
recurrent state, checkpoints) is her register file.

- **One context, one execution stream: the occupant's,** holding the whole context. Every other
  mind's blocks stay in host memory exactly as she left them (`unified_tier.rs`, `STREAMS = 1`).
- **A switch is a swap.** The occupant is saved (her rows copied to her blocks, the stream emptied),
  then the next mind is bound into the emptied stream. `Roles` sequences it: next staged, handoff,
  previous retired, so nothing is dropped or read early.
- **Measured live:** a swap takes 21-26 ms p50 on the M5 (aa734cc01c: max 263 ms; 2026-11-19/20:
  mean ~21 ms). The in-process `persona_switch` validator measures about 6 ms.
- **A step decodes one mind or a batch of resident minds, whichever the cost model says is faster.**
  Batched decode in one graph was faster at every depth measured on the 0.5B (aggregate 1/2/4 minds:
  156/203/264 tok/s at 30k, 194/389/784 at 2k; 2026-11-24). The earlier receipt of four parallel
  decodes at ~2 tok/s each was not batched decode in one graph, and is not used. A batch needs every
  member's KV resident; when minds outnumber memory, the lane rotates them and the swap is the cost.
- **How many Cores a node has is its measured capability:** one on the M5.

Because a swap is that cheap, the Core can switch at almost every step, round robin, and every mind
sees the GPU as if it were her own (Joel, 2026-11-20: "as if all lanes exist but it doesn't degrade").

## What is wrong today (receipts)

The M5, Qwen3.6-35B-A3B Q4_K_M, build 0b4d23ffe9, 23:34-23:49Z 2026-11-24:

| | p50 | p90 | max |
|---|---|---|---|
| wait for the Core | 12.1 s | 144 s | 277 s |
| prefill | 2.5 s | 23 s | 207 s |
| decode | 5.2 s | 16.8 s | 26.6 s |
| turn | 25 s | 293 s | 526 s |

33 resident turns; 26 of them waited more than 1 s. On every row `wait_ms == longest_service_gap_ms`:
she waited out another mind's whole turn. The cause is one rule in `persona_scheduler/frame.rs`: "a
turn in progress runs to its end". The switch was never the cost. The rule never used it.

#6258 (reverted by #6263) went the other way: a pool of five seats, sized from the serving plan's lane
count. Seven minds over five seats, each holding 100k-140k tokens of a 262k pool, paged constantly:
4,191 evictions in 15 minutes (Codex, 23:56-00:11Z).

## The change: the lane picks who steps, at every step

One behavior changes: `frame.rs`'s whole-turn pick is replaced by the SAGE lane, which is already
built and simulated.

- **At every step boundary,** the lane reads each mind's newest `Need` and `Decoding` (state, never an
  await) and picks who steps next. Stride shares by need; a rise in urgency takes the next step;
  nobody starves.
- **The wait bound is the guarantee:** 1 s (Joel, 2026-11-24: "Any over 2 s are wrong. Probably over
  1 s"). The deadline guard brings in any mind whose slack is gone.
- **The slice is the control law's state,** fed by the measured swap cost and step time: it climbs
  aggregate throughput against the bound. No constant, no permit, no seat count.
- **A step is sized to fit the slice.** A prefill chunk is as large as fits the measured step budget,
  never "the whole batch because the turn keeps the Core". A 2,048-token chunk is about 3.2 s at
  today's cold rate (8044b75b: 133,210 tokens in 207 s, 23:43Z), so at depth a chunk is smaller.

In SAGE's own simulator (three residents, 20 ms steps, 26 ms switch, 1 s bound, 300 s), the lane made
46.3 steps/s with the worst wait at 978 ms, against 24.5 steps/s switching every step (SAGE.md).

## The parts (SAGE components on the engine's bus)

| Part | Owns | Hears | Says |
|---|---|---|---|
| **Lane** (`continuum_sage::slice::Lane`, exists) | who steps next, the bound, the control law | each mind's `Need`, `Decoding`; `Stepped` | `Turn` (who holds the Core) |
| **Core worker** (the engine thread: the one serial piece) | the context; one step for the mind the lane named; the swap through `Roles` when it is another mind | `Turn`, submissions | tokens to Emit; `Stepped` (step and swap times); each mind's `Decoding` |
| **Her turn** (state per mind, not a part) | waiting, prefilling from `pos`, decoding; her sampler and checkpoints | her requests | her `Need` |
| **Emit, Record** (exist, off the engine thread) | text, stop sequences, the caller; probe rows | tokens, facts | answers, rows |

Learning keeps its place, between her turns while no turn waits.

## What this deletes

- `persona_scheduler/frame.rs` and the whole-turn rule in the driver loop.
- The driver's own pick (`Driver::pick`): the lane is the one owner of who steps.
- Any fixed chunk size taken from the batch "because the turn keeps the Core" (`prefill_chunk`).

The Core's streams are the batch width the cost model picks: one when batching does not pay, more when it does (each resident mind in her own stream). When the lane rotates a mind in, her storage is exchanged into a stream as a transfer (below). Never a seat count from a plan.

## The validator first (seconds, no live residents)

`persona-lane-bench` drives the real Core (`UnifiedTier` on a model) with N minds at real depths
(2k, 30k, 130k) and mixed turns: a 2,000-token prefill beside a 50-token answer. It reports:
- every mind's worst wait from ready to her next step, swap time included;
- turns completed;
- aggregate and per-mind tok/s;
- swap time by depth.

It runs on the 0.5B and the qwen35moe hybrid on any machine, and on the 35B on the M5 with serving
down. Its first run is today's whole-turn engine: the baseline row, reproducing the live shape.

## The swap is a transfer: a pointer, DMA, or one block. Never a copy loop, never an encoding

Read 2026-11-24 (fork `src/llama-kv-cache-persona.cpp`, `src/llama-persona-file.cpp`; Rust
`persona_scheduler/driver.rs`, `preserve.rs`), the swap today:

| Where | Today | The transfer |
|---|---|---|
| Save (`persona_commit_save`, :140-175) | `ggml_backend_tensor_get` per layer: her whole KV copied into a `std::vector` on the heap | **Unified memory (M5):** her KV lives in her own device buffer; the stream's tensors point at it. A swap is a pointer change: zero bytes moved |
| Bind (:314-331) | `ggml_backend_tensor_set` per layer: copied back | the same pointer change |
| **Discrete GPU (Windows, the 5090)** | the same synchronous copies, to pageable heap memory | parked blocks in pinned host memory (the backend's host buffer type); async DMA on a copy stream, overlapped with the active mind's decode; while VRAM has room, minds stay resident and swap by pointer |
| The driver (`switch_to`, driver.rs:892) | retire, stage, settle, handoff in series: the GPU idles through every swap | `Roles` as built: next staged under her copy-in fence while the active mind decodes; handoff at the step boundary; previous retired after |
| **Flash** (`llama_persona_write_file`; `preserve.rs`) | rows `fwrite`n from the heap copy; a per-cell loop for the cell table; every mind's token history as JSON in the saved-state manifest | her blocks written from her buffer in one transfer (or her file mapped as her host buffer, so parking to flash is the OS's page-out); her cells and tokens as raw arrays in her own block file. No text |

A step never runs a CPU loop over cells or rows, and never encodes to a string. A swap that is not a
pointer or a DMA overlapped with compute is the defect.

## The objects: two memory cases, two flash cases, one contract

Every choice below is a trade-off measured per node, never a rule. The code is an object hierarchy:
traits for the contracts, one adapter per case holding only what differs. Never free functions
branching on the platform. It extends what exists: `continuum-persona-engine`'s `residency::Tier`
("one contract for every residency tier ... an adapter holds only the calls that differ"), `Fence`,
`Execute` and `pipeline::Roles`.

**What the node is (measured once at boot, published on the engine's bus):**
- `MemoryTopology`: `Unified { bytes }` (the M5) or `Discrete { vram, pinned_host }` (the 5090,
  Windows), from what the backend reports.
- `FlashTier`: `Fast` or `Slow`, with measured sequential and random read and write rates. The hot
  tier's `DriveKind` already reads the drive kind from the OS; this adds the rates.
- `CostModel`, measured live, never constants:
  - swap ms by bytes and direction;
  - graph rebuild ms;
  - decode step ms by depth and by batch width;
  - prefill ms by tokens and depth.

  The lane's control law reads it.

**Where her KV lives: `Placement`, the trait behind `Tier`'s moves:**

| Adapter | Her storage | A move | Its fence |
|---|---|---|---|
| `UnifiedPlacement` | an address range reserved at window size, committed only as she grows | the Core's layer tensors exchanged with hers (pointer) | complete once the graph owner has rebuilt; her old storage is retained until the last pass that read it completes (Metal records buffer and offset at encode) |
| `DiscretePlacement` | VRAM while the measured budget holds her; pinned host memory when parked | async DMA of her used rows only, on a copy stream, overlapped with the active mind's decode | the copy stream's event; the graph owner invalidates a captured graph at the binding change (CUDA graph replay reuses captured addresses on a reused graph id: Codex, `ggml_cuda_graph_update_required`) |

**Where she goes when memory is short: `Backing`, the trait behind the flash tier:**

| Adapter | Park | Restore |
|---|---|---|
| `FastFlash` (NVMe) | her storage is backed by her own file; parking is the OS writing back dirty pages | page-ins, with read-ahead advised for the whole of her |
| `SlowFlash` (SATA, HDD) | one sequential block write | one sequential block read, prefetched ahead of her turn from the lane's forecast of who is next; never a page fault on the hot path |

Her tokens and cells sit beside her KV in the same file as raw arrays. No JSON, no per-cell loop.

**Who owns what:**
- `GraphBinding` owns graph reuse against the current binding: it invalidates on a move, or keeps a
  built graph per mind if the cost model says rebuilds cost more.
- The `Core` owns the context and `Roles` (next staged under her fence while the active mind runs).
- The `Lane` owns who steps, and how many: `StepPlan::One(mind)` or `StepPlan::Batch(minds)`. Batch
  exists only if the bench shows batched decode beating switching at real depth; it is a variant
  chosen by the cost model, not a principle either way.
- `SharedPrefix` (only if measured worth it): the same physical pages mapped copy-on-write at the
  start of each mind's range, per layer:
  - unified memory: `mach_vm_remap`;
  - discrete: the CUDA virtual memory API (`cuMemCreate` plus `cuMemMap`).

  Valid only for an identical start under the same gene set.

**SAGE components, wherever a concern has its own cadence.** Each is a `Component` on the engine's
`Bus`: it hears topics, says topics and has a need, so nothing is wired to anything. A synchronous
move inside the Core's step stays an adapter behind `Tier`, since it is part of one serial step.

| Component | Hears | Says | Need |
|---|---|---|---|
| **Lane** (exists) | each mind's `Need`, `Decoding`; `Cost` | `Turn`, and `Forecast` (who steps next, and after) | Urgent while any mind's slack is near the bound |
| **Core** (the engine thread: the one serial piece) | `Turn`, submissions | `Stepped` (step, swap and rebuild times); tokens to Emit | Want while a turn waits |
| **Cost** | `Stepped`, `Moved` | `Cost` (the cost model as state) | Content; Want on a frame whose times drift from the model |
| **Topology** | the backend at boot; memory pressure | `Topology` (`MemoryTopology`, `FlashTier` and its measured rates) | rests after boot; wakes on pressure |
| **Prefetch** | `Forecast`, `Topology` | `Moved` | Want when a forecast mind is parked on slow flash: her blocks are read on its own worker (`ctx.off`) before her turn |
| **Parker** | memory pressure, `Forecast`, `Topology` | `Moved` | Want under pressure: the mind forecast last is written to her backing off the engine thread |

The Core never waits on Prefetch or Parker: a mind not yet resident is simply not in the lane's
`Turn` until `Moved` says she is.

**Rust:**
- Adapters are chosen once at boot from `MemoryTopology` and `FlashTier`, then held as one object.
  The inner loops are monomorphized generics; boot-time choice is enum dispatch, never a branch in
  the step.
- Storage is RAII: `Drop` releases reserved ranges, pinned buffers and mappings. Storage still read
  by in-flight GPU work is held by an `InFlight<T>` until its `Completion` lands, so a move can never
  free what the device is still reading.
- The context stays on the engine thread (it is `!Send`); only handles and fences cross threads.
- `unsafe` lives only in the `llama` crate's FFI wrappers. The fork's side is C++ classes in its own
  files (`llama-persona-storage.{h,cpp}`) with one-line hooks into upstream code.

**The validator for every adapter** (Codex): A/B/A switching between distinct histories at
identical shapes, with graph reuse on. After each switch, compare next-token logits and recurrent
state with isolated controls. Report the end-to-end ready-to-step wait, including fences and
recapture.

## Use every core: the GPU never waits on the CPU

The M5 has 18 CPU cores (6 performance, 12 efficiency) and a 20-core GPU (`sysctl hw.perflevel*`,
`system_profiler`). The only serial work in the system is the GPU's step. Everything else runs
beside it on the CPU, as SAGE components on their own workers. That is why SAGE exists: no concern
waits for another, so all of them can be busy at once.

**Receipt that it is wasted today:** `persona.engine.occupancy` on the M5 (canary 645541ab6,
2026-11-25 01:34Z) read `busy_permille 558`. The GPU was busy 56% of a 40 s window, with its longest
idle gap at 1.8 s, while turns were waiting.

**The rule.** The next step's inputs are ready before the current step lands, so the GPU goes from
one step straight into the next, like a frame pipeline that double-buffers:
- the next mind's swap (a pointer), her tokens, and her graph, built while the current step runs;
- tokenizing her new tail (tool results, messages);
- detokenizing, stop sequences and the caller (Emit);
- probe rows (Record);
- prompt assembly for each mind (her context codec);
- prefetch and parking (Prefetch, Parker);
- embeddings, recall and learning preparation.

Each of these runs on its own worker, never on the engine thread. The engine thread only submits
steps, reads results, and does the swap.

**The gate:** `busy_permille >= 950` while any turn waits, measured live. A 1.8 s idle gap with turns
waiting is a defect with an owner, never a tuning knob.

## Built and measured

**The pointer switch** (#6264; fork `0df7f9a3e`, `src/llama-persona-storage.cpp`):
- On the Core (one stream) each mind's KV lives in storage of her own, the stream's exact shape,
  reserved by her first save. A mind never parked costs nothing.
- A save or bind exchanges the stream's layer tensors with hers (data, buffer, extra), repoints the
  stream views, and swaps buffer ownership with the data, so freeing a parked mind frees only memory
  nobody reads.
- The context drops its built graphs after a move. A data clear zeroes what the stream reads, never
  a parked mind's storage.
- Her file streams from her storage and reads back into it, in the same format, so old files still
  read.

Measured on the M5, 2026-11-25, with live serving sharing the GPU:

| | Copy (before) | Pointer (now) |
|---|---|---|
| Switch, 0.5B, at 2k / 8k / 30k (`test-persona-blocks --test-core`, A/B/A byte-identical to isolated controls) | 1.5 / 5.9 / 22.8 ms | 0.008-0.016 ms |
| Switch through the Rust engine, 4 minds (`persona_switch`) | 2.0 / 6.5 / 26 ms | 0.01 ms; sustained = decode |
| The hybrid (qwen35moe) at 8k | | 0.018 ms |
| The graph rebuilt after a switch (her next token) | | +0.1-1 ms, once |
| Her file at 30k (369 MB) | | written in 32 ms, read in 40 ms |

**Memory:** the 35B holds about 6.8 KB of KV per token (8044b75b's file: 941 MB for 139k tokens), so
about 1.8 GB per parked mind at the 262k window.

**Open:** CUDA graph replay with a repointed tensor (Codex, `ggml_cuda_graph_update_required`). The
5090 runs the same one-stream Core and does not deploy this until his `--test-core` run passes there.

**Batched decode against switching** (`llama-batched-bench`, the fork, Qwen2.5 0.5B, aggregate decode
tok/s for 1/2/4 minds): 194/389/784 at 2k, 208/447/603 at 8k, 156/203/264 at 30k. Batching wins at
every depth measured. The 35B's rows need serving down.

## The order, one PR each, merged before the next

| # | Slice | Gate (validator, then the 15-minute live read) |
|---|---|---|
| 1 | The validator: swap time and bytes moved at 2k/30k/130k on the 0.5B, the hybrid and the 35B; today's whole-turn engine as the baseline row | reproduces the live shape (a short turn waits out a long one); swap ms by depth on the M5 |
| 2 | **Pointer swap on unified memory** (fork, in its own files): each mind's KV in her own device buffer, the stream's tensors pointed at hers | swap at 130k: 0 bytes moved, < 1 ms; replies byte-identical (the fork's continuation tests) |
| 3 | The lane replaces `frame.rs`; steps sized to the slice; Roles overlap in the driver | worst ready-to-step wait <= 1 s at 4 minds, swap included; turns completed >= baseline; aggregate tok/s >= one mind alone minus the swap share |
| 4 | **Discrete GPU:** pinned host buffers and async DMA overlapped with decode; resident-by-pointer while VRAM has room | the 5090: swap hidden under decode (step time unchanged with a swap in flight) |
| 5 | **Flash:** one block write from her buffer (or mmap); cells and tokens binary in her file; the JSON token history deleted | stop and restore at 130k: write and read at the drive's measured sequential rate |
| 6 | Her new tail prefilled while she is in a tool (her step, when the lane gives her one) | submit to first token < 1 s after a tool result |

Each slice deletes the path it replaces in the same PR. A live read worse than the one before it sends
the node back at once.

## Questions for review

1. **Swap cost at depth** (Fable, slice 1). At 130k tokens on the 35B, how long are the save and the
   bind? If the swap grows with depth, slice 3 comes before the bound holds at four deep minds.
2. **Chunked prefill under the bound** (Codex). The lane sizes the chunk from the measured step time.
   Is there any caller that relies on a turn's prefill landing in one piece (the view checkpoint
   already ends a chunk exactly at her view)?
3. **Learning** (Cormac). Learning still runs when no turn waits; slice 2 changes who steps, not when
   learning runs. Does anything in LearnPort or `learning/said` assume a whole turn ran uninterrupted?
4. **The deploy tracker** (Codex). It replaced a manual rollback with canary at 23:54Z. A live
   rollback should hold until canary is fixed, not be undone by the tracker.
