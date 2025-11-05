/**
 * Ollama Request Queue & RAG Quality Integration Tests
 * ====================================================
 *
 * Tests three critical production fixes:
 *
 * 1. Ollama Request Queue - Concurrency management prevents timeout cascades
 * 2. RAG Role Assignment - Correct 'user'/'assistant' roles by sender type
 * 3. RAG Content Quality - Clean message text without [QUESTION] markers
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';

// Seed data constants
const GENERAL_ROOM_ID = '5e71a0c8-0303-4eb8-a478-3a121248';
const HELPER_PERSONA_ID = 'd3bc6b75-e731-4cfc-83e3-d2bab1e86a9c';
const PRIMARY_USER_ID = 'd7e1d6dd-7a2d-4ea5-bf63-6e0c7e1dde72'; // Primary human user from seed

interface AIGenerateResult {
  success: boolean;
  result?: unknown;
  error?: string;
  isTimeout?: boolean;
}

interface RAGInspectResult {
  ragContext?: {
    conversationHistory?: Array<{
      role?: string;
      name?: string;
      senderType?: string;
      content?: string;
    }>;
  };
}

interface MessageCreateResult {
  data?: { id?: string };
  id?: string;
}

/**
 * Test Ollama Queue & RAG Quality Fixes
 */
async function testOllamaQueueAndRAGQuality(): Promise<void> {
  console.log('🧪 Ollama Queue & RAG Quality Tests');
  console.log('====================================\n');

  const testMessageIds: string[] = [];

  try {
    // TEST 1: Ollama Queue prevents timeout cascades
    console.log('📋 TEST 1: Ollama Request Queue');
    console.log('================================');

    const prompts = [
      'What is TypeScript?',
      'Explain async/await',
      'What are promises?',
      'Define REST API',
      'What is GraphQL?',
      'Explain microservices'
    ];

    console.log(`📤 Sending ${prompts.length} concurrent AI requests...`);
    const startTime = Date.now();

    // Execute all requests concurrently
    const promises = prompts.map((prompt, idx) =>
      new Promise<AIGenerateResult>((resolve) => {
        runJtagCommand(
          `ai/generate --provider=ollama --model=phi3:mini --prompt="${prompt}" --temperature=0.7 --maxTokens=50 --requestId=test-${Date.now()}-${idx}`
        )
          .then((result) => {
            resolve({ success: true, result });
          })
          .catch((error: Error) => {
            const errorMsg = error.message;
            resolve({
              success: false,
              error: errorMsg,
              isTimeout: errorMsg.includes('timeout') ?? errorMsg.includes('timed out')
            });
          });
      })
    );

    const results = await Promise.all(promises);
    const totalTime = Date.now() - startTime;

    // Analyze results
    const successful = results.filter((r) => r.success === true);
    const failed = results.filter((r) => r.success === false);
    const timedOut = failed.filter((r) => r.isTimeout === true);

    console.log(`\n📊 Results:`);
    console.log(`   Total time: ${totalTime}ms`);
    console.log(`   Successful: ${successful.length}/${prompts.length}`);
    console.log(`   Failed: ${failed.length}/${prompts.length}`);
    console.log(`   Timeouts: ${timedOut.length}/${prompts.length}`);

    const timeoutRate = timedOut.length / prompts.length;
    console.log(`   Timeout rate: ${(timeoutRate * 100).toFixed(1)}%`);

    // Verify queue prevented cascade timeouts (was 67% before fix)
    if (timeoutRate < 0.2) {
      console.log('✅ PASS: Queue prevented cascade timeouts\n');
    } else {
      console.log('❌ FAIL: Timeout rate too high\n');
      throw new Error(`Timeout rate ${(timeoutRate * 100).toFixed(1)}% exceeds 20% threshold`);
    }

    // TEST 2: RAG Role Assignment
    console.log('📋 TEST 2: RAG Role Assignment');
    console.log('===============================');

    // Create a human message (should get 'user' role in RAG)
    console.log('📝 Creating test human message...');
    const humanMessage = (await runJtagCommand(
      `data/create --collection=chat_messages --data='${JSON.stringify({
        roomId: GENERAL_ROOM_ID,
        senderId: PRIMARY_USER_ID,
        senderName: 'Test Human',
        senderType: 'human',
        content: {
          text: 'Can someone help me understand WebSocket transports?',
          attachments: []
        },
        status: 'sent',
        priority: 'normal',
        timestamp: new Date().toISOString(),
        reactions: []
      })}'`
    )) as MessageCreateResult;

    const humanMessageId = humanMessage.data?.id ?? humanMessage.id ?? '';
    if (humanMessageId) {
      testMessageIds.push(humanMessageId);
    }

    // Build RAG context for Helper AI (persona)
    console.log('🔨 Building RAG context...');
    const ragResult = (await runJtagCommand(
      `rag/inspect --personaId=${HELPER_PERSONA_ID} --contextId=${GENERAL_ROOM_ID}`
    )) as RAGInspectResult;

    console.log('\n📋 Analyzing RAG context roles...');

    const conversationHistory = ragResult.ragContext?.conversationHistory ?? [];
    console.log(`   Messages in RAG: ${conversationHistory.length}`);

    // Group messages by role
    const humanMessages = conversationHistory.filter((msg) => msg.senderType === 'human');
    const aiMessages = conversationHistory.filter(
      (msg) =>
        (msg.name && ['Helper AI', 'Teacher AI', 'CodeReview AI', 'Claude Code', 'System'].includes(msg.name)) ??
        (msg.senderType && ['agent', 'persona', 'system'].includes(msg.senderType))
    );

    console.log(`   Human messages: ${humanMessages.length}`);
    console.log(`   AI messages: ${aiMessages.length}`);

    // Verify role assignments
    const humanRoleCorrect = humanMessages.every((msg) => msg.role === 'user');
    const aiRoleCorrect = aiMessages.every((msg) => msg.role === 'assistant');

    console.log(`   Human → 'user': ${humanRoleCorrect ? '✅' : '❌'}`);
    console.log(`   AI → 'assistant': ${aiRoleCorrect ? '✅' : '❌'}`);

    if (humanRoleCorrect && aiRoleCorrect) {
      console.log('✅ PASS: Roles match sender types correctly\n');
    } else {
      console.log('❌ FAIL: Role assignment incorrect\n');
      throw new Error('RAG role assignment does not match sender types');
    }

    // TEST 3: [QUESTION] Marker Removal
    console.log('📋 TEST 3: RAG Content Quality');
    console.log('===============================');

    // Create messages with question marks (would trigger [QUESTION] marker before fix)
    const testMessages = [
      'What is the best way to handle errors?',
      'How do I implement authentication?',
      'Can you explain dependency injection?'
    ];

    console.log('📝 Creating test messages with questions...');
    for (const text of testMessages) {
      const result = (await runJtagCommand(
        `data/create --collection=chat_messages --data='${JSON.stringify({
          roomId: GENERAL_ROOM_ID,
          senderId: PRIMARY_USER_ID,
          senderName: 'Test Human',
          senderType: 'human',
          content: { text, attachments: [] },
          status: 'sent',
          priority: 'normal',
          timestamp: new Date().toISOString(),
          reactions: []
        })}'`
      )) as MessageCreateResult;
      const messageId = result.data?.id ?? result.id ?? '';
      if (messageId) {
        testMessageIds.push(messageId);
      }
    }

    // Build RAG context
    console.log('🔨 Building RAG context...');
    const ragResult2 = (await runJtagCommand(
      `rag/inspect --personaId=${HELPER_PERSONA_ID} --contextId=${GENERAL_ROOM_ID}`
    )) as RAGInspectResult;

    console.log('\n🔍 Checking for [QUESTION] markers in RAG content...');

    const conversationHistory2 = ragResult2.ragContext?.conversationHistory ?? [];

    // Check every message content for [QUESTION] markers
    const messagesWithMarkers: string[] = [];
    for (const msg of conversationHistory2) {
      if (msg.content && msg.content.includes('[QUESTION]')) {
        messagesWithMarkers.push(msg.content);
      }
    }

    console.log(`   Total messages: ${conversationHistory2.length}`);
    console.log(`   Messages with [QUESTION]: ${messagesWithMarkers.length}`);

    if (messagesWithMarkers.length > 0) {
      console.log('   Found [QUESTION] markers in:');
      messagesWithMarkers.forEach((content, idx) => {
        console.log(`   ${idx + 1}. "${content.substring(0, 60)}..."`);
      });
    }

    if (messagesWithMarkers.length === 0) {
      console.log('✅ PASS: No [QUESTION] markers in RAG content\n');
    } else {
      console.log('❌ FAIL: Found [QUESTION] markers in RAG content\n');
      throw new Error('[QUESTION] markers still present in RAG context');
    }

    // All tests passed
    console.log('🎉 ALL TESTS PASSED!');
    console.log('✨ Ollama Queue and RAG Quality fixes working correctly');
  } finally {
    // Cleanup test messages
    console.log('\n🧹 Cleaning up test messages...');
    for (const id of testMessageIds) {
      try {
        await runJtagCommand(`data/delete --collection=chat_messages --id=${id}`);
      } catch {
        // Ignore cleanup errors
      }
    }
  }
}

testOllamaQueueAndRAGQuality().catch((error) => {
  console.error('❌ Test failed:', error instanceof Error ? error.message : String(error));
  process.exit(1);
});
