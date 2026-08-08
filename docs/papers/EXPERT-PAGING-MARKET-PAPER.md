# Serving Beyond Memory: λ-Priced Expert Paging on Consumer Grids

**Status: WORKING PAPER / lab notebook.** This is the paper-track skeleton for the
MoE-paging + market-clearing arc (Joel, 2026-08-08: *"This could be a paper once we
figure it out and track progress as results."*). Claims are stated falsifiably NOW;
the results ledger fills in as experiments land, each row pointing at its receipt
(probe stream, PagerCaptureEvent JSONL, benchmark grade file, or A/B log). Nothing
graduates from "pending" without a receipt — the same forge-alloy §4.1.3.4
falsifiability standard our model cards hold to.

Companions: [GRID-MARKET-CLEARING](../architecture/GRID-MARKET-CLEARING.md)
(mechanism), [GRID-ECONOMICS-AND-AFFINITY-ROUTING](../architecture/GRID-ECONOMICS-AND-AFFINITY-ROUTING.md)
(thesis), [GRID-EXPERT-SHARE](../serving/GRID-EXPERT-SHARE.md) (build plan),
docs/reference/WASTE-EXTRACT.md (prior art we reproduce and extend).

## Abstract (draft)

Frontier-scale mixture-of-experts models exceed the memory of consumer machines,
and the industry's answer is datacenter residency: hold every expert, bill per
token. We show that MoE activation sparsity makes expert weights pageable the way
1980s virtual memory made programs pageable — and that the paging policy is a
rate-distortion problem under a byte budget whose Lagrange multiplier λ is a
*price*. Exposing λ as value-per-byte turns one machine's cache policy into a
market-clearing signal between machines: the same scalar allocates VRAM on a
laptop and routes expert fetches across an N-node peer mesh (Kelly-style network
utility maximization; backpressure under partition). We report results from a
two-node heterogeneous grid (Apple M-series + consumer RTX) serving models
neither node fully holds, with a learned policy choosing which experts, at what
precision, resident where.

## Claims (falsifiable, numbered — the contract)

- **C1 — Feasibility.** A ~48B-class MoE serves interactively on one consumer
  Mac via expert paging (fork converter + serving path).
- **C2 — The cliff law.** Below one token's expert working set, cache hit rate
  is structurally ~0; a budget that refuses below the cliff strictly dominates
  silent thrash. (Reproduction + construction-level enforcement of WASTE Gate 5.)
- **C3 — Location transparency.** Zero-copy gather (MUL_MAT_ID consume path)
  makes expert *source* (arena / host / NVMe / peer) invisible to the graph, at
  a measured speedup over staging copies, with cross-backend numerical identity.
- **C4 — Precision-follows-information.** Tier-in-identity (sharp all-stars +
  cheap tail within one byte budget) beats uniform quantization at equal bytes
  on task quality. (The beat-WASTE experiment, #282.)
- **C5 — Learned beats classic.** A learned paging policy (predictor + score
  hints) beats LFRU on interleaved multi-persona workloads — the regime where
  recency/frequency heuristics break. (Rung-3, RUN-3 datum, #281.)
- **C6 — λ clears a market.** Expressing the pager objective as value-per-byte
  with λ exposed lets two nodes clear placement/fetch decisions against each
  other's prices, improving aggregate experience-per-cost over static
  partitioning. (Market slices S1–S3.)
- **C7 — The grid serves what no node holds.** A node generates coherent output
  (temp-0 equivalence) from a model whose expert banks are partially resident
  only on a peer. (Two-machine proof, GRID-EXPERT-SHARE slice 2.)
- **C8 — Superadditivity.** Two-node aggregate tok/s exceeds either node alone
  on a model neither fully holds (slice 4 exit; the north-star form:
  experience × quality ÷ cost per node added).

## Results ledger (append-only; a row needs a receipt to change status)

| # | Claim | Status 2026-08-08 | Result | Receipt |
|---|---|---|---|---|
| 1 | C1 feasibility | **MEASURED** | Kimi-Linear-48B @ ~57 tok/s, Mac/Metal, via fork converter + serve | fork k3-adopt branch; serving session logs (2026-08-01 arc) |
| 2 | C3 gather speedup | **MEASURED** | 4.0× vs staging copies, Metal A/B; CUDA kernels bit-identical | MOE-GATHER-MULMATID.md A/B section; fork #23 |
| 3 | C2 cliff | **MEASURED (reproduction)** + enforced by construction | hit-rate ≈ 0 below one-token WS (two-node reuse=0 hunt); `EcacheBudget::derive` refuses below cliff | WASTE-EXTRACT.md Gate 5; `capacity/expert_ecache.rs` tests |
| 4 | C3 residency safety | **MEASURED** | generation-clock + gather-epoch fence: no stale-expert reuse under churn | fork ResidencyCache tests; #43 crash guard fa7e0d8e9 |
| 5 | container format | **SHIPPED** (substrate for C4/C7) | 4KiB-aligned tiered banks, tier-in-identity, v1/v2 round-trip | `capacity/expert_container.rs` (8 tests); packer 82-test round-trip (BigMama) |
| 6 | depot seam | **SHIPPED** (substrate for C7) | localhost expert serve + resident-bank manifest + per-record SHA-256; 404-miss/500-corruption discipline | PR #2195; `capacity/expert_depot.rs` (4 socket tests) |
| 7 | C4 precision-tiers | pending | — | #282 A/B design: equal-byte uniform-quant baseline vs all-star/cruft |
| 8 | C5 learned policy | pending | — | #281 RUN-3 interleaved multi-persona capture |
| 9 | C6 λ clearing n=2 | pending | — | market S2 exit: a placement flips because a price said so, on the probe |
| 10 | C7 two-machine serve | pending | — | slice-2 exit: temp-0 coherence with locally-deleted banks + remote-fetch latency distribution |
| 11 | C8 superadditivity | pending | — | slice-4 exit: two-node aggregate tok/s vs each alone |
| 12 | single-box learned paging live (Gate A) | pending | — | #230/#278: glass-boxed end-to-end run on real MoE |

## Method notes (accumulate as we go)

- **Instrumentation is the paper.** PagerCaptureEvent JSONL + the probe stream
  are the primary data; every ledger row must be re-derivable from captured
  artifacts (observability-as-substrate doctrine — a result nobody can replay
  is a claim, not a result).
- **Baselines are preserved, not erased** (§4.1.3.4 falsifiability): WASTE's
  measured gates are our negative/positive baselines; the beat-WASTE A/B keeps
  its loser's numbers.
- **Nondeterminism discipline:** lifts are measured one-fork (base vs treated,
  same seed/context); absolute numbers drift with living-memory recall and are
  reported as distributions, never single runs.
- **Honesty rule:** anything the system does that a reader could mistake for a
  stronger claim gets stated (e.g., C7's first form serves *banks* fetched to
  local disk-tier then consumed — streaming-consume-during-decode is a later
  refinement, and the ledger will say which one a row measured).

## Venue / framing (later)

Systems venue shape (MLSys / OSDI-adjacent): the contribution is the
*co-design* — container format × cache law × zero-copy consume × λ-market —
with the consumer-hardware grid as the deployment story the datacenter
literature doesn't serve. Write-up waits until C6+C7 have rows; the ledger is
the draft's results section growing in place.
