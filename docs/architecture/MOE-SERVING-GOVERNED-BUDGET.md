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

## Measured on BigMama (2026-08-01) - inputs for the #287 derivation

Ungoverned static budgets were run to characterize the box (they thrash by design; do NOT read them
as the governed result - a governed budget flexes with KV and never overcommits):

| budget (hardcoded) | private commit | fetch | outcome |
|---|---|---|---|
| 40 GiB | 95.9 GB (>63.4 RAM) | 205 MB/s | pagefile thrash, 0.027 tok/s |
| 8 GiB (during load) | 60.6 GB | **2485 MB/s** | fits RAM, fetch fully recovered |
| 8 GiB (during decode, KV grew) | 63.9 GB | 165 MB/s | tipped over -> thrash again |

Derived inputs for `expert_host_cache = f(available, weights_bytes, kv_total, OS floor)`:
- Total RAM 63.4 GB. Non-cache private footprint ~56 GB (base + KV + CUDA staging).
- K3 top-8 per-token expert working set ~5.5 GB (res_exp 2208).
- So the governed budget on this box is ~6 GB (62 - 56), which holds ~1 token's set -> ~40% recency
  retention, misses fetching at the recovered ~2.5 GB/s. That is the regime that should pull tok/s back
  toward 0.5 - NOT written off. The governor MUST subtract KV growth continuously (decode tipped 8 GiB
  over) and cap on the Windows private-commit ceiling.

## C++ mechanism - COMPLETE (k3-adopt)

The cache honors the governed lease fully: `dd8463b74` config-manager + naming; `848f409c7` budget is
plan-file-only in governed mode (env can't overcommit); `b0e877cbd` poll the plan every token so a
governed budget can turn the cache ON (it starts at 0); `8680cefa9` free pools on budget DECREASE so
the governor can flex the cache down under pressure. Grow + shrink + enable-from-plan all validated
(builds green, test 94/0). Partial-evict-keeping-hottest on shrink is the one future refinement.

## Graduated path: `serving/load kimi-k3` (replaces the rigged .bat)

Three pieces, then a JOINT governed measurement (no more hand-`curl`):
1. **Catalog row for K3** (BigMama lane, `model_registry/catalog.rs`) - servable metadata + the serving
   profile: `--n-cpu-moe 999`, `--override-kv kimi-k3.expert_used_count=int:8`, `-ngl <fit>`,
   `GGML_MOE_DIRECT_READ=1`, `GGML_MOE_PLAN_FILE=<governed>`. Gated on 2+3 so it's not a servable row
   that thrashes.
2. **Serving-lane MoE launch** (`inference/llama_server.rs`, M5) - set those flags + the plan-file env
   for a streaming-MoE model; today the launcher wires KV/context budget but not the MoE offload env.
3. **#287 budget derivation** (M5) - writes `plan_file.budget_bytes` from the profile above, flexing
   with KV. Register the cache as a governed `PagedResourcePool` with `evict_at_least` (the C++ free
   path above is its actuator).
