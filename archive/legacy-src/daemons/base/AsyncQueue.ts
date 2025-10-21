/**
 * Async Queue - Thread-safe queue with mutex/semaphore synchronization
 * Supports efficient producer/consumer pattern for daemon message processing
 */

import { EventEmitter } from 'events';

export class AsyncQueue<T> extends EventEmitter {
  private queue: T[] = [];
  private waitingConsumers: Array<{
    resolve: (value: T[]) => void;
    reject: (error: Error) => void;
    batchSize: number;
  }> = [];
  private mutex = new AsyncMutex();
  private semaphore: AsyncSemaphore;
  private maxSize: number;
  private isShuttingDown = false;

  constructor(maxSize: number = 10000) {
    super();
    this.maxSize = maxSize;
    this.semaphore = new AsyncSemaphore(0); // Start with 0 items available
  }

  /**
   * Add item to queue - returns immediately
   */
  async enqueue(item: T): Promise<void> {
    await this.mutex.lock();
    try {
      if (this.isShuttingDown) {
        throw new Error('Queue is shutting down');
      }
      
      if (this.queue.length >= this.maxSize) {
        throw new Error('Queue is full');
      }

      this.queue.push(item);
      this.semaphore.release(); // Signal that item is available
      this.emit('enqueued', item);
    } finally {
      this.mutex.unlock();
    }
  }

  /**
   * Remove single item from queue - blocks if empty
   */
  async dequeue(): Promise<T | null> {
    const result = await this.dequeueBatch(1);
    return result[0] || null;
  }

  /**
   * Remove multiple items from queue - blocks if empty
   */
  async dequeueBatch(batchSize: number = 1): Promise<T[]> {
    // Wait for at least one item to be available
    await this.semaphore.acquire();
    
    await this.mutex.lock();
    try {
      if (this.isShuttingDown && this.queue.length === 0) {
        return [];
      }

      // Take up to batchSize items
      const items: T[] = [];
      const actualBatchSize = Math.min(batchSize, this.queue.length);
      
      for (let i = 0; i < actualBatchSize; i++) {
        const item = this.queue.shift();
        if (item !== undefined) {
          items.push(item);
        }
      }

      // Release semaphore permits for items we didn't take
      // (we acquired 1 permit but might take more items)
      for (let i = 1; i < actualBatchSize; i++) {
        this.semaphore.tryAcquire(); // Remove extra permits
      }

      this.emit('dequeued', items);
      return items;
    } finally {
      this.mutex.unlock();
    }
  }

  /**
   * Get current queue size
   */
  size(): number {
    return this.queue.length;
  }

  /**
   * Check if queue is empty
   */
  isEmpty(): boolean {
    return this.queue.length === 0;
  }

  /**
   * Drain queue - process all remaining items
   */
  async drain(): Promise<void> {
    this.isShuttingDown = true;
    
    // Wake up any waiting consumers
    this.waitingConsumers.forEach(({ resolve }) => {
      resolve([]);
    });
    this.waitingConsumers = [];

    this.emit('draining');
    
    // Release all remaining semaphore permits
    while (this.queue.length > 0) {
      this.semaphore.release();
    }
    
    this.emit('drained');
  }
}

/**
 * Async Mutex - Ensures exclusive access to shared resources
 */
export class AsyncMutex {
  private locked = false;
  private waitingQueue: Array<{
    resolve: () => void;
    reject: (error: Error) => void;
  }> = [];

  async lock(): Promise<void> {
    if (!this.locked) {
      this.locked = true;
      return;
    }

    return new Promise((resolve, reject) => {
      this.waitingQueue.push({ resolve, reject });
    });
  }

  unlock(): void {
    if (!this.locked) {
      throw new Error('Cannot unlock a mutex that is not locked');
    }

    if (this.waitingQueue.length > 0) {
      const { resolve } = this.waitingQueue.shift()!;
      resolve();
    } else {
      this.locked = false;
    }
  }
}

/**
 * Async Semaphore - Controls access to a limited resource
 */
export class AsyncSemaphore {
  private permits: number;
  private waitingQueue: Array<{
    resolve: () => void;
    reject: (error: Error) => void;
  }> = [];

  constructor(permits: number) {
    this.permits = permits;
  }

  async acquire(): Promise<void> {
    if (this.permits > 0) {
      this.permits--;
      return;
    }

    return new Promise((resolve, reject) => {
      this.waitingQueue.push({ resolve, reject });
    });
  }

  release(): void {
    if (this.waitingQueue.length > 0) {
      const { resolve } = this.waitingQueue.shift()!;
      resolve();
    } else {
      this.permits++;
    }
  }

  tryAcquire(): boolean {
    if (this.permits > 0) {
      this.permits--;
      return true;
    }
    return false;
  }

  available(): number {
    return this.permits;
  }
}