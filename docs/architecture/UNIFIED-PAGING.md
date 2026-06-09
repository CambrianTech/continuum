# Unified Paging Architecture (Rust)

> **One Rust primitive. Every layer that pages uses it. Levers exposed for intelligent (eventually ML-driven) resource control.** LoRA adapters, KV cache, MoE experts, model weights, embeddings, recalled memories — all instances of the same pattern. The drift between hand-rolled paging implementations IS the bug; extract the shared shape and the system gains coherence at every layer.

Status: design — 2026-04-18. Authored after the persona path stalled tonight because llama.cpp's per-slot KV reservation (262144 tokens × N personas) blew through 32 GB of RAM, while LoRA paging (genome registry), memory recall (TieredMemoryCache), and vision-cache dedup all implemented their own version of the same primitive — none aware of each other.

The primitive lives in **Rust** (`continuum-core`), per Joel's Rust-first rule. The actual paged resources (LoRA weights, KV cache, MoE experts, model handles, embeddings) all live in Rust memory; the pool managing them belongs where they live. Performance and concurrency demand it: paging is on the hot path for every chat turn, and the parallel persona case needs lock-free reads.

---

## The thesis

The system already pages at every layer where it matters:

- **LoRA / genome adapters** — page in by skill domain demand, evict LRU
- **Persona slots** — page in by activity, page out under pressure (Phase 2/3 design)
- **Memory recall (TieredMemoryCache)** — L1/L2/L3 stale-while-revalidate
- **Sentinel skills** — page in by task
- **KV cache (PagedAttention via vllm-metal)** — page in by token allocation
- **MoE experts** — page in by router decision (when MoE forge ships)
- **Vision description cache** — content-addressed dedup
- **Embedding cache** — should be content-addressed but isn't (0/64 hit rate observed)
- **Model weights** — DMR loads once per model, all personas share

All of them implement *some* version of: load on demand, dedup in flight, reference-count to keep alive, evict under pressure. The implementations drifted. **The drift is the bug.** When pressure rises in one pool the others don't know; identical content gets cached twice; consumers needing to pin across pools do it N different ways.

The fix: one primitive — `PagedResourcePool<K, V>` — that all pageable resources adopt. Plus a `PressureBroker` (separate module, follow-up) that reads pressure from each pool and orchestrates eviction holistically. Eventually the broker becomes ML-driven (Apple's RTOS-with-ML approach, possibly LLM-mediated for high-level decisions).

---

## The primitive (Rust)

`core/continuum-core/src/paging/pool.rs` — generic resource pool with the operations every paging layer needs:

```rust
pub struct PagedResourcePool<K, V> { /* ... */ }

impl<K, V> PagedResourcePool<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    pub fn new(config: PoolConfig<V>) -> Self;
    pub fn get(&self, key: &K) -> Option<V>;        // L1 hit, no load
    pub async fn load_or_share<F, Fut>(&self, key: K, loader: F)
        -> Result<V, String>;                       // single-flight load
    pub fn pin(&self, key: &K) -> Option<PinHandle<K, V>>;
    pub fn insert(&self, key: K, value: V);         // for content-hash pools
    pub fn evict(&self, key: &K) -> bool;
    pub async fn stats(&self) -> PoolStats;
}
```

Construction is intentionally explicit (no `Option<>`, every choice declared per Joel's required-not-optional discipline):

```rust
let pool = PagedResourcePool::new(PoolConfig {
    name: "lora-adapters".to_string(),
    max_bytes: 4 * 1024 * 1024 * 1024,    // 4 GB
    sizer: Arc::new(|adapter| adapter.byte_size),
    eviction_priority: size_weighted_lru(),
});
```

### Properties

- **Lock-free reads on the hot path** — `get()` runs under `RwLock::read()`. Per-entry atomics (`AtomicU64` for `last_access_at`, `access_count`; `AtomicU32` for `pin_count`) update without serializing. Concurrent personas hit the cache in parallel.
- **Single-flight via `futures::future::Shared`** — concurrent `load_or_share()` for the same key share ONE loader invocation. Eliminates the duplicate-work race that today causes 14 personas to each fetch the same memory recall.
- **Reference-counted pin** — `pin(key)` returns a `PinHandle`. While at least one handle is alive, eviction skips the entry. Drop releases automatically.
- **Pressure-driven eviction** — `maybe_evict()` triggers when occupancy exceeds `max_bytes`, drops unpinned entries in `eviction_priority` order until back to 75% of capacity. Between firings, occupancy can sit anywhere `≤ max_bytes`.
- **Reject-promise cleanup** — failed loads don't poison the cache slot; the inflight slot is removed on both `Ok` and `Err`. (The exact bug we hit on `CodebaseIndexer.queryCacheLoad` earlier today.)
- **Atomic counters** — `hits`, `misses`, `evictions` are `AtomicU64`. No mutex contention on stat updates.

### Levers exposed (for the future PressureBroker / ML-driven control)

The primitive's design point is *exposing levers* so an intelligent control layer can drive the system without rewriting the pools:

- **`eviction_priority` is a function** — pluggable per-pool strategy. Defaults: `lru_priority`, `size_weighted_lru`. A future broker can install custom policies (e.g., MoE experts prioritized by recent router score; KV cache by prefix-match potential; LoRA adapters by recent-task fitness).
- **`stats()` is rich** — pressure ratio, hit/miss counters, eviction counter, in-flight count, pinned vs unpinned count. Sufficient input for an ML model to predict pressure trajectory.
- **`pin()` / `evict()` are public** — the broker can pin proactively (warm a recipe's adapters), evict surgically (drop a low-priority pool entry without waiting for natural pressure).
- **Per-pool budget (`max_bytes`)** — the broker can dynamically reshape budgets across pools (more KV when fewer LoRAs are active; more model weights when batch sizes are high).
- **Sizer is a function** — consumers can supply weighted sizing (e.g., GPU bytes count more than CPU bytes; quantized weights count differently than full-precision).

### Test coverage (8 passing)

```
get_returns_none_on_miss_and_value_on_hit
load_or_share_dedups_concurrent_loads          // 3 callers → 1 loader invocation
pin_prevents_eviction_under_pressure
maybe_evict_keeps_total_within_max_bytes
eviction_drops_to_target_when_far_over          // single big insert → 75% target
dropped_pin_handle_releases_ref_count
failed_load_does_not_poison_cache               // critical correctness invariant
stats_pressure_tracks_occupancy
```

### What's NOT in the primitive (intentionally)

- **Distributed coherency across machines.** The pool is single-node. Grid-level paging is a separate layer that uses this as a building block.
- **Persistence to disk on eviction.** Pools are in-memory caches. If a consumer wants L2 disk persistence (TieredMemoryCache's L2/L3 do this), it composes two pools or uses the loader to read from disk.
- **Async eviction.** Eviction is synchronous in `maybe_evict`. Heavy values that need async cleanup (e.g., free GPU memory) should put the cleanup in `Drop` for `V` or pre-arrange via the loader's contract.

---

## Migration plan

This commit adds the primitive. Subsequent commits migrate consumers one at a time. Each migration is small and isolated.

### Phase 1 (this commit)
- Add `PagedResourcePool<K, V>` in `continuum-core/src/paging/`
- 8 unit tests covering hot paths and correctness invariants
- Design doc (this file)
- Zero behavior change to existing code

### Phase 2 — embedding cache (`EmbeddingCache`)
- Wrap `PagedResourcePool<ContentHash, Vec<f32>>`
- Replace ad-hoc `Map` cache in `RustEmbeddingClient`
- Fixes the observed `0/64` hit rate on `CodebaseIndexer` re-runs

### Phase 3 — LoRA / genome adapters (`LoRAAdapterPool`)
- Wrap `PagedResourcePool<AdapterId, LoadedAdapter>`
- `GenomeRegistry` becomes a thin wrapper that pins adapters per active task
- Adapter unload on `Drop` for `LoadedAdapter` frees GPU memory

### Phase 4 — KV cache (the immediate user-visible win)
- Route chat through vllm-metal (memento's #925 install) — vllm uses PagedAttention natively, which IS unified paging at the inference layer
- For the GGUF/llama.cpp fallback path: thin Rust wrapper exposes llama.cpp's slot reservation as `PoolStats` so the broker can see it
- Forge MLX-format Qwen3.5 (forge-pipeline addition) so we get PagedAttention without losing forging benefits

### Phase 5 — MoE experts (`MoEExpertPool`)
- When MoE-forged Qwen3.5-A8B-MoE lands: `PagedResourcePool<ExpertId, ExpertWeights>`
- Router pins active experts per token; pool evicts cold experts under pressure
- Enables 32B-MoE intelligence on 16 GB MacBook Air

### Phase 6 — TieredMemoryCache reformulation
- TieredMemoryCache becomes an instance: `MemoryRecallPool` with multi-stage loader
- L1 cache becomes the pool itself; L2/L3 become loaders that compose
- Stale-while-revalidate becomes a generic helper on top of the pool

### Phase 7 — PressureBroker (and the ML lever-pulling)
- Cross-pool eviction orchestration
- Reads `stats().pressure` from each registered pool via IPC
- When global memory pressure rises, calls `evict()` on the pools with highest priority for shedding
- Same broker that gates inference admission and persona slot allocation
- **The ML/LLM control layer plugs in here**: trained model OR LLM consumes the rich stats stream, predicts pressure trajectory, decides eviction policy. Apple's RTOS-with-ML approach, but with the option of LLM-mediated high-level decisions for novel situations.

---

## Why this matters strategically

Paging at every layer is what lets a small machine serve large intelligence:

| Layer | Strategy | Effect |
|---|---|---|
| Model weights | Load once, share | N consumers, 1 copy |
| KV cache | PagedAttention | Per-token allocation, no reservation |
| LoRA adapters | Ref-counted | 14 personas using same skill, 1 copy |
| MoE experts | Router-driven | 32B params on disk, 8B hot per token |
| Memory | Tiered L1/L2/L3 | Recent instant, deep on demand |
| Embeddings | Content-addressed | Same text → same vector, never recomputed |

Combined: a 16 GB MacBook Air running a forged 32B-MoE persona where only the active expert is hot, the active LoRA is shared, KV is paged per-token, memory recalls instantly. **Total intelligence ≫ resident footprint, on every machine class.**

The drift between hand-rolled implementations is what's preventing this today. Unifying it isn't an optimization; it's the foundation that lets every other speedup compound coherently. And by exposing the levers cleanly, we leave room for the system itself (eventually with ML/LLM in the broker) to manage its own resources rather than relying on brittle hand-tuned thresholds.

---

## Acceptance for this commit

- `core/continuum-core/src/paging/pool.rs` exists with the documented interface
- `core/continuum-core/src/paging/mod.rs` re-exports the public surface
- `pub mod paging;` added to `lib.rs`
- 8 unit tests pass; cargo check clean (61 dead-code warnings, no errors)
- Zero behavior change to existing code (additive only — no consumer migrated yet)
- Design doc in `docs/architecture/UNIFIED-PAGING.md`

The primitive is the foundation. The wins arrive as consumers migrate.
