/**
 * VisionDescriptionCache — Two-tier content-addressed cache for vision descriptions.
 *
 * Cache hierarchy:
 * - L1: In-memory TS Map (instant, process-scoped, 500 entries)
 * - L1.5: Rust in-process HashMap via IPC (sub-ms, survives TS restarts, 2000 entries)
 * - In-flight deduplication (concurrent callers await the same promise)
 * - Content-addressing via SHA-256(first 4KB + length)
 *
 * Read path: L1 (TS Map) → L1.5 (Rust HashMap) → miss
 * Write path: L1 (TS Map) + L1.5 (Rust HashMap) simultaneously
 *
 * Separated from VisionDescriptionService so the cache layer is reusable
 * across screenshots, canvas, video frames — anything that needs
 * content-addressed caching of expensive inference results.
 */

import { createHash } from 'crypto';
import { RustCoreIPCClient } from '../../workers/continuum-core/bindings/RustCoreIPC';
import type { VisionDescription } from './VisionDescriptionService';

interface CachedEntry {
  description: VisionDescription;
  lastAccessedAt: number;
}

/** Evict entries not accessed for 30 minutes. Access-based, not creation-based. */
const EVICTION_IDLE_MS = 30 * 60 * 1000;

/** Max L1 cache entries. Each is ~1KB (description text + metadata). */
const MAX_L1_ENTRIES = 500;

export class VisionDescriptionCache {
  /** L1: in-memory Map. Process-lifetime, instant access. */
  private readonly _l1 = new Map<string, CachedEntry>();

  /** In-flight deduplication: content key → promise. */
  private readonly _inflight = new Map<string, Promise<VisionDescription | null>>();

  /** Periodic eviction timer */
  private _evictionTimer: ReturnType<typeof setInterval> | null = null;

  constructor() {
    // Evict idle entries every 5 minutes
    this._evictionTimer = setInterval(() => this.evictIdle(), 5 * 60 * 1000);
    if (this._evictionTimer.unref) this._evictionTimer.unref();
  }

  /**
   * Content-address key: SHA-256 of first 4KB + total length.
   * Hashing the full base64 of a 5MB image would be wasteful — first 4KB + length
   * is sufficient to distinguish images while keeping key generation <1ms.
   */
  contentKey(base64Data: string): string {
    const sample = base64Data.slice(0, 4096);
    return createHash('sha256').update(`${sample}:${base64Data.length}`).digest('hex').slice(0, 16);
  }

  /**
   * Check the status of a description for a given content key.
   * Returns 'cached' if ready, 'inflight' if being processed, 'none' if unknown.
   */
  status(key: string): 'cached' | 'inflight' | 'none' {
    const entry = this._l1.get(key);
    if (entry) {
      entry.lastAccessedAt = Date.now();
      return 'cached';
    }
    if (this._inflight.has(key)) {
      return 'inflight';
    }
    return 'none';
  }

  /**
   * Get a cached description by content key. Returns null on miss.
   * Bumps lastAccessedAt on hit (access-based TTL).
   */
  get(key: string): VisionDescription | null {
    const entry = this._l1.get(key);
    if (entry) {
      entry.lastAccessedAt = Date.now();
      return entry.description;
    }
    return null;
  }

  /**
   * Check Rust L1.5 cache. Returns null on miss or IPC failure.
   * Called by VisionDescriptionService when L1 misses.
   */
  async getFromRust(key: string): Promise<VisionDescription | null> {
    try {
      const rust = RustCoreIPCClient.getInstance();
      const result = await rust.visionCacheGet(key);
      if (result.found && result.description) {
        // Promote to L1 for future sync access
        const desc: VisionDescription = {
          description: result.description,
          modelId: result.model || 'unknown',
          provider: result.provider || 'unknown',
          timestamp: new Date().toISOString(),
          responseTimeMs: result.processingTimeMs || 0,
        };
        this.put(key, desc);
        return desc;
      }
    } catch {
      // Rust not available — degrade gracefully to L1-only
    }
    return null;
  }

  /**
   * Store a description in L1 cache + Rust L1.5.
   */
  put(key: string, description: VisionDescription): void {
    // LRU eviction: drop least recently accessed if at capacity
    if (this._l1.size >= MAX_L1_ENTRIES) {
      let oldestKey: string | null = null;
      let oldestTime = Infinity;
      for (const [k, v] of this._l1) {
        if (v.lastAccessedAt < oldestTime) {
          oldestTime = v.lastAccessedAt;
          oldestKey = k;
        }
      }
      if (oldestKey) this._l1.delete(oldestKey);
    }
    this._l1.set(key, { description, lastAccessedAt: Date.now() });

    // Write-through to Rust L1.5 (fire-and-forget, non-blocking)
    try {
      const rust = RustCoreIPCClient.getInstance();
      rust.visionCachePut(key, {
        description: description.description,
        model: description.modelId,
        provider: description.provider,
        processingTimeMs: description.responseTimeMs,
        confidence: 0.85,
      }).catch(() => { /* Rust not available */ });
    } catch {
      // Rust not connected yet
    }
  }

  /**
   * Register an in-flight promise for deduplication.
   * Concurrent callers for the same key will await the same promise.
   */
  registerInflight(key: string, promise: Promise<VisionDescription | null>): void {
    this._inflight.set(key, promise);
  }

  /**
   * Get the in-flight promise for a key, if one exists.
   */
  getInflight(key: string): Promise<VisionDescription | null> | undefined {
    return this._inflight.get(key);
  }

  /**
   * Clear the in-flight marker for a key (call in finally block).
   */
  clearInflight(key: string): void {
    this._inflight.delete(key);
  }

  /** Cache stats for diagnostics */
  get stats(): { l1Size: number; maxL1: number; inflightCount: number } {
    return {
      l1Size: this._l1.size,
      maxL1: MAX_L1_ENTRIES,
      inflightCount: this._inflight.size,
    };
  }

  /**
   * Evict entries not accessed within the idle window.
   */
  private evictIdle(): void {
    const cutoff = Date.now() - EVICTION_IDLE_MS;
    const keysToDelete: string[] = [];
    for (const [key, entry] of this._l1) {
      if (entry.lastAccessedAt < cutoff) {
        keysToDelete.push(key);
      }
    }
    for (const key of keysToDelete) {
      this._l1.delete(key);
    }
    if (keysToDelete.length > 0) {
      console.log(`[VisionDescriptionCache] Evicted ${keysToDelete.length} idle entries (${this._l1.size} remaining)`);
    }
  }

  /** Cleanup timer on shutdown */
  destroy(): void {
    if (this._evictionTimer) {
      clearInterval(this._evictionTimer);
      this._evictionTimer = null;
    }
  }
}
