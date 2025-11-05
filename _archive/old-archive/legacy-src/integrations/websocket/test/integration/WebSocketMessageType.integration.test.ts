/**
 * WebSocket Message Type Integration Test
 * 
 * Layer 4: System Integration - Validates WebSocket command response message types
 * 
 * Tests the fix for the browser console logging issue where WebSocketDaemon
 * was sending 'command_response' but browser expected 'execute_command_response'
 */

import { WebSocketDaemon } from '../../WebSocketDaemon';
import WebSocket from 'ws';

describe('Layer 4: WebSocket Message Type Integration', () => {
  let daemon: WebSocketDaemon;
  let client: WebSocket;
  const testPort = 9001; // Use different port to avoid conflicts

  beforeAll(async () => {
    // Start WebSocket daemon for testing
    daemon = new WebSocketDaemon({ port: testPort, host: 'localhost' });
    
    // Register a mock command processor
    const mockCommandProcessor = {
      name: 'command-processor',
      handleMessage: async (message: any) => {
        if (message.type === 'command.execute' && message.data.command === 'console') {
          return {
            success: true,
            data: {
              forwarded: true,
              timestamp: new Date().toISOString(),
              consoleEntry: {
                action: message.data.parameters.action,
                message: message.data.parameters.message,
                source: message.data.parameters.source || 'test'
              }
            }
          };
        }
        return { success: false, error: 'Unknown command' };
      }
    };
    
    daemon.registerDaemon(mockCommandProcessor);
    await daemon.start();
  });

  afterAll(async () => {
    if (client && client.readyState === WebSocket.OPEN) {
      client.close();
    }
    if (daemon) {
      await daemon.stop();
    }
  });

  beforeEach(() => {
    if (client && client.readyState === WebSocket.OPEN) {
      client.close();
    }
  });

  test('should send execute_command_response message type (not command_response)', (done) => {
    client = new WebSocket(`ws://localhost:${testPort}`);
    
    const testRequestId = `test-${Date.now()}`;
    let responseReceived = false;

    client.on('open', () => {
      // Send console command
      const message = {
        type: 'execute_command',
        data: {
          command: 'console',
          params: JSON.stringify({
            action: 'log',
            message: 'Test message type fix',
            source: 'integration-test'
          }),
          requestId: testRequestId,
          sessionId: 'test-session'
        }
      };

      client.send(JSON.stringify(message));
    });

    client.on('message', (data) => {
      try {
        const response = JSON.parse(data.toString());
        
        // Skip connection confirmation messages
        if (response.type === 'connection_confirmed') {
          return;
        }

        // Validate the response message type
        if (response.requestId === testRequestId) {
          responseReceived = true;
          
          // CRITICAL TEST: Message type must be 'execute_command_response'
          expect(response.type).toBe('execute_command_response');
          expect(response.type).not.toBe('command_response'); // Old broken type
          
          // Validate response structure
          expect(response.success).toBe(true);
          expect(response.command).toBe('console');
          expect(response.data).toBeDefined();
          expect(response.data.forwarded).toBe(true);
          
          client.close();
          done();
        }
      } catch (error) {
        done(error);
      }
    });

    client.on('error', (error) => {
      done(error);
    });

    // Timeout safety
    setTimeout(() => {
      if (!responseReceived) {
        client.close();
        done(new Error('Test timeout - no response received'));
      }
    }, 5000);
  });

  test('should send execute_command_response for error responses', (done) => {
    client = new WebSocket(`ws://localhost:${testPort}`);
    
    const testRequestId = `error-test-${Date.now()}`;
    let responseReceived = false;

    client.on('open', () => {
      // Send invalid command to trigger error response
      const message = {
        type: 'execute_command',
        data: {
          command: 'nonexistent-command',
          params: '{}',
          requestId: testRequestId,
          sessionId: 'test-session'
        }
      };

      client.send(JSON.stringify(message));
    });

    client.on('message', (data) => {
      try {
        const response = JSON.parse(data.toString());
        
        // Skip connection confirmation messages
        if (response.type === 'connection_confirmed') {
          return;
        }

        if (response.requestId === testRequestId) {
          responseReceived = true;
          
          // CRITICAL TEST: Error responses also use execute_command_response
          expect(response.type).toBe('execute_command_response');
          expect(response.success).toBe(false);
          expect(response.error).toBeDefined();
          
          client.close();
          done();
        }
      } catch (error) {
        done(error);
      }
    });

    client.on('error', (error) => {
      done(error);
    });

    // Timeout safety
    setTimeout(() => {
      if (!responseReceived) {
        client.close();
        done(new Error('Error test timeout - no response received'));
      }
    }, 5000);
  });

  test('should maintain backward compatibility with browser client expectations', (done) => {
    client = new WebSocket(`ws://localhost:${testPort}`);
    
    const testRequestId = `compat-test-${Date.now()}`;
    let responseReceived = false;

    client.on('open', () => {
      // Simulate exact browser client message format
      const message = {
        type: 'execute_command',
        data: {
          command: 'console',
          params: JSON.stringify({
            action: 'health_report',
            message: 'Browser compatibility test',
            source: 'browser-client-simulation'
          }),
          requestId: testRequestId,
          sessionId: 'browser-session'
        }
      };

      client.send(JSON.stringify(message));
    });

    client.on('message', (data) => {
      try {
        const response = JSON.parse(data.toString());
        
        if (response.type === 'connection_confirmed') {
          return;
        }

        if (response.requestId === testRequestId) {
          responseReceived = true;
          
          // Validate browser client expectations are met
          expect(response.type).toBe('execute_command_response');
          expect(response.requestId).toBe(testRequestId);
          expect(response.command).toBe('console');
          expect(response.success).toBe(true);
          expect(response.timestamp).toBeDefined();
          
          // Validate console command specific response
          expect(response.data.forwarded).toBe(true);
          expect(response.data.consoleEntry).toBeDefined();
          expect(response.data.consoleEntry.action).toBe('health_report');
          
          client.close();
          done();
        }
      } catch (error) {
        done(error);
      }
    });

    client.on('error', (error) => {
      done(error);
    });

    setTimeout(() => {
      if (!responseReceived) {
        client.close();
        done(new Error('Compatibility test timeout'));
      }
    }, 5000);
  });
});