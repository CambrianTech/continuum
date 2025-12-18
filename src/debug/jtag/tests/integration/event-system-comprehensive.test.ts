#!/usr/bin/env tsx
/**
 * Event System Comprehensive Integration Test
 * 
 * REFINED TESTING: Clean, focused, well-structured test for cross-environment events.
 * Tests the complete server→browser event flow with proper verification.
 */

import { jtag } from '../../server-index';
import { 
  createBrowserEventListenerCode,
  createBrowserEventProofCode,
  waitForEventPropagation,
  validateEventTestResult,
  cleanupBrowserProofElements
} from '../shared/EventTestUtilities';

interface TestSuite {
  name: string;
  test: () => Promise<void>;
}

async function runComprehensiveEventTests() {
  console.log('🧪 COMPREHENSIVE EVENT SYSTEM INTEGRATION TEST');
  console.log('='.repeat(60));
  
  let client: any;
  const testResults: Array<{ name: string; success: boolean; error?: string }> = [];
  
  try {
    // Connect to JTAG system
    console.log('🔌 Connecting to JTAG system...');
    client = await jtag.connect({ targetEnvironment: 'server' });
    console.log('✅ Connected successfully\n');
    
    const testSuites: TestSuite[] = [
      {
        name: 'Basic Cross-Environment Events',
        test: () => testBasicCrossEnvironmentEvents(client)
      },
      {
        name: 'Room-Scoped Event Delivery',
        test: () => testRoomScopedEventDelivery(client)
      },
      {
        name: 'Chat Message Event Emission',
        test: () => testChatMessageEventEmission(client)
      }
    ];
    
    // Run all test suites
    for (const suite of testSuites) {
      console.log(`🎯 Running: ${suite.name}`);
      console.log('-'.repeat(40));
      
      try {
        await suite.test();
        testResults.push({ name: suite.name, success: true });
        console.log(`✅ ${suite.name}: PASSED\n`);
      } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        testResults.push({ name: suite.name, success: false, error: errorMsg });
        console.error(`❌ ${suite.name}: FAILED - ${errorMsg}\n`);
      }
    }
    
    // Final cleanup
    await cleanupBrowserProofElements(client, [
      'basic-event-proof',
      'room-scoped-proof',
      'chat-message-proof'
    ]);
    
    // Summary
    const passed = testResults.filter(r => r.success).length;
    const total = testResults.length;
    
    console.log('📊 TEST RESULTS SUMMARY');
    console.log('='.repeat(60));
    testResults.forEach(result => {
      const status = result.success ? '✅ PASS' : '❌ FAIL';
      console.log(`${status} ${result.name}${result.error ? ` - ${result.error}` : ''}`);
    });
    
    console.log(`\n🎯 Final Score: ${passed}/${total} tests passed`);
    
    if (passed === total) {
      console.log('🎉 ALL TESTS PASSED - Event system is working correctly!');
      process.exit(0);
    } else {
      console.log('💥 Some tests failed - event system needs fixes');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('💥 Test suite crashed:', error);
    process.exit(1);
  }
}

/**
 * Test 1: Basic cross-environment event flow
 */
async function testBasicCrossEnvironmentEvents(client: any): Promise<void> {
  const proofElementId = 'basic-event-proof';
  
  // Setup browser event listener
  await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: createBrowserEventListenerCode('test-event', proofElementId)
    }
  });
  
  // Emit test event from server
  await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: `
        // Emit a test event that should bridge to browser
        const jtagSystem = global.jtagSystem || require('../../server-index').jtagSystem;
        if (jtagSystem?.eventManager?.events) {
          jtagSystem.eventManager.events.emit('test-event', {
            source: 'server-integration-test',
            data: 'Basic cross-environment test'
          });
          console.log('📤 SERVER: Test event emitted');
        } else {
          console.error('❌ SERVER: JTAG event system not available');
        }
      `
    }
  });
  
  // Wait and verify
  await waitForEventPropagation(1500);
  
  const result = await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: createBrowserEventProofCode(proofElementId)
    }
  });
  
  validateEventTestResult('Basic Cross-Environment Events', result.result);
}

/**
 * Test 2: Room-scoped event delivery
 */
async function testRoomScopedEventDelivery(client: any): Promise<void> {
  const proofElementId = 'room-scoped-proof';
  
  // Setup browser listener for room-specific events
  await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: createBrowserEventListenerCode('chat-room-test-message-sent', proofElementId)
    }
  });
  
  // Send chat message to specific room
  const chatResult = await client.commands['collaboration/chat/send']({
    roomId: 'test-room',
    content: 'Room-scoped event test message',
    sessionId: generateUUID()
  });
  
  if (!chatResult.success) {
    throw new Error(`Chat command failed: ${chatResult.error}`);
  }
  
  // Wait and verify
  await waitForEventPropagation(1500);
  
  const result = await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: createBrowserEventProofCode(proofElementId)
    }
  });
  
  validateEventTestResult('Room-Scoped Event Delivery', result.result);
}

/**
 * Test 3: Chat message event emission
 */
async function testChatMessageEventEmission(client: any): Promise<void> {
  const proofElementId = 'chat-message-proof';
  
  // Setup browser listener for chat message events
  await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: createBrowserEventListenerCode('chat-message-sent', proofElementId)
    }
  });
  
  // Send chat message that should emit cross-environment event
  const chatResult = await client.commands['collaboration/chat/send']({
    roomId: 'integration-test-room',
    content: 'Chat message integration test',
    sessionId: generateUUID()
  });
  
  if (!chatResult.success) {
    throw new Error(`Chat command failed: ${chatResult.error}`);
  }
  
  // Wait and verify
  await waitForEventPropagation(2000);
  
  const result = await client.commands.exec({
    code: {
      type: 'inline',
      language: 'javascript',
      source: createBrowserEventProofCode(proofElementId)
    }
  });
  
  validateEventTestResult('Chat Message Event Emission', result.result);
}

// Run the comprehensive test suite
runComprehensiveEventTests();