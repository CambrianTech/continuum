/**
 * PersonaGenome Unit Tests (Phase 6 - New LoRAAdapter Architecture)
 *
 * Tests the LoRA adapter paging system with LRU eviction
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { PersonaGenome } from '../../system/user/server/modules/PersonaGenome';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('PersonaGenome', () => {
  let genome: PersonaGenome;

  beforeEach(() => {
    genome = new PersonaGenome({
      baseModel: 'llama3.2:3b',
      memoryBudgetMB: 200,  // Small budget for testing eviction
      adaptersPath: './test-adapters',
      initialAdapters: [
        { name: 'conversational', domain: 'chat', path: './test-adapters/conversational.safetensors', sizeMB: 50, priority: 0.7 },
        { name: 'typescript-expertise', domain: 'code', path: './test-adapters/typescript.safetensors', sizeMB: 60, priority: 0.6 },
        { name: 'rust-expertise', domain: 'code', path: './test-adapters/rust.safetensors', sizeMB: 55, priority: 0.6 },
        { name: 'self-improvement', domain: 'self', path: './test-adapters/self.safetensors', sizeMB: 40, priority: 0.5 }
      ]
    });
  });

  describe('Initialization', () => {
    it('should initialize with config', () => {
      const state = genome.getState();

      expect(state.baseModel).toBe('llama3.2:3b');
      expect(state.memoryBudgetMB).toBe(200);
      expect(state.memoryUsedMB).toBe(0);
      expect(state.memoryPressure).toBe(0);
      expect(state.availableAdapters).toHaveLength(4);
      expect(state.activeAdapters).toHaveLength(0);
      expect(state.currentAdapter).toBeNull();
      expect(state.learningMode).toBe(false);
    });

    it('should register adapters but not load them', () => {
      const state = genome.getState();

      // Adapters registered
      expect(state.availableAdapters).toHaveLength(4);

      // But not loaded yet
      expect(state.activeAdapters).toHaveLength(0);
      expect(state.memoryUsedMB).toBe(0);
    });
  });

  describe('Adapter Activation (Cache Miss)', () => {
    it('should load adapter on first access', async () => {
      await genome.activateSkill('conversational');

      const state = genome.getState();
      expect(state.activeAdapters).toHaveLength(1);
      expect(state.memoryUsedMB).toBe(50);
      expect(state.currentAdapter).toBe('conversational');
    });

    it('should load multiple adapters', async () => {
      await genome.activateSkill('conversational');  // 50MB
      await genome.activateSkill('typescript-expertise');  // 60MB

      const state = genome.getState();
      expect(state.activeAdapters).toHaveLength(2);
      expect(state.memoryUsedMB).toBe(110);
    });

    it('should warn when activating unregistered skill', async () => {
      await genome.activateSkill('python-expertise');

      const state = genome.getState();
      expect(state.activeAdapters).toHaveLength(0);
      expect(state.currentAdapter).toBeNull();
    });
  });

  describe('Adapter Caching (Cache Hit)', () => {
    it('should use cached adapter on second access', async () => {
      await genome.activateSkill('conversational');
      const stateAfterLoad = genome.getState();
      expect(stateAfterLoad.activeAdapters).toHaveLength(1);
      expect(stateAfterLoad.memoryUsedMB).toBe(50);

      await genome.activateSkill('conversational');
      const stateAfterHit = genome.getState();

      // No change in memory or adapter count
      expect(stateAfterHit.activeAdapters).toHaveLength(1);
      expect(stateAfterHit.memoryUsedMB).toBe(50);
    });

    it('should update currentAdapter on cache hit', async () => {
      await genome.activateSkill('conversational');
      await genome.activateSkill('typescript-expertise');

      // Switch back to conversational (cache hit)
      await genome.activateSkill('conversational');

      const state = genome.getState();
      expect(state.currentAdapter).toBe('conversational');
    });
  });

  describe('Memory Budget Enforcement', () => {
    it('should respect memory budget', async () => {
      // Load adapters up to budget (200MB)
      await genome.activateSkill('conversational');     // 50MB
      await genome.activateSkill('typescript-expertise'); // 60MB (110MB total)
      await genome.activateSkill('rust-expertise');      // 55MB (165MB total)
      await genome.activateSkill('self-improvement');    // 40MB (205MB > 200MB!)

      const state = genome.getState();
      expect(state.memoryUsedMB).toBeLessThanOrEqual(200);
    });

    it('should track memory pressure', async () => {
      await genome.activateSkill('conversational');     // 50MB
      let state = genome.getState();
      expect(state.memoryPressure).toBeCloseTo(0.25, 2);  // 50/200

      await genome.activateSkill('typescript-expertise'); // 60MB
      state = genome.getState();
      expect(state.memoryPressure).toBeCloseTo(0.55, 2);  // 110/200
    });
  });

  describe('LRU Eviction', () => {
    it('should evict LRU adapter when memory full', async () => {
      // Fill budget
      await genome.activateSkill('conversational');     // 50MB (accessed first)
      await genome.activateSkill('typescript-expertise'); // 60MB
      await genome.activateSkill('rust-expertise');      // 55MB (165MB)

      // Load 4th adapter (exceeds budget, should evict conversational)
      await genome.activateSkill('self-improvement');    // 40MB

      const state = genome.getState();
      expect(state.memoryUsedMB).toBeLessThanOrEqual(200);

      // conversational should be evicted
      const activeNames = state.activeAdapters.map(a => a.name);
      expect(activeNames).not.toContain('conversational');
    });

    it('should keep recently used adapters', async () => {
      await genome.activateSkill('conversational');
      await genome.activateSkill('typescript-expertise');
      await genome.activateSkill('rust-expertise');

      // Access conversational again (make it recently used)
      await genome.activateSkill('conversational');

      // Load 4th adapter (should evict typescript, not conversational)
      await genome.activateSkill('self-improvement');

      const state = genome.getState();
      const activeNames = state.activeAdapters.map(a => a.name);

      expect(activeNames).toContain('conversational');  // Recently used, kept
      expect(activeNames).not.toContain('typescript-expertise');  // LRU, evicted
    });

    it('should never evict critical adapters (priority > 0.9)', async () => {
      genome.registerAdapter({
        name: 'critical-core',
        domain: 'core',
        path: './critical.safetensors',
        sizeMB: 50,
        priority: 0.95  // Critical!
      });

      await genome.activateSkill('critical-core');
      await genome.activateSkill('conversational');
      await genome.activateSkill('typescript-expertise');
      await genome.activateSkill('rust-expertise');

      // Load another adapter (critical should NEVER be evicted)
      await genome.activateSkill('self-improvement');

      const state = genome.getState();
      const activeNames = state.activeAdapters.map(a => a.name);

      expect(activeNames).toContain('critical-core');  // Never evicted!
    });
  });

  describe('Learning Mode', () => {
    it('should enable learning mode for loaded adapter', async () => {
      await genome.activateSkill('typescript-expertise');

      await genome.enableLearningMode('typescript-expertise');

      const state = genome.getState();
      expect(state.learningMode).toBe(true);
    });

    it('should throw when enabling learning on unloaded adapter', async () => {
      await expect(genome.enableLearningMode('conversational'))
        .rejects
        .toThrow('Cannot enable learning mode - adapter conversational not loaded');
    });

    it('should disable learning mode', async () => {
      await genome.activateSkill('typescript-expertise');
      await genome.enableLearningMode('typescript-expertise');

      await genome.disableLearningMode('typescript-expertise');

      const state = genome.getState();
      expect(state.learningMode).toBe(false);
    });
  });

  describe('State Management', () => {
    it('should track current adapter', async () => {
      await genome.activateSkill('conversational');
      expect(genome.getCurrentAdapter()?.getName()).toBe('conversational');

      await genome.activateSkill('typescript-expertise');
      expect(genome.getCurrentAdapter()?.getName()).toBe('typescript-expertise');
    });

    it('should return all active adapters', async () => {
      await genome.activateSkill('conversational');
      await genome.activateSkill('typescript-expertise');

      const active = genome.getActiveAdapters();
      expect(active).toHaveLength(2);
      expect(active.map(a => a.getName())).toContain('conversational');
      expect(active.map(a => a.getName())).toContain('typescript-expertise');
    });

    it('should return all adapters (active + available)', async () => {
      await genome.activateSkill('conversational');

      const all = genome.getAllAdapters();
      expect(all.length).toBeGreaterThanOrEqual(4);  // At least the initial 4
    });

    it('should calculate memory pressure', async () => {
      expect(genome.getMemoryPressure()).toBe(0);

      await genome.activateSkill('conversational');  // 50MB
      expect(genome.getMemoryPressure()).toBeCloseTo(0.25, 2);
    });
  });

  describe('Shutdown', () => {
    it('should unload all adapters', async () => {
      await genome.activateSkill('conversational');
      await genome.activateSkill('typescript-expertise');

      let state = genome.getState();
      expect(state.activeAdapters).toHaveLength(2);
      expect(state.memoryUsedMB).toBeGreaterThan(0);

      await genome.shutdown();

      state = genome.getState();
      expect(state.activeAdapters).toHaveLength(0);
      expect(state.memoryUsedMB).toBe(0);
      expect(state.currentAdapter).toBeNull();
    });
  });

  describe('Integration Scenarios', () => {
    it('should handle task-based adapter switching', async () => {
      // Chat task
      await genome.activateSkill('conversational');
      expect(genome.getCurrentAdapter()?.getDomain()).toBe('chat');

      // Code task
      await genome.activateSkill('typescript-expertise');
      expect(genome.getCurrentAdapter()?.getDomain()).toBe('code');

      // Self-task
      await genome.activateSkill('self-improvement');
      expect(genome.getCurrentAdapter()?.getDomain()).toBe('self');
    });

    it('should maintain working set via LRU', async () => {
      // Simulate alternating between chat and code
      await genome.activateSkill('conversational');
      await genome.activateSkill('typescript-expertise');
      await genome.activateSkill('conversational');  // Hit
      await genome.activateSkill('typescript-expertise');  // Hit

      const state = genome.getState();
      // Both should stay in cache
      expect(state.activeAdapters).toHaveLength(2);
    });
  });
});
