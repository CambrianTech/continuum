/**
 * MockLoRAAdapter Tests
 *
 * Tests for the mock LoRA adapter that doesn't require GPU.
 * Verifies:
 * - Load/unload lifecycle
 * - Memory size tracking
 * - LRU eviction scoring
 * - Priority handling
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { MockLoRAAdapter, type MockLoRAConfig } from './MockLoRAAdapter';
import { randomUUID } from 'crypto';
import type { UUID } from '../../core/types/CrossPlatformUUID';

describe('MockLoRAAdapter', () => {
  let adapter: MockLoRAAdapter;
  let config: MockLoRAConfig;

  beforeEach(() => {
    config = {
      id: randomUUID() as UUID,
      name: 'test-adapter',
      domain: 'test',
      sizeMB: 256,
      priority: 0.5
    };
    adapter = new MockLoRAAdapter(config);
  });

  describe('Construction', () => {
    it('should create adapter with config', () => {
      expect(adapter.getName()).toBe('test-adapter');
      expect(adapter.getDomain()).toBe('test');
      expect(adapter.getSize()).toBe(256);
      expect(adapter.getPriority()).toBe(0.5);
      expect(adapter.isLoaded()).toBe(false);
    });

    it('should default priority to 0.5 if not provided', () => {
      const adapterNoPriority = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'no-priority',
        domain: 'test',
        sizeMB: 128
      });
      expect(adapterNoPriority.getPriority()).toBe(0.5);
    });
  });

  describe('load()', () => {
    it('should load adapter and update state', async () => {
      expect(adapter.isLoaded()).toBe(false);
      expect(adapter.getLoadedAt()).toBeNull();

      await adapter.load();

      expect(adapter.isLoaded()).toBe(true);
      expect(adapter.getLoadedAt()).toBeGreaterThan(0);
      expect(adapter.getLastUsedAt()).toBeGreaterThan(0);
      expect(adapter.getUsageCount()).toBe(1);
    });

    it('should simulate loading delay based on size', async () => {
      const smallAdapter = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'small',
        domain: 'test',
        sizeMB: 128  // <256MB = 50ms delay
      });

      const start = Date.now();
      await smallAdapter.load();
      const elapsed = Date.now() - start;

      expect(elapsed).toBeGreaterThanOrEqual(45);  // Allow some tolerance
      expect(elapsed).toBeLessThan(200);
    });

    it('should throw error if already loaded', async () => {
      await adapter.load();

      await expect(adapter.load()).rejects.toThrow('already loaded');
    });
  });

  describe('unload()', () => {
    it('should unload adapter and reset state', async () => {
      await adapter.load();
      expect(adapter.isLoaded()).toBe(true);

      await adapter.unload();

      expect(adapter.isLoaded()).toBe(false);
      expect(adapter.getLoadedAt()).toBeNull();
    });

    it('should throw error if not loaded', async () => {
      await expect(adapter.unload()).rejects.toThrow('not loaded');
    });
  });

  describe('markUsed()', () => {
    it('should update lastUsedAt timestamp', async () => {
      await adapter.load();
      const firstUsed = adapter.getLastUsedAt();

      // Wait a bit
      await new Promise(resolve => setTimeout(resolve, 10));

      adapter.markUsed();
      const secondUsed = adapter.getLastUsedAt();

      expect(secondUsed).toBeGreaterThan(firstUsed!);
    });

    it('should increment usage count', async () => {
      await adapter.load();
      expect(adapter.getUsageCount()).toBe(1);

      adapter.markUsed();
      expect(adapter.getUsageCount()).toBe(2);

      adapter.markUsed();
      expect(adapter.getUsageCount()).toBe(3);
    });
  });

  describe('calculateEvictionScore()', () => {
    it('should return -Infinity for unloaded adapter', () => {
      expect(adapter.calculateEvictionScore()).toBe(-Infinity);
    });

    it('should return -Infinity for high-priority adapter', async () => {
      const highPriorityAdapter = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'high-priority',
        domain: 'test',
        sizeMB: 256,
        priority: 0.95  // >0.9 = never evict
      });

      await highPriorityAdapter.load();

      expect(highPriorityAdapter.calculateEvictionScore()).toBe(-Infinity);
    });

    it('should increase score with age', async () => {
      await adapter.load();

      const score1 = adapter.calculateEvictionScore();

      // Wait a bit to age the adapter
      await new Promise(resolve => setTimeout(resolve, 100));

      const score2 = adapter.calculateEvictionScore();

      expect(score2).toBeGreaterThan(score1);
    });

    it('should decrease score with higher priority', async () => {
      const lowPriority = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'low',
        domain: 'test',
        sizeMB: 256,
        priority: 0.3
      });

      const highPriority = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'high',
        domain: 'test',
        sizeMB: 256,
        priority: 0.7
      });

      await lowPriority.load();
      await highPriority.load();

      // Wait so both have same age
      await new Promise(resolve => setTimeout(resolve, 50));

      const lowScore = lowPriority.calculateEvictionScore();
      const highScore = highPriority.calculateEvictionScore();

      // Lower priority = higher score = more likely to evict
      expect(lowScore).toBeGreaterThan(highScore);
    });

    it('should reset score when markUsed() is called', async () => {
      await adapter.load();

      // Wait to build up age
      await new Promise(resolve => setTimeout(resolve, 100));
      const oldScore = adapter.calculateEvictionScore();

      // Mark as used (resets age)
      adapter.markUsed();
      const newScore = adapter.calculateEvictionScore();

      expect(newScore).toBeLessThan(oldScore);
    });
  });

  describe('getState()', () => {
    it('should return complete adapter state', async () => {
      await adapter.load();
      adapter.markUsed();

      const state = adapter.getState();

      expect(state.name).toBe('test-adapter');
      expect(state.domain).toBe('test');
      expect(state.sizeMB).toBe(256);
      expect(state.priority).toBe(0.5);
      expect(state.loaded).toBe(true);
      expect(state.loadedAt).toBeGreaterThan(0);
      expect(state.lastUsedAt).toBeGreaterThan(0);
      expect(state.usageCount).toBe(2);  // load() + markUsed()
    });
  });

  describe('Integration: Simulating LRU eviction', () => {
    it('should correctly identify least-recently-used adapter', async () => {
      // Create 3 adapters with different priorities
      const adapter1 = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 256,
        priority: 0.5
      });

      const adapter2 = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'adapter-2',
        domain: 'test',
        sizeMB: 256,
        priority: 0.5
      });

      const adapter3 = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'adapter-3',
        domain: 'test',
        sizeMB: 256,
        priority: 0.8  // Higher priority
      });

      // Load all adapters
      await adapter1.load();
      await new Promise(resolve => setTimeout(resolve, 50));

      await adapter2.load();
      await new Promise(resolve => setTimeout(resolve, 50));

      await adapter3.load();

      // Wait a bit more to let adapter1 age
      await new Promise(resolve => setTimeout(resolve, 50));

      // Use adapter2 and adapter3 (adapter1 is now LRU)
      adapter2.markUsed();
      adapter3.markUsed();

      // Calculate scores
      const score1 = adapter1.calculateEvictionScore();
      const score2 = adapter2.calculateEvictionScore();
      const score3 = adapter3.calculateEvictionScore();

      // adapter1 should have highest score (oldest, least recently used)
      expect(score1).toBeGreaterThan(score2);
      expect(score1).toBeGreaterThan(score3);

      // adapter2 and adapter3 were both just used, so their scores are both ~0
      // adapter3 has higher priority (0.8 vs 0.5), but both ages are effectively 0
      // so both scores end up ~0 (can't differentiate when freshly used)
      expect(score2).toBeLessThanOrEqual(0.01);  // Essentially 0
      expect(score3).toBeLessThanOrEqual(0.01);  // Essentially 0
    });
  });
});
