/**
 * TieredMemoryCache - L1/L2/L3 Memory Recall Architecture
 *
 * Biological principle: the hippocampus doesn't block cognition.
 * You think with whatever memories are readily available.
 * Deeper recall happens in the background and arrives for future thoughts.
 *
 * Cache Tiers (like CPU L1/L2/L3):
 *
 *   L1 — In-memory Map (~0ms)
 *     What you already know. Instant. Used for RAG builds without blocking.
 *     Populated by L2/L3 results. 30s TTL (safety net — events keep it fresh).
 *
 *   L2 — Rust IPC multi-layer recall (~30ms)
 *     Quick recall via Rayon-parallelized 6-layer algorithm.
 *     Runs in background, updates L1 when complete.
 *     Stale-while-revalidate: returns L1 immediately, refreshes async.
 *
 *   L3 — Deep research (seconds to minutes)
 *     Sentinel-driven deep recall: web research, cross-persona queries,
 *     semantic clustering, knowledge graph traversal.
 *     Results arrive asynchronously, enrich L1 for future responses.
 *     (Adapter interface ready — implementation is future work)
 *
 * Each tier is a MemoryRecallAdapter. The cache orchestrates them:
 * - RAG build calls recall() -> returns L1 instantly (never blocks)
 * - If L1 is stale, kicks off L2 refresh in background
 * - L3 can be triggered for important topics (future)
 *
 * This is the same pattern as HTTP stale-while-revalidate,
 * CPU cache hierarchies, and biological memory systems.
 */

import type { PersonaMemory } from '../shared/RAGTypes';

/** Wire format for a single memory from Rust IPC recall */
interface RustRecallMemory {
  readonly id: string;
  readonly memory_type: string;
  readonly content: string;
  readonly timestamp: string;
  readonly relevance_score?: number;
  readonly importance?: number;
}

/** Wire format for layer timing from Rust IPC recall */
interface RustLayerTiming {
  readonly layer: string;
  readonly results_found: number;
}

/** Wire format for Rust multi-layer recall response */
interface RustRecallResponse {
  readonly memories: readonly RustRecallMemory[];
  readonly total_candidates?: number;
  readonly recall_time_ms?: number;
  readonly layer_timings?: readonly RustLayerTiming[];
}

/** Minimal interface for PersonaUser's cognition bridge access */
interface HasRustCognitionBridge {
  readonly rustCognitionBridge: {
    memoryMultiLayerRecall(params: {
      query_text: string | null;
      room_id: string;
      max_results: number;
      layers: null;
    }): Promise<RustRecallResponse>;
  } | null;
}

/**
 * Recall request context — what we're trying to remember
 */
export interface RecallRequest {
  readonly personaId: string;
  readonly roomId: string;
  readonly queryText?: string;
  readonly maxResults: number;
}

/**
 * Recall result from any tier
 */
export interface RecallResult {
  readonly memories: PersonaMemory[];
  readonly tier: 'L1' | 'L2' | 'L3';
  readonly latencyMs: number;
  readonly metadata?: Record<string, unknown>;
}

/**
 * MemoryRecallAdapter — pluggable recall strategy per cache tier.
 * Same adapter pattern as visual/audio source adapters.
 */
export interface MemoryRecallAdapter {
  /** Tier identifier */
  readonly tier: 'L1' | 'L2' | 'L3';
  /** Human-readable name */
  readonly name: string;
  /** Expected latency range description */
  readonly latencyProfile: string;

  /**
   * Recall memories for the given request.
   * Each adapter has different speed/depth tradeoffs.
   */
  recall(request: RecallRequest): Promise<RecallResult>;

  /**
   * Whether this adapter is available (e.g., Rust IPC connected, sentinel running)
   */
  isAvailable(): boolean;
}


// ═══════════════════════════════════════════════════════════════════════════
// L1 — In-Memory Cache Adapter
// ═══════════════════════════════════════════════════════════════════════════

interface L1CacheEntry {
  memories: PersonaMemory[];
  cachedAt: number;
  queryText?: string;
}

export class L1InMemoryAdapter implements MemoryRecallAdapter {
  readonly tier = 'L1' as const;
  readonly name = 'in-memory-cache';
  readonly latencyProfile = '~0ms';

  private cache: Map<string, L1CacheEntry> = new Map();
  private readonly ttlMs: number;

  constructor(ttlMs: number = 30_000) {
    this.ttlMs = ttlMs;
  }

  /** Cache key: persona-scoped (memories are per-persona, not per-room in Rust recall) */
  private key(personaId: string): string {
    return personaId;
  }

  async recall(request: RecallRequest): Promise<RecallResult> {
    const entry = this.cache.get(this.key(request.personaId));
    if (entry) {
      return {
        memories: entry.memories.slice(0, request.maxResults),
        tier: 'L1',
        latencyMs: 0,
        metadata: {
          cachedAt: entry.cachedAt,
          ageMs: Date.now() - entry.cachedAt,
          originalQuery: entry.queryText,
        }
      };
    }
    return { memories: [], tier: 'L1', latencyMs: 0 };
  }

  isAvailable(): boolean {
    return true; // Always available
  }

  /** Check if cache entry exists and is fresh */
  isFresh(personaId: string): boolean {
    const entry = this.cache.get(this.key(personaId));
    if (!entry) return false;
    return (Date.now() - entry.cachedAt) < this.ttlMs;
  }

  /** Check if cache entry exists at all (even stale) */
  hasEntry(personaId: string): boolean {
    return this.cache.has(this.key(personaId));
  }

  /** Populate L1 from a higher-tier recall result */
  populate(personaId: string, memories: PersonaMemory[], queryText?: string): void {
    this.cache.set(this.key(personaId), {
      memories,
      cachedAt: Date.now(),
      queryText,
    });
  }

  /** Get cached memories for a persona (returns empty array if no entry) */
  getMemories(personaId: string): PersonaMemory[] {
    const entry = this.cache.get(this.key(personaId));
    return entry ? entry.memories : [];
  }

  /** Invalidate cache for a persona (e.g., after new memory consolidation) */
  invalidate(personaId: string): void {
    this.cache.delete(this.key(personaId));
  }

  /** Clear all entries */
  clear(): void {
    this.cache.clear();
  }
}


// ═══════════════════════════════════════════════════════════════════════════
// L2 — Rust IPC Multi-Layer Recall Adapter
// ═══════════════════════════════════════════════════════════════════════════

export class L2RustRecallAdapter implements MemoryRecallAdapter {
  readonly tier = 'L2' as const;
  readonly name = 'rust-multi-layer-recall';
  readonly latencyProfile = '~30-200ms';

  async recall(request: RecallRequest): Promise<RecallResult> {
    const startTime = performance.now();

    try {
      // Lazy import to avoid circular deps — same pattern as SemanticMemorySource
      const { UserDaemonServer } = await import('../../../daemons/user-daemon/server/UserDaemonServer');
      const userDaemon = UserDaemonServer.getInstance();

      if (!userDaemon) {
        return { memories: [], tier: 'L2', latencyMs: performance.now() - startTime };
      }

      const personaUser = userDaemon.getPersonaUser(request.personaId);
      if (!personaUser) {
        return { memories: [], tier: 'L2', latencyMs: performance.now() - startTime };
      }

      const bridge = ('rustCognitionBridge' in personaUser)
        ? (personaUser as HasRustCognitionBridge).rustCognitionBridge
        : null;
      if (!bridge) {
        return { memories: [], tier: 'L2', latencyMs: performance.now() - startTime };
      }

      // Single IPC call -> Rust runs all 6 recall layers in parallel via Rayon
      const result = await bridge.memoryMultiLayerRecall({
        query_text: request.queryText ?? null,
        room_id: request.roomId,
        max_results: request.maxResults,
        layers: null, // All layers
      });

      const memories: PersonaMemory[] = result.memories.map((mem: RustRecallMemory) => ({
        id: mem.id,
        type: this.mapMemoryType(mem.memory_type),
        content: mem.content,
        timestamp: new Date(mem.timestamp),
        relevanceScore: mem.relevance_score ?? mem.importance ?? 0,
      }));

      const latencyMs = performance.now() - startTime;

      return {
        memories,
        tier: 'L2',
        latencyMs,
        metadata: {
          totalCandidates: result.total_candidates,
          rustRecallMs: result.recall_time_ms,
          layers: result.layer_timings?.map((l: RustLayerTiming) => `${l.layer}(${l.results_found})`).join(', '),
        }
      };
    } catch (error: unknown) {
      return {
        memories: [],
        tier: 'L2',
        latencyMs: performance.now() - startTime,
        metadata: { error: error instanceof Error ? error.message : String(error) }
      };
    }
  }

  isAvailable(): boolean {
    return true; // Availability checked at recall time
  }

  private mapMemoryType(type: string): PersonaMemory['type'] {
    const validTypes = ['observation', 'pattern', 'reflection', 'preference', 'goal'];
    return validTypes.includes(type) ? type as PersonaMemory['type'] : 'observation';
  }
}


// ═══════════════════════════════════════════════════════════════════════════
// TieredMemoryCache — Orchestrator
// ═══════════════════════════════════════════════════════════════════════════

/**
 * TieredMemoryCache — the hippocampus that never blocks.
 *
 * Usage:
 *   const cache = TieredMemoryCache.instance;
 *   const result = await cache.recall(request);
 *   // Returns L1 instantly. L2 refreshes in background if stale.
 *
 * Memory writes (from Hippocampus consolidation) invalidate L1
 * so next recall picks up new memories via L2.
 */
export class TieredMemoryCache {
  private static _instance: TieredMemoryCache | null = null;

  private l1: L1InMemoryAdapter;
  private l2: L2RustRecallAdapter;
  // L3 adapters registered dynamically (sentinel, web research, etc.)
  private l3Adapters: MemoryRecallAdapter[] = [];

  // In-flight L2 refreshes — prevents duplicate background calls
  private refreshing: Map<string, Promise<void>> = new Map();

  private log: (message: string) => void;

  static get instance(): TieredMemoryCache {
    if (!TieredMemoryCache._instance) {
      TieredMemoryCache._instance = new TieredMemoryCache();
    }
    return TieredMemoryCache._instance;
  }

  /** Reset singleton (for testing) */
  static reset(): void {
    TieredMemoryCache._instance?.l1.clear();
    TieredMemoryCache._instance = null;
  }

  private constructor(logger?: (message: string) => void) {
    this.l1 = new L1InMemoryAdapter(30_000); // 30s TTL
    this.l2 = new L2RustRecallAdapter();
    this.log = logger || ((_msg: string) => {});
  }

  /** Set logger after construction (since singleton is lazy) */
  setLogger(logger: (message: string) => void): void {
    this.log = logger;
  }

  /**
   * Register an L3 deep recall adapter (sentinel, web research, etc.)
   * L3 adapters run asynchronously and populate L1 for future responses.
   */
  registerL3Adapter(adapter: MemoryRecallAdapter): void {
    if (adapter.tier !== 'L3') {
      throw new Error(`registerL3Adapter requires tier='L3', got '${adapter.tier}'`);
    }
    this.l3Adapters.push(adapter);
    this.log(`Registered L3 adapter: ${adapter.name} (${adapter.latencyProfile})`);
  }

  /**
   * Recall memories — NEVER blocks on IPC.
   *
   * 1. Returns L1 cache immediately (0ms)
   * 2. If L1 is stale, kicks off background L2 refresh
   * 3. First call for a persona IS a cache miss — makes L2 call (blocking once)
   * 4. All subsequent calls return from L1 cache
   *
   * This is the main entry point for SemanticMemorySource.
   */
  async recall(request: RecallRequest): Promise<RecallResult> {
    const personaId = request.personaId;

    // L1 cache hit — return instantly
    if (this.l1.hasEntry(personaId)) {
      const result = await this.l1.recall(request);

      // If stale, trigger background refresh (don't await)
      if (!this.l1.isFresh(personaId)) {
        this.triggerBackgroundRefresh(request);
      }

      return result;
    }

    // L1 cache miss (cold start) — must call L2 synchronously once
    return this.warmCache(request);
  }

  /**
   * Warm the cache for a persona. Called on cold start (first recall)
   * or can be called proactively during persona initialization.
   */
  async warmCache(request: RecallRequest): Promise<RecallResult> {
    const startTime = performance.now();
    const result = await this.l2.recall(request);

    // Populate L1 with L2 results
    this.l1.populate(request.personaId, result.memories, request.queryText);

    const totalMs = performance.now() - startTime;
    this.log(`L2 warm: ${result.memories.length} memories in ${totalMs.toFixed(1)}ms for ${request.personaId.slice(0, 8)}`);

    return {
      ...result,
      latencyMs: totalMs,
    };
  }

  /**
   * Background L2 refresh — fire-and-forget.
   * Updates L1 when complete. Coalesces duplicate refreshes.
   */
  private triggerBackgroundRefresh(request: RecallRequest): void {
    const key = request.personaId;

    // Already refreshing — skip
    if (this.refreshing.has(key)) return;

    const refreshPromise = (async () => {
      try {
        const result = await this.l2.recall(request);
        if (result.memories.length > 0) {
          this.l1.populate(request.personaId, result.memories, request.queryText);
          this.log(`L2 background refresh: ${result.memories.length} memories in ${result.latencyMs.toFixed(1)}ms for ${request.personaId.slice(0, 8)}`);
        }
      } catch (error: unknown) {
        const message = error instanceof Error ? error.message : String(error);
        this.log(`L2 background refresh failed for ${request.personaId.slice(0, 8)}: ${message}`);
      } finally {
        this.refreshing.delete(key);
      }
    })();

    this.refreshing.set(key, refreshPromise);
  }

  /**
   * Trigger L3 deep recall — asynchronous, results arrive later.
   * For important topics that warrant deeper research.
   * Results populate L1 for future RAG builds.
   */
  triggerDeepRecall(request: RecallRequest): void {
    for (const adapter of this.l3Adapters) {
      if (!adapter.isAvailable()) continue;

      // Fire-and-forget — results enrich future responses
      adapter.recall(request).then(result => {
        if (result.memories.length > 0) {
          // Merge L3 results into L1 (append, don't replace)
          const existing = this.l1.hasEntry(request.personaId)
            ? this.l1.getMemories(request.personaId)
            : [];

          // Deduplicate by ID
          const existingIds = new Set(existing.map((m: PersonaMemory) => m.id));
          const newMemories = result.memories.filter(m => !existingIds.has(m.id));

          if (newMemories.length > 0) {
            this.l1.populate(
              request.personaId,
              [...existing, ...newMemories],
              request.queryText
            );
            this.log(`L3 deep recall: +${newMemories.length} memories from ${adapter.name} for ${request.personaId.slice(0, 8)}`);
          }
        }
      }).catch((error: unknown) => {
        const message = error instanceof Error ? error.message : String(error);
        this.log(`L3 ${adapter.name} failed: ${message}`);
      });
    }
  }

  /**
   * Invalidate L1 cache for a persona.
   * Called by Hippocampus after memory consolidation — new memories
   * should be picked up on next L2 refresh.
   */
  invalidate(personaId: string): void {
    this.l1.invalidate(personaId);
  }

  /**
   * Pre-warm cache for a persona. Called during persona startup.
   * Makes the first recall instant instead of waiting for L2.
   */
  async preWarm(personaId: string, roomId: string): Promise<void> {
    await this.warmCache({
      personaId,
      roomId,
      maxResults: 50,
    });
  }
}
