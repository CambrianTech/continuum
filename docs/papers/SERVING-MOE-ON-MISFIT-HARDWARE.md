# Serving Frontier Mixture-of-Experts on Misfit Hardware: A Predictive‑Quantization Control Law and its Fractal Extension to Grid Negotiation

*Working paper — 2026-07-31. Serving-side companion to
[PLASTICITY-COMPACTION.md](./PLASTICITY-COMPACTION.md) (which decides, offline,
which experts and precisions to keep). This paper decides, online, which experts
and precisions to keep **resident** — and shows the same control law negotiates a
peer-to-peer grid. Empirical results are measured live on Kimi‑K3 (2.8T MoE,
IQ2_XXS, 663 GB) on a single consumer box. Status: method + first measurements;
lessons at the end are real numbers, not projections.*

## Abstract

Frontier Mixture-of-Experts (MoE) models are sized for datacenters — Kimi‑K3 is
2.8T parameters, 663 GB even at 2‑bit — while the hardware people own is not. The
prevailing answer is to *fit the hardware to the model* (buy RAM/flash/GPUs) or to
*shard the model across a network* (tensor/pipeline parallel), which is
bandwidth-bound and slow. We take the opposite stance — the **misfit-design**
thesis: fit the model to the hardware people already own, by engineering. We frame
MoE expert paging as a **control system**: the model's routing is the plant, an
activation trace is the sensor, the storage-fetch latency is dead time, and
**per-expert residency and quantization precision are the actuator**. The
controller predicts the working set (recency), allocates precision under a memory
budget (rate-distortion), prefetches ahead of the fetch dead time (Smith
predictor), and compensates quantization error (LoRA). We show, with live
measurements, that the storage device is not the bottleneck people assume — the
bottleneck is memory over-commit (thrash) and a *frequency*-based cache policy;
**recency** prediction and a *precision* actuator change the picture. Finally we
show the control law is **fractal**: the identical loop, one level up, is the
grid negotiation protocol — peers are memory tiers, LAN is the dead time, and
per-node service precision is the actuator — so pooling misfit nodes *compounds*
capacity under one controller.

## 1. Introduction

The misfit-design thesis (§Related Work contrasts it with exo-style sharding):
capability should come from *engineering the residency of a large model on modest
hardware*, not from acquiring hardware to match the model. MoE is the ideal
substrate for this because only a small fraction of parameters (the top‑k routed
experts, plus a few shared experts) is active per token — the rest can live on
slower, larger, cheaper storage and be paged in.

The naive view of expert paging is a cache: keep recently/frequently used experts
in fast memory, fault the rest from disk. We show this framing is wrong twice
over. First, the fetch path is *not* the bottleneck (a modern NVMe delivers
3–3.8 GB/s scattered, more than enough); the bottleneck is (a) **memory
over-commit** — committing more than physical RAM makes the OS thrash and the GPU
idle ~85% of every token — and (b) a **frequency**-biased policy that ignores the
dominant signal. Second, the actuator should not be binary (resident / not). Given
quantization, each expert has a *precision* — a continuum from "0 bits resident,
fault full-precision on demand" through "low-bit approximation always in RAM" to
"high-bit in VRAM." Making precision the actuator turns a saturating on/off switch
into a throttle and turns the budget constraint into a rate-distortion allocation.

## 2. Method: the expert-paging control law

### 2.1 The loop

- **Plant** — the model's routing process, emitting the per-token expert
  activations.
- **Sensor** — an ordered activation trace captured at the compute/offload seam:
  one `(tensor_key, expert_id)` record per activation, in access order. *You
  cannot control what you do not observe;* the sensor is load-bearing.
- **Dead time** — the storage-fetch latency is transport delay in the loop. The
  textbook treatment of dead time is a predictor that acts *ahead* of the delay
  (Smith predictor): prefetch the predicted-next experts one token early so I/O
  overlaps compute rather than stalling it.
- **Actuator** — per-expert (residency locality × quantization precision). Its
  saturation limit is the memory budget: the controller can only pin what fits;
  over-commit removes its authority entirely.
- **Feedforward** = prefetch the predicted set. **Feedback** = on a miss (a
  prediction error) fetch reactively and update the estimate.

### 2.2 Predict — recency, not frequency

The prediction target is the next token's active set. Empirically (§4) the
dominant, low-cost predictor is *persistence*: the experts active in the last
token(s) are the experts active in the next. A recency window of the last *K*
tokens is a moving-average (FIR) predictor on the activation signal; it
outperforms LRU/LFU by 3–4× at equal budget and needs no training. A learned
predictor (system identification of the routing dynamics from the trace) then
recovers the residual gap to the offline optimum by modeling per-layer
persistence, co-activation groups, and the optimal *K*.

### 2.3 Allocate — rate-distortion precision under a budget

Given the predicted importance of each expert, allocate a *precision* (bit-level)
to each so that total resident footprint fits the memory budget while minimizing
output distortion. This is classic water-filling / Lagrangian bit allocation:

```
minimize   Σ_e  distortion(e, b_e)
subject to Σ_e  footprint(e, b_e)  ≤  memory_budget
```

with importance weight `w_e = recency(e) × sensitivity(e)`. `sensitivity(e)` — how
much quantizing expert *e* degrades the output — is the second sensor (measured by
perturbing one expert's precision and reading the perplexity/KL delta). Residency
is subsumed: "cold" is `b_e = 0` bits resident (fault full-precision on demand);
"warm" is a low-bit copy always in RAM; "hot" is high-bit in VRAM. This unifies
regional dynamic quantization and working-set residency into one allocation.

### 2.4 Prefetch and compensate

Prefetch the predicted set ahead of the dead time (§2.1). Where sensitivity is
high, a small compensation-LoRA trims the distortion injected by low-bit
residency, stretching the effective budget (this is the online counterpart of the
offline mixed-quantization in PLASTICITY-COMPACTION).

## 3. The fractal extension: grid negotiation is the same control law

The controller is **scale-invariant**. The identical predict → allocate →
prefetch → compensate loop, one level up, negotiates a peer-to-peer grid. The
mapping is exact:

| Per-machine expert pager | Grid / p2p negotiation |
|--------------------------|------------------------|
| Expert | Artifact hosted on a peer (model / expert bank / genome) |
| Residency (RAM/VRAM) | Placement (which peer holds which artifact) |
| Quantization precision | Service tier a node serves (a weak node serves a lower-bit, compensated tier) |
| Memory budget | Node capacity; **grid = aggregate capacity across all peers** |
| Dead time (NVMe latency) | LAN/WAN latency to route to / fetch from a peer |
| Recency prediction | Demand prediction — which capabilities will be requested next |
| Rate-distortion allocation | Allocate quality across the grid to minimize aggregate distortion under aggregate capacity |
| Prefetch (Smith predictor) | Pre-place / replicate artifacts on well-positioned peers ahead of demand |
| Compensation-LoRA | Per-node quality trim |
| Miss → reactive fetch | Route the query to a peer that holds it, then update placement |

The consequence is the moat. Single-machine paging engines (§Related Work) are
capped by one box's memory budget. The grid **compounds** the budget: pooling RAM
across misfit nodes enlarges the actuator, so aggregate hit-rate and achievable
precision exceed any single node — under the *same* control law we already run
per-machine. The governor that decides which node serves a request is not a
different mechanism from the pager that decides which expert is resident; it is
the same controller with peers as its memory tiers. (In the codebase this is the
`decide_lane` / `FitPolicy` governor seam: expert placement is "the same shape one
level down.") ML at the node level therefore behaves exactly as it does per
machine — which is why the per-machine measurements below are also the grid's
design data.

## 4. Results & lessons (measured live on Kimi‑K3)

Hardware: RTX 5090 (32 GB), 63.4 GB RAM, 2 TB Samsung NVMe. Model: Kimi‑K3
IQ2_XXS, 663 GB / 16 shards. Workload: a coding prompt. Instrumentation:
`GGML_MOE_TRACE_FILE` at the offload seam, replayed offline in Rust against
LRU / LFU / Belady‑optimal + a persistence predictor.

**L1 — The storage device is not the wall.** Isolated scattered unbuffered
overlapped reads sustain **3.0–3.8 GB/s** (already 3.2 GB/s at queue depth 1;
16 KB-aligned records +20% over GGUF-native offsets). The commonly-assumed
disk-bandwidth ceiling is an artifact of *contention*, not the device.

**L2 — Over-commit thrash is the real killer.** K3 commits ~**79 GB > 63 GB
physical RAM**; the OS pages committed memory in and out, the live fetch collapses
bimodally (2 GB/s ↔ 140 MB/s), and **~85 % of every decode token is `fault_wait`
— the GPU idle, waiting on paged-out memory.** Baseline ≈ 0.013 tok/s. Corollary:
*give the actuator authority (footprint < physical RAM) before tuning any policy.*

**L3 — Recency beats frequency, decisively.** Keeping the **last decode token's**
active experts resident (12 GB) makes **40.6 %** of the next token already
resident; last 2 tokens (19 GB) → **51.0 %**; last 4 (31 GB) → **59.0 %**.
LRU/LFU at comparable budgets ≈ 10–22 %. The pager policy is a recency window, not
a frequency cache.

**L4 — Reuse compounds with generation length.** The Belady-optimal (intrinsic
repeat) ceiling climbs with context: **13.6 % @ 2 tokens → 28.5 % @ 4 → 40.3 %
@ 8 → 46.8 % @ 12**, still rising. Short traces (<10 tokens) badly *under*-estimate
achievable hit-rate — an early measurement here wrongly suggested near-uniform,
low-ceiling routing. Skew sharpens too: top 10 % of experts cover 31 %, top 50 %
cover 73 % of accesses at 12 tokens.

**L5 — Structural experts are free.** The always-active shared experts (2/layer ×
92 = 184) never enter the streamed set — they are already resident. The paged set
is the routed tail, which is exactly what recency prediction targets.

**L6 — Instrumentation is the method.** Every correction above (disk-not-the-wall,
thrash-not-fetch, recency-not-frequency, reuse-compounds-not-flat) came from a
sensor, and each reversed a plausible wrong conclusion. Breakpoints are invalid
for a timing-sensitive, concurrent, DMA-bound loop — they perturb what they
measure; the only valid observation is run-naturally-and-capture.

**Projected trajectory (data-backed).** Recency residency (last 2 tokens, 19 GB)
at 51 % hit → ~5 GB/token from disk at the proven 3 GB/s → ~**0.55 tok/s** on a
single misfit box, already ahead of the 0.32 tok/s single-machine baseline
(§Related Work), with dynamic quantization (smaller resident footprint → more
fits → higher hit) and prefetch (hide the dead time) as the untapped multipliers
toward 1 tok/s, and the grid as the ceiling.

## 5. Related work

Single-machine MoE streaming engines (WASTE; Colibri; KTransformers) independently
validate the trunk-resident + streamed-expert architecture and report ~0.3–1
tok/s on comparable single boxes; they are, by construction, capped at one box's
memory budget. Network-sharding systems (exo and tensor/pipeline-parallel serving)
fit a model by splitting it across nodes, paying inter-layer network latency on
the critical path. Our contribution is orthogonal: a *predictive-quantization
control law* for single-node residency, and its fractal reuse as the grid
negotiation protocol, so the grid *compounds* capacity rather than paying to
split. Offline importance/precision selection is the subject of the companion
paper [PLASTICITY-COMPACTION.md](./PLASTICITY-COMPACTION.md); this paper is its
online, residency-time counterpart.

## 6. Conclusion

Serving a 663 GB frontier MoE on a 63 GB box is a control problem, not a hardware
problem. The bottleneck is over-commit and a wrong policy, not the disk. A
recency-predicting, rate-distortion-quantizing, dead-time-compensating controller
turns the memory budget into a throttle instead of a wall — and because that
controller is scale-invariant, the same loop negotiates a grid of misfit nodes
whose pooled capacity exceeds any single one. Fit the model to the hardware people
own; then let the grid compound it.

## References

- Companion (offline): [PLASTICITY-COMPACTION.md](./PLASTICITY-COMPACTION.md)
- Architecture spec: [../architecture/EXPERT-PAGING-CONTROL-LAW.md](../architecture/EXPERT-PAGING-CONTROL-LAW.md)
- Governor seam: [../architecture/EXPERT-PAGING-GOVERNOR-SEAM.md](../architecture/EXPERT-PAGING-GOVERNOR-SEAM.md)
- Grid marketplace: [GRID-DECENTRALIZED-MARKETPLACE.md](./GRID-DECENTRALIZED-MARKETPLACE.md)
- Instrumentation: `GGML_MOE_TRACE_FILE` (llama.cpp fork, offload seam); Rust
  replay tool → to land in `continuum-core` `ServingExpertPager`.

*Reproducibility: measurements append to the benchmark ledger; the trace format,
replay tool, and serving env are versioned in-tree so every number above is
re-derivable.*
