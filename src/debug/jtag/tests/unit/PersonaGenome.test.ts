/**
 * PersonaGenome Unit Tests
 *
 * Tests the LoRA adapter paging system (slingshot approach)
 * Validates virtual memory-style management with LRU eviction
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { PersonaGenome, type LoadedAdapter, type GenomeStats } from '../../system/user/server/modules/PersonaGenome';
import { GenomeEntity, type GenomeLayerReference } from '../../system/genome/entities/GenomeEntity';
import { GenomeLayerEntity } from '../../system/genome/entities/GenomeLayerEntity';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('PersonaGenome', () => {
  let genome: PersonaGenome;
  const personaId: UUID = 'test-persona-id' as UUID;
  const personaName = 'TestPersona';

  beforeEach(() => {
    genome = new PersonaGenome(personaId, personaName, {
      maxMemoryMB: 200,  // Small budget for testing eviction
      enableLogging: false
    });
  });

  describe('Configuration', () => {
    it('should initialize with default config', () => {
      const defaultGenome = new PersonaGenome(personaId, personaName);
      const stats = defaultGenome.getStats();

      expect(stats.memoryBudgetMB).toBe(512);  // Default budget
      expect(stats.totalAdaptersLoaded).toBe(0);
      expect(stats.currentMemoryUsageMB).toBe(0);
    });

    it('should initialize with custom config', () => {
      const customGenome = new PersonaGenome(personaId, personaName, {
        maxMemoryMB: 256,
        enableLogging: false
      });

      const stats = customGenome.getStats();
      expect(stats.memoryBudgetMB).toBe(256);
    });
  });

  describe('Genome Loading', () => {
    it('should accept genome entity with layer stack', () => {
      const genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        {
          layerId: 'layer-1' as UUID,
          traitType: 'conversational',
          orderIndex: 0,
          weight: 1.0,
          enabled: true
        },
        {
          layerId: 'layer-2' as UUID,
          traitType: 'typescript-expert',
          orderIndex: 1,
          weight: 1.0,
          enabled: true
        }
      ];

      genome.setGenome(genomeEntity);

      // Should not throw, and can activate skills
      expect(async () => {
        await genome.activateSkill('conversational');
      }).not.toThrow();
    });

    it('should throw if activating skill without genome', async () => {
      await expect(genome.activateSkill('conversational'))
        .rejects
        .toThrow('No genome loaded');
    });
  });

  describe('Adapter Loading (Cache Miss)', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        {
          layerId: 'layer-1' as UUID,
          traitType: 'conversational',
          orderIndex: 0,
          weight: 1.0,
          enabled: true
        },
        {
          layerId: 'layer-2' as UUID,
          traitType: 'typescript-expert',
          orderIndex: 1,
          weight: 1.0,
          enabled: true
        },
        {
          layerId: 'layer-3' as UUID,
          traitType: 'rust-expert',
          orderIndex: 2,
          weight: 1.0,
          enabled: true
        }
      ];

      genome.setGenome(genomeEntity);
    });

    it('should load adapter from disk on first access', async () => {
      const statsBefore = genome.getStats();
      expect(statsBefore.cacheMisses).toBe(0);

      await genome.activateSkill('conversational');

      const statsAfter = genome.getStats();
      expect(statsAfter.cacheMisses).toBe(1);
      expect(statsAfter.cacheHits).toBe(0);
      expect(statsAfter.totalAdaptersLoaded).toBe(1);
      expect(statsAfter.currentMemoryUsageMB).toBeGreaterThan(0);
    });

    it('should set current adapter after activation', async () => {
      await genome.activateSkill('typescript-expert');

      const current = genome.getCurrentAdapter();
      expect(current).not.toBeNull();
      expect(current?.traitType).toBe('typescript-expert');
    });

    it('should track load latency', async () => {
      await genome.activateSkill('conversational');

      const stats = genome.getStats();
      expect(stats.loadLatencyMs).toBeGreaterThan(0);
      expect(stats.loadLatencyMs).toBeLessThan(100);  // Mock latency is 10-50ms
    });

    it('should add adapter to active set', async () => {
      await genome.activateSkill('conversational');

      const activeAdapters = genome.getActiveAdapters();
      expect(activeAdapters).toHaveLength(1);
      expect(activeAdapters[0].traitType).toBe('conversational');
    });
  });

  describe('Adapter Caching (Cache Hit)', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        {
          layerId: 'layer-1' as UUID,
          traitType: 'conversational',
          orderIndex: 0,
          weight: 1.0,
          enabled: true
        }
      ];

      genome.setGenome(genomeEntity);
    });

    it('should use cached adapter on second access', async () => {
      // First access - cache miss
      await genome.activateSkill('conversational');
      const statsAfterMiss = genome.getStats();
      expect(statsAfterMiss.cacheMisses).toBe(1);
      expect(statsAfterMiss.cacheHits).toBe(0);

      // Second access - cache hit
      await genome.activateSkill('conversational');
      const statsAfterHit = genome.getStats();
      expect(statsAfterHit.cacheMisses).toBe(1);  // Still 1
      expect(statsAfterHit.cacheHits).toBe(1);  // Now 1
    });

    it('should update lastUsedAt on cache hit', async () => {
      await genome.activateSkill('conversational');

      const firstAccess = genome.getCurrentAdapter();
      const firstTime = firstAccess?.lastUsedAt || 0;

      // Wait a bit
      await new Promise(resolve => setTimeout(resolve, 10));

      await genome.activateSkill('conversational');
      const secondAccess = genome.getCurrentAdapter();
      const secondTime = secondAccess?.lastUsedAt || 0;

      expect(secondTime).toBeGreaterThan(firstTime);
    });

    it('should not increase memory usage on cache hit', async () => {
      await genome.activateSkill('conversational');
      const memoryAfterLoad = genome.getStats().currentMemoryUsageMB;

      await genome.activateSkill('conversational');
      const memoryAfterHit = genome.getStats().currentMemoryUsageMB;

      expect(memoryAfterHit).toBe(memoryAfterLoad);
    });
  });

  describe('Memory Budget Enforcement', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      // Create genome with 4 layers
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        { layerId: 'layer-1' as UUID, traitType: 'trait-1', orderIndex: 0, weight: 1.0, enabled: true },
        { layerId: 'layer-2' as UUID, traitType: 'trait-2', orderIndex: 1, weight: 1.0, enabled: true },
        { layerId: 'layer-3' as UUID, traitType: 'trait-3', orderIndex: 2, weight: 1.0, enabled: true },
        { layerId: 'layer-4' as UUID, traitType: 'trait-4', orderIndex: 3, weight: 1.0, enabled: true }
      ];

      genome.setGenome(genomeEntity);
    });

    it('should respect memory budget', async () => {
      const stats = genome.getStats();
      const budget = stats.memoryBudgetMB;

      // Load adapters until budget exceeded
      await genome.activateSkill('trait-1');  // 50MB
      await genome.activateSkill('trait-2');  // 50MB (100MB total)
      await genome.activateSkill('trait-3');  // 50MB (150MB total)
      await genome.activateSkill('trait-4');  // 50MB (200MB total, at budget limit)

      const finalStats = genome.getStats();
      expect(finalStats.currentMemoryUsageMB).toBeLessThanOrEqual(budget);
    });

    it('should not exceed memory budget even after many loads', async () => {
      const budget = genome.getStats().memoryBudgetMB;

      // Load all adapters
      await genome.activateSkill('trait-1');
      await genome.activateSkill('trait-2');
      await genome.activateSkill('trait-3');
      await genome.activateSkill('trait-4');

      // Try to load again (should trigger eviction)
      await genome.activateSkill('trait-1');

      const stats = genome.getStats();
      expect(stats.currentMemoryUsageMB).toBeLessThanOrEqual(budget);
    });
  });

  describe('LRU Eviction', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      // Create genome with 5 layers (budget only fits 4 at 50MB each)
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        { layerId: 'layer-1' as UUID, traitType: 'trait-1', orderIndex: 0, weight: 1.0, enabled: true },
        { layerId: 'layer-2' as UUID, traitType: 'trait-2', orderIndex: 1, weight: 1.0, enabled: true },
        { layerId: 'layer-3' as UUID, traitType: 'trait-3', orderIndex: 2, weight: 1.0, enabled: true },
        { layerId: 'layer-4' as UUID, traitType: 'trait-4', orderIndex: 3, weight: 1.0, enabled: true },
        { layerId: 'layer-5' as UUID, traitType: 'trait-5', orderIndex: 4, weight: 1.0, enabled: true }
      ];

      genome.setGenome(genomeEntity);
    });

    it('should evict LRU adapter when memory full', async () => {
      // Load 4 adapters (fill budget: 200MB)
      await genome.activateSkill('trait-1');  // 50MB (accessed first)
      await genome.activateSkill('trait-2');  // 50MB
      await genome.activateSkill('trait-3');  // 50MB
      await genome.activateSkill('trait-4');  // 50MB

      // Load 5th adapter (should evict trait-1 as LRU)
      await genome.activateSkill('trait-5');  // 50MB

      const stats = genome.getStats();
      expect(stats.evictions).toBeGreaterThan(0);
      expect(stats.currentMemoryUsageMB).toBeLessThanOrEqual(200);

      // trait-1 should no longer be loaded
      const activeAdapters = genome.getActiveAdapters();
      const hasTrait1 = activeAdapters.some(a => a.traitType === 'trait-1');
      expect(hasTrait1).toBe(false);
    });

    it('should keep recently used adapters', async () => {
      // Load adapters
      await genome.activateSkill('trait-1');
      await genome.activateSkill('trait-2');
      await genome.activateSkill('trait-3');
      await genome.activateSkill('trait-4');

      // Access trait-1 again (make it recently used)
      await genome.activateSkill('trait-1');

      // Load trait-5 (should evict trait-2, not trait-1)
      await genome.activateSkill('trait-5');

      const activeAdapters = genome.getActiveAdapters();
      const hasTrait1 = activeAdapters.some(a => a.traitType === 'trait-1');
      const hasTrait2 = activeAdapters.some(a => a.traitType === 'trait-2');

      expect(hasTrait1).toBe(true);  // Recently used, kept
      expect(hasTrait2).toBe(false);  // LRU, evicted
    });

    it('should track eviction count', async () => {
      // Fill budget
      await genome.activateSkill('trait-1');
      await genome.activateSkill('trait-2');
      await genome.activateSkill('trait-3');
      await genome.activateSkill('trait-4');

      const statsBefore = genome.getStats();
      expect(statsBefore.evictions).toBe(0);

      // Trigger eviction
      await genome.activateSkill('trait-5');

      const statsAfter = genome.getStats();
      expect(statsAfter.evictions).toBeGreaterThan(0);
    });
  });

  describe('Trait Priority (Layer Stack Order)', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      // Create genome with duplicate traits (higher orderIndex wins)
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        { layerId: 'layer-1' as UUID, traitType: 'conversational', orderIndex: 0, weight: 0.5, enabled: true },
        { layerId: 'layer-2' as UUID, traitType: 'conversational', orderIndex: 1, weight: 1.0, enabled: true }  // Higher priority
      ];

      genome.setGenome(genomeEntity);
    });

    it('should use highest priority layer for trait', async () => {
      await genome.activateSkill('conversational');

      const current = genome.getCurrentAdapter();
      expect(current?.layerId).toBe('layer-2');  // Higher orderIndex
    });
  });

  describe('Disabled Layers', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        { layerId: 'layer-1' as UUID, traitType: 'conversational', orderIndex: 0, weight: 1.0, enabled: false }  // Disabled
      ];

      genome.setGenome(genomeEntity);
    });

    it('should not load disabled layers', async () => {
      await expect(genome.activateSkill('conversational'))
        .rejects
        .toThrow('No layer found for trait');
    });
  });

  describe('Statistics Tracking', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        { layerId: 'layer-1' as UUID, traitType: 'trait-1', orderIndex: 0, weight: 1.0, enabled: true },
        { layerId: 'layer-2' as UUID, traitType: 'trait-2', orderIndex: 1, weight: 1.0, enabled: true }
      ];

      genome.setGenome(genomeEntity);
    });

    it('should track total adapters loaded', async () => {
      await genome.activateSkill('trait-1');
      await genome.activateSkill('trait-2');

      const stats = genome.getStats();
      expect(stats.totalAdaptersLoaded).toBe(2);
    });

    it('should track cache hit ratio', async () => {
      // Load and access multiple times
      await genome.activateSkill('trait-1');  // Miss
      await genome.activateSkill('trait-1');  // Hit
      await genome.activateSkill('trait-1');  // Hit

      const stats = genome.getStats();
      expect(stats.cacheMisses).toBe(1);
      expect(stats.cacheHits).toBe(2);
    });

    it('should track current memory usage', async () => {
      await genome.activateSkill('trait-1');
      const statsAfterOne = genome.getStats();
      expect(statsAfterOne.currentMemoryUsageMB).toBeGreaterThan(0);

      await genome.activateSkill('trait-2');
      const statsAfterTwo = genome.getStats();
      expect(statsAfterTwo.currentMemoryUsageMB).toBeGreaterThan(statsAfterOne.currentMemoryUsageMB);
    });

    it('should track average load latency', async () => {
      await genome.activateSkill('trait-1');
      await genome.activateSkill('trait-2');

      const stats = genome.getStats();
      expect(stats.loadLatencyMs).toBeGreaterThan(0);
    });
  });

  describe('Unload All', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Test Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        { layerId: 'layer-1' as UUID, traitType: 'trait-1', orderIndex: 0, weight: 1.0, enabled: true }
      ];

      genome.setGenome(genomeEntity);
    });

    it('should clear all active adapters', async () => {
      await genome.activateSkill('trait-1');
      expect(genome.getActiveAdapters()).toHaveLength(1);

      await genome.unloadAll();
      expect(genome.getActiveAdapters()).toHaveLength(0);
    });

    it('should reset memory usage', async () => {
      await genome.activateSkill('trait-1');
      expect(genome.getStats().currentMemoryUsageMB).toBeGreaterThan(0);

      await genome.unloadAll();
      expect(genome.getStats().currentMemoryUsageMB).toBe(0);
    });

    it('should clear current adapter', async () => {
      await genome.activateSkill('trait-1');
      expect(genome.getCurrentAdapter()).not.toBeNull();

      await genome.unloadAll();
      expect(genome.getCurrentAdapter()).toBeNull();
    });
  });

  describe('Integration Scenarios', () => {
    let genomeEntity: GenomeEntity;

    beforeEach(() => {
      genomeEntity = new GenomeEntity();
      genomeEntity.id = 'genome-1' as UUID;
      genomeEntity.name = 'Multi-Domain Genome';
      genomeEntity.personaId = personaId;
      genomeEntity.baseModel = 'llama-3.1-8B';
      genomeEntity.layers = [
        { layerId: 'layer-1' as UUID, traitType: 'conversational', orderIndex: 0, weight: 1.0, enabled: true },
        { layerId: 'layer-2' as UUID, traitType: 'typescript-expert', orderIndex: 1, weight: 1.0, enabled: true },
        { layerId: 'layer-3' as UUID, traitType: 'rust-expert', orderIndex: 2, weight: 1.0, enabled: true },
        { layerId: 'layer-4' as UUID, traitType: 'chess-player', orderIndex: 3, weight: 1.0, enabled: true }
      ];

      genome.setGenome(genomeEntity);
    });

    it('should handle task-based adapter switching', async () => {
      // Chat domain - load conversational
      await genome.activateSkill('conversational');
      expect(genome.getCurrentAdapter()?.traitType).toBe('conversational');

      // Code domain - load typescript-expert
      await genome.activateSkill('typescript-expert');
      expect(genome.getCurrentAdapter()?.traitType).toBe('typescript-expert');

      // Game domain - load chess-player
      await genome.activateSkill('chess-player');
      expect(genome.getCurrentAdapter()?.traitType).toBe('chess-player');
    });

    it('should maintain working set via LRU', async () => {
      // Simulate alternating between two tasks
      await genome.activateSkill('conversational');
      await genome.activateSkill('typescript-expert');
      await genome.activateSkill('conversational');  // Hit
      await genome.activateSkill('typescript-expert');  // Hit

      const stats = genome.getStats();
      expect(stats.cacheHits).toBe(2);  // Both stayed in cache
    });

    it('should handle memory pressure gracefully', async () => {
      // Load all adapters (budget: 200MB, 4 layers × 50MB each = exactly 200MB)
      await genome.activateSkill('conversational');
      await genome.activateSkill('typescript-expert');
      await genome.activateSkill('rust-expert');
      await genome.activateSkill('chess-player');

      // Now try to load the first one again (cache hit, no eviction)
      await genome.activateSkill('conversational');

      // Memory should be at budget limit, no evictions yet (exact fit)
      let stats = genome.getStats();
      expect(stats.currentMemoryUsageMB).toBeLessThanOrEqual(200);

      // Now load a NEW adapter (5th adapter, should trigger eviction)
      const genomeEntity = genome['genome'] as GenomeEntity;  // Access private for test
      genomeEntity.layers.push({
        layerId: 'layer-5' as UUID,
        traitType: 'python-expert',
        orderIndex: 4,
        weight: 1.0,
        enabled: true
      });

      await genome.activateSkill('python-expert');

      // Now evictions should have occurred
      stats = genome.getStats();
      expect(stats.currentMemoryUsageMB).toBeLessThanOrEqual(200);
      expect(stats.evictions).toBeGreaterThan(0);  // Eviction occurred for 5th adapter
    });
  });
});
