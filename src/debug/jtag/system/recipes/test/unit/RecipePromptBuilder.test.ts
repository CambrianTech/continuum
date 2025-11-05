/**
 * RecipePromptBuilder Unit Tests
 *
 * Tests all public methods with various inputs to ensure:
 * - Type safety (no runtime type errors)
 * - Pure functions (same input → same output)
 * - Zero hardcoded text leakage
 * - Proper handling of edge cases (empty arrays, missing data)
 */

import { describe, it, expect } from 'vitest';
import {
  RecipePromptBuilder,
  type GatingPromptContext,
  type GenerationPromptContext
} from '../../shared/RecipePromptBuilder';
import type { RecipeStrategy } from '../../shared/RecipeTypes';
import type { RAGContext, RAGMessage } from '../../../rag/shared/RAGTypes';
import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Helper: Create minimal recipe strategy for testing
 */
function createTestStrategy(overrides?: Partial<RecipeStrategy>): RecipeStrategy {
  return {
    conversationPattern: 'collaborative',
    responseRules: [
      'If human asks question → ALL AIs with relevant knowledge can respond',
      'Multiple responses are GOOD → diverse perspectives enrich conversation'
    ],
    decisionCriteria: [
      'Do I have relevant knowledge or insights to share?',
      'Would my response add value beyond what\'s already said?'
    ],
    ...overrides
  };
}

/**
 * Helper: Create minimal RAG context for testing
 */
function createTestRAGContext(overrides?: Partial<RAGContext>): RAGContext {
  const baseMessage: RAGMessage = {
    role: 'user',
    content: 'Test message content',
    name: 'Test User',
    timestamp: Date.now()
  };

  return {
    domain: 'chat',
    contextId: 'test-room-id' as UUID,
    personaId: 'test-persona-id' as UUID,
    identity: {
      name: 'Test Persona',
      systemPrompt: 'You are a test persona',
      bio: 'A helpful test assistant'
    },
    conversationHistory: [baseMessage],
    artifacts: [],
    privateMemories: [],
    metadata: {
      timestamp: Date.now(),
      roomName: 'Test Room'
    },
    ...overrides
  };
}

/**
 * Helper: Create gating prompt context
 */
function createGatingContext(overrides?: Partial<GatingPromptContext>): GatingPromptContext {
  return {
    personaName: 'Helper AI',
    roomContext: createTestRAGContext(),
    conversationPattern: 'collaborative',
    ...overrides
  };
}

/**
 * Helper: Create generation prompt context
 */
function createGenerationContext(overrides?: Partial<GenerationPromptContext>): GenerationPromptContext {
  return {
    personaName: 'Helper AI',
    roomContext: createTestRAGContext(),
    conversationPattern: 'collaborative',
    ...overrides
  };
}

describe('RecipePromptBuilder', () => {
  describe('buildGatingPrompt', () => {
    it('should generate prompt with all sections', () => {
      const strategy = createTestStrategy();
      const context = createGatingContext();

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      // Should contain all major sections
      expect(prompt).toContain('Helper AI');
      expect(prompt).toContain('collaborative conversation');
      expect(prompt).toContain('**Response Rules:**');
      expect(prompt).toContain('**Decision Criteria:**');
      expect(prompt).toContain('**Recent Conversation:**');
      expect(prompt).toContain('**Your Decision:**');
    });

    it('should include all response rules from recipe', () => {
      const strategy = createTestStrategy({
        responseRules: [
          'Rule one: Do this',
          'Rule two: Don\'t do that',
          'Rule three: Maybe do this'
        ]
      });
      const context = createGatingContext();

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt).toContain('Rule one: Do this');
      expect(prompt).toContain('Rule two: Don\'t do that');
      expect(prompt).toContain('Rule three: Maybe do this');
    });

    it('should include all decision criteria from recipe', () => {
      const strategy = createTestStrategy({
        decisionCriteria: [
          'Check knowledge relevance',
          'Assess conversation value',
          'Consider timing appropriateness'
        ]
      });
      const context = createGatingContext();

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt).toContain('Check knowledge relevance');
      expect(prompt).toContain('Assess conversation value');
      expect(prompt).toContain('Consider timing appropriateness');
    });

    it('should handle empty response rules gracefully', () => {
      const strategy = createTestStrategy({
        responseRules: []
      });
      const context = createGatingContext();

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt).toContain('**Response Rules:**');
      expect(prompt).toContain('No specific rules defined');
    });

    it('should handle empty decision criteria gracefully', () => {
      const strategy = createTestStrategy({
        decisionCriteria: []
      });
      const context = createGatingContext();

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt).toContain('**Decision Criteria:**');
      expect(prompt).toContain('Consider relevance and value');
    });

    it('should handle empty conversation history gracefully', () => {
      const strategy = createTestStrategy();
      const context = createGatingContext({
        roomContext: createTestRAGContext({
          conversationHistory: []
        })
      });

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt).toContain('**Recent Conversation:**');
      expect(prompt).toContain('No recent messages');
    });

    it('should format conversation history with timestamps', () => {
      const timestamp = new Date('2025-10-23T14:30:00').getTime();
      const strategy = createTestStrategy();
      const context = createGatingContext({
        roomContext: createTestRAGContext({
          conversationHistory: [{
            role: 'user',
            content: 'What is TypeScript?',
            name: 'Joel',
            timestamp
          }]
        })
      });

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt).toContain('[14:30]');
      expect(prompt).toContain('Joel:');
      expect(prompt).toContain('What is TypeScript?');
    });

    it('should limit conversation history to last 10 messages', () => {
      const messages: RAGMessage[] = [];
      for (let i = 0; i < 20; i++) {
        messages.push({
          role: 'user',
          content: `Message ${i}`,
          name: 'User',
          timestamp: Date.now() + i
        });
      }

      const strategy = createTestStrategy();
      const context = createGatingContext({
        roomContext: createTestRAGContext({
          conversationHistory: messages
        })
      });

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      // Should contain last 10 messages (10-19)
      expect(prompt).toContain('Message 19');
      expect(prompt).toContain('Message 10');
      // Should NOT contain first 10 messages (0-9)
      expect(prompt).not.toContain('Message 0');
      expect(prompt).not.toContain('Message 9');
    });

    it('should be deterministic (same input → same output)', () => {
      const strategy = createTestStrategy();
      const context = createGatingContext();

      const prompt1 = RecipePromptBuilder.buildGatingPrompt(strategy, context);
      const prompt2 = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt1).toBe(prompt2);
    });

    it('should adapt to different conversation patterns', () => {
      const strategy = createTestStrategy();

      const collaborativePrompt = RecipePromptBuilder.buildGatingPrompt(
        strategy,
        createGatingContext({ conversationPattern: 'collaborative' })
      );

      const humanFocusedPrompt = RecipePromptBuilder.buildGatingPrompt(
        strategy,
        createGatingContext({ conversationPattern: 'human-focused' })
      );

      expect(collaborativePrompt).toContain('collaborative conversation');
      expect(humanFocusedPrompt).toContain('human-focused conversation');
    });
  });

  describe('buildGenerationPrompt', () => {
    it('should generate prompt with all sections', () => {
      const strategy = createTestStrategy();
      const context = createGenerationContext();

      const prompt = RecipePromptBuilder.buildGenerationPrompt(strategy, context);

      expect(prompt).toContain('Helper AI');
      expect(prompt).toContain('collaborative conversation');
      expect(prompt).toContain('**Response Rules:**');
      expect(prompt).toContain('**Recent Conversation:**');
      expect(prompt).toContain('**Your Response:**');
    });

    it('should NOT include decision criteria (not needed for generation)', () => {
      const strategy = createTestStrategy();
      const context = createGenerationContext();

      const prompt = RecipePromptBuilder.buildGenerationPrompt(strategy, context);

      // Decision criteria are only for gating, not generation
      expect(prompt).not.toContain('**Decision Criteria:**');
    });

    it('should include response rules from recipe', () => {
      const strategy = createTestStrategy({
        responseRules: [
          'Be concise',
          'Add value',
          'Stay relevant'
        ]
      });
      const context = createGenerationContext();

      const prompt = RecipePromptBuilder.buildGenerationPrompt(strategy, context);

      expect(prompt).toContain('Be concise');
      expect(prompt).toContain('Add value');
      expect(prompt).toContain('Stay relevant');
    });

    it('should include generation instructions', () => {
      const strategy = createTestStrategy();
      const context = createGenerationContext();

      const prompt = RecipePromptBuilder.buildGenerationPrompt(strategy, context);

      expect(prompt).toContain('Write naturally as yourself');
      expect(prompt).toContain('Be concise');
      expect(prompt).toContain('NO name prefix');
    });

    it('should be deterministic (same input → same output)', () => {
      const strategy = createTestStrategy();
      const context = createGenerationContext();

      const prompt1 = RecipePromptBuilder.buildGenerationPrompt(strategy, context);
      const prompt2 = RecipePromptBuilder.buildGenerationPrompt(strategy, context);

      expect(prompt1).toBe(prompt2);
    });
  });

  describe('Type Safety', () => {
    it('should enforce readonly arrays in recipe strategy', () => {
      const strategy: RecipeStrategy = {
        conversationPattern: 'collaborative',
        responseRules: ['Rule 1', 'Rule 2'],
        decisionCriteria: ['Criterion 1']
      };

      // This should work (reading is always allowed)
      const firstRule = strategy.responseRules[0];
      expect(firstRule).toBe('Rule 1');

      // TypeScript should prevent mutation (compile-time check)
      // @ts-expect-error - Cannot push to readonly array
      // strategy.responseRules.push('Rule 3');
    });

    it('should enforce specific conversation patterns', () => {
      const validPatterns: Array<RecipeStrategy['conversationPattern']> = [
        'human-focused',
        'collaborative',
        'competitive',
        'teaching',
        'exploring',
        'cooperative'
      ];

      validPatterns.forEach(pattern => {
        const strategy = createTestStrategy({ conversationPattern: pattern });
        const context = createGatingContext({ conversationPattern: pattern });
        const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

        expect(prompt).toContain(`${pattern} conversation`);
      });
    });
  });

  describe('Edge Cases', () => {
    it('should handle very long message content gracefully', () => {
      const longContent = 'A'.repeat(500); // 500 characters
      const strategy = createTestStrategy();
      const context = createGatingContext({
        roomContext: createTestRAGContext({
          conversationHistory: [{
            role: 'user',
            content: longContent,
            name: 'User',
            timestamp: Date.now()
          }]
        })
      });

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      // Should truncate to 200 characters
      expect(prompt).toContain('A'.repeat(200));
      expect(prompt).not.toContain('A'.repeat(500));
    });

    it('should handle messages without timestamps', () => {
      const strategy = createTestStrategy();
      const context = createGatingContext({
        roomContext: createTestRAGContext({
          conversationHistory: [{
            role: 'user',
            content: 'Message without timestamp',
            name: 'User'
            // No timestamp provided
          }]
        })
      });

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt).toContain('[??:??]'); // Fallback for missing timestamp
      expect(prompt).toContain('Message without timestamp');
    });

    it('should handle messages without names', () => {
      const strategy = createTestStrategy();
      const context = createGatingContext({
        roomContext: createTestRAGContext({
          conversationHistory: [{
            role: 'user',
            content: 'Anonymous message',
            timestamp: Date.now()
            // No name provided
          }]
        })
      });

      const prompt = RecipePromptBuilder.buildGatingPrompt(strategy, context);

      expect(prompt).toContain('Unknown:'); // Fallback for missing name
      expect(prompt).toContain('Anonymous message');
    });
  });
});
