/**
 * WriteDebouncer - Batches and deduplicates data writes
 *
 * Prevents write storms by:
 * 1. Deduplicating identical writes (same collection/id/data)
 * 2. Batching writes over time windows
 * 3. Using requestIdleCallback for non-critical writes
 * 4. Coalescing multiple updates to same entity
 *
 * Pattern: Write-Behind Cache
 * - Local state updated immediately
 * - Server writes batched and debounced
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

export interface PendingWrite {
  collection: string;
  id: UUID;
  data: Record<string, unknown>;
  timestamp: number;
  hash: string;
}

export interface WriteDeBouncerConfig {
  /** Minimum ms between flushes (default: 500) */
  debounceMs: number;
  /** Maximum ms to hold writes before forced flush (default: 5000) */
  maxDelayMs: number;
  /** Use requestIdleCallback when available (default: true) */
  useIdleCallback: boolean;
}

const DEFAULT_CONFIG: WriteDeBouncerConfig = {
  debounceMs: 500,
  maxDelayMs: 5000,
  useIdleCallback: true,
};

/**
 * Simple hash for deduplication
 */
function hashWrite(collection: string, id: UUID, data: Record<string, unknown>): string {
  try {
    return `${collection}:${id}:${JSON.stringify(data)}`;
  } catch {
    return `${collection}:${id}:${Date.now()}`;
  }
}

export class WriteDeBouncer {
  private pending = new Map<string, PendingWrite>(); // key = collection:id
  private recentHashes = new Set<string>(); // Deduplication window
  private hashWindowStart = Date.now();
  private flushTimeout: ReturnType<typeof setTimeout> | null = null;
  private oldestWrite = 0;
  private config: WriteDeBouncerConfig;
  private flushCallback: (writes: PendingWrite[]) => Promise<void>;

  // Stats
  private stats = {
    totalWrites: 0,
    deduplicatedWrites: 0,
    batchedWrites: 0,
    flushedBatches: 0,
  };

  constructor(
    flushCallback: (writes: PendingWrite[]) => Promise<void>,
    config: Partial<WriteDeBouncerConfig> = {}
  ) {
    this.flushCallback = flushCallback;
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Queue a write for batched processing
   * Returns true if write was queued, false if deduplicated
   */
  enqueue(collection: string, id: UUID, data: Record<string, unknown>): boolean {
    const now = Date.now();
    this.stats.totalWrites++;

    // Reset hash window every 100ms
    if (now - this.hashWindowStart > 100) {
      this.recentHashes.clear();
      this.hashWindowStart = now;
    }

    // Deduplication check
    const hash = hashWrite(collection, id, data);
    if (this.recentHashes.has(hash)) {
      this.stats.deduplicatedWrites++;
      console.debug(`📦 WriteDeBouncer: Deduplicated ${collection}:${id}`);
      return false;
    }
    this.recentHashes.add(hash);

    // Coalesce with existing pending write for same entity
    const key = `${collection}:${id}`;
    const existing = this.pending.get(key);

    if (existing) {
      // Merge data (newer overwrites older)
      existing.data = { ...existing.data, ...data };
      existing.timestamp = now;
      existing.hash = hash;
      this.stats.batchedWrites++;
    } else {
      this.pending.set(key, {
        collection,
        id,
        data,
        timestamp: now,
        hash,
      });

      if (this.oldestWrite === 0) {
        this.oldestWrite = now;
      }
    }

    // Schedule flush
    this.scheduleFlush();

    return true;
  }

  /**
   * Schedule a flush with debouncing
   */
  private scheduleFlush(): void {
    const now = Date.now();

    // If we've been holding writes too long, force flush
    if (this.oldestWrite > 0 && now - this.oldestWrite > this.config.maxDelayMs) {
      this.flush();
      return;
    }

    // Clear existing timeout
    if (this.flushTimeout) {
      clearTimeout(this.flushTimeout);
    }

    // Schedule new flush
    this.flushTimeout = setTimeout(() => {
      this.flush();
    }, this.config.debounceMs);
  }

  /**
   * Flush pending writes
   */
  async flush(): Promise<void> {
    if (this.flushTimeout) {
      clearTimeout(this.flushTimeout);
      this.flushTimeout = null;
    }

    if (this.pending.size === 0) {
      return;
    }

    // Capture and clear pending writes
    const writes = Array.from(this.pending.values());
    this.pending.clear();
    this.oldestWrite = 0;
    this.stats.flushedBatches++;

    console.log(`📦 WriteDeBouncer: Flushing ${writes.length} writes (batch #${this.stats.flushedBatches})`);

    // Execute flush
    if (this.config.useIdleCallback && typeof requestIdleCallback !== 'undefined') {
      requestIdleCallback(async () => {
        try {
          await this.flushCallback(writes);
        } catch (error) {
          console.error('📦 WriteDeBouncer: Flush failed:', error);
        }
      });
    } else {
      try {
        await this.flushCallback(writes);
      } catch (error) {
        console.error('📦 WriteDeBouncer: Flush failed:', error);
      }
    }
  }

  /**
   * Force immediate flush (for shutdown, etc.)
   */
  async forceFlush(): Promise<void> {
    await this.flush();
  }

  /**
   * Get current stats
   */
  getStats(): typeof this.stats & { pendingCount: number } {
    return {
      ...this.stats,
      pendingCount: this.pending.size,
    };
  }

  /**
   * Reset stats
   */
  resetStats(): void {
    this.stats = {
      totalWrites: 0,
      deduplicatedWrites: 0,
      batchedWrites: 0,
      flushedBatches: 0,
    };
  }
}

// Singleton for user_states writes
let userStateDebouncer: WriteDeBouncer | null = null;

export function getUserStateDebouncer(
  flushCallback: (writes: PendingWrite[]) => Promise<void>
): WriteDeBouncer {
  if (!userStateDebouncer) {
    userStateDebouncer = new WriteDeBouncer(flushCallback, {
      debounceMs: 500,    // Wait 500ms after last write
      maxDelayMs: 5000,   // But never wait more than 5s
      useIdleCallback: true,
    });
  }
  return userStateDebouncer;
}
