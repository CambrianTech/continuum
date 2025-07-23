/**
 * Generic Priority Queue - Reusable for any message type
 */

import type { TimerHandle } from '../CrossPlatformTypes';

export enum Priority {
  CRITICAL = 0,
  HIGH = 1,
  NORMAL = 2,
  LOW = 3
}

export interface QueuedItem<T> {
  item: T;
  priority: Priority;
  timestamp: number;
  retryCount: number;
  maxRetries: number;
  id: string;
}

export interface QueueConfig {
  maxSize: number;
  maxRetries: number;
  flushInterval: number;
  persistenceKey?: string;
}

export class PriorityQueue<T> {
  private queue: QueuedItem<T>[] = [];
  private config: QueueConfig;
  private processing = false;
  private flushTimer?: TimerHandle;

  constructor(config: Partial<QueueConfig> = {}) {
    this.config = {
      maxSize: 1000,
      maxRetries: 3,
      flushInterval: 500,
      ...config
    };
  }

  /**
   * Add item to queue with priority ordering
   */
  enqueue(item: T, priority: Priority = Priority.NORMAL): string {
    const queuedItem: QueuedItem<T> = {
      item,
      priority,
      timestamp: Date.now(),
      retryCount: 0,
      maxRetries: this.config.maxRetries,
      id: `item_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`
    };

    // Insert based on priority (lower number = higher priority)
    const insertIndex = this.queue.findIndex(queued => queued.priority > priority);
    if (insertIndex === -1) {
      this.queue.push(queuedItem);
    } else {
      this.queue.splice(insertIndex, 0, queuedItem);
    }

    // Maintain queue size
    while (this.queue.length > this.config.maxSize) {
      const lowestPriority = Math.max(...this.queue.map(q => q.priority));
      const removeIndex = this.queue.findIndex(q => q.priority === lowestPriority);
      if (removeIndex !== -1) {
        this.queue.splice(removeIndex, 1);
      }
    }

    return queuedItem.id;
  }

  /**
   * Start processing queue with flush handler
   */
  startProcessing(flushHandler: (items: QueuedItem<T>[]) => Promise<QueuedItem<T>[]>): void {
    if (this.processing) return;
    
    this.processing = true;
    
    this.flushTimer = setInterval(async () => {
      if (this.queue.length === 0) return;

      const itemsToFlush = [...this.queue];
      this.queue = [];

      try {
        const failedItems = await flushHandler(itemsToFlush);
        
        // Re-queue failed items with retry logic
        failedItems.forEach(failed => {
          failed.retryCount++;
          if (failed.retryCount <= failed.maxRetries) {
            const delay = Math.min(1000 * Math.pow(2, failed.retryCount), 30000);
            setTimeout(() => {
              this.queue.unshift(failed);
            }, delay);
          }
        });

      } catch (error) {
        // Re-queue all items on handler error
        this.queue = [...itemsToFlush, ...this.queue];
      }
    }, this.config.flushInterval);
  }

  /**
   * Stop processing
   */
  stopProcessing(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = undefined;
    }
    this.processing = false;
  }

  /**
   * Get queue status
   */
  get status() {
    const priorityBreakdown = {} as Record<Priority, number>;
    Object.values(Priority).forEach(priority => {
      if (typeof priority === 'number') {
        priorityBreakdown[priority] = this.queue.filter(q => q.priority === priority).length;
      }
    });

    return {
      size: this.queue.length,
      processing: this.processing,
      priorityBreakdown
    };
  }

  /**
   * Clear queue
   */
  clear(): void {
    this.queue = [];
  }
}