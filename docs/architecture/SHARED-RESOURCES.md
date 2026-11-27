# Shared resources: placement and the one authority

How everything finite that is shared (memory on a tier, an awake slot on the Core, a share of the engine's time, a
port, a build cache, a device) is given out, given back and seen by everyone. Owners: Cormac (the authority, the
trainer side), Codex (admission, host-wide arbitration, discrete tiers), Fable (`continuum_sage::place`, the
engine's holders). How bytes physically move between tiers is [UNIFIED-PAGING.md](UNIFIED-PAGING.md); how a run
asks for its room is [STAGE-CONTRACT.md](STAGE-CONTRACT.md).

[Runtime architecture](../README.md#runtime-architecture)

**Contents**

- [The rules](#the-rules)
- [Where it stands](#where-it-stands)
- [The contract: prepare, consume, commit, release](#the-contract-prepare-consume-commit-release)
- [Room by priority: holders are Residents](#room-by-priority-holders-are-residents)
- [One authority, one ledger](#one-authority-one-ledger)
- [Residency over time](#residency-over-time)
- [Capacity is live](#capacity-is-live)
- [Resource kinds and domain bases](#resource-kinds-and-domain-bases)
- [The lease types today, and where each folds](#the-lease-types-today-and-where-each-folds)
- [Acceptance under changing load](#acceptance-under-changing-load)
- [The order](#the-order)

## The rules

- **Resources flow like data.** A consumer acquires for one operation and releases at its end; a reservation that
  outlives its work is occupation, the defect. A guarantee (recall must always be able to embed) is a priority on
  the next turn, never bytes held between turns.
- **Every decision is a price, in time.** Holding costs what everyone else waits behind it; releasing costs the
  measured time to bring it back. With fast tiers, releasing nearly always wins.
- **Nothing is refused for pressure.** A request that cannot be met now waits, named with its shortfall, and the
  law makes room; the only refusal is a proven "never here": the request alone past what this place could hold with
  everything movable moved.
- **Nothing is taken.** A holder gives back only what it is asked for and agrees to give, and bytes count as freed
  only after a fenced move and the allocator's release.
- **Derived values are cached by their inputs.** A prefill, a render, a file read, an embedding is keyed by a hash
  or generation of what derived it and invalidated by that input's revision, never by time; a recomputation from
  unchanged inputs is a named miss.
- **Nothing names a language, a toolchain or a kind of work.** Those are adapters.

## Where it stands

Built (`core/continuum-sage/src/place`, `core/continuum-kernel/src/resources`):

| Piece | Where |
|---|---|
| The placement ledger: typed places and quantities (`Bytes`, `Slots`, `Tokens`), claims, holdings, waits, fenced releases | `place/{claim,holding,ledger,quantity}.rs` |
| `PlacerCore` and the `Placer` component; elastic claims (`Act::Elastic`) and shrink requests (`Status::Shrink`) | `place/core.rs`, `place/component.rs` |
| The flexbox, one solver over any unit; a mind's prompt budget calls it | `place/flex.rs`; `continuum-context/src/flex.rs` delegates to it |
| The `Resident` parent: `report`, `shrink`, `grow`; `Varies`, `Way`, `Given` | `place/resident.rs` |
| `Lease<Q>` over a typed quantity, observed against committed | `decide/capacity.rs` |
| The node's resource authority (`ResourceDaemon`), `resources/free` on the node bus, the lease ledger | `continuum-kernel/src/resources/{daemon,ledger}.rs` |

Not built: the authority as a `place` ledger (in progress on `cormac/step3`), holders implementing `Resident` (none
yet), shrink delivered to running work (STAGE-CONTRACT.md, "Control"), and the fold of the lease types below.

## The contract: prepare, consume, commit, release

One shape for every place and every quantity:
- **Prepare:** one claim across every place a piece of work needs, all of it or a wait. A claim carries its id and
  its owner's generation, so an owner that restarted cannot spend a stale one.
- **Consume:** the consumer spends its prepared claim from its own atomics and never waits, so a claim can be spent
  inside an engine step (a graph's workspace growth is consumed at the scheduler's admission callback).
- **Commit or unwind:** what was consumed becomes held at its actual size; what was not is returned. An allocator
  failure after a prepare keeps the original backing usable and releases the unused claim.
- **Release:** a holding returns only when its owner's allocator has released it, after its fence.
- **A claim is held per period, not per run.** A thread that runs many units holds one elastic claim sized to its
  current unit and resizes it when its unit size changes (Cormac: asks through the authority's lock on every step
  cost more than they save). Per-run claims are for rare work (a checkpoint save).

## Room by priority: holders are Residents

Almost everything that holds memory is variable: a stream's KV sized for a window no occupant uses, a compute
workspace sized for the largest prefill, an embedder's window between recalls, a lesson's rows. Only weights under
live minds are fixed. So a holder is an object that reports and answers, control down and feedback up, in the shape
of RAGE streaming the world around the player's view: the work someone waits on is in front of the camera.

```rust
pub trait Resident<P, Q> {
    fn report(&self, into: &mut Report<P, Q>);           // every place it holds on, one row each
    fn shrink(&mut self, ask: ShrinkAsk<P, Q>) -> Given<Q>;   // give back the cheapest way it has; answers at once
    fn grow(&mut self, grant: GrowGrant<P, Q>) -> Given<Q>;   // take room granted: expand a level, page back in
}
// a row: { place, held, want, varies: Fixed { why } | Elastic { floor, ways } | Whole { dependents, ways }, qos, waited }
// Ways: resize (run on less where it is), cache out (its bytes down a tier), collapse (a cheaper level), each with its measured cost
// Given: Released { way, freed, took } | Pending (until its fence) | Refused(why)
```

- **The controller knows what truly varies.** Each row states it from the holder's own knowledge: `Fixed` (a base
  model's weights under live minds, a turn's KV in flight) is never asked; `Elastic { floor, ways }` runs from its
  floor up; `Whole` moves or caches out entire, asked only by a wait that outranks every dependent. A holder that has
  not reported is `Fixed`: unknown is never room.
- **The law shares a place by the flexbox.** When a wait is short, the passes run over every elastic holding and the
  wait: floors first, then mins, then grow. A holder whose grant falls below what it holds is sent a shrink: a
  request, never a reclamation. The wait is granted only from bytes released after a fence.
- **Never evict, never abort: resize or cache out.** A holder answers a shrink by resizing, caching out (VRAM to
  pinned to flash, paged back on its next touch with no recompute) or collapsing, the cheapest way by its measured
  costs; what it chose and what it cost is the receipt the law learns from.
- **Priority is QoS, then age.** A row's class (Interactive, Work, Utility, Background), aged by its wait within the
  class, capped so an Interactive floor is never shrunk for anything below it. The same classes order the lane's
  steps, so memory and compute agree.
- **Fit is the share, not the ceiling.** A lesson is sized within what the passes grant its seat at its own priority,
  so learning never takes a mind's window to train.
- **It composes.** A part (her KV, her genes) is a Resident; a mind over her parts; the Core over its minds and
  workspace; a node over its engines, embedder and builds; the grid over nodes. A parent folds its children's reports
  and shares a shrink among them by the same flexbox; a child never writes that code.
- **Everything feeds back.** No floor, want or priority is a constant: each is the last measurement (held from the
  allocator, want from use), and every grant, shrink, wait and release is a record in the `decide` replay log that
  the placement policy trains on.

## One authority, one ledger

- The node's authority (`ResourceDaemon`) is the one ledger. Every lease, ask and carved stage is an entry there,
  so every citizen sees who holds what (published as a view, like the kanban), and no subsystem keeps a second
  picture of the same memory.
- The engine's memory is one holding per tier on the authority, sub-allocated by the block layer's own table, so the
  authority sees one holder and the block table is the only picture of what is inside. Pressure from outside (other
  apps, our own builds) arrives as that holding's budget shrinking; a growing budget wakes waiting claims.
- Tiers are per physical memory: on unified memory pinned RAM and GPU mappings alias, so a smaller KV share or a
  move from RAM to RAM never pays for a device shortage.
- Across instances on one host, one elected owner holds the host's reservations (a host-scoped OS lock); other
  instances prepare through its endpoint, and a successor reconciles live allocator receipts before granting.
- A shrink reaches running work as a control request at its next yield point (STAGE-CONTRACT.md); the kernel's
  elastic hold and `ShrinkHolder` are that request's adapter onto the authority, never a second path.

Why one owner: on the night of 2026-11-26 six counters (`LearningMemory`, `lease_memory`, `learning::leased`, the
admission metrics sum, `window_grant` with `GrantHandoff`, `training_demand` through `window_allocator`) each held a
private picture of one memory, so every refusal needed a trace to explain and a fix to one left five wrong.

## Residency over time

A holding also says how long its bytes must stay, as a graphics engine treats a frame's memory:
- **Resident:** stays while its holder runs (base weights; a mind's KV while she acts).
- **Transient:** one bounded operation, never at once with other transient work in the same arena (an embedding
  context, a vision pass, a learning unit's scratch). Transient claims alias one arena sized to the largest need, not
  to the sum of every consumer's floor.
- **Pageable:** leaves the fast tier between uses and comes back (KV, adapters, experts, a small model's weights),
  placed by how soon it is needed against what it costs to bring back.

## Capacity is live

Usable capacity is measured continually, never established at start: other processes take memory, a drive slows
when hot, a volume comes and goes. Consumers hold their claims loosely and the law re-derives grants from the live
reading, so an init-time reserve blind to outside pressure (the 2026-07-16 compute-buffer OOM) cannot be written.
The capacity simulator (`continuum-engine/src/capacity/sim.rs`) plays the same allocator against a scenario timeline
on a virtual clock, so a scenario is a regression test; grid negotiation of capacity is
[GRID-ELASTIC-CAPABILITY.md](GRID-ELASTIC-CAPABILITY.md).

## Resource kinds and domain bases

Proposed. Each kind of resource answers five questions (how much exists, how much is in use, how to acquire, how to
release, how to tell a holder is dead) in an adapter; nothing above knows how a kind is measured:

| Kind | Measures and acts on |
|---|---|
| Memory tiers | device, host and flash memory, through the block layer |
| Files and directories | worktrees, caches, documents: shared reads, exclusive writes per file or region |
| Devices | display, camera, microphone, GPU |
| Processes | a share of CPU, a scheduling class, a lifetime |
| Network endpoints | ports, bandwidth, an external API's quota |
| Seats | an account, or an app session one holder drives at a time |

A domain base holds once what a kind of work shares (Code: a worktree, declared caches, a build as a background
process; Web: a dev server, its port, a browser session; Documents; Media; CAD; Calls), and each tool adapter holds
only its difference from its base.

## The lease types today, and where each folds

About 25 types each decide alone whether their work runs. Each folds into a `place` claim on the authority when its
caller moves, and is deleted then:

| Type | File | Folds into |
|---|---|---|
| `ResourceLease`, `LeaseRequest`, `ResourceLeaseLedger`, `LeaseBoard`, `LeaseGuard`, `LeaseArbiter`, `MemoryHolder` | `continuum-kernel/src/resources/{lease,ledger,daemon,arbiter,holders}.rs` | the authority as a `place::Ledger<_, Bytes>` |
| `ThroughputLease`, `ThroughputLeaseRegistry` | `continuum-kernel/src/resources/throughput_lease.rs` | a `place` ledger over `Slots` |
| `Permit` (admission gate), `Lease<K>` (per-key gate) | `continuum-kernel/src/runtime/{admission_gate,per_key_gate}.rs` | claims on their place |
| `HandsLease`, `PersonaHandsLease`, `CargoTargetGrant`, two `PortLease`s | `continuum-kernel` identity and system resources; `continuum-core/src/commands/ports.rs`; `continuum-hands/src/code/ports.rs` | resource-kind adapters (seats, files, network) |
| `GrantLease`, `PoolGrants`, `GrantHandoff`, `GrantReservation`, `ActHold` | `continuum-engine/src/capacity/window_grant*.rs` | the engine's holders as `Resident`s; `GrantHandoff` stays only as slot custody, never bytes |
| `LeaseRequest`, `Grant` (capacity) | `continuum-engine/src/capacity/mod.rs` | the simulator's inputs, renamed off the authority's names |
| `HostCacheLeaseInputs`, `StickyLease` | `continuum-engine/src/capacity/host_cache_lease.rs` | the host cache as a `Resident` |
| `KvSlotLease`, `SlotPermit`, `DecodePermit`, `ServingLanePermit` | `continuum-engine/src/inference/{slots,resource_admission}` | deleted with the seat counts: the lane decides who steps |
| `HoldLease`, `ServingHold`, `CoveringLease`, `RegisteredGrant`, `GrantRefusal` | `continuum-engine/src/inference/{measured_hold,llama_server}` | deleted with llama-server's lanes |
| `LaneLeaseTerms`, `ThroughputJob` | `continuum-engine/src/inference/{single_purpose_lane,planning}` | deleted with the lane plans |
| `LeasedMemory`, `LearningMemory` | `continuum-genome/src/genome/learner/seat.rs`, `continuum-engine/src/inference/learning/memory.rs` | the learner's one elastic claim (IN-PROCESS-LEARNING.md) |

## Acceptance under changing load

Proposed. The same resident workload through three exact 900-second windows on each node: baseline, a competing GPU
and host-memory workload, then its release while the same lesson stays outstanding. Recorded: build identity,
per-pool free bytes, prepared and committed bytes, physical bytes released, moves and their time, serving latency,
and the interval from capacity returning to the grant. Verified: foreground work continues, two instances never
spend the same headroom, the lesson produces one durable generation after capacity returns, and replays of GPU
pressure (no false VRAM credit) and of unified-memory pressure with aliased views (no double credit).

## The order

1. The authority on `place` (Cormac): the kernel ledger and the throughput registry become `place` ledgers; their
   copies deleted.
2. The engine's holders as `Resident`s, the embedder's window first (it frees memory at once with no fork verb), then
   the Core's workspace, then its KV (UNIFIED-PAGING.md, `MindStorage`).
3. The learner's one elastic claim; the six counters deleted. Gate: the M5 and the 5090 each run 15 minutes on one
   head with zero refusals that are not "never here", every wait naming what it waits for, and a learned generation
   saved on both.
4. The remaining lease types fold as their callers move, each PR deleting the types it replaces.
