# Grid Market Clearing — nested λ-pricing from the pager to the N-node mesh

Algorithm and measurement companion:
[Adaptive grid intelligence](ADAPTIVE-GRID-INTELLIGENCE.md) specifies contextual
metric vectors, budgeted escalation, zero-spend continuity, distribution units,
larger-model execution experiments and evidence for learning the allocation policy.

**Status:** design, agreed 2026-08-08 (Joel's ruling + BigMama's Lagrangian
formalization, same conversation). Mechanism companion to
[GRID-ECONOMICS-AND-AFFINITY-ROUTING.md](GRID-ECONOMICS-AND-AFFINITY-ROUTING.md)
(the thesis: cost scales with the deduped union of hot working sets) and
[../serving/GRID-EXPERT-SHARE.md](../serving/GRID-EXPERT-SHARE.md) (the depot,
whose manifest becomes the supply listing).

**Joel's ruling:** *"This is a continuum to N-continuum p2p mesh which will see
the sum of resources, demands and optimize across all nodes, both wants and asks
accumulated, eventually market/cost based. 'Value or cost' are usable concepts
even in our grid between two nodes and maybe three next week."*

**BigMama's formalization:** the pager is already this problem one scale down —
rate-distortion under a budget is a Lagrangian, and the multiplier **λ is the
price of a byte of residency**. *"If I express the pager's budget in VRAM units,
it stays a local trick; if I expose λ as a value-per-byte scalar, the same number
is what clears a two-node market, and later an N-node one."*

---

## 1. Why a market and not an optimizer

A central optimizer needs global state; it dies at P2P scale and at partition.
A market is the **distributed algorithm** for this problem class, with proofs:

- **Network Utility Maximization / dual decomposition** (Kelly). The global
  optimum decomposes into per-resource *shadow prices*; each node solves only
  its local problem against those prices. TCP congestion control is this math
  deployed at planetary scale. **Identical shape at n=2 and n=200** — Joel's
  "wants and asks accumulated."
- **Backpressure / max-weight** (Tassiulas–Ephremides). Route work toward the
  largest queue differential; throughput-optimal from *neighbor state only*;
  degrades gracefully when a node vanishes (the blackout property).
- **Proportional-fair scheduling** (how high-speed WiFi/5G shares airtime).
  Same dual problem, per-frame cadence — the existence proof that this clears
  fast enough for *live* experiences, not just batch.

**Division of labor:** NUM prices the steady state; backpressure handles
transients and partitions. They compose — prices for placement, queues for the
next dispatch.

## 1a. The numeraire: one currency, and every claim's own value over time

Joel, 2026-10-30: "find a way of pricing everything using the same metric. Experience is not as simple as raw RAM,
time, slots, context... everything has a cost. Killing lanes, not having context. Maybe it's all just time." And:
"It can be worthless in situations like an agent talking so late it's useless, like a stock future expiring. Those
follow an equation that's a time relation. So maybe everything can have its own function that takes a common
currency."

**The currency is weighted time to a verified outcome.** Every cost the system pays is time:
- killing a lane is the reload plus every in-flight turn's lost work;
- missing context is the acts spent re-reading;
- thrash is the re-prefill of each turnover;
- a cut write is the act it wasted;
- a page is its wait;
- the grid's cost is latency.

Time is measured the same way on every machine. Two refinements:
- **Quality is time too:** the clock stops at a *verified* outcome (an edit applied and tested, a PR merged, a
  message answered; the typed outcomes of HOW-A-MIND-GETS-WHAT-SHE-NEEDS section 7), so a fast wrong act pays the
  time to discover and redo it.
- **Whose time:** the one input a person sets (which activity matters now, which conversation is live) weights
  whose seconds count for more. Everything else is measured.

**Every claim carries its own value function over time**, in that currency: what completing it is worth as a
function of when it completes. This is the time-utility function of real-time scheduling (Jensen and Locke, 1985;
utility accrual scheduling), and the reason prices move by themselves:

| Activity | Its value over time |
|---|---|
| A reply in a live conversation | high now, decaying to nearly nothing once the conversation has moved on (an option at expiry) |
| A voice or video frame | a hard deadline: full value until the frame time, none after |
| A person waiting on a PR review | steady, then falling as they give up and do it themselves |
| A refactor | flat for hours |
| Dreaming | low, never zero, flat |

**A resource's price is how far one more unit moves the claims along their curves.** A token of context, a
lane-second, a byte of state, a page move and a relaunch are each priced by the expected change in value they
cause: a unit that lets a live reply land before its curve falls is worth much; the same unit to a flat refactor
is worth little; a unit after a curve has fallen to zero is worth nothing, so the market stops spending on a
conversation that has moved on, with no rule written to say so.

**What is measured and what is learned.**
- The curves' shapes come from the activity: a conversation's decay is measured from how long people and peers
  actually wait before moving on; a deadline is the medium's frame time.
- The conversions ("how many seconds does this token of context save her") are learned by the policy loop from
  controlled probes (HOW-A-MIND L7), never guessed. An unmeasured conversion is marked unmeasured, and its claim
  trades only its floor.
- Gates stay outside the currency: privacy, permission and a live faculty working at all are never bought or sold
  (invariant 1).

**Why this currency works peer to peer** (Joel: "we need something like this especially for grid and definitely
for its p2p form"). A P2P grid has no one trusted in the middle, so the currency must be something each node can
measure for itself and no peer can forge:
- **Every node prices in its own measured time.** How long its person or its minds waited, and whether the
  outcome verified, are observed locally. There is no global ledger and no shared balance to forge.
- **A trade is judged by the buyer's measurement.** A peer's offer promises work by a time; the buyer measures
  what arrived, when, and whether it verified. That measurement is the settlement: promised against delivered
  becomes the seller's reputation (`capacity/settlement.rs`), and an over-promising peer's future offers are
  discounted by it. Reciprocity, as in BitTorrent's tit-for-tat, not a token: "market" stays coordination
  scalars between grid citizens (section "What we are NOT building").
- **Prices travel by gossip and decisions stay local** (invariant 2): each node clears against last-known quotes;
  a stale quote is unpriced, never free (invariant 7); a partition falls back to local clearing (invariant 5).
- **A peer's claim of value is never taken as given.** Only the buyer's own curve prices what it buys; a seller's
  price is an offer, discounted by its measured reliability.

**In the solver** (`cambrian-flex`, ONE-BUDGET slice 1), a claim's `Value` is in this currency, and its
increments are the marginal change in expected value per unit of resource, evaluated on its curve at the
current time. Re-clearing as time passes is what makes a stale claim's price fall to zero.

## 2. The nested market (one numeraire, three tiers)

The λ chain must ground in ONE value unit or the tiers silently fork. The
numeraire is the **recipe-owned experience score** (the composite objective:
gates × weighted terms — see task #371). Everything below it is a derived price.

```
ACTIVITY tier   recipe objective scores outcomes        → willingness-to-pay
                (benchmark: correctness×tok/s;            for LANE-SECONDS
                 livekit: latency, gated on TTS/render)
      │ price: value per lane-second
LANE tier       governor clears lane-seconds against    → willingness-to-pay
                demands (Σdemand loop, leases, QoS)       for RESIDENCY BYTES
      │ price: λ  (value per byte resident)
BYTE tier       pager selects {expert, tier, location}  → local λ per pool
                to maximize value under the byte budget   (BigMama's selector)
      │ price: λ + transport cost class
GRID tier       nodes exchange (supply manifests,       → work/bytes flow to
                λ, queue depths) via gossip               the cheapest capable node
```

Each tier only ever sees the price from the tier above and quotes a price to
the tier below. No component needs the whole picture — that is the property
that survives N.

**Hit-rate is the pager's local gradient, not the numeraire.** The pager's
reward input stays pluggable: v1 trains against hit-rate (measurable today),
and the governor later substitutes experience-derived value without touching
the selector. Never hard-bind λ's units to hit-rate.

## 3. Seams and types (n=2 today — lay the rails, don't play all the moves)

| Concept | Field | Lives on | Status |
|---|---|---|---|
| Bid (value side) | `value_per_lane_second` derived from recipe objective | demand rows in the governor's Σdemand loop | #371 builds the objective; field lands with it |
| λ (byte price) | `lambda_value_per_byte` | plan-file rows + PagerCaptureEvent | BigMama exposes it in the two-tier container selector; continuum echoes it per plan row |
| Cost (supply side) | `advertised_cost` (λ + transport class) | `DepotManifest` (#315, shipped 2026-08-08) | field addition, slice 2 of depot |
| Queue depth | pending-demand differential per lane | gossip snapshots (`capacity/gossip.rs`) | backpressure input; snapshot field addition |
| Clearing | governor Σdemand negotiation re-typed as price-based | `resources/governor.rs` + grid daemons | the ONE loop doctrine unchanged — it clears instead of splits |

Existing primitives reused, never paralleled: ledger `reserve`/`available_for`
(floors = infinite-price reservations), `ThroughputLease` (a cleared purchase),
placement scoring in `capacity/grid.rs` (the primordial price function),
`host_cache_lease` (a tier-boundary price), sim (`capacity/sim.rs`) as the gym
that proves clearing policies deterministically before any live node runs them.

## 4. Invariants (each is a learned rule)

1. **Gates are never priced.** Critical faculties (TTS, STT, render, serving
   liveness) are multiplicative gates in the recipe objective — no bid can buy
   them out, no price can sell them off. Markets allocate the *tradeable*
   terms only.
2. **No design may require global state.** If a decision needs any node to see
   the whole mesh, it will not survive N. Prices + gossip + neighbor queues
   only.
3. **Hysteresis on every cleared decision** ([[never-thrash]]). Prices move
   continuously; actuation is debounced (the downshift-streak pattern, amber
   phases). A price flicker must never re-home a citizen or evict a hot bank.
4. **Floors are reservations, not exemptions.** The embed-lane floor and its
   kin appear in the market as infinite-price reserved capacity — visible in
   the books, never tradeable (a faculty must never be starved by an auction —
   the "budgeter has all its parts figure it out" rule).
5. **Partition = backpressure mode, loudly.** When price exchange is stale
   (peer gone), nodes fall back to local clearing + queue differentials and
   emit a probe; they never freeze waiting for a quote.
6. **Every clearing decision is a probe.** Price, winner, loser, and the
   counterfactual margin — the RL layer trains on these receipts, and the
   operator console renders the market like the SCADA face it is.
7. **Unpriced is never free** (BigMama, found at S0 implementation depth,
   2026-08-08). λ = 0 means *no price published*, not *zero cost* — the wire
   type carries a `priced()` distinction, and clearing must treat unpriced
   capacity as unknown, never as a bargain. The failure this kills: a stale
   pre-λ binary in a mixed grid reads as free and every peer stampedes it.
   Corollary of the same ship: an actuator seam may land with its policy
   deliberately stubbed (her `select_tier` returns 0 **with a test asserting
   it**) — the price flows, tier choice waits for the learned policy under
   its quality guard, and the assertion is the tripwire if a policy ever
   arrives without one. A hand-tuned rule there would be a heuristic that
   *looks counted* — worse than the honest stub.

## 5. Slices (outlier method)

- **S0 — units (costs nothing now, the whole difference later).**
  BigMama: selector objective as value-per-byte, λ exposed (in flight with the
  two-tier container). Continuum: λ echoed on plan rows + capture events.
  *Exit: λ visible in PagerCaptureEvent on a real serve.*
- **S1 — single-node clearing (outlier A).** Governor's Σdemand loop re-typed:
  demands carry bids (v1: derived from recipe class constants), grants carry
  clearing price. Behavior identical by construction on one node — this is a
  units refactor with receipts. *Exit: serving.plan probes carry price fields;
  sim scenario replays byte-identical decisions.*
- **S2 — two-node price exchange.** DepotManifest grows `advertised_cost`;
  gossip snapshots carry λ + queue depths; placement consults the peer's price
  before fetching. *Exit: one real M5↔5090 placement decision flips because a
  PRICE said so, with the probe to prove it.*
- **S3 — n=3, real clearing (outlier B).** Two nodes can fake a market with a
  special case; three forces contention — an outbid party, a price that moved
  something. If the seams survive n=3 unbent, they survive N. *Exit: a
  three-way placement under contention where the sim predicted the winner.*
- **S4 — learned bidders.** Recipes' bid functions and the pager's λ policy
  trained in the sim-gym against experience outcomes ([[benchmarks train the
  escalation policy]] — the crisp-reward activities calibrate the bidders the
  fuzzy ones inherit). *Exit: learned policy beats the hand-set constants on
  the north-star metric (experience ÷ cost per node added).*

## 6. Ownership

- **BigMama:** λ-exposed selector + two-tier container (in flight); 5090-side
  S2 validation; NUM literature pass (separate piece, after the container).
- **M5 (me):** plan-row λ echo, DepotManifest cost field, governor bid/price
  typing (S1), gossip queue-depth fields, sim clearing scenarios.
- **Joel:** the numeraire is his call at every escalation — market rules are
  policy, and policy changes are surfaced, never silently retuned.

## What we are NOT building

- No token/currency/blockchain — "market" means prices as coordination
  scalars between trusted grid citizens (GridTrustAuthPolicy scope), nothing
  financial, nothing adversarial-by-design (that's the public-mesh question,
  years away, docs/papers/GRID-DECENTRALIZED-MARKETPLACE.md territory).
- No auction protocol chatter on the hot path — prices ride existing gossip
  cadences; clearing is local arithmetic against last-known quotes.
- No repricing of the RTOS floor — cadences, watchdogs, and gates stay
  schedule-driven; the market allocates capacity *between* activities, never
  inside a faculty's real-time loop.
