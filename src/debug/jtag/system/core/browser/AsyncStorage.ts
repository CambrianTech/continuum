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
        console.error(`AsyncStorage: Failed to write ${write.key}:`, error);
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
        console.error(`AsyncStorage: Sync flush failed for ${write.key}:`, error);
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
