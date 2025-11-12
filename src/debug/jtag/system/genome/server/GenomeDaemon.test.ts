/**
 * GenomeDaemon Tests
 *
 * Tests for global genome coordination daemon.
 * Verifies:
 * - Singleton pattern
 * - Persona registration/unregistration
 * - Adapter loading with memory management
 * - LRU eviction across personas
 * - Thrashing protection
 * - Global statistics tracking
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { GenomeDaemon, type GenomeDaemonConfig } from './GenomeDaemon';
import { MockLoRAAdapter } from '../shared/MockLoRAAdapter';
import { randomUUID } from 'crypto';
import type { UUID } from '../../../core/types/CrossPlatformUUID';

describe('GenomeDaemon', () => {
  let config: GenomeDaemonConfig;
  let daemon: GenomeDaemon;

  beforeEach(() => {
    // Reset singleton before each test
    GenomeDaemon.resetInstance();

    config = {
      totalMemoryMB: 2048,
      defaultPersonaQuotaMB: 512,
      hysteresisSeconds: 0.1,  // 100ms for tests
      enableThrashingProtection: true
    };

    daemon = GenomeDaemon.getInstance(config);
  });

  afterEach(() => {
    GenomeDaemon.resetInstance();
  });

  describe('Singleton Pattern', () => {
    it('should return same instance on subsequent calls', () => {
      const instance1 = GenomeDaemon.getInstance();
      const instance2 = GenomeDaemon.getInstance();

      expect(instance1).toBe(instance2);
    });

    it('should require config on first initialization', () => {
      GenomeDaemon.resetInstance();

      expect(() => {
        GenomeDaemon.getInstance();
      }).toThrow('GenomeDaemon must be initialized with config');
    });

    it('should reset instance for testing', () => {
      const instance1 = GenomeDaemon.getInstance(config);
      GenomeDaemon.resetInstance();

      const instance2 = GenomeDaemon.getInstance(config);
      expect(instance1).not.toBe(instance2);
    });
  });

  describe('Persona Registration', () => {
    it('should register persona with default quota', () => {
      const personaId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      const state = daemon.getPersonaState(personaId);
      expect(state.getPersonaId()).toBe(personaId);
      expect(state.getDisplayName()).toBe('Test Persona');
      expect(state.getMemoryQuota()).toBe(512);
      expect(state.getPriority()).toBe(0.5);
    });

    it('should register persona with custom quota and priority', () => {
      const personaId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Custom Persona', 1024, 0.8);

      const state = daemon.getPersonaState(personaId);
      expect(state.getMemoryQuota()).toBe(1024);
      expect(state.getPriority()).toBe(0.8);
    });

    it('should reject duplicate registration', () => {
      const personaId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      expect(() => {
        daemon.registerPersona(personaId, 'Duplicate');
      }).toThrow(/Persona .* is already registered/);
    });

    it('should list all registered personas', () => {
      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;

      daemon.registerPersona(persona1, 'Persona 1');
      daemon.registerPersona(persona2, 'Persona 2');

      const personas = daemon.listPersonas();

      expect(personas).toHaveLength(2);
      expect(personas.map(p => p.getPersonaId())).toContain(persona1);
      expect(personas.map(p => p.getPersonaId())).toContain(persona2);
    });
  });

  describe('Persona Unregistration', () => {
    it('should unregister persona', async () => {
      const personaId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');
      await daemon.unregisterPersona(personaId);

      expect(() => {
        daemon.getPersonaState(personaId);
      }).toThrow(/Persona .* is not registered/);
    });

    it('should unload adapters when unregistering persona', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      // Register and load adapter
      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });
      daemon.getRegistry().register(adapter);
      await daemon.loadAdapter(personaId, adapterId);

      expect(daemon.getMemoryUsed()).toBe(256);

      // Unregister persona
      await daemon.unregisterPersona(personaId);

      // Adapter should be unloaded
      expect(daemon.getMemoryUsed()).toBe(0);
    });

    it('should reject unregistering unknown persona', async () => {
      const personaId = randomUUID() as UUID;

      await expect(async () => {
        await daemon.unregisterPersona(personaId);
      }).rejects.toThrow(/Persona .* is not registered/);
    });
  });

  describe('Adapter Loading', () => {
    it('should load adapter for persona', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });

      daemon.getRegistry().register(adapter);

      const loaded = await daemon.loadAdapter(personaId, adapterId);

      expect(loaded).toBe(true);
      expect(daemon.getMemoryUsed()).toBe(256);
      expect(daemon.getStats().totalLoadCount).toBe(1);
    });

    it('should skip loading if already loaded', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });

      daemon.getRegistry().register(adapter);

      await daemon.loadAdapter(personaId, adapterId);
      await daemon.loadAdapter(personaId, adapterId);  // Second load

      // Should only load once
      expect(daemon.getStats().totalLoadCount).toBe(1);
      expect(adapter.getUsageCount()).toBe(2);  // But mark as used
    });

    it('should reject loading non-existent adapter', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      await expect(async () => {
        await daemon.loadAdapter(personaId, adapterId);
      }).rejects.toThrow(/Adapter .* not found in registry/);
    });

    it('should reject loading for non-existent persona', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });

      daemon.getRegistry().register(adapter);

      await expect(async () => {
        await daemon.loadAdapter(personaId, adapterId);
      }).rejects.toThrow(/Persona .* is not registered/);
    });
  });

  describe('Adapter Unloading', () => {
    it('should unload adapter', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });

      daemon.getRegistry().register(adapter);

      await daemon.loadAdapter(personaId, adapterId);
      await daemon.unloadAdapter(personaId, adapterId);

      expect(daemon.getMemoryUsed()).toBe(0);
      expect(daemon.getStats().totalEvictionCount).toBe(1);
    });

    it('should reject unloading non-loaded adapter', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 256
      });

      daemon.getRegistry().register(adapter);

      await expect(async () => {
        await daemon.unloadAdapter(personaId, adapterId);
      }).rejects.toThrow(/Adapter .* is not loaded/);
    });
  });

  describe('Memory Management', () => {
    it('should track global memory usage', async () => {
      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      daemon.registerPersona(persona1, 'Persona 1');
      daemon.registerPersona(persona2, 'Persona 2');

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

      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);

      await daemon.loadAdapter(persona1, adapter1);
      await daemon.loadAdapter(persona2, adapter2);

      expect(daemon.getMemoryUsed()).toBe(896);
      expect(daemon.getMemoryAvailable()).toBe(1152);
      expect(daemon.getMemoryUtilization()).toBeCloseTo(0.4375);
    });

    it('should report when memory is available', () => {
      expect(daemon.canLoadAdapter(512)).toBe(true);
      expect(daemon.canLoadAdapter(2048)).toBe(true);
      expect(daemon.canLoadAdapter(3000)).toBe(false);
    });

    it('should update available memory after loads', async () => {
      const personaId = randomUUID() as UUID;
      const adapterId = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      const adapter = new MockLoRAAdapter({
        id: adapterId,
        name: 'test-adapter',
        domain: 'test',
        sizeMB: 1024
      });

      daemon.getRegistry().register(adapter);

      expect(daemon.canLoadAdapter(1500)).toBe(true);  // 2048MB total > 1500MB
      await daemon.loadAdapter(personaId, adapterId);
      expect(daemon.canLoadAdapter(1024)).toBe(true);  // 1024MB used, 1024MB available
      expect(daemon.canLoadAdapter(1500)).toBe(false); // Only 1024MB available now
    });
  });

  describe('LRU Eviction', () => {
    it('should evict oldest adapter when memory full', async () => {
      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;
      const adapter3 = randomUUID() as UUID;

      daemon.registerPersona(persona1, 'Persona 1');
      daemon.registerPersona(persona2, 'Persona 2');

      const mock1 = new MockLoRAAdapter({
        id: adapter1,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 1024
      });

      const mock2 = new MockLoRAAdapter({
        id: adapter2,
        name: 'adapter-2',
        domain: 'test',
        sizeMB: 768
      });

      const mock3 = new MockLoRAAdapter({
        id: adapter3,
        name: 'adapter-3',
        domain: 'test',
        sizeMB: 512
      });

      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);
      daemon.getRegistry().register(mock3);

      // Load two adapters (total 1792MB)
      await daemon.loadAdapter(persona1, adapter1);
      await new Promise(resolve => setTimeout(resolve, 50));
      await daemon.loadAdapter(persona2, adapter2);

      // Load third adapter (512MB) - needs eviction
      await new Promise(resolve => setTimeout(resolve, 150));  // Make adapter1 old
      const loaded = await daemon.loadAdapter(persona1, adapter3);

      expect(loaded).toBe(true);
      expect(daemon.getStats().totalEvictionCount).toBe(1);

      // adapter1 should be evicted (oldest)
      const state1 = daemon.getPersonaState(persona1);
      expect(state1.hasActiveAdapter(adapter1)).toBe(false);
      expect(state1.hasActiveAdapter(adapter3)).toBe(true);
    });

    it('should evict multiple adapters if needed', async () => {
      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const persona3 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;
      const adapter3 = randomUUID() as UUID;

      daemon.registerPersona(persona1, 'Persona 1');
      daemon.registerPersona(persona2, 'Persona 2');
      daemon.registerPersona(persona3, 'Persona 3');

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

      const mock3 = new MockLoRAAdapter({
        id: adapter3,
        name: 'adapter-3',
        domain: 'test',
        sizeMB: 1024
      });

      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);
      daemon.getRegistry().register(mock3);

      // Load two adapters (total 1280MB)
      await daemon.loadAdapter(persona1, adapter1);
      await new Promise(resolve => setTimeout(resolve, 50));
      await daemon.loadAdapter(persona2, adapter2);

      // Load large adapter (1024MB) - needs to evict both
      await new Promise(resolve => setTimeout(resolve, 150));
      await daemon.loadAdapter(persona3, adapter3);

      expect(daemon.getStats().totalEvictionCount).toBeGreaterThanOrEqual(2);
      expect(daemon.getMemoryUsed()).toBeLessThanOrEqual(2048);
    });
  });

  describe('Thrashing Protection', () => {
    it('should prevent evicting recently loaded adapters', async () => {
      // Reset daemon with smaller memory and long hysteresis
      GenomeDaemon.resetInstance();
      daemon = GenomeDaemon.getInstance({
        totalMemoryMB: 1024,
        defaultPersonaQuotaMB: 512,
        hysteresisSeconds: 60,  // 60s window
        enableThrashingProtection: true
      });

      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      daemon.registerPersona(persona1, 'Persona 1');
      daemon.registerPersona(persona2, 'Persona 2');

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

      // Load adapter that fills most memory (768MB of 1024MB)
      await daemon.loadAdapter(persona1, adapter1);

      // Try to load second adapter immediately (512MB, would require evicting adapter1)
      const loaded = await daemon.loadAdapter(persona2, adapter2);

      // Should fail due to thrashing protection
      expect(loaded).toBe(false);
      expect(daemon.getStats().totalThrashingEvents).toBe(1);
    });

    it('should allow eviction after hysteresis window', async () => {
      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      daemon.registerPersona(persona1, 'Persona 1');
      daemon.registerPersona(persona2, 'Persona 2');

      const mock1 = new MockLoRAAdapter({
        id: adapter1,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 1536
      });

      const mock2 = new MockLoRAAdapter({
        id: adapter2,
        name: 'adapter-2',
        domain: 'test',
        sizeMB: 1024
      });

      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);

      await daemon.loadAdapter(persona1, adapter1);

      // Wait for hysteresis window to pass
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should succeed now
      const loaded = await daemon.loadAdapter(persona2, adapter2);
      expect(loaded).toBe(true);
      expect(daemon.getStats().totalThrashingEvents).toBe(0);
    });

    it('should allow disabling thrashing protection', async () => {
      daemon = GenomeDaemon.getInstance({
        totalMemoryMB: 1024,
        defaultPersonaQuotaMB: 512,
        hysteresisSeconds: 60,
        enableThrashingProtection: false  // Disabled
      });

      const persona1 = randomUUID() as UUID;
      const persona2 = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      daemon.registerPersona(persona1, 'Persona 1');
      daemon.registerPersona(persona2, 'Persona 2');

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

      await daemon.loadAdapter(persona1, adapter1);

      // Should succeed even immediately (protection disabled)
      const loaded = await daemon.loadAdapter(persona2, adapter2);
      expect(loaded).toBe(true);
    });
  });

  describe('Statistics', () => {
    it('should track global statistics', async () => {
      const personaId = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

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

      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);

      await daemon.loadAdapter(personaId, adapter1);
      await daemon.loadAdapter(personaId, adapter2);
      await daemon.unloadAdapter(personaId, adapter1);

      const stats = daemon.getStats();

      expect(stats.totalLoadCount).toBe(2);
      expect(stats.totalEvictionCount).toBe(1);
      expect(stats.currentMemoryUsedMB).toBe(384);
      expect(stats.averageLoadTimeMs).toBeGreaterThan(0);
    });

    it('should calculate average load time', async () => {
      const personaId = randomUUID() as UUID;
      const adapter1 = randomUUID() as UUID;
      const adapter2 = randomUUID() as UUID;

      daemon.registerPersona(personaId, 'Test Persona');

      const mock1 = new MockLoRAAdapter({
        id: adapter1,
        name: 'adapter-1',
        domain: 'test',
        sizeMB: 128  // Fast load
      });

      const mock2 = new MockLoRAAdapter({
        id: adapter2,
        name: 'adapter-2',
        domain: 'test',
        sizeMB: 768  // Slow load
      });

      daemon.getRegistry().register(mock1);
      daemon.getRegistry().register(mock2);

      await daemon.loadAdapter(personaId, adapter1);
      await daemon.loadAdapter(personaId, adapter2);

      const stats = daemon.getStats();

      // Average should be between fast and slow
      expect(stats.averageLoadTimeMs).toBeGreaterThan(50);
      expect(stats.averageLoadTimeMs).toBeLessThan(200);
    });
  });
});
