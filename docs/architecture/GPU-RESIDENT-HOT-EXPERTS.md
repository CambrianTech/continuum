# GPU-Resident Hot Experts — "trend to full GPU only" (task #23)

**Goal (Joel):** the biggest single speedup — the residency system should *trend itself toward full-GPU*:
keep the hot expert working set resident in VRAM so decode's hot path computes fully on GPU with **zero
per-token NVMe fetch and zero host→VRAM copy**. As the pager identifies the stable hot set, it promotes
hot→VRAM / evicts cold, and the working set self-organizes toward VRAM residency.

## Where the time actually goes (measured, top-8, ~1.9 s/token = 0.53 tok/s)

The op-offload MoE path (`ggml-backend.cpp` ~1648-1824) does, per layer, per token:
1. **NVMe fetch** of missed experts into the pinned host cache (recency ~62% hit → ~38% miss). ~0.53 s.
2. **host→VRAM copy** of the *used* experts into `input_cpy` (the VRAM tensor the `MUL_MAT_ID` reads),
   via `ggml_backend_tensor_set_async`. ~0.2 s.
3. **GPU compute** (the expert matmuls — already GPU-native). The rest.

Two structural inefficiencies, independent of the fetch:
- **Redundant copies:** `input_cpy` persists across tokens (it's the split's VRAM buffer), but every token
  re-copies *all* used experts — including the ~62% that were already resident from last token.
- **Serialization:** `ggml_backend_synchronize` (lines 1660, 1679) forces fetch→copy→compute to run
  serially per layer instead of pipelining.

## Design — three increments, lowest-risk first

### Increment 1 — copy-skip (VRAM residency tracking on `input_cpy`)
Per split's `input_cpy` VRAM buffer, track the **set of expert ids currently resident** (last copied). Each
token, copy only the experts NOT already resident (the misses vs last token); skip the rest. At ~62%
token-to-token overlap this eliminates ~62% of the host→VRAM copies. No new VRAM — `input_cpy` already
holds them. Care: the sched double-buffers (`sched->cur_copy`) → keep a resident-set **per buffer id**.
Correctness: `MUL_MAT_ID` only reads the selected `ids`, so a skipped-but-still-resident expert is valid.

### Increment 2 — persistent hot-expert VRAM cache
A device-side residency cache (VRAM analog of the pinned-host `ResidencyCache`): promote the hottest
experts (recency + the plan-file pin list) into a **persistent VRAM region** that survives across layers
and tokens. On expert access the seam checks VRAM-residency first: **hit ⇒ GPU-native, no fetch, no copy**;
miss ⇒ current fetch→copy path. Evict cold by the same score-hint/generation logic as the host cache.
This is where "trend to full GPU" lives: the promotion loop grows the VRAM-resident fraction as the hot
set stabilizes.

### Increment 3 — pipeline (de-serialize)
Overlap the copy/fetch of the next layer's experts with the current layer's compute (double-buffer +
drop the hard syncs where the dependency allows). Bounded by sequential routing (layer N+1's experts
depend on N's output), but the copy of already-known experts can overlap.

## The 32 GB constraint — the rate-distortion knob
On a 32 GB card the resident attention (~28-33 GB) fills VRAM, leaving little for expert residency. So the
"trend" is bounded here and becomes a **rate-distortion allocation**: VRAM spent on hot experts vs
high-fidelity attention. The **imatrix** (running now) is what makes that tradeoff intelligent — it lets us
down-quant the resident attention *and* the cold experts by measured importance, freeing VRAM the hot set
promotes into. On a bigger card (or smaller MoE) the trend approaches genuine full-GPU. This is the
misfit-moat fractal: same code, the governor spends the VRAM budget differently per device
([[device-fit-repeatable-primitive]], [[dynamic-precision-tiers-and-diversity]]).

## Sequence
imatrix keystone (async now) → increment 1 (copy-skip, low-risk, measurable alone) → increment 2
(VRAM hot cache, the "full-GPU trend") → increment 3 (pipeline). Each increment is measured on its own via
the `k3-bench` harness before stacking. Ref: [[k3-beats-waste-decisively]] (compute/copy-bound at top-8).
