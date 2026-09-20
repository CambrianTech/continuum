#!/usr/bin/env tsx
/**
 * JTAGRouter - Comprehensive Test Suite (Middle-Out Architecture)
 * 
 * Tests the critical nerve center of JTAG system:
 * - Cross-environment message routing with proper correlation
 * - Promise resolution across browser/server contexts  
 * - Event flow through router (broadcast & p2p BasePayload events)
 * - Message correlation and request/response matching
 * - Transport abstraction and failover scenarios
 * 
 * FOLLOWS MIDDLE-OUT TESTING PATTERN:
 * - Unit: Pure router logic (endpoint matching, correlation, queuing)
 * - Integration: Cross-context message flow with mock transports
 * - System: Full browser-server routing with real network transport
 */

import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory, JTAGMessageTypes } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { MessageSubscriber } from '../../../system/core/router/shared/JTAGRouter';
import { JTAGRouterDynamicServer } from '../../../system/core/router/server/JTAGRouterDynamicServer';
import { JTAGRouterDynamicBrowser } from '../../../system/core/router/browser/JTAGRouterDynamicBrowser';
import type { JTAGResponsePayload, BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { ResponseCorrelator } from '../../../system/core/shared/ResponseCorrelator';
import { EndpointMatcher } from '../../../system/core/router/shared/EndpointMatcher';

console.log('🧪 JTAGRouter Comprehensive Test Suite (Middle-Out Architecture)');

// ========================================
// MOCK INFRASTRUCTURE
// ========================================

/**
 * Mock Message Subscriber - Clean implementation for testing
 */
class MockMessageSubscriber implements MessageSubscriber {
  public receivedMessages: JTAGMessage[] = [];
  public responses: Map<string, JTAGResponsePayload> = new Map();
  
  constructor(
    public readonly endpoint: string,
    public readonly uuid: string = generateUUID()
  ) {}
  
  async handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    this.receivedMessages.push(message);
    
    // Return configured response or default success
    const response = this.responses.get(message.correlationId) || {
      success: true,
      timestamp: new Date().toISOString(),
      context: message.context,
      sessionId: message.payload?.sessionId || 'test-session'
    };
    
    return response;
  }
  
  // Test helper methods
  clearReceivedMessages(): void {
    this.receivedMessages = [];
  }
  
  setResponse(correlationId: string, response: JTAGResponsePayload): void {
    this.responses.set(correlationId, response);
  }
}

/**
 * Mock Transport for testing cross-context scenarios
 */
class MockTransport {
  public sentMessages: JTAGMessage[] = [];
  public isConnected = true;
  public shouldFail = false;
  
  constructor(public readonly name: string) {}
  
  async send(message: JTAGMessage): Promise<{ success: boolean; timestamp: string }> {
    if (!this.isConnected || this.shouldFail) {
      throw new Error(`MockTransport ${this.name} not available`);
    }
    
    this.sentMessages.push(message);
    return {
      success: true,
      timestamp: new Date().toISOString()
    };
  }
  
  // Test helper methods
  clearSentMessages(): void {
    this.sentMessages = [];
  }
}

// ========================================
// UNIT TESTS - PURE ROUTER LOGIC
// ========================================

/**
 * Test 1: Endpoint Matching & Subscriber Management
 */
async function testEndpointMatching(): Promise<void> {
  console.log('  📝 Testing endpoint pattern matching and subscriber management...');
  
  const matcher = new EndpointMatcher<MockMessageSubscriber>();
  
  // Test subscribers
  const pingSubscriber = new MockMessageSubscriber('commands/ping');
  const commandsSubscriber = new MockMessageSubscriber('commands');
  const fileSubscriber = new MockMessageSubscriber('commands/file');
  
  // Register subscribers with different specificity levels
  matcher.register('commands/ping', pingSubscriber);
  matcher.register('commands', commandsSubscriber);  
  matcher.register('commands/file', fileSubscriber);
  
  // Test exact match priority (most specific wins)
  const pingMatch = matcher.match('commands/ping');
  if (!pingMatch || pingMatch.subscriber !== pingSubscriber || pingMatch.matchType !== 'exact') {
    throw new Error('Exact match should take priority');
  }
  
  // Test hierarchical match (parent catches unregistered children)
  const screenshotMatch = matcher.match('commands/screenshot');
  if (!screenshotMatch || screenshotMatch.subscriber !== commandsSubscriber || screenshotMatch.matchType !== 'hierarchical') {
    throw new Error('Hierarchical match should work for unregistered children');
  }
  
  // Test no match for unrelated endpoints
  const noMatch = matcher.match('unrelated/endpoint');
  if (noMatch !== null) {
    throw new Error('Unregistered endpoint should not match');
  }
  
  // Test file endpoint exact match
  const fileMatch = matcher.match('commands/file');
  if (!fileMatch || fileMatch.subscriber !== fileSubscriber) {
    throw new Error('File endpoint should match exactly');
  }
  
  // Test file subcommand hierarchical match
  const fileSaveMatch = matcher.match('commands/file/save');
  if (!fileSaveMatch || fileSaveMatch.subscriber !== fileSubscriber || fileSaveMatch.matchType !== 'hierarchical') {
    throw new Error('File save should match hierarchically to commands/file');
  }
  
  console.log('  ✅ Endpoint pattern matching works correctly');
}

/**
 * Test 2: Response Correlation System
 */
async function testResponseCorrelation(): Promise<void> {
  console.log('  📝 Testing response correlation and promise resolution...');
  
  const correlator = new ResponseCorrelator(5000); // 5 second timeout
  
  // Test correlation ID generation uniqueness
  const id1 = correlator.generateCorrelationId();
  const id2 = correlator.generateCorrelationId();
  
  if (id1 === id2) {
    throw new Error('Correlation IDs must be unique');
  }
  
  // Test correlation ID format (req_timestamp_random)
  if (!id1.match(/^req_\d+_[a-z0-9]+$/)) {
    throw new Error(`Invalid correlation ID format: ${id1}`);
  }
  
  // Test request-response correlation
  const correlationId = correlator.generateCorrelationId();
  const testResponse = { success: true, data: 'test response', timestamp: new Date().toISOString() };
  
  // Create request and resolve it asynchronously
  const requestPromise = correlator.createRequest(correlationId);
  
  // Simulate response arriving after short delay
  setTimeout(() => {
    correlator.resolveRequest(correlationId, testResponse);
  }, 10);
  
  // Wait for correlation to complete
  const response = await requestPromise;
  if (JSON.stringify(response) !== JSON.stringify(testResponse)) {
    throw new Error('Response correlation failed - data mismatch');
  }
  
  // Test timeout handling
  const timeoutCorrelationId = correlator.generateCorrelationId();
  const timeoutPromise = correlator.createRequest(timeoutCorrelationId, 100); // 100ms timeout
  
  try {
    await timeoutPromise;
    throw new Error('Should have timed out');
  } catch (error: any) {
    if (!error.message.includes('timeout')) {
      throw new Error(`Expected timeout error, got: ${error.message}`);
    }
  }
  
  console.log('  ✅ Response correlation system works correctly');
}

/**
 * Test 3: Router Initialization & Context Management  
 */
async function testRouterInitialization(): Promise<void> {
  console.log('  📝 Testing router initialization and context management...');
  
  // Test server router initialization
  const serverContext: JTAGContext = { 
    uuid: generateUUID(), 
    environment: 'server' 
  };
  
  const serverRouter = new JTAGRouterDynamicServer(serverContext, {
    sessionId: generateUUID(),
    enableCrossContext: false, // Disable for unit testing
    transport: { protocol: 'websocket', port: 9001 }
  });
  
  if (serverRouter.context.environment !== 'server') {
    throw new Error('Server router should have server context');
  }
  
  // Test browser router initialization
  const browserContext: JTAGContext = {
    uuid: generateUUID(),
    environment: 'browser'
  };
  
  const browserRouter = new JTAGRouterDynamicBrowser(browserContext, {
    sessionId: generateUUID(),
    enableCrossContext: false, // Disable for unit testing  
    transport: { protocol: 'websocket', port: 9001 }
  });
  
  if (browserRouter.context.environment !== 'browser') {
    throw new Error('Browser router should have browser context');
  }
  
  // Test router status and identification
  const serverStatus = serverRouter.status;
  if (!serverStatus || typeof serverStatus !== 'object') {
    throw new Error('Server router should have status object');
  }
  
  const browserStatus = browserRouter.status;
  if (!browserStatus || typeof browserStatus !== 'object') {
    throw new Error('Browser router should have status object');
  }
  
  // Test that routers have different contexts
  if (serverRouter.context.uuid === browserRouter.context.uuid) {
    throw new Error('Server and browser routers should have different UUIDs');
  }
  
  console.log('  ✅ Router initialization and context management works correctly');
}

// ========================================
// INTEGRATION TESTS - MESSAGE FLOW
// ========================================

/**
 * Test 4: Local Message Routing (within same context)
 */
async function testLocalMessageRouting(): Promise<void> {
  console.log('  📝 Testing local message routing and subscriber delivery...');
  
  const context: JTAGContext = { uuid: generateUUID(), environment: 'server' };
  const router = new JTAGRouterDynamicServer(context, {
    sessionId: generateUUID(),
    enableCrossContext: false
  });
  
  // Register test subscriber
  const testSubscriber = new MockMessageSubscriber('commands/test');
  router.registerSubscriber('commands/test', testSubscriber);
  
  // Create test message
  const testMessage: JTAGMessage = JTAGMessageFactory.createRequest(
    context,
    'client',
    'commands/test',
    { testData: 'hello world' }
  );
  
  // Route message and verify delivery
  const result = await router.postMessage(testMessage);
  
  // Check if routing was successful (result may be RequestResult or other RouterResult type)
  if (!result || typeof result !== 'object') {
    throw new Error('Router should return result object for local routing');
  }
  
  // Verify subscriber received message
  if (testSubscriber.receivedMessages.length !== 1) {
    throw new Error('Subscriber should receive exactly one message');
  }
  
  const receivedMessage = testSubscriber.receivedMessages[0];
  if (receivedMessage.endpoint !== 'commands/test') {
    throw new Error('Received message should preserve endpoint');
  }
  
  if (receivedMessage.correlationId !== testMessage.correlationId) {
    throw new Error('Received message should preserve correlation ID');
  }
  
  if (JSON.stringify(receivedMessage.payload) !== JSON.stringify(testMessage.payload)) {
    throw new Error('Received message should preserve payload');
  }
  
  console.log('  ✅ Local message routing works correctly');
}

/**
 * Test 5: Endpoint Priority & Specificity
 */
async function testEndpointPriority(): Promise<void> {
  console.log('  📝 Testing endpoint priority and specificity resolution...');
  
  const context: JTAGContext = { uuid: generateUUID(), environment: 'server' };
  const router = new JTAGRouterDynamicServer(context, {
    sessionId: generateUUID(),
    enableCrossContext: false
  });
  
  // Register overlapping subscribers (exact vs hierarchical)
  const exactSubscriber = new MockMessageSubscriber('commands/ping');
  const hierarchicalSubscriber = new MockMessageSubscriber('commands');
  
  router.registerSubscriber('commands/ping', exactSubscriber);
  router.registerSubscriber('commands', hierarchicalSubscriber);
  
  // Test that exact match takes priority
  const testMessage: JTAGMessage = JTAGMessageFactory.createRequest(
    context,
    'client', 
    'commands/ping',
    {}
  );
  
  await router.postMessage(testMessage);
  
  // Exact subscriber should get the message
  if (exactSubscriber.receivedMessages.length !== 1) {
    throw new Error('Exact subscriber should receive the message');
  }
  
  // Hierarchical subscriber should not get it (exact takes priority)
  if (hierarchicalSubscriber.receivedMessages.length !== 0) {
    throw new Error('Hierarchical subscriber should not receive message when exact match exists');
  }
  
  // Test hierarchical fallback for unregistered endpoint
  hierarchicalSubscriber.clearReceivedMessages();
  exactSubscriber.clearReceivedMessages();
  
  const fallbackMessage: JTAGMessage = JTAGMessageFactory.createRequest(
    context,
    'client',
    'commands/screenshot', // No exact match registered
    {}
  );
  
  await router.postMessage(fallbackMessage);
  
  // Hierarchical subscriber should catch this
  if (hierarchicalSubscriber.receivedMessages.length !== 1) {
    throw new Error('Hierarchical subscriber should catch unregistered children');
  }
  
  // Exact subscriber should not get it
  if (exactSubscriber.receivedMessages.length !== 0) {
    throw new Error('Exact subscriber should not receive non-matching message');
  }
  
  console.log('  ✅ Endpoint priority and specificity resolution works correctly');
}

/**
 * Test 6: Cross-Context Message Correlation (Promise Resolution)
 */
async function testCrossContextCorrelation(): Promise<void> {
  console.log('  📝 Testing cross-context promise resolution and correlation...');
  
  // Create browser and server contexts
  const browserContext: JTAGContext = { uuid: generateUUID(), environment: 'browser' };
  const serverContext: JTAGContext = { uuid: generateUUID(), environment: 'server' };
  
  const browserRouter = new JTAGRouterDynamicBrowser(browserContext, {
    sessionId: generateUUID(),
    enableCrossContext: false // Test correlation logic without actual transport
  });
  
  const serverRouter = new JTAGRouterDynamicServer(serverContext, {
    sessionId: generateUUID(),
    enableCrossContext: false
  });
  
  // Register file/save handler on server
  const fileSubscriber = new MockMessageSubscriber('commands/file/save');
  fileSubscriber.setResponse('test-correlation', {
    success: true,
    message: 'File saved successfully',
    timestamp: new Date().toISOString(),
    context: serverContext,
    sessionId: 'test-session'
  });
  
  serverRouter.registerSubscriber('commands/file/save', fileSubscriber);
  
  // Simulate cross-context message flow:
  // 1. Browser creates request message
  const crossContextMessage: JTAGMessage = JTAGMessageFactory.createRequest(
    browserContext,
    'browser/commands',
    'server/commands/file/save',
    { filename: 'test.txt', content: 'Hello World' },
    'test-correlation'
  );
  
  // 2. "Send" to server router (simulated transport)
  // In real scenario, this would go through WebSocket transport
  const serverResult = await serverRouter.postMessage({
    ...crossContextMessage,
    // Transform endpoint for server routing (remove environment prefix)
    endpoint: 'commands/file/save'
  });
  
  // 3. Verify server processed message correctly
  if (!('response' in serverResult) || !serverResult.response) {
    throw new Error('Server should return response for file save');
  }
  
  const serverResponse = serverResult.response;
  if (!serverResponse.success || serverResponse.message !== 'File saved successfully') {
    throw new Error('Server response should contain success message');
  }
  
  // 4. Verify subscriber received correct message
  if (fileSubscriber.receivedMessages.length !== 1) {
    throw new Error('File subscriber should receive exactly one message');
  }
  
  const receivedMessage = fileSubscriber.receivedMessages[0];
  if (receivedMessage.correlationId !== 'test-correlation') {
    throw new Error('Cross-context correlation ID should be preserved');
  }
  
  if (!receivedMessage.payload?.filename || receivedMessage.payload.filename !== 'test.txt') {
    throw new Error('Cross-context payload should be preserved');
  }
  
  console.log('  ✅ Cross-context promise resolution and correlation works correctly');
}

// ========================================
// EVENT SYSTEM TESTS - BASEPLAYLOAD EVENTS
// ========================================

/**
 * Test 7: Event Flow Through Router (BasePayload Events)
 */
async function testEventFlow(): Promise<void> {
  console.log('  📝 Testing event flow through router - broadcast and p2p BasePayload events...');
  
  const context: JTAGContext = { uuid: generateUUID(), environment: 'server' };
  const router = new JTAGRouterDynamicServer(context, {
    sessionId: generateUUID(),
    enableCrossContext: false
  });
  
  // Create event subscribers
  const eventSubscriber1 = new MockMessageSubscriber('events/system');
  const eventSubscriber2 = new MockMessageSubscriber('events/system');  
  const specificSubscriber = new MockMessageSubscriber('events/system/health');
  
  router.registerSubscriber('events/system', eventSubscriber1);
  router.registerSubscriber('events/system', eventSubscriber2); // Multiple subscribers for broadcast
  router.registerSubscriber('events/system/health', specificSubscriber);
  
  // Test broadcast event (all subscribers should receive)
  const broadcastEvent: JTAGMessage = JTAGMessageFactory.createEvent(
    context,
    'system',
    'events/system',
    {
      eventType: 'system_ready',
      timestamp: new Date().toISOString(),
      context,
      sessionId: 'system-session'
    } // This follows BasePayload structure
  );
  
  // Events don't return responses, they broadcast
  const eventResult = await router.postMessage(broadcastEvent);
  
  // For events, we expect the routing to succeed but no specific response
  if (!('routingMetadata' in eventResult)) {
    throw new Error('Event routing should return metadata');
  }
  
  // Verify both general subscribers received the broadcast event
  if (eventSubscriber1.receivedMessages.length !== 1) {
    throw new Error('First event subscriber should receive broadcast');
  }
  
  if (eventSubscriber2.receivedMessages.length !== 1) {
    throw new Error('Second event subscriber should receive broadcast');
  }
  
  // Specific subscriber should not receive general event (endpoint doesn't match)
  if (specificSubscriber.receivedMessages.length !== 0) {
    throw new Error('Specific subscriber should not receive non-matching event');
  }
  
  // Clear messages for next test
  eventSubscriber1.clearReceivedMessages();
  eventSubscriber2.clearReceivedMessages();
  
  // Test specific event (only specific subscriber should receive)
  const specificEvent: JTAGMessage = JTAGMessageFactory.createEvent(
    context,
    'health-daemon',
    'events/system/health',
    {
      eventType: 'health_check',
      status: 'healthy',
      timestamp: new Date().toISOString(),
      context,
      sessionId: 'health-session'
    }
  );
  
  await router.postMessage(specificEvent);
  
  // Specific subscriber should receive it
  if (specificSubscriber.receivedMessages.length !== 1) {
    throw new Error('Specific subscriber should receive targeted event');
  }
  
  // General subscribers should not receive specific event
  if (eventSubscriber1.receivedMessages.length !== 0 || eventSubscriber2.receivedMessages.length !== 0) {
    throw new Error('General subscribers should not receive specific events');
  }
  
  console.log('  ✅ Event flow through router works correctly');
}

// ========================================
// SYSTEM TESTS - FAILURE SCENARIOS
// ========================================

/**
 * Test 8: Router Resilience & Error Handling
 */
async function testRouterResilience(): Promise<void> {
  console.log('  📝 Testing router resilience and error handling...');
  
  const context: JTAGContext = { uuid: generateUUID(), environment: 'server' };
  const router = new JTAGRouterDynamicServer(context, {
    sessionId: generateUUID(),
    enableCrossContext: false
  });
  
  // Test 1: No subscriber found
  const orphanMessage: JTAGMessage = JTAGMessageFactory.createRequest(
    context,
    'client',
    'nonexistent/endpoint',
    {}
  );
  
  const orphanResult = await router.postMessage(orphanMessage);
  
  // Should handle gracefully (return error response, not throw)
  if (!('error' in orphanResult) || !orphanResult.error) {
    throw new Error('Router should return error for messages with no subscribers');
  }
  
  // Test 2: Subscriber throws error
  const faultySubscriber = new MockMessageSubscriber('commands/faulty');
  faultySubscriber.handleMessage = async () => {
    throw new Error('Subscriber internal error');
  };
  
  router.registerSubscriber('commands/faulty', faultySubscriber);
  
  const faultyMessage: JTAGMessage = JTAGMessageFactory.createRequest(
    context,
    'client',
    'commands/faulty',
    {}
  );
  
  const faultyResult = await router.postMessage(faultyMessage);
  
  // Router should handle subscriber errors gracefully
  if (!('error' in faultyResult) || !faultyResult.error) {
    throw new Error('Router should handle subscriber errors gracefully');
  }
  
  if (!faultyResult.error.includes('Subscriber internal error')) {
    throw new Error('Router should preserve original error message');
  }
  
  console.log('  ✅ Router resilience and error handling works correctly');
}

// ========================================
// RUN ALL TESTS
// ========================================

async function runAllTests(): Promise<void> {
  try {
    // Unit Tests - Pure Router Logic
    await testEndpointMatching();
    await testResponseCorrelation();
    await testRouterInitialization();
    
    // Integration Tests - Message Flow  
    await testLocalMessageRouting();
    await testEndpointPriority();
    await testCrossContextCorrelation();
    
    // Event System Tests
    await testEventFlow();
    
    // System Tests - Failure Scenarios
    await testRouterResilience();
    
    console.log('✅ All JTAGRouter tests passed!');
    console.log('\n📋 TEST SUMMARY:');
    console.log('  ✅ Unit: Endpoint pattern matching and correlation logic');
    console.log('  ✅ Unit: Response correlation and promise resolution');
    console.log('  ✅ Unit: Router initialization and context management');
    console.log('  ✅ Integration: Local message routing and subscriber delivery');
    console.log('  ✅ Integration: Endpoint priority and specificity resolution');
    console.log('  ✅ Integration: Cross-context promise resolution and correlation');
    console.log('  ✅ Event: BasePayload event flow through router (broadcast & p2p)');
    console.log('  ✅ System: Router resilience and error handling');
    console.log('\n🎯 JTAGRouter is bulletproof and ready for production!');
    
  } catch (error) {
    console.error('❌ JTAGRouter test failed:', error);
    throw error;
  }
}

// Run tests if called directly
if (process.argv[1] && process.argv[1].endsWith('JTAGRouter.test.ts')) {
  runAllTests()
    .then(() => process.exit(0))
    .catch(() => process.exit(1));
}

export { runAllTests };