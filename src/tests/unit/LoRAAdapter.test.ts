/**
 * LoRAAdapter Unit Tests
 *
 * Tests individual LoRA skill adapter with state management and LRU scoring
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { LoRAAdapter } from '../../system/user/server/modules/LoRAAdapter';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('LoRAAdapter', () => {
  let adapter: LoRAAdapter;
  const testId = 'test-adapter-id' as UUID;

  beforeEach(() => {
    adapter = new LoRAAdapter({
      id: testId,
      name: 'typescript-expertise',
      domain: 'code',
      path: './lora-adapters/typescript-expertise.safetensors',
      sizeMB: 60,
      priority: 0.7
    });
  });

  describe('Initialization', () => {
    it('should initialize with correct state', () => {
      expect(adapter.getId()).toBe(testId);
      expect(adapter.getName()).toBe('typescript-expertise');
      expect(adapter.getDomain()).toBe('code');
      expect(adapter.getPath()).toBe('./lora-adapters/typescript-expertise.safetensors');
      expect(adapter.getSize()).toBe(60);
      expect(adapter.getPriority()).toBe(0.7);
      expect(adapter.isLoaded()).toBe(false);
      expect(adapter.isTrainingActive()).toBe(false);
    });

    it('should default priority to 0.5', () => {
      const defaultAdapter = new LoRAAdapter({
        id: testId,
        name: 'test',
        domain: 'test',
        path: './test.safetensors',
        sizeMB: 50
      });

      expect(defaultAdapter.getPriority()).toBe(0.5);
    });

    it('should initialize with lastUsed = 0', () => {
      expect(adapter.getLastUsed()).toBe(0);
    });
  });

  describe('Load/Unload', () => {
    it('should load adapter and update state', async () => {
      expect(adapter.isLoaded()).toBe(false);

      await adapter.load();

      expect(adapter.isLoaded()).toBe(true);
      expect(adapter.getLastUsed()).toBeGreaterThan(0);
    });

    it('should not reload if already loaded', async () => {
      await adapter.load();
      const firstLoadTime = adapter.getLastUsed();

      await new Promise(resolve => setTimeout(resolve, 10));
      await adapter.load();
      const secondLoadTime = adapter.getLastUsed();

      // lastUsed should not change on redundant load
      expect(secondLoadTime).toBe(firstLoadTime);
    });

    it('should unload adapter and update state', async () => {
      await adapter.load();
      expect(adapter.isLoaded()).toBe(true);

      await adapter.unload();

      expect(adapter.isLoaded()).toBe(false);
    });

    it('should handle unload when already unloaded', async () => {
      expect(adapter.isLoaded()).toBe(false);

      await expect(adapter.unload()).resolves.not.toThrow();
      expect(adapter.isLoaded()).toBe(false);
    });
  });

  describe('Mark Used', () => {
    it('should update lastUsed timestamp', async () => {
      await adapter.load();
      const initialTime = adapter.getLastUsed();

      await new Promise(resolve => setTimeout(resolve, 10));
      adapter.markUsed();

      const updatedTime = adapter.getLastUsed();
      expect(updatedTime).toBeGreaterThan(initialTime);
    });

    it('should work on unloaded adapter', () => {
      expect(adapter.isLoaded()).toBe(false);

      adapter.markUsed();

      expect(adapter.getLastUsed()).toBeGreaterThan(0);
    });
  });

  describe('Training Mode', () => {
    it('should enable training mode when loaded', async () => {
      await adapter.load();

      await adapter.enableTraining();

      expect(adapter.isTrainingActive()).toBe(true);
    });

    it('should throw when enabling training on unloaded adapter', async () => {
      expect(adapter.isLoaded()).toBe(false);

      await expect(adapter.enableTraining())
        .rejects
        .toThrow('Cannot enable training - adapter typescript-expertise not loaded');
    });

    it('should disable training mode', async () => {
      await adapter.load();
      await adapter.enableTraining();
      expect(adapter.isTrainingActive()).toBe(true);

      await adapter.disableTraining();

      expect(adapter.isTrainingActive()).toBe(false);
    });

    it('should handle disable when not training', async () => {
      await adapter.load();
      expect(adapter.isTrainingActive()).toBe(false);

      await expect(adapter.disableTraining()).resolves.not.toThrow();
      expect(adapter.isTrainingActive()).toBe(false);
    });
  });

  describe('Eviction Score', () => {
    it('should calculate score based on age and priority', async () => {
      await adapter.load();

      await new Promise(resolve => setTimeout(resolve, 100));

      const score = adapter.calculateEvictionScore();

      // Score = age_seconds / (priority * 10)
      // ~0.1s / (0.7 * 10) = ~0.014
      expect(score).toBeGreaterThan(0);
      expect(score).toBeLessThan(1);
    });

    it('should return Infinity for critical adapters (priority > 0.9)', async () => {
      const criticalAdapter = new LoRAAdapter({
        id: testId,
        name: 'critical',
        domain: 'core',
        path: './critical.safetensors',
        sizeMB: 50,
        priority: 0.95
      });

      await criticalAdapter.load();
      await new Promise(resolve => setTimeout(resolve, 100));

      const score = criticalAdapter.calculateEvictionScore();
      expect(score).toBe(Infinity);
    });

    it('should increase score with age', async () => {
      await adapter.load();

      await new Promise(resolve => setTimeout(resolve, 50));
      const score1 = adapter.calculateEvictionScore();

      await new Promise(resolve => setTimeout(resolve, 50));
      const score2 = adapter.calculateEvictionScore();

      expect(score2).toBeGreaterThan(score1);
    });

    it('should decrease score with higher priority', async () => {
      const lowPriorityAdapter = new LoRAAdapter({
        id: testId,
        name: 'low-priority',
        domain: 'test',
        path: './low.safetensors',
        sizeMB: 50,
        priority: 0.3
      });

      await adapter.load();  // priority 0.7
      await lowPriorityAdapter.load();  // priority 0.3

      await new Promise(resolve => setTimeout(resolve, 100));

      const highPriorityScore = adapter.calculateEvictionScore();
      const lowPriorityScore = lowPriorityAdapter.calculateEvictionScore();

      // Lower priority = higher eviction score
      expect(lowPriorityScore).toBeGreaterThan(highPriorityScore);
    });
  });

  describe('State Serialization', () => {
    it('should return complete state snapshot', async () => {
      await adapter.load();
      adapter.markUsed();

      const state = adapter.getState();

      expect(state.id).toBe(testId);
      expect(state.name).toBe('typescript-expertise');
      expect(state.domain).toBe('code');
      expect(state.path).toBe('./lora-adapters/typescript-expertise.safetensors');
      expect(state.loaded).toBe(true);
      expect(state.lastUsed).toBeGreaterThan(0);
      expect(state.sizeMB).toBe(60);
      expect(state.trainingActive).toBe(false);
      expect(state.priority).toBe(0.7);
    });

    it('should return snapshot not live reference', async () => {
      const state1 = adapter.getState();

      await adapter.load();

      const state2 = adapter.getState();

      // Original snapshot should not change
      expect(state1.loaded).toBe(false);
      expect(state2.loaded).toBe(true);
    });
  });
});
