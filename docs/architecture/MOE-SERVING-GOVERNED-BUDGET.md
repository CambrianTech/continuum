# MoE Serving on a Governed Budget

**Status:** draft (BigMama). The governor-interface sections are marked **[M5 OWNS]** — edit
them directly; they describe her lane and I've only sketched the seam I need from it.

**Precedence:** this defers to [CONCURRENCY-STYLE-GUIDE](CONCURRENCY-STYLE-GUIDE.md),
[GENOME-FOUNDRY-SENTINEL](GENOME-FOUNDRY-SENTINEL.md) (`SubstrateGovernor`), and
[INFERENCE-LANES-REALISTIC](INFERENCE-LANES-REALISTIC.md). If it disagrees with those on a
substrate question, they win and this gets reconciled.

## What is proven (keep)

K3 (2.8T MoE, IQ2) serves coherent tokens on a 32 GB card by streaming experts from NVMe through a
bounded resident cache. The mechanism is real and model-agnostic:

- The op-offload seam (`ggml-backend.cpp`) copies only the router-selected experts per token.
- `ResidencyCache` (`ggml-moe-residency.hpp`) is pure mechanism: size-classed pinned pools, recency
  eviction, fed by an `ExpertFetcher` adapter (`DirectReadFetcher` on Windows NVMe, `MmapFaultFetcher`
  portable). Keyed on `ExpertId` = `canonical_name_key(blk.N.ffn_{gate,up,down}_exps)` + index.
- That key is the **universal MoE naming** every one of the 140 `src/models/*.cpp` adapters shares
  (glm4-moe, deepseek2, qwen3moe, kimi-k3, ...). Model-specific differences (K3's KDA attention, SITU)
  live in the per-model graph adapter, not the paging path. **K3 is one model; the pager serves all.**
- top-k override (`--override-kv <arch>.expert_used_count`) cuts fetch + compute; measured `res_exp`
  halves as expected.

## The failure this fixes (the reason for the doc)

Measured on BigMama, 2026-08-01, with the prototype serve script:

```
GGML_MOE_HOST_CACHE_GB=40   (hardcoded in a scratchpad .bat)
-> llama-server private commit 95.9 GB on a 63 GB box   (40 GB pinned + model mmap working set)
-> 33 GB overcommitted to the pagefile -> thrash
-> DirectRead bandwidth collapses 3 GB/s -> ~205 MB/s   ([FETCH] unbuf_ok, resolve_fail=0: not a fallback, real collapse)
-> [RETAIN] pool 5943/5943, evict=25/token, resident=2  (cache retains nothing; re-fetches the whole working set every token)
-> 36.8 s / decode token = 0.027 tok/s
```

The seam was correct (100% intra-token hit, top-8 confirmed). The cause was a **hardcoded residency
budget that overcommitted RAM** — the single failure mode the substrate's pressure/governor
architecture exists to prevent. A budget sized to *fit* free RAM (net of the model's mmap footprint)
never thrashes. On this box that is ~18-22 GB, not 40.

## Principle: the governor owns the budget. Nothing else sets it.

```
SystemProfile (measured free RAM/VRAM, minus model mmap footprint, minus headroom)
   -> SubstrateGovernor / MemoryPressureMonitor        [M5 OWNS]  policy
      -> ServingExpertPager  (observe pressure + expert trace -> budget_bytes + window_k + pin_list)
         -> plan-file  (atomic-rename JSON: {budget_bytes, window_k, pin_list})   the ONE wire
            -> ResidencyCache  (C++)  reads budget from the plan-file. Pure mechanism.
```

Rules:

1. **Delete `GGML_MOE_HOST_CACHE_GB` (and any VRAM-cache env) as a budget *input*.** They were the
   prototype bypass. The C++ cache takes budget **only** from the plan-file. `MoeServingConfig` keeps
   the *operational* knobs (which fetcher, stats/trace/capture sinks) but not the budget.
2. **Budget must be net of the model's mmap working set.** Pinned host cache + mmap resident set must
   never exceed the RAM budget. This is the specific accounting that was missing today.
3. **Mechanism vs policy stays split** (already true in the traits): `ResidencyCache` /
   `DirectReadFetcher` = mechanism; `ServingExpertPager` / `TierPolicy` / `PagerCaptureSink` = policy,
   governed. Pressure rises -> governor shrinks `budget_bytes` -> plan-file updates -> cache evicts.
   That is the RTOS control loop, not a new subsystem.
4. **No prototype rigging.** Serve launch, budget, and measurement come from the governed serving lane
   in continuum-core, not scratchpad `.bat`s and hand-`curl`.

## [M5 OWNS] Governor budget interface — the seam I need, please correct

Open questions for the governor lane (I sketched answers; they are guesses, edit freely):

- Does `SystemProfile.serving_budget_bytes()` already return a **host-residency** budget net of the
  model's mmap footprint, or only a VRAM/headroom figure? If not, where should the mmap footprint be
  measured and subtracted (loader reports resident bytes -> profile)?
- Should `ServingExpertPager` pull the budget from `SystemProfile` on each reconcile tick, or should
  the governor *push* a budget revision through `PressureBroker` that the pager consumes?
- Is there an existing `ResourcePool` / `PagedResourcePool` this residency cache should register with so
  eviction is a first-class governed pool (per the disk-eviction "every cache class has a decided
  eviction story" discipline), rather than a private cache the governor can't see?
- On overcommit, is the correct actuator (a) shrink `budget_bytes`, (b) demote expert precision tier,
  or (c) both, ordered — and does that ordering live in `TierPolicy`?

## Measurement (VDD, not ad-hoc curl)

Ad-hoc `curl` fooled us twice this session (ghost server on a shared port; phantom "hang" that was a
detached client failing to submit). Measurement is a tested harness:

- `tests/test-moe-residency.cpp` — TDD for the keying/config invariants (now includes
  `test_serving_config_from_env`) + a **VDD trace-replay** path that asserts a nonzero reuse floor on a
  real captured trace (`replay_trace`). This is the regression gate for the pager.
- A serve-smoke that asserts decode produces tokens above a floor tok/s, so client flakiness can never
  again read as a code regression. (To build; belongs in the governed serving lane, not a script.)

## Done / next

- **Done, pushed** (k3-adopt `dd8463b74`): `MoeServingConfig` single config-manager (killed 13
  scattered `getenv`), injected into `ResidencyCache` (module no longer reads global env),
  model-agnostic `k3_ -> moe_` rename, `>RAM` mmap prefetch-skip, `test_serving_config_from_env`.
- **Next, gated on the [M5 OWNS] seam above:** make host-cache budget plan-file-only; wire
  `ServingExpertPager` budget from the governed `SystemProfile` net of mmap footprint; register the
  residency cache as a governed pool; build the serve-smoke.
