#!/usr/bin/env tsx
/**
 * Transport Layer Unit Tests
 * 
 * Tests the JTAG transport system including WebSocket transports,
 * transport factories, message handling, and cross-environment compatibility.
 */

import { WebSocketTransportBase } from '../../system/transports/websocket-transport/shared/WebSocketTransportBase';
import { TransportBase } from '../../system/transports/shared/TransportBase';
import type { JTAGMessage, JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGTransport, TransportSendResult, TransportConfig } from '../../system/transports/shared/TransportTypes';

console.log('🧪 Transport Layer Unit Test Suite');

// Mock WebSocket implementation for testing
class MockWebSocket {
  public readyState = 1; // WebSocket.OPEN
  public sentMessages: string[] = [];
  public handlers: { [key: string]: Function[] } = {};

  send(data: string): void {
    this.sentMessages.push(data);
  }

  addEventListener(event: string, handler: Function): void {
    if (!this.handlers[event]) {
      this.handlers[event] = [];
    }
    this.handlers[event].push(handler);
  }

  close(): void {
    this.readyState = 3; // WebSocket.CLOSED
  }

  // Simulate receiving a message
  simulateMessage(data: any): void {
    const messageEvent = { data };
    this.handlers['message']?.forEach(handler => handler(messageEvent));
  }

  // Simulate connection open
  simulateOpen(): void {
    this.readyState = 1;
    this.handlers['open']?.forEach(handler => handler());
  }

  // Simulate error
  simulateError(error: Error): void {
    this.handlers['error']?.forEach(handler => handler(error));
  }
}

// Test WebSocket transport implementation
class TestWebSocketTransport extends WebSocketTransportBase {
  public readonly name = 'TestWebSocketTransport';
  private socket?: MockWebSocket;

  async connect(): Promise<void> {
    this.socket = new MockWebSocket();
    this.connected = true;
    this.socket.simulateOpen();
  }

  async send(message: JTAGMessage): Promise<TransportSendResult> {
    if (!this.socket || !this.connected) {
      throw new Error('Transport not connected');
    }

    try {
      this.sendWebSocketMessage(this.socket, message);
      return this.createResult(true, 1);
    } catch (error) {
      return this.createResult(false, 0);
    }
  }

  async disconnect(): Promise<void> {
    if (this.socket) {
      this.socket.close();
      this.connected = false;
    }
  }

  // Expose socket for testing
  getSocket(): MockWebSocket | undefined {
    return this.socket;
  }
}

// Test transport base implementation
class TestTransportBase extends TransportBase {
  public readonly name = 'TestTransportBase';
  private sendCount = 0;

  async send(message: JTAGMessage): Promise<TransportSendResult> {
    this.sendCount++;
    
    // Simulate different success/failure scenarios based on message content
    if (message.payload && typeof message.payload === 'object' && 'simulateFailure' in message.payload) {
      return this.createResult(false, 0);
    }

    return this.createResult(true, 1);
  }

  async disconnect(): Promise<void> {
    this.connected = false;
  }

  getSendCount(): number {
    return this.sendCount;
  }
}

function testTransportBaseInterface() {
  console.log('  📝 Testing transport base interface...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const transport = new TestTransportBase();
      
      // Test basic properties
      if (transport.name !== 'TestTransportBase') {
        reject(new Error('Transport name should be set correctly'));
        return;
      }
      
      if (transport.isConnected() !== false) {
        reject(new Error('Transport should start disconnected'));
        return;
      }
      
      // Test message handler
      let receivedMessage: JTAGMessage | null = null;
      transport.setMessageHandler((message) => {
        receivedMessage = message;
      });
      
      const testMessage: JTAGMessage = {
        messageType: 'event',
        context: { uuid: 'test', environment: 'server' },
        origin: 'test',
        endpoint: 'test',
        payload: { data: 'test' },
        correlationId: 'test-id'
      };
      
      // Simulate handling incoming message (protected method access via any)
      (transport as any).handleIncomingMessage(testMessage);
      
      if (!receivedMessage || receivedMessage.endpoint !== 'test') {
        reject(new Error('Message handler should receive and process messages'));
        return;
      }
      
      console.log('  ✅ Transport base interface works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testTransportSendResults() {
  console.log('  📝 Testing transport send results...');
  
  return new Promise<void>((resolve, reject) => {
    const transport = new TestTransportBase();
    
    const successMessage: JTAGMessage = {
      messageType: 'request',
      context: { uuid: 'test', environment: 'server' },
      origin: 'test',
      endpoint: 'test/success',
      payload: { data: 'success' },
      correlationId: 'success-id'
    };
    
    const failureMessage: JTAGMessage = {
      messageType: 'request',
      context: { uuid: 'test', environment: 'server' },
      origin: 'test',
      endpoint: 'test/failure',
      payload: { simulateFailure: true },
      correlationId: 'failure-id'
    };
    
    // Test successful send
    transport.send(successMessage).then((result) => {
      if (!result.success || result.sentCount !== 1) {
        reject(new Error('Successful send should return success result'));
        return;
      }
      
      if (!result.timestamp || typeof result.timestamp !== 'string') {
        reject(new Error('Send result should include timestamp'));
        return;
      }
      
      // Test failed send
      return transport.send(failureMessage);
    }).then((result) => {
      if (result.success || result.sentCount !== 0) {
        reject(new Error('Failed send should return failure result'));
        return;
      }
      
      // Test send count tracking
      if (transport.getSendCount() !== 2) {
        reject(new Error('Transport should track send attempts'));
        return;
      }
      
      console.log('  ✅ Transport send results work');
      resolve();
    }).catch(reject);
  });
}

function testWebSocketTransportBase() {
  console.log('  📝 Testing WebSocket transport base...');
  
  return new Promise<void>((resolve, reject) => {
    const transport = new TestWebSocketTransport();
    
    // Test configuration defaults
    const config = (transport as any).config;
    if (config.reconnectAttempts !== 5 || config.reconnectDelay !== 1000) {
      reject(new Error('WebSocket config should have proper defaults'));
      return;
    }
    
    // Test session ID functionality
    transport.setSessionId('test-session-123');
    const sessionId = (transport as any).sessionId;
    if (sessionId !== 'test-session-123') {
      reject(new Error('Session ID should be stored correctly'));
      return;
    }
    
    // Test handshake creation
    const handshake = (transport as any).createSessionHandshake();
    if (handshake.type !== 'session_handshake' || handshake.sessionId !== 'test-session-123') {
      reject(new Error('Session handshake should be created correctly'));
      return;
    }
    
    // Test handshake detection
    const isHandshake = (transport as any).isSessionHandshake(handshake);
    if (!isHandshake) {
      reject(new Error('Should detect session handshake messages'));
      return;
    }
    
    const notHandshake = (transport as any).isSessionHandshake({ type: 'regular_message' });
    if (notHandshake) {
      reject(new Error('Should not detect non-handshake messages as handshakes'));
      return;
    }
    
    console.log('  ✅ WebSocket transport base works');
    resolve();
  });
}

function testWebSocketMessageHandling() {
  console.log('  📝 Testing WebSocket message handling...');
  
  return new Promise<void>((resolve, reject) => {
    const transport = new TestWebSocketTransport();
    
    transport.connect().then(() => {
      const socket = transport.getSocket();
      if (!socket) {
        reject(new Error('Socket should be available after connect'));
        return;
      }
      
      // Test message sending
      const testMessage: JTAGMessage = {
        messageType: 'event',
        context: { uuid: 'test', environment: 'browser' },
        origin: 'browser/test',
        endpoint: 'server/test',
        payload: { test: 'websocket' },
        correlationId: 'ws-test-id'
      };
      
      return transport.send(testMessage);
    }).then((result) => {
      if (!result.success) {
        reject(new Error('WebSocket send should succeed'));
        return;
      }
      
      const socket = transport.getSocket();
      if (!socket || socket.sentMessages.length !== 1) {
        reject(new Error('Message should be sent through WebSocket'));
        return;
      }
      
      // Validate sent message format
      const sentMessage = JSON.parse(socket.sentMessages[0]);
      if (sentMessage.endpoint !== 'server/test' || sentMessage.payload.test !== 'websocket') {
        reject(new Error('Sent message should preserve structure'));
        return;
      }
      
      // Test message parsing
      const testData = JSON.stringify({ type: 'test_message', data: 'parsed' });
      const parsed = (transport as any).parseWebSocketMessage(testData);
      if (parsed.type !== 'test_message' || parsed.data !== 'parsed') {
        reject(new Error('Message parsing should work correctly'));
        return;
      }
      
      console.log('  ✅ WebSocket message handling works');
      resolve();
    }).catch(reject);
  });
}

function testWebSocketConnectionStates() {
  console.log('  📝 Testing WebSocket connection states...');
  
  return new Promise<void>((resolve, reject) => {
    const transport = new TestWebSocketTransport();
    
    // Test initial disconnected state
    if (transport.isConnected()) {
      reject(new Error('Transport should start disconnected'));
      return;
    }
    
    // Test connection
    transport.connect().then(() => {
      if (!transport.isConnected()) {
        reject(new Error('Transport should be connected after connect'));
        return;
      }
      
      const socket = transport.getSocket();
      if (!socket || socket.readyState !== 1) {
        reject(new Error('Socket should be in open state'));
        return;
      }
      
      // Test disconnection
      return transport.disconnect();
    }).then(() => {
      if (transport.isConnected()) {
        reject(new Error('Transport should be disconnected after disconnect'));
        return;
      }
      
      const socket = transport.getSocket();
      if (socket && socket.readyState !== 3) {
        reject(new Error('Socket should be in closed state'));
        return;
      }
      
      console.log('  ✅ WebSocket connection states work');
      resolve();
    }).catch(reject);
  });
}

function testWebSocketMessageParsing() {
  console.log('  📝 Testing WebSocket message parsing compatibility...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const transport = new TestWebSocketTransport();
      
      // Test string parsing
      const stringData = '{"type":"string","value":"test"}';
      const stringParsed = (transport as any).parseWebSocketMessage(stringData);
      if (stringParsed.type !== 'string' || stringParsed.value !== 'test') {
        reject(new Error('String message parsing should work'));
        return;
      }
      
      // Test ArrayBuffer parsing (browser environment simulation)
      const jsonString = '{"type":"buffer","value":"array"}';
      const encoder = new TextEncoder();
      const uint8Array = encoder.encode(jsonString);
      // Convert Uint8Array to ArrayBuffer
      const arrayBuffer = uint8Array.buffer.slice(uint8Array.byteOffset, uint8Array.byteOffset + uint8Array.byteLength);
      const bufferParsed = (transport as any).parseWebSocketMessage(arrayBuffer);
      if (bufferParsed.type !== 'buffer' || bufferParsed.value !== 'array') {
        reject(new Error('ArrayBuffer message parsing should work'));
        return;
      }
      
      console.log('  ✅ WebSocket message parsing compatibility works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testWebSocketInvalidJsonHandling() {
  console.log('  📝 Testing WebSocket invalid JSON handling...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const transport = new TestWebSocketTransport();
      
      // Test invalid JSON handling - should throw error
      let errorThrown = false;
      try {
        (transport as any).parseWebSocketMessage('invalid json {');
      } catch (error) {
        errorThrown = true;
        // Expected error for invalid JSON
      }
      
      if (!errorThrown) {
        reject(new Error('Invalid JSON should throw error'));
        return;
      }
      
      console.log('  ✅ WebSocket invalid JSON handling works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testTransportEventSystem() {
  console.log('  📝 Testing transport event system...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const transport = new TestWebSocketTransport();
      
      // Mock event system
      const emittedEvents: Array<{ type: string; data: any }> = [];
      const mockEventSystem = {
        emit: (type: string, data: any) => {
          emittedEvents.push({ type, data });
        },
        on: () => {},
        off: () => {},
        once: () => {}
      };
      
      transport.setEventSystem(mockEventSystem);
      
      // Test event emission (protected method access)
      (transport as any).emitTransportEvent('ERROR', {
        error: 'Test error',
        context: 'test'
      });
      
      if (emittedEvents.length !== 1) {
        reject(new Error('Event should be emitted'));
        return;
      }
      
      const event = emittedEvents[0];
      if (event.data.transportType !== 'websocket' || event.data.error !== 'Test error') {
        reject(new Error('Event should include transport type and data'));
        return;
      }
      
      console.log('  ✅ Transport event system works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testTransportClientIdGeneration() {
  console.log('  📝 Testing transport client ID generation...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const transport = new TestWebSocketTransport();
      
      // Test default client ID generation
      const id1 = (transport as any).generateClientId();
      const id2 = (transport as any).generateClientId();
      
      if (id1 === id2) {
        reject(new Error('Client IDs should be unique'));
        return;
      }
      
      if (!id1.startsWith('ws_') || !id2.startsWith('ws_')) {
        reject(new Error('Client IDs should have default prefix'));
        return;
      }
      
      // Test custom prefix
      const customId = (transport as any).generateClientId('custom');
      if (!customId.startsWith('custom_')) {
        reject(new Error('Client ID should use custom prefix'));
        return;
      }
      
      // Test ID format
      const idPattern = /^[a-z]+_\d+_[a-z0-9]{6}$/;
      if (!idPattern.test(id1) || !idPattern.test(customId)) {
        reject(new Error('Client ID should match expected format'));
        return;
      }
      
      console.log('  ✅ Transport client ID generation works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

// Run all tests
async function runAllTests() {
  try {
    await testTransportBaseInterface();
    await testTransportSendResults();
    await testWebSocketTransportBase();
    await testWebSocketMessageHandling();
    await testWebSocketConnectionStates();
    await testWebSocketMessageParsing();
    await testWebSocketInvalidJsonHandling();
    await testTransportEventSystem();
    await testTransportClientIdGeneration();
    
    console.log('✅ All transport layer unit tests passed!');
    console.log('\\n📋 TEST SUMMARY:');
    console.log('  ✅ Transport base interface and message handling');
    console.log('  ✅ Transport send results and error handling');
    console.log('  ✅ WebSocket transport base configuration');
    console.log('  ✅ WebSocket message sending and receiving');
    console.log('  ✅ WebSocket connection state management');
    console.log('  ✅ Cross-platform message parsing compatibility');
    console.log('  ✅ Transport event system integration');
    console.log('  ✅ Unique client ID generation');
    console.log('\\n🎯 Transport layer is ready for integration testing!');
    
    process.exit(0);
  } catch (error) {
    console.error('❌ Transport layer unit test failed:', error);
    process.exit(1);
  }
}

runAllTests();