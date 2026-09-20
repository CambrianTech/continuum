/**
 * AdapterRegistry Tests
 *
 * Tests for the global adapter registry.
 * Verifies:
 * - Registration (add/remove adapters)
 * - Lookup (by ID, by name)
 * - Listing (all, by domain)
 * - Duplicate prevention
 * - Loaded adapter protection
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { AdapterRegistry } from './AdapterRegistry';
import { MockLoRAAdapter, type MockLoRAConfig } from './MockLoRAAdapter';
import { randomUUID } from 'crypto';
import type { UUID } from '../../core/types/CrossPlatformUUID';

describe('AdapterRegistry', () => {
  let registry: AdapterRegistry;
  let adapter1: MockLoRAAdapter;
  let adapter2: MockLoRAAdapter;
  let adapter3: MockLoRAAdapter;

  beforeEach(() => {
    registry = new AdapterRegistry();

    adapter1 = new MockLoRAAdapter({
      id: randomUUID() as UUID,
      name: 'wine-expertise',
      domain: 'knowledge',
      sizeMB: 512,
      priority: 0.8
    });

    adapter2 = new MockLoRAAdapter({
      id: randomUUID() as UUID,
      name: 'action-hero-style',
      domain: 'personality',
      sizeMB: 256,
      priority: 0.6
    });

    adapter3 = new MockLoRAAdapter({
      id: randomUUID() as UUID,
      name: 'typescript-expertise',
      domain: 'knowledge',
      sizeMB: 384,
      priority: 0.7
    });
  });

  describe('register()', () => {
    it('should register a new adapter', () => {
      registry.register(adapter1);

      expect(registry.count()).toBe(1);
      expect(registry.hasId(adapter1.getId())).toBe(true);
      expect(registry.hasName(adapter1.getName())).toBe(true);
    });

    it('should register multiple adapters', () => {
      registry.register(adapter1);
      registry.register(adapter2);
      registry.register(adapter3);

      expect(registry.count()).toBe(3);
    });

    it('should throw error if adapter ID already registered', () => {
      registry.register(adapter1);

      expect(() => registry.register(adapter1)).toThrow('already registered');
    });

    it('should throw error if adapter name already registered', () => {
      registry.register(adapter1);

      // Create adapter with same name but different ID
      const duplicate = new MockLoRAAdapter({
        id: randomUUID() as UUID,
        name: adapter1.getName(),
        domain: 'test',
        sizeMB: 128
      });

      expect(() => registry.register(duplicate)).toThrow('already registered');
    });
  });

  describe('getById()', () => {
    it('should retrieve adapter by ID', () => {
      registry.register(adapter1);

      const found = registry.getById(adapter1.getId());
      expect(found).toBe(adapter1);
      expect(found?.getName()).toBe('wine-expertise');
    });

    it('should return undefined for unknown ID', () => {
      const found = registry.getById(randomUUID() as UUID);
      expect(found).toBeUndefined();
    });
  });

  describe('getByName()', () => {
    it('should retrieve adapter by name', () => {
      registry.register(adapter1);

      const found = registry.getByName('wine-expertise');
      expect(found).toBe(adapter1);
      expect(found?.getId()).toBe(adapter1.getId());
    });

    it('should return undefined for unknown name', () => {
      const found = registry.getByName('nonexistent');
      expect(found).toBeUndefined();
    });
  });

  describe('hasId() and hasName()', () => {
    it('should check if adapter exists by ID', () => {
      registry.register(adapter1);

      expect(registry.hasId(adapter1.getId())).toBe(true);
      expect(registry.hasId(randomUUID() as UUID)).toBe(false);
    });

    it('should check if adapter exists by name', () => {
      registry.register(adapter1);

      expect(registry.hasName('wine-expertise')).toBe(true);
      expect(registry.hasName('nonexistent')).toBe(false);
    });
  });

  describe('listAll()', () => {
    it('should return empty array for empty registry', () => {
      expect(registry.listAll()).toEqual([]);
    });

    it('should list all registered adapters', () => {
      registry.register(adapter1);
      registry.register(adapter2);
      registry.register(adapter3);

      const all = registry.listAll();
      expect(all).toHaveLength(3);
      expect(all).toContain(adapter1);
      expect(all).toContain(adapter2);
      expect(all).toContain(adapter3);
    });
  });

  describe('listByDomain()', () => {
    it('should return empty array for unknown domain', () => {
      registry.register(adapter1);

      expect(registry.listByDomain('nonexistent')).toEqual([]);
    });

    it('should list adapters filtered by domain', () => {
      registry.register(adapter1);  // knowledge
      registry.register(adapter2);  // personality
      registry.register(adapter3);  // knowledge

      const knowledge = registry.listByDomain('knowledge');
      expect(knowledge).toHaveLength(2);
      expect(knowledge).toContain(adapter1);
      expect(knowledge).toContain(adapter3);

      const personality = registry.listByDomain('personality');
      expect(personality).toHaveLength(1);
      expect(personality).toContain(adapter2);
    });
  });

  describe('getMetadata()', () => {
    it('should return adapter metadata', () => {
      registry.register(adapter1);

      const meta = registry.getMetadata(adapter1.getId());
      expect(meta).toEqual({
        id: adapter1.getId(),
        name: 'wine-expertise',
        domain: 'knowledge',
        sizeMB: 512,
        priority: 0.8
      });
    });

    it('should return undefined for unknown adapter', () => {
      const meta = registry.getMetadata(randomUUID() as UUID);
      expect(meta).toBeUndefined();
    });
  });

  describe('unregister()', () => {
    it('should unregister adapter by ID', () => {
      registry.register(adapter1);
      expect(registry.count()).toBe(1);

      registry.unregister(adapter1.getId());
      expect(registry.count()).toBe(0);
      expect(registry.hasId(adapter1.getId())).toBe(false);
      expect(registry.hasName(adapter1.getName())).toBe(false);
    });

    it('should throw error if adapter not registered', () => {
      expect(() => registry.unregister(randomUUID() as UUID))
        .toThrow('not registered');
    });

    it('should throw error if adapter is loaded', async () => {
      registry.register(adapter1);
      await adapter1.load();

      expect(() => registry.unregister(adapter1.getId()))
        .toThrow('Cannot unregister loaded adapter');
    });
  });

  describe('count()', () => {
    it('should return 0 for empty registry', () => {
      expect(registry.count()).toBe(0);
    });

    it('should return correct count', () => {
      registry.register(adapter1);
      expect(registry.count()).toBe(1);

      registry.register(adapter2);
      expect(registry.count()).toBe(2);

      registry.unregister(adapter1.getId());
      expect(registry.count()).toBe(1);
    });
  });

  describe('clear()', () => {
    it('should clear empty registry', () => {
      registry.clear();
      expect(registry.count()).toBe(0);
    });

    it('should clear all adapters', () => {
      registry.register(adapter1);
      registry.register(adapter2);
      registry.register(adapter3);

      registry.clear();
      expect(registry.count()).toBe(0);
      expect(registry.listAll()).toEqual([]);
    });

    it('should throw error if any adapters are loaded', async () => {
      registry.register(adapter1);
      registry.register(adapter2);

      await adapter1.load();

      expect(() => registry.clear()).toThrow('Cannot clear registry');
    });
  });

  describe('Integration: Multiple adapters with same domain', () => {
    it('should handle multiple adapters in same domain', () => {
      // Register 2 knowledge adapters
      registry.register(adapter1);  // wine-expertise (knowledge)
      registry.register(adapter3);  // typescript-expertise (knowledge)

      const knowledge = registry.listByDomain('knowledge');
      expect(knowledge).toHaveLength(2);

      // Should be able to retrieve each by name
      const wine = registry.getByName('wine-expertise');
      const ts = registry.getByName('typescript-expertise');
      expect(wine).toBe(adapter1);
      expect(ts).toBe(adapter3);
    });
  });
});
