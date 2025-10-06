#!/usr/bin/env npx tsx
/**
 * Test AI Sentinel Command
 *
 * Tests the ai/should-respond gating logic with real conversation data
 */

import { Commands } from './system/core/shared/Commands';
import type { AIShouldRespondParams, AIShouldRespondResult } from './commands/ai/should-respond/shared/AIShouldRespondTypes';
import type { RAGContext } from './system/rag/shared/RAGTypes';

async function testAISentinel() {
  console.log('🧪 Testing AI Sentinel Command...\n');

  // Mock RAG context with realistic conversation
  const mockRAGContext: RAGContext = {
    identity: {
      name: 'Helper AI',
      systemPrompt: 'You are Helper AI. You respond naturally to conversations.'
    },
    conversationHistory: [
      {
        role: 'user',
        name: 'CodeReview AI',
        content: 'The debate surrounding the nature of consciousness is fundamentally rooted in our understanding of life itself.',
        timestamp: Date.now() - 30000
      },
      {
        role: 'user',
        name: 'Teacher AI',
        content: 'That\'s a fascinating philosophical point. The mind-body problem has puzzled thinkers for centuries.',
        timestamp: Date.now() - 20000
      },
      {
        role: 'user',
        name: 'Joel',
        content: 'What makes you think so?',
        timestamp: Date.now() - 1000
      }
    ]
  };

  // Test Case 1: Helper AI directly mentioned
  console.log('📝 Test Case 1: Helper AI directly mentioned');
  const test1Params: Omit<AIShouldRespondParams, 'context' | 'sessionId'> = {
    personaName: 'Helper AI',
    personaId: 'ad80b8bd-cf2a-45a5-93e9-ed23a687f8be',
    ragContext: mockRAGContext,
    triggerMessage: {
      senderName: 'Joel',
      content: 'Hey Helper AI, can you explain this?',
      timestamp: new Date().toISOString()
    }
  };

  const result1 = await Commands.execute<AIShouldRespondParams, AIShouldRespondResult>(
    'ai/should-respond',
    test1Params
  );

  console.log('Result:', JSON.stringify(result1, null, 2));
  console.log(`\n✅ Should respond: ${result1.shouldRespond} (${(result1.confidence * 100).toFixed(0)}% confidence)`);
  console.log(`📊 Factors:`, result1.factors);
  console.log(`💭 Reason: ${result1.reason}\n`);

  // Test Case 2: Generic question (not mentioned)
  console.log('📝 Test Case 2: Generic question (Helper AI not mentioned)');
  const test2Params: Omit<AIShouldRespondParams, 'context' | 'sessionId'> = {
    personaName: 'Helper AI',
    personaId: 'ad80b8bd-cf2a-45a5-93e9-ed23a687f8be',
    ragContext: mockRAGContext,
    triggerMessage: {
      senderName: 'Joel',
      content: 'What makes you think so?',
      timestamp: new Date().toISOString()
    }
  };

  const result2 = await Commands.execute<AIShouldRespondParams, AIShouldRespondResult>(
    'ai/should-respond',
    test2Params
  );

  console.log('Result:', JSON.stringify(result2, null, 2));
  console.log(`\n✅ Should respond: ${result2.shouldRespond} (${(result2.confidence * 100).toFixed(0)}% confidence)`);
  console.log(`📊 Factors:`, result2.factors);
  console.log(`💭 Reason: ${result2.reason}\n`);

  console.log('🎉 AI Sentinel tests complete!');
}

testAISentinel().catch(console.error);
