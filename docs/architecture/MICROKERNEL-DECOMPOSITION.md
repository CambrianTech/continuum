# Microkernel decomposition of `continuum-core`

**Status:** the plan and its invariant, measured on canary `bc42092af`, 2026-10-18.
**Owner:** Fable (this session) for the invariant and the ring table; each cut has its own
card and owner (below). The invariant is enforced by a `source-hygiene` ratchet from the day
this lands, so the number below can only fall.

## What exists today, measured

| | lines |
|---|---|
| the workspace | 740,643 |
| `continuum-core` (one crate) | 687,676 (93%) |
| files of 1,000 lines or more | 45 |
| the four largest files | 12,357 / 11,807 / 9,838 / 9,822 |

Fan-in and fan-out across the core's top-level modules (a module counts once per module it
imports with `crate::<mod>::`):

| module | fan-in | fan-out | what it is |
|---|---|---|---|
| `time`, `identity`, `fs_portable`, `paths`, `paging` | 23, 22, 8, 12, 10 | 0, 2, 0, 2, 2 | leaves: the foundation, already clean |
| `runtime` | 20 | 16 | the microkernel (`ServiceModule`, bus, probes, pressure); imports `routing`, `sdk_codegen`, `modules`, `persona` back |
| `modules` | 21 | 36 | the service modules AND their commands AND their wiring, in one directory |
| `cognition` | 21 | 32 | the mind; imports `modules` from 27 files |
| `commands` | 8 | 35 | adapters over modules (`sdk_codegen` 281 uses, `modules` 216) |
| `persona` | 16 | 29 | the citizen; imports `modules` from 20 files |
| `ipc` | 9 | 29 | the socket surface; imports `persona` and `runtime` |
| `live` | 6 | 8 | the realtime stack (37,458 lines); imports only `gpu`, `runtime`, `inference`, `system_resources` |

The hairball is five modules (`cognition`, `modules`, `commands`, `persona`, `ipc`) that
each import 29 to 36 of the others. Everything else is already a layer.

## The rings

A module lives in one ring. A ring imports only rings at or below it. That is the whole
architecture; everything else is a consequence.

| ring | name | modules | what it may know |
|---|---|---|---|
| 0 | **kernel** | `time`, `identity`, `fs_portable`, `paths`, `concurrency`, `paging`, `logging`, `orm`, `system_resources`, `runtime`, `governor`, `capacity`, `resources`, `gpu` | the machine: clocks, ids, files, memory, disk, GPU, pressure, the bus, the `ServiceModule` contract. Nothing about models, minds, rooms or commands. |
| 1 | **engine** | `inference`, `inference_capability`, `model_registry`, `ai`, `live`, `media`, `sensory`, `tool_parsing` | serving: lanes, slots, pages, rates, adapters, the realtime media plane. Knows the kernel. Nothing about minds, rooms or commands. |
| 2 | **mind** | `cognition`, `persona`, `genome`, `memory`, `rag`, `perception`, `experience`, `recipe`, `context`, `vdd` | the citizen: deliberation, working memory, genome, recall, activities. Knows the engine and the kernel. Nothing about sockets, CLI verbs or the module registry. |
| 3 | **surface** | `commands`, `ipc`, `routing`, `sdk_codegen`, `airc`, `http`, `modules`, `interface`, `contracts`, `events`, `forge`, `provisioning`, `comms`, `ffi`, `bin` | how the outside reaches in: commands, the socket, airc, the registry of service modules, the binaries. Knows everything below. |

One fact, one owner, one ring. The served model's identity is a ring-1 struct; a ring-2
reader looks it up and never re-derives it. The disk level is a ring-0 fact; the room's
health line (ring 3) renders it. A rate ladder is one function in ring 1 and both axes call
it. These are not style points; each of them cost a day this week
(`docs/architecture/INFERENCE-LANES-REALISTIC.md` § "How a lane change is judged" has the
receipts).

## The invariant, as a ratchet

`source-hygiene::layer_discipline` counts every `crate::<mod>::` import from a lower ring
into a higher ring. **Baseline on canary `bc42092af`: 256 upward imports across 73 module
pairs.** The ratchet fails any PR that raises it. The largest pairs today:

| from | to | files |
|---|---|---|
| ring 2 `cognition` | ring 3 `modules` | 27 |
| ring 2 `persona` | ring 3 `modules` | 20 |
| ring 2 `cognition` | ring 3 `sdk_codegen` | 18 |
| ring 1 `inference` | ring 2 `cognition` | 9 |
| ring 2 `genome` | ring 3 `forge` | 9 |
| ring 0 `runtime` | ring 3 `routing` | 8 |
| ring 0 `runtime` | ring 3 `sdk_codegen` | 7 |

Each upward import is one of three things, and the fix is the same each time:

1. **A fact owned too high.** `cognition` imports `modules` to read serving state that is a
   ring-1 fact. Move the fact's struct down to its ring; the reader looks it up.
2. **A verb owned too high.** `runtime` imports `sdk_codegen` and `routing` to describe its
   own commands. The kernel exposes a trait; ring 3 implements the description.
3. **A module in the wrong ring.** `modules/` holds service modules that are ring 1 or 2
   logic beside their ring-3 command wiring. Split each by owner: the logic goes to its
   ring, the wiring stays.

## Cut order, each with its gate

Every cut is a pure move verified by test-name parity and the ratchets, lands on canary
through review, and deploys to a resident node only after the pace receipt (tool calls per
hour, prefill seconds per act) is no worse than the build before it. One cut per deploy.

| # | cut | why first | gate | card |
|---|---|---|---|---|
| 1 | the four largest files into directory modules by owner (`llm_deliberation_faculty` done; `serving_daemon`, `llama_server`, `bin/continuum` in flight) | a file nobody can read whole has no owner | same test names and count; hygiene green | this session |
| 2 | the layering ratchet lands with baseline 256 | the invariant must exist before the cuts, or the cuts drift | ratchet green on canary | this session |
| 3 | `live` → crate `continuum-live` (37,458 lines, fan-out 8, all downward) | the cleanest big boundary; the thin clients and headless nodes never link it | core compiles without it under a feature; test parity | b8e047fd |
| 4 | ring 0 → crate `continuum-kernel` (≈90k lines) after `runtime` drops its 15 upward imports | the kernel is what every target links; it must compile alone and fast | `cargo check -p continuum-kernel` with no ring-1+ dependency; a grid node links kernel+engine only | new, owner Fable |
| 5 | `modules/` split by owner: logic to its ring, wiring stays in ring 3 | removes the biggest upward edges (`cognition`→`modules` 27, `persona`→`modules` 20) | ratchet count falls by those pairs | new, per module |
| 6 | ring 1 → crate `continuum-engine`; ring 2 → crate `continuum-mind` | the device targets: Apple TV / Quest 3 link neither; a fee-limited AWS node links engine only; the first-impression device links all | each target's `cargo check` time, measured before and after | new |

## The device targets (the reason for the crates)

| target | links |
|---|---|
| Apple TV, Quest 3, mobile (thin clients) | protocol + client crates only; never the core |
| a single first-impression device | kernel + engine + mind + surface |
| a grid inference node | kernel + engine |
| a fee-limited AWS node | kernel + engine, headless, no avatar, no webrtc |

"It compiles fast on the small target" is the acceptance test, per
`crate-split-follows-the-device-targets`. Compile time and separation of concerns are the
same property.

## What this is not

Not a rewrite. Not a sweep. Every step is a pure move with a parity gate, or a fact moved
to its owner with the ratchet count as the receipt. The week of 2026-10-12 showed what four
correct changes shipped together do to a live node; this plan ships one at a time and reads
the pace before and after each.
