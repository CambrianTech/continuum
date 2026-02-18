/**
 * LayerCache - LRU cache for LoRA layers
 * ========================================
 *
 * Implements an LRU (Least Recently Used) cache for loaded LoRA layers.
 * This is the "virtual memory manager" for genome layers - frequently
 * accessed layers stay in memory, rarely-used layers are evicted.
 *
 * Features:
 * - LRU eviction policy (least recently used layers removed first)
 * - Size-based limits (max total bytes)
 * - Access tracking for statistics
 * - Hit/miss rate monitoring
 *
 * Analogous to virtual memory paging:
 * - Cache hit = page in RAM (fast access)
 * - Cache miss = page on disk (need to load)
 * - Eviction = swap out to disk
 */

import type { UUID } from '../../../system/core/types/JTAGTypes';
import type {
  LoadedLayer,
  CacheEntry,
  CacheStats,
  CacheConfig,
} from '../shared/GenomeAssemblyTypes';

/**
 * LayerCache - LRU cache implementation for LoRA layers
 */
export class LayerCache {
  private entries: Map<UUID, CacheEntry>;
  private accessOrder: UUID[]; // Ordered by access time (oldest first)
  private config: CacheConfig;

  // Statistics
  private hits: number = 0;
  private misses: number = 0;
  private evictions: number = 0;
  private totalSizeBytes: number = 0;

  constructor(config?: Partial<CacheConfig>) {
    this.entries = new Map();
    this.accessOrder = [];

    // Default configuration
    this.config = {
      maxSizeBytes: config?.maxSizeBytes || 4 * 1024 * 1024 * 1024, // 4GB default
      maxEntries: config?.maxEntries,
      trackStats: config?.trackStats ?? true,
      evictionStrategy: config?.evictionStrategy || 'lru',
    };

    console.log(`💾 LayerCache initialized:`);
    console.log(`   Max size: ${this.formatBytes(this.config.maxSizeBytes)}`);
    console.log(`   Eviction strategy: ${this.config.evictionStrategy}`);
  }

  /**
   * Get layer from cache
   * @param layerId UUID of layer
   * @returns Cached layer or null if miss
   */
  get(layerId: UUID): LoadedLayer | null {
    const entry = this.entries.get(layerId);

    if (entry) {
      // Cache hit
      this.hits++;
      this.updateAccessTime(layerId);

      console.log(`✅ Cache HIT: ${layerId} (${this.getHitRate().toFixed(2)}% hit rate)`);
      return entry.layer;
    }

    // Cache miss
    this.misses++;
    console.log(`❌ Cache MISS: ${layerId} (${this.getHitRate().toFixed(2)}% hit rate)`);
    return null;
  }

  /**
   * Store layer in cache
   * @param layerId UUID of layer
   * @param layer Loaded layer to cache
   */
  set(layerId: UUID, layer: LoadedLayer): void {
    // Check if layer already exists (update)
    const existing = this.entries.get(layerId);
    if (existing) {
      // Update existing entry
      existing.layer = layer;
      existing.lastAccessed = Date.now();
      existing.accessCount++;
      this.updateAccessTime(layerId);

      console.log(`🔄 Cache UPDATE: ${layerId}`);
      return;
    }

    // New entry - check if eviction needed
    const needsEviction = this.needsEviction(layer.sizeBytes);
    if (needsEviction) {
      this.evictToMakeSpace(layer.sizeBytes);
    }

    // Create cache entry
    const entry: CacheEntry = {
      layerId,
      layer,
      lastAccessed: Date.now(),
      accessCount: 1,
      sizeBytes: layer.sizeBytes,
      cachedAt: Date.now(),
    };

    this.entries.set(layerId, entry);
    this.accessOrder.push(layerId);
    this.totalSizeBytes += layer.sizeBytes;

    console.log(`💾 Cache SET: ${layerId} (${this.formatBytes(layer.sizeBytes)})`);
    console.log(`   Cache: ${this.entries.size} entries, ${this.formatBytes(this.totalSizeBytes)} / ${this.formatBytes(this.config.maxSizeBytes)}`);
  }

  /**
   * Check if layer is in cache
   * @param layerId UUID of layer
   * @returns True if cached
   */
  has(layerId: UUID): boolean {
    return this.entries.has(layerId);
  }

  /**
   * Manually evict a specific layer
   * @param layerId UUID of layer to evict
   * @returns True if evicted, false if not in cache
   */
  evict(layerId: UUID): boolean {
    const entry = this.entries.get(layerId);
    if (!entry) {
      return false;
    }

    // Remove from cache
    this.entries.delete(layerId);
    this.accessOrder = this.accessOrder.filter((id) => id !== layerId);
    this.totalSizeBytes -= entry.sizeBytes;
    this.evictions++;

    console.log(`🗑️  Cache EVICT: ${layerId} (${this.formatBytes(entry.sizeBytes)} freed)`);
    return true;
  }

  /**
   * Clear entire cache
   */
  clear(): void {
    const entriesCount = this.entries.size;
    const sizeFreed = this.totalSizeBytes;

    this.entries.clear();
    this.accessOrder = [];
    this.totalSizeBytes = 0;

    console.log(`🗑️  Cache CLEARED: ${entriesCount} entries, ${this.formatBytes(sizeFreed)} freed`);
  }

  /**
   * Get cache statistics
   * @returns Current cache stats
   */
  getStats(): CacheStats {
    return {
      entries: this.entries.size,
      totalSizeBytes: this.totalSizeBytes,
      hitRate: this.getHitRate() / 100, // Convert to 0.0 - 1.0
      hits: this.hits,
      misses: this.misses,
      evictionCount: this.evictions,
      maxSizeBytes: this.config.maxSizeBytes,
      utilization: this.totalSizeBytes / this.config.maxSizeBytes,
    };
  }

  /**
   * Reset statistics
   */
  resetStats(): void {
    this.hits = 0;
    this.misses = 0;
    this.evictions = 0;
  }

  /**
   * Get all cached layer IDs
   * @returns Array of cached layer IDs
   */
  getCachedLayerIds(): UUID[] {
    return Array.from(this.entries.keys());
  }

  /**
   * Get cache entry details
   * @param layerId UUID of layer
   * @returns Cache entry or null
   */
  getEntry(layerId: UUID): CacheEntry | null {
    const entry = this.entries.get(layerId);
    return entry ? { ...entry } : null;
  }

  // ============================================================================
  // Private Helper Methods
  // ============================================================================

  /**
   * Check if eviction is needed to fit new layer
   */
  private needsEviction(newLayerSize: number): boolean {
    // Check size limit
    if (this.totalSizeBytes + newLayerSize > this.config.maxSizeBytes) {
      return true;
    }

    // Check entry count limit
    if (this.config.maxEntries && this.entries.size >= this.config.maxEntries) {
      return true;
    }

    return false;
  }

  /**
   * Evict layers to make space for new layer
   * Uses LRU policy (evict least recently used first)
   */
  private evictToMakeSpace(requiredBytes: number): void {
    console.log(`⚠️  Need to evict layers to free ${this.formatBytes(requiredBytes)}`);

    let freedBytes = 0;
    const evictedIds: UUID[] = [];

    // Evict least recently used layers until we have enough space
    while (
      this.accessOrder.length > 0 &&
      (freedBytes < requiredBytes || this.totalSizeBytes + requiredBytes - freedBytes > this.config.maxSizeBytes)
    ) {
      // Get least recently used layer (first in accessOrder)
      const lruLayerId = this.accessOrder[0];
      const entry = this.entries.get(lruLayerId);

      if (!entry) {
        // Shouldn't happen, but handle gracefully
        this.accessOrder.shift();
        continue;
      }

      // Evict this layer
      this.entries.delete(lruLayerId);
      this.accessOrder.shift();
      this.totalSizeBytes -= entry.sizeBytes;
      freedBytes += entry.sizeBytes;
      this.evictions++;
      evictedIds.push(lruLayerId);

      console.log(`   🗑️  Evicted: ${lruLayerId} (${this.formatBytes(entry.sizeBytes)})`);
    }

    console.log(`✅ Eviction complete: ${evictedIds.length} layers, ${this.formatBytes(freedBytes)} freed`);
  }

  /**
   * Update access time for a layer (move to end of LRU list)
   */
  private updateAccessTime(layerId: UUID): void {
    const entry = this.entries.get(layerId);
    if (!entry) {
      return;
    }

    // Update last accessed timestamp
    entry.lastAccessed = Date.now();
    entry.accessCount++;

    // Move to end of access order (most recently used)
    this.accessOrder = this.accessOrder.filter((id) => id !== layerId);
    this.accessOrder.push(layerId);
  }

  /**
   * Calculate hit rate as percentage
   */
  private getHitRate(): number {
    const total = this.hits + this.misses;
    if (total === 0) {
      return 0;
    }
    return (this.hits / total) * 100;
  }

  /**
   * Format bytes as human-readable string
   */
  private formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';

    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const k = 1024;
    const i = Math.floor(Math.log(bytes) / Math.log(k));

    return `${(bytes / Math.pow(k, i)).toFixed(2)} ${units[i]}`;
  }
}
