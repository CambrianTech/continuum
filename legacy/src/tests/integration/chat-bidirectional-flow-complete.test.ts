/**
 * Comprehensive Bidirectional Chat Flow Test
 * 
 * This test replicates the test-bidirectional-chat.sh functionality in TypeScript.
 * Tests the complete chat pipeline: browser→server AND server→browser
 * 
 * Validates:
 * - Browser to server message sending
 * - Server to browser message sending
 * - Rapid sequential messaging
 * - Mixed timing patterns
 * - Message attribution (User vs Assistant)
 * - Duplication detection
 * - Session ID persistence
 * - Shadow DOM navigation and widget interaction
 */

import { JTAGClientFactory } from '../../api/shared/JTAGClientFactory';
import type { JTAGClient } from '../../api/shared/types/JTAGClientTypes';
import type { UUID } from '../../system/core/types/UUID';

interface ChatMessage {
  content: string;
  senderId: string;
  type: 'user' | 'assistant';
  timestamp: number;
}

interface BidirectionalTestResult {
  totalMessages: number;
  browserMessages: number;
  serverMessages: number;
  browserDuplicates: number;
  serverDuplicates: number;
  attributionWorking: boolean;
  sessionIdSet: boolean;
}

async function testBidirectionalChatFlow() {
  console.log('🧪 Starting COMPREHENSIVE bidirectional chat test...');
  console.log('Testing complete chat pipeline: browser→server AND server→browser');
  console.log('');

  let client: JTAGClient | null = null;

  try {
    // Connect to JTAG system
    client = await JTAGClientFactory.createClient({
      sessionId: 'deadbeef-cafe-4bad-8ace-5e551000c0de' as UUID,
      environment: 'server'
    });

    console.log('✅ Connected to JTAG system');

    // Helper function to send from browser
    const sendBrowserMessage = async (message: string, testName: string): Promise<void> => {
      console.log(`📱 ${testName}: BROWSER sending "${message}"...`);
      
      const result = await client!.commands.exec({
        code: `
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input && chatWidget.sendMessage) {
  input.value = '${message}';
  chatWidget.sendMessage();
  return 'BROWSER message sent: ${message}';
} else {
  return 'ERROR: Widget or input not found';
}
        `,
        environment: 'browser'
      });

      console.log(`   Result: ${result.data}`);
      await new Promise(resolve => setTimeout(resolve, 1000));
    };

    // Helper function to send from server
    const sendServerMessage = async (message: string, testName: string): Promise<void> => {
      console.log(`🖥️  ${testName}: SERVER sending "${message}"...`);
      
      const result = await client!.commands.chat.sendMessage({
        roomId: 'general',
        content: message,
        senderType: 'server'
      });

      console.log(`   Result: ${result.success ? 'SUCCESS' : 'FAILED'}`);
      await new Promise(resolve => setTimeout(resolve, 1000));
    };

    // Clear chat for clean test
    console.log('🧹 Clearing chat history for clean test...');
    await client.commands.exec({
      code: `
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
if (chatWidget) {
  chatWidget.messages = [];
  chatWidget.renderWidget();
  return 'Chat cleared for bidirectional test';
}
return 'Could not clear chat';
      `,
      environment: 'browser'
    });

    await new Promise(resolve => setTimeout(resolve, 2000));

    console.log('');
    console.log('🔥 BIDIRECTIONAL TEST SEQUENCE:');
    console.log('   Testing alternating browser/server message sending');
    console.log('');

    // Test 1: Browser starts
    await sendBrowserMessage('BROWSER-MSG-1: Starting bidirectional test', 'Test 1A');

    // Test 2: Server responds  
    await sendServerMessage('SERVER-MSG-1: Server received and responding', 'Test 1B');

    // Test 3: Browser continues conversation
    await sendBrowserMessage('BROWSER-MSG-2: Browser acknowledges server response', 'Test 2A');

    // Test 4: Server sends multiple quickly
    await sendServerMessage('SERVER-MSG-2: Rapid server message 1', 'Test 2B');
    await sendServerMessage('SERVER-MSG-3: Rapid server message 2', 'Test 2C');

    // Test 5: Browser responds to rapid messages
    await sendBrowserMessage('BROWSER-MSG-3: Browser handling rapid server messages', 'Test 3A');

    // Test 6: Mixed timing test
    await sendBrowserMessage('BROWSER-MSG-4: Testing mixed timing', 'Test 4A');
    await new Promise(resolve => setTimeout(resolve, 500));
    await sendServerMessage('SERVER-MSG-4: Server with short delay', 'Test 4B');
    await new Promise(resolve => setTimeout(resolve, 2000));
    await sendBrowserMessage('BROWSER-MSG-5: Browser with longer delay', 'Test 4C');

    console.log('');
    console.log('⏳ Waiting for all bidirectional messages to process...');
    await new Promise(resolve => setTimeout(resolve, 3000));

    // Comprehensive bidirectional analysis
    console.log('📊 Analyzing BIDIRECTIONAL chat state...');
    const analysisResult = await client.commands.exec({
      code: `
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');

if (!chatWidget) return { error: 'Chat widget not found' };

const messages = chatWidget.messages || [];
const currentSessionId = chatWidget.currentSessionId;

// Separate browser vs server messages
const browserMessages = messages.filter(msg => 
  msg.content.includes('BROWSER-MSG') && (currentSessionId && msg.senderId === currentSessionId)
);

const serverMessages = messages.filter(msg => 
  msg.content.includes('SERVER-MSG') && (!currentSessionId || msg.senderId !== currentSessionId)
);

// Count duplicates by type
const browserDuplicates = {};
const serverDuplicates = {};

messages.forEach(msg => {
  if (msg.content.includes('BROWSER-MSG')) {
    browserDuplicates[msg.content] = (browserDuplicates[msg.content] || 0) + 1;
  } else if (msg.content.includes('SERVER-MSG')) {
    serverDuplicates[msg.content] = (serverDuplicates[msg.content] || 0) + 1;
  }
});

const browserDups = Object.entries(browserDuplicates).filter(([_, count]) => count > 1);
const serverDups = Object.entries(serverDuplicates).filter(([_, count]) => count > 1);

// Attribution analysis
const wrongSideBrowser = messages.filter(msg => 
  msg.content.includes('BROWSER-MSG') && msg.type !== 'user'
).length;

const wrongSideServer = messages.filter(msg => 
  msg.content.includes('SERVER-MSG') && msg.type !== 'assistant'  
).length;

console.log('');
console.log('🏁 BIDIRECTIONAL TEST RESULTS:');
console.log('==============================');
console.log('Total messages:', messages.length);
console.log('Browser messages (should be on RIGHT):', browserMessages.length);
console.log('Server messages (should be on LEFT):', serverMessages.length);
console.log('Current session ID:', currentSessionId || 'NOT SET');
console.log('');

console.log('🎯 ATTRIBUTION ANALYSIS:');
console.log('Browser messages on wrong side:', wrongSideBrowser);
console.log('Server messages on wrong side:', wrongSideServer);
console.log('Attribution working:', wrongSideBrowser === 0 && wrongSideServer === 0);
console.log('');

console.log('🔍 DUPLICATION ANALYSIS:');
console.log('Browser message duplicates:', browserDups.length);
if (browserDups.length > 0) {
  browserDups.forEach(([content, count]) => {
    console.log(\`  ⚠️  BROWSER: "\${content.slice(0, 40)}..." appears \${count} times\`);
  });
}
console.log('Server message duplicates:', serverDups.length);
if (serverDups.length > 0) {
  serverDups.forEach(([content, count]) => {
    console.log(\`  ⚠️  SERVER: "\${content.slice(0, 40)}..." appears \${count} times\`);
  });
}

console.log('');
console.log('💬 CONVERSATION FLOW:');
messages.filter(msg => 
  msg.content.includes('BROWSER-MSG') || msg.content.includes('SERVER-MSG')
).slice(0, 12).forEach((msg, i) => {
  const side = (currentSessionId && msg.senderId === currentSessionId) ? 'RIGHT' : 'LEFT';
  const type = msg.type === 'user' ? 'USER' : 'ASSISTANT';
  const source = msg.content.includes('BROWSER-MSG') ? 'BROWSER' : 'SERVER';
  console.log(\`  [\${i+1}] \${source}: "\${msg.content.slice(0, 50)}..." - Type: \${type}, Side: \${side}\`);
});

return {
  totalMessages: messages.length,
  browserMessages: browserMessages.length,
  serverMessages: serverMessages.length,
  browserDuplicates: browserDups.length,
  serverDuplicates: serverDups.length,
  attributionWorking: wrongSideBrowser === 0 && wrongSideServer === 0,
  sessionIdSet: !!currentSessionId
};
      `,
      environment: 'browser'
    });

    const results = analysisResult.data as BidirectionalTestResult;

    // Take screenshot for visual verification
    console.log('');
    console.log('📸 Taking bidirectional test screenshot...');
    await client.commands.screenshot({
      querySelector: 'chat-widget',
      filename: 'bidirectional-chat-test.png'
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
        test: 'Browser messages > 0',
        passed: results.browserMessages > 0,
        actual: results.browserMessages
      },
      {
        test: 'Server messages > 0', 
        passed: results.serverMessages > 0,
        actual: results.serverMessages
      },
      {
        test: 'No browser message duplicates',
        passed: results.browserDuplicates === 0,
        actual: results.browserDuplicates
      },
      {
        test: 'No server message duplicates',
        passed: results.serverDuplicates === 0,
        actual: results.serverDuplicates
      },
      {
        test: 'Attribution working correctly',
        passed: results.attributionWorking,
        actual: results.attributionWorking
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
    console.log('📊 TESTED SCENARIOS:');
    console.log('   • Browser→Server messaging');
    console.log('   • Server→Browser messaging');
    console.log('   • Rapid sequential server messages');
    console.log('   • Mixed timing patterns');
    console.log('   • Attribution for both directions');
    console.log('   • Duplication detection for both sides');
    console.log('   • Session ID persistence');
    console.log('   • Shadow DOM navigation');
    console.log('');

    if (allPassed) {
      console.log('🎉 BIDIRECTIONAL CHAT TEST: ALL ASSERTIONS PASSED!');
      console.log('🔥 Chat system is working correctly for bidirectional communication');
      return { success: true, results };
    } else {
      console.log('💥 BIDIRECTIONAL CHAT TEST: SOME ASSERTIONS FAILED!');
      console.log('🔧 Check the detailed analysis above for issues');
      return { success: false, results };
    }

  } catch (error) {
    console.error('❌ Bidirectional chat test failed with error:', error);
    return { success: false, error: error.message };
  } finally {
    if (client) {
      await client.disconnect();
    }
  }
}

// Run the test if this file is executed directly
if (require.main === module) {
  testBidirectionalChatFlow().then(result => {
    console.log('');
    console.log('🏁 Test completed:', result.success ? 'SUCCESS' : 'FAILURE');
    process.exit(result.success ? 0 : 1);
  }).catch(error => {
    console.error('💥 Test execution failed:', error);
    process.exit(1);
  });
}

export { testBidirectionalChatFlow };
export type { BidirectionalTestResult };