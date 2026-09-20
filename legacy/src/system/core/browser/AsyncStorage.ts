/**
 * AsyncStorage - Non-blocking localStorage wrapper
 *
 * Problem: localStorage.* operations are synchronous and block the main thread.
 * When multiple widgets read/write state, this causes UI hangs.
 *
 * Solution: Queue writes with debouncing, defer to requestIdleCallback.
 * Reads check pending queue first (most recent value wins).
 *
 * Usage:
 *   // Instead of localStorage.setItem(key, value)
 *   asyncStorage.setItem(key, value);
 *
 *   // Instead of localStorage.getItem(key)
 *   const value = asyncStorage.getItem(key);
 */

interface QueuedWrite {
  key: string;
  value: string | null; // null means removeItem
  timestamp: number;
}

class AsyncStorageQueue {
  private queue: Map<string, QueuedWrite> = new Map();
  private flushScheduled = false;
  private readonly DEBOUNCE_MS = 50;
  private readonly BATCH_SIZE = 10;

  /**
   * Queue a write operation (non-blocking)
   */
  setItem(key: string, value: string): void {
    this.queue.set(key, {
      key,
      value,
      timestamp: Date.now()
    });
    this.scheduleFlush();
  }

  /**
   * Queue a remove operation (non-blocking)
   */
  removeItem(key: string): void {
    this.queue.set(key, {
      key,
      value: null,
      timestamp: Date.now()
    });
    this.scheduleFlush();
  }

  /**
   * Get item - checks pending writes first, then localStorage
   * This is intentionally synchronous for read-your-writes consistency
   */
  getItem(key: string): string | null {
    // Check pending writes first (most recent value)
    const pending = this.queue.get(key);
    if (pending !== undefined) {
      return pending.value;
    }
    // Fall back to actual storage
    return localStorage.getItem(key);
  }

  /**
   * Check if key exists (in queue or storage)
   */
  hasItem(key: string): boolean {
    if (this.queue.has(key)) {
      return this.queue.get(key)!.value !== null;
    }
    return localStorage.getItem(key) !== null;
  }

  /**
   * Get all keys (combines queue and storage)
   */
  keys(): string[] {
    const storageKeys = new Set<string>();

    // Get all localStorage keys
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key) storageKeys.add(key);
    }

    // Add pending creates, remove pending deletes
    for (const [key, write] of this.queue) {
      if (write.value !== null) {
        storageKeys.add(key);
      } else {
        storageKeys.delete(key);
      }
    }

    return Array.from(storageKeys);
  }

  /**
   * Schedule a flush to localStorage (deferred, batched)
   */
  private scheduleFlush(): void {
    if (this.flushScheduled) return;
    this.flushScheduled = true;

    // Use setTimeout for debouncing, then requestIdleCallback for actual write
    setTimeout(() => {
      if (typeof requestIdleCallback === 'function') {
        requestIdleCallback(() => this.flush(), { timeout: 1000 });
      } else {
        // Fallback for environments without requestIdleCallback
        setTimeout(() => this.flush(), 0);
      }
    }, this.DEBOUNCE_MS);
  }

  /**
   * Flush pending writes to localStorage
   */
  private flush(): void {
    this.flushScheduled = false;

    if (this.queue.size === 0) return;

    // Take a snapshot and clear queue
    const batch = Array.from(this.queue.values());
    this.queue.clear();

    // Process in chunks to avoid blocking
    this.processBatch(batch, 0);
  }

  /**
   * Process batch of writes in chunks
   */
  private processBatch(batch: QueuedWrite[], startIndex: number): void {
    const endIndex = Math.min(startIndex + this.BATCH_SIZE, batch.length);

    // Process this chunk
    for (let i = startIndex; i < endIndex; i++) {
      const write = batch[i];
      try {
        if (write.value !== null) {
          localStorage.setItem(write.key, write.value);
        } else {
          localStorage.removeItem(write.key);
        }
      } catch (error) {
        if (error instanceof DOMException && error.name === 'QuotaExceededError') {
          this.evictEntityCache();
          // Retry once after eviction
          try {
            if (write.value !== null) {
              localStorage.setItem(write.key, write.value);
            }
          } catch {
            console.warn(`AsyncStorage: Quota exceeded for ${write.key} even after eviction`);
          }
        } else {
          console.error(`AsyncStorage: Failed to write ${write.key}:`, error);
        }
      }
    }

    // Schedule next chunk if more remaining
    if (endIndex < batch.length) {
      if (typeof requestIdleCallback === 'function') {
        requestIdleCallback(() => this.processBatch(batch, endIndex), { timeout: 500 });
      } else {
        setTimeout(() => this.processBatch(batch, endIndex), 0);
      }
    }
  }

  /**
   * Evict oldest entity cache entries to free localStorage quota.
   *
   * Targets `continuum-entity-*` keys (the browser entity cache).
   * Removes the oldest 50% by entity timestamp. These are cache entries —
   * the server has the source of truth; evicted data re-fetches on next access.
   */
  private evictEntityCache(): void {
    const ENTITY_PREFIX = 'continuum-entity-';
    const entityKeys: { key: string; timestamp: number }[] = [];

    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key?.startsWith(ENTITY_PREFIX)) {
        let timestamp = 0;
        try {
          const raw = localStorage.getItem(key);
          if (raw) {
            const parsed = JSON.parse(raw);
            const ts = parsed.updatedAt || parsed.createdAt || parsed.timestamp;
            timestamp = typeof ts === 'string' ? new Date(ts).getTime() : (ts || 0);
          }
        } catch {
          // Unparseable — evict first (timestamp 0 = oldest)
        }
        entityKeys.push({ key, timestamp });
      }
    }

    if (entityKeys.length === 0) return;

    // Sort oldest first, remove 50%
    entityKeys.sort((a, b) => a.timestamp - b.timestamp);
    const removeCount = Math.max(1, Math.ceil(entityKeys.length * 0.5));

    for (let i = 0; i < removeCount; i++) {
      localStorage.removeItem(entityKeys[i].key);
    }

    console.log(`AsyncStorage: Evicted ${removeCount}/${entityKeys.length} entity cache entries to free quota`);
  }

  /**
   * Force immediate flush (use sparingly, e.g., before page unload)
   */
  flushSync(): void {
    for (const write of this.queue.values()) {
      try {
        if (write.value !== null) {
          localStorage.setItem(write.key, write.value);
        } else {
          localStorage.removeItem(write.key);
        }
      } catch (error) {
        if (error instanceof DOMException && error.name === 'QuotaExceededError') {
          this.evictEntityCache();
          try {
            if (write.value !== null) {
              localStorage.setItem(write.key, write.value);
            }
          } catch {
            // Still full — skip
          }
        } else {
          console.error(`AsyncStorage: Sync flush failed for ${write.key}:`, error);
        }
      }
    }
    this.queue.clear();
    this.flushScheduled = false;
  }

  /**
   * Get queue stats for debugging
   */
  getStats(): { pendingWrites: number; pendingKeys: string[] } {
    return {
      pendingWrites: this.queue.size,
      pendingKeys: Array.from(this.queue.keys())
    };
  }
}

/**
 * Singleton instance - use this throughout the app
 */
export const asyncStorage = new AsyncStorageQueue();

// Ensure pending writes are flushed before page unload
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => {
    asyncStorage.flushSync();
  });
}
