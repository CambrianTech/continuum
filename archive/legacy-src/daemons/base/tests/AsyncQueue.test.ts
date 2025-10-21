/**
 * Async Queue Tests - Test mutex/semaphore queue implementation
 * Tests the core synchronization primitives for process-based daemons
 */

import { describe, it, beforeEach, afterEach, expect, vi } from 'vitest';
import { AsyncQueue, AsyncMutex, AsyncSemaphore } from '../AsyncQueue';

describe('AsyncQueue', () => {
  let queue: AsyncQueue<string>;

  beforeEach(() => {
    queue = new AsyncQueue<string>(10);
  });

  afterEach(async () => {
    await queue.drain();
  });

  describe('Basic Operations', () => {
    it('should enqueue and dequeue items', async () => {
      await queue.enqueue('item1');
      await queue.enqueue('item2');
      
      expect(queue.size()).toBe(2);
      
      const item1 = await queue.dequeue();
      const item2 = await queue.dequeue();
      
      expect(item1).toBe('item1');
      expect(item2).toBe('item2');
      expect(queue.size()).toBe(0);
    });

    it('should handle empty queue', () => {
      expect(queue.isEmpty()).toBe(true);
      expect(queue.size()).toBe(0);
    });

    it('should respect max size limit', async () => {
      const smallQueue = new AsyncQueue<string>(2);
      
      await smallQueue.enqueue('item1');
      await smallQueue.enqueue('item2');
      
      await expect(smallQueue.enqueue('item3')).rejects.toThrow('Queue is full');
      
      await smallQueue.drain();
    });
  });

  describe('Batch Operations', () => {
    it('should dequeue multiple items in batch', async () => {
      await queue.enqueue('item1');
      await queue.enqueue('item2');
      await queue.enqueue('item3');
      
      const batch = await queue.dequeueBatch(2);
      
      expect(batch).toEqual(['item1', 'item2']);
      expect(queue.size()).toBe(1);
    });

    it('should handle batch size larger than queue size', async () => {
      await queue.enqueue('item1');
      
      const batch = await queue.dequeueBatch(5);
      
      expect(batch).toEqual(['item1']);
      expect(queue.size()).toBe(0);
    });

    it('should handle empty batch request', async () => {
      await queue.enqueue('item1');
      
      const batch = await queue.dequeueBatch(0);
      
      expect(batch).toEqual([]);
      expect(queue.size()).toBe(1);
    });
  });

  describe('Concurrent Access', () => {
    it('should handle concurrent enqueue operations', async () => {
      const promises = [];
      
      for (let i = 0; i < 5; i++) {
        promises.push(queue.enqueue(`item${i}`));
      }
      
      await Promise.all(promises);
      
      expect(queue.size()).toBe(5);
    });

    it('should handle concurrent dequeue operations', async () => {
      // Pre-fill queue
      for (let i = 0; i < 5; i++) {
        await queue.enqueue(`item${i}`);
      }
      
      const promises = [];
      const results: (string | null)[] = [];
      
      for (let i = 0; i < 5; i++) {
        promises.push(
          queue.dequeue().then(item => {
            results.push(item);
          })
        );
      }
      
      await Promise.all(promises);
      
      expect(results).toHaveLength(5);
      expect(results.every(item => item !== null)).toBe(true);
      expect(queue.size()).toBe(0);
    });
  });

  describe('Blocking Behavior', () => {
    it('should block dequeue when queue is empty', async () => {
      let resolved = false;
      
      const dequeuePromise = queue.dequeue().then(() => {
        resolved = true;
      });
      
      // Should not resolve immediately
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(resolved).toBe(false);
      
      // Add item to unblock
      await queue.enqueue('item1');
      await dequeuePromise;
      
      expect(resolved).toBe(true);
    });

    it('should unblock multiple waiting dequeuers', async () => {
      const results: string[] = [];
      
      // Start multiple dequeuers
      const dequeuePromises = [
        queue.dequeue().then(item => results.push(item!)),
        queue.dequeue().then(item => results.push(item!)),
        queue.dequeue().then(item => results.push(item!))
      ];
      
      // Add items to unblock
      await queue.enqueue('item1');
      await queue.enqueue('item2');
      await queue.enqueue('item3');
      
      await Promise.all(dequeuePromises);
      
      expect(results).toHaveLength(3);
      expect(results.sort()).toEqual(['item1', 'item2', 'item3']);
    });
  });

  describe('Shutdown and Drain', () => {
    it('should drain queue and prevent new enqueues', async () => {
      await queue.enqueue('item1');
      await queue.enqueue('item2');
      
      const drainPromise = queue.drain();
      
      // Should prevent new enqueues
      await expect(queue.enqueue('item3')).rejects.toThrow('Queue is shutting down');
      
      await drainPromise;
      
      expect(queue.size()).toBe(2); // Items still in queue but marked for draining
    });

    it('should emit drain events', async () => {
      const drainingHandler = vi.fn();
      const drainedHandler = vi.fn();
      
      queue.on('draining', drainingHandler);
      queue.on('drained', drainedHandler);
      
      await queue.enqueue('item1');
      await queue.drain();
      
      expect(drainingHandler).toHaveBeenCalled();
      expect(drainedHandler).toHaveBeenCalled();
    });
  });

  describe('Event Emission', () => {
    it('should emit enqueued events', async () => {
      const handler = vi.fn();
      queue.on('enqueued', handler);
      
      await queue.enqueue('item1');
      
      expect(handler).toHaveBeenCalledWith('item1');
    });

    it('should emit dequeued events', async () => {
      const handler = vi.fn();
      queue.on('dequeued', handler);
      
      await queue.enqueue('item1');
      await queue.dequeue();
      
      expect(handler).toHaveBeenCalledWith(['item1']);
    });
  });
});

describe('AsyncMutex', () => {
  let mutex: AsyncMutex;

  beforeEach(() => {
    mutex = new AsyncMutex();
  });

  describe('Basic Locking', () => {
    it('should allow single lock', async () => {
      await mutex.lock();
      mutex.unlock();
      
      // Should not throw
      expect(true).toBe(true);
    });

    it('should prevent double unlock', () => {
      expect(() => mutex.unlock()).toThrow('Cannot unlock a mutex that is not locked');
    });

    it('should block second lock until first is released', async () => {
      await mutex.lock();
      
      let secondLockAcquired = false;
      
      const secondLockPromise = mutex.lock().then(() => {
        secondLockAcquired = true;
      });
      
      // Should not acquire immediately
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(secondLockAcquired).toBe(false);
      
      // Release first lock
      mutex.unlock();
      await secondLockPromise;
      
      expect(secondLockAcquired).toBe(true);
    });
  });

  describe('Queue Management', () => {
    it('should handle multiple waiting locks', async () => {
      await mutex.lock();
      
      const results: number[] = [];
      
      // Start multiple lock attempts
      const lockPromises = [
        mutex.lock().then(() => results.push(1)),
        mutex.lock().then(() => results.push(2)),
        mutex.lock().then(() => results.push(3))
      ];
      
      // Release locks one by one
      mutex.unlock();
      await new Promise(resolve => setTimeout(resolve, 1));
      
      mutex.unlock();
      await new Promise(resolve => setTimeout(resolve, 1));
      
      mutex.unlock();
      await Promise.all(lockPromises);
      
      expect(results).toEqual([1, 2, 3]);
    });
  });
});

describe('AsyncSemaphore', () => {
  let semaphore: AsyncSemaphore;

  beforeEach(() => {
    semaphore = new AsyncSemaphore(2);
  });

  describe('Basic Operations', () => {
    it('should allow acquisition up to permit count', async () => {
      await semaphore.acquire();
      await semaphore.acquire();
      
      expect(semaphore.available()).toBe(0);
    });

    it('should block when no permits available', async () => {
      await semaphore.acquire();
      await semaphore.acquire();
      
      let thirdAcquired = false;
      
      const thirdAcquirePromise = semaphore.acquire().then(() => {
        thirdAcquired = true;
      });
      
      // Should not acquire immediately
      await new Promise(resolve => setTimeout(resolve, 10));
      expect(thirdAcquired).toBe(false);
      
      // Release permit
      semaphore.release();
      await thirdAcquirePromise;
      
      expect(thirdAcquired).toBe(true);
    });

    it('should track available permits', () => {
      expect(semaphore.available()).toBe(2);
      
      semaphore.release();
      expect(semaphore.available()).toBe(3);
    });
  });

  describe('Try Acquire', () => {
    it('should try acquire when permits available', () => {
      expect(semaphore.tryAcquire()).toBe(true);
      expect(semaphore.available()).toBe(1);
    });

    it('should fail try acquire when no permits available', async () => {
      await semaphore.acquire();
      await semaphore.acquire();
      
      expect(semaphore.tryAcquire()).toBe(false);
      expect(semaphore.available()).toBe(0);
    });
  });

  describe('Release Management', () => {
    it('should release to waiting acquirers', async () => {
      await semaphore.acquire();
      await semaphore.acquire();
      
      const results: number[] = [];
      
      // Start waiting acquirers
      const acquirePromises = [
        semaphore.acquire().then(() => results.push(1)),
        semaphore.acquire().then(() => results.push(2))
      ];
      
      // Release permits
      semaphore.release();
      semaphore.release();
      
      await Promise.all(acquirePromises);
      
      expect(results).toEqual([1, 2]);
    });
  });
});