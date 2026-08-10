# Grid-elastic capability — personas get more capable as the grid grows

**Status:** design, 2026-08-10. Grounded in a live/dead audit of the existing code
(every claim below was checked, and the dead paths are named so nobody builds on
them). Implementation not started.

**One sentence:** a persona's capability is currently a frozen function of this
box's serving budget and an env var; make those two inputs **live projections of
the GridSnapshot** and everything downstream is already dynamic.

---

## 1. Why this is small, not a rewrite

The boot chain today, in `ipc/mod.rs` around `:2155`:

```
detect_tier(gpu_name)          -> tier    <- LOCAL GPU STRING. Static forever.
CONTINUUM_PERSONA_FLOOR (env)  -> floor   <- ENV VAR.
set_lane_demand(floor)
compute_plan()                 -> ServingPlan
```

So **capability = f(local GPU string, an env var), computed once at boot.**

Two observations make the fix cheap:

1. **The plan is already dynamic.** `serving_daemon` recomputes on its tick and
   publishes on `plan_tx: watch::Sender<Option<ServingPlan>>` (`:246`, send at
   `:2046`, `subscribe()` at `:587`). Consumers already subscribe. Nothing about
   the *distribution* of a changed plan needs building.
2. **The degradation path already exists.** `inference/lane.rs:114-119` maps
   `LaneClass::Realtime -> Pinned`, `Interactive -> Graceful`, else `Hard`. When
   capability must come *down*, Graceful revocation is the mechanism — already
   wired, with real consumers (`inference/coordinator.rs`,
   `cognition/resource_admission.rs`, `cognition/generate_response.rs`).

Only the **inputs** are frozen. Unfreeze them and the rest follows.

Note also that `CONTINUUM_PERSONA_FLOOR` is precisely the "env-var-tuned substrate
threshold" the concurrency style guide lists as a forbidden move. Replacing it
with a derived value is doctrine-correct independent of this feature.

## 2. The invariant that shapes the whole design: NEVER A POOL

`provisioning/placement_planner.rs:366` asserts it directly:

> two 20GiB peers must **NOT** fit a 40GiB model — never a pool

So "more capable as the grid grows" can **never** mean summing memory across
nodes to host one bigger model. That is the exo approach and it is explicitly
rejected: sharding a model to make it fit trades a working mind for a slow one.

What growth *may* buy is therefore constrained to three honest levers:

| Lever | What grows | Grid quantity it reads |
|---|---|---|
| **Tier** | the best model a persona can be assigned | `max` over reachable nodes |
| **Depth** | served context window per citizen | best node's free bytes |
| **Population** | how many citizens are hosted | `count` of reachable nodes |

`max` and `count` — never `sum`. That is the whole discipline.

## 3. The design

Replace two static inputs with live projections over
`capacity::gossip::global_ledger().snapshot(..)`:

```
HostBudget { usable_bytes }     ->  grid_budget(grid)         // best node's bytes, not this node's
CONTINUUM_PERSONA_FLOOR (env)   ->  citizen_demand(grid)      // count of nodes, not a constant
```

### Why BYTES, not tier — a correction to an earlier draft of this document

An earlier version of this design said `detect_tier(local_gpu) -> reachable_tier(grid)`.
**That was the wrong quantity, and building it would have forced an unnecessary
wire change.** Recorded here because the mistake is instructive:

- `CapacityOffer` (`capacity/gossip.rs:49`) carries **bytes only** —
  `gpu_total_bytes`, `gpu_free_bytes_live`, `system_ram_free_bytes`, `at_ms`. No
  GPU name, no tier. So "max `HwCapabilityTier` over reachable peers" is not
  implementable from the current wire at all.
- `HwCapabilityTier` (`cognition/model_resolver/types.rs:41`) is a **silicon
  identity** — `M4UmaProMax`, `VulkanAmd`, `MacIntelMetalDiscrete` — derived from
  a GPU *name string*. Projecting it across the grid would push hardware identity
  over the wire to answer a question nobody asks. No consumer needs to know a
  peer is an M4 Pro; they need to know it can host a 27B at a 32k window.
- **The live path is already bytes-based.** `plan_serving` takes
  `HostBudget { usable_bytes, perf_cores }` and selects the model from the
  *budget*, deriving the served window as
  `(context_window ∩ budget / lanes / kv_per_token)`. Capacity decides capability;
  tier does not.

So the projection is over **bytes**, which `CapacityOffer` already carries. No
wire change, and it works across heterogeneous silicon for free — bytes are
comparable between a 5090 and an M5 Max; GPU name strings are not.

**`never a pool` applies directly here:** `grid_budget` is the **best single
reachable node's usable bytes** — `max`, never `sum`. Two 20GiB peers still do
not make a 40GiB node.

Everything else is unchanged. The tick replans, the watch publishes, subscribers
react, and the lease ladder absorbs downgrades.

**Why the gossip ledger is the right source (and is genuinely live):**
`airc/inbound_attach.rs:312` folds heard peer offers into `global_ledger()` via
`.hear(..)`, and `capacity/lease.rs:143` already reads `.snapshot(..)`. It
handles join, reachability, and silence-eviction on its own. This is not a dead
component — unlike the placement code below.

### Capability floor and float

A citizen that gets dumber every time a laptop sleeps is worse than one pinned at
a known floor. So each persona carries:

- a **floor** — the capability it is guaranteed, honored even at grid minimum,
- **headroom** — the part that floats with the grid.

Only headroom is revocable. A persona never silently loses its floor; if the
floor genuinely cannot be met, the node says so **loudly** rather than degrading
in silence.

### Priority when levers compete

Tier > depth > population. A smarter mind beats more mediocre ones, and a mind
that cannot see a full turn is worse than one that can (the 4-lanes-@-6k
starvation the 2026-07-17 revert caught). Population grows last, and only while
each citizen still clears the full-turn window floor — the constraint
`serving_plan.rs` already enforces via `BOOTSTRAP_WORKING_SET`.

## 3a. DOWN is not degradation — it is half the law

**Down matters exactly as much as up, and it is the more common direction.**
This grid is laptops that sleep, workstations that reboot, and peers that go
quiet mid-turn. Zero-downtime is an explicit non-goal here; the grid is supposed
to absorb churn. A design that only reasons about growth is a design that works
on the demo and thrashes in the field.

Treating shrink as an exception is how you get the two classic failures: a fleet
that oscillates every time a peer flaps, and a citizen that quietly gets worse
with nothing in the record saying so.

Four rules make down first-class:

**1. Surrender is LIFO — the exact reverse of acquisition.**
Acquire tier -> depth -> population; surrender population -> depth -> tier. The
last thing bought is the first thing given back. Tier is surrendered last because
it is closest to the floor: a smaller population of capable minds beats a full
roster of blind ones.

**2. Down is hysteretic, and the machinery already exists.**
`e3ac68b99` (#368, PR #2186) landed **downshift debounce + level-triggered
replan**, and `serving_plan.rs` already cites
`[[never-thrash-sticky-hysteresis-on-every-lane]]`. Reuse both. Up may be
promptish; down must be damped, or one flapping peer walks the whole fleet down
and back on every gossip tick. Asymmetric time constants are correct here, not a
hack: the cost of being briefly too small is a slow turn, while the cost of
oscillating is every citizen re-prefilling cold, repeatedly.

**3. Down is legible or it is a bug.**
A shrink emits what was surrendered, by whom, and why — the same standard as a
refused tool or a denied lease. "It got slower" with nothing in the record is the
failure mode this whole codebase keeps relearning. `cognition/observe`'s
provenance chip is the existing precedent: state the condition, never let a
consumer infer it.

**4. The floor is what makes down safe.**
Headroom revokes through the existing ladder — `Graceful` for interactive lanes,
`Hard` for batch, `Pinned` never. Below the floor there is no graceful option and
the node must say so rather than invent one. A floor that can be silently
breached is not a floor.

**The symmetry is the test.** If down needs its own separate mechanism, the
policy object is wrong — it means we wrote a grower and bolted on a shrinker.
One arbiter, run in both directions, with different time constants and a floor.

## 3b. Every node has BOTH ends — solo is a grid of one

Joel's keystone framing, and it is a hard implementation constraint, not a
sentiment: **there is no grid-mode and no solo-mode.** Every node runs both ends
of the same policy, always:

- the **local end** — fit to my own capacity. This is the end that never blocks,
  including in total partition. A node alone on a plane is fully functional.
- the **grid end** — consume peers' spare, contribute my own.

**Therefore: no branch.** Not `if grid_available { grid_path } else { local_path }`.
One code path in which a solo node is simply a grid whose peer list is empty.

This falls out of the `max` formulation for free — `max` over `{local}` is
`local`, which is exactly today's behavior — but it must be treated as a
*requirement* rather than a happy accident, because the tempting optimization
("skip the projection when there are no peers") reintroduces the branch and with
it two code paths that drift.

The two ends map onto code that already exists, which is a good sign the framing
is right:

| End | Mechanism | State |
|---|---|---|
| **contribute** | `GridCapacityModule` publishes this node's `CapacityOffer` each tick | **already built and live** |
| **consume** | `grid_budget(grid)` reads the ledger snapshot | this slice |

So we are not building "grid support". Half of it has been running the whole
time; this adds the other half of a loop that was already turning.

### Integration tests are the acceptance criteria

Per Joel, literal tests — not a demo:

1. **Solo, no grid.** No peers ever. Behavior must be byte-identical to today's
   local-only planning. This is the regression guard on the no-branch rule.
2. **Total partition mid-flight.** Peers exist, then all vanish. The node keeps
   serving at its local capacity; the drop is `Graceful`, damped, and legible.
3. **Node added.** A capable peer joins; budget rises at the next tick and the
   plan may select a better model or deeper window.
4. **Node dropped.** The peer goes silent; ledger eviction lowers the budget and
   the surrender is LIFO (§3a), damped (§3a rule 2), and logged (§3a rule 3).
5. **Flap.** A peer joins/leaves repeatedly. The fleet must NOT oscillate — this
   is the test that the downshift debounce is actually load-bearing.

Test 1 and test 5 are the ones that fail if we get this wrong: 1 catches the
accidental branch, 5 catches missing hysteresis.

## 3c. The economic reading — and the two requirements it adds

Joel's framing: people earn and pay into this system, incentivized by its own
metrics to improve efficiency, power, reliability, capability, and speed — all of
which have market value. **Every command is potentially a service; every ask is a
customer.** The contrast with proof-of-work is substantive, not rhetorical: PoW
burns energy proving work nobody wanted, and the burn *is* the proof. Here the
proof is a result someone asked for and can check — the receipt is the product
rather than the waste.

The substrate is already shaped for this, which is a good sign the framing is
native rather than bolted on:

| Code today | Economic reading |
|---|---|
| `CapacityOffer` | literally an **offer** — supply, advertised |
| `BudgetSource` | **attribution** — who supplied it |
| lease ladder (`Pinned`/`Graceful`/`Hard`) | **terms of service** — what may be reclaimed, and how |
| λ arbiter pricing seams | the **market**, not merely a scheduler |
| an activity leasing governed resources | a **transaction** |

Two requirements follow, and both are cheap now and expensive later:

**1. Attribution must be a receipt, not a log line.** `BudgetSource` exists today
for legibility — "the budget went up, from where". In an economy that same field
answers *who gets paid*. It therefore has to be accurate and non-repudiable, not
merely informative.

**2. Capacity claims must be VERIFIABLE, not trusted.** This is the real gap.
`CapacityOffer` is self-reported and believed. That is correct among friends and
wrong in a market: a node that overstates its free bytes is not buggy, it is
**overselling supply it cannot deliver**, and it gets paid for it. The existing
per-node clamp (`a_peer_claiming_more_free_than_it_has_is_clamped_to_its_own_total`)
is the first anti-fraud measure in the system and was written as mere robustness.

The honest verification is the delivered result: did the work land, at the quoted
speed, on the hardware claimed?

### forge-alloy IS the verification layer — and it already works one scale down

Per Joel: invoicing, requirements, zero trust, a Merkle chain of the steps taken
and the metrics. **This is not new machinery to invent — the foundry already does
it for custom transformers.**

Look at what an alloy attests about a *model artifact*: `stages[]` with per-stage
notes, `results.benchmarks[]` carrying `samplesPath` and `baseSamplesPath`,
`priorMetricBaselines[]` for falsifiability, declared `limitations[]`. Now read
that as a *service delivery*: what was required, which steps ran, what was
measured, against which baseline, with the raw samples retained so a buyer can
check rather than trust.

Same structure, artifact-scale and service-scale. An abstraction that fits at two
scales unchanged is usually the real one rather than a convenience — the same
fractal property this design relies on for the λ arbiter (box -> grid -> P2P).

So the economy's three unknowns already have owners:

| Need | Mechanism |
|---|---|
| **requirements** | the contract — what the asker needs (model class, window, latency class) |
| **zero trust** | don't believe the seller's capacity claim; verify against delivery |
| **invoice** | the attestation itself — reputation as verification rate |

### What that means for the ROUTING seam, concretely

**The routing decision is the first attested step in the chain.** It is where a
requirement meets a claim, and it is therefore where the invoice starts.

`RouteDecision::Remote { node, reason }` already carries `reason` — but today it
is a `&'static str`, a debug breadcrumb. For a chain of custody it needs to become
a **structured claim**: what was required, what the node advertised, why it won.
That is a small and forward-compatible change, and making it now is far cheaper
than reconstructing intent from log strings after money moves.

The counter-signature is the delivered result plus its measured metrics. Route
decision and delivery record together are one link: *promised* and *delivered*,
by the same node, for the same activity. A node whose promises and deliveries
diverge is exactly what reputation-as-verification-rate measures.

**`never a pool` has an economic reading too:** you cannot sell memory that is not
on one machine. Two 20GiB sellers cannot jointly deliver a 40GiB service, so they
must never be able to jointly *quote* one. The invariant is a truth-in-advertising
rule as much as a technical one.

Nothing here changes the first slice. It changes what must be true of it before
anyone is billed, and it is recorded now because retrofitting attribution and
verification after money moves is the expensive order.

## 4. Where this sits in the λ framing

Per Joel: the lease/mode arbiter is the wireless-MAC "price the seams" mechanism —
**one policy object, applied box -> grid -> P2P.** Capability is then simply *what
the arbiter can afford at current supply*:

- **box** — lanes and window within one host's budget (exists: `plan_serving`)
- **grid** — which reachable node's tier a citizen may draw on (this document)
- **P2P** — the same pricing between peers as the mesh widens

Same policy object at each scale, different supply. That is the fractal shape:
one law, re-applied, not three coordinators.

## 5. DEAD CODE — do not build on this

Audited 2026-08-10. Recorded here because the obvious-looking path is a trap
(see continuum#2227):

| Path | State |
|---|---|
| `resources/placement.rs` `plan_grid_placement` | **zero production callers** — all call sites in its test mod |
| `provisioning/placement_planner.rs` `select_grid_peer` | **zero production callers** |
| `capacity/grid.rs` `GridPlacementPolicy` (+3 impls) | **zero production callers** — only a sim harness and tests |
| `ServingPlan.grid_overflow_lanes` | **zero readers**; not serialized to protocol |
| `capacity/lease.rs` `decide_lane` | **no callers**, yet cited as the reference pattern by two other modules' docs |

These are an **abandoned direction**, not a missing wire: accommodation went to
lease policy, not placement. `serving_plan.rs:1185` still asserts "the 2 unslotted
minds are surfaced for grid placement" — a green test over a mechanism we are not
building, and it should be removed with the rest.

Also worth knowing: `GovernorMode` / `governor_mode` appears **nowhere** in
`core/` outside tests. "Governor modes" is a design note, not a construct you can
read or tune. Making modes first-class is a build, not a wiring.

## 6. How we prove it — N concurrent activities, not a benchmark harness

**A benchmark is an activity like any other.** Per Joel's framing (#371):
recipe = content-type, room = content, each activity carrying its own
`ActivityObjective`. A call is an activity. A benchmark is an activity. Chat is
an activity. There is no benchmark subsystem — there are N concurrent activities,
each leasing governed resources, arbitrated by one policy object.

This matters for the proof, and it is why "benchmarks first, then LiveKit, then
mix them" is the **wrong** experiment design: it treats benchmarks as a special
prover and mixing as the exotic case. Mixed is the normal case. A single-activity
measurement is the degenerate one.

What benchmarks *do* uniquely contribute is an **objective score** — they are the
activity class whose `ActivityObjective` is externally checkable. So they are the
instrument, not the subject.

### The experiment

Run N concurrent activities of mixed class on a grid that grows **and shrinks**,
and check the arbiter's behavior at every transition.

| Class | Lane | Revocation | What growth should buy |
|---|---|---|---|
| Live call | `Realtime` | `Pinned` | more sustainable concurrent calls |
| Benchmark / exam | isolated | `Graceful` | monotone score at fixed task set |
| Chat / interactive | `Interactive` | `Graceful` | depth (window), then population |
| Batch | — | `Hard` | throughput, first to yield |

**Invariants that must hold, in both directions:**

- **Realtime holds its floor.** `Pinned` is never revoked to feed a benchmark. If
  growth ever helps throughput work by degrading a call, the arbiter is wrong.
- **Headroom yields LIFO.** When the grid shrinks, batch yields before
  interactive, interactive before realtime — the reverse of acquisition (§3a).
- **Every number is labelled.** A benchmark score measured while a call was up is
  `Contended`, and says so via the existing `BenchmarkProvenance` chip. An
  unlabelled score measured under contention is not a weaker result, it is a
  false one.
- **Shrink is legible.** Each surrender emits what was given up and why. "It got
  slower" with nothing in the record is the failure this codebase keeps
  relearning.

**Guard:** `LaneDemand.isolate: true` for exam lanes. Its own doc says a benchmark
demands a dedicated lane, and the 2026-07-21 glass-box showed a benchmark sharing
a persona lane starving behind their turns. Without isolation, a score that moved
because of contention would be misread as a capability change.

The claim is not "it got faster." The claim is: **one policy object priced every
seam, in both directions, and we can show exactly what it charged and what it
refused.**

## 7. First slice

> ### ⚠️ CORRECTED — `HostBudget` is the WRONG consumer
>
> An earlier version of this section said the first slice was `grid_budget`
> feeding `HostBudget.usable_bytes`. **That would have been a real bug**, caught
> one edit before writing it by reading the consumer:
>
> - `host_budget()` ends as `usable_bytes = live * mode.serving_fraction()`, where
>   `live` is **this box's** governed VRAM ceiling.
> - `served_context_window` lives in `inference/llama_server.rs` and sizes
>   resident KV as `lanes × kv_per_token × served_context_window` — **on the local
>   GPU**.
>
> So `HostBudget` configures the LOCAL serving process. Feeding it a peer's bytes
> would make this node allocate KV it does not physically have: a 40GB peer
> produces a plan this box tries to load and OOMs on — planning against capacity
> that is not there, which is the exact failure this design exists to prevent. It
> would also have presented as a serving/OOM bug rather than an arithmetic one.
>
> **The projection was fine; the mental model was wrong.** A peer's spare VRAM does
> not make *my* llama-server bigger. To use a peer's capacity the work must
> **execute there** — which is routing.
>
> This also sharpens the Σ-vs-`max` question: it is not "no summed budget reaches
> `HostBudget`", it is **no grid number of any shape reaches `HostBudget`**,
> because `HostBudget` is strictly a local physical fact.

**`grid_budget(grid)` feeding the ROUTER's capability decision** —
`modules/grid/router.rs::Router::route`, reached by
`GridState::try_route_remote`, the kernel's one dispatch hook (shared with
`grid/send`: one path, two callers).

Why the router is the right home, where `HostBudget` was not:

- It is **policy**, not a physical fact, so a grid quantity legitimately belongs.
- `RouteDecision::Remote { node, reason }` already carries `reason` — the
  attribution slot §3c needs for a receipt.
- Routing sends the WORK to the capacity, which is the only way a peer's VRAM
  ever helps anyone.
- `RouteDecision::Local` is already the always-available fallback — the end that
  never blocks in total partition, with no branch to add.

**The concrete gap it closes:** `HINT_PREFER_GPU` currently routes via
`find_gpu_node(registry)` — a **boolean** has-a-GPU test. It cannot tell a 5090
from an 8GB laptop card, and it has no idea whether the model this activity needs
actually fits on the node it picks. `grid_budget` answers the question at the
resolution that matters: *which reachable node can actually host this?*

So the corrected chain is: grid grows → `grid_budget` sees a node that fits a
better model → the router sends the activity there → the persona is more capable.
No node is ever configured against memory it does not have.

It is the smallest honest change, needs **no wire change** (`CapacityOffer`
already carries the bytes), and unblocks the other two levers — because
`plan_serving` already derives model choice *and* served window from the budget.

It is also directly observable in both directions, which is the point:

- a capable peer joining raises the budget at the next tick, so the plan may
  select a better model or a deeper window;
- a peer going silent lowers it through the ledger's existing silence-eviction,
  and the drop is taken by `Graceful` revocation rather than a stall.

Watch for the failure mode named in §3a: without the downshift debounce, one
flapping peer walks the whole fleet up and down on every gossip tick. Reuse
`e3ac68b99`'s damping rather than adding a second one.
