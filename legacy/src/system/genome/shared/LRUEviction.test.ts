/**
 * LRU Eviction Algorithm Tests
 *
 * Tests for weighted LRU eviction with thrashing protection.
 * Verifies:
 * - Correct eviction candidate selection
 * - Memory targeting (evict until target freed)
 * - Priority weighting (don't evict high-priority)
 * - Thrashing protection (hysteresis)
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
  calculateEvictionCandidates,
  selectAdaptersToEvict,
  wouldCauseThrashing,
  filterThrashingAdapters,
  selectAdaptersWithThrashingProtection
} from './LRUEviction';
import { MockLoRAAdapter, type MockLoRAConfig } from './MockLoRAAdapter';
import { randomUUID } from 'crypto';
import type { UUID } from '../../core/types/CrossPlatformUUID';

describe('LRUEviction', () => {
  let adapter1: MockLoRAAdapter;
  let adapter2: MockLoRAAdapter;
  let adapter3: MockLoRAAdapter;
  let highPriority: MockLoRAAdapter;

  beforeEach(async () => {
    // Create 3 normal adapters with different sizes
    adapter1 = new MockLoRAAdapter({
      id: randomUUID() as UUID,
      name: 'adapter-1',
      domain: 'test',
      sizeMB: 512,
      priority: 0.5
    });

    adapter2 = new MockLoRAAdapter({
      id: randomUUID() as UUID,
      name: 'adapter-2',
      domain: 'test',
      sizeMB: 256,
      priority: 0.5
    });

    adapter3 = new MockLoRAAdapter({
      id: randomUUID() as UUID,
      name: 'adapter-3',
      domain: 'test',
      sizeMB: 384,
      priority: 0.6
    });

    // Create high-priority adapter (should never evict)
    highPriority = new MockLoRAAdapter({
      id: randomUUID() as UUID,
      name: 'high-priority',
      domain: 'test',
      sizeMB: 256,
      priority: 0.95  // >0.9 = never evict
    });

    // Load all adapters with delays to create age differences
    await adapter1.load();
    await new Promise(resolve => setTimeout(resolve, 50));

    await adapter2.load();
    await new Promise(resolve => setTimeout(resolve, 50));

    await adapter3.load();
    await new Promise(resolve => setTimeout(resolve, 50));

    await highPriority.load();
  });

  describe('calculateEvictionCandidates()', () => {
    it('should return empty array for empty input', () => {
      const candidates = calculateEvictionCandidates([]);
      expect(candidates).toEqual([]);
    });

    it('should calculate scores for all loaded adapters', () => {
      const candidates = calculateEvictionCandidates([adapter1, adapter2, adapter3]);

      expect(candidates).toHaveLength(3);
      expect(candidates.every(c => c.score >= 0)).toBe(true);
    });

    it('should sort by score (highest first)', () => {
      const candidates = calculateEvictionCandidates([adapter1, adapter2, adapter3]);

      // Scores should be in descending order
      for (let i = 0; i < candidates.length - 1; i++) {
        expect(candidates[i].score).toBeGreaterThanOrEqual(candidates[i + 1].score);
      }
    });

    it('should filter out non-evictable adapters (score = -Infinity)', () => {
      const candidates = calculateEvictionCandidates([adapter1, highPriority]);

      // High-priority adapter should be filtered out
      expect(candidates).toHaveLength(1);
      expect(candidates[0].adapter).toBe(adapter1);
    });

    it('should identify oldest adapter as highest score', () => {
      const candidates = calculateEvictionCandidates([adapter1, adapter2, adapter3]);

      // adapter1 is oldest (loaded first)
      expect(candidates[0].adapter).toBe(adapter1);
    });
  });

  describe('selectAdaptersToEvict()', () => {
    it('should return empty array if target is 0', () => {
      const toEvict = selectAdaptersToEvict([adapter1, adapter2], 0);
      expect(toEvict).toEqual([]);
    });

    it('should return empty array if target is negative', () => {
      const toEvict = selectAdaptersToEvict([adapter1, adapter2], -100);
      expect(toEvict).toEqual([]);
    });

    it('should select single adapter if it meets target', () => {
      // adapter1 is 512MB, target 400MB
      const toEvict = selectAdaptersToEvict([adapter1, adapter2, adapter3], 400);

      expect(toEvict).toHaveLength(1);
      expect(toEvict[0]).toBe(adapter1);  // Oldest
    });

    it('should select multiple adapters to reach target', () => {
      // Need to free 700MB: adapter1 (512) + adapter2 (256) = 768MB
      const toEvict = selectAdaptersToEvict([adapter1, adapter2, adapter3], 700);

      expect(toEvict).toHaveLength(2);
      expect(toEvict[0]).toBe(adapter1);  // Oldest
      expect(toEvict[1]).toBe(adapter2);  // Second oldest
    });

    it('should select all adapters if target exceeds total memory', () => {
      // Total: 512 + 256 + 384 = 1152MB, target 2000MB
      const toEvict = selectAdaptersToEvict([adapter1, adapter2, adapter3], 2000);

      expect(toEvict).toHaveLength(3);
    });

    it('should respect priority (never evict high-priority)', () => {
      // Even with high target, high-priority adapter not selected
      const toEvict = selectAdaptersToEvict([adapter1, highPriority], 1000);

      expect(toEvict).toHaveLength(1);
      expect(toEvict[0]).toBe(adapter1);
      expect(toEvict).not.toContain(highPriority);
    });
  });

  describe('wouldCauseThrashing()', () => {
    it('should return false for unloaded adapter', () => {
      const unloaded = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'unloaded',
        domain: 'test',
        sizeMB: 128
      });

      expect(wouldCauseThrashing(unloaded)).toBe(false);
    });

    it('should return true for recently loaded adapter', async () => {
      const recent = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'recent',
        domain: 'test',
        sizeMB: 128
      });

      await recent.load();

      // Just loaded, within hysteresis window (default 60s)
      expect(wouldCauseThrashing(recent, 60)).toBe(true);
    });

    it('should return false for old adapter', () => {
      // adapter1 was loaded >150ms ago (3x50ms delays)
      expect(wouldCauseThrashing(adapter1, 0.1)).toBe(false);
    });

    it('should respect custom hysteresis window', async () => {
      const recent = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'recent',
        domain: 'test',
        sizeMB: 128
      });

      await recent.load();

      // With 0.01s hysteresis (10ms), should be safe after delays
      await new Promise(resolve => setTimeout(resolve, 20));
      expect(wouldCauseThrashing(recent, 0.01)).toBe(false);
    });
  });

  describe('filterThrashingAdapters()', () => {
    it('should return empty array for empty input', () => {
      const filtered = filterThrashingAdapters([]);
      expect(filtered).toEqual([]);
    });

    it('should filter out recently loaded adapters', async () => {
      const recent = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'recent',
        domain: 'test',
        sizeMB: 128
      });

      await recent.load();

      // Use short hysteresis (0.1s) so adapter1 (loaded 150+ms ago) is safe
      const filtered = filterThrashingAdapters([adapter1, recent], 0.1);

      // adapter1 is old (safe), recent is new (filtered)
      expect(filtered).toHaveLength(1);
      expect(filtered[0]).toBe(adapter1);
    });

    it('should keep all adapters if none are recent', () => {
      // All adapters loaded >150ms ago
      const filtered = filterThrashingAdapters([adapter1, adapter2, adapter3], 0.1);

      expect(filtered).toHaveLength(3);
    });
  });

  describe('selectAdaptersWithThrashingProtection()', () => {
    it('should select adapters without thrashing if possible', () => {
      // adapter1 is old enough (loaded first)
      const result = selectAdaptersWithThrashingProtection(
        [adapter1, adapter2, adapter3],
        400,
        0.1  // 100ms hysteresis
      );

      expect(result.toEvict).toHaveLength(1);
      expect(result.toEvict[0]).toBe(adapter1);
      expect(result.freedMB).toBe(512);
      expect(result.wouldThrash).toBe(false);
    });

    it('should skip recent adapters to avoid thrashing', async () => {
      const recent = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'recent',
        domain: 'test',
        sizeMB: 512
      });

      await recent.load();

      // Recent adapter has high score but is within hysteresis window
      const result = selectAdaptersWithThrashingProtection(
        [recent, adapter1],
        400,
        0.1  // 100ms hysteresis - adapter1 is safe, recent is not
      );

      // Should select adapter1 (old) instead of recent
      expect(result.toEvict).toContain(adapter1);
      expect(result.toEvict).not.toContain(recent);
    });

    it('should indicate thrashing if target cannot be met', async () => {
      // All adapters recently loaded
      const recent1 = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'recent1',
        domain: 'test',
        sizeMB: 256
      });

      const recent2 = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'recent2',
        domain: 'test',
        sizeMB: 256
      });

      await recent1.load();
      await recent2.load();

      const result = selectAdaptersWithThrashingProtection(
        [recent1, recent2],
        400,
        60  // All recent, can't evict without thrashing
      );

      expect(result.toEvict).toHaveLength(0);
      expect(result.freedMB).toBe(0);
      expect(result.wouldThrash).toBe(true);
    });

    it('should return partial list if some memory can be freed safely', async () => {
      const recent = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'recent',
        domain: 'test',
        sizeMB: 512
      });

      await recent.load();

      // Need 700MB: adapter1 (512) is safe, recent (512) would thrash
      const result = selectAdaptersWithThrashingProtection(
        [adapter1, recent],
        700,
        0.1  // 100ms hysteresis - adapter1 safe, recent not
      );

      expect(result.toEvict).toHaveLength(1);
      expect(result.toEvict[0]).toBe(adapter1);
      expect(result.freedMB).toBe(512);
      expect(result.wouldThrash).toBe(true);  // Couldn't reach target
    });
  });

  describe('Integration: Complex eviction scenario', () => {
    it('should handle multi-persona eviction with priorities and hysteresis', async () => {
      // Scenario: 3 personas, 2GB GPU memory, need to load 600MB adapter

      // Persona 1: Old, low priority (should evict first)
      const persona1Adapter = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'persona1-adapter',
        domain: 'knowledge',
        sizeMB: 512,
        priority: 0.4
      });

      // Persona 2: Recent, medium priority (protected by hysteresis)
      const persona2Adapter = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'persona2-adapter',
        domain: 'personality',
        sizeMB: 384,
        priority: 0.6
      });

      // Persona 3: Old, high priority (should never evict)
      const persona3Adapter = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: 'persona3-adapter',
        domain: 'code',
        sizeMB: 256,
        priority: 0.95
      });

      // Load in sequence
      await persona1Adapter.load();
      await new Promise(resolve => setTimeout(resolve, 100));

      await persona3Adapter.load();
      await new Promise(resolve => setTimeout(resolve, 100));

      await persona2Adapter.load();  // Most recent

      // Try to free 600MB with thrashing protection
      const result = selectAdaptersWithThrashingProtection(
        [persona1Adapter, persona2Adapter, persona3Adapter],
        600,
        0.15  // 150ms hysteresis
      );

      // Should select persona1 (old, low priority)
      // Should NOT select persona2 (recent, protected by hysteresis)
      // Should NOT select persona3 (high priority, never evict)
      expect(result.toEvict).toHaveLength(1);
      expect(result.toEvict[0]).toBe(persona1Adapter);
      expect(result.freedMB).toBe(512);
      expect(result.wouldThrash).toBe(true);  // Can't reach 600MB without thrashing
    });
  });
});
