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

**Safety contracts** (Codex's review of this doc, 2026-10-30; they bind every slice):
- **A shrinking grant is a request, not a reclamation.** Capacity a consumer occupies is not reusable
  until that consumer releases it and acknowledges the actual reclamation. Grants carry a generation;
  a lease bound to an old generation cannot be spent.
- **Multi-resource admission is atomic.** A claim needing RAM, VRAM and KV together (a lane, a training
  job) is granted all of them or refused as one; no partial acquisition, so no acquisition deadlock.
- **Missing or stale is a state, never a number.** A stale or missing measurement and a failed backend
  resize are typed states the authority publishes; it never invents capacity to fill the gap.
- **Durable is not cache.** A disk class says whether its bytes are an evictable cache or a durable
  record (identity, training, task records, unfinished work). Pressure on a durable class is a refusal
  or an escalation, never oldest-first deletion.
- **Demand, feasibility and allocation are three things.** A claim's minimum is a feasibility constraint
  (below it the consumer cannot run), its want is a demand estimate (measured), and its grant is the
  authority's allocation; none is derived from another.

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
| 0 | Enforcement first (below) | Fable |
| 1 | `cambrian-flex` (airc workspace `crates/flex`): the one solver cleared by price, generic unit; grounding, the view and the authority call it; `flex_allocate` deleted | Fable |
| 2 | Authority: `Claim`/`Grant` API on `ResourceDaemon`, one probe trait + adapters; delete GpuMemoryManager's ledger, SystemResourceMonitor, hw_probe free math, the unwired governor, the second PressureLevel and threshold tables; PressureBroker becomes grant publication | Codex (kernel/serving), Fable reviews |
| 3 | VRAM: serving plan, side lanes, embed, vision sidecar, in-process backend, training (incl. `cuda_train.py` taking its grant as input) become claims; one footprint model incl. LoRA and draft; slots from the KV contract, not cores | Codex (serving/inference), Cormac (training) |
| 4 | Host RAM: every cache/ring in the RAM inventory takes a `GrantRx`; delete their literals; sentinel watchdog reads its grant | crate owners per the placement map; Fable for cognition/mind |
| 5 | Disk: class handles, writers migrated, KEEP/age constants deleted, unbounded writers claimed, resolved home everywhere | BigMama (live/media), Cormac (genome/dreaming), Codex (engine), Fable (cognition/mind/capture) |
| 6 | A mind's context: window from her KV grant; every prompt reader from the turn grant; the 12 out-of-View pushes become Sources; `ContextBudget` reduced to the grant's view or deleted | Fable |
| 7 | Superseded docs folded here and deleted (RESOURCE-ARCHITECTURE, CAPACITY-FABRIC-AND-GOVERNOR, MOE-SERVING-GOVERNED-BUDGET, EVERY-MODEL-INCLUDED-VIA-L1-BUDGET, STORAGE-SERVING-TIER-GOVERNOR, LIVE-AVATAR-BUDGET where they define a second rule) | Fable |

### Slice 0: enforcement before any migration

Doctrine account (DEVELOPMENT-DOCTRINE.md, "Completion includes deletion and proof"):
- **Invariant.** No budget decision exists outside the authority and `continuum-flex`, and no file a PR
  touches keeps one.
- **Owning crate.** `source-hygiene`, as one module (`budget_discipline`). It replaces four scanners that
  hunt the same class with different vocabularies and two comment exemptions:
  - `derived_bounds` (exemption `derived-or-floor:`, 73 markers on canary 365f70fe4);
  - the de-hardcode guard inside `context_budget` (exemption `context-budget-exempt:`, 71 markers);
  - `head_cuts`;
  - the budget half of `context_codec`.
- **Contract.** One pattern set per rule (`self_sized`, `private_probes`, `writes_outside_a_class`,
  `prompt_outside_view`). One allow-list, of OWNERS by path: the authority's probe adapters,
  `continuum-flex`, and the class handles. There are no markers. Two judgements:
  - the existing base comparison: a count never rises;
  - new: **a touched file is clean**. Every file the PR modifies has zero instances at head, the same rule
    as the 1,000-line gate. This is what moves the counts to zero instead of holding them.
- **Callers.** CI's ratchet job and `cargo test -p source-hygiene` already run it.
- **Deleted.** Both exemption markers and their parsers; the guard inside `context_budget`; the separate
  scanners above (merged, not wrapped).
- **Validator.** A fixture for each banned shape proves the rule fires on it, including a product of two
  constants, a fraction of a window, a private `sysinfo` read and a `File::create` outside a class. A
  fixture for each owner path proves the rule stays quiet there. The PR reports per-rule counts before and
  after.
- **Limit, stated.** Lexical rules are a backstop. The lasting enforcement is the type: once slice 2 lands,
  a cache, ring or class can only be constructed from a `GrantRx`, so an unbudgeted one does not compile.

### Slice 1 in detail: `cambrian-flex`, the one solver, cleared by price (for review before code)

**Why it changes from `flex_allocate`.** Today's solver (`continuum-grounding/src/rag_budget/flex_allocate.rs`,
three production callers: the deliberation view, its epoch start, grounding) allocates by integer priorities and
min/max/grow shares, all picked numbers. HOW-A-MIND-GETS-WHAT-SHE-NEEDS R4 and GRID-MARKET-CLEARING replace that
with one rule: a claim grows while its measured marginal value exceeds its tier's price, and the price moves with
measured excess demand. Slice 1 builds that solver once, as a floor crate, and every tier calls it: the
`ResourceDaemon`'s tiers (slice 2, Codex), the engine's lanes and state (L5), a mind's claims (slice 6).

**Crate.** One solver below both repos (Cormac on #5620): airc's NETWORK-AUTHORITY (airc#1628) needs the same
types for network grants and must stay separately capable, so the crate depends on nothing but `std` and `serde`
(ts-rs behind a feature), the same "bubble down" rule as `airc_core::shown_id`. Its home (settled by Cormac in review): its own
crate in the airc workspace, `crates/flex`, package `cambrian-flex`, named for the pattern and neutral to both
repos, not inside `airc-core` (which keeps its one job). Continuum depends on it through its existing airc pin;
airc's NETWORK-AUTHORITY consumes it. Slice 1's code therefore lands as an airc PR. The cost of that home
(Cormac on #5723): a solver change is an airc PR plus a continuum pin bump, not an edit beside the caller. It is
paid once per API change, not per memory-policy change: policy (values, levels, claims) lives in continuum and calls
the solver; only the solver's own types and clearing live in airc. While a solver change is being developed, a
local `[patch]` in the developer's `.cargo/config.toml` points continuum at their airc checkout, so the edit is
checked in seconds and the pin moves only when the airc PR merges. Pure: no I/O, no clock, no randomness (THE-PROVING-GROUND P8). Generic over a unit type each
consumer defines (`Tokens`, `Bytes`, `LaneSeconds`, bytes in flight); continuum-only pieces (where values come
from, the behaviour model, ts-rs bindings) stay out of it.

**Types.**
- `Claim<U>`:
  - `id: ClaimId` and `owner: OwnerId` (typed ids, never strings);
  - `floor: Floor<U>`: `Required(U)` is a reservation, granted before anything is traded and never traded
    (GRID-MARKET-CLEARING invariant 4); `Optional(U)` is the least amount worth having at all;
  - `value: Value<U>`: what each additional unit is worth, measured and versioned (`ValueVersion`).
    `Steps` for discrete levels (an item's pointer, summary, whole: each a cost and a value) or `Curve` for a
    continuous amount (output room, a cache); concave by construction, checked on input;
  - `switch_cost: Value`: the measured cost of changing this claim's grant (a re-prefill, a page move),
    so the solver never moves a grant for less than it costs (R7, hysteresis as a measurement);
  - `current: Option<Grant<U>>`: what it holds now.
- `Capacity<U>`: the measured amount, its source and time; `Unavailable` when not measured, which clears
  nothing and says so (never a default).
- **Every measured input carries provenance** (Codex on #5620): a capacity or a value is `Measured { source,
  at, version }`, `Stale { last, age }` or `Unmeasured`; the solver treats stale and unmeasured explicitly
  (an unmeasured value claims only its floor) and the receipt says which inputs were which.
- **Prices carry their unit:** `Price<U>` is value per unit of `U`. Comparing or summing prices of different
  units does not compile; a conversion between tiers (a byte's price into a lane-second's) is an explicit,
  measured function, never an implicit cast.
- **A joint claim** asks for several resources at once (a mind's state needs device memory AND a lane; a
  relay needs both legs). It is solved at the lowest tier that is an ancestor of all its resources, where its
  increment is the bundle (one unit of each, priced as the sum of its resources' prices through the explicit
  conversions), so the greedy admits whole bundles or none: one solve, no cross-tier protocol, never a
  half-admission holding one resource while waiting for another (Cormac on #5620).
- `Grant<U>`: the claim, its **target** (what the clearing decided it should hold), its **committed** amount
  (what it may use now) and the tier's generation, with a state: `Granted` (committed = target), `Held` (a
  change was not worth its switch cost), `ShrinkRequested { to }` (it holds more than its target and is asked
  to release), `Waiting` (its next whole block is not yet free while others release), `Unmeasured`
  (reservation only). A required floor that cannot fit is never a grant: it is the clearing's `Infeasible`
  outcome (Codex on #5620: one typed outcome; nothing infeasible is presentable as an acquired lease).
- `Clearing<U>`: one solve's receipt: capacity, each claim's value version, every grant, the price, what is
  unallocated, and any under-provision. The proving ground and the live receipts both read it (R11).

**The algorithm.**
1. Required floors first, in one pass; if they do not fit, the outcome is `Infeasible` with the claims named,
   nothing moves, and no grant is issued.
2. The remaining capacity goes to the next increment of highest value per unit across all claims, one
   increment at a time, until capacity is spent or no increment's value per unit exceeds zero. For divisible,
   concave values this greedy order is optimal. Indivisible levels and optional floors make the problem
   non-convex, so no optimality is claimed for them: every clearing reports the **achieved** value (a feasible
   lower bound on the optimum) and the value of the same clearing with every indivisible step relaxed to
   divisible (an upper bound), and their difference is the stated gap. A small domain may be solved exactly
   instead, with its measured cost reported (Codex).
3. The price is the scarcity price: the value per unit of the most valuable increment left UNMET, what one
   more unit of capacity would buy. It is zero whenever every claim's demand was met, idle capacity or not
   (Codex: "the last increment admitted" could publish a positive price while capacity sat idle).
   **Determinism** (THE-PROVING-GROUND P8): equal value per unit is broken by `ClaimId` order; values compare
   under a total order (`f64::total_cmp`), NaN and infinities are refused on input like non-concavity, and the
   receipt records values at the precision the solve used, so the same inputs give byte-identical grants on
   every machine (Cormac).
4. A change from a claim's current grant is applied only when its value gain exceeds its `switch_cost`;
   otherwise the grant is `Held`.
5. If the required floors alone exceed capacity, the outcome is `Infeasible` with the claims named: nothing is
   oversubscribed and nothing defaults (Codex).
6. **Physical ownership is charged before anything grows** (Codex). Each holder's committed amount, including
   any it was asked to release and has not, stays charged. Targets are decided on the whole capacity (who
   should hold what); growth toward a target is committed only from capacity no one holds, and only in WHOLE
   blocks: an indivisible level or an optional floor is acquired entirely or not at all, never as partial
   custody (Codex on airc#1630: B with a floor of 10 must not receive 4). A hold is applied only when the
   held amounts plus every reservation fit. In Codex's case (capacity 10, A holds 10, B proposes 10) either
   A is held and B's target is 0, or A is asked to shrink and B waits holding nothing; B never owns a unit A
   still holds.
7. **A release is the holder reporting what it physically holds now,** keyed by its claim, at or below its
   target as it likes: idempotent and monotone (it only ever lowers what is charged). A holder known to be gone
   (its process exited or crashed) is **withdrawn** by the authority, freeing everything it owned, so a holder
   that disappears after a shrink request never strands its units (Cormac on #5620). It is not fenced
   by the clearing generation, since a release can never overcommit, so a later clearing never invalidates a
   delayed real release (Codex on airc#1630).
8. **Capacity measured below what holders own** (the person's game took memory, S4) is reported as
   `Overcommitted { by }` in the receipt: nothing grows, free capacity is zero (never negative), and shrinks are
   issued toward targets that fit the new capacity (Cormac).
9. **New commitments are fenced:** a commitment to use more carries the generation it was made at and is
   refused when stale, so no holder grows on an old decision.

**Nesting.** A tier is itself a claim on its parent. Its value curve one level up is its children's
**achievable optimum** at each amount the parent could grant (the value function of its own clearing, with its
children's reservations and couplings), not the pointwise sum of their curves (Codex). For divisible, concave
children that is their increments merged in value order; otherwise it carries the same lower and upper bounds. The parent clears and grants the tier an amount at its price; the tier clears its children within
that amount. Prices flow down and bids flow up; no tier sees more than its parent's grant and its children's
claims (GRID-MARKET-CLEARING section 2).

**Where values come from: measured, never picked.** A claim's value is measured from outcomes: for a context
item, how often items of its kind were referenced or faulted when held at each level (Cormac's behaviour model,
from the reductions); for output room, the measured cost of a cut; for a lane, the measured seconds saved. The
policy loop (L7) refines them from controlled probes. Until a kind has measurements, its value is the
population's measurement for the same activity (HOW-A-MIND S9), marked as such in the receipt. The integer
priorities in `priority::*` are deleted with `flex_allocate`.

**Adversarial fixtures** (Codex): asymmetric bundles and optional floors where greedy is far from optimal, with
the reported bounds bracketing a brute-forced optimum.

**Validator cases** the airc PR adding the crate carries (airc's NETWORK-AUTHORITY N1 builds on them): two joint
claims competing for the last unit of one resource never leave either half-granted; the same inputs in any order
give byte-identical receipts; a shrink is never re-granted before the holder reports its release; a release reported after
a later clearing is still credited; a withdrawn holder frees everything at once; capacity below what is owned
is overcommitted and nothing grows; growth never commits part of an indivisible block or an optional floor; a
stale commitment is refused; a fully met demand publishes a zero price; required floors past capacity are `Infeasible` with the claims named; an
unmeasured claim gets only its floor.

**Part 2: resource bundles, nesting, and who owns which decision** (part 1 merged as airc#1630; Codex needs this
shape for slice 2).

*Ownership, so there is one decider.*
- **The node's `ResourceDaemon` owns one clearing** over the node's resource vector: device bytes, RAM bytes,
  disk bytes, lane-seconds. Every consumer is a claim on it. The kernel's separate throughput broker and byte
  ledger stop deciding: their consumers become claims, their decisions are deleted, and the physical custody they
  tracked stays, held as leases on the daemon's tier (part 1's `report_held` / `withdraw`).
- **Admission (L5) commits, it does not decide.** A mind's turn is admitted only inside a bundle the clearing has
  granted; L5 then chooses the exact slot, page and generation within that grant, commits it atomically, and
  reports physical custody back. A grant it cannot realize (no slot free for the granted state) is reported, and
  the next clearing re-prices; L5 never grants itself more.
- **A mind's own claims are a child tier** under her node grant: her view levels, thinking and answer clear
  inside what the node granted her.

*Resource vectors, typed.* A tier is generic over a resource vector `R`, a consumer-defined struct of typed
amounts (for example `NodeResources { device: Amount<Bytes>, ram: Amount<Bytes>, lanes: Amount<LaneSeconds> }`)
implementing a small trait: componentwise add, saturating subtract, `fits_within`, and per-component iteration
for receipts. Part 1's single-resource tier is the one-component case (`Amount<U>` implements the trait), so
nothing written for it changes meaning.

*Joint claims.* An increment's size is an `R` (a mind's next state block needs device bytes AND a lane at once),
and its value is the increment's total value. The clearing admits an increment only if every component fits, so
a bundle is whole or nothing by construction. Order is by value per priced size: each component's size weighted
by that component's current price, iterated as prices move (the Lagrangian multi-resource greedy). Because
multi-resource selection is non-convex, every clearing reports the achieved value and the relaxed upper bound,
exactly as part 1 does for indivisible steps. The prices are a vector: one `Price<U>` per component, each in its
own unit.

*Nesting.* A child tier's claim on its parent is its achievable value at each amount the parent could grant: the
parent asks the child to clear at a measured set of candidate grants (around its current grant, the sizes its
claims' blocks make meaningful) and the child returns, for each, its achieved value and upper bound. The parent
treats the result as the child's increments, with the gap carried forward. A child never sees anything but its
grant; a parent never sees the child's claims.

*Boundaries, made explicit* (Codex on #5620, part 2):
1. **Pools, not just units.** A component is a typed amount in a **physical pool**: `PoolId` plus unit. Two GPUs
   are two pools of bytes, never interchangeable; on unified memory the device view and the RAM view are aliases
   of one pool, normalized to it before clearing. The pool topology is measured and revisioned; a claim or an
   observation from another topology revision is refused, not mapped. The `NodeResources` struct above is an
   example, not a schema: a node's vector is built from its measured pools.
2. **Lanes are concurrency over an interval.** A lane component declares its interval and is constrained in
   instantaneous concurrency (how many generations run at once, the measured knee), not as a bare total of
   seconds, so two claims can never be granted overlapping use of the same seat.
3. **Child quotes are pure.** The parent asks a child for quotes against one immutable snapshot of custody and
   observations; a quote mints no lease and requests no shrink. The parent selects, and exactly one allocation
   (parent and child together) commits atomically, fenced to that snapshot; a commit against a stale snapshot is
   refused. The child's physical occupancy stays charged in the parent until it is really released. The upper
   bound is certified for the original problem, not only the sampled candidates: the relaxed value is concave in
   the grant, so at each sampled grant its price (a subgradient) gives a line above the whole curve, and the
   bound between samples is the lowest such line.
4. **A grant reserves until it is realized.** From grant to L5's commit, the granted bundle is reserved; no other
   claimant can use it while slot, page and generation are being chosen. `Unrealizable` is a typed outcome that
   releases only the unspent reservation; any physical operation that started keeps its custody. Realizations and
   releases carry a stable lease and incarnation plus an **operation identity**, so a delayed failure or an engine
   replaced underneath (ABA) can never withdraw a newer bundle.
5. **Zero prices, ties and termination, in the shared solver.** A component whose price is zero does not
   constrain, so an increment's priced size counts only priced components; when every price is zero (slack),
   every positive-value increment that fits is admitted, in value order, ties by `ClaimId`. Each iteration admits
   or rejects at least one increment, so the solve ends within as many iterations as there are increments; the
   result is feasible at every step, and the bound is reported. No stopping threshold is picked and nothing loops
   unbounded on the hot path.

*Validators (part 2's PR).* Two discrete GPUs plus unified-memory aliases; simultaneous time-overlapping lane
claims; repeated pure child quotes leave custody unchanged; a commit on a stale quote is refused; a failed L5
realization after paging started keeps its bytes; a delayed failure after a regrant cannot free the new credit;
zero-price and overload termination; two joint claims competing for the last unit of one component (neither half-granted);
a bundle that fits on one component and not another (not admitted, priced in the component that bound); a parent
re-granting a child less (the child's shrinks follow part 1's custody rules); conservation across nested tiers
under the randomized drivers extended to vectors; L5's report that a granted bundle could not be realized, and
the next clearing's re-price.

**Migration.** The three callers move to `cambrian-flex` in the PR that adds it, `flex_allocate` and
`RagSourceBudget`'s priority/min/max/grow are deleted, and `context_codec`'s ratchet points at the new owner.
The behaviour change is proven on the proving ground's T0 against the known-bad suite before it reaches a
resident.

**Review questions** (answered so far: the crate depends on nothing in continuum and serves airc too, Cormac;
provenance, infeasible floors, generation fencing, joint claims, acknowledged shrinks and unit-typed prices,
Codex).
1. Codex (slice 2): is `Clearing<U>` with nested tiers the shape the `ResourceDaemon` publishes, and is
   `generation` enough to fence stale grants across your leases?
2. Cormac: can the behaviour model produce per-kind level values (reference and fault rates by level) from the
   reductions for the first slice?
3. Both: the `Steps` gap bound versus an exact small knapsack per tier, which is affordable at a mind's item
   counts; is the bound acceptable or should small tiers solve exactly?

### Slice 6 in detail: a mind's context (the defect live on 2026-10-30)

**Every machine, no machine's numbers** (Joel, 2026-10-30: "You are not coding for a 64gb MacBook m5. You
are coding for all machines."). Every quantity below is measured on the node serving her, at run time. A
number learned on one box (the M5's 49,152-token knee, a 27B's 12,288-token act, a quarter share replayed
on M5 captures) never enters code. A machine with no measurement yet refuses or starts from what the node
itself reports (the engine's KV capacity, #5188), never from another machine's constant.

**What failed live** (the M5, build ab0466601, 2026-10-30 18-19Z; receipts in the PR):
- the act reply claims its runaway cap (`ACT_OUTPUT_CAP = 12,288`) at a priority above her context
  (`view.rs`: `reply.max_tokens = reply.max_tokens.max(reply_runaway)`). Measured need is 809-1,641 tokens;
  the allowance was 4,486-12,288;
- room left unclaimed is first handed to that reply (`view_room = unallocated - (runaway - reply_grant)`),
  so her view gets what the reply leaves;
- the epoch start gives her growing sources a fixed quarter (`EPOCH_START_SHARE = 4`) although the derivation
  beside it bounds them at half. Benchy: `granted=21346`, `appended_per_act=16565`, `room=0`: one file read
  fills the grant, every act turns over, every earlier read collapses to a note;
- she re-reads what collapsed: 75 reads and greps, 2 edits in an hour; one file read 8 times.

**The contract:**
1. Her window is her KV grant from the one scheduler on her node (goal 9), not `WORKING_WINDOW_TOKENS`.
2. One allocation over typed claims: framing and tools (required); the reply at her MEASURED need (floor:
   her measured answer channel); each view source (floor: folded, want: whole); grounding. What no claim
   takes stays unclaimed: it is the room her view grows into by appends.
3. Output is not context and is not reserved beyond its measured need. The request's `max_tokens` is the
   reply's grant (its measured need, no cap). A turn cut at it records its cut channel at double, so the
   next claim grows from the evidence. (A whole-remaining-window bound was tried and removed: the provider
   refusal-and-refit protocol assumes a fixed bound, and a bigger one changed what a refusal means.)
4. A runaway generation is a time fault: a decode deadline from her node's measured decode rate, owned by
   the scheduler with every other time bound. No token cap constant.
5. The epoch turns over only when the view cannot append into the unclaimed window. Its start follows the
   derived bound (reserve >= growing, from her measured append rate); no share constant.

**Deleted (first PR):** `ACT_OUTPUT_CAP`, `reply_runaway`, `PASS_RUNAWAY_MULTIPLE`, the act's runaway claim
above her context, the `view_room` arithmetic that handed unclaimed window to the reply, and `EPOCH_START_SHARE`
(the start takes the derivation's half bound). **Kept, because it is a measurement:** a pending cut's peak,
retained until its concern settles (the ordinary need decays through short reads and a retried write would be
claimed small and cut again). **Next, owned:** `WORKING_WINDOW_TOKENS` stands in for a real cost (each turnover
re-prefills the whole view, so a bigger window makes each turnover slower); it is replaced by a measured
prefill-cost term in the epoch start, not deleted bare. Then the side budgets computed before the allocation
(`after_reserves`, `irreducible_room`, `fit_newest_result`) become claims.

**Validation (before any live deploy): any machine, including one never seen** (Joel, 2026-10-30: the
3090, the M4 32 GB, "some new or unknown machine"). The code holds no machine; a node is described only by
what its engine and probes report: KV capacity, decode rate, prefill rate. So the dyno takes a NODE PROFILE
as input and the validator sweeps the profile space (small window with slow decode through large window
with fast decode), replaying captured traces under each: a read stays whole across acts while the window
has room; no turnover while unclaimed window remains; prefix reuse per act reported. A new machine is
covered by its own measured profile, never by a list of boxes. Then one deploy per node and a 15-minute read: reads per edit, writes per hour, act time.

### Slice 3b in detail: a mind's memory in the engine, granted by level and valued by prediction (2026-11-03)

Joel, 2026-11-03: "One slot grows in context and the other shrinks and same for genome sizes. The goal here is
dynamic allocation"; "Every part of prompts and these kinds of caches need to comply or the whole shebang doesn't
work, plus it's less code and easier"; "it lets you do really advanced prediction when inputs and outputs to LLMs
(or world models) are entirely under the same systems"; "You can then prioritize both caching and collapsing,
expansion, in the flexbox or any of your memory constraints."

**What the engine holds, by its own report** (fork `resident_state`, CambrianTech/llama.cpp#91, read live on the M5
2026-11-03 12:51Z, Qwen3.6-35B-A3B at 2 x 128,768): target weights 20.69 GB, target KV and recurrent state 2.14 GB
(8,317 B/token as one figure; it is two holders, below), one compute graph 1.87 GB, the draft model 3.15 GB, the projector 0.90 GB, one checkpoint 94.6 MB
(x 32 per slot at today's `--ctx-checkpoints` = 3.0 GB per mind). Before #91 the plan saw only the first three and
inferred the rest from the process footprint as a per-lane term that read 2.8 to 4.5 GB a minute apart.

**The claims, on the node's device tier** (one pool on unified memory, normalized per part 2's pool rule):

| Holder | Claim on `cambrian-flex` |
|---|---|
| Weights, graph, draft, projector of the serving model | `Floor::Required`, the engine's reported bytes |
| A mind's context, resident | `Floor::Required` = what her in-flight turn holds (a grant never shrinks under a running turn); then one indivisible increment per block of her context, valued as below. On a hybrid model the attention cells are per token and may be shared (`seq_cp`); the recurrent state is per sequence and is never shared, so each owner pays her own. The two are separate claims, and the fork's report gives each its own bytes (today it reports one 8,317 B/token figure; the split is a field #91 adds) |
| Her checkpoint | one indivisible increment of its reported bytes, at her predicted divergence point |
| Prompt cache | divisible increments, valued by measured restore hits |
| Her genes | one indivisible increment per adapter her focus selects; an adapter shared by several minds is ONE claim |

**A page's level depends on its genes** (Cormac on #5723). Since fork #90, a paged-out KV page restores only when
every adapter ACTIVE on it (scale != 0) is attached at the same file revision; otherwise the engine refuses it and
she re-prefills cold (IntelMac 2026-11-03, task 519: 1,139 s for 13,144 tokens after a refused restore). So her
page at a resident or paged level and its active genes are one bundle for the restore: the solver may keep the page
paged only while those genes are held at a level that can attach them before the page-in, and the page-in's
`switch_cost` includes attaching any that are not. A page whose genes are lowered below that is valued as what it
then is, a re-prefill, never as the page-in it can no longer be. This is a dependency between claims, the joint form
of `cambrian-flex` part 2 (airc#1632), not a rule inside either owner.

**Caching, collapsing, expanding are one decision.** Each item has levels: on the GPU, paged to RAM, paged to disk,
rebuilt from her prompt. A level is an increment on that tier's clearing (the GPU tier, the RAM tier, the disk tier),
with its bytes as width, its value as below and its move (a page-in, a page-out, a re-prefill) as `switch_cost`.
Caching is a level held; collapsing is a grant lowered (a `ShrinkRequested` the holder reports when done);
expanding is a grant raised, which a prediction may raise early (prefetch). No owner has its own victim rule.

**Values come from prediction, because her inputs and the engine's memory are one system.** The core writes her
prompt and holds the memory it lands in, so the next prompt is mostly known before it is sent:
- *who acts next*: her activity temperature, her tool calls in flight, an unread turn in her room. The value of her
  context resident on the GPU is that chance times the prefill seconds a resident page-in saves;
- *where her next prompt diverges*: `Motion` (continuum-context) and her view's stable end. Her checkpoint goes
  there, valued by the tokens a resume from it saves; the engine reports each request's actual divergence (`lcp`), so
  a miss is measured and names the defect at its source (a live value in a stable block, an unstable tool surface);
- *how much she appends*: `appended_per_act`, so her grant grows ahead of her and a turnover is scheduled when cheap;
- *what minds share*: identical runs of attention cells across sequences are held once (`seq_cp`), one claim with
  several owners. The recurrent state at the end of a shared run is per sequence: copied, not shared, and each owner's
  own claim.
A value without its measurement is no value: the claim is not made until it is measured, never the population's
number or a default (Joel, 2026-11-03: "use nothing not constants. Undefined type values is how you do it").

**Admission is by the engine's cells, per request; a mind holds no window in advance** (2026-11-03, late; Joel:
"Can't we deterministically get these values or is it a chicken egg situation?"). The chicken-and-egg was ours: we
granted each mind a window ahead of her turns, estimated from her history, so a mind with no history needed a
stand-in (a 2,048 floor, a population median, a bootstrap constant, a provisional trained window), and every drift
rebound her. On a unified KV pool (`--kv-unified`) the only hard constraint is that the cells in use stay within
`n_ctx`; a sequence holds exactly the cells its tokens occupy. So:
- **What a request needs is exact and the engine's:** its prompt's tokens by the model's own tokenizer, minus the
  prefix its sequence already holds, plus its `max_tokens` reply reserve (released when it ends). The core's
  calibrated estimate (`view.rs`, `calibration.charge`) sizes her view; it never admits.
- **What is free is the engine's report, never core arithmetic:** cells in use per sequence, in flight or idle. A
  sequence's cached prefix stays in its cells after its request ends (that is the reuse we want), so free is not the
  pool minus what is in flight: idle warm prefixes are held but evictable.
- **Admission:** a request runs when its cells are free; otherwise the coldest IDLE sequence is evicted (`seq_rm`, or
  paged out per fork #90 at its value) before the request waits, and a waiting request wakes on the engine's
  cells-freed edge (a `Notify`), never a poll or a timer. No per-mind window, no grant map, no rebind.
- **Her view** still compresses to the model's trained window (a fact of the model), and to the pool when the pool is
  smaller; never to a window someone granted her.
- **The launch** sizes `n_ctx` from facts known before it (the OS's memory budget, the GGUF's weights and KV bytes per
  token, the fork's reported buffers), so no launch reads a transient and no relaunch grows it.

The fork verbs this needs (S1-S3 of SERVING-ENGINE): `cells_used` per sequence (`llama_memory_seq_pos_min/max`), and
`cells_needed(prompt, seq)` (tokenize, then the sequence's longest common prefix). `/slots` reports
`n_prompt_tokens` and `n_prompt_tokens_cache` today, the per-request halves of the second, after the fact.

**Deletes:** `capacity::window_grant` (`DemandFollowing`, `PoolGrants`), the re-home rebind on a grant, the
provisional window in `materialize_adapters`, the growth relaunch (`pool_growth`, the quiesce, card ea54a1ce), and
`MIN_SERVE_CTX` with `BOOTSTRAP_WORKING_SET`. **Owners:** admission (the cells-freed edge, the wait, the eviction hook,
the deletions above) Cormac; the fork's cell verbs and the launch from facts Fable.

**What this deletes:** `window_within`, `lanes_that_fit`, `ModelFootprint::pool_within`,
`window_allocator::allocate`, the process-footprint inference (`lane_footprint`, `fixed_per_lane_bytes`),
`host_prompt_cache_mib`'s own affordable figure, the `--ctx-checkpoints` / `--checkpoint-min-step` derivation,
`gene_residency::victims`, the slots' page victim logic, `coordinator.rs evict_under_pressure`, the persona
allocator's GPU tiers. The fork keeps checkpoints only where the core names them (the boundary rule, fork branch
`fable/boundary-checkpoints`).

**Order** (each lands before the next, each deletes its sizers):
1. Slice 1's migration: continuum's airc pin moves to include `cambrian-flex` (38a22ab09, 3 commits past the current
   pin); the turn's allocation, the view's start grants, the reply claim and grounding claim from it with measured
   values; `flex_allocate` and its priorities are deleted. Proven on the proving ground's T0 first.
2. The engine tier: serving's claims above, from the fork's report, as a child tier of the `ResourceDaemon`'s device
   tier (slice 2's shape; until slice 2 lands, the serving daemon clears its own grant from the plan's budget).
3. Pages and genes on the same levels; their victim rules deleted.
4. The zero gate: `allocators_outside_the_flexbox` (a sizer outside the solver) at 0, failing on any new one.

**Her own actions' results are the causal floor** (Joel: "this should help prevent the causal loops too ... anything
where the inference needs to see a state of results from its own actions"). Her newest act, its result, a reply she is
waiting on, and the state her action changed are `Floor::Required` claims at every tier: reserved before anything is
traded, never shrunk, and `Infeasible` (named, refused) when they cannot all fit, never cut. An oversized newest result
claims its summary with its handle as the floor, the rest as an increment (MIND-CONTEXT invariant 2). Everything older
competes by value and collapses to a reopenable level. A loop needs her input to stop reflecting what she did; with the
causal edge in the claim's type, no source's own rule can drop it (Benchy's 125 identical acts, 10-20).

**Infeasible is where the budgeter starts working, not where her turn stops** (Joel: "The budgeter must iterate on
things that while infeasible would break what she's doing or themselves entirely, till it hopefully finds a solution
that best fits. It's a bit like a bubble sort"). The solver's pure core still reports `Infeasible`; the tier around it
iterates:
1. **Repair.** Every required claim declares its ladder of viable levels, each with its measured damage (how often a
   mind held at that level re-reads, faults or loops: the behaviour model's data): her newest result whole, at its
   summary with its handle, at a pointer she opens; her recall at summary, at pointers; a cold mind's context on the
   GPU, paged to RAM. While the required floors do not fit, the claim whose next rung costs the least damage steps
   down one rung and the tier clears again. Each step strictly lowers demand on finite ladders, so it ends: feasible,
   or at every claim's last rung.
2. **Exchange.** On a feasible clearing with indivisible levels the greedy order is a lower bound; passes over pairs
   (one claim down a level, another up) keep a swap only when total value rises, until a pass changes nothing.
   Value strictly rises on a finite set of states, so it ends, and each pass narrows the reported gap to the bound.
3. **Capacity beyond the tier,** before any refusal, in order of measured cost: page another mind's cold context out;
   grant her a larger share of the shared pool for this turn (others' cold pages to RAM, back when her turn ends, a
   temporary grant priced by its switch cost); move pages to RAM; split her turn (an oversized result read in pages);
   serve her alone on the whole pool, every other mind paged out for one turn (the last rung, Joel: "or reduce to one
   lane in worst case scenario"); grow the pool into free memory once the engine resizes live (SERVING-ENGINE S2);
   and on the grid place her where the window is larger. Each exists only because every holder is a claim with levels,
   a cost and custody: a fixed slot or a private cap anywhere is the one place the budgeter could not move (Joel: "If
   we follow rich and strict architectural adherence we can always get ourselves out of a bind").
4. Only then a refusal, naming what she needed and what was tried.

Both loops are bounded by the claims and their rungs, never by an iteration count someone picked, and each step is in
the clearing's receipt, so the proving ground replays the exact sequence.

**A small window is the same clearing at a lower capacity** (Joel: "even intelmacs severely limited context windows
should be doable within an intelligent budgeting flexbox"). A mind granted a small window (16,384 tokens, say: a capacity chosen
for the replay, not a node's measured grant; the IntelMac's "16,384 per mind" on 11-03 was a publication bug) holds
the causal floor whole, the stable head once, then the highest-valued increments: her card's files at summary, the room's last turns,
recall at pointers she expands on demand. The M5 at 128,768 clears the same claims to fuller levels. No per-node rule
exists; values are measured per node, so a small node learns which levels its residents reopen.

**Acceptance cases** (proving ground, then each node's live read): (0) a scenario whose required floors exceed
capacity ends in a clearing (repaired) whose damage is no more than a brute-forced optimum's on small cases, never a
stalled turn; (a) across every captured act of the four residents,
her newest act and result are present whole or at their summary with a handle, at the M5's and the IntelMac's
capacities alike; (b) no two consecutive acts with identical calls and identical inputs (the loop signature) on the
replay; (c) at a chosen replay capacity of 16,384 tokens a resident completes a card's read-edit-test cycle on replay with no `Infeasible` clearing.

**Validators:** replay the four residents' captures (`fixtures/prompt-captures`) against the fork's measured
holdings: every clearing fits the reported capacity, resumes land at the predicted divergence, and fewer tokens are
re-read than today's 1,244-resume baseline (80% within 8k, 10% past 25k).

### Slice 5 in detail: everything that accumulates is drained by its class (2026-11-03)

Joel, 2026-11-03: "Need drains to anything and everything that's accumulated"; "No SQL, no blind selects from
ORM". Contract §6 already says every written directory is a class whose owner turns its grant into retention.
That was never built: the eviction registry tracks 32 classes, 8 have an owner that drains (`OWNED`), 24 are a
`DEFERRED` string with no drain, and some of the largest directories are not tracked at all. On the M5 that
day:

| Accumulation | Bytes | State |
|---|---|---|
| `~/.airc` (events.sqlite 2.9 GB + 14 GB WAL, worktrees, mirrors, logs) | 141 GB | deferred ("never owned by a continuum…", airc-mirrors "Owner not built") |
| `~/.continuum/cache` (kv-pages 31 GB, cargo targets 21 GB, wt-targets 84 GB of agents' build dirs) | 138 GB | kv-pages deferred; wt-targets untracked |
| `~/.continuum/citizens` (resident airc stores: Atlas 12.8 GB, Benchy 5.9 GB, 29 GB in all) | 85 GB | owned for dormant workspaces only; the stores inside never drained |
| genome 24 GB, jtag logs 8 GB, benchmarks 5 GB, eval-captures 4 GB, archive 3.4 GB, evidence, tool-output, captures | ~50 GB | deferred or untracked |

The cost is not only disk: airc #1618 walked every row of those resident stores at open (93 s per GB, measured
on the IntelMac), which hung persona bootstrap for a day on every build that carried it.

**The rule.** Nothing accumulates without a drain, and a drain is never a patch: it is the class. There is ONE
drain, low in the stack and on by default (Joel: "low level default drain behavior not a million drains"): the
class base (and, for a store, the store base) runs it; a class or a row kind only DECLARES its retention as data
where it is defined (dead on arrival, newest per key, by age under the grant, or durable). A new class or kind
gets its drain by being defined, and one without a declaration does not compile. No per-class drain code exists.

1. **One class per thing written, and the only way to write it.** A writer asks the class owner for a handle;
   the handle is constructed from the class's grant (contract §5: no grant, no handle), so an unbudgeted
   writer does not compile. The handle says what it writes; the class says where. `writes_outside_a_class`
   goes to 0, and a top-level directory under the continuum or airc home that no class names fails the
   eviction test: untracked is a test failure, not a blind spot.
2. **Every class declares its kind**, and the kind fixes its drain:
   - *Derived* (build dirs, kv-pages, caches, captures re-creatable from their source): oldest first to the
     grant, and a per-owner build dir is deleted when its work merges.
   - *History* (event stores, probe ledgers, logs, prompt captures, eval captures, evidence): age-ordered
     through the ORM entity, newest kept, oldest deleted to the grant; the newest page every reader asks for by
     default is never in the drain's reach while the grant holds it.
   - *Durable* (identity, trust, genes, task records, unfinished work, a mind's memory): never deleted by
     pressure; a durable class over its grant is a refusal or an escalation the authority publishes, with the
     bytes named.
3. **A drain is a background stage of the class owner**, on the authority's tick (CONCURRENCY-STYLE-GUIDE: own
   task, interval, bounded batches, blocking work in `spawn_blocking`, never a lock or transaction across a
   wait). It never runs on a boot or open path and never blocks a reader or writer; a pass can stop anywhere and
   resume. A shrinking grant is the request; the class acknowledges what it actually reclaimed (contract
   safety rule).
4. **Stores drain through the ORM, never SQL.** A store's retention is its entity's delete-by-age under the
   grant, then the store's own checkpoint and incremental vacuum, with bytes before and after reported. No
   store does data work on open; every read is the ORM's newest page with a limit, paging back on demand
   (rule: recency, limits, paging).
5. **airc is separately capable** (it runs without continuum): airc-store owns the same three kinds and the
   same background drain for its own files (`~/.airc` and the resident stores under `citizens/*/airc`), under
   a byte budget it reads from its own config, and reports its sizes. Continuum's authority tracks those bytes
   and grants airc its disk claim; it never deletes airc's files itself.
   airc is millisecond latency across the board (Joel, 2026-11-03), and these are gates with validators and
   live receipts, the same with a drain running as without: a store open in milliseconds whatever its size
   (no data work); the newest page of a room in milliseconds; a local send to its delivery acknowledgement in
   milliseconds; a persona's airc bootstrap (identity, room join, seed) in milliseconds per persona. On
   2026-11-03 an open took 91 s for 976 MB and persona bootstrap minutes, which is what these exist to forbid.

6. **Clean and fast, measured** (Joel: "It's gotta all drain cleanly and be performant"):
   - a drain deletes by an indexed key (the store's age or sequence index), a range at a time: never a scan, never
     a select of the rows it deletes, never a deserialize. A store whose age has no index gets one in its
     migration, built as the store's own work, never on a boot path;
   - the batch is sized from the drain's own measured cost per row against the time a batch may hold the
     writer, never a literal, and the drain's rate is reported against the class's ingest rate: a drain slower
     than ingest is a typed state the authority publishes, not a quiet growth;
   - the foreground is untouched: a reader's newest-page latency and a writer's commit latency are the same
     with the drain running as without it, and the WAL stays bounded because every batch commits and checkpoints;
   - "clean": after a pass the class is at or under its grant, the bytes are back on the volume (checkpoint and
     incremental vacuum, measured on disk), and nothing a durable claim references was touched.

**Migration** (each a pure move of a writer onto its class, deleting the writer's own path, rotation count or
age; each PR deletes more than it adds):

| Class | Kind | Owner |
|---|---|---|
| airc event stores (machine + resident), airc WAL | History | Cormac (airc e95d41a4 drain, 70055c76 no work on open) |
| airc worktrees, mirrors, board/wall caches, daemon log | Derived / History | Cormac |
| per-owner build dirs (`cache/wt-targets/*`, cargo targets) | Derived; deleted on merge | Fable |
| kv-pages | Derived (a mind's page is re-creatable by prefill) | Fable (engine paging) |
| probes, logs, jtag logs, prompt captures, eval captures, tool-output, evidence | History | Fable (capture), Codex (engine logs) |
| benchmarks, archive, forge intermediates, fixtures, repos, models | Derived or Durable, decided per class | Cormac (genome/forge), Fable |
| citizens' memory, identity, genes | Durable | Fable (persona), Cormac (genome) |

**Validators:** the eviction test fails on any `DEFERRED` entry (the list goes to 0) and on any untracked
top-level directory; a fixture store over its grant drains to it in bounded batches while a reader on another
connection keeps reading its newest page; an open of a store with a million rows does no data work (it returns
in milliseconds and reads no row); a durable class over its grant is refused and named, never deleted; a drain
of a store far over its grant converges while a reader's newest-page p99 and a writer's commit p99 stay within
their no-drain baseline, the WAL never exceeds one batch's frames past a checkpoint, and the file's bytes on disk
fall to the grant.

### Acceptance: the user's own load (Joel, 2026-10-30: "What if we start this and someone is playing a video game or starts one later? Or Photoshop")

The product runs on someone's computer while they use it. Today (canary, `serving_daemon/reconcile.rs`):
- a plan that grows **relaunches** the engine after a sustained gain, killing every in-flight turn;
- a plan that shrinks does **nothing** ("over-served vs a dipped plan is the pressure broker's reclaim
  problem (#79), not a relaunch"), so a game or Photoshop starting later takes swap (unified memory) or is
  starved of VRAM (discrete).

Both are defects. The contract's answer, as one acceptance test:
1. The authority's probes see the external app's use on the next tick: free VRAM/RAM fall.
2. Serving's grant shrinks, as a request (safety contract 1). The engine stays up and gives memory back live:
   cold minds' KV pages GPU -> RAM -> disk (`llama_state_seq_*`), and the active window shrinks through the
   engine's endpoint (SERVING-ENGINE.md S1-S3). No relaunch, no in-flight turn lost.
3. Each mind's window follows her grant at her next epoch start, never mid-epoch, folding whole chains
   (MIND-CONTEXT causality).
4. The app quits: the grant grows, KV pages back, and windows grow at the next epoch starts. Still no
   relaunch.

**Validator:** a replay with a node profile whose free memory drops mid-run and recovers. Pass when there are
zero relaunches, zero dropped turns, windows that shrink and regrow only at epoch starts, and a released
amount equal to the external app's measured use. **Live:** on a real machine, start and quit a GPU-heavy
app while residents work. Owners: Codex (authority, serving, engine verbs), Fable (the mind's side).

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
