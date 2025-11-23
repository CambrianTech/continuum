/**
 * Unit tests for SemanticCompressionAdapter
 *
 * Tests LLM-based synthesis of related thoughts into compressed insights
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { SemanticCompressionAdapter } from '../../../system/user/server/modules/cognitive/memory/adapters/SemanticCompressionAdapter';
import type { WorkingMemoryEntry } from '../../../system/user/server/modules/cognition/memory/InMemoryCognitionStorage';
import type { ConsolidationContext } from '../../../system/user/server/modules/cognitive/memory/adapters/MemoryConsolidationAdapter';
import { MemoryType } from '../../../system/user/server/modules/MemoryTypes';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import { ISOString } from '../../../data/domains/CoreTypes';

// Mock AIProviderDaemon
vi.mock('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon', () => ({
  AIProviderDaemon: {
    generateText: vi.fn()
  }
}));

describe('SemanticCompressionAdapter', () => {
  let adapter: SemanticCompressionAdapter;
  let mockContext: ConsolidationContext;

  beforeEach(() => {
    adapter = new SemanticCompressionAdapter({
      synthesisModel: 'llama3.2:3b',
      maxThoughtsPerGroup: 10
    });

    mockContext = {
      personaId: generateUUID(),
      personaName: 'TestPersona',
      sessionId: generateUUID(),
      timestamp: new Date()
    };

    // Reset mocks
    vi.clearAllMocks();
  });

  describe('getName()', () => {
    it('should return adapter name', () => {
      expect(adapter.getName()).toBe('SemanticCompressionAdapter');
    });
  });

  describe('doesSynthesis()', () => {
    it('should return true (performs synthesis)', () => {
      expect(adapter.doesSynthesis()).toBe(true);
    });
  });

  describe('consolidate()', () => {
    it('should return empty result for empty thoughts array', async () => {
      const result = await adapter.consolidate([], mockContext);

      expect(result.memories).toEqual([]);
      expect(result.metadata?.synthesisCount).toBe(0);
      expect(result.metadata?.groupsCreated).toBe(0);
    });

    it('should group thoughts by contextId and domain', async () => {
      const { AIProviderDaemon } = await import('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon');

      // Mock LLM response
      (AIProviderDaemon.generateText as any).mockResolvedValue({
        text: 'User prefers concrete examples over abstract explanations'
      });

      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'User asked for code example',
          importance: 0.7,
          domain: 'teaching',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Abstract explanation failed',
          importance: 0.6,
          domain: 'teaching',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'pattern',
          thoughtContent: 'Code examples work better',
          importance: 0.8,
          domain: 'teaching',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      // Should create 1 group (same contextId + domain)
      expect(result.metadata?.groupsCreated).toBe(1);
      expect(result.metadata?.synthesisCount).toBe(1);

      // Should produce 1 synthesized memory
      expect(result.memories).toHaveLength(1);
      expect(result.memories[0].content).toBe('User prefers concrete examples over abstract explanations');
      expect(result.memories[0].type).toBe(MemoryType.OBSERVATION); // Most common type
      expect(result.memories[0].source).toBe('semantic-compression');
    });

    it('should create multiple groups for different contexts/domains', async () => {
      const { AIProviderDaemon } = await import('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon');

      // Mock LLM to return different synthesis for each group
      (AIProviderDaemon.generateText as any)
        .mockResolvedValueOnce({ text: 'Teaching insight' })
        .mockResolvedValueOnce({ text: 'Coding insight' });

      const thoughts: WorkingMemoryEntry[] = [
        // Group 1: room-123 + teaching
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Teaching thought 1',
          importance: 0.7,
          domain: 'teaching',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Teaching thought 2',
          importance: 0.6,
          domain: 'teaching',
          contextId: 'room-123',
          shareable: false
        },
        // Group 2: room-456 + coding
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'decision',
          thoughtContent: 'Coding decision 1',
          importance: 0.8,
          domain: 'coding',
          contextId: 'room-456',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      expect(result.metadata?.groupsCreated).toBe(2);
      expect(result.metadata?.synthesisCount).toBe(2);
      expect(result.memories).toHaveLength(2);

      // First memory should be from teaching group
      expect(result.memories[0].content).toBe('Teaching insight');
      expect(result.memories[0].context.domain).toBe('teaching');

      // Second memory should be from coding group
      expect(result.memories[1].content).toBe('Coding insight');
      expect(result.memories[1].context.domain).toBe('coding');
    });

    it('should calculate average importance for groups', async () => {
      const { AIProviderDaemon } = await import('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon');

      (AIProviderDaemon.generateText as any).mockResolvedValue({
        text: 'Synthesized insight'
      });

      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Thought 1',
          importance: 0.6,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Thought 2',
          importance: 0.8,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      // Average importance should be (0.6 + 0.8) / 2 = 0.7
      expect(result.memories[0].importance).toBe(0.7);
    });

    it('should infer memory type from dominant thought type', async () => {
      const { AIProviderDaemon } = await import('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon');

      (AIProviderDaemon.generateText as any).mockResolvedValue({
        text: 'Decision insight'
      });

      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'decision',
          thoughtContent: 'Decision 1',
          importance: 0.7,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'decision',
          thoughtContent: 'Decision 2',
          importance: 0.8,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Observation',
          importance: 0.6,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      // Should infer DECISION type (2 decisions vs 1 observation)
      expect(result.memories[0].type).toBe(MemoryType.DECISION);
    });

    it('should create fallback memory when synthesis fails', async () => {
      const { AIProviderDaemon } = await import('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon');

      // Mock LLM to throw error
      (AIProviderDaemon.generateText as any).mockRejectedValue(
        new Error('LLM service unavailable')
      );

      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Most important thought',
          importance: 0.9,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Less important thought',
          importance: 0.5,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      // Should still create 1 memory (fallback)
      expect(result.memories).toHaveLength(1);

      // Should use most important thought as fallback
      expect(result.memories[0].content).toBe('Most important thought');
      expect(result.memories[0].importance).toBe(0.9);
      expect(result.memories[0].source).toBe('working-memory-fallback');
      expect(result.memories[0].tags).toContain('fallback');
    });

    it('should pass correct synthesis prompt to LLM', async () => {
      const { AIProviderDaemon } = await import('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon');

      (AIProviderDaemon.generateText as any).mockResolvedValue({
        text: 'Synthesized insight'
      });

      const thoughts: WorkingMemoryEntry[] = [
        {
          id: generateUUID(),
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Test thought',
          importance: 0.7,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      await adapter.consolidate(thoughts, mockContext);

      // Verify LLM was called with correct parameters
      expect(AIProviderDaemon.generateText).toHaveBeenCalledWith({
        messages: expect.arrayContaining([
          expect.objectContaining({
            role: 'user',
            content: expect.stringContaining('You are TestPersona')
          })
        ]),
        model: 'llama3.2:3b',
        temperature: 0.3,
        maxTokens: 200,
        preferredProvider: 'ollama'
      });

      // Verify prompt includes thought content
      const callArgs = (AIProviderDaemon.generateText as any).mock.calls[0][0];
      expect(callArgs.messages[0].content).toContain('Test thought');
      expect(callArgs.messages[0].content).toContain('Extract the KEY PATTERN');
    });

    it('should track original thought IDs in synthesizedFrom', async () => {
      const { AIProviderDaemon } = await import('../../../daemons/ai-provider-daemon/shared/AIProviderDaemon');

      (AIProviderDaemon.generateText as any).mockResolvedValue({
        text: 'Synthesized insight'
      });

      const thoughtId1 = generateUUID();
      const thoughtId2 = generateUUID();

      const thoughts: WorkingMemoryEntry[] = [
        {
          id: thoughtId1,
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Thought 1',
          importance: 0.7,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        },
        {
          id: thoughtId2,
          createdAt: Date.now(),
          thoughtType: 'observation',
          thoughtContent: 'Thought 2',
          importance: 0.6,
          domain: 'test',
          contextId: 'room-123',
          shareable: false
        }
      ];

      const result = await adapter.consolidate(thoughts, mockContext);

      // Should track original thought IDs
      expect(result.memories[0].context.synthesizedFrom).toEqual([thoughtId1, thoughtId2]);
      expect(result.memories[0].context.thoughtCount).toBe(2);
    });
  });
});
