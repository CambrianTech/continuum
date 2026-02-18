/**
 * Bag-of-Words Response Detection Integration Tests
 *
 * Tests the ai/should-respond-fast command which provides fast, deterministic
 * response detection using keyword matching, question detection, and scoring.
 * This is the first-pass filter before expensive LLM calls.
 *
 * Server-side only - no browser/widget interaction needed.
 * Uses existing seed data (Teacher AI persona, general room).
 */

import { describe, test, expect } from 'vitest';
import { execSync } from 'child_process';
import type { ShouldRespondFastResult } from '../../commands/ai/should-respond-fast/shared/ShouldRespondFastTypes';

// Use existing test persona from seed data
const TEST_PERSONA_ID = 'cf3f6d30-1177-4d6f-8033-2b2eb83c6d1c'; // Teacher AI from seed
const TEST_ROOM_ID = '5e71a0c8-0303-4eb8-a478-3a121248'; // general room from seed

/**
 * Execute BOW command via CLI
 */
function executeBOWCommand(
  messageText: string,
  config?: {
    personaName?: string;
    domainKeywords?: string[];
    responseThreshold?: number;
    cooldownSeconds?: number;
    alwaysRespondToMentions?: boolean;
    weights?: Record<string, number>;
  }
): ShouldRespondFastResult {
  const configJson = JSON.stringify(config ?? {});
  const escapedMessage = messageText.replace(/'/g, "'\\''");
  const escapedConfig = configJson.replace(/'/g, "'\\''");

  const cmd = `./jtag ai/should-respond-fast --personaId="${TEST_PERSONA_ID}" --contextId="${TEST_ROOM_ID}" --messageText='${escapedMessage}' --config='${escapedConfig}'`;

  try {
    const output = execSync(cmd, {
      encoding: 'utf8',
      timeout: 10000,
      shell: true,
      maxBuffer: 1024 * 1024
    });

    return JSON.parse(output);
  } catch (error) {
    console.error('BOW command failed:', error instanceof Error ? error.message : error);
    throw error;
  }
}

describe('Bag-of-Words Response Detection - Core Functionality', () => {
  test('direct mention should score high and trigger response', () => {
    const result = executeBOWCommand(
      '@Teacher AI can you explain TypeScript generics?',
      {
        personaName: 'Teacher AI',
        domainKeywords: ['typescript', 'javascript', 'programming', 'code'],
        responseThreshold: 50,
        cooldownSeconds: 0
      }
    );

    expect(result.success).toBe(true);
    expect(result.shouldRespond).toBe(true);
    expect(result.score).toBeGreaterThanOrEqual(100); // At least mention=100

    // Verify score breakdown
    expect(result.scoreBreakdown.directMention).toBe(100);
    expect(result.scoreBreakdown.domainKeywords).toBeGreaterThan(0); // "typescript"
    expect(result.scoreBreakdown.isQuestion).toBe(20); // Has "?"

    // Verify signals
    expect(result.signals.wasMentioned).toBe(true);
    expect(result.signals.matchedKeywords).toContain('typescript');
    expect(result.signals.isQuestion).toBe(true);
  });

  test('keyword match without mention should trigger response if above threshold', () => {
    const result = executeBOWCommand(
      'Can someone help me debug this JavaScript promise issue?',
      {
        personaName: 'Teacher AI',
        domainKeywords: ['javascript', 'typescript', 'debugging', 'promise'],
        responseThreshold: 50,
        cooldownSeconds: 0
      }
    );

    expect(result.success).toBe(true);
    expect(result.shouldRespond).toBe(true);
    expect(result.score).toBeGreaterThanOrEqual(50);

    // Should have keyword matches
    expect(result.scoreBreakdown.domainKeywords).toBeGreaterThan(0);
    expect(result.signals.matchedKeywords.length).toBeGreaterThan(0);
  });

  test('irrelevant message should not trigger response', () => {
    const result = executeBOWCommand(
      "What's the weather like today?",
      {
        personaName: 'Teacher AI',
        domainKeywords: ['typescript', 'javascript', 'programming', 'code'],
        responseThreshold: 50,
        cooldownSeconds: 0
      }
    );

    expect(result.success).toBe(true);
    expect(result.shouldRespond).toBe(false);
    expect(result.score).toBeLessThan(50);

    // Should not have matched keywords
    expect(result.scoreBreakdown.domainKeywords).toBe(0);
    expect(result.signals.matchedKeywords).toHaveLength(0);
    expect(result.signals.wasMentioned).toBe(false);
  });

  test('alwaysRespondToMentions should override low score', () => {
    const result = executeBOWCommand(
      '@Teacher AI', // Just mention, no keywords
      {
        personaName: 'Teacher AI',
        domainKeywords: ['completely-unrelated-keyword'],
        responseThreshold: 150, // Very high threshold
        alwaysRespondToMentions: true,
        cooldownSeconds: 0
      }
    );

    expect(result.success).toBe(true);
    expect(result.shouldRespond).toBe(true);
    expect(result.signals.wasMentioned).toBe(true);
    expect(result.score).toBeLessThan(150); // Score below threshold
    // But still responds due to alwaysRespondToMentions
  });

  test('question detection should add score bonus', { timeout: 15000 }, () => {
    const questionTests = [
      'How do I use TypeScript?',
      'What are the benefits of strict typing?',
      'Why should I learn TypeScript?',
      'Can you help me with this?',
      'Is TypeScript better than JavaScript?'
    ];

    for (const questionText of questionTests) {
      const result = executeBOWCommand(
        questionText,
        {
          personaName: 'Teacher AI',
          domainKeywords: ['typescript', 'javascript'],
          cooldownSeconds: 0
        }
      );

      expect(result.success).toBe(true);
      expect(result.signals.isQuestion).toBe(true);
      expect(result.scoreBreakdown.isQuestion).toBe(20);
    }
  });

  test('threshold configuration should control response decision', () => {
    const messageText = 'typescript'; // Just keyword, no mention

    // Low threshold - should respond
    const result1 = executeBOWCommand(
      messageText,
      {
        personaName: 'Teacher AI',
        domainKeywords: ['typescript'],
        responseThreshold: 30,
        cooldownSeconds: 0
      }
    );
    expect(result1.shouldRespond).toBe(true);

    // High threshold - should not respond
    const result2 = executeBOWCommand(
      messageText,
      {
        personaName: 'Teacher AI',
        domainKeywords: ['typescript'],
        responseThreshold: 200,
        cooldownSeconds: 0
      }
    );
    expect(result2.shouldRespond).toBe(false);
  });
});

describe('Bag-of-Words Response Detection - Edge Cases', () => {
  test('empty message should not crash', () => {
    const result = executeBOWCommand(
      '',
      {
        personaName: 'Teacher AI',
        domainKeywords: ['test'],
        cooldownSeconds: 0
      }
    );

    expect(result.success).toBe(true);
    expect(result.shouldRespond).toBe(false);
  });

  test('very long message should process correctly', () => {
    const longMessage = 'typescript '.repeat(100) + '?'; // 100 keyword mentions

    const result = executeBOWCommand(
      longMessage,
      {
        personaName: 'Teacher AI',
        domainKeywords: ['typescript'],
        cooldownSeconds: 0
      }
    );

    expect(result.success).toBe(true);
    expect(result.scoreBreakdown.domainKeywords).toBeGreaterThan(0);
  });

  test('special characters should not break parsing', { timeout: 15000 }, () => {
    const specialMessages = [
      '@Teacher AI test <script>alert("xss")</script>',
      'Teacher AI: {json: "test"}',
      '@Teacher AI test with newlines',
      'Teacher AI emoji test 😀'
    ];

    for (const messageText of specialMessages) {
      const result = executeBOWCommand(
        messageText,
        {
          personaName: 'Teacher AI',
          domainKeywords: ['test'],
          cooldownSeconds: 0
        }
      );

      expect(result.success).toBe(true);
      // Should not crash, even if detection varies
    }
  });

  test('missing config should use defaults', () => {
    const result = executeBOWCommand(
      '@Teacher AI test'
      // No config provided
    );

    expect(result.success).toBe(true);
    // Should use default config values
  });

  test('persona name variations should be detected', { timeout: 15000 }, () => {
    const mentionVariations = [
      '@Teacher AI explain this',
      'Teacher AI can you help?',
      'hey teacher ai, quick question'
    ];

    for (const messageText of mentionVariations) {
      const result = executeBOWCommand(
        messageText,
        {
          personaName: 'Teacher AI',
          domainKeywords: [],
          cooldownSeconds: 0
        }
      );

      expect(result.success).toBe(true);
      // Should detect mention (case insensitive)
      expect(result.signals.wasMentioned).toBe(true);
    }
  });

  test('custom scoring weights should affect results', () => {
    const result = executeBOWCommand(
      'TypeScript question',
      {
        personaName: 'Teacher AI',
        domainKeywords: ['typescript'],
        weights: {
          directMention: 100,
          domainKeyword: 200, // Boosted keyword weight
          conversationContext: 30,
          isQuestion: 20,
          publicMessage: 10,
          roomActivity: 5
        },
        cooldownSeconds: 0
      }
    );

    expect(result.success).toBe(true);
    // With boosted keyword weight, should have high score
    expect(result.scoreBreakdown.domainKeywords).toBeGreaterThanOrEqual(200);
  });
});

describe('Bag-of-Words Response Detection - Cooldown Behavior', () => {
  test('cooldown should prevent rapid responses', () => {
    // Use a unique persona+context combo to avoid cooldown from other tests
    const uniquePersonaId = 'cooldown-test-persona-' + Date.now();
    const uniqueContextId = 'cooldown-test-context-' + Date.now();

    // Helper function for cooldown test with unique IDs
    function executeCooldownTest(messageText: string, config: Parameters<typeof executeBOWCommand>[1]) {
      const configJson = JSON.stringify(config ?? {});
      const escapedMessage = messageText.replace(/'/g, "'\\''");
      const escapedConfig = configJson.replace(/'/g, "'\\''");

      const cmd = `./jtag ai/should-respond-fast --personaId="${uniquePersonaId}" --contextId="${uniqueContextId}" --messageText='${escapedMessage}' --config='${escapedConfig}'`;

      const output = execSync(cmd, {
        encoding: 'utf8',
        timeout: 10000,
        shell: true,
        maxBuffer: 1024 * 1024
      });

      return JSON.parse(output);
    }

    // First call - should respond
    const result1 = executeCooldownTest(
      '@Teacher AI first question',
      {
        personaName: 'Teacher AI',
        domainKeywords: ['typescript'],
        cooldownSeconds: 60
      }
    );

    expect(result1.success).toBe(true);
    expect(result1.shouldRespond).toBe(true);

    // Second call immediately after - should be blocked by cooldown
    const result2 = executeCooldownTest(
      '@Teacher AI second question',
      {
        personaName: 'Teacher AI',
        domainKeywords: ['typescript'],
        cooldownSeconds: 60
      }
    );

    expect(result2.success).toBe(true);
    expect(result2.shouldRespond).toBe(false);
    expect(result2.reasoning).toContain('Cooldown');
  });
});
