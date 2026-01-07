/**
 * SyncQueue - Persisted Queue for Offline Operations
 *
 * Stores pending sync operations in localStorage so they survive page refreshes.
 * Operations are processed FIFO (oldest first) when connection is restored.
 *
 * Part of the offline-first dual-storage ORM architecture.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * A sync operation waiting to be sent to the server
 */
export interface SyncOperation {
  /** Operation type */
  readonly type: 'create' | 'update' | 'delete';
  /** Target collection */
  readonly collection: string;
  /** Entity ID */
  readonly id: UUID;
  /** Entity data (for create/update) */
  readonly data?: unknown;
  /** When this operation was queued */
  readonly timestamp: string;
}

/**
 * SyncQueue - FIFO queue persisted to localStorage
 *
 * Usage:
 * ```typescript
 * const queue = new SyncQueue();
 *
 * // Enqueue offline operations
 * queue.enqueue({ type: 'update', collection: 'user_states', id: '123', data: {...} });
 *
 * // Process on reconnect
 * while (queue.hasItems()) {
 *   const op = queue.peek();
 *   await sendToServer(op);
 *   queue.dequeue(); // Remove after success
 * }
 * ```
 */
export class SyncQueue {
  private static readonly STORAGE_KEY = 'continuum-sync-queue';

  /**
   * Add an operation to the end of the queue
   */
  enqueue(op: Omit<SyncOperation, 'timestamp'>): void {
    const queue = this.getQueue();
    queue.push({ ...op, timestamp: new Date().toISOString() });
    this.saveQueue(queue);
  }

  /**
   * Remove and return the first operation from the queue
   */
  dequeue(): SyncOperation | undefined {
    const queue = this.getQueue();
    const op = queue.shift();
    this.saveQueue(queue);
    return op;
  }

  /**
   * Look at the first operation without removing it
   */
  peek(): SyncOperation | undefined {
    return this.getQueue()[0];
  }

  /**
   * Check if there are pending operations
   */
  hasItems(): boolean {
    return this.getQueue().length > 0;
  }

  /**
   * Get the number of pending operations
   */
  get length(): number {
    return this.getQueue().length;
  }

  /**
   * Get all pending operations (read-only)
   */
  getQueue(): SyncOperation[] {
    try {
      const stored = localStorage.getItem(SyncQueue.STORAGE_KEY);
      if (!stored) return [];

      const parsed = JSON.parse(stored);
      // Validate it's an array
      if (!Array.isArray(parsed)) {
        console.warn('SyncQueue: Invalid queue format, clearing');
        this.clear();
        return [];
      }
      return parsed;
    } catch (error) {
      console.warn('SyncQueue: Parse error, clearing queue:', error);
      this.clear();
      return [];
    }
  }

  /**
   * Clear all pending operations
   */
  clear(): void {
    localStorage.removeItem(SyncQueue.STORAGE_KEY);
  }

  /**
   * Save queue to localStorage
   */
  private saveQueue(queue: SyncOperation[]): void {
    if (queue.length === 0) {
      localStorage.removeItem(SyncQueue.STORAGE_KEY);
    } else {
      localStorage.setItem(SyncQueue.STORAGE_KEY, JSON.stringify(queue));
    }
  }

  /**
   * Debug: log queue contents
   */
  debug(): void {
    const queue = this.getQueue();
    console.group('SyncQueue Debug');
    console.log('Pending operations:', queue.length);
    queue.forEach((op, i) => {
      console.log(`  ${i + 1}. ${op.type} ${op.collection}:${op.id} @ ${op.timestamp}`);
    });
    console.groupEnd();
  }
}
