/**
 * PersonaGenomeState Tests
 *
 * Tests for per-persona genome memory tracking.
 * Verifies:
 * - Memory quota enforcement
 * - Adapter load/unload tracking
 * - Statistics collection
 * - Eviction score calculation
 * - Serialization/deserialization
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { PersonaGenomeState, type PersonaGenomeConfig, type GenomeStats } from './PersonaGenomeState';
import { randomUUID } from 'crypto';
import type { UUID } from '../../../core/types/CrossPlatformUUID';

describe('PersonaGenomeState', () => {
  let config: PersonaGenomeConfig;
  let state: PersonaGenomeState;

  beforeEach(() => {
    config = {
      personaId: randomUUID() as UUID,
      displayName: 'Test Persona',
      memoryQuotaMB: 1024,
      priority: 0.7
    };

    state = new PersonaGenomeState(config);
  });

  describe('Construction', () => {
    it('should create with valid config', () => {
      expect(state.getPersonaId()).toBe(config.personaId);
      expect(state.getDisplayName()).toBe(config.displayName);
      expect(state.getMemoryQuota()).toBe(1024);
      expect(state.getPriority()).toBe(0.7);
    });

    it('should default priority to 0.5', () => {
      const defaultState = new PersonaGenomeState({
        personaId: randomUUID() as UUID,
        displayName: 'Default',
        memoryQuotaMB: 512
      });

      expect(defaultState.getPriority()).toBe(0.5);
    });

    it('should reject negative priority', () => {
      expect(() => {
        new PersonaGenomeState({
          personaId: randomUUID() as UUID,
          displayName: 'Invalid',
          memoryQuotaMB: 512,
          priority: -0.1
        });
      }).toThrow('Priority must be between 0 and 1');
    });

    it('should reject priority > 1', () => {
      expect(() => {
        new PersonaGenomeState({
          personaId: randomUUID() as UUID,
          displayName: 'Invalid',
          memoryQuotaMB: 512,
          priority: 1.5
        });
      }).toThrow('Priority must be between 0 and 1');
    });

    it('should reject zero memory quota', () => {
      expect(() => {
        new PersonaGenomeState({
          personaId: randomUUID() as UUID,
          displayName: 'Invalid',
          memoryQuotaMB: 0
        });
      }).toThrow('Memory quota must be positive');
    });

    it('should reject negative memory quota', () => {
      expect(() => {
        new PersonaGenomeState({
          personaId: randomUUID() as UUID,
          displayName: 'Invalid',
          memoryQuotaMB: -512
        });
      }).toThrow('Memory quota must be positive');
    });
  });

  describe('Memory Tracking', () => {
    it('should start with zero memory used', () => {
      expect(state.getMemoryUsed()).toBe(0);
      expect(state.getMemoryAvailable()).toBe(1024);
      expect(state.getMemoryUtilization()).toBe(0);
    });

    it('should track memory usage after load', () => {
      const adapterId = randomUUID() as UUID;
      state.recordLoad(adapterId, 512, 100);

      expect(state.getMemoryUsed()).toBe(512);
      expect(state.getMemoryAvailable()).toBe(512);
      expect(state.getMemoryUtilization()).toBe(0.5);
    });

    it('should accumulate memory from multiple loads', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 256, 50);
      state.recordLoad(adapter2, 384, 75);

      expect(state.getMemoryUsed()).toBe(640);
      expect(state.getMemoryAvailable()).toBe(384);
      expect(state.getMemoryUtilization()).toBeCloseTo(0.625);
    });

    it('should reduce memory after unload', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 50);

      state.recordUnload(adapter1, 512);

      expect(state.getMemoryUsed()).toBe(256);
      expect(state.getMemoryAvailable()).toBe(768);
    });

    it('should handle unloading all adapters', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 50);

      state.recordUnload(adapter1, 512);
      state.recordUnload(adapter2, 256);

      expect(state.getMemoryUsed()).toBe(0);
      expect(state.getMemoryAvailable()).toBe(1024);
    });

    it('should detect over-quota condition', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 768, 100);
      expect(state.isOverQuota()).toBe(false);

      state.recordLoad(adapter2, 512, 100);  // Total 1280MB > 1024MB quota
      expect(state.isOverQuota()).toBe(true);
    });

    it('should check if adapter can be loaded', () => {
      const adapter1 = randomUUID() as UUID;

      expect(state.canLoadAdapter(512)).toBe(true);
      expect(state.canLoadAdapter(1024)).toBe(true);
      expect(state.canLoadAdapter(1500)).toBe(false);

      state.recordLoad(adapter1, 768, 100);

      expect(state.canLoadAdapter(256)).toBe(true);
      expect(state.canLoadAdapter(512)).toBe(false);
    });
  });

  describe('Adapter Management', () => {
    it('should track active adapters', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      expect(state.getActiveAdapterCount()).toBe(0);
      expect(state.hasActiveAdapter(adapter1)).toBe(false);

      state.recordLoad(adapter1, 512, 100);

      expect(state.getActiveAdapterCount()).toBe(1);
      expect(state.hasActiveAdapter(adapter1)).toBe(true);
      expect(state.hasActiveAdapter(adapter2)).toBe(false);

      state.recordLoad(adapter2, 256, 50);

      expect(state.getActiveAdapterCount()).toBe(2);
      expect(state.hasActiveAdapter(adapter2)).toBe(true);
    });

    it('should list active adapters', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 50);

      const active = state.getActiveAdapters();

      expect(active).toHaveLength(2);
      expect(active).toContain(adapter1);
      expect(active).toContain(adapter2);
    });

    it('should remove adapter from active list on unload', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 50);

      state.recordUnload(adapter1, 512);

      expect(state.getActiveAdapterCount()).toBe(1);
      expect(state.hasActiveAdapter(adapter1)).toBe(false);
      expect(state.hasActiveAdapter(adapter2)).toBe(true);
    });

    it('should reject duplicate load', () => {
      const adapterId = randomUUID() as UUID;

      state.recordLoad(adapterId, 512, 100);

      expect(() => {
        state.recordLoad(adapterId, 256, 50);
      }).toThrow(/Adapter .* is already loaded/);
    });

    it('should reject unload of non-loaded adapter', () => {
      const adapterId = randomUUID() as UUID;

      expect(() => {
        state.recordUnload(adapterId, 512);
      }).toThrow(/Adapter .* is not loaded/);
    });
  });

  describe('Statistics', () => {
    it('should track load count', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      expect(state.getLoadCount()).toBe(0);

      state.recordLoad(adapter1, 512, 100);
      expect(state.getLoadCount()).toBe(1);

      state.recordLoad(adapter2, 256, 50);
      expect(state.getLoadCount()).toBe(2);
    });

    it('should track last activated timestamp', () => {
      const adapter1 = randomUUID() as UUID;

      expect(state.getLastActivatedAt()).toBeNull();

      const before = Date.now();
      state.recordLoad(adapter1, 512, 100);
      const after = Date.now();

      const lastActivated = state.getLastActivatedAt();
      expect(lastActivated).not.toBeNull();
      expect(lastActivated!).toBeGreaterThanOrEqual(before);
      expect(lastActivated!).toBeLessThanOrEqual(after);
    });

    it('should track total load time', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 75);

      expect(state.getTotalLoadTime()).toBe(175);
    });

    it('should calculate average load time', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;
      const adapter3 = randomUUID() as UUID;

      expect(state.getAverageLoadTime()).toBe(0);

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 50);
      state.recordLoad(adapter3, 128, 25);

      expect(state.getAverageLoadTime()).toBeCloseTo(58.33, 1);
    });

    it('should track eviction count', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      expect(state.getEvictionCount()).toBe(0);

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 50);

      state.recordUnload(adapter1, 512);
      expect(state.getEvictionCount()).toBe(1);

      state.recordUnload(adapter2, 256);
      expect(state.getEvictionCount()).toBe(2);
    });

    it('should return stats snapshot', () => {
      const adapter1 = randomUUID() as UUID;

      state.recordLoad(adapter1, 512, 100);
      state.recordUnload(adapter1, 512);

      const stats = state.getStats();

      expect(stats.loadCount).toBe(1);
      expect(stats.totalLoadTimeMs).toBe(100);
      expect(stats.totalEvictionCount).toBe(1);
      expect(stats.lastActivatedAt).not.toBeNull();
    });
  });

  describe('Priority Management', () => {
    it('should allow updating priority', () => {
      expect(state.getPriority()).toBe(0.7);

      state.setPriority(0.9);
      expect(state.getPriority()).toBe(0.9);
    });

    it('should reject invalid priority update', () => {
      expect(() => {
        state.setPriority(-0.1);
      }).toThrow('Priority must be between 0 and 1');

      expect(() => {
        state.setPriority(1.5);
      }).toThrow('Priority must be between 0 and 1');
    });
  });

  describe('Eviction Score', () => {
    it('should return -Infinity for high-priority personas', () => {
      const highPriority = new PersonaGenomeState({
        personaId: randomUUID() as UUID,
        displayName: 'High Priority',
        memoryQuotaMB: 512,
        priority: 0.95
      });

      expect(highPriority.calculateEvictionScore()).toBe(-Infinity);
    });

    it('should return 0 for just-activated persona', () => {
      const adapter = randomUUID() as UUID;
      state.recordLoad(adapter, 512, 100);

      // Just loaded, age ≈ 0
      const score = state.calculateEvictionScore();
      expect(score).toBeGreaterThanOrEqual(0);
      expect(score).toBeLessThan(0.1);
    });

    it('should increase score with age', async () => {
      const adapter = randomUUID() as UUID;
      state.recordLoad(adapter, 512, 100);

      const scoreInitial = state.calculateEvictionScore();

      // Wait 50ms
      await new Promise(resolve => setTimeout(resolve, 50));

      const scoreAfterDelay = state.calculateEvictionScore();

      expect(scoreAfterDelay).toBeGreaterThan(scoreInitial);
    });

    it('should weight score by priority', async () => {
      const lowPriority = new PersonaGenomeState({
        personaId: randomUUID() as UUID,
        displayName: 'Low Priority',
        memoryQuotaMB: 512,
        priority: 0.3
      });

      const highPriority = new PersonaGenomeState({
        personaId: randomUUID() as UUID,
        displayName: 'High Priority',
        memoryQuotaMB: 512,
        priority: 0.8
      });

      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      lowPriority.recordLoad(adapter1, 256, 50);
      highPriority.recordLoad(adapter2, 256, 50);

      // Wait for age to accumulate
      await new Promise(resolve => setTimeout(resolve, 10));

      // Low priority = higher eviction score
      expect(lowPriority.calculateEvictionScore()).toBeGreaterThan(
        highPriority.calculateEvictionScore()
      );
    });

    it('should increase score when over quota', async () => {
      // Create two personas with same priority, same age, but different quota status
      const underQuota = new PersonaGenomeState({
        personaId: randomUUID() as UUID,
        displayName: 'Under Quota',
        memoryQuotaMB: 2048,
        priority: 0.5
      });

      const overQuota = new PersonaGenomeState({
        personaId: randomUUID() as UUID,
        displayName: 'Over Quota',
        memoryQuotaMB: 512,
        priority: 0.5
      });

      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      // Load same adapter (768MB total) into both
      underQuota.recordLoad(adapter1, 768, 100);
      overQuota.recordLoad(adapter2, 768, 100);

      // Wait for age to accumulate
      await new Promise(resolve => setTimeout(resolve, 10));

      const scoreUnder = underQuota.calculateEvictionScore();
      const scoreOver = overQuota.calculateEvictionScore();

      // Over-quota persona should have 2x the score (pressure multiplier)
      expect(scoreOver).toBeGreaterThan(scoreUnder * 1.5);
    });

    it('should never return negative score (except -Infinity)', async () => {
      const adapter = randomUUID() as UUID;
      state.recordLoad(adapter, 512, 100);

      await new Promise(resolve => setTimeout(resolve, 50));

      const score = state.calculateEvictionScore();
      expect(score).toBeGreaterThanOrEqual(0);
    });
  });

  describe('Serialization', () => {
    it('should serialize to JSON', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 50);

      const json = state.toJSON();

      expect(json.personaId).toBe(config.personaId);
      expect(json.displayName).toBe('Test Persona');
      expect(json.memoryUsedMB).toBe(768);
      expect(json.memoryQuotaMB).toBe(1024);
      expect(json.priority).toBe(0.7);
      expect(json.activeAdapters).toEqual(
        expect.arrayContaining([adapter1, adapter2])
      );
      expect((json.stats as GenomeStats).loadCount).toBe(2);
    });

    it('should deserialize from JSON', () => {
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      state.recordLoad(adapter1, 512, 100);
      state.recordLoad(adapter2, 256, 50);
      state.recordUnload(adapter1, 512);

      const json = state.toJSON();
      const restored = PersonaGenomeState.fromJSON(json);

      expect(restored.getPersonaId()).toBe(state.getPersonaId());
      expect(restored.getDisplayName()).toBe(state.getDisplayName());
      expect(restored.getMemoryUsed()).toBe(state.getMemoryUsed());
      expect(restored.getMemoryQuota()).toBe(state.getMemoryQuota());
      expect(restored.getPriority()).toBe(state.getPriority());
      expect(restored.getActiveAdapterCount()).toBe(1);
      expect(restored.hasActiveAdapter(adapter2)).toBe(true);
      expect(restored.getLoadCount()).toBe(2);
      expect(restored.getEvictionCount()).toBe(1);
    });

    it('should round-trip serialize/deserialize', () => {
      const adapter = randomUUID() as UUID;
      state.recordLoad(adapter, 512, 100);

      const json = state.toJSON();
      const restored = PersonaGenomeState.fromJSON(json);
      const json2 = restored.toJSON();

      expect(json).toEqual(json2);
    });
  });

  describe('Integration: Memory pressure scenario', () => {
    it('should handle realistic multi-adapter scenario', () => {
      // Scenario: Persona with 1GB quota loads 3 adapters
      const wineExpertise = randomUUID() as UUID;
      const actionHeroStyle = randomUUID() as UUID;
      const generalKnowledge = randomUUID() as UUID;

      // Load Layer 1: Wine expertise (512MB)
      state.recordLoad(wineExpertise, 512, 150);
      expect(state.canLoadAdapter(256)).toBe(true);
      expect(state.isOverQuota()).toBe(false);

      // Load Layer 2: Action hero style (256MB)
      state.recordLoad(actionHeroStyle, 256, 75);
      expect(state.getMemoryUsed()).toBe(768);
      expect(state.canLoadAdapter(256)).toBe(true);

      // Load Layer 3: General knowledge (384MB) - EXCEEDS QUOTA
      state.recordLoad(generalKnowledge, 384, 120);
      expect(state.getMemoryUsed()).toBe(1152);
      expect(state.isOverQuota()).toBe(true);

      // Evict oldest layer (wine expertise)
      state.recordUnload(wineExpertise, 512);
      expect(state.getMemoryUsed()).toBe(640);
      expect(state.isOverQuota()).toBe(false);

      // Stats
      expect(state.getLoadCount()).toBe(3);
      expect(state.getEvictionCount()).toBe(1);
      expect(state.getActiveAdapterCount()).toBe(2);
      expect(state.getAverageLoadTime()).toBeCloseTo(115, 0);
    });
  });
});
