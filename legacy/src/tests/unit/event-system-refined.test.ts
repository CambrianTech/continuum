/**
 * Event System Refined Unit Tests
 * 
 * REFINED TESTING: Clean, focused unit tests with proper mocking and isolation.
 * Tests EventsDaemon behavior without external dependencies.
 */

import { EventsDaemonServer } from '../../daemons/events-daemon/server/EventsDaemonServer';
import { EventsDaemonBrowser } from '../../daemons/events-daemon/browser/EventsDaemonBrowser';
import { JTAGMessageFactory } from '../../system/core/types/JTAGTypes';
import { EVENT_ENDPOINTS } from '../../daemons/events-daemon/shared/EventEndpoints';
import { 
  createTestContext, 
  createMockEventSubscriber,
  createTestEventPayload
} from '../shared/EventTestUtilities';

interface TestCase {
  name: string;
  test: () => Promise<void>;
}

/**
 * Mock router with message tracking for verification
 */
class MockRouter {
  public sentMessages: Array<{ endpoint: string; payload: any }> = [];
  
  async postMessage(message: any) {
    this.sentMessages.push({
      endpoint: message.endpoint,
      payload: message.payload
    });
    console.log(`📤 MockRouter: Routed to ${message.endpoint}`);
    return { success: true, delivered: true };
  }
  
  clearMessages() {
    this.sentMessages = [];
  }
  
  getMessagesSentTo(targetEndpoint: string) {
    return this.sentMessages.filter(msg => 
      msg.endpoint.includes(targetEndpoint)
    );
  }
}

async function runEventSystemUnitTests() {
  console.log('🧪 EVENT SYSTEM REFINED UNIT TESTS');
  console.log('='.repeat(50));
  
  const testResults: Array<{ name: string; success: boolean; error?: string }> = [];
  
  const testCases: TestCase[] = [
    {
      name: 'EventsDaemonServer - Basic Event Handling',
      test: testServerDaemonBasicHandling
    },
    {
      name: 'EventsDaemonServer - Cross-Environment Routing',
      test: testServerDaemonCrossEnvironmentRouting
    },
    {
      name: 'EventsDaemonServer - Infinite Loop Prevention',
      test: testServerDaemonInfiniteLoopPrevention
    },
    {
      name: 'EventsDaemonBrowser - DOM Event Dispatch',
      test: testBrowserDaemonDOMDispatch
    }
  ];
  
  // Run all test cases
  for (const testCase of testCases) {
    console.log(`🎯 Running: ${testCase.name}`);
    console.log('-'.repeat(30));
    
    try {
      await testCase.test();
      testResults.push({ name: testCase.name, success: true });
      console.log(`✅ ${testCase.name}: PASSED\n`);
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      testResults.push({ name: testCase.name, success: false, error: errorMsg });
      console.error(`❌ ${testCase.name}: FAILED - ${errorMsg}\n`);
    }
  }
  
  // Summary
  const passed = testResults.filter(r => r.success).length;
  const total = testResults.length;
  
  console.log('📊 UNIT TEST RESULTS');
  console.log('='.repeat(50));
  testResults.forEach(result => {
    const status = result.success ? '✅ PASS' : '❌ FAIL';
    console.log(`${status} ${result.name}${result.error ? ` - ${result.error}` : ''}`);
  });
  
  console.log(`\n🎯 Final Score: ${passed}/${total} unit tests passed`);
  
  if (passed === total) {
    console.log('🎉 ALL UNIT TESTS PASSED');
    process.exit(0);
  } else {
    console.log('💥 Some unit tests failed');
    process.exit(1);
  }
}

/**
 * Test 1: Server daemon basic event handling
 */
async function testServerDaemonBasicHandling(): Promise<void> {
  const context = createTestContext('server');
  const mockRouter = new MockRouter();
  const daemon = new EventsDaemonServer(context, mockRouter as any);
  
  const testPayload = createTestEventPayload('test-event');
  const message = JTAGMessageFactory.createEvent(
    context,
    'test-origin',
    `events/${EVENT_ENDPOINTS.BRIDGE}`,
    testPayload
  );
  
  const result = await daemon.handleMessage(message);
  
  if (!result.success || !result.bridged) {
    throw new Error(`Expected success=true, bridged=true, got success=${result.success}, bridged=${result.bridged}`);
  }
}

/**
 * Test 2: Server daemon cross-environment routing
 */
async function testServerDaemonCrossEnvironmentRouting(): Promise<void> {
  const context = createTestContext('server');
  const mockRouter = new MockRouter();
  const daemon = new EventsDaemonServer(context, mockRouter as any);
  
  const testPayload = createTestEventPayload('cross-env-test');
  const message = JTAGMessageFactory.createEvent(
    context,
    'test-origin',
    `events/${EVENT_ENDPOINTS.BRIDGE}`,
    testPayload
  );
  
  // Process event
  await daemon.handleMessage(message);
  
  // Verify router received cross-environment routing request
  const browserMessages = mockRouter.getMessagesSentTo('browser/events');
  
  if (browserMessages.length === 0) {
    throw new Error('Expected server daemon to route event to browser environment');
  }
  
  console.log(`✅ Cross-environment routing verified: ${browserMessages.length} message(s) sent to browser`);
}

/**
 * Test 3: Server daemon infinite loop prevention
 */
async function testServerDaemonInfiniteLoopPrevention(): Promise<void> {
  const context = createTestContext('server');
  const mockRouter = new MockRouter();
  const daemon = new EventsDaemonServer(context, mockRouter as any);
  
  // Create event that originates from this same context (should prevent recursion)
  const testPayload = createTestEventPayload('recursion-test');
  testPayload.originContextUUID = context.uuid; // Mark as originating from this context
  
  const message = JTAGMessageFactory.createEvent(
    context,
    'test-origin',
    `events/${EVENT_ENDPOINTS.BRIDGE}`,
    testPayload
  );
  
  const result = await daemon.handleMessage(message);
  
  // Should still process successfully but not create infinite loops
  if (!result.success) {
    throw new Error('Daemon should handle origin context events without failing');
  }
  
  // Verify no excessive routing occurred
  const routedMessages = mockRouter.sentMessages;
  if (routedMessages.length > 2) {
    throw new Error(`Expected minimal routing, got ${routedMessages.length} messages (possible infinite loop)`);
  }
  
  console.log('✅ Infinite loop prevention verified');
}

/**
 * Test 4: Browser daemon DOM event dispatch
 */
async function testBrowserDaemonDOMDispatch(): Promise<void> {
  // Mock DOM environment
  let dispatchedEvents: Array<{ type: string; detail: any }> = [];
  
  global.document = {
    dispatchEvent: (event: any) => {
      dispatchedEvents.push({
        type: event.type,
        detail: event.detail
      });
      console.log(`✨ DOM EVENT DISPATCHED: ${event.type}`);
      return true;
    }
  } as any;
  
  global.CustomEvent = class MockCustomEvent {
    type: string;
    detail: any;
    
    constructor(type: string, options: any) {
      this.type = type;
      this.detail = options.detail;
    }
  } as any;
  
  const context = createTestContext('browser');
  const mockRouter = new MockRouter();
  const daemon = new EventsDaemonBrowser(context, mockRouter as any);
  
  // Use a mapped event that the DOMEventBridge will dispatch
  const testPayload = createTestEventPayload('chat-message-sent', 'test-room');
  const message = JTAGMessageFactory.createEvent(
    context,
    'test-origin',
    `events/${EVENT_ENDPOINTS.BRIDGE}`,
    testPayload
  );
  
  await daemon.handleMessage(message);
  
  // Wait for DOM event dispatch
  await new Promise(resolve => setTimeout(resolve, 100));
  
  // Verify DOM event was dispatched (should be mapped to 'chat:message-received')
  const domEvents = dispatchedEvents.filter(e => 
    e.type === 'chat:message-received' || 
    e.type === 'chat-message-sent'
  );
  
  if (domEvents.length === 0) {
    console.log('📊 Dispatched events:', dispatchedEvents.map(e => e.type));
    throw new Error('Expected browser daemon to dispatch DOM event for chat-message-sent');
  }
  
  console.log(`✅ DOM event dispatch verified: ${domEvents.length} DOM event(s) dispatched`);
  console.log(`   Event types: ${domEvents.map(e => e.type).join(', ')}`);
}

// Run the refined unit test suite
runEventSystemUnitTests();