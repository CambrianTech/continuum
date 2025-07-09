/**
 * Integration tests for ContinuumBrowserClient
 * Tests the complete module with all components working together
 */

import { describe, it, expect, beforeEach, afterEach, jest } from '@jest/globals';
import { ContinuumBrowserClient } from '../../ContinuumBrowserClient';

describe('ContinuumBrowserClient Integration', () => {
  let client: ContinuumBrowserClient;
  let mockWebSocket: any;

  beforeEach(() => {
    // Mock WebSocket with event simulation
    mockWebSocket = {
      send: jest.fn(),
      close: jest.fn(),
      readyState: 1, // WebSocket.OPEN
      onopen: null,
      onmessage: null,
      onclose: null,
      onerror: null
    };
    
    global.WebSocket = jest.fn(() => mockWebSocket);
    global.navigator = { userAgent: 'test-agent' } as any;
    global.window = { location: { href: 'http://localhost:3000' } } as any;
    global.document = {
      addEventListener: jest.fn(),
      removeEventListener: jest.fn(),
      dispatchEvent: jest.fn()
    } as any;
    
    client = new ContinuumBrowserClient();
  });

  afterEach(() => {
    jest.clearAllMocks();
  });

  describe('WebSocket lifecycle', () => {
    it('should initialize WebSocket connection', () => {
      expect(global.WebSocket).toHaveBeenCalledWith('ws://localhost:9000');
    });

    it('should send client_init message on connection', () => {
      // Simulate WebSocket open
      if (mockWebSocket.onopen) {
        mockWebSocket.onopen();
      }
      
      expect(mockWebSocket.send).toHaveBeenCalledWith(
        expect.stringContaining('"type":"client_init"')
      );
    });

    it('should handle connection_confirmed message', () => {
      const clientId = 'test-client-123';
      const message = {
        type: 'connection_confirmed',
        data: { clientId }
      };
      
      // Simulate message reception
      if (mockWebSocket.onmessage) {
        mockWebSocket.onmessage({ data: JSON.stringify(message) });
      }
      
      expect(client.clientId).toBe(clientId);
    });

    it('should handle session_ready message', () => {
      const sessionId = 'test-session-456';
      const message = {
        type: 'session_ready',
        data: { sessionId }
      };
      
      // Simulate message reception
      if (mockWebSocket.onmessage) {
        mockWebSocket.onmessage({ data: JSON.stringify(message) });
      }
      
      expect(client.sessionId).toBe(sessionId);
      expect(client.state).toBe('ready');
    });
  });

  describe('console forwarding integration', () => {
    it('should enable console forwarding on connected state', () => {
      const originalLog = console.log;
      
      // Simulate WebSocket connection
      if (mockWebSocket.onopen) {
        mockWebSocket.onopen();
      }
      
      // Check that console is overridden
      expect(console.log).not.toBe(originalLog);
    });

    it('should queue console messages before ready state', () => {
      // Simulate connected state (console forwarding enabled)
      if (mockWebSocket.onopen) {
        mockWebSocket.onopen();
      }
      
      // Log something before ready state
      console.log('test message');
      
      // Should not send immediately (queued)
      expect(mockWebSocket.send).not.toHaveBeenCalledWith(
        expect.stringContaining('"type":"execute_command"')
      );
    });
  });

  describe('command execution integration', () => {
    beforeEach(() => {
      // Set up client in ready state
      client.clientId = 'test-client';
      client.sessionId = 'test-session';
      
      // Simulate WebSocket open and session ready
      if (mockWebSocket.onopen) {
        mockWebSocket.onopen();
      }
      
      const sessionMessage = {
        type: 'session_ready',
        data: { sessionId: 'test-session' }
      };
      
      if (mockWebSocket.onmessage) {
        mockWebSocket.onmessage({ data: JSON.stringify(sessionMessage) });
      }
    });

    it('should send execute_command message when executing command', async () => {
      const commandPromise = client.execute('test-command', { param1: 'value1' });
      
      expect(mockWebSocket.send).toHaveBeenCalledWith(
        expect.stringContaining('"type":"execute_command"')
      );
      
      // Simulate command response
      const responseMessage = {
        type: 'execute_command_response',
        success: true,
        data: { result: 'success' },
        requestId: expect.any(String)
      };
      
      // Extract request ID from sent message
      const sentMessage = JSON.parse(mockWebSocket.send.mock.calls[0][0]);
      responseMessage.requestId = sentMessage.data.requestId;
      
      // Simulate response
      if (mockWebSocket.onmessage) {
        mockWebSocket.onmessage({ data: JSON.stringify(responseMessage) });
      }
      
      const result = await commandPromise;
      expect(result).toEqual({ result: 'success' });
    });

    it('should reject command execution when not connected', async () => {
      // Reset to not ready state
      client.sessionId = null;
      
      await expect(client.execute('test-command')).rejects.toThrow(
        'Continuum not ready'
      );
    });
  });
});