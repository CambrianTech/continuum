/**
 * PriorityQueue - Reusable priority-based message queue
 *
 * Used by:
 * - SessionDaemon (expiry scheduling)
 * - AIProviderDaemon (request routing)
 * - PersonaUser (task scheduling)
 * - Any daemon needing prioritization
 *
 * Enables natural AI flow: high-priority work processed first,
 * background work happens when idle.
 */

export interface PriorityQueueItem<T> {
  item: T;
  priority: number;
  insertedAt: number;
}

export class PriorityQueue<T> {
  private items: Array<PriorityQueueItem<T>> = [];

  /**
   * Add item to queue with priority
   * @param item - The item to queue
   * @param priority - 0.0 (low) to 1.0 (high)
   */
  enqueue(item: T, priority: number): void {
    if (priority < 0 || priority > 1) {
      throw new Error(`Priority must be between 0.0 and 1.0, got ${priority}`);
    }

    this.items.push({
      item,
      priority,
      insertedAt: Date.now()
    });

    // Sort by priority (high to low), then FIFO for same priority
    this.items.sort((a, b) => {
      const priorityDiff = b.priority - a.priority;
      if (Math.abs(priorityDiff) > 0.001) {
        return priorityDiff; // Higher priority first
      }
      return a.insertedAt - b.insertedAt; // FIFO for same priority
    });
  }

  /**
   * Remove and return highest priority item
   */
  dequeue(): T | undefined {
    const entry = this.items.shift();
    return entry?.item;
  }

  /**
   * Look at highest priority item without removing
   */
  peek(): T | undefined {
    return this.items[0]?.item;
  }

  /**
   * Check if queue is empty
   */
  isEmpty(): boolean {
    return this.items.length === 0;
  }

  /**
   * Get queue size
   */
  get size(): number {
    return this.items.length;
  }

  /**
   * Clear all items
   */
  clear(): void {
    this.items = [];
  }

  /**
   * Get items by priority range (useful for throttling)
   */
  getByPriorityRange(min: number, max: number): T[] {
    return this.items
      .filter(entry => entry.priority >= min && entry.priority <= max)
      .map(entry => entry.item);
  }

  /**
   * Get all items (for debugging/inspection)
   */
  toArray(): T[] {
    return this.items.map(entry => entry.item);
  }

  /**
   * Remove specific item from queue
   */
  remove(predicate: (item: T) => boolean): boolean {
    const index = this.items.findIndex(entry => predicate(entry.item));
    if (index !== -1) {
      this.items.splice(index, 1);
      return true;
    }
    return false;
  }
}

/**
 * Priority levels - standard convention across the system
 */
export enum Priority {
  CRITICAL = 1.0,    // User-facing requests (chat inference)
  HIGH = 0.8,        // Important background work (memory consolidation)
  NORMAL = 0.5,      // Regular operations (file operations)
  LOW = 0.2,         // Retries, deferred work
  BACKGROUND = 0.1   // Health checks, cleanup
}
