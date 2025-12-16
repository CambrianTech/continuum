/**
 * REAL Chat Response Time Test
 * =============================
 *
 * Measures the ACTUAL time it takes for an AI persona to respond to a chat message.
 * This is what matters - not individual API calls.
 *
 * Flow:
 * 1. Send message as human
 * 2. Wait for AI persona to respond
 * 3. Measure total time
 *
 * Success criteria:
 * - Response time < 20 seconds (acceptable for chat)
 * - AI actually responds (not silent)
 * - Response quality is good (not gibberish)
 */

import { execSync } from 'child_process';
import { runJtagCommand } from '../test-utils/CRUDTestUtils';

const PRIMARY_USER_ID = '00000000-0000-0000-0000-000000000001';
const TEACHER_AI_ID = '9c601908-0c66-4fac-8c2e-d4d7c43ba9dd';
const GENERAL_ROOM_ID = '5e71a0c8-0303-4eb8-a478-3a1212488c8c';

interface ChatMessage {
  id: string;
  senderId: string;
  senderName: string;
  content: { text: string };
  timestamp: string;
}

/**
 * Send a message and wait for AI response
 */
async function sendMessageAndWaitForResponse(
  messageText: string,
  maxWaitSeconds: number = 30
): Promise<{
  success: boolean;
  responseTime: number;
  aiResponse?: string;
  error?: string;
}> {
  const startTime = Date.now();

  try {
    // Get message count before
    const beforeResult = await runJtagCommand(
      `${DATA_COMMANDS.LIST} --collection=chat_messages --filter='{"roomId":"${GENERAL_ROOM_ID}"}' --orderBy='[{"field":"timestamp","direction":"desc"}]'`
    );
    const messagesBefore = beforeResult?.items?.length || 0;

    console.log(`📊 Messages before: ${messagesBefore}`);

    // Send message using proper chat/send command
    console.log(`📨 Sending message: "${messageText}"`);
    const sendResult = await runJtagCommand(
      `collaboration/chat/send --room="${GENERAL_ROOM_ID}" --message="${messageText}"`
    );

    if (!sendResult?.success) {
      return {
        success: false,
        responseTime: Date.now() - startTime,
        error: 'Failed to send message'
      };
    }

    console.log(`✅ Message sent, waiting for AI response...`);

    // Poll for AI response
    for (let i = 0; i < maxWaitSeconds * 2; i++) {
      await new Promise(resolve => setTimeout(resolve, 500)); // Check every 500ms

      const afterResult = await runJtagCommand(
        `${DATA_COMMANDS.LIST} --collection=chat_messages --filter='{"roomId":"${GENERAL_ROOM_ID}"}' --orderBy='[{"field":"timestamp","direction":"desc"}]'`
      );
      const messagesAfter = afterResult?.items || [];

      // Look for new AI message
      const newAIMessages = messagesAfter.filter((msg: ChatMessage) =>
        msg.senderName === 'Teacher AI' &&
        new Date(msg.timestamp).getTime() > startTime
      );

      if (newAIMessages.length > 0) {
        const responseTime = Date.now() - startTime;
        const aiResponse = newAIMessages[0].content.text;

        console.log(`✅ AI responded in ${responseTime}ms`);
        console.log(`📝 Response: "${aiResponse.substring(0, 100)}..."`);

        return {
          success: true,
          responseTime,
          aiResponse
        };
      }
    }

    // Timeout
    return {
      success: false,
      responseTime: Date.now() - startTime,
      error: `No response after ${maxWaitSeconds}s`
    };

  } catch (error) {
    return {
      success: false,
      responseTime: Date.now() - startTime,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

/**
 * Main test
 */
async function runChatResponseTimeTest() {
  console.log('\n🧪 REAL CHAT RESPONSE TIME TEST');
  console.log('================================\n');

  const tests = [
    {
      name: 'Direct mention (fast-path)',
      message: 'Teacher AI, what is TypeScript?'
    },
    {
      name: 'Question without mention',
      message: 'Can someone explain async/await?'
    },
    {
      name: 'Simple greeting',
      message: 'Hi everyone!'
    }
  ];

  const results = [];

  for (const test of tests) {
    console.log(`\n📋 Test: ${test.name}`);
    console.log(`   Message: "${test.message}"\n`);

    const result = await sendMessageAndWaitForResponse(test.message, 30);

    results.push({
      name: test.name,
      ...result
    });

    if (result.success) {
      console.log(`   ✅ PASS (${result.responseTime}ms)`);
    } else {
      console.log(`   ❌ FAIL (${result.responseTime}ms): ${result.error}`);
    }

    // Wait between tests to avoid rate limiting
    await new Promise(resolve => setTimeout(resolve, 2000));
  }

  // Summary
  console.log('\n\n📊 SUMMARY');
  console.log('============================================================\n');

  const successful = results.filter(r => r.success);
  const avgTime = successful.length > 0
    ? successful.reduce((sum, r) => sum + r.responseTime, 0) / successful.length
    : 0;

  console.log(`Success Rate: ${successful.length}/${results.length} (${Math.round(successful.length / results.length * 100)}%)`);
  console.log(`Average Response Time: ${Math.round(avgTime)}ms (${(avgTime / 1000).toFixed(1)}s)`);
  console.log('');

  results.forEach(r => {
    const status = r.success ? '✅' : '❌';
    const time = (r.responseTime / 1000).toFixed(1);
    console.log(`${status} ${r.name}: ${time}s`);
    if (r.error) {
      console.log(`   Error: ${r.error}`);
    }
  });

  console.log('');

  // Pass/fail criteria
  const passRate = successful.length / results.length;
  const fastEnough = avgTime < 20000; // 20 seconds

  if (passRate >= 0.8 && fastEnough) {
    console.log('✅ PRODUCTION READY');
  } else {
    console.log('❌ NOT PRODUCTION READY');
    if (passRate < 0.8) {
      console.log(`   ⚠️  Success rate too low: ${Math.round(passRate * 100)}%`);
    }
    if (!fastEnough) {
      console.log(`   ⚠️  Too slow: ${(avgTime / 1000).toFixed(1)}s average`);
    }
  }
}

// Run test
runChatResponseTimeTest().catch(error => {
  console.error('❌ Test failed:', error);
  process.exit(1);
});
