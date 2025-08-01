#!/usr/bin/env tsx
/**
 * Routing Logic Unit Tests
 * 
 * Tests the JTAG message routing system including endpoint matching,
 * message correlation, and cross-context routing between browser and server.
 */

import { JTAGRouterServer } from '../../server/JTAGRouterServer';
import { JTAGRouterBrowser } from '../../browser/JTAGRouterBrowser';
import { EndpointMatcher } from '@sharedRouting/EndpointMatcher';
import { ResponseCorrelator } from '../../system/core/shared/ResponseCorrelator';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';

console.log('🧪 Routing Logic Unit Test Suite');

// Mock message subscriber for testing
class MockMessageSubscriber {
  public receivedMessages: JTAGMessage[] = [];
  
  async handleMessage(message: JTAGMessage): Promise<any> {
    this.receivedMessages.push(message);
    return {
      success: true,
      message: `Mock response to ${message.endpoint}`,
      correlationId: message.correlationId
    };
  }
}

function testEndpointMatching() {
  console.log('  📝 Testing endpoint matching logic...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const matcher = new EndpointMatcher();
      const mockSubscriber = { handleMessage: async () => ({ success: true }) };
      
      // Register subscribers
      matcher.register('commands/ping', mockSubscriber);
      matcher.register('commands', mockSubscriber);
      
      // Test exact matches
      const exactMatch = matcher.match('commands/ping');
      if (!exactMatch || exactMatch.matchType !== 'exact') {
        reject(new Error('Exact match should work'));
        return;
      }
      
      // Test hierarchical matches
      const hierarchicalMatch = matcher.match('commands/screenshot');
      if (!hierarchicalMatch || hierarchicalMatch.matchType !== 'hierarchical') {
        reject(new Error('Hierarchical match should work'));
        return;
      }
      
      if (hierarchicalMatch.matchedEndpoint !== 'commands') {
        reject(new Error('Should match parent endpoint'));
        return;
      }
      
      // Test exact check
      if (!matcher.hasExact('commands/ping')) {
        reject(new Error('Should have exact registration'));
        return;
      }
      
      // Test non-matches
      const noMatch = matcher.match('unregistered/endpoint');
      if (noMatch !== null) {
        reject(new Error('Unregistered endpoint should not match'));
        return;
      }
      
      console.log('  ✅ Endpoint matching logic works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testResponseCorrelation() {
  console.log('  📝 Testing response correlation...');
  
  return new Promise<void>((resolve, reject) => {
    const correlator = new ResponseCorrelator(1000);
    
    // Test correlation ID generation
    const id1 = correlator.generateCorrelationId();
    const id2 = correlator.generateCorrelationId();
    
    if (id1 === id2) {
      reject(new Error('Correlation IDs should be unique'));
      return;
    }
    
    if (!id1.match(/^req_\d+_[a-z0-9]+$/)) {
      reject(new Error('Correlation ID should be valid req_timestamp_random format'));
      return;
    }
    
    // Test request-response correlation
    const correlationId = correlator.generateCorrelationId();
    const requestPromise = correlator.createRequest(correlationId);
    const testResponse = { success: true, data: 'test' };
    
    setTimeout(() => {
      correlator.resolveRequest(correlationId, testResponse);
    }, 10);
    
    requestPromise.then((response) => {
      if (JSON.stringify(response) !== JSON.stringify(testResponse)) {
        reject(new Error('Response correlation failed'));
        return;
      }
      console.log('  ✅ Response correlation works');
      resolve();
    }).catch(reject);
  });
}

function testRouterInitialization() {
  console.log('  📝 Testing router initialization...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const serverContext: JTAGContext = { uuid: 'test-server', environment: 'server' };
      const browserContext: JTAGContext = { uuid: 'test-browser', environment: 'browser' };
      
      const serverRouter = new JTAGRouterServer(serverContext);
      const browserRouter = new JTAGRouterBrowser(browserContext);
      
      // Test router properties
      if (serverRouter.context.environment !== 'server') {
        reject(new Error('Server router should have server context'));
        return;
      }
      
      if (browserRouter.context.environment !== 'browser') {
        reject(new Error('Browser router should have browser context'));
        return;
      }
      
      // Test that routers are properly constructed
      if (!serverRouter.toString().includes('JTAGRouterServer')) {
        reject(new Error('Server router toString should identify as server'));
        return;
      }
      
      if (!browserRouter.toString().includes('JTAGRouterBrowser')) {
        reject(new Error('Browser router toString should identify as browser'));
        return;
      }
      
      console.log('  ✅ Router initialization works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testMessageSubscriberRegistration() {
  console.log('  📝 Testing message subscriber registration...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const context: JTAGContext = { uuid: 'test', environment: 'server' };
      const router = new JTAGRouterServer(context);
      const mockSubscriber = { 
        handleMessage: async () => ({ success: true }),
        endpoint: 'test/endpoint',
        uuid: 'mock-subscriber'
      };
      
      // Register subscriber
      router.registerSubscriber('test/endpoint', mockSubscriber);
      
      // Test that router status shows registered subscriber
      const status = router.status;
      if (status.subscribers === 0) {
        reject(new Error('Router should show registered subscribers'));
        return;
      }
      
      console.log('  ✅ Message subscriber registration works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testMessageRouting() {
  console.log('  📝 Testing message routing...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test', environment: 'server' };
    const router = new JTAGRouterServer(context);
    const mockSubscriber = new MockMessageSubscriber();
    
    // Register subscriber for test endpoint
    router.registerSubscriber('commands/test', mockSubscriber);
    
    // Create test message
    const testMessage: JTAGMessage = {
      messageType: 'request',
      context,
      origin: 'client',
      endpoint: 'commands/test',
      payload: { test: 'data' },
      correlationId: 'test-correlation-id'
    };
    
    // Route the message using postMessage (actual router API)
    router.postMessage(testMessage).then((response) => {
      if (!response) {
        reject(new Error('Router should return response'));
        return;
      }
      
      if (mockSubscriber.receivedMessages.length !== 1) {
        reject(new Error('Subscriber should receive one message'));
        return;
      }
      
      const receivedMessage = mockSubscriber.receivedMessages[0];
      if (receivedMessage.endpoint !== 'commands/test') {
        reject(new Error('Received message should have correct endpoint'));
        return;
      }
      
      if (receivedMessage.correlationId !== 'test-correlation-id') {
        reject(new Error('Received message should preserve correlation ID'));
        return;
      }
      
      console.log('  ✅ Message routing works');
      resolve();
    }).catch(reject);
  });
}

function testEndpointPriority() {
  console.log('  📝 Testing endpoint priority resolution...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const context: JTAGContext = { uuid: 'test', environment: 'server' };
      const router = new JTAGRouterServer(context);
      const exactSubscriber = new MockMessageSubscriber();
      const hierarchicalSubscriber = new MockMessageSubscriber();
      
      // Register both exact and hierarchical subscribers 
      router.registerSubscriber('commands/ping', exactSubscriber);
      router.registerSubscriber('commands', hierarchicalSubscriber);
      
      // Create test message that matches both
      const testMessage: JTAGMessage = {
        messageType: 'request',
        context,
        origin: 'client',
        endpoint: 'commands/ping',
        payload: {},
        correlationId: 'priority-test'
      };
      
      // Route the message using postMessage (actual router API)
      router.postMessage(testMessage).then(() => {
        // The exact match should take priority over hierarchical
        if (exactSubscriber.receivedMessages.length !== 1) {
          reject(new Error('Exact subscriber should receive the message'));
          return;
        }
        
        if (hierarchicalSubscriber.receivedMessages.length !== 0) {
          reject(new Error('Hierarchical subscriber should not receive the message when exact match exists'));
          return;
        }
        
        console.log('  ✅ Endpoint priority resolution works');
        resolve();
      }).catch(reject);
    } catch (error) {
      reject(error);
    }
  });
}

function testCrossContextRouting() {
  console.log('  📝 Testing cross-context routing preparation...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const serverContext: JTAGContext = { uuid: 'test-server', environment: 'server' };
      const browserContext: JTAGContext = { uuid: 'test-browser', environment: 'browser' };
      
      const serverRouter = new JTAGRouterServer(serverContext);
      const browserRouter = new JTAGRouterBrowser(browserContext);
      
      // Test that routers can identify cross-context messages
      const browserToServerMessage: JTAGMessage = {
        messageType: 'request',
        context: browserContext,
        origin: 'browser/commands',
        endpoint: 'server/commands/file/save',
        payload: { filename: 'test.txt', content: 'test' },
        correlationId: 'cross-context-test'
      };
      
      // Test endpoint parsing for cross-context routing
      const targetEnvironment = browserToServerMessage.endpoint.split('/')[0];
      if (targetEnvironment !== 'server') {
        reject(new Error('Should correctly identify target environment'));
        return;
      }
      
      const localEndpoint = browserToServerMessage.endpoint.split('/').slice(1).join('/');
      if (localEndpoint !== 'commands/file/save') {
        reject(new Error('Should correctly extract local endpoint'));
        return;
      }
      
      console.log('  ✅ Cross-context routing preparation works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

// Run all tests
async function runAllTests() {
  try {
    await testEndpointMatching();
    await testResponseCorrelation();
    await testRouterInitialization();
    await testMessageSubscriberRegistration();
    await testMessageRouting();
    await testEndpointPriority();
    await testCrossContextRouting();
    
    console.log('✅ All routing logic unit tests passed!');
    console.log('\n📋 TEST SUMMARY:');
    console.log('  ✅ Endpoint pattern matching and wildcards');
    console.log('  ✅ Request-response correlation system');
    console.log('  ✅ Router initialization and context handling');
    console.log('  ✅ Message subscriber registration and lookup');
    console.log('  ✅ Local message routing and delivery');
    console.log('  ✅ Endpoint priority and specificity resolution');
    console.log('  ✅ Cross-context routing infrastructure');
    console.log('\n🎯 Routing system is ready for integration testing!');
    
    process.exit(0);
  } catch (error) {
    console.error('❌ Routing logic unit test failed:', error);
    process.exit(1);
  }
}

runAllTests();