/**
 * Genome Commands Integration Tests
 *
 * Tests genome commands with GenomeDaemon integration.
 * Verifies:
 * - Command execution (activate, deactivate, stats, register, unregister)
 * - Cache hits (already loaded)
 * - Cache misses (needs loading)
 * - Eviction (memory full)
 * - Multi-persona scenarios
 * - Thrashing detection
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  genomeActivate,
  genomeDeactivate,
  genomeStats,
  genomeRegister,
  genomeUnregister
} from './GenomeServer';
import { GenomeDaemon } from '../../../system/genome/server/GenomeDaemon';
import { MockLoRAAdapter } from '../../../system/genome/shared/MockLoRAAdapter';
import { randomUUID } from 'crypto';
import type { UUID } from '../../../core/types/CrossPlatformUUID';

describe('Genome Commands', () => {
  beforeEach(() => {
    // Reset daemon before each test
    GenomeDaemon.resetInstance();
  });

  afterEach(() => {
    GenomeDaemon.resetInstance();
  });

  describe('genome/register', () => {
    it('should register persona with default quota', async () => {
      const personaId = randomUUID() as UUID;

      const result = await genomeRegister({
        personaId,
        displayName: 'Test Persona'
      });

      expect(result.success).toBe(true);
      expect(result.registered).toBe(true);

      // Verify registration
      const stats = await genomeStats({ personaId });
      expect(stats.personas).toHaveLength(1);
      expect(stats.personas[0].displayName).toBe('Test Persona');
    });

    it('should register persona with custom quota and priority', async () => {
      const personaId = randomUUID() as UUID;

      const result = await genomeRegister({
        personaId,
        displayName: 'Custom Persona',
        quotaMB: 2048,
        priority: 0.8
      });

      expect(result.success).toBe(true);

      const stats = await genomeStats({ personaId });
      expect(stats.personas[0].memoryQuotaMB).toBe(2048);
      expect(stats.personas[0].priority).toBe(0.8);
    });

    it('should reject duplicate registration', async () => {
      const personaId = randomUUID() as UUID;

      await genomeRegister({ personaId, displayName: 'Persona 1' });
      const result = await genomeRegister({ personaId, displayName: 'Duplicate' });

      expect(result.success).toBe(false);
      expect(result.error).toMatch(/already registered/);
    });
  });

  describe('genome/unregister', () => {
    it('should unregister persona', async () => {
      const personaId = randomUUID() as UUID;

      await genomeRegister({ personaId, displayName: 'Test Persona' });
      const result = await genomeUnregister({ personaId });

      expect(result.success).toBe(true);
      expect(result.unregistered).toBe(true);

      // Verify unregistration
      const stats = await genomeStats({});
      expect(stats.personas).toHaveLength(0);
    });

    it('should reject unregistering unknown persona', async () => {
      const personaId = randomUUID() as UUID;

      const result = await genomeUnregister({ personaId });

      expect(result.success).toBe(false);
      expect(result.error).toMatch(/not registered/);
    });
  });

  describe('genome/activate', () => {
    it('should activate adapter for persona', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      // Register persona
      await genomeRegister({ personaId, displayName: 'Test Persona' });

      // Register adapter
      const daemon = GenomeDaemon.getInstance();
      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });
      daemon.getRegistry().register(adapter);

      // Activate
      const result = await genomeActivate({ personaId, adapterId });

      expect(result.success).toBe(true);
      expect(result.loaded).toBe(true);
      expect(result.thrashingDetected).toBe(false);

      // Verify activation
      const stats = await genomeStats({ personaId });
      expect(stats.personas[0].activeAdapters).toContain(adapterId);
      expect(stats.personas[0].memoryUsedMB).toBe(256);
    });

    it('should handle cache hit (already loaded)', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      await genomeRegister({ personaId, displayName: 'Test Persona' });

      const daemon = GenomeDaemon.getInstance();
      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });
      daemon.getRegistry().register(adapter);

      // First load
      await genomeActivate({ personaId, adapterId });

      // Second load (cache hit)
      const result = await genomeActivate({ personaId, adapterId });

      expect(result.success).toBe(true);
      expect(result.loaded).toBe(true);
      expect(adapter.getUsageCount()).toBe(2);  // Marked as used twice
    });

    it('should handle cache miss (needs loading)', async () => {
      const personaId = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      await genomeRegister({ personaId, displayName: 'Test Persona' });

      const daemon = GenomeDaemon.getInstance();
      const mock1 = new MockLoRAAdapter({
        id: adapter1,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 256
      });
      const mock2 = new MockLoRAAdapter({
        id: adapter2,
        name: 'adapter-2',
        domain: 'test',
        sizeMB: 384
      });
      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);

      // Load adapter1
      await genomeActivate({ personaId, adapterId: adapter1 });

      // Load adapter2 (cache miss, needs loading)
      const result = await genomeActivate({ personaId, adapterId: adapter2 });

      expect(result.success).toBe(true);
      expect(result.loaded).toBe(true);

      // Both should be loaded (enough memory)
      const stats = await genomeStats({ personaId });
      expect(stats.personas[0].activeAdapters).toHaveLength(2);
    });

    it('should trigger eviction when memory full', async () => {
      // Use small memory config to force eviction
      GenomeDaemon.resetInstance();
      const daemon = GenomeDaemon.getInstance({
        totalMemoryMB: 1024,
        defaultPersonaQuotaMB: 512,
        hysteresisSeconds: 0.1,  // Short for tests
        enableThrashingProtection: true
      });

      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;
      const adapter3 = randomUUID() as UUID;

      await genomeRegister({ personaId: persona1, displayName: 'Persona 1' });
      await genomeRegister({ personaId: persona2, displayName: 'Persona 2' });

      const mock1 = new MockLoRAAdapter({
        id: adapter1,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 512
      });
      const mock2 = new MockLoRAAdapter({
        id: adapter2,
        name: 'adapter-2',
        domain: 'test',
        sizeMB: 384
      });
      const mock3 = new MockLoRAAdapter({
        id: adapter3,
        name: 'adapter-3',
        domain: 'test',
        sizeMB: 256
      });
      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);
      daemon.getRegistry().register(mock3);

      // Load two adapters (total 896MB)
      await genomeActivate({ personaId: persona1, adapterId: adapter1 });
      await new Promise(resolve => setTimeout(resolve, 50));
      await genomeActivate({ personaId: persona2, adapterId: adapter2 });

      // Wait for hysteresis window
      await new Promise(resolve => setTimeout(resolve, 150));

      // Load third adapter (256MB) - should trigger eviction
      const result = await genomeActivate({ personaId: persona1, adapterId: adapter3 });

      expect(result.success).toBe(true);
      expect(result.loaded).toBe(true);

      // Verify eviction occurred
      const stats = await genomeStats({});
      expect(stats.totalEvictionCount).toBeGreaterThan(0);
    });

    it('should detect thrashing', async () => {
      // Use small memory and long hysteresis to force thrashing
      GenomeDaemon.resetInstance();
      const daemon = GenomeDaemon.getInstance({
        totalMemoryMB: 1024,
        defaultPersonaQuotaMB: 512,
        hysteresisSeconds: 60,  // Long window
        enableThrashingProtection: true
      });

      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      await genomeRegister({ personaId: persona1, displayName: 'Persona 1' });
      await genomeRegister({ personaId: persona2, displayName: 'Persona 2' });

      const mock1 = new MockLoRAAdapter({
        id: adapter1,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 768
      });
      const mock2 = new MockLoRAAdapter({
        id: adapter2,
        name: 'adapter-2',
        domain: 'test',
        sizeMB: 512
      });
      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);

      // Load adapter1 (768MB of 1024MB)
      await genomeActivate({ personaId: persona1, adapterId: adapter1 });

      // Try to load adapter2 immediately (would need to evict adapter1)
      const result = await genomeActivate({ personaId: persona2, adapterId: adapter2 });

      expect(result.success).toBe(true);
      expect(result.loaded).toBe(false);
      expect(result.thrashingDetected).toBe(true);

      // Verify thrashing stat
      const stats = await genomeStats({});
      expect(stats.totalThrashingEvents).toBe(1);
    });

    it('should reject activating for unregistered persona', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      const result = await genomeActivate({ personaId, adapterId });

      expect(result.success).toBe(false);
      expect(result.error).toMatch(/not registered/);
    });

    it('should reject activating non-existent adapter', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      await genomeRegister({ personaId, displayName: 'Test Persona' });

      const result = await genomeActivate({ personaId, adapterId });

      expect(result.success).toBe(false);
      expect(result.error).toMatch(/not found in registry/);
    });
  });

  describe('genome/deactivate', () => {
    it('should deactivate adapter', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      await genomeRegister({ personaId, displayName: 'Test Persona' });

      const daemon = GenomeDaemon.getInstance();
      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });
      daemon.getRegistry().register(adapter);

      // Activate then deactivate
      await genomeActivate({ personaId, adapterId });
      const result = await genomeDeactivate({ personaId, adapterId });

      expect(result.success).toBe(true);
      expect(result.unloaded).toBe(true);

      // Verify deactivation
      const stats = await genomeStats({ personaId });
      expect(stats.personas[0].activeAdapters).toHaveLength(0);
      expect(stats.personas[0].memoryUsedMB).toBe(0);
    });

    it('should reject deactivating non-loaded adapter', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      await genomeRegister({ personaId, displayName: 'Test Persona' });

      const daemon = GenomeDaemon.getInstance();
      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });
      daemon.getRegistry().register(adapter);

      const result = await genomeDeactivate({ personaId, adapterId });

      expect(result.success).toBe(false);
      expect(result.error).toMatch(/not loaded/);
    });
  });

  describe('genome/stats', () => {
    it('should return global stats', async () => {
      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;

      await genomeRegister({ personaId: persona1, displayName: 'Persona 1' });
      await genomeRegister({ personaId: persona2, displayName: 'Persona 2' });

      const daemon = GenomeDaemon.getInstance();
      const mock1 = new MockLoRAAdapter({
        id: adapter1,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 512
      });
      daemon.getRegistry().register(mock1);

      await genomeActivate({ personaId: persona1, adapterId: adapter1 });

      const stats = await genomeStats({});

      expect(stats.success).toBe(true);
      expect(stats.globalMemoryUsedMB).toBe(512);
      expect(stats.globalMemoryTotalMB).toBe(8192);
      expect(stats.personas).toHaveLength(2);
      expect(stats.totalLoadCount).toBe(1);
    });

    it('should return stats for specific persona', async () => {
      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;

      await genomeRegister({ personaId: persona1, displayName: 'Persona 1' });
      await genomeRegister({ personaId: persona2, displayName: 'Persona 2' });

      const stats = await genomeStats({ personaId: persona1 });

      expect(stats.success).toBe(true);
      expect(stats.personas).toHaveLength(1);
      expect(stats.personas[0].personaId).toBe(persona1);
    });

    it('should track memory utilization', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      await genomeRegister({ personaId, displayName: 'Test Persona' });

      const daemon = GenomeDaemon.getInstance();
      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 1024  // 1GB of 8GB
      });
      daemon.getRegistry().register(adapter);

      await genomeActivate({ personaId, adapterId });

      const stats = await genomeStats({});

      expect(stats.globalMemoryUtilization).toBeCloseTo(0.125);  // 1/8 = 0.125
      expect(stats.personas[0].memoryUtilization).toBeCloseTo(1.0);  // 1024/1024 = 1.0
    });
  });

  describe('Multi-persona scenarios', () => {
    it('should handle 3 personas with 2 adapter slots', async () => {
      // Scenario: 3 personas, total 1536MB memory, need to swap
      GenomeDaemon.resetInstance();
      const daemon = GenomeDaemon.getInstance({
        totalMemoryMB: 1536,
        defaultPersonaQuotaMB: 512,
        hysteresisSeconds: 0.1,
        enableThrashingProtection: true
      });

      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const persona3 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;
      const adapter3 = randomUUID() as UUID;

      await genomeRegister({ personaId: persona1, displayName: 'Persona 1' });
      await genomeRegister({ personaId: persona2, displayName: 'Persona 2' });
      await genomeRegister({ personaId: persona3, displayName: 'Persona 3' });

      const mock1 = new MockLoRAAdapter({
        id: adapter1,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 512
      });
      const mock2 = new MockLoRAAdapter({
        id: adapter2,
        name: 'adapter-2',
        domain: 'test',
        sizeMB: 512
      });
      const mock3 = new MockLoRAAdapter({
        id: adapter3,
        name: 'adapter-3',
        domain: 'test',
        sizeMB: 768  // Larger to force eviction
      });
      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);
      daemon.getRegistry().register(mock3);

      // Load adapters for persona1 and persona2 (fills memory)
      await genomeActivate({ personaId: persona1, adapterId: adapter1 });
      await new Promise(resolve => setTimeout(resolve, 50));
      await genomeActivate({ personaId: persona2, adapterId: adapter2 });

      // Wait for hysteresis
      await new Promise(resolve => setTimeout(resolve, 150));

      // Load adapter for persona3 (should evict adapter1)
      const result = await genomeActivate({ personaId: persona3, adapterId: adapter3 });

      expect(result.success).toBe(true);
      expect(result.loaded).toBe(true);

      // Verify LRU eviction
      const stats = await genomeStats({});
      expect(stats.totalEvictionCount).toBeGreaterThan(0);
      expect(stats.globalMemoryUsedMB).toBeLessThanOrEqual(1536);

      // Persona1's adapter should be evicted
      const persona1Stats = await genomeStats({ personaId: persona1 });
      expect(persona1Stats.personas[0].activeAdapters).toHaveLength(0);
    });
  });
});
