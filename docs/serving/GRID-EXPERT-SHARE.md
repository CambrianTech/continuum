# Grid Expert Share — the GridFetcher arc

**Thesis (Joel, 2026-08-03, on the Cut-2 gather proof): "That means grid share
will work."** Correct — the gather was the load-bearing piece. This doc plans
the rest: N misfit machines collectively serving one big MoE, each node hot on
its share of the expert population, misses resolved from PEERS before disk/HF,
consumed zero-copy through the proven table path.

Related: #180 (expert-affinity paging across the grid), #300 (one download per
GRID), #102 (PlacementPolicy seam), #282 (prefetch overlap), the five-level
grid-cache doctrine, and `docs/serving/MOE-GATHER-MULMATID.md` (the consumption
primitive this builds on).

## What is already proven (do not rebuild)

| Grid-share requirement | Existing, validated primitive |
|---|---|
| Consume an expert from ANY location, zero-copy | gather MUL_MAT_ID (Metal 4.0× A/B; CUDA kernels bit-identical) |
| Stable cross-node expert identity | `ExpertId` semantic keying — (tensor, expert), universal across MoEs |
| Safe reuse under churn | ResidencyCache generation clock + gather-epoch eviction fence |
| Pluggable byte source | `ExpertFetcher` trait (`fetch`/`fetch_many` batched) — adapters: mmap, NVMe direct-read, packed container, device-upload |
| Demand telemetry | `[MOE-PAGER]` line + PagerCaptureEvent JSONL (hotness measured prompt-dependent → placement must be dynamic) |
| Governed budgets | plan-file (`GGML_MOE_PLAN_FILE`) budget/window/pin actuators |
| Grid control plane | airc: delivery receipts (#280), route health, GridTrustAuthPolicy |
| Engagement honesty | fail-loud pager warning (e696f67bc) |

## Architecture decision: depot, not fork-side networking

The fork stays THIN. llama.cpp never learns about peers, trust, or routing —
it gains ONE more `ExpertFetcher` adapter that GETs bytes from a **localhost
expert depot**. The depot is a continuum-core module that owns all grid
intelligence: manifest publication, peer resolution, trust, verification,
disk-tier caching. This is the microkernel split (airc = control plane, direct
peer streams = data plane, same shape as the LiveKit media split) and it keeps
the fork upstreamable.

```
llama-server ──GET /expert/{key}──▶ local depot (continuum-core)
                                      │ resident? serve from artifact/disk-tier
                                      │ miss? resolve via airc manifests
                                      ▼
                              peer depot ──bytes+hash──▶ verify → cache → serve
```

Miss-with-no-peer falls through to today's behavior (local artifact or absent).
The depot can DEGRADE serving; it can never break it.

## Slices (outlier method: A = local/simple, B = maximally different)

**Slice 0 — manifest instrument (read-only, cheap).** Depot publishes its
resident-expert manifest (ExpertIds + artifact hashes + tier) on airc,
refreshed on the recency cadence. No consumer yet. Validates: manifest size,
churn rate, airc fit. *Exit: two nodes see each other's manifests.*

**Slice 1 — depot outlier A: localhost serve.** `expert_depot` module serves
expert bytes by key over localhost HTTP from its own artifacts (mmap/container
readers already exist). Fork gains `GridFetcher` (`GGML_MOE_DEPOT_URL`):
`fetch` = one GET, `fetch_many` = parallel GETs. Single machine, zero network
unknowns. *Exit: `[MOE-PAGER]` parity (hit-rate, coherence, tok/s within
noise) vs the mmap fetcher on OLMoE — proves the seam.*

**Slice 2 — grid resolve outlier B: the two-machine proof.** Depot resolves
misses from peer depots (manifest lookup → direct GET → BLAKE3/SHA-256 verify
→ disk-tier cache → serve). Test that matters: the M5 serves a model whose
expert files are PARTIALLY DELETED locally — those experts exist only on
BigMama's node. Coherent output = the grid served them. *Exit: temp-0
coherence + measured remote-fetch latency distribution. This is the
"two machines prove the grid" milestone.*

**Slice 3 — latency-aware prefetch.** Remote fetch is 10–100× NVMe; the win
requires hiding it. Fetch-source latency class flows into the plan-file;
prefetch depth scales with measured latency; the pager predictor (rung-2/3,
already ported) supplies candidates. *Exit: A2-style degradation curve for
remote-expert fraction — graceful, no cliff, quantified fault_wait vs
remote-%.*

**Slice 4 — governed placement.** The Σdemand governor negotiates pin hints
per node from observed demand (later: co-occurrence cliques, #228/#229). No
new actuator — plan-file pins already exist per node; this only decides their
VALUES. *Exit: two-node aggregate tok/s beats either node alone on a model
neither fully holds.*

**Slice 5 — the flagship measure.** V4-Flash (or K3) across M5 + 5090 where
NEITHER node holds the full artifact. Metric: persona-tok/s × quality ÷ cost
per node added (the north-star form). This is the first real datum for the
grid-frontier ("ų王") claim.

## Guardrails (each is a learned rule, not a nicety)

- **Disk tier is governed on day one**: depot cache gets a `TrackedDir` row +
  an eviction decision in `disk_eviction.rs` BEFORE first write (the 460GB
  rule — the test fails on an undecided cache class; do not weaken it).
- **Content-hash verify every remote byte** — a wrong expert is silent model
  corruption; verify-fail is LOUD and quarantines the peer entry.
- **Trust = grid citizens only** (GridTrustAuthPolicy); depot never serves or
  fetches outside the trusted mesh; secrets never transit airc plaintext.
- **Never block serving**: every depot path has the current behavior as
  fallback; timeouts are budgeted, not open-ended.
- **Headroom precheck inherited** (the kernel-panic rule) — depot admission
  respects the same free-RAM discipline as everything else.
- **Engagement honesty**: the fail-loud pager warning covers the depot too —
  configured-but-dead grid share must announce itself.

## Ownership

- **M5**: GridFetcher fork adapter (mirror of DirContainerFetcher), depot
  module + manifest + peer resolve (continuum-core), Slices 0–2.
- **BigMama**: node-B validation (Windows path handling, 5090 as the
  expert-rich peer), Slice 3 latency calibration on her measured lanes,
  co-owner of Slices 4–5 measurement.
- **Sequencing**: starts after the 5090 V4-Flash gather A/B closes Cut-2.
  Slices 0–1 are safe to start immediately after (no dependency on her lane).

## What we are NOT building

- No parallel transfer protocol — airc for control, plain authenticated HTTP
  between depots for data, until measurement says otherwise.
- No static expert→node assignment — hotness is measured prompt-dependent;
  placement stays a dynamic, governed decision.
- No fork-side grid logic — the fork's entire grid surface is one URL env var.
