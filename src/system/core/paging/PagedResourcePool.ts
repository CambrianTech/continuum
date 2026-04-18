/**
 * PagedResourcePool — the unified paging primitive for Continuum.
 *
 * The system already pages at multiple layers without sharing the pattern:
 *   - LoRA / genome adapters (page in by skill domain demand)
 *   - Persona slots (page in by activity / pressure)
 *   - Memory recall (TieredMemoryCache: L1/L2/L3 stale-while-revalidate)
 *   - Sentinel skills (page in by task)
 *   - KV cache (PagedAttention does this in vllm-metal)
 *   - MoE experts (page in by router decision)
 *   - Vision description cache (content-addressed dedup)
 *
 * Each implements its own version of: load-on-demand, dedup-in-flight,
 * reference-count to keep alive, evict-under-pressure. Drift between
 * implementations is the bug — different consumers interpret pressure
 * differently, evict differently, miss the cache differently.
 *
 * `PagedResourcePool<TKey, TValue>` extracts the common shape:
 *
 *   - **Single-flight load** — concurrent requests for the same key share
 *     ONE in-flight promise. No duplicate work, no race.
 *   - **Reference-counted pinning** — consumers pin a resource to keep it
 *     resident. When the last reference drops, the resource becomes
 *     evictable (not immediately evicted — eviction is pressure-driven).
 *   - **Pressure-aware eviction** — the broker can request the pool drop
 *     unpinned entries by some priority (LRU, size-weighted, custom).
 *   - **Content-addressed reuse** — TKey can be a hash of content so
 *     identical inputs return the cached value without recomputation.
 *
 * The same primitive serves:
 *   `KVCachePool<PrefixHash, KVPages>` — vllm-metal handles internally;
 *     a thin wrapper exposes its semantics through the same interface.
 *   `LoRAAdapterPool<AdapterId, LoadedAdapter>` — genome registry adopts.
 *   `EmbeddingCache<ContentHash, Vector>` — fixes the 0/64 hit-rate.
 *   `ModelWeightsPool<ModelId, LoadedModel>` — DMR plus future direct loads.
 *   `MemoryRecallPool<RoomId, RecalledMemories>` — TieredMemoryCache becomes
 *     an instance with L1/L2/L3 loader chain.
 *
 * The `PressureBroker` (separate module) reads `pressure()` from each pool
 * and orchestrates eviction across types — one brain managing the whole
 * memory garden.
 *
 * See: docs/architecture/UNIFIED-PAGING.md for the full architectural
 * picture and migration plan.
 */

/** A handle returned by `pin()`. Caller releases via `unpin()` (or `release()` on the handle).
 * While at least one handle is active, the resource will not be evicted. */
export interface ResourceHandle<TValue> {
  readonly value: TValue;
  release(): void;
}

/** Loader signature — invoked when the pool needs to materialize a missing key.
 * Async by design: most resources (model weights, KV pages, embeddings,
 * deep memory recall) load asynchronously. */
export type ResourceLoader<TKey, TValue> = (key: TKey) => Promise<TValue>;

/** Sizer signature — returns the byte cost (or arbitrary unit cost) of a value.
 * Used by pressure calculations and size-weighted eviction. Pools can use
 * a constant-1 sizer if size doesn't matter (LRU-only eviction). */
export type ResourceSizer<TValue> = (value: TValue) => number;

/** Eviction priority — lower = evict first. Pools use this to decide which
 * unpinned entry to drop when pressure rises. Default: LRU (priority = -lastAccessTime). */
export type EvictionPriority<TValue> = (entry: PoolEntry<TValue>) => number;

/** Internal entry shape — exposed read-only via `entries()` for inspection
 * and via `EvictionPriority` callbacks. */
export interface PoolEntry<TValue> {
  readonly value: TValue;
  readonly sizeBytes: number;
  readonly pinCount: number;
  readonly loadedAt: number; // ms epoch
  readonly lastAccessAt: number; // ms epoch
  readonly accessCount: number;
}

/** Pool configuration — required so callers think about budget intentionally
 * (per Joel: required not optional). Defaults are explicit values, not
 * `Option<>` — sensible numbers that the pool consumer accepts on purpose. */
export interface PagedResourcePoolConfig<TValue> {
  /** Human-readable identifier — appears in pressure logs and metrics. */
  readonly name: string;
  /** Maximum total `sizer(value)` units the pool will hold before
   * pressure forces eviction. Eviction targets dropping back to 75% of this. */
  readonly maxBytes: number;
  /** How big each value is. Default `() => 1` for count-based pools. */
  readonly sizer: ResourceSizer<TValue>;
  /** Eviction order callback. Default: LRU (oldest lastAccessAt evicted first). */
  readonly evictionPriority: EvictionPriority<TValue>;
  /** Loader to materialize missing keys. */
  readonly loader: ResourceLoader<unknown, TValue>;
}

/** Statistics snapshot for monitoring + PressureBroker decisions. */
export interface PoolStats {
  readonly name: string;
  readonly entryCount: number;
  readonly pinnedCount: number;
  readonly totalBytes: number;
  readonly maxBytes: number;
  readonly pressure: number; // 0..1; ratio of usage to capacity
  readonly hitCount: number;
  readonly missCount: number;
  readonly evictionCount: number;
  readonly inflightCount: number;
}

/**
 * The unified paging primitive. Generic over key and value types so the
 * same implementation serves KV cache pages, LoRA adapters, model weights,
 * embeddings, recalled memories, MoE experts.
 */
export class PagedResourcePool<TKey, TValue> {
  private readonly entries: Map<string, MutablePoolEntry<TValue>> = new Map();
  private readonly inflight: Map<string, Promise<TValue>> = new Map();
  private hits = 0;
  private misses = 0;
  private evictions = 0;

  constructor(private readonly config: PagedResourcePoolConfig<TValue>) {}

  /** Get a value by key without pinning. Returns null on miss (no load).
   * Updates lastAccessAt and increments access counter. */
  get(key: TKey): TValue | null {
    const k = this.keyHash(key);
    const entry = this.entries.get(k);
    if (!entry) {
      this.misses++;
      return null;
    }
    entry.lastAccessAt = Date.now();
    entry.accessCount++;
    this.hits++;
    return entry.value;
  }

  /** Load-or-share — if the key isn't present, invoke the loader. Concurrent
   * calls for the same key share ONE loader invocation (single-flight). */
  async loadOrShare(key: TKey): Promise<TValue> {
    const k = this.keyHash(key);
    const existing = this.entries.get(k);
    if (existing) {
      existing.lastAccessAt = Date.now();
      existing.accessCount++;
      this.hits++;
      return existing.value;
    }
    this.misses++;
    const inflight = this.inflight.get(k);
    if (inflight) return inflight;
    // No entry, no in-flight: start a new load. Store the promise so
    // concurrent callers piggyback. Clean up on settle (success OR fail —
    // .finally avoids the rejected-promise-poisons-cache bug).
    const promise = (async () => {
      const value = await this.config.loader(key);
      const sizeBytes = this.config.sizer(value);
      const now = Date.now();
      const entry: MutablePoolEntry<TValue> = {
        value, sizeBytes, pinCount: 0, loadedAt: now, lastAccessAt: now, accessCount: 1,
      };
      this.entries.set(k, entry);
      this.maybeEvict();
      return value;
    })();
    this.inflight.set(k, promise);
    promise.finally(() => this.inflight.delete(k));
    return promise;
  }

  /** Pin an entry to keep it resident. Returns a handle whose `release()`
   * decrements the pin count. The entry remains evictable when pinCount=0. */
  pin(key: TKey): ResourceHandle<TValue> | null {
    const k = this.keyHash(key);
    const entry = this.entries.get(k);
    if (!entry) return null;
    entry.pinCount++;
    entry.lastAccessAt = Date.now();
    let released = false;
    return {
      value: entry.value,
      release: () => {
        if (released) return;
        released = true;
        entry.pinCount = Math.max(0, entry.pinCount - 1);
      },
    };
  }

  /** Force-evict by key, regardless of pin count. Use sparingly — the
   * normal path is `maybeEvict()` triggered by pressure. */
  evict(key: TKey): boolean {
    const k = this.keyHash(key);
    if (!this.entries.delete(k)) return false;
    this.evictions++;
    return true;
  }

  /** Snapshot statistics for monitoring or a PressureBroker query. */
  stats(): PoolStats {
    let totalBytes = 0;
    let pinnedCount = 0;
    for (const entry of this.entries.values()) {
      totalBytes += entry.sizeBytes;
      if (entry.pinCount > 0) pinnedCount++;
    }
    return {
      name: this.config.name,
      entryCount: this.entries.size,
      pinnedCount,
      totalBytes,
      maxBytes: this.config.maxBytes,
      pressure: this.config.maxBytes > 0 ? totalBytes / this.config.maxBytes : 0,
      hitCount: this.hits,
      missCount: this.misses,
      evictionCount: this.evictions,
      inflightCount: this.inflight.size,
    };
  }

  /** Reduce occupancy to 75% of maxBytes by evicting unpinned entries
   * in eviction-priority order (lowest priority first). Pinned entries
   * are never touched here; the consumer decides when to release. */
  private maybeEvict(): void {
    const targetBytes = Math.floor(this.config.maxBytes * 0.75);
    let totalBytes = 0;
    for (const entry of this.entries.values()) totalBytes += entry.sizeBytes;
    if (totalBytes <= this.config.maxBytes) return;
    // Build candidate list: only unpinned entries, sorted by eviction priority.
    const candidates: Array<[string, MutablePoolEntry<TValue>]> = [];
    for (const [k, entry] of this.entries) {
      if (entry.pinCount === 0) candidates.push([k, entry]);
    }
    candidates.sort(([, a], [, b]) =>
      this.config.evictionPriority(a) - this.config.evictionPriority(b)
    );
    for (const [k, entry] of candidates) {
      if (totalBytes <= targetBytes) break;
      this.entries.delete(k);
      totalBytes -= entry.sizeBytes;
      this.evictions++;
    }
  }

  /** Stable string hash of the key — supports complex object keys. Default
   * uses JSON serialization; consumers with structured keys (tuples, etc.)
   * can pre-hash to a string and pass that as TKey. */
  private keyHash(key: TKey): string {
    if (typeof key === 'string') return key;
    if (typeof key === 'number' || typeof key === 'boolean') return String(key);
    return JSON.stringify(key);
  }
}

/** Default LRU eviction priority — lower priority = evict first. We
 * negate lastAccessAt so the OLDEST access becomes the LOWEST priority. */
export function lruPriority<TValue>(): EvictionPriority<TValue> {
  return (entry) => -entry.lastAccessAt;
}

/** Size-weighted LRU — among equally old entries, evict the largest first
 * (frees more memory per eviction). Useful for embedding caches and
 * model-weight pools where some entries are dramatically larger than others. */
export function sizeWeightedLruPriority<TValue>(): EvictionPriority<TValue> {
  return (entry) => -entry.lastAccessAt - entry.sizeBytes * 0.001;
}

/** Constant unit-1 sizer for count-based pools. */
export function unitSizer<TValue>(): ResourceSizer<TValue> {
  return () => 1;
}

// Internal mutable view of PoolEntry — exposed only within this module.
interface MutablePoolEntry<TValue> {
  value: TValue;
  sizeBytes: number;
  pinCount: number;
  loadedAt: number;
  lastAccessAt: number;
  accessCount: number;
}
