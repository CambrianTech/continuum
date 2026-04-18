# Unified Paging Architecture

> **One primitive. Every layer that pages uses it.** LoRA adapters, KV cache, MoE experts, model weights, embeddings, recalled memories — all instances of the same pattern. The drift between hand-rolled paging implementations IS the bug; extract the shared shape and the system gains coherence at every layer.

Status: design — 2026-04-18. Authored after a session where the persona path stalled because llama.cpp's per-slot KV reservation (262144 tokens × N personas) blew through 32 GB of RAM, while LoRA paging (genome registry), memory recall (TieredMemoryCache), and vision-cache dedup all implemented their own version of the same primitive — none aware of each other.

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
- **Embedding cache** — should be content-addressed dedup but isn't (0/64 hit rate observed)
- **Model weights** — DMR loads once per model, all personas share

All of them implement *some* version of: load on demand, dedup in flight, reference-count to keep alive, evict under pressure. The implementations drifted. Each consumer has its own pressure interpretation, eviction policy, hit/miss counter, single-flight handling. **The drift is the bug.** When pressure rises in one pool the others don't know; when the same content arrives in two pools both cache it twice; when a consumer needs to pin a resource across pools it has to do it N different ways.

The fix: one primitive — `PagedResourcePool<TKey, TValue>` — that all pageable resources adopt. Plus a `PressureBroker` that reads pressure from each pool and orchestrates eviction holistically.

---

## The primitive

`src/system/core/paging/PagedResourcePool.ts` (this PR) — a generic resource pool with the operations every paging layer needs:

```typescript
class PagedResourcePool<TKey, TValue> {
  get(key): TValue | null                       // L1 hit, no load
  loadOrShare(key): Promise<TValue>             // single-flight load
  pin(key): ResourceHandle | null               // ref-counted hold
  evict(key): boolean                           // forced drop
  stats(): PoolStats                            // pressure, hit rate, count
}
```

Construction is intentionally explicit (no Option<>, every choice declared):

```typescript
new PagedResourcePool({
  name: 'lora-adapters',
  maxBytes: 4 * 1024 * 1024 * 1024,           // 4 GB pool
  sizer: (adapter) => adapter.weights.byteLength,
  evictionPriority: sizeWeightedLruPriority(),
  loader: async (id) => loadAdapterFromDisk(id),
});
```

### Properties

- **Single-flight** — concurrent `loadOrShare(k)` for the same key share ONE loader invocation. Eliminates the duplicate-work race that today causes 14 personas to each fetch the same memory recall.
- **Reference-counted pin** — `pin(k)` returns a handle. While at least one handle is alive, eviction skips the entry. When the last handle releases, the entry becomes evictable (not immediately evicted — eviction is pressure-driven).
- **Pressure-driven eviction** — `maybeEvict()` triggers when occupancy exceeds `maxBytes`, drops unpinned entries in `evictionPriority` order until back to 75% of capacity.
- **Reject-promise-cleanup** — `inflight.delete(k)` runs in `.finally`, so a rejected loader doesn't poison the cache slot for future attempts.
- **Eviction policies as functions** — `lruPriority`, `sizeWeightedLruPriority` provided; consumers supply custom policies (e.g., MoE expert pool might prioritize by recent router score; KV pool by prefix match potential).

### What's NOT in the primitive (intentionally)

- **Distributed coherency** across machines. The pool is single-node. Grid-level paging is a separate layer that uses this as a building block.
- **Persistence to disk on eviction.** Pools are in-memory caches. If a consumer wants L2 disk persistence (TieredMemoryCache's L2/L3 do this), it composes two pools or uses the loader to read from disk.
- **Async eviction.** Eviction is synchronous in `maybeEvict`. Heavy values that need async cleanup (e.g., free GPU memory) should make their `value` an object with an `unload()` method called on eviction (future hook).

---

## Migration plan

This PR adds the primitive. Subsequent PRs migrate consumers one at a time. Each migration is small and isolated.

### Phase 1 (this PR)
- Add `PagedResourcePool<TKey, TValue>` + `PressureBroker` interface placeholder
- Design doc (this file)
- Zero behavior change to existing code

### Phase 2 — embedding cache
- `EmbeddingCache extends PagedResourcePool<ContentHash, Float32Array>`
- Fixes the observed 0/64 hit rate on `CodebaseIndexer` re-runs
- Replaces ad-hoc `Map` cache in `RustEmbeddingClient`

### Phase 3 — LoRA / genome adapters
- `LoRAAdapterPool extends PagedResourcePool<AdapterId, LoadedAdapter>`
- `GenomeRegistry` becomes a thin wrapper that pins adapters per active task
- Adapter unload on eviction frees GPU memory

### Phase 4 — KV cache (the immediate user-visible win)
- Route chat through vllm-metal (memento's #925 install) — vllm uses PagedAttention natively, which IS the unified paging at the inference layer
- For the GGUF/llama.cpp fallback path, expose a `KVCachePool` wrapper that surfaces llama.cpp's slot reservation as pool stats so the broker can see it
- Forge MLX-format Qwen3.5 so we get PagedAttention without losing forging benefits

### Phase 5 — MoE experts
- When MoE-forged Qwen3.5-A8B-MoE lands: `MoEExpertPool extends PagedResourcePool<ExpertId, ExpertWeights>`
- Router pins active experts per token; pool evicts cold experts under pressure
- Enables 32B-MoE intelligence on 16 GB MacBook Air

### Phase 6 — TieredMemoryCache reformulation
- TieredMemoryCache becomes an instance: `MemoryRecallPool` with multi-stage loader
- L1 cache becomes the pool itself; L2/L3 become loaders that compose
- Stale-while-revalidate becomes a generic helper on top of the pool

### Phase 7 — PressureBroker
- Cross-pool eviction orchestration
- Reads `stats().pressure` from each registered pool
- When global memory pressure rises, calls `evict()` on the pools with highest priority for shedding (MoE experts before pinned LoRAs, etc.)
- Same broker that gates inference admission and persona slot allocation

---

## Why this matters strategically

Paging at every layer is what lets a small machine serve large intelligence:

| Layer | Paging strategy | Effect |
|---|---|---|
| Model weights | Load once, share across all personas | Same model, N consumers, 1 copy in memory |
| KV cache | PagedAttention | Dynamic per-token allocation, no per-slot reservation |
| LoRA adapters | Genome registry, ref-counted | 14 personas using "typescript-expertise" hold ONE copy |
| MoE experts | Router-driven activation | 32B params on disk, 8B hot per token |
| Memory | Tiered L1/L2/L3 recall | Recent thoughts instant, deep history on demand |
| Embeddings | Content-addressed dedup | Same text → same vector, never recomputed |

Combined: a MacBook Air with 16 GB unified memory can run a forged 32B-MoE persona where only the active expert is hot, the active LoRA is shared, the KV is paged per-token, and memory recalls instantly from L1 with deeper recall in the background. **Total intelligence ≫ resident footprint, on every machine class.** That's the architectural promise — uniformly delivered by one primitive.

The drift between hand-rolled implementations IS what's preventing this today. Unifying it isn't an optimization; it's the foundation that lets every other speedup compound coherently.

---

## Acceptance for this PR

- `src/system/core/paging/PagedResourcePool.ts` exists with the documented interface
- `tsc` clean
- Zero behavior change to existing code (additive only — no consumer migrated yet)
- Design doc in `docs/architecture/UNIFIED-PAGING.md`
- Follow-up issues filed for each migration phase

The primitive is the foundation. The wins arrive as consumers migrate.
