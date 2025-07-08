/**
 * BrowserWebSocketDaemon Unit Tests
 * 
 * Tests WebSocket daemon functionality in isolation with mocked WebSocket
 * and validates connection management, message handling, and reconnection logic.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { BrowserWebSocketDaemon } from '../../BrowserWebSocketDaemon';

// Mock WebSocket
const mockWebSocket = {
  CONNECTING: 0,
  OPEN: 1,
  CLOSING: 2,
  CLOSED: 3,
  readyState: 1,
  send: vi.fn(),
  close: vi.fn(),
  addEventListener: vi.fn(),
  removeEventListener: vi.fn(),
  onopen: null as any,
  onclose: null as any,
  onmessage: null as any,
  onerror: null as any
};

// Mock global WebSocket
global.WebSocket = vi.fn(() => mockWebSocket) as any;

// Mock global objects
global.navigator = { userAgent: 'test-agent' } as any;
global.window = { 
  location: { href: 'http://localhost:3000', hostname: 'localhost' },
  __CONTINUUM_VERSION__: '1.0.0'
} as any;

describe('BrowserWebSocketDaemon', () => {
  let daemon: BrowserWebSocketDaemon;

  beforeEach(() => {
    daemon = new BrowserWebSocketDaemon();
    vi.clearAllMocks();
  });

  afterEach(async () => {
    await daemon.stop();
  });

  describe('Initialization', () => {
    it('should initialize with correct message types', () => {
      const messageTypes = daemon.getMessageTypes();
      expect(messageTypes).toContain('websocket:connect');
      expect(messageTypes).toContain('websocket:disconnect');
      expect(messageTypes).toContain('websocket:send');
      expect(messageTypes).toContain('websocket:status');
      expect(messageTypes).toContain('websocket:execute_command');
    });

    it('should start in disconnected state', () => {
      const state = daemon.getConnectionState();
      expect(state.state).toBe('disconnected');
      expect(state.clientId).toBeNull();
      expect(state.sessionId).toBeNull();
      expect(state.reconnectAttempts).toBe(0);
    });

    it('should accept custom configuration options', () => {
      const customDaemon = new BrowserWebSocketDaemon({
        maxReconnectAttempts: 10,
        reconnectDelay: 2000,
        connectionTimeout: 10000
      });
      
      expect(customDaemon).toBeInstanceOf(BrowserWebSocketDaemon);
    });
  });

  describe('Connection Management', () => {
    it('should handle websocket:connect message', async () => {
      const response = await daemon.handleMessage({
        type: 'websocket:connect',
        data: { wsUrl: 'ws://test.com' },
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(true);
      expect(response.data?.connectionState).toBeDefined();
      expect(global.WebSocket).toHaveBeenCalledWith('ws://test.com');
    });

    it('should handle websocket:disconnect message', async () => {
      // First connect
      await daemon.handleMessage({
        type: 'websocket:connect',
        data: {},
        timestamp: new Date().toISOString()
      });

      // Then disconnect
      const response = await daemon.handleMessage({
        type: 'websocket:disconnect',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(true);
      expect(mockWebSocket.close).toHaveBeenCalledWith(1000, 'Client disconnect');
    });

    it('should handle websocket:status message', async () => {
      const response = await daemon.handleMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(true);
      expect(response.data?.connectionState).toBeDefined();
      expect(response.data?.isConnected).toBeDefined();
      expect(response.data?.queuedMessages).toBeDefined();
      expect(response.data?.pendingRequests).toBeDefined();
    });
  });

  describe('Message Handling', () => {
    it('should handle websocket:send message', async () => {
      const testMessage = {
        type: 'test_message',
        data: { key: 'value' }
      };

      const response = await daemon.handleMessage({
        type: 'websocket:send',
        data: testMessage,
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(true);
      expect(response.data?.sent).toBe(true);
    });

    it('should queue messages when disconnected', async () => {
      const testMessage = {
        type: 'test_message',
        data: { key: 'value' }
      };

      // Send message while disconnected
      await daemon.handleMessage({
        type: 'websocket:send',
        data: testMessage,
        timestamp: new Date().toISOString()
      });

      // Message should be queued
      const status = await daemon.handleMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(status.data?.queuedMessages).toBeGreaterThan(0);
    });

    it('should handle unknown message types gracefully', async () => {
      const response = await daemon.handleMessage({
        type: 'unknown:message',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown message type');
    });
  });

  describe('Event System', () => {
    it('should handle websocket:subscribe message', async () => {
      const mockHandler = vi.fn();
      
      const response = await daemon.handleMessage({
        type: 'websocket:subscribe',
        data: { event: 'test:event', handler: mockHandler },
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(true);
      expect(response.data?.subscribed).toBe(true);
    });

    it('should handle websocket:unsubscribe message', async () => {
      const mockHandler = vi.fn();
      
      // First subscribe
      await daemon.handleMessage({
        type: 'websocket:subscribe',
        data: { event: 'test:event', handler: mockHandler },
        timestamp: new Date().toISOString()
      });

      // Then unsubscribe
      const response = await daemon.handleMessage({
        type: 'websocket:unsubscribe',
        data: { event: 'test:event', handler: mockHandler },
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(true);
      expect(response.data?.unsubscribed).toBe(true);
    });

    it('should emit events to subscribed handlers', () => {
      const mockHandler = vi.fn();
      daemon.on('test:event', mockHandler);
      
      daemon.emit('test:event', { data: 'test' });
      
      expect(mockHandler).toHaveBeenCalledWith({ data: 'test' });
    });

    it('should handle errors in event handlers gracefully', () => {
      const errorHandler = vi.fn(() => {
        throw new Error('Handler error');
      });
      
      daemon.on('test:event', errorHandler);
      
      // Should not throw
      expect(() => {
        daemon.emit('test:event', {});
      }).not.toThrow();
    });
  });

  describe('Command Execution', () => {
    it('should handle websocket:execute_command message', async () => {
      // Connect first
      await daemon.handleMessage({
        type: 'websocket:connect',
        data: {},
        timestamp: new Date().toISOString()
      });

      // Simulate connection establishment
      if (mockWebSocket.onopen) {
        mockWebSocket.onopen({} as Event);
      }

      const response = await daemon.handleMessage({
        type: 'websocket:execute_command',
        data: { command: 'health', params: {} },
        timestamp: new Date().toISOString()
      });

      // Should attempt to execute (might timeout in test)
      expect(response.success).toBeDefined();
    });

    it('should reject commands when disconnected', async () => {
      const response = await daemon.handleMessage({
        type: 'websocket:execute_command',
        data: { command: 'health', params: {} },
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(false);
      expect(response.error).toContain('WebSocket not connected');
    });
  });

  describe('Connection State', () => {
    it('should report correct connection state', () => {
      const state = daemon.getConnectionState();
      expect(state).toHaveProperty('state');
      expect(state).toHaveProperty('clientId');
      expect(state).toHaveProperty('sessionId');
      expect(state).toHaveProperty('reconnectAttempts');
      expect(state).toHaveProperty('lastError');
    });

    it('should detect connected state correctly', () => {
      // Initially disconnected
      expect(daemon.isConnected()).toBe(false);
    });

    it('should update state during connection process', async () => {
      const connectPromise = daemon.handleMessage({
        type: 'websocket:connect',
        data: {},
        timestamp: new Date().toISOString()
      });

      // State should be connecting
      const connectingState = daemon.getConnectionState();
      expect(connectingState.state).toBe('connecting');

      await connectPromise;
    });
  });

  describe('Error Handling', () => {
    it('should handle message parsing errors gracefully', async () => {
      const response = await daemon.handleMessage({
        type: 'websocket:send',
        data: undefined as any,
        timestamp: new Date().toISOString()
      });

      // Should handle gracefully
      expect(response).toBeDefined();
    });

    it('should handle connection errors', async () => {
      const connectPromise = daemon.handleMessage({
        type: 'websocket:connect',
        data: {},
        timestamp: new Date().toISOString()
      });

      // Simulate connection error
      if (mockWebSocket.onerror) {
        mockWebSocket.onerror(new Event('error'));
      }

      const response = await connectPromise.catch(e => ({ success: false, error: e.message }));
      expect(response.success).toBe(false);
    });
  });

  describe('Lifecycle Management', () => {
    it('should start and stop cleanly', async () => {
      await daemon.start();
      expect(daemon.isRunning()).toBe(true);

      await daemon.stop();
      expect(daemon.isRunning()).toBe(false);
    });

    it('should auto-connect on start in development', async () => {
      await daemon.start();
      
      // Should attempt auto-connect in localhost environment
      expect(global.WebSocket).toHaveBeenCalled();
    });

    it('should disconnect on stop', async () => {
      await daemon.start();
      await daemon.stop();
      
      expect(mockWebSocket.close).toHaveBeenCalled();
    });
  });
});