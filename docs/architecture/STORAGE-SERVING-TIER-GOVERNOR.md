# Storage serving-tier governor — NVMe↔cold contention, managed like VRAM/RAM

**Status:** design (2026-08-02, BigMama). Prompted by Joel: *"Like vram and memory, this
contention has to be managed too between cold storage and nvme."* Written because completing the K3
expert container hit a manual disk fight (delete the 662 GB C: copy? pack from D:?) that the
substrate should have resolved on its own.

## The gap

`capacity::system_profile::DriveRole` today has two roles — `System` (OS/working) and `Cold` (the
big offload drive) — and documents `Cold` as *"where MoE expert sets are paged into VRAM on
demand."* **That is wrong for a spinning HDD.** Measured ([[hdd-vs-nvme-is-a-residency-tier]]):
streaming an expert bank off a 130 MB/s HDD = 156–544 s/token = unservable. The model conflates two
physically different tiers:

- **Hot serving tier** — the artifact being *paged per token* (the K3 expert **container**). MUST
  live on NVMe (2.6 GB/s). This is a device-like resource with a **budget** (NVMe free − system
  reserve) and **contention** (two big models' containers can't both be resident).
- **Frozen tier** — artifacts NOT streamed per token: source GGUFs, backups, models-not-currently
  served. These belong on the Cold drive. A GGUF the foundry already re-packed into a container is
  frozen — its NVMe copy is pure duplication.

Nothing governs the boundary, so a human (or an agent) hand-decides which 662 GB file to delete to
fit a 667 GB container. That is the VRAM-OOM story from a year ago, one tier down.

## The model: NVMe is a governed hot-serving tier

Reuse the machinery that already exists for `cargo-target` — do NOT invent a parallel one:

- `system_resources::disk_reporters::TrackedDir` — a named, measured disk cache class.
- `paging::pool::ResourcePool` (`capacity_bytes` / `usage_bytes` / `evict_at_least`) — the eviction
  contract. `CargoTargetPool` is the worked example.
- `disk_eviction::every_cache_class_has_a_decided_eviction_story` — the test that FAILS on an
  undecided class (CLAUDE.md disk doctrine).

Add **one new `ResourcePool`**: `NvmeServingTierPool`.

- `capacity_bytes` = NVMe (System drive) total − a system reserve (OS/build headroom, governed, not
  a magic constant).
- `usage_bytes` = bytes of hot serving artifacts resident on NVMe (containers + device-fit
  overrides + primary GGUFs currently served).
- **`evict_at_least(want)` = migrate the coldest FROZEN / DUPLICATE artifact off NVMe to the Cold
  drive** (not delete — *migrate*, and a verified duplicate that already exists on Cold is a pure
  drop). Coldest-first: models not currently served, then source GGUFs whose experts are already in
  a container, then LRU by last-served. Never evict the artifact the active serve is paging.

The `DriveRole` doc is corrected: `Cold` = frozen storage, **never** the per-token streaming tier.

## Serving integration (dissolves the manual fight)

The serving planner already computes what a model needs. Extend it: before a serve, ask the storage
governor to **make the hot tier resident** —

```
storage.ensure_hot_resident(model) ->
    needs = container_bytes(model) + device_fit_override_bytes(model)   // the per-token-paged set
    if nvme_free >= needs { place / keep on NVMe; done }
    else { NvmeServingTierPool.evict_at_least(needs - nvme_free) }      // migrate frozen/dupes to Cold
    if still short -> Unfittable: route to grid, LOUD (no silent HDD-stream fallback)
```

`capacity::device_fit` already returns `Unfittable` for the VRAM tier; this is the exact same shape
one level down for the **NVMe** tier. The two compose: a model is GPU-servable on this box iff its
resident tier fits VRAM AND its paged tier fits NVMe (after governed frozen-eviction).

## The K3 worked example (what should have happened today)

1. Foundry packs the K3 expert container (667 GB, hot) onto NVMe.
2. `ensure_hot_resident(kimi-k3)` sees NVMe short by ~76 GB.
3. `NvmeServingTierPool.evict_at_least(76 GB)` finds the **C: IQ2 GGUF** — its experts are already in
   the container (frozen) AND a verified identical copy exists on the Cold drive
   (`D:\continuum-cold\…\UD-IQ2_XXS`, 16 shards, 662 GB). It is a pure duplicate → drop from NVMe.
4. Container fits. Serve. **No human deletes anything; the governor tiered it.**

## Boundaries / ownership

- The `ResourcePool` + `TrackedDir` + `disk_eviction` machinery is M5's `system_resources` /
  governor lane — this pool registers there, same as `CargoTargetPool`.
- Duplicate detection (an NVMe GGUF whose identical twin is on Cold) is a small content/shard
  identity check; a hash or (size + shard count + name) match is enough for the "safe to drop"
  verdict — never drop an NVMe artifact without a verified Cold twin.
- Migration cost (NVMe→Cold write, or Cold→NVMe promote for a re-served cold model) is real disk
  bandwidth; the governor leases it like any other, and a promote from a 130 MB/s HDD is a
  first-token latency the planner must surface (it is why "serve a cold model" is not free).

## Why this is the right shape

The pager control law is fractal ([[pager-control-law-is-fractal-to-grid]]): VRAM↔RAM↔NVMe↔Cold is
the same predict-recency → allocate-under-budget → evict-coldest loop at every level. The serving
tier on NVMe is just the next rung down from the expert pager on VRAM — and one rung up from the
grid pooling capacity across nodes. Managing it with the SAME `ResourcePool` contract keeps the
substrate coherent instead of growing a bespoke disk-juggler.
