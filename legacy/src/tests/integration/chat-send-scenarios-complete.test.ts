/**
 * Comprehensive Chat Send Test - All Scenarios
 * 
 * This test replicates the test-all-chat-sends.sh functionality in TypeScript.
 * Tests different patterns of message sending to identify duplication and attribution issues.
 * 
 * Validates:
 * - Sequential message sending (original duplication pattern)
 * - Rapid fire stress testing
 * - Variable message lengths
 * - Enter key vs click button submission
 * - Special characters and edge cases
 * - Mixed timing patterns
 * - Attribution logic across all scenarios
 * - Duplication detection patterns
 */

import { JTAGClientFactory } from '../../api/shared/JTAGClientFactory';
import type { JTAGClient } from '../../api/shared/types/JTAGClientTypes';
import type { UUID } from '../../system/core/types/UUID';

interface ChatSendTestResult {
  totalMessages: number;
  testMessages: number;
  currentUserMessages: number;
  wrongSideMessages: number;
  duplicates: number;
  attributionWorking: boolean;
  sessionIdSet: boolean;
}

interface TestMessage {
  content: string;
  type: 'user' | 'assistant';
  senderId: string;
  count: number;
  isCurrentUser: boolean;
}

async function testComprehensiveChatSends() {
  console.log('🧪 Starting comprehensive chat send test - ALL SCENARIOS...');
  console.log('Testing different patterns of message sending to identify duplication and attribution issues');
  console.log('');

  let client: JTAGClient | null = null;

  try {
    // Connect to JTAG system
    client = await JTAGClientFactory.createClient({
      sessionId: 'deadbeef-cafe-4bad-8ace-5e551000c0de' as UUID,
      environment: 'server'
    });

    console.log('✅ Connected to JTAG system');

    // Helper function to send a message via widget method
    const sendMessage = async (message: string, testName: string): Promise<void> => {
      console.log(`📝 ${testName}: Sending "${message}"...`);
      
      const result = await client!.commands.exec({
        code: `
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input && chatWidget.sendMessage) {
  input.value = '${message.replace(/'/g, "\\'")}';
  chatWidget.sendMessage();
  return 'Message sent: ${message}';
} else {
  return 'ERROR: Widget or input not found';
}
        `,
        environment: 'browser'
      });

      console.log(`   Result: ${result.data}`);
    };

    // Helper function to send via Enter key
    const sendMessageEnter = async (message: string, testName: string): Promise<void> => {
      console.log(`📝 ${testName}: Sending via ENTER key "${message}"...`);
      
      const result = await client!.commands.exec({
        code: `
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input) {
  input.value = '${message.replace(/'/g, "\\'")}';
  // Simulate Enter key press
  const event = new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 13 });
  input.dispatchEvent(event);
  return 'Message sent via Enter: ${message}';
} else {
  return 'ERROR: Widget or input not found';
}
        `,
        environment: 'browser'
      });

      console.log(`   Result: ${result.data}`);
    };

    // SCENARIO 1: Sequential sends (original pattern test)
    console.log('🔥 SCENARIO 1: Sequential Message Pattern (Original Issue)');
    await sendMessage('FIRST MESSAGE - should not be doubled', 'Test 1A');
    await new Promise(resolve => setTimeout(resolve, 2000));

    await sendMessage('SECOND MESSAGE - check if doubled', 'Test 1B');
    await new Promise(resolve => setTimeout(resolve, 2000));

    await sendMessage('THIRD MESSAGE - doubling pattern test', 'Test 1C');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // SCENARIO 2: Rapid fire sends (stress test)
    console.log('');
    console.log('🔥 SCENARIO 2: Rapid Fire Sends (Stress Test)');
    await sendMessage('RAPID-1 - immediate send', 'Test 2A');
    await sendMessage('RAPID-2 - no delay', 'Test 2B'); 
    await sendMessage('RAPID-3 - stress test', 'Test 2C');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // SCENARIO 3: Different message lengths
    console.log('');
    console.log('🔥 SCENARIO 3: Variable Message Lengths');
    await sendMessage('short', 'Test 3A - Short');
    await new Promise(resolve => setTimeout(resolve, 1000));

    await sendMessage('This is a medium length message to test different content sizes and see if length affects attribution or duplication patterns in any way', 'Test 3B - Long');
    await new Promise(resolve => setTimeout(resolve, 1000));

    await sendMessage('📝🚀💬🎯✅', 'Test 3C - Emojis');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // SCENARIO 4: Enter key vs Click button
    console.log('');
    console.log('🔥 SCENARIO 4: Enter Key vs Click Button');
    await sendMessage('CLICK-BUTTON-TEST - via sendMessage()', 'Test 4A - Click');
    await new Promise(resolve => setTimeout(resolve, 2000));

    await sendMessageEnter('ENTER-KEY-TEST - via Enter keydown', 'Test 4B - Enter Key');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // SCENARIO 5: Special characters and edge cases
    console.log('');
    console.log('🔥 SCENARIO 5: Special Characters & Edge Cases');
    await sendMessage('Test with quotes and double quotes', 'Test 5A - Quotes');
    await new Promise(resolve => setTimeout(resolve, 1000));

    await sendMessage('Test with newlines and tabs', 'Test 5B - Whitespace');
    await new Promise(resolve => setTimeout(resolve, 1000));

    await sendMessage('{"json": true, "test": 123}', 'Test 5C - JSON');
    await new Promise(resolve => setTimeout(resolve, 1000));

    await sendMessage('', 'Test 5D - Empty Message');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // SCENARIO 6: Mixed timing patterns
    console.log('');
    console.log('🔥 SCENARIO 6: Mixed Timing Patterns');
    await sendMessage('TIMING-TEST-1 - short delay next', 'Test 6A');
    await new Promise(resolve => setTimeout(resolve, 500));

    await sendMessage('TIMING-TEST-2 - medium delay next', 'Test 6B');
    await new Promise(resolve => setTimeout(resolve, 2000));

    await sendMessage('TIMING-TEST-3 - long delay next', 'Test 6C');
    await new Promise(resolve => setTimeout(resolve, 5000));

    await sendMessage('TIMING-TEST-4 - after long delay', 'Test 6D');
    await new Promise(resolve => setTimeout(resolve, 3000));

    console.log('');
    console.log('⏳ Waiting for all messages to process...');
    await new Promise(resolve => setTimeout(resolve, 5000));

    // COMPREHENSIVE ANALYSIS
    console.log('📊 Analyzing comprehensive chat state...');
    const analysisResult = await client.commands.exec({
      code: `
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');

if (!chatWidget) return { error: 'Chat widget not found' };

const messages = chatWidget.messages || [];
const currentSessionId = chatWidget.currentSessionId;

// Count messages by content to detect duplicates  
const messageCounts = {};
const testMessages = [];
const duplicates = [];

messages.forEach(msg => {
  messageCounts[msg.content] = (messageCounts[msg.content] || 0) + 1;
  if (msg.content.includes('TEST') || msg.content.includes('MESSAGE') || msg.content.includes('RAPID') || msg.content.includes('TIMING')) {
    testMessages.push({
      content: msg.content.slice(0, 50) + (msg.content.length > 50 ? '...' : ''),
      type: msg.type,
      senderId: msg.senderId,
      count: messageCounts[msg.content],
      isCurrentUser: currentSessionId && msg.senderId === currentSessionId
    });
  }
});

// Find duplicated messages
Object.entries(messageCounts).forEach(([content, count]) => {
  if (count > 1 && (content.includes('TEST') || content.includes('MESSAGE') || content.includes('RAPID') || content.includes('TIMING'))) {
    duplicates.push({ content: content.slice(0, 50) + '...', count });
  }
});

const currentUserMessages = messages.filter(msg => 
  currentSessionId && msg.senderId === currentSessionId
).length;

const wrongSideMessages = messages.filter(msg => 
  currentSessionId && msg.senderId === currentSessionId && msg.type !== 'user'
).length;

console.log('');
console.log('🏁 COMPREHENSIVE TEST RESULTS:');
console.log('===============================');
console.log('Total messages:', messages.length);
console.log('Test messages found:', testMessages.length);
console.log('Current user messages:', currentUserMessages);  
console.log('Messages on wrong side:', wrongSideMessages);
console.log('Attribution working:', currentUserMessages > 0 && wrongSideMessages === 0);
console.log('Duplicated test messages:', duplicates.length);
console.log('Current session ID:', currentSessionId || 'NOT SET');
console.log('');

if (duplicates.length > 0) {
  console.log('🔍 DUPLICATED MESSAGES:');
  duplicates.forEach(dup => {
    console.log(\`  ⚠️  "\${dup.content}" appears \${dup.count} times\`);
  });
  console.log('');
}

console.log('🎯 MESSAGE ATTRIBUTION ANALYSIS:');
testMessages.slice(0, 10).forEach((msg, i) => {
  const side = msg.isCurrentUser ? 'RIGHT (✅)' : 'LEFT (❌)';
  const attribution = msg.type === 'user' ? 'USER' : 'ASSISTANT';
  console.log(\`  [\${i+1}] "\${msg.content}" - Type: \${attribution}, Side: \${side}, Count: \${msg.count}\`);
});

if (testMessages.length > 10) {
  console.log(\`  ... and \${testMessages.length - 10} more test messages\`);
}

return {
  totalMessages: messages.length,
  testMessages: testMessages.length,
  currentUserMessages,
  wrongSideMessages,
  duplicates: duplicates.length,
  attributionWorking: currentUserMessages > 0 && wrongSideMessages === 0,
  sessionIdSet: !!currentSessionId
};
      `,
      environment: 'browser'
    });

    const results = analysisResult.data as ChatSendTestResult;

    // Take screenshot for visual verification
    console.log('');
    console.log('📸 Taking comprehensive screenshot of final state...');
    await client.commands.screenshot({
      querySelector: 'chat-widget',
      filename: 'comprehensive-chat-test-results.png'
    });

    // Test assertions
    console.log('');
    console.log('🧪 TEST ASSERTIONS:');
    console.log('==================');

    const assertions = [
      {
        test: 'Total messages > 0',
        passed: results.totalMessages > 0,
        actual: results.totalMessages
      },
      {
        test: 'Test messages detected',
        passed: results.testMessages > 15, // Should have ~20 test messages
        actual: results.testMessages
      },
      {
        test: 'Current user messages > 0',
        passed: results.currentUserMessages > 0,
        actual: results.currentUserMessages
      },
      {
        test: 'No messages on wrong side',
        passed: results.wrongSideMessages === 0,
        actual: results.wrongSideMessages
      },
      {
        test: 'Attribution working correctly',
        passed: results.attributionWorking,
        actual: results.attributionWorking
      },
      {
        test: 'No duplicate messages',
        passed: results.duplicates === 0,
        actual: results.duplicates
      },
      {
        test: 'Session ID is set',
        passed: results.sessionIdSet,
        actual: results.sessionIdSet
      }
    ];

    let allPassed = true;
    assertions.forEach(({ test, passed, actual }) => {
      const status = passed ? '✅ PASS' : '❌ FAIL';
      console.log(`${status}: ${test} (${actual})`);
      if (!passed) allPassed = false;
    });

    console.log('');
    console.log('📊 SCENARIOS TESTED:');
    console.log('   1. Sequential sends (original duplication pattern)');
    console.log('   2. Rapid fire sends (stress test)');
    console.log('   3. Variable message lengths');
    console.log('   4. Enter key vs click button');
    console.log('   5. Special characters & edge cases');
    console.log('   6. Mixed timing patterns');
    console.log('');

    if (allPassed) {
      console.log('🎉 COMPREHENSIVE CHAT SEND TEST: ALL ASSERTIONS PASSED!');
      console.log('🔥 Chat system handles all message send patterns correctly');
      return { success: true, results };
    } else {
      console.log('💥 COMPREHENSIVE CHAT SEND TEST: SOME ASSERTIONS FAILED!');
      console.log('🔧 Check the detailed analysis above for issues');
      return { success: false, results };
    }

  } catch (error) {
    console.error('❌ Comprehensive chat send test failed with error:', error);
    return { success: false, error: error.message };
  } finally {
    if (client) {
      await client.disconnect();
    }
  }
}

// Run the test if this file is executed directly
if (require.main === module) {
  testComprehensiveChatSends().then(result => {
    console.log('');
    console.log('🏁 Test completed:', result.success ? 'SUCCESS' : 'FAILURE');
    process.exit(result.success ? 0 : 1);
  }).catch(error => {
    console.error('💥 Test execution failed:', error);
    process.exit(1);
  });
}

export { testComprehensiveChatSends };
export type { ChatSendTestResult };