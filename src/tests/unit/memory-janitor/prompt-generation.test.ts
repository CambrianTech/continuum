/**
 * Unit tests for MemoryJanitorDaemon LLM prompt generation
 *
 * Tests the Pass 2 prompt building logic
 */

import { describe, it, expect } from 'vitest';

interface WorkingMemoryEntity {
  id: string;
  personaId: string;
  content: string;
  timestamp: Date;
  domain: 'chat' | 'code' | 'academy' | 'self';
  ephemeral: boolean;
  consolidated: boolean;
  importance: number;
}

interface PersonaUser {
  id: string;
  displayName: string;
}

/**
 * Build LLM consolidation prompt (extracted for testing)
 */
function buildConsolidationPrompt(persona: PersonaUser, items: WorkingMemoryEntity[]): string {
  return `
You are consolidating working memory for ${persona.displayName}.

Review these ${items.length} memory items and for EACH item:
1. Classify as "ephemeral" (safe to delete) or "insight" (preserve as knowledge)
2. If insight: Generate 1-2 sentence summary preserving key information
3. If insight: Extract 3-5 semantic tags

Classification rules:
- Ephemeral: Routine chatter, greetings, status updates, redundant information
- Insight: New knowledge, user preferences, important decisions, technical learnings

Items:
${items.map((item, i) => `
[${i}] (importance: ${item.importance}, domain: ${item.domain})
${item.content.slice(0, 500)}
`).join('\n')}

Return JSON (MUST be valid JSON, no markdown):
{
  "items": [
    {
      "index": 0,
      "type": "ephemeral",
      "reason": "Routine greeting with no new information"
    },
    {
      "index": 1,
      "type": "insight",
      "summary": "User prefers TypeScript over JavaScript for type safety in large codebases",
      "tags": ["typescript", "type-safety", "preferences"],
      "confidence": 0.9
    }
  ]
}
`.trim();
}

describe('MemoryJanitorDaemon - Prompt Generation', () => {
  const mockPersona: PersonaUser = {
    id: 'persona-1',
    displayName: 'Helper AI'
  };

  describe('Basic prompt structure', () => {
    it('should include persona name in prompt', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Test message',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      expect(prompt).toContain('Helper AI');
      expect(prompt).toContain('consolidating working memory');
    });

    it('should include item count in prompt', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Test message 1',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        },
        {
          id: '2',
          personaId: 'persona-1',
          content: 'Test message 2',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.6
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      expect(prompt).toContain('Review these 2 memory items');
    });

    it('should include classification rules', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Test',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      expect(prompt).toContain('Classification rules:');
      expect(prompt).toContain('Ephemeral: Routine chatter');
      expect(prompt).toContain('Insight: New knowledge');
    });

    it('should include JSON example', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Test',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      expect(prompt).toContain('Return JSON');
      expect(prompt).toContain('"type": "ephemeral"');
      expect(prompt).toContain('"type": "insight"');
      expect(prompt).toContain('"summary":');
      expect(prompt).toContain('"tags":');
    });
  });

  describe('Item formatting', () => {
    it('should format items with index, importance, and domain', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'User prefers dark mode',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.8
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      expect(prompt).toContain('[0]');
      expect(prompt).toContain('importance: 0.8');
      expect(prompt).toContain('domain: chat');
      expect(prompt).toContain('User prefers dark mode');
    });

    it('should truncate long content to 500 chars', () => {
      const longContent = 'A'.repeat(1000); // 1000 chars

      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: longContent,
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      // Should contain truncated version (500 chars)
      expect(prompt).toContain('A'.repeat(500));
      // Should NOT contain full 1000 chars
      expect(prompt).not.toContain('A'.repeat(501));
    });

    it('should format multiple items with correct indices', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'First item',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        },
        {
          id: '2',
          personaId: 'persona-1',
          content: 'Second item',
          timestamp: new Date(),
          domain: 'code',
          ephemeral: false,
          consolidated: false,
          importance: 0.7
        },
        {
          id: '3',
          personaId: 'persona-1',
          content: 'Third item',
          timestamp: new Date(),
          domain: 'self',
          ephemeral: false,
          consolidated: false,
          importance: 0.9
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      // Check indices
      expect(prompt).toContain('[0]');
      expect(prompt).toContain('[1]');
      expect(prompt).toContain('[2]');

      // Check content
      expect(prompt).toContain('First item');
      expect(prompt).toContain('Second item');
      expect(prompt).toContain('Third item');

      // Check domains
      expect(prompt).toContain('domain: chat');
      expect(prompt).toContain('domain: code');
      expect(prompt).toContain('domain: self');
    });
  });

  describe('Prompt quality checks', () => {
    it('should provide clear instructions for LLM', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Test',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      // Check for clear task description
      expect(prompt).toContain('Classify as');
      expect(prompt).toContain('Generate');
      expect(prompt).toContain('Extract');

      // Check for examples
      expect(prompt).toContain('ephemeral');
      expect(prompt).toContain('insight');
    });

    it('should emphasize JSON format requirement', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Test',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      expect(prompt).toContain('MUST be valid JSON');
      expect(prompt).toContain('no markdown');
    });

    it('should be a reasonable token count for llama3.2:3b', () => {
      // Test with realistic batch size (15 items)
      const items: WorkingMemoryEntity[] = [];
      for (let i = 0; i < 15; i++) {
        items.push({
          id: `${i}`,
          personaId: 'persona-1',
          content: 'User mentioned they prefer TypeScript for its strong typing and IDE support. This helps catch errors early.',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5 + (i * 0.03)
        });
      }

      const prompt = buildConsolidationPrompt(mockPersona, items);

      // Rough token estimation: ~4 chars per token
      const estimatedTokens = prompt.length / 4;

      // llama3.2:3b context window is 8192 tokens
      // Prompt should be < 2000 tokens to leave room for response
      expect(estimatedTokens).toBeLessThan(2000);

      // Should be substantial enough to provide good context
      expect(estimatedTokens).toBeGreaterThan(500);
    });
  });

  describe('Edge cases', () => {
    it('should handle single item', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Single item',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      expect(prompt).toContain('Review these 1 memory items');
      expect(prompt).toContain('[0]');
      expect(prompt).toContain('Single item');
    });

    it('should handle items with special characters', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'User said: "TypeScript > JavaScript" & loves ${template} strings',
          timestamp: new Date(),
          domain: 'code',
          ephemeral: false,
          consolidated: false,
          importance: 0.7
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      // Special characters should be preserved
      expect(prompt).toContain('"TypeScript > JavaScript"');
      expect(prompt).toContain('${template}');
      expect(prompt).toContain('&');
    });

    it('should handle items with newlines', () => {
      const items: WorkingMemoryEntity[] = [
        {
          id: '1',
          personaId: 'persona-1',
          content: 'Line 1\nLine 2\nLine 3',
          timestamp: new Date(),
          domain: 'chat',
          ephemeral: false,
          consolidated: false,
          importance: 0.5
        }
      ];

      const prompt = buildConsolidationPrompt(mockPersona, items);

      // Newlines should be preserved
      expect(prompt).toContain('Line 1\nLine 2\nLine 3');
    });
  });
});
