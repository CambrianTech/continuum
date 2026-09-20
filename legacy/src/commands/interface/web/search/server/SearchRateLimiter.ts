/**
 * SearchRateLimiter — Rate limiting and response caching for web search APIs
 *
 * Tracks API usage quotas (Brave: 2000/month free tier) and provides:
 * - Per-provider quota tracking with automatic reset
 * - In-flight request deduplication (same query → shared promise)
 * - LRU cache with TTL for search results (avoids redundant queries in sentinel loops)
 * - Auto-fallback to DuckDuckGo when Brave quota is exhausted
 */

import type { SearchResult } from '../shared/WebSearchTypes';

// ============================================================================
// Configuration
// ============================================================================

interface QuotaConfig {
  /** Maximum requests per period */
  maxRequests: number;
  /** Period duration in milliseconds */
  periodMs: number;
}

interface CacheEntry {
  results: SearchResult[];
  totalResults: number;
  createdAt: number;
}

const BRAVE_QUOTA: QuotaConfig = {
  maxRequests: 2000,
  periodMs: 30 * 24 * 60 * 60 * 1000, // 30 days
};

const CACHE_TTL_MS = 24 * 60 * 60 * 1000; // 24 hours
const MAX_CACHE_ENTRIES = 500;

// ============================================================================
// Rate Limiter
// ============================================================================

export class SearchRateLimiter {
  private _braveRequestCount = 0;
  private _braveWindowStart = Date.now();
  private _cache = new Map<string, CacheEntry>();
  private _inflight = new Map<string, Promise<{ results: SearchResult[]; totalResults: number }>>();

  /**
   * Whether the Brave API quota is available.
   * Returns false when quota is exhausted for the current period.
   */
  get braveAvailable(): boolean {
    this._resetWindowIfExpired();
    return this._braveRequestCount < BRAVE_QUOTA.maxRequests;
  }

  /**
   * Remaining Brave API requests in current period
   */
  get braveRemaining(): number {
    this._resetWindowIfExpired();
    return Math.max(0, BRAVE_QUOTA.maxRequests - this._braveRequestCount);
  }

  /**
   * Record a Brave API request (call AFTER successful request)
   */
  recordBraveRequest(): void {
    this._resetWindowIfExpired();
    this._braveRequestCount++;
  }

  /**
   * Check the cache for a query. Returns undefined on miss.
   */
  getCached(query: string, maxResults: number, domains?: string[]): { results: SearchResult[]; totalResults: number } | undefined {
    const key = this._cacheKey(query, maxResults, domains);
    const entry = this._cache.get(key);

    if (!entry) return undefined;

    // Check TTL
    if (Date.now() - entry.createdAt > CACHE_TTL_MS) {
      this._cache.delete(key);
      return undefined;
    }

    return { results: entry.results, totalResults: entry.totalResults };
  }

  /**
   * Store results in cache
   */
  setCached(query: string, maxResults: number, domains: string[] | undefined, results: SearchResult[], totalResults: number): void {
    const key = this._cacheKey(query, maxResults, domains);

    // Evict oldest entries if cache is full
    if (this._cache.size >= MAX_CACHE_ENTRIES) {
      const firstKey = this._cache.keys().next().value;
      if (firstKey !== undefined) {
        this._cache.delete(firstKey);
      }
    }

    this._cache.set(key, { results, totalResults, createdAt: Date.now() });
  }

  /**
   * Deduplicate in-flight requests. If the same query is already being
   * executed, returns the existing promise instead of starting a new request.
   */
  getInflight(query: string, maxResults: number, domains?: string[]): Promise<{ results: SearchResult[]; totalResults: number }> | undefined {
    const key = this._cacheKey(query, maxResults, domains);
    return this._inflight.get(key);
  }

  /**
   * Register an in-flight request. Returns a cleanup function to call when done.
   */
  setInflight(query: string, maxResults: number, domains: string[] | undefined, promise: Promise<{ results: SearchResult[]; totalResults: number }>): () => void {
    const key = this._cacheKey(query, maxResults, domains);
    this._inflight.set(key, promise);
    return () => this._inflight.delete(key);
  }

  /**
   * Get usage stats for diagnostics
   */
  get stats(): { braveUsed: number; braveRemaining: number; cacheSize: number; cacheHitRate: string } {
    this._resetWindowIfExpired();
    return {
      braveUsed: this._braveRequestCount,
      braveRemaining: this.braveRemaining,
      cacheSize: this._cache.size,
      cacheHitRate: 'N/A',
    };
  }

  private _cacheKey(query: string, maxResults: number, domains?: string[]): string {
    const domainKey = domains?.sort().join(',') ?? '';
    return `${query}|${maxResults}|${domainKey}`;
  }

  private _resetWindowIfExpired(): void {
    if (Date.now() - this._braveWindowStart >= BRAVE_QUOTA.periodMs) {
      this._braveRequestCount = 0;
      this._braveWindowStart = Date.now();
    }
  }
}

/**
 * Singleton instance — shared across all WebSearch invocations in the process
 */
export const searchRateLimiter = new SearchRateLimiter();
