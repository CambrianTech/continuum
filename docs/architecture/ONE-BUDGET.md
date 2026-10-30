# One Budget: one authority, one algorithm, one contract for VRAM, RAM, disk and a mind's context

**Status:** design, precedence-winning (2026-10-30; supersedes the 2026-07-20 VRAM-only version of this file).
Owner of the contract: Fable. Owners of each migration are named in "The rewrite".

**Read first:** [`CONCURRENCY-STYLE-GUIDE.md`](CONCURRENCY-STYLE-GUIDE.md) (the daemon shape),
[`MIND-CONTEXT-AND-ENGINE-CONFIG.md`](MIND-CONTEXT-AND-ENGINE-CONFIG.md) (the View, which stays the one codec of what she sees).

## Why

Joel (2026-10-17..30): "It's like the governor. If you don't use it for literally everything it doesn't work." "All disk,
vram, ram, and rag budgeting through one central contract and zero one offs or hacks." "Never patch ever." "Revision is
a continuous process. Never ever have technical debt."

A survey of canary 365f70fe4 (2026-10-30, file:line inventories in the PR that carries this doc) found every resource with
an owner and most consumers deciding for themselves:

| Resource | Owners today | Independent deciders |
|---|---|---|
| VRAM / KV | serving plan; GpuMemoryManager; hw_probe; single_purpose_lane probe; cuda_train.py | 5 probes, 7 "usable" fractions (0.6, 0.8, 0.95, 0.96, mode 0.55-0.96, plan_fill), 4 footprint models (`footprint_for`, `gguf x 1.25`, weights-only, in-process caps), 4 window rules, slots capped by CPU cores and `MAX_LANES = 8`, LoRA bytes absent |
| Host RAM | MemoryPressureMonitor; PressureBroker; PagedResourcePool self-eviction; ResourceDaemon; an unwired kernel governor; SystemResourceMonitor | ~40 caches/rings with literal caps; 3 threshold tables; 3 reserve formulas; 7 private RAM reads; a sentinel that kills on its own 1 GiB rule. The broker's only RAM pool frees nothing |
| Disk | TrackedDir + broker pools | 13 self-retention constants (KEEP=8/4/3/1, 128 GiB, 24 h, 200 files, 50 GiB); ~15 unbounded writers; 2 OWNED classes with no pool; 9 writers bypassing the resolved home |
| A mind's context | `flex_allocate` + View | 49,152 working-window constant; ~40 prompt readers sized by `ContextBudget` fractions or literals outside the turn's grant; 12 message pushes outside the View; `derived-or-floor` comments exempting literals |

Each local rule starves something else without knowing it. That is the whole class of incident behind the 2048 window,
Aiko's bare results, Benchy's 125 repeated acts and the 460 GB disk.

## The contract

**1. One authority.** `ResourceDaemon` (continuum-kernel `resources/`) is the only owner of accounting AND decision for
every resource. No second monitor, ledger, governor, pool-that-decides-for-itself, or probe. It runs on its existing
substrate tick and publishes one `watch` snapshot of grants. Consumers never read memory, VRAM or disk usage; they
borrow their grant.

**2. One measurement per resource, on the authority's tick.** One probe each for VRAM (per device), host RAM, disk
(per volume), and per-engine KV capacity (what the engine reports, #5188). The probes are adapters behind one trait
(`continuum-host` pattern: logic once, OS/backend files only the diff). Every other `sysinfo` / `mem_get_info` /
`nvidia-smi` / Metal working-set read is deleted.

**3. One algorithm.** The flexbox allocator (priority, floor, min, max, grow) moves down out of `continuum-grounding` into
a floor crate named for the pattern (`continuum-flex`), generic over its unit (`Bytes`, `Tokens`). The authority's policy
for bytes and the turn's allocation for tokens are the same function. No fraction, share, divisor or headroom literal
exists outside it; a claim's floor is its measured need, its max its measured want.

**4. Everything is a claim; a claim gets a grant; a grant nests.**

```
device VRAM / host RAM / volume            (measured capacity)
 └─ claims: serving engine, side lanes, training, embed, renderer, caches, tool-output, captures, logs ...
     serving engine grant (bytes)
      └─ weights + KV pool + compute     (footprint: ONE model, from the engine's own report)
          KV pool grant  →  per-mind KV grants   (one RTOS scheduler, activity temperature = priority, goal 9)
                              └─ a mind's window (tokens) = her KV grant ÷ measured bytes/token
                                  └─ the turn's flex allocation over her window: framing, tools, reply,
                                     history, grounding, recall, every View source, working memory, tool/output
```

A grant changes when capacity or demand changes, in either direction. A consumer reconciles to its new grant: a cache
sheds to it, a lane resizes live (the engine stays up, SERVING-ENGINE.md), a disk class's owner deletes oldest-first to it,
a mind's View collapses oldest-first to it. Shrinking a grant IS the eviction request; there is no separate pressure
alert path, threshold table or `evict_at_least` heuristic.

**5. Typed.** `Claim { id: ClaimId, resource: ResourceKind, priority, floor, min, max }` and `Grant { id, amount,
generation }` are the only types that cross the seam. A consumer that holds no grant cannot allocate (state made
unrepresentable: the cache constructor takes a `GrantRx`, not a capacity).

**6. Disk writes through its class.** Every directory the substrate writes is a class with a claim; writers get a handle
from the class owner (which resolves the continuum home) and never pick a path, a rotation count or an age. The class
owner turns its grant into retention. Stores are ORM entities; nothing opens a file database or runs SQL outside the ORM.

**7. Prompt content only through the turn's grant and the View.** Every reader that renders into her prompt takes its
size from its claim's grant in the turn's one allocation and reaches her only as an Item of a View Source.
`ContextBudget`'s fractions, `WORKING_WINDOW_TOKENS`, `ACT_OUTPUT_CAP`, recall buckets, `.take(N)`, `KEEP`/`*_CAP` literals
are deleted. The working-window knee becomes the measured prefill-cost term of her window claim, not a constant.

## Ratchets (they go to zero, they do not hold)

Consolidated into source-hygiene, each counting to a target of 0 with the count required to fall every PR that touches
the area:
- `self_sized_consumers`: a literal capacity, cap, KEEP, max-age, fraction, divisor or headroom on any bounded
  structure, cache, file class or prompt content outside `continuum-flex` and the authority's probes. The
  `derived-or-floor` comment exemption is removed; a floor is a claim field.
- `private_probes`: any memory/VRAM/disk measurement outside the authority's probe adapters.
- `writes_outside_a_class`: a filesystem write whose path is not from a class handle.
- `prompt_messages_outside_the_view` (existing), extended to every crate (it scans only `cognition/` and `persona/`
  today, so `tool/output.rs` escaped) and repointed at the moved owners (`continuum-prompt`, `continuum-grounding`).
- The eviction test asserts a REGISTERED claimant per class, not a decision string.

## The rewrite (crate-sized PRs, each deletes more than it adds, each lands before the next)

| # | Change | Owner |
|---|---|---|
| 1 | `continuum-flex`: move `flex_allocate` down, generic unit; grounding and the authority both call it; ratchet skeleton | Fable |
| 2 | Authority: `Claim`/`Grant` API on `ResourceDaemon`, one probe trait + adapters; delete GpuMemoryManager's ledger, SystemResourceMonitor, hw_probe free math, the unwired governor, the second PressureLevel and threshold tables; PressureBroker becomes grant publication | Codex (kernel/serving), Fable reviews |
| 3 | VRAM: serving plan, side lanes, embed, vision sidecar, in-process backend, training (incl. `cuda_train.py` taking its grant as input) become claims; one footprint model incl. LoRA and draft; slots from the KV contract, not cores | Codex (serving/inference), Cormac (training) |
| 4 | Host RAM: every cache/ring in the RAM inventory takes a `GrantRx`; delete their literals; sentinel watchdog reads its grant | crate owners per the placement map; Fable for cognition/mind |
| 5 | Disk: class handles, writers migrated, KEEP/age constants deleted, unbounded writers claimed, resolved home everywhere | BigMama (live/media), Cormac (genome/dreaming), Codex (engine), Fable (cognition/mind/capture) |
| 6 | A mind's context: window from her KV grant; every prompt reader from the turn grant; the 12 out-of-View pushes become Sources; `ContextBudget` reduced to the grant's view or deleted | Fable |
| 7 | Superseded docs folded here and deleted (RESOURCE-ARCHITECTURE, CAPACITY-FABRIC-AND-GOVERNOR, MOE-SERVING-GOVERNED-BUDGET, EVERY-MODEL-INCLUDED-VIA-L1-BUDGET, STORAGE-SERVING-TIER-GOVERNOR, LIVE-AVATAR-BUDGET where they define a second rule) | Fable |

Gates: each PR runs the affected crates' tests locally (serving down or a non-serving executor), CI confirms, then a
15-minute live read on the residents. The receipts: ratchet counts before/after, lines added/removed, and the residents'
median act and prefix reuse.

## Forbidden moves

- A consumer reading memory, VRAM or disk usage, or computing its own footprint.
- A capacity, cap, KEEP, age, fraction, divisor, headroom or bucket literal anywhere but a measured claim field.
- A second allocator, scheduler, ledger, governor or "manager" beside the authority.
- An alert/shed path separate from a grant shrinking.
- A write to a path the writer chose; a store outside the ORM; raw SQL for any purpose, repairs included.
- Anything she perceives that is not an Item of a View Source sized by the turn's grant.
- A fallback when the grant or measurement is missing: refuse with a typed error naming what is missing.
