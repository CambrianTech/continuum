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
- **When to switch is a learned policy, never a quantum** (next section). No constant, no permit, no
  seat count, and no single global slice: how long a mind keeps the Core falls out of what continuing
  and switching are each worth for her, at this step, on this machine.
- **Nothing is hand-tuned.** The policy starts from this node's measured swap, rebuild and step times
  and learns for its own throughput from there. Whether that comes out at every step, every 20, or
  differently for a deep cold mind and a shallow warm one is for it to find, never a
  number set here (UNIFIED-PAGING.md, "No hand tuning").
- **A step is sized to fit the slice.** A prefill chunk is as large as fits the measured step budget,
  never "the whole batch because the turn keeps the Core". A 2,048-token chunk is about 3.2 s at
  today's cold rate (8044b75b: 133,210 tokens in 207 s, 23:43Z), so at depth a chunk is smaller.

In SAGE's own simulator (three residents, 20 ms steps, 26 ms switch, 1 s bound, 300 s), the lane made
46.3 steps/s with the worst wait at 978 ms, against 24.5 steps/s switching every step (SAGE.md).

## When to switch: a policy with history, not a quantum (Joel, 2026-11-25)

Joel: "It doesn't do every other token or inference unless that was found to be highest throughput.
Maybe it's every 5 inferences or 20 ... It's math and algorithm. You can't go dumb on it." And: "Maybe
it learns that larger KV caches are worth sticking longer ... That's why sophistication and
extensibility of that algorithm, and that it carries a history to learn or work from, along with
extensible history itself, because it matters to each algorithm what metrics and data it needs."

**What a dumb algorithm is, so we never build one:** any rule whose output is a count or a time set
in advance: switch every k steps, a slice of s ms, k learned per machine but the same for every mind.
That covers the global extremum-seeking slice in `continuum_sage::slice::tuner` too: it is
adaptive, but it answers one number for all minds from step and switch times alone. It stays, as
one policy among the ones compared, never as the design.

**What Apple does, read** (`xnu/doc/scheduler/sched_clutch_edge.md`, 2026-11-25). The XNU Clutch/Edge
scheduler separates mechanism from policy in three levels: root buckets chosen earliest-deadline-first
with a worst-case execution latency per class (FG 0, IN 37.5 ms, DF 75 ms, UT 150 ms, BG 250 ms) and
warp budgets; thread groups ordered by an interactivity score from their recent blocked time against
CPU time (the FreeBSD ULE idea); threads by decaying priority. Edge places a thread by an EWMA
scheduling latency (higher-QoS load × average execution latency) against a per-edge migration weight,
and CLPC feeds each thread group's history (utilization, runnable time) into its preferred cluster.
Its quantum, though, is a fixed table per class (FG 10 ms ... BG 2 ms): the adaptivity is all around
the quantum. Ours can be the decision itself, because we know things a kernel cannot: the exact cost
of each switch (measured at every one), each mind's exact residency (our block layer), her own
history of turn lengths, and batching as an action (a CPU core cannot run two threads in one
instruction; our Core can step two minds in one batch).

**The decision.** At every step boundary the lane chooses one action from an extensible set:
`Continue`, `Switch(mind)`, `Batch(minds)` (one step for several minds in their own streams, where the
cost model finds batching pays; `llama-batched-bench` above says it does at every depth measured),
`Idle`. A mind's run length is an outcome of these choices, never a parameter.

**Mechanism and policy, separated as XNU does.**
- *The envelope* (mechanism, never learned, never overridden): no ready mind waits past her deadline
  (the wait bound per urgency, earliest deadline first: an addressed mind's deadline is shortest);
  stride shares by need, so nobody starves; a mind whose predicted remainder is under one switch
  finishes first. A policy chooses only inside it, so a policy that explores or is simply wrong can
  cost throughput but never a mind's wait.
- *The policy* (a trait, several implementations, swapped live, compared on replay): proposes the
  action of highest expected value.

**What any policy is estimating.** Over the next decision window:
- continuing: her progress per engine-second in her phase (a prefill chunk moves hundreds of tokens
  per step; a decode step moves one), and what is left of her turn (predicted from her own turn-length
  history in this activity);
- switching to B: B's progress, plus the waiting it relieves (each waiting mind's need × a penalty
  that steepens as her wait nears her deadline), less the switch's cost;
- batching: the measured aggregate rate at that width and depth, less the per-mind rate it dilutes.

The switch's cost is measured, per pair and per node, never assumed: the pointer move (0.01 ms on the
M5 now, 2026-11-25), the graph rebuild for her next token (+0.1-1 ms measured above), the pages of
hers that are cold (her non-resident bytes, which the block layer knows exactly, at this node's
measured fault rate), the warmth A loses (her step time after a switch against before), and on a
discrete GPU any DMA not hidden under decode. So a deep mind whose pages went cold costs more to bring
in and is worth keeping longer once in, and a shallow warm one is cheap to rotate: Joel's "larger KV
caches worth sticking longer", arrived at from the costs, not written as a rule.

**One interface for every decision; the lane is one use case** (Joel and Codex, 2026-11-25: the
algorithm and the history are both extensible, as OpenCV's algorithms share one interface with their
own parameters and state; SAGE.md carries the interface, Codex's draft):
- A *use case* declares its typed observations, the actions available at a decision, and the measured
  outcome of each. The lane: observations below; actions Continue, Switch, Batch, Idle; outcome the
  need-weighted progress and every wait over the next window.
- A *policy* reads a *view* of the history and scores the available actions. Policy and view vary
  independently: an EWMA, a windowed distribution, a per-pair matrix, a per-activity histogram are
  views; a cost model, a bandit, a learned model are policies.
- The *executor* is the resource's existing owner (the Core worker here; the block layer for
  residency; the renderer for a grade; the grid for placement). It carries out the choice and records
  what happened. A policy never keeps a private log.
- A *decision record* names the exact owner-published revisions it read, the action taken and the
  outcome that followed. Replay hands another policy exactly the information available at that
  choice and nothing later; an unchosen action has no recorded outcome.
- The revisions read are recorded as (owner, handle, revision), binary, never the values: replay reads
  the values from the owners' own history. Copying the inputs into every record would grow history by
  the whole read set at every decision, and the lane decides every few milliseconds.
- Each view declares the window it needs; an owner keeps its history to the widest window declared
  plus what replay asks for, priced in the ledger like any memory. No unbounded log.
- A use case may mark an outcome as a constraint, never a reward term: next-token continuity after a
  move and the codec's law (the head never changes, the newest is never clipped) are gates no policy
  trades against throughput.
- Every action is priced against the one scarcity ledger (memory, Core time, bandwidth, the drive).
- The design test: **a new use case is added without editing any existing policy, and a new policy
  is tried against recorded history without changing the producer or the executor.**

**Storage placement is a use case too** (Joel, 2026-11-25: "Codex is gonna be slow as shit if his
runtime is running on the slowest possible drive. SAGE shouldn't be picking that"). Today a drive is
"cold" by size alone (`detect_drives`: the largest drive that is not the system drive), the storage
tier is a hand-set `CONTINUUM_STORAGE_PATH`, and only model weights have a hot-tier rule
(`hot_tier.rs`, written after the 5090 served a 27B from its 16 TB SATA disk for months). Worktrees,
cargo target dirs, the core binary, the continuum home and the KV block root land wherever they are
pointed; on 2026-11-25 a SAGE design worktree sat on that same disk.
- *Observations:* each volume's measured sequential and random read and write rates and sync latency
  (`capacity/io-probe`, today run only on demand, measured once per volume at boot and then kept
  current from the IO we already time: the block layer's syncs, renames and page faults, the build's
  reads), its free space, and each working set's access pattern and size (hot: random reads, syncs,
  builds; cold: written once, read rarely).
- *Actions:* place a working set on a volume; move it (a hot set up, a cold duplicate down).
- *Outcomes:* build and check time, restore time, act time, and the disk-pressure level of each volume.
- *Envelope:* a hot working set never goes to a volume measured slower than the fastest one with
  room; when the fast volume is full, the disk-pressure owner frees it by moving cold duplicates
  down, never by moving hot work onto the slow disk. The cold role is for archives only.

**The observations we already measure: the starting catalog.** Each has one owner that publishes it
and a receipt that it is measured today; each use case reads the ones it needs.

| Observation | Owner (publishes) | Measured today | Read by |
|---|---|---|---|
| switch time, per pair of minds | Core worker | `persona_switch` (0.01 ms, 35B, 2026-11-25) | lane |
| graph rebuild for her next token | Core worker | `test-persona-blocks --test-core` (+0.1-1 ms) | lane |
| step time by phase, depth, batch width | Core worker | `persona_switch`, `llama-batched-bench` | lane, batching |
| aggregate tok/s by batch width and depth | Core worker | `llama-batched-bench` (194/389/784 at 2k) | lane's `Batch` |
| wait from ready to stepped, by urgency | lane | `frame.rs` `wait_ms` rows (p50 12.1 s, 2026-11-24) | every use case's envelope |
| time in tools against time on the Core, per act | her turn | act probes | lane (interactivity) |
| turn length, per mind and activity | her turn | act and turn probes | lane (finish-first, remainder) |
| first token after a tool result | her turn | submit-to-first-token probe | lane, prefill during tools |
| prefix reused per act | context codec | node health ("prefix reuse 63%", BigMama 13:36Z) | residency, codec |
| re-processed against appended tokens per act | context codec | CONTINUUM-ENGINE governing law (Aris 12:1, 10-27) | codec |
| resident and cold bytes, per mind | block layer | `MappedRegion` demote/promote | lane (switch cost), residency |
| restore time, by part (map, attach, record, rename) | block layer | `persona_handoff` (3.4 ms at 1k; 6.8-9.7 ms at 30k) | residency, stop |
| hand-over time, bytes moved, record bytes | block layer, preserve | `persona_handoff` (record 18 KB) | residency, stop |
| next-token continuity after a move | block layer | `persona_handoff` against its control (32/32) | residency (a correctness outcome: a gate, never traded) |
| evictions | residency | #6258's 4,191 in 15 minutes | residency |
| drive read and write rate, fault rate | node | the block layer's syncs and faults | residency, lane |
| memory and VRAM free | node, the ledger | the pressure monitor | residency, placement |
| frame time and observed quality grade | renderer | render probes | rendering |
| link throughput and queue delay | grid | airc and grid probes | peer placement |
| acts per hour, tool calls per hour, median act | the node | the CLAUDE.md gates | the outcome every use case answers to |

**Published policies are candidates, scored on our outcomes.** Apple's KVP (learned per-token
eviction) and EpiCache (episodes, per-layer budgets) answer a different decision from the lane's:
which of one mind's tokens to keep under a budget. In Continuum that decision belongs to the context
codec and to residency, and it is bound by the codec's law: the head never changes, nothing already
sent moves, the newest is never clipped. Evicting tokens inside a live prefix breaks exact prefix
reuse and next-token continuity, the two outcomes we measure best. So they enter as candidate policies
where they can be fair: for a parked mind's cold state (what stays resident and what goes to flash)
and for the codec's collapse at an epoch start. Each is scored against the baseline of an exact warm
prefix, on matched resident traces, by reuse, continuity, act time and memory held. The exact prefix
may win outright.

**The policies, compared before any reaches a resident:**
0. `Fixed` and today's whole-turn rule: the baselines, never production.
1. The global extremum-seeking slice (`tuner`, built): adaptive but blind to who she is.
2. *The cost model:* the expected-value rule above, every term an online estimate from history. No
   learning beyond estimation, yet run lengths already differ by mind.
3. *A contextual bandit* over the action, its features the views above (her depth, cold bytes, phase,
   predicted remainder, urgency, interactivity, step time, device), its reward the need-weighted
   tokens per engine-second over the next window, less a steep penalty for any wait near a deadline.
   Exploration stays inside the envelope (control both ways, CLAUDE.md).
4. *A learned model* trained offline on every node's recorded history (the grid pools it), served
   through the same trait.

**Counterfactuals.** A replay observes only the action that was taken; an unchosen action has no
recorded outcome. Policies are compared on the simulator fitted from the trace (`slice::sim::fit`,
built) and then live, inside the envelope, never by pretending the log answers what was not done.

**What changes in `continuum_sage::slice`.** `Control` (told only step and switch times; answers one
slice) becomes `Policy`: given the situation (every ready mind's views and the running one's), it
answers an action. `Slicer` keeps the envelope (deadlines, stride, finish-first). `Tuner` becomes
policy 1. `sim` and `trace` gain the new records.

**Gates** (on `persona-lane-bench`, then the 15-minute live read):
- worst ready-to-step wait inside the bound at 4 minds, switch included;
- aggregate need-weighted tok/s at or above every baseline's, on the same trace;
- on a mixed load (deep cold minds beside shallow warm ones) the run lengths differ by mind, and the
  difference raises throughput over policy 1's single slice; if it does not, policy 1 stays and the
  data says why;
- the same binary on the M5 and the IntelMac settles on different behaviour with nothing set by hand.

## The parts (SAGE components on the engine's bus)

| Part | Owns | Hears | Says |
|---|---|---|---|
| **Lane** (`continuum_sage::slice::Lane`, exists) | who steps next: the envelope, the policy, and the history both read | each mind's `Need`, `Decoding`; `Stepped`, `Switched` | `Turn` (who holds the Core) |
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

**Where her KV lives: `Placement`, the trait behind `Tier`'s moves.** Implemented once by the block layer ([UNIFIED-PAGING.md](UNIFIED-PAGING.md): `Retag`, `Dma`, `Mapping`, `Staged` links and the `Streamer`); the tables here are what the Core needs from it.

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

Each CPU-side piece is also vectorized, with the platform's own libraries first:
- BLAS for the matrix work: Accelerate on macOS, MKL or OpenBLAS elsewhere. That covers embeddings
  and similarity (the revisioned vector corpus in the core CPU workstream).
- SIMD for the per-element loops (std::simd or the platform intrinsics behind one adapter):
  detokenizing, stop-sequence matching, sampling's softmax and top-k over the vocabulary.
- Every hot CPU loop has a measured before and after on the bench; one that does not get faster is
  not merged.

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
