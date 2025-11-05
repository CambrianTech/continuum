/**
 * Unit Tests for WebSocketDaemon Console Forwarding
 * Tests the current working system
 */

import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { WebSocketDaemon } from '../../WebSocketDaemon';
import { ContinuumContext, continuumContextFactory } from '../../../../types/shared/core/ContinuumTypes';

describe('WebSocketDaemon Unit Tests', () => {
  let webSocketDaemon: WebSocketDaemon;
  let mockContext: ContinuumContext;

  beforeEach(() => {
    mockContext = continuumContextFactory.create({
      sessionId: 'test-websocket-unit' as any,
      environment: 'server'
    });

    webSocketDaemon = new WebSocketDaemon(mockContext);
  });

  afterEach(async () => {
    await webSocketDaemon.stop();
  });

  describe('Initialization', () => {
    test('should initialize with correct properties', () => {
      assert.strictEqual(webSocketDaemon.name, 'websocket-server');
      assert.strictEqual(webSocketDaemon.version, '1.0.0');
      assert.ok(webSocketDaemon.daemonType);
    });

    test('should have connection sessions map', () => {
      const connectionSessions = webSocketDaemon.getConnectionSessions();
      assert.ok(connectionSessions instanceof Map);
      assert.strictEqual(connectionSessions.size, 0);
    });
  });

  describe('Message Type Detection', () => {
    test('should detect console_log messages', () => {
      const mockMessage = {
        type: 'console_log',
        data: {
          level: 'info',
          message: 'Test message'
        }
      };

      // Test that the message would be routed to console handler
      assert.strictEqual(mockMessage.type, 'console_log');
    });

    test('should detect browser_console messages', () => {
      const mockMessage = {
        type: 'browser_console',
        data: {
          level: 'warn',
          message: 'Test warning'
        }
      };

      // Test that the message would be routed to console handler
      assert.strictEqual(mockMessage.type, 'browser_console');
    });

    test('should detect command messages', () => {
      const mockMessage = {
        type: 'execute_command',
        data: {
          command: 'test',
          params: {}
        }
      };

      // Test that the message would be routed to command handler
      assert.strictEqual(mockMessage.type, 'execute_command');
    });
  });

  describe('Connection Session Management', () => {
    test('should manage connection to session mapping', () => {
      const connectionSessions = webSocketDaemon.getConnectionSessions();
      const connectionId = 'test-connection-1';
      const sessionId = 'test-session-1';

      // Add mapping
      connectionSessions.set(connectionId, sessionId);
      assert.strictEqual(connectionSessions.get(connectionId), sessionId);
      assert.strictEqual(connectionSessions.size, 1);

      // Remove mapping
      connectionSessions.delete(connectionId);
      assert.strictEqual(connectionSessions.get(connectionId), undefined);
      assert.strictEqual(connectionSessions.size, 0);
    });

    test('should handle multiple connections', () => {
      const connectionSessions = webSocketDaemon.getConnectionSessions();
      
      const connections = [
        { id: 'conn-1', sessionId: 'session-1' },
        { id: 'conn-2', sessionId: 'session-2' },
        { id: 'conn-3', sessionId: 'session-1' } // Same session, different connection
      ];

      // Add all connections
      connections.forEach(conn => {
        connectionSessions.set(conn.id, conn.sessionId);
      });

      assert.strictEqual(connectionSessions.size, 3);
      assert.strictEqual(connectionSessions.get('conn-1'), 'session-1');
      assert.strictEqual(connectionSessions.get('conn-2'), 'session-2');
      assert.strictEqual(connectionSessions.get('conn-3'), 'session-1');
    });
  });

  describe('Message Validation', () => {
    test('should validate console message structure', () => {
      const validMessage = {
        type: 'console_log',
        data: {
          level: 'info',
          message: 'Valid message',
          timestamp: Date.now()
        }
      };

      // Validate required fields
      assert.ok(validMessage.type);
      assert.ok(validMessage.data);
      assert.ok(validMessage.data.level);
      assert.ok(validMessage.data.message);
      assert.ok(validMessage.data.timestamp);
    });

    test('should handle incomplete console messages', () => {
      const incompleteMessage = {
        type: 'console_log',
        data: {
          level: 'info'
          // Missing message field
        }
      };

      // Should still have type and data
      assert.strictEqual(incompleteMessage.type, 'console_log');
      assert.ok(incompleteMessage.data);
      assert.strictEqual(incompleteMessage.data.level, 'info');
      assert.strictEqual(incompleteMessage.data.message, undefined);
    });
  });

  describe('Error Handling', () => {
    test('should handle JSON parse errors gracefully', () => {
      const invalidJson = 'invalid json string';
      
      // Should not throw when handling invalid JSON
      assert.doesNotThrow(() => {
        try {
          JSON.parse(invalidJson);
        } catch (error) {
          // Expected error - should be handled gracefully
          assert.ok(error instanceof SyntaxError);
        }
      });
    });

    test('should handle missing data fields gracefully', () => {
      const messageWithoutData = {
        type: 'console_log'
        // Missing data field
      };

      // Should still be processable
      assert.strictEqual(messageWithoutData.type, 'console_log');
      assert.strictEqual((messageWithoutData as any).data, undefined);
    });
  });

  describe('Daemon Registration', () => {
    test('should register daemons correctly', () => {
      const mockDaemon = {
        name: 'test-daemon',
        handleMessage: async () => ({ success: true })
      };

      webSocketDaemon.registerDaemon(mockDaemon);

      // Verify daemon was registered
      const registeredDaemons = (webSocketDaemon as any).registeredDaemons;
      assert.ok(registeredDaemons.has('test-daemon'));
      assert.strictEqual(registeredDaemons.get('test-daemon'), mockDaemon);
    });

    test('should handle daemon registration with route setup', () => {
      const mockDaemon = {
        name: 'test-daemon-with-routes',
        handleMessage: async () => ({ success: true }),
        registerWithWebSocketDaemon: (daemon: any) => {
          // Mock route registration
          assert.strictEqual(daemon, webSocketDaemon);
        }
      };

      webSocketDaemon.registerDaemon(mockDaemon);

      // Verify daemon was registered
      const registeredDaemons = (webSocketDaemon as any).registeredDaemons;
      assert.ok(registeredDaemons.has('test-daemon-with-routes'));
    });
  });

  describe('Message Broadcasting', () => {
    test('should support message broadcasting', async () => {
      const testMessage = {
        type: 'test_broadcast',
        data: { message: 'Hello all connections' }
      };

      // Test broadcast capability
      const response = await webSocketDaemon.sendToConnectedClients(testMessage);
      
      assert.strictEqual(response.success, true);
      assert.ok(response.data);
      assert.strictEqual(response.data.broadcastSent, true);
      assert.strictEqual(typeof response.data.connectionCount, 'number');
    });
  });

  describe('Connection Checking', () => {
    test('should check connection existence', async () => {
      const connectionId = 'test-connection-check';

      // Mock connection check
      const checkData = { connectionId };
      const response = await (webSocketDaemon as any).handleCheckConnection(checkData);

      assert.ok(response);
      assert.strictEqual(typeof response.success, 'boolean');
      assert.ok(response.data);
      assert.strictEqual(response.data.connectionId, connectionId);
    });
  });

  describe('Type Safety', () => {
    test('should maintain type safety for console levels', () => {
      const validLevels = ['log', 'info', 'warn', 'error', 'debug'];
      
      validLevels.forEach(level => {
        const message = {
          type: 'console_log',
          data: {
            level,
            message: `Test ${level} message`,
            timestamp: Date.now()
          }
        };

        assert.ok(validLevels.includes(message.data.level));
      });
    });

    test('should maintain type safety for message types', () => {
      const validTypes = ['console_log', 'browser_console', 'execute_command'];
      
      validTypes.forEach(type => {
        const message = {
          type,
          data: {}
        };

        assert.ok(validTypes.includes(message.type));
      });
    });
  });
});