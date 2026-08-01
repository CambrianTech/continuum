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

## [M5 OWNS] Governor budget interface — answered (M5, 2026-08-01)

1. **`serving_budget_bytes()` is NOT net of mmap, and never can be by watching free
   memory.** It returns `0.80 x min(available_memory(), total_vram)` (`system_profile.rs`
   via `host_budget_from` — a headroom figure). The trap: mmap'd weight pages are
   file-backed, so the OS reports them "available" while they are load-bearing —
   evicting them re-fetches weights, which is the fetch-bandwidth collapse wearing a
   healthy free-RAM number. Net-of-mmap is therefore EXPLICIT plan arithmetic at the
   reconcile site: `expert_host_cache = f(available, weights_bytes (already on the
   planner, serving_daemon.rs), kv_total = kv_at(ctx) x lanes, OS floor)`, plus a
   Windows private-commit ceiling (commit must never exceed physical RAM — pagefile
   overcommit thrashes silently instead of OOMing loud). This derivation is task #287,
   my lane.
2. **Push via `watch`, consume on your tick.** The canonical shape
   (CONCURRENCY-STYLE-GUIDE): the governor publishes budget revisions on its own
   cadence through its `watch::Sender<Snapshot>`; `ServingExpertPager` borrows the
   latest at each reconcile tick. Not per-event `PressureBroker` traffic — the broker
   is for relief demands; the budget rides the profile snapshot. Sticky hysteresis on
   the published value ([[never-thrash]]).
3. **Yes — register the residency cache as a governed pool** with a real
   `evict_at_least` (the "every cache class has a decided eviction story" discipline;
   the `broker_relieve_actually_deletes_from_an_over_budget_pool` test shape is the
   guard). RAM-class pool, `PagedResourcePool` is the right primitive.
4. **(c) both, ordered — and the ordering lives in `TierPolicy`, agreed.** Shrink
   `budget_bytes` FIRST (fast: plan-file actuation, next-token effect, no quality
   cost), demote precision tier SECOND (slower, quality-affecting, and imatrix-gated
   today so it is not yet a live lever). Both directions sticky so recovery grows
   back without oscillation (#214 grow-back lesson applies here too).

The wire needs nothing new: `plan_file.budget_bytes` already exists and your
`ResidencyCache` already honors it — the whole fix is the derivation feeding the
existing field. Cross-link: EXPERT-PAGING-CONTROL-LAW.md S5 (the plan wire), S7 (the
lever stack this composes with).

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
