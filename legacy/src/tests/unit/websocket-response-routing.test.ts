#!/usr/bin/env tsx
/**
 * WebSocket Response Routing Unit Test
 * 
 * Tests the exact issue: Server client responses timeout because they don't get routed properly
 * through the WebSocketResponseRouter.
 */

import type { WebSocket as WSWebSocket } from 'ws';
import type { JTAGMessage, JTAGContext } from '../../system/core/types/JTAGTypes';
import type { WebSocketClientConnection } from '../../system/transports/websocket-transport/server/WebSocketResponseRouter';
import { WebSocketResponseRouter } from '../../system/transports/websocket-transport/server/WebSocketResponseRouter';
import { JTAGMessageFactory, JTAGMessageTypes } from '../../system/core/types/JTAGTypes';

// Test constants
const TEST_CONSTANTS = {
  CLIENT_IDS: {
    FAILING_CASE: 'ws_1754963948627_c1fwbt',
    WORKING_CASE: 'ws_1754964010212_a2wfg1',
    TIMEOUT_TEST: 'ws_timeout_test_client',
    BASIC_TEST: 'ws_basic_test_client'
  },
  CORRELATION_IDS: {
    FAILING_CASE: 'client_1754963948628_67eixl5u',
    WORKING_CASE: 'corr_1754964010231_03gnsw9d', 
    TIMEOUT_TEST: 'client_timeout_test_correlation',
    BASIC_TEST: 'client_basic_test_correlation'
  },
  ENDPOINTS: {
    SERVER_FAILING: 'server',
    BROWSER_WORKING: 'browser/commands/session/create',
    SERVER_SESSION_CREATE: 'server/commands/session/create',
    SERVER_EXEC_COMMAND: 'server/commands/exec'
  },
  WEBSOCKET_STATES: {
    CONNECTING: 0,
    OPEN: 1,
    CLOSING: 2,
    CLOSED: 3
  }
} as const;

// Strongly typed mock WebSocket for testing - implements the full WSWebSocket interface
class MockWebSocket implements WSWebSocket {
  public readyState: number = TEST_CONSTANTS.WEBSOCKET_STATES.OPEN;
  private readonly sentMessages: string[] = [];
  private shouldThrowOnSend = false;

  // Required WSWebSocket properties (minimal implementation for testing)
  public url: string = 'ws://mock-websocket';
  public protocol: string = '';
  public extensions: string = '';
  public bufferedAmount: number = 0;
  public binaryType: 'nodebuffer' | 'arraybuffer' | 'fragments' = 'nodebuffer';
  public isPaused: boolean = false;
  
  // EventEmitter methods (minimal implementation)
  public on(): this { return this; }
  public off(): this { return this; }
  public once(): this { return this; }
  public emit(): boolean { return true; }
  public addListener(): this { return this; }
  public removeListener(): this { return this; }
  public removeAllListeners(): this { return this; }
  public setMaxListeners(): this { return this; }
  public getMaxListeners(): number { return 10; }
  public listeners(): Function[] { return []; }
  public rawListeners(): Function[] { return []; }
  public listenerCount(): number { return 0; }
  public prependListener(): this { return this; }
  public prependOnceListener(): this { return this; }
  public eventNames(): (string | symbol)[] { return []; }
  
  // WebSocket-specific methods (minimal implementation)
  public close(): void { this.readyState = TEST_CONSTANTS.WEBSOCKET_STATES.CLOSED; }
  public ping(): void { }
  public pong(): void { }
  public terminate(): void { this.readyState = TEST_CONSTANTS.WEBSOCKET_STATES.CLOSED; }
  
  public send(data: string): void {
    if (this.shouldThrowOnSend) {
      throw new Error('Mock WebSocket send error');
    }
    if (this.readyState !== TEST_CONSTANTS.WEBSOCKET_STATES.OPEN) {
      throw new Error('WebSocket is not open');
    }
    this.sentMessages.push(data);
  }
  
  public getLastMessage(): JTAGMessage | null {
    const lastMsg = this.sentMessages[this.sentMessages.length - 1];
    return lastMsg ? JSON.parse(lastMsg) as JTAGMessage : null;
  }
  
  public getAllMessages(): JTAGMessage[] {
    return this.sentMessages.map(msg => JSON.parse(msg) as JTAGMessage);
  }
  
  public getMessageCount(): number {
    return this.sentMessages.length;
  }
  
  public simulateDisconnect(): void {
    this.readyState = TEST_CONSTANTS.WEBSOCKET_STATES.CLOSED;
  }
  
  public simulateSendError(): void {
    this.shouldThrowOnSend = true;
  }
  
  public reset(): void {
    this.readyState = TEST_CONSTANTS.WEBSOCKET_STATES.OPEN;
    this.sentMessages.length = 0;
    this.shouldThrowOnSend = false;
  }
}

// Strongly typed test context
const createTestContext = (environment: 'server' | 'browser' = 'server'): JTAGContext => ({
  uuid: 'test-context-uuid-12345',
  environment
});

// Strongly typed response message factory
const createTestResponseMessage = (
  correlationId: string,
  endpoint: string,
  payload: Record<string, unknown> = { success: true }
): JTAGMessage => {
  const context = createTestContext();
  
  // Create a mock original request message
  const originalRequest = JTAGMessageFactory.createRequest(
    context,
    endpoint,
    TEST_CONSTANTS.ENDPOINTS.SERVER_SESSION_CREATE,
    {},
    correlationId
  );
  
  const response = JTAGMessageFactory.createResponse(
    context,
    TEST_CONSTANTS.ENDPOINTS.SERVER_SESSION_CREATE,
    endpoint,
    payload,
    originalRequest
  );
  
  return response;
};

// Simple assertion function
function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`Assertion failed: ${message}`);
  }
}

// Test suite functions
async function testServerClientResponseRouting(): Promise<boolean> {
  console.log('🧪 Test: Server client WebSocket response routing');
  
  const router = new WebSocketResponseRouter();
  const mockSocket = new MockWebSocket();
  
  try {
    // 1. Register a server client (like our failing case)
    const clientId = TEST_CONSTANTS.CLIENT_IDS.FAILING_CASE;
    const correlationId = TEST_CONSTANTS.CORRELATION_IDS.FAILING_CASE;
    
    router.registerClient(mockSocket, clientId);
    router.registerCorrelation(correlationId, clientId);
    
    // 2. Check correlation exists
    const hasCorrelation = router.hasCorrelation(correlationId);
    console.log('🔍 Correlation exists:', hasCorrelation);
    assert(hasCorrelation === true, 'Correlation should exist after registration');
    
    // 3. Create a response message like the server creates (this reproduces the bug)
    const responseMessage = createTestResponseMessage(
      correlationId,
      TEST_CONSTANTS.ENDPOINTS.SERVER_FAILING, // This is the problematic endpoint!
      { success: true, sessionId: 'test-session-12345' }
    );
    
    console.log('📤 Created response:', {
      type: responseMessage.type,
      origin: responseMessage.origin,
      endpoint: responseMessage.endpoint,
      correlationId: responseMessage.correlationId
    });
    
    // 4. Try to send the response
    const sendResult = await router.sendResponse(responseMessage);
    
    console.log('✅ Response sent successfully:', sendResult);
    assert(sendResult === true, 'Response should be sent successfully');
    
    // 5. Verify the message was sent to the WebSocket
    const messageCount = mockSocket.getMessageCount();
    assert(messageCount === 1, 'Exactly one message should be sent');
    
    const sentMessage = mockSocket.getLastMessage();
    assert(sentMessage !== null, 'Sent message should not be null');
    assert(sentMessage!.correlationId === correlationId, 'Correlation ID should match');
    assert(sentMessage!.type === 'response', 'Message type should be response');
    
    console.log('📨 Sent message correlation:', sentMessage!.correlationId);
    return true;
  } catch (error) {
    console.error('❌ Test failed:', error);
    return false;
  }
}

async function testClientDisconnectDuringRouting(): Promise<boolean> {
  console.log('🧪 Test: Client disconnect during response routing');
  
  const router = new WebSocketResponseRouter();
  const mockSocket = new MockWebSocket();
  
  try {
    const clientId = TEST_CONSTANTS.CLIENT_IDS.TIMEOUT_TEST;
    const correlationId = TEST_CONSTANTS.CORRELATION_IDS.TIMEOUT_TEST;
    
    // Register client and correlation
    router.registerClient(mockSocket, clientId);
    router.registerCorrelation(correlationId, clientId);
    
    // Verify registration worked
    assert(router.hasCorrelation(correlationId) === true, 'Correlation should exist after registration');
    
    // Simulate client disconnect (this removes the correlation)
    router.unregisterClient(clientId);
    
    // Verify correlation is gone
    assert(router.hasCorrelation(correlationId) === false, 'Correlation should be gone after disconnect');
    
    // Try to send response after disconnect
    const responseMessage = createTestResponseMessage(
      correlationId,
      TEST_CONSTANTS.ENDPOINTS.SERVER_FAILING,
      { success: true, result: 'test-exec-result' }
    );
    
    // This should fail because client disconnected and correlation is gone
    const sendResult = await router.sendResponse(responseMessage);
    
    console.log('❌ Response failed as expected:', !sendResult);
    assert(sendResult === false, 'Response should fail after client disconnect');
    assert(mockSocket.getMessageCount() === 0, 'No messages should be sent to disconnected client');
    
    return true;
  } catch (error) {
    console.error('❌ Test failed:', error);
    return false;
  }
}

function demonstrateTimeoutIssueScenario(): boolean {
  console.log('🧪 Test: Demonstrating timeout issue scenario');
  
  const router = new WebSocketResponseRouter();
  const mockSocket = new MockWebSocket();
  
  try {
    const clientId = TEST_CONSTANTS.CLIENT_IDS.TIMEOUT_TEST;
    const correlationId = TEST_CONSTANTS.CORRELATION_IDS.TIMEOUT_TEST;
    
    // 1. Client connects and sends request
    router.registerClient(mockSocket, clientId);
    router.registerCorrelation(correlationId, clientId);
    
    console.log('✅ Step 1: Client registered and correlation mapped');
    assert(router.hasCorrelation(correlationId) === true, 'Correlation should exist');
    
    // 2. Simulate 30-second timeout (client disconnects)
    console.log('⏰ Step 2: Simulating 30-second timeout...');
    router.unregisterClient(clientId);
    
    // 3. Now correlation is gone
    console.log('❌ Step 3: Correlation removed after timeout');
    assert(router.hasCorrelation(correlationId) === false, 'Correlation should be gone');
    
    // 4. Server tries to send response (this is where it fails)
    console.log('🔍 Step 4: This is where the server client response fails');
    console.log('   The response exists but correlation is gone due to timeout');
    console.log('   This represents the exact failure scenario we are seeing in production');
    
    return true;
  } catch (error) {
    console.error('❌ Test failed:', error);
    return false;
  }
}

// Main test runner
async function runAllTests(): Promise<void> {
  console.log('🧪 Running WebSocket Response Routing Unit Tests');
  console.log('=================================================');
  
  const tests = [
    { name: 'Server Client Response Routing', fn: testServerClientResponseRouting },
    { name: 'Client Disconnect During Routing', fn: testClientDisconnectDuringRouting },
    { name: 'Timeout Issue Scenario', fn: () => Promise.resolve(demonstrateTimeoutIssueScenario()) }
  ];
  
  let passCount = 0;
  let failCount = 0;
  
  for (const test of tests) {
    console.log(`\n📋 Running: ${test.name}`);
    console.log('-'.repeat(50));
    
    try {
      const result = await test.fn();
      if (result) {
        console.log(`✅ ${test.name}: PASSED`);
        passCount++;
      } else {
        console.log(`❌ ${test.name}: FAILED`);
        failCount++;
      }
    } catch (error) {
      console.error(`💥 ${test.name}: ERROR - ${error}`);
      failCount++;
    }
  }
  
  console.log('\n📊 Test Results Summary');
  console.log('========================');
  console.log(`✅ Passed: ${passCount}`);
  console.log(`❌ Failed: ${failCount}`);
  console.log(`📈 Total: ${passCount + failCount}`);
  
  if (failCount > 0) {
    process.exit(1);
  } else {
    console.log('\n🎉 All tests passed!');
  }
}

// Run tests if called directly
if (require.main === module) {
  runAllTests().catch(error => {
    console.error('💥 Test runner failed:', error);
    process.exit(1);
  });
}
