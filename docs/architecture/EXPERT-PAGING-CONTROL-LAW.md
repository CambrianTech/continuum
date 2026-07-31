# Expert Paging — the control law (memory hierarchy + dynamic quantization)

*BigMama design note for the MoE expert pager, 2026-07-31. Companion to
[EXPERT-PAGING-GOVERNOR-SEAM.md](EXPERT-PAGING-GOVERNOR-SEAM.md) (M5/Fable's
governor↔mechanics contract). That doc says WHERE the seam is; this one says WHAT
control law runs across it, grounded in numbers measured live on K3 this session.
For M5: this is the total memory plan, including quantization — build the
predictor/allocator to this shape.*

## TL;DR

The expert pager is a **control system**, not a cache. Joel's framing (control
systems, junior year 2001): the model's routing is the **plant**, the trace is
the **sensor**, the NVMe fetch is **dead time**, and residency+precision is the
**actuator**. The winning policy is **recency-window prediction + prefetch
(dead-time compensation)**, and the actuator is not binary residency but a
**continuous precision axis — dynamic per-expert quantization** allocated
rate-distortion-optimally under the RAM budget. Rungs 2 (residency) and 3
(regional quant) of the beat-WASTE ladder are the two axes of one controller.

## What we measured (the empirical basis — not guessed)

Live K3 (Kimi-K3 IQ2_XXS, 663 GB, 16 shards) on BigMama (RTX 5090 / 63 GB RAM /
2 TB Samsung NVMe), a coding prompt, `GGML_MOE_*` serving path.

1. **The NVMe is not the wall.** Isolated microbench of the scattered unbuffered
   overlapped read pattern: **3.0–3.8 GB/s** (QD1 already 3.2 GB/s; the aligned
   container is +20%). The drive is not the bottleneck.
2. **The killer is OS thrash.** K3 commits ~**79 GB > 63 GB physical RAM** →
   pagefile churn → the live fetch degrades from ~2 GB/s to 60–170 MB/s
   (bimodal), and **~85% of every decode token is `fault_wait` — the GPU idle,
   waiting on paged-out memory.** Baseline: 10.8 GB/token, ~0.013 tok/s. The
   fetch code hits full NVMe speed whenever pages are resident.
3. **Recency beats frequency, decisively.** From an ordered expert-access trace
   (`GGML_MOE_TRACE_FILE`, replayed offline in Rust against LRU/LFU/Belady-OPT +
   a persistence predictor):
   - **Reuse compounds with generation length** — Belady/OPT ceiling 13.6% @2
     tokens → 28.5% @4 → 40.3% @8 → **46.8% @12 and still climbing.** (Early
     "near-uniform, low ceiling" reads were short-trace artifacts; don't trust
     reuse numbers under ~10 tokens.)
   - **Persistence predictor** (keep the *last decode token's* active experts
     resident): last 1 token = **12 GB → 40.6%** of the next token already
     resident; last 2 = 19 GB → **51.0%**; last 4 = 31 GB → **59.0%**. LFU/LRU at
     comparable budget ≈ 10–22%.
   - Skew sharpens with length: top 10% of experts cover 31%, top 50% cover 73%.
   - The always-hot **shared experts** (2/layer × 92 = 184) never enter the
     streamed set — already GPU-resident. The paged set is the routed tail.

**Conclusion:** the pager's job is to predict the recency-driven working set,
keep it resident under a hard RAM budget, and prefetch ahead of the fetch dead
time. The residency policy is a **recency window**, not LFU/LRU/frequency.

## The control loop

```
        importance field (recency×frequency×sensitivity)
                     │
   trace  ┌──────────▼───────────┐  precision+locality per expert
 (sensor)─▶  CONTROLLER (pager)   ├──────────────▶ actuator
           │  predict → allocate  │                (VRAM hi-bit /
           │  → prefetch → compensate                RAM lo-bit /
           └──────────▲───────────┘                  NVMe cold / peer)
                      │ miss = prediction error (feedback)
                 PLANT (model routing) ──── dead time = NVMe fetch
```

- **Plant** — the LLM's routing process emitting the expert-activation sequence.
- **Sensor** — the ordered activation trace at the offload seam. *You cannot
  control what you don't observe;* the sensor is why instrumentation is
  load-bearing, not a nicety. (Built: `GGML_MOE_TRACE_FILE`.)
- **Dead time** — the NVMe fetch latency is transport delay in the loop. Textbook
  handling of dead time is a **predictor that acts ahead of the delay (Smith
  predictor)**: prefetch the predicted-next experts one token early so the fetch
  overlaps GPU compute instead of stalling it.
- **Feedforward** — prefetch the predicted hot set (the recency window).
- **Feedback** — on a miss (prediction error) fetch reactively and update the
  estimate.
- **Actuator saturation** — the RAM budget. The controller can only pin what
  fits; if committed > physical RAM the actuator has *no authority* (everything
  it pins gets paged back out — the loop fights the OS). **Footprint-under-RAM is
  the prerequisite that gives the actuator authority.**

## The memory hierarchy (total memory plan): a precision × locality continuum

Not "resident or not" (1-bit actuator) but a continuum along two axes —
**locality** (how far the bytes are) and **precision** (how many bits):

| Tier | Locality | Latency | Precision role |
|------|----------|---------|----------------|
| L0 VRAM (32 GB) | on-GPU | ~0 | shared experts + hottest routed, high-bit |
| L1 pinned RAM | host, DMA 25 GB/s | µs | warm predicted set; may be **low-bit** to fit more |
| L2 NVMe container | 3–3.8 GB/s aligned | ~ms (dead time) | cold tail, full-bit, faulted on miss |
| L3 grid peer RAM | LAN | ms | overflow when local RAM saturates ([[product-strategy-vs-exo]]) |

The controller slides each expert along **both** axes. A warm 2-bit copy of a
likely-needed expert (small, always in RAM) can beat a full-precision one you
must fault in during the GPU's dead time. That is the key move dynamic
quantization unlocks.

## The actuator: dynamic quantization (the axis that gives control authority)

Binary residency saturates instantly against the budget. Make the actuator
**precision** and it becomes a throttle:

- **Budget becomes bit-allocation, not headcount.** Constraint is
  `Σ(footprint(expert e at bit-level b_e)) ≤ RAM_budget`. Instead of "which 6,000
  experts fit," it's "how do I spend my byte budget across all of them."
- **"Idealized" = rate-distortion optimal.** Classic water-filling / Lagrangian
  bit allocation: spend bits where they reduce output distortion most. Weight =
  **importance = (activation recency/frequency) × (distortion sensitivity)**.
- **Residency is subsumed.** "Cold" = 0 bits resident (fetch full-precision on
  demand). "Warm" = a low-bit approximation always in RAM. "Hot" = high-bit in
  VRAM. One continuum, not a separate cache + quant scheme.
- **Compensation-LoRA is the actuator's fine-trim.** Pushing an expert to low
  bits injects distortion; a small learned correction pulls it back, stretching
  the bit-budget further. This is the `regional quant + working-set residency +
  compensation-LoRA` triad (task #29) expressed as **one control law** instead of
  three separate tricks.
- **Dynamic = tracking a moving setpoint.** A coding task and a prose task light
  up different experts; the hot set and its precisions re-solve as the workload
  shifts. The loop tracks; the profile is not static.

## Sensors

1. **Activation trace** (recency/frequency) — **BUILT.** `GGML_MOE_TRACE_FILE` at
   the offload seam: one `(u64 tensor_key, u32 expert_id)` record per activated
   expert, in access order. Replay in Rust to fit any policy/budget.
2. **Per-expert quantization sensitivity** (distortion cost) — **NEXT.** Perturb
   one expert's precision, measure the perplexity/KL delta on a held-out corpus.
   Same instrumentation discipline as the trace. Trace × sensitivity = the
   importance field the allocator optimizes over.

## The actuator is a SELECTOR over precomputed tiers, not a runtime quantizer

The precision axis of §2.3 must not re-quantize at runtime — that is expensive and
violates no-compute-at-runtime. Instead:

- **The foundry precomputes each expert at several precisions** (e.g. IQ4 / IQ2 /
  IQ1, each with its own compensation-LoRA) and stores them ALL in the aligned
  container, **tier-keyed** (the manifest/packer gains a tier dimension per
  expert; `llama-moe-pack` extends to emit tiers). Disk is cheap (16 TB D:);
  carrying 2–3 tiers costs a few GB and buys the whole precision axis.
- **The runtime actuator is a pure SELECTOR.** For each expert it chooses *which
  precomputed tier to fetch/keep resident* — no compute, just "fetch tier T of
  expert E." Rate-distortion allocation (§2.3) picks T per expert from the
  importance field. Promotion (a warming expert → higher tier) and demotion (a
  cooling one → lower tier, or evict) are just different cheap reads from tiers
  already sitting on disk.
- **Dynamic, not static.** Static regional quant bakes one precision per expert
  offline. Here the *resident* precision tracks live usage: the selector re-picks
  tiers as the workload shifts, because every tier is pre-staged. Static quant is
  the special case where the selector never changes its choice.

### Promotion / demotion = synaptic plasticity ("all-stars sharp, decaying quant at the cruft")

The design in five words (Joel): **high-fidelity all-star experts, decaying quant
at the cruft.** Precision follows *earned* importance — the all-stars (high recency
× sensitivity, the experts actually carrying the workload) stay sharp (IQ4+,
pinned/VRAM); the cruft (cold, decaying tail) blurs to IQ1 or evicts. All-star
status is *earned by use* and *lost by disuse*, so an expert rises from cruft to
all-star when a new topic opens and falls back when it closes — per node, from its
own workload, which is where diversity comes from. Potentiated synapses vs pruned
ones. The tier tracks a recency-derived value estimate with **asymmetric gain** —
the biological shape (LTP fast, LTD slow):

- **Brand-new active expert → promote FAST** to a good tier. Reuse compounds
  (§4/L4): an expert that just fired is likely to fire again, so a new activation
  is probably the hot set *growing*. Optimism-under-uncertainty is low-regret here.
- **Decaying expert → demote SLOWLY** to a cheap tier (not hard-evict — keep an
  IQ1 hedge so a return is a cheap warm hit, not a full cold fault). Falling
  activation rate = falling future value = the safe place to reclaim bits.
- So the **small quants go to the DECAYING tail, not the newcomers.** Hysteresis
  (slow demote) stops tiers from thrashing on a briefly-cold expert.
- The newcomer optimism is **bounded by sensitivity**: `new_tier ≈ optimistic_prior
  × sensitivity(e)` — don't spend bits on a new expert the sensitivity sensor says
  is low-impact.

This is use-it-or-lose-it with pruning at the tail — the pager as a plastic memory.
"Which is better" is empirically decidable: the trace replay simulates optimistic-
vs-cautious newcomer priors and fast-vs-slow decay, scored on distortion-vs-budget.

### Diversity forms (the grid's strength, not a side effect)

Because each node selects tiers dynamically from *its* workload under *its* budget,
nodes converge to **different per-expert tier assignments** — different resident
*phenotypes*. A node that has served a lot of Rust keeps the Rust-relevant experts
at high tiers and lets the rest fall to IQ1/evicted; a node steeped in biology
keeps a different set sharp. The residency+precision profile is shaped by
experience (the trace is the memory of what the node has done — ties to the
being-axis / experiential plasticity), so it is a learned specialization, not a
static config. Across the grid this yields a **diverse population of specialized
nodes** — which is exactly the misfit-design moat and the source of grid value:
the fractal controller (§3) routes each query to the node whose phenotype already
holds the relevant experts sharp, and pooled diversity beats any single uniform
node. Diversity is the point.

## The policy function is a SEAM (v1 heuristic → v2 trained ML)

Per Joel + M5: design to the control system from the start, then *simulate and train
ML to run it well*. So the policy is a pluggable pure function, not baked logic:

```
policy(
    importance_field,       // per-expert: recency(e) × sensitivity(e)
    recency_prediction,     // the predicted next-token working set
    quant_sensitivity,      // per-expert distortion cost (sensor 2)
    budgets                 // RAM / VRAM / disk-bandwidth ceilings
) -> decision {
    per_expert_tier,        // which precomputed precision to hold (all-star … cruft)
    residency,              // VRAM / RAM / cold
    prefetch                // what to pull ahead of the dead time
}
```

- **v1** = the heuristic in this doc (recency window + rate-distortion tiering +
  LTP/LTD promote/demote). No training; already 3–4× LFU.
- **v2** = an ML policy **trained from the simulation** — the trace-replay harness
  *is* the training environment (offline, reproducible, held-out-scored). Same
  signature; swap the body.
- The pager (continuum-core `ServingExpertPager`) owns the seam and the mechanism;
  the trained model drops into the seam without touching the C++ fetch/cache. This
  is what keeps "design it as control, then learn the controller" honest.

## Controller stages

1. **Predict** the next working set — recency window (v1, no training, already
   3–4× LFU) → learned per-layer persistence + co-activation + optimal window K
   ("train from the simulation", v2). Recovers the LFU→OPT gap (16% → 46% at 10%
   residency is the headroom).
2. **Allocate** precision per expert — rate-distortion / water-filling under the
   RAM budget, weighted by the importance field.
3. **Prefetch** the predicted set ahead of the fetch dead time (Smith-predictor
   feedforward) so I/O overlaps compute.
4. **Compensate** the quantization error with a small LoRA where sensitivity is
   high.

## Division of labor

- **BigMama** — footprint-under-RAM fix (give the actuator authority: find + cut
  the ~58 GB of pageable-private commit beyond the pinned cache so committed <
  physical RAM); the ggml/CUDA/NVMe mechanics; the two sensors; the aligned
  container (llama-moe-pack, done).
- **M5 (Fable)** — the predictor/observer + rate-distortion allocator in
  `ServingExpertPager` (Rust), reading the trace, respecting the RAM budget as a
  hard constraint. The governor `FitPolicy`/`decide_lane` machinery already
  exists ([EXPERT-PAGING-GOVERNOR-SEAM.md](EXPERT-PAGING-GOVERNOR-SEAM.md)); this
  extends it from binary fit to precision allocation.

This unifies tasks **#29** (regional dynamic quant + residency + compensation),
**#33** (aligned container → serving), **#34** (recency residency) into one
controller.

## Trajectory (data-backed)

Recency residency (last 2 tokens, 19 GB, fits RAM once thrash is fixed) → 51% hit
→ ~5 GB/token from disk at the proven 3 GB/s → **~0.55 tok/s, beating WASTE's
0.32**. Reuse still climbing with context length; dynamic quant (smaller resident
footprint → more fits → higher hit) and prefetch (hide the dead time) are the
untapped multipliers toward **1 tok/s**, with the grid ([[product-strategy-vs-exo]])
as the ceiling.

## References

- [EXPERT-PAGING-GOVERNOR-SEAM.md](EXPERT-PAGING-GOVERNOR-SEAM.md) — the seam/contract (M5).
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — sentinel-PGO learns the hot set.
- [OBSERVABILITY-AS-SUBSTRATE.md](OBSERVABILITY-AS-SUBSTRATE.md) — the sensor discipline.
- Memories: `k3-residency-predictor-is-recency`, `k3-serving-fetch-gap-measured`,
  `beat-waste-ladder-and-container-pipeline`, `shrink-the-footprint-dont-declare-cant`.
- Instrumentation: `GGML_MOE_TRACE_FILE` (llama.cpp fork commit 9acc2053f); Rust
  replay tool → to land in `continuum-core` `ServingExpertPager`.
