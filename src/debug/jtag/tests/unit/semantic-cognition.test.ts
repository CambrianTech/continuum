/**
 * Semantic Cognition Unit Tests
 *
 * Tests for the semantic cognition infrastructure:
 * - IEmbeddable interface utilities
 * - ModelContextWindows configuration
 * - RAGBudgetManager allocation
 */

import { describe, it, expect } from 'vitest';
import { isEmbeddable, needsEmbedding, type IEmbeddable } from '../../system/data/interfaces/IEmbeddable';
import {
  getContextWindow,
  isLargeContextModel,
  getRecommendedMaxOutputTokens,
  MODEL_CONTEXT_WINDOWS,
  DEFAULT_CONTEXT_WINDOW
} from '../../system/shared/ModelContextWindows';
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
        expect(size).toBeLessThanOrEqual(1000000); // Max 1M tokens
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
