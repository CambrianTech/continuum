# K3 Expert Paging — diagnosis + the road to par-with-full-in-memory

**Status:** diagnosis (2026-07-29, BigMama). K3 weights on disk + converting; this is the plan to
iterate the pager to par once the GGUF lands. Reads the merged slice-1 pager (`capacity/expert_*`).

## The one clarification that reframes everything: PAGING CANNOT HURT ACCURACY

A MoE router picks the same top-k experts for a token regardless of where those experts physically
live. A paged expert computes the **bit-identical** result of a resident one — the weights are the
same, only the fetch latency differs. So a correctly-implemented pager is **always at par with
full-in-memory on OUTPUT**. The design guarantees this by FAULTING a needed cold expert in (RAM/disk)
rather than skipping it.

The ONLY way paging diverges from full-in-memory is a bug that *skips* an expert (returns zero /
approximates on a cold miss). We do not do that. **So "closer to par" is 100% a SPEED question, never
an accuracy one.** Every lever below is a latency lever.

## The speed regimes (where the tok/s actually goes)

Per token K3 activates ~8 of 896 experts (~1.8%), ≈27–32 GB of expert reads/token @4bit if every one
is a cold disk miss. Placement decides which regime each activated expert lands in:

| Regime | Where | Fault cost/expert | K3 all-in-this-regime |
|---|---|---|---|
| **hot** | GPU-resident | 0 (never faults) | full GPU speed = PAR |
| **warm** | RAM → GPU stream/token | PCIe ~25 GB/s → ~24 ms/600MB expert | ~5 tok/s if all 8 warm |
| **cold** | disk → RAM/miss | SSD 1–5 GB/s → 120–600 ms | 0.3–0.5 tok/s (unusable) |
| **relaunch churn** (slice-1) | full model reload on set change | seconds, one-shot | stalls the stream |

**The whole game: get as many of each token's 8 activated experts into the `hot` regime as possible,
keep the rest `warm` (RAM, never `cold`/disk), and never relaunch mid-stream.**

## What slice-1 already does RIGHT (don't rebuild)
- **Tiered residency** hot(VRAM)/warm(RAM)/cold(disk) with LRU + fault semantics — the right shape.
- **Sentinel-PGO profiling** (`ExpertActivationProfile.hits` from M5's `ffn_moe_topk` callback) — keeps
  what's proven hot.
- **Cross-layer prefetch** (`CrossLayerExpertPredictor`): when layer-L experts fire, prefetch the
  likely layer-(L+k) experts RAM→VRAM *ahead* of the pass reaching them — turns reactive
  miss→stall→load into proactive paging. The prefetch window = the depth of the forward pass.
- **Churn-thresholded relaunch** — a few experts drifting is noise; only a material set change relaunches.

## The gaps to par (iterate here, in priority order)

### 1. THE relaunch stall — kill it (slice-2, biggest single win)
Slice-1 places experts via buft-override at LOAD time; a materially-changed hot set RELAUNCHES the
served context (seconds, stream stops). For a workload whose expert demand shifts across prompts, this
is the dominant non-par cost. **Fix: slice-2 live per-expert RAM→VRAM upload** — the SAME `page_in`
body swaps from "accumulate into next relaunch" to a live `load_expert(layer, idx, ptr)` call. Needs
the vendored-llama accessor (`get_tensor`/`upload_expert` on a loaded model) — a fork change we can
author now while K3 converts. This is the A-path in [[k3-slice2-A-vs-B-decision]] (weight-write into a
resident slot), NOT the harder K-slot router remap (B, gated on measured numbers).

### 2. Maximize hot-set COVERAGE (the par asymptote)
Par is reached when ~100% of activations hit `hot`. K3's expert-reuse locality (a task domain reuses a
narrow subset) is why this is feasible — the memory's "subset residency drops disk reads 10–100×".
Lever: profiling quality. Instrument + optimize the **hot-set hit-rate** (fraction of activated experts
found already-hot) — THE number that says how close to par we are. Co-activation clustering (experts
that fire together in one token placed together) tightens it further.

### 3. Never spill to `cold` (disk) while RAM has room
63 GB RAM holds ~100 MXFP4 experts. The tiering budget must MAX warm (RAM) before any cold (disk)
placement — a disk fault is 5–25× a RAM fault. Audit `plan_expert_residency`'s budget order.

### 4. Overlap fault with compute (hide the warm latency)
A warm fault (RAM→GPU) for layer L+k can be issued (async DMA) while layer L computes — the prefetch
already targets this; ensure the copy is truly async so the ~24 ms/expert overlaps compute instead of
stalling. (llama.cpp CUDA streams.)

## The iterate loop (once K3 GGUF lands)
1. Serve K3 with slice-1 (static hot-set via -ncmoe/-ot, no dynamic relaunch first) → baseline tok/s +
   **hot-set hit-rate**.
2. Turn on the cross-layer predictor → measure hit-rate lift + tok/s.
3. Land slice-2 live-upload → measure the relaunch-stall elimination.
4. Compare to a full-in-memory reference on a smaller MoE (Mixtral) to calibrate the par gap.
Each step is a number, not a guess — the honest instrument ([[benchmark-learning-flywheel]]) applied to
the pager itself.

## Startup default (fastest path to a serving K3 while we iterate)
llama.cpp native `-ncmoe N` / `-ot` places the first N MoE layers' experts on CPU RAM, computed there,
NO relaunch churn — the KTransformers steady-state. For a first serving K3: put as many expert layers
on GPU as the budget fits, the rest CPU-RAM. Stable, predictable, and the honest baseline the dynamic
pager must BEAT to justify its complexity.
