/**
 * Semantic Cognition Unit Tests
 *
 * Tests for the semantic cognition infrastructure:
 * - IEmbeddable interface utilities
 * - ModelContextWindows configuration
 * - RAGBudgetManager allocation
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { isEmbeddable, needsEmbedding, type IEmbeddable } from '../../system/data/interfaces/IEmbeddable';
import {
  getContextWindow,
  getInferenceSpeed,
  isLargeContextModel,
  isSlowLocalModel,
  getRecommendedMaxOutputTokens,
  MODEL_CONTEXT_WINDOWS,
  DEFAULT_CONTEXT_WINDOW
} from '../../system/shared/ModelContextWindows';
import { ModelRegistry } from '../../system/shared/ModelRegistry';
import {
  RAGBudgetManager,
  allocateChatBudget,
  type RAGSourceBudget
} from '../../system/rag/shared/RAGBudgetManager';

describe('IEmbeddable', () => {
  describe('isEmbeddable', () => {
    it('should return true for objects with getEmbeddableContent method', () => {
      const embeddable = {
        getEmbeddableContent: () => 'test content'
      };
      expect(isEmbeddable(embeddable)).toBe(true);
    });

    it('should return false for objects without getEmbeddableContent', () => {
      expect(isEmbeddable({})).toBe(false);
      expect(isEmbeddable({ content: 'test' })).toBe(false);
    });

    it('should return false for null and undefined', () => {
      expect(isEmbeddable(null)).toBe(false);
      expect(isEmbeddable(undefined)).toBe(false);
    });

    it('should return false for primitives', () => {
      expect(isEmbeddable('string')).toBe(false);
      expect(isEmbeddable(123)).toBe(false);
    });
  });

  describe('needsEmbedding', () => {
    it('should return true for entity without embedding', () => {
      const entity: IEmbeddable = {
        getEmbeddableContent: () => 'test'
      };
      expect(needsEmbedding(entity)).toBe(true);
    });

    it('should return true for entity with empty embedding', () => {
      const entity: IEmbeddable = {
        getEmbeddableContent: () => 'test',
        embedding: []
      };
      expect(needsEmbedding(entity)).toBe(true);
    });

    it('should return false for entity with valid embedding', () => {
      const entity: IEmbeddable = {
        getEmbeddableContent: () => 'test',
        embedding: [0.1, 0.2, 0.3],
        embeddedAt: new Date().toISOString() as any
      };
      expect(needsEmbedding(entity)).toBe(false);
    });

    it('should return true for stale embedding', () => {
      const staleDate = new Date(Date.now() - 10 * 24 * 60 * 60 * 1000); // 10 days ago
      const entity: IEmbeddable = {
        getEmbeddableContent: () => 'test',
        embedding: [0.1, 0.2, 0.3],
        embeddedAt: staleDate.toISOString() as any
      };
      expect(needsEmbedding(entity, 7 * 24 * 60 * 60 * 1000)).toBe(true); // 7 day max age
    });
  });
});

describe('ModelContextWindows', () => {
  describe('getContextWindow', () => {
    it('should return correct context window for known models', () => {
      expect(getContextWindow('gpt-4')).toBe(8192);
      expect(getContextWindow('gpt-4o')).toBe(128000);
      expect(getContextWindow('claude-3-opus')).toBe(200000);
      expect(getContextWindow('llama3.2:3b')).toBe(128000);
    });

    it('should return default for unknown models', () => {
      expect(getContextWindow('unknown-model')).toBe(DEFAULT_CONTEXT_WINDOW);
    });

    it('should handle versioned model names', () => {
      // Base model lookup when version not in table
      expect(getContextWindow('llama3.2:7b')).toBe(128000); // Should find llama3.2
    });

    it('should have reasonable values for all models', () => {
      for (const [model, size] of Object.entries(MODEL_CONTEXT_WINDOWS)) {
        expect(size).toBeGreaterThan(0);
        expect(size).toBeLessThanOrEqual(1100000); // Max ~1M tokens (Gemini 2.0 Flash is 1048576)
      }
    });
  });

  describe('isLargeContextModel', () => {
    it('should return true for large context models', () => {
      expect(isLargeContextModel('gpt-4o')).toBe(true); // 128K
      expect(isLargeContextModel('claude-3-opus')).toBe(true); // 200K
    });

    it('should return false for small context models', () => {
      expect(isLargeContextModel('gpt-4')).toBe(false); // 8K
    });
  });

  describe('getRecommendedMaxOutputTokens', () => {
    it('should return reasonable output tokens for small models', () => {
      const tokens = getRecommendedMaxOutputTokens('gpt-4');
      expect(tokens).toBeGreaterThan(0);
      expect(tokens).toBeLessThanOrEqual(8192 * 0.25); // Max 25% of context
    });

    it('should cap output tokens for large models', () => {
      const tokens = getRecommendedMaxOutputTokens('claude-3-opus');
      expect(tokens).toBeLessThanOrEqual(4096); // Capped at 4K
    });
  });
});

describe('ModelRegistry - Provider-Scoped Lookups', () => {
  let registry: ModelRegistry;

  beforeEach(() => {
    registry = ModelRegistry.sharedInstance();
    registry.clear();
  });

  it('should store same model under different providers without collision', () => {
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    // Scoped lookups return correct values
    expect(registry.contextWindow('meta-llama/Llama-3.1-8B-Instruct', 'candle')).toBe(1400);
    expect(registry.contextWindow('meta-llama/Llama-3.1-8B-Instruct', 'together')).toBe(131072);
  });

  it('should return largest context window for unscoped lookup with multiple providers', () => {
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    // Unscoped returns largest (cloud wins for backward compat)
    expect(registry.contextWindow('meta-llama/Llama-3.1-8B-Instruct')).toBe(131072);
  });

  it('should return single provider entry for unscoped lookup with one provider', () => {
    registry.register({
      modelId: 'claude-sonnet-4-5-20250929',
      contextWindow: 200000,
      provider: 'anthropic',
      discoveredAt: Date.now()
    });

    expect(registry.contextWindow('claude-sonnet-4-5-20250929')).toBe(200000);
  });

  it('should return undefined for unknown provider', () => {
    registry.register({
      modelId: 'gpt-4o',
      contextWindow: 128000,
      provider: 'openai',
      discoveredAt: Date.now()
    });

    expect(registry.contextWindow('gpt-4o', 'candle')).toBeUndefined();
  });

  it('getAll should return all providers for a model', () => {
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    const all = registry.getAll('meta-llama/Llama-3.1-8B-Instruct');
    expect(all.length).toBe(2);
    const providers = all.map(m => m.provider).sort();
    expect(providers).toEqual(['candle', 'together']);
  });

  it('should apply date-suffix normalization within provider scope', () => {
    registry.register({
      modelId: 'claude-sonnet-4-5',
      contextWindow: 200000,
      provider: 'anthropic',
      discoveredAt: Date.now()
    });

    // Date-suffix stripped lookup should find it
    expect(registry.contextWindow('claude-sonnet-4-5-20250929', 'anthropic')).toBe(200000);
  });

  it('discoveredCount should reflect provider-scoped entries', () => {
    registry.register({
      modelId: 'llama-8b',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'llama-8b',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    // Two entries (one per provider), not one
    expect(registry.discoveredCount).toBe(2);
  });
});

describe('Provider-Scoped ModelContextWindows', () => {
  beforeEach(() => {
    ModelRegistry.sharedInstance().clear();
  });

  it('getContextWindow should return provider-scoped value from registry', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    expect(getContextWindow('meta-llama/Llama-3.1-8B-Instruct', 'candle')).toBe(1400);
    expect(getContextWindow('meta-llama/Llama-3.1-8B-Instruct', 'together')).toBe(131072);
  });

  it('getContextWindow should fall back to static map when provider not in registry', () => {
    // No registry entries — should use static map
    expect(getContextWindow('meta-llama/Llama-3.1-8B-Instruct', 'candle')).toBe(1400);
  });

  it('getInferenceSpeed should return local TPS for local provider in registry', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });

    // Bug fix verification: should return 40 TPS (static map), not 1000 TPS (cloud assumption)
    const speed = getInferenceSpeed('meta-llama/Llama-3.1-8B-Instruct', 'candle');
    expect(speed).toBe(40);  // From MODEL_INFERENCE_SPEEDS static map
  });

  it('getInferenceSpeed should return 1000 TPS for cloud provider in registry', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    const speed = getInferenceSpeed('meta-llama/Llama-3.1-8B-Instruct', 'together');
    expect(speed).toBe(1000);  // Cloud API speed
  });

  it('isSlowLocalModel should be true for candle models', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 1400,
      provider: 'candle',
      discoveredAt: Date.now()
    });

    expect(isSlowLocalModel('meta-llama/Llama-3.1-8B-Instruct', 'candle')).toBe(true);
  });

  it('isSlowLocalModel should be false for cloud models', () => {
    const registry = ModelRegistry.sharedInstance();
    registry.register({
      modelId: 'meta-llama/Llama-3.1-8B-Instruct',
      contextWindow: 131072,
      provider: 'together',
      discoveredAt: Date.now()
    });

    expect(isSlowLocalModel('meta-llama/Llama-3.1-8B-Instruct', 'together')).toBe(false);
  });
});

describe('RAGBudgetManager', () => {
  describe('allocate', () => {
    it('should allocate minimums to all sources', () => {
      const manager = new RAGBudgetManager('gpt-4o');
      const sources: RAGSourceBudget[] = [
        { sourceId: 'conversation', priority: 10, minTokens: 1000, maxTokens: 5000 },
        { sourceId: 'memories', priority: 5, minTokens: 500, maxTokens: 2000 }
      ];

      const result = manager.allocate(sources, { system: 500, completion: 3000 });

      // All sources should get at least their minimum
      const convAlloc = result.allocations.find(a => a.sourceId === 'conversation');
      const memAlloc = result.allocations.find(a => a.sourceId === 'memories');

      expect(convAlloc?.allocatedTokens).toBeGreaterThanOrEqual(1000);
      expect(memAlloc?.allocatedTokens).toBeGreaterThanOrEqual(500);
    });

    it('should distribute extra tokens by priority', () => {
      const manager = new RAGBudgetManager('gpt-4'); // 8K context - smaller to see distribution
      const sources: RAGSourceBudget[] = [
        { sourceId: 'high', priority: 10, minTokens: 100, maxTokens: 10000 },
        { sourceId: 'low', priority: 1, minTokens: 100, maxTokens: 10000 }
      ];

      const result = manager.allocate(sources, { system: 500, completion: 1000 });

      const highAlloc = result.allocations.find(a => a.sourceId === 'high');
      const lowAlloc = result.allocations.find(a => a.sourceId === 'low');

      // Higher priority should get more tokens (10:1 ratio)
      // With 8192 - 500 - 1000 = 6692 available, after minimums (200), 6492 to distribute
      // High gets ~90% (10/11), low gets ~10% (1/11)
      expect(highAlloc!.allocatedTokens).toBeGreaterThan(lowAlloc!.allocatedTokens);
    });

    it('should respect maximum token limits', () => {
      const manager = new RAGBudgetManager('gpt-4o');
      const sources: RAGSourceBudget[] = [
        { sourceId: 'limited', priority: 10, minTokens: 100, maxTokens: 1000 }
      ];

      const result = manager.allocate(sources, { system: 500, completion: 3000 });

      const alloc = result.allocations.find(a => a.sourceId === 'limited');
      expect(alloc?.allocatedTokens).toBeLessThanOrEqual(1000);
    });

    it('should handle insufficient tokens gracefully', () => {
      const manager = new RAGBudgetManager('gpt-4'); // 8K context
      const sources: RAGSourceBudget[] = [
        { sourceId: 'big', priority: 10, minTokens: 5000, maxTokens: 10000 },
        { sourceId: 'bigger', priority: 10, minTokens: 5000, maxTokens: 10000 }
      ];

      // Reserve most of the context
      const result = manager.allocate(sources, { system: 500, completion: 3000 });

      // Should have warnings about insufficient tokens
      expect(result.warnings.length).toBeGreaterThan(0);
      // Should still allocate something proportionally
      expect(result.totalAllocated).toBeGreaterThan(0);
    });

    it('should calculate correct available tokens', () => {
      const manager = new RAGBudgetManager('gpt-4'); // 8192 context
      const sources: RAGSourceBudget[] = [
        { sourceId: 'test', priority: 10, minTokens: 0, maxTokens: 10000 }
      ];

      const result = manager.allocate(sources, { system: 500, completion: 3000 });

      // Available = 8192 - 500 - 3000 = 4692
      expect(result.availableForSources).toBe(4692);
    });
  });

  describe('getChatBudget', () => {
    it('should return budget with expected sources', () => {
      const budget = RAGBudgetManager.getChatBudget('gpt-4o');

      const sourceIds = budget.map(s => s.sourceId);
      expect(sourceIds).toContain('conversation');
      expect(sourceIds).toContain('memories');
    });

    it('should have higher min for large context models', () => {
      const largeBudget = RAGBudgetManager.getChatBudget('gpt-4o'); // 128K
      const smallBudget = RAGBudgetManager.getChatBudget('gpt-4'); // 8K

      const largeConv = largeBudget.find(s => s.sourceId === 'conversation');
      const smallConv = smallBudget.find(s => s.sourceId === 'conversation');

      expect(largeConv!.minTokens).toBeGreaterThan(smallConv!.minTokens);
    });
  });

  describe('allocateChatBudget', () => {
    it('should return valid allocation for any model', () => {
      const result = allocateChatBudget('gpt-4o');

      expect(result.modelId).toBe('gpt-4o');
      expect(result.allocations.length).toBeGreaterThan(0);
      expect(result.totalAllocated).toBeGreaterThan(0);
    });
  });
});
