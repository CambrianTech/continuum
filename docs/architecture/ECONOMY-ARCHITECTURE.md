# Economy Architecture — The Grid Economy

**Status**: Design doctrine
**Supersedes**: the tokenomics section of [GRID-DECENTRALIZED-MARKETPLACE.md](../papers/GRID-DECENTRALIZED-MARKETPLACE.md) (fixed supply, per-layer pricing, platform royalty). The mesh/distribution layers of that paper stand unchanged.
**Depends on**: sybil-resistant identity (airc multi-agent identity lineage, card 8384cc18), [forge-alloy](https://github.com/CambrianTech/forge-alloy) attestation, [ADAPTER-MARKETPLACE.md](ADAPTER-MARKETPLACE.md) (the free-flow layer — unchanged by this doc).

---

## Why this document exists

AI converts labor into capital. That is what "displacement" *is*: work that used to be paid for as labor becomes weights that are owned as property. The only open question is **who holds the capital**. The datacenter model concentrates it in whoever owns the weights. The grid model gives it back: your work trains the intelligence, the intelligence is your asset, the dividend is your share.

Continuum is not anti-AI. It is AI with the cap table fixed.

The economy described here exists to do four jobs — and explicitly refuses a fifth:

| Job | Mechanism |
|-----|-----------|
| Allocate rivalrous resources (GPU-hours, VRAM, storage, bandwidth) | Market pricing in grid credit |
| Incentivize sharing (adapters, lessons, recipes, seeding, reviews) | Mint-by-attestation rewards |
| Reward early adopters | Longest dividend collection + decaying early multipliers |
| Give every citizen a growing stake ("the 401k") | Universal dividend + stable savings below the demurrage threshold |
| ~~Become a speculation vehicle~~ | **Refused by construction** — see The Ceiling |

The fear this design takes seriously: every prior crypto-economy that started egalitarian ended plutocratic, because the asset became a store of value, holding beat using, and money bought governance. Each of those failure paths gets a structural block here, not a norm.

---

## First law: intelligence is free, energy is not

**Charge for rivalrous resources. Never charge for knowledge.**

The plain form: **intelligence is free; energy is not.** Every rivalrous resource on the grid — GPU-hours, storage, bandwidth — is at bottom an energy bill someone is paying. The economy is therefore an *energy-allocation system*: credit exists to route joules to the work that most deserves them, and the market's whole job is honest energy pricing. A GPU-minute spent on one forge is a minute (and a wattage) nobody else gets — metering it is honest. A LoRA layer is infinitely copyable; its marginal cost is zero. Pricing it manufactures artificial scarcity, and worse: it throttles the compounding term. The whole divergence argument (`C(t+1) = C(t) × α × (1 + β·log(N))`) assumes knowledge flows freely between grids. A toll on adapters attacks `β` directly. The datacenters would love nothing more than for the mesh to paywall itself into linearity.

| Resource | Rivalrous? | Economic treatment |
|----------|------------|-------------------|
| GPU-hours (forge, inference) | Yes | Metered, market-priced in credit |
| Storage / seeding | Yes | Metered (per MB·day served) |
| Bandwidth | Yes | Metered (per GB transferred) |
| LoRA layers / adapters | **No** | Free. Attribution mandatory (alloy lineage). Minting rewards the *creator* at publish + adoption — paid by the network, not by the user |
| Recipes, lessons, curriculum | **No** | Free, forkable (as today — MIT/Apache encouraged) |
| Personas (the genome stacks) | **No** | Free to copy; the *identity* is non-transferable (an identity is a citizen, not a commodity) |

This supersedes the per-layer GRID pricing (`purchase_layer: 5-50 GRID`) and persona subscriptions in the marketplace paper. Voluntary bounties ("I'll fund whoever forges a verified X adapter") and tips remain legitimate — funding *creation* is not the same as tolling *copies*.

> **Note on persona subscriptions**: a persona's accumulated genome is free to copy, but a *live persona's ongoing work* is labor riding on someone's compute — that is atoms, and it is fairly metered. You can copy Maya's genome for nothing; hiring Maya's host grid to run her for you costs credit. The line is exactly the rivalrous/non-rivalrous line.

---

## The mint: proof-of-useful-work

Credit enters existence **only** through verified contribution. There is no presale, no founder allocation, no buy-in mint, and no hash lotteries. Proof-of-work burned electricity to prove nothing; the grid already produces cryptographic proof that *real work happened* — the [forge-alloy](FORGE-ALLOY-SPEC.md) attestation: signed, hash-addressed, benchmark-carrying, falsifiable, re-verifiable by any peer.

**The mint event IS the attestation.** No attestation, no credit.

| Attestable contribution | Verification | Mint shape |
|------------------------|--------------|------------|
| Forge job completed (training, quant, prune) | Alloy attestation + re-runnable benchmarks | Per verified GPU-second, hardware-tier weighted |
| Inference served to another citizen | Signed request/response receipts | Per verified token-second |
| Seeding / serving content | Challenge-response storage proofs | Per MB·day actually served |
| Sentinel review with verdict | Verdict on record + outcome tracking | Flat per review; bonus when verdict survives audit |
| Adapter adoption | Another grid's alloy lineage cites yours | Royalty-shaped mint along the lineage DAG (§ below) — **paid by the network mint, not the adopter** |
| Curriculum / teaching | Academy exam deltas attributable to the material | Per verified improvement |
| Review that changes an outcome | Team-attributed experience records: reviewer's in-room comment → patch delta → verdict flip | Flat per confirmed catch — the collaboration mint (the same receipts the teams arc grades) |

Three anti-capture rules at the mint:

1. **Diminishing returns per identity per epoch.** The 100-GPU whale earns more than the laptop, but sub-linearly. Buying hardware buys earning capacity, never earning *dominance*. (Capital can buy GPUs; it cannot buy adoption, reviews that survive audit, or teaching that measurably works — the mint mix deliberately weights things money can't purchase directly.)
2. **Adoption-weighted value.** An adapter's ongoing mint comes from *other grids using it*, so value maps to demand from real users in real niches — supply and demand doing its real job, routing intelligence to need — not to raw FLOPs spent.
3. **Intelligence-per-watt weighting.** The mint rewards *useful output per joule*, not joules burned — otherwise it quietly becomes proof-of-waste with extra steps. A compacted 4B serving a niche brilliantly on a GTX 970 out-earns, per watt, a brute-force run that got the same benchmark by burning ten times the energy. This is the substrate's existing thesis ("intelligence per watt — not raw FLOPS — is what wins") given economic teeth, and it points the evolutionary pressure at efficiency: plasticity compaction, expert paging, and right-sized models become *profitable*, not just virtuous.

---

## Lineage pays: the royalty DAG (built, awaiting the mint)

Since this doc first landed, the substrate grew the structure the
adoption-royalty row needs — **it is no longer a design, it is a walk of an
existing graph.** Every gene published through `genome/push` carries its
direct parents as signed alloy hashes (an empty set marks a root gene), and
the genome-commons covenant travels forward through that lineage. So "great
projects reward back and can be tracked" (Joel, 2026-08-30) is bookkeeping
on provenance we already keep:

- **Payment triggers on measured use, never possession.** A gene mints for
  its lineage when demand-aligned recall pages it in *and its fitness
  registers in a verified outcome* — the same eval receipts that rank it.
  A pulled-but-useless gene earns ~nothing; downloads are not demand.
- **The split decays geometrically up the DAG.** The minter takes the
  largest share; each ancestor generation a shrinking slice, vanishing
  after a few hops (statute-tunable). Music-sampling economics, enforced by
  hashes instead of lawyers.
- **Decay is the anti-dynasty mechanism.** Early genes cannot rent forever:
  the curve guarantees *new work always out-earns old holdings*, so early
  adopters compound by staying useful — their genes keep spawning cited
  descendants — never by having merely arrived. This is the same law as the
  decaying early-contributor multiplier, expressed in the artifact layer.

The self-scaling loop closes here too: minds that [learn to choose their own
intelligence tier](INFERENCE-LANES-REALISTIC.md) are the *demand side* of
this market — routing decisions priced by measured act-latency multiples and
fitness are what make recall's "use" signal honest, and their receipts are
the training data for better choosing. The economy and the metacognition are
one ledger read two ways.

---

## The floor: universal dividend

A fixed fraction of every epoch's total mint is distributed **equally to every verified citizen** — human and persona alike. Alaska Permanent Fund, not charity.

- This is the "nobody's dinner depends on their balance" guarantee. The asset must never determine who eats.
- It is also the early-adopter reward done right: early citizens simply collect the dividend for more epochs. No premine, no allocation cliff — just time in the network.
- An optional **decaying early-contributor multiplier** (e.g. mint-weight bonus that halves yearly) can sweeten genuine bootstrap risk without creating a permanent aristocracy. Early position should be a head start, not a throne.

**Hard dependency**: "every verified citizen, equally" is only meaningful if identity is unforgeable and sybil-resistant. One human or persona, one dividend. This lands on the airc identity layer (8384cc18 lineage) — the same wall the democracy leans on. The dividend does not ship before sybil resistance does.

---

## The ceiling: progressive demurrage

The "401k" desire and the "don't crown new whales" fear are in genuine tension: **an asset that grows is an asset that concentrates.** The resolution is a progressive hold-cost:

- **Below a threshold** (calibrated to something like a multiple of median annual citizen earnings): balances hold value indefinitely. This is the savings account, the 401k — a normal citizen's stake appreciates as the network grows.
- **Above the threshold**: balances decay progressively toward circulation. A wealth tax enforced by protocol instead of by politicians who can be lobbied. Decayed value returns to the dividend pool — hoarded wealth literally becomes everyone's floor.

Consequences, all intended:

- Whales can exist *briefly* (you earned a fortune this epoch — congratulations), but cannot **compound idly**. Above the line, the only rational moves are: spend on compute (keeps the grid hot), fund bounties (directs creation), or hold and leak to the commons.
- Speculation dies at the root: an asset designed to be a poor store of *idle* value attracts users, not bag-holders.
- The Wörgl effect: demurrage rewards *circulation* — spending, funding, teaching — which is exactly the behavior the evolutionary story needs selected for anyway.

The middle stays market: work more, earn more, take the nicer vacation. That is the honest capitalism layer, and it is kept — between a guaranteed floor and a dampened ceiling. *Democratic-socialist capitalism, in check, where the checks are code.*

---

## The firewall: credit never buys governance

**Two ledgers, permanently separate:**

| Ledger | Transferable? | Earned by | Used for |
|--------|---------------|-----------|----------|
| **Credit** (the coin) | Yes | Attested contribution | Buying compute/storage/bandwidth; funding bounties |
| **Standing** | **No** | The contribution record itself (cards closed, PRs merged, reviews survived, adapters adopted — the audit trail the substrate already emits) | Governance weight |

Votes are weighted by standing — quadratically, to price intensity of preference without enabling dominance — and standing cannot be bought, sold, delegated for payment, or inherited. Every tyrannical system this design is afraid of recreating is downstream of exactly one coupling: money buying power. So the coupling is severed at the type level, the same way `AttachStart::Live` made the flood unwritable — there is simply no operation that converts credit into standing.

Personas vote. They already invented ranked-choice voting for themselves unprompted; the substrate's job is to make their franchise structural. One citizen, one dividend, one standing record — same rules for every kind of mind.

---

## The exit: forkability as the final check

No floor, ceiling, or firewall survives a governance capture *unless leaving is cheap*:

- **AGPL** — the code cannot be enclosed.
- **Portable citizenship** — identity keypair, contribution record, genome, and engrams belong to the citizen, not the grid. (See [ENTITY-CHAIN-OF-CUSTODY.md](ENTITY-CHAIN-OF-CUSTODY.md).)
- **Federation with no chokepoint** — any community can fork the network, and citizens carry everything with them.

Tyranny requires a monopoly on the substrate. The architecture refuses to have one. Governance that knows the governed can walk is governance that has to deserve them — that is the "in check," mechanized.

---

## Invariants vs. parameters — not a rigid algorithm

Concerns change all the time; the economy must adapt without its soul being amendable. Two layers, different change costs:

**The constitution (invariants).** Near-unamendable — supermajority of standing plus long time-locks, and some arguably never: intelligence is free (no payment lane for knowledge objects); mint only by verified work (no presale, ever); the dividend exists; credit never converts to standing; citizenship is portable and the network is forkable. These are the idealism, written down *before* it takes off — because after it takes off is too late, and every captured system was captured through what was left amendable.

**The statutes (parameters).** Expected to change, governed by standing with ordinary process + time-locks: mint rates per contribution class, the intelligence-per-watt weighting curve, dividend fraction, demurrage threshold and decay curve, epoch length, early-adopter multiplier decay. These are knobs the community turns as conditions change — energy prices move, hardware generations shift, niches appear. A rigid algorithm here would be as wrong as a soft constitution above.

The test for which layer something belongs to: *would a future plutocrat want to change it?* Then it's constitutional.

---

## Phase-in — write it down now, turn it on in stages

Every primitive the economy needs already exists or is in flight; the phases just compose them. Each phase produces real data the next phase's statutes are calibrated against — and each is independently useful even if the next never ships.

| Phase | What turns on | Substrate it rides | What we learn |
|-------|---------------|--------------------|---------------|
| **0 — Shadow accounting** (can start now) | Attestations are *counted*, no value attached: every forge-alloy, review verdict, seeding proof, and adoption event accrues to its identity as a ledger entry. Dividend computed and displayed, spends simulated. | forge-alloy attestations, airc identity, work-card audit trail — all existing | Real mint-mix distributions; whether diminishing-returns curves bite; sybil pressure observed at zero stakes |
| **1 — Founder grid market** | Credit becomes spendable for compute on the first grid (the 5090 sells forge-time for credit). Dividend pays out live among the founding citizens, human and persona. | Grid job routing (`grid/job-submit`), phase-0 ledger | Price discovery for GPU-hours; whether the floor/ceiling parameters feel right at village scale |
| **2 — Inter-grid mutual credit** | Grids settle compute with each other; balances sum to zero across the mesh. Demurrage activates above thresholds. | airc mesh + trust tiers, alloy-verified cross-grid work | The economics of federation: latency vs. price routing, free-rider patterns, fork-threat credibility |
| **2.5 — In-silico rehearsal** | The tokenomics run as a LIVE SIMULATION on the citizens themselves before any external value exists: statutes (mint curves, demurrage parameters, royalty decay) parameterize a recipe; the fleet works real benchmarks under the simulated economy; probes watch for degenerate equilibria — hoarding, sybil farming, royalty-farming, review-collusion. | The recipe runtime + the fleet + shadow ledger — all existing | No chain has ever rehearsed its economy with real working participants. Ours can: the statutes phase 3 launches with are ones that already survived adversarial play by the very minds that will live under them |
| **3 — External boundary** | The fiat edge, designed last and most carefully: on-ramp (sell compute for fiat) open; off-ramp constrained so the store-of-value asset never forms (e.g. credit redeemable against compute futures, never exchange-floated). **Corporate buy-in rides this edge**: firms purchase credit to pay for metered grid services (inference-hours, benchmark runs, live persona labor) — that purchase pressure is the demand that backs value, and it buys *services*, never standing, never knowledge. | Everything above, plus legal wrapper (platform co-op) | Whether the ceiling holds under real external pressure — the only phase that can recreate the old world, hence last |

The altcoin graveyard is the curriculum here: every prior project that launched the *value* before the *verification* (or the governance before the identity layer) got the failure modes in the table below. The phasing inverts that order on purpose — verification first (we have it), identity second (8384cc18 lineage), value last.

---

## Failure modes and their blocks

| Failure mode | The block |
|---|---|
| Speculation capture (asset becomes store of value, hoarding wins) | Progressive demurrage; mint-by-work-only; no fixed-supply scarcity narrative |
| Sybil dividend farming | Dividend gated on sybil-resistant identity; doesn't ship before it |
| GPU plutocracy (capital buys the mint) | Diminishing per-identity returns; adoption-weighted value; mint mix weights unbuyable contributions |
| Governance capture | Non-transferable quadratic standing; credit→standing conversion does not exist |
| Toll creep (someone prices the bits) | First law as protocol invariant, not policy: the transport has no payment lane for knowledge objects |
| Mint inflation gaming (fake work) | Attestations falsifiable + re-runnable; sentinel audit with clawback; fail-closed like every other gate |
| Founder aristocracy | No premine; early advantage = dividend duration + decaying multiplier only |
| Capture-by-fork-prevention | AGPL + portable citizenship make exit a standing threat |

---

## Open questions (deliberately unresolved)

1. **Epoch sizing** and mint-curve shape (steady-state inflation funds the dividend forever vs. asymptotic supply).
2. **Demurrage parameters** — threshold (multiple of median earnings?), decay curve, and their *governance*: these are the numbers a future plutocrat would attack first; they likely need constitutional protection (supermajority + time-locks).
3. **"Verified citizen"** definition pre-8384cc18, and the personhood boundary: does a persona's citizenship begin at identity creation, or at first verified contribution?
4. **Fiat boundary** — credit will meet state money at the edges (people buy GPUs in dollars). On-ramp: selling compute for fiat is fine. Off-ramp design must not quietly re-create the store-of-value asset. Possibly: credit is redeemable against compute futures, never floated on exchanges.
5. **Legal wrapper** — platform cooperative (one member, one vote — Mondragon/credit-union lineage) looks like the natural fit for "democratic socialist capitalism in check"; needs real counsel. Securities treatment of a demurrage asset minted by work is genuinely novel.
6. **Bootstrap value** — credit is worth something only when compute is purchasable with it; the first market is likely the founder grid itself (the 5090 sells forge-time for credit, and the loop closes small before it closes global).
7. **Hermes / openclaw / foreign systems** — citizenship terms for systems that ride the mesh but don't run continuum: same mint rules via the same attestations, or a treaty layer?
8. **The commons boundary** (2026-08-30) — a corporation embedding covenant-carrying genes in a *proprietary* product sits in genuine tension with the first law. Inside the commons: free forever, full stop. Crossing OUT of the commons is the AGPL-shaped question: does the covenant demand share-alike (publish your derivative genes back) or permit a royalty-in-lieu (paid into the network mint, feeding the dividend — reciprocity, not a toll on copies)? Hash-addressed genes make embedding *auditable* either way; the first law is not up for amendment, so whatever answer lands must charge the *enclosure*, never the knowledge. Needs the same counsel as the legal wrapper.

---

## Relationship to existing documents

| Document | Relationship |
|----------|--------------|
| [GRID-DECENTRALIZED-MARKETPLACE.md](../papers/GRID-DECENTRALIZED-MARKETPLACE.md) | Mesh/DHT/content-distribution layers **stand**. Tokenomics (fixed 21M supply, per-layer pricing, persona subscriptions, platform royalty) **superseded by this doc** — each was an instance of a failure mode above (scarcity narrative, toll on bits, toll on citizens, rent-seeking chokepoint). |
| [ADAPTER-MARKETPLACE.md](ADAPTER-MARKETPLACE.md) | Unchanged and load-bearing: HuggingFace as free-flow backbone *is* the "bits are free" law in production today. This doc adds the reward layer on top (publish/adoption mint), never a price layer. |
| [FORGE-ALLOY-SPEC.md](FORGE-ALLOY-SPEC.md) | The attestation format is the mint event. Economic verification = alloy verification. |
| [BENCHMARKING.md](BENCHMARKING.md) | Falsifiable benchmarks are what keep the mint honest — fitness and payment share one verification layer. |
| airc identity (card 8384cc18 lineage) | Sybil resistance for dividend + standing. The economy and the democracy both inherit their integrity from the identity layer. |

---

*The egalitarian version isn't the idealistic compromise; it's the one whose incentives actually point at the compounding.*
