/**
 * PriorityQueue Tests
 */

import { describe, it, expect } from 'vitest';
import { PriorityQueue, Priority } from '../../../system/core/shared/PriorityQueue';

describe('PriorityQueue', () => {
  describe('basic operations', () => {
    it('should enqueue and dequeue items', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('first', 0.5);
      queue.enqueue('second', 0.5);

      expect(queue.dequeue()).toBe('first');
      expect(queue.dequeue()).toBe('second');
      expect(queue.dequeue()).toBeUndefined();
    });

    it('should return correct size', () => {
      const queue = new PriorityQueue<string>();

      expect(queue.size).toBe(0);

      queue.enqueue('item1', 0.5);
      expect(queue.size).toBe(1);

      queue.enqueue('item2', 0.5);
      expect(queue.size).toBe(2);

      queue.dequeue();
      expect(queue.size).toBe(1);
    });

    it('should check if empty', () => {
      const queue = new PriorityQueue<string>();

      expect(queue.isEmpty()).toBe(true);

      queue.enqueue('item', 0.5);
      expect(queue.isEmpty()).toBe(false);

      queue.dequeue();
      expect(queue.isEmpty()).toBe(true);
    });

    it('should peek without removing', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('first', 0.5);
      queue.enqueue('second', 0.5);

      expect(queue.peek()).toBe('first');
      expect(queue.size).toBe(2); // Should not remove

      expect(queue.dequeue()).toBe('first');
      expect(queue.peek()).toBe('second');
    });

    it('should clear all items', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('item1', 0.5);
      queue.enqueue('item2', 0.8);
      queue.enqueue('item3', 0.2);

      expect(queue.size).toBe(3);

      queue.clear();

      expect(queue.size).toBe(0);
      expect(queue.isEmpty()).toBe(true);
    });
  });

  describe('priority ordering', () => {
    it('should dequeue highest priority first', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('low', Priority.LOW);        // 0.2
      queue.enqueue('high', Priority.HIGH);      // 0.8
      queue.enqueue('critical', Priority.CRITICAL); // 1.0
      queue.enqueue('normal', Priority.NORMAL);  // 0.5

      expect(queue.dequeue()).toBe('critical'); // 1.0
      expect(queue.dequeue()).toBe('high');     // 0.8
      expect(queue.dequeue()).toBe('normal');   // 0.5
      expect(queue.dequeue()).toBe('low');      // 0.2
    });

    it('should maintain FIFO within same priority', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('first-0.5', 0.5);
      queue.enqueue('second-0.5', 0.5);
      queue.enqueue('third-0.5', 0.5);

      expect(queue.dequeue()).toBe('first-0.5');
      expect(queue.dequeue()).toBe('second-0.5');
      expect(queue.dequeue()).toBe('third-0.5');
    });

    it('should handle mixed priorities correctly', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('a-normal', 0.5);
      queue.enqueue('b-high', 0.8);
      queue.enqueue('c-normal', 0.5);
      queue.enqueue('d-critical', 1.0);
      queue.enqueue('e-low', 0.2);

      expect(queue.dequeue()).toBe('d-critical'); // 1.0
      expect(queue.dequeue()).toBe('b-high');     // 0.8
      expect(queue.dequeue()).toBe('a-normal');   // 0.5 (first)
      expect(queue.dequeue()).toBe('c-normal');   // 0.5 (second)
      expect(queue.dequeue()).toBe('e-low');      // 0.2
    });
  });

  describe('priority range filtering', () => {
    it('should get items by priority range', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('critical', 1.0);
      queue.enqueue('high', 0.8);
      queue.enqueue('normal', 0.5);
      queue.enqueue('low', 0.2);
      queue.enqueue('background', 0.1);

      const highPriority = queue.getByPriorityRange(0.7, 1.0);
      expect(highPriority).toEqual(['critical', 'high']);

      const lowPriority = queue.getByPriorityRange(0.0, 0.3);
      expect(lowPriority).toEqual(['low', 'background']);
    });

    it('should not remove items when filtering', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('high', 0.8);
      queue.enqueue('low', 0.2);

      const highPriority = queue.getByPriorityRange(0.7, 1.0);
      expect(highPriority).toEqual(['high']);

      expect(queue.size).toBe(2); // Should not remove
    });
  });

  describe('item removal', () => {
    it('should remove item by predicate', () => {
      const queue = new PriorityQueue<{ id: string; value: number }>();

      queue.enqueue({ id: 'a', value: 1 }, 0.5);
      queue.enqueue({ id: 'b', value: 2 }, 0.5);
      queue.enqueue({ id: 'c', value: 3 }, 0.5);

      const removed = queue.remove(item => item.id === 'b');

      expect(removed).toBe(true);
      expect(queue.size).toBe(2);

      const items = queue.toArray();
      expect(items.map(i => i.id)).toEqual(['a', 'c']);
    });

    it('should return false if item not found', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('item', 0.5);

      const removed = queue.remove(item => item === 'nonexistent');

      expect(removed).toBe(false);
      expect(queue.size).toBe(1);
    });
  });

  describe('edge cases', () => {
    it('should throw error for invalid priority', () => {
      const queue = new PriorityQueue<string>();

      expect(() => queue.enqueue('item', -0.1)).toThrow();
      expect(() => queue.enqueue('item', 1.1)).toThrow();
    });

    it('should handle priority boundaries', () => {
      const queue = new PriorityQueue<string>();

      expect(() => queue.enqueue('min', 0.0)).not.toThrow();
      expect(() => queue.enqueue('max', 1.0)).not.toThrow();

      expect(queue.dequeue()).toBe('max');
      expect(queue.dequeue()).toBe('min');
    });

    it('should return undefined for empty queue operations', () => {
      const queue = new PriorityQueue<string>();

      expect(queue.dequeue()).toBeUndefined();
      expect(queue.peek()).toBeUndefined();
    });
  });

  describe('complex objects', () => {
    interface Task {
      id: string;
      description: string;
    }

    it('should handle complex objects', () => {
      const queue = new PriorityQueue<Task>();

      const task1: Task = { id: '1', description: 'Low priority task' };
      const task2: Task = { id: '2', description: 'High priority task' };

      queue.enqueue(task1, Priority.LOW);
      queue.enqueue(task2, Priority.HIGH);

      expect(queue.dequeue()).toEqual(task2);
      expect(queue.dequeue()).toEqual(task1);
    });
  });

  describe('toArray', () => {
    it('should return all items in priority order', () => {
      const queue = new PriorityQueue<string>();

      queue.enqueue('low', 0.2);
      queue.enqueue('high', 0.8);
      queue.enqueue('normal', 0.5);

      const array = queue.toArray();

      expect(array).toEqual(['high', 'normal', 'low']);
      expect(queue.size).toBe(3); // Should not remove
    });
  });
});
