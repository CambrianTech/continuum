/**
 * Integration Tests for WebSocket Console Forwarding
 * Tests the current working system (not the LoggerDaemon)
 */

import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { WebSocketDaemon } from '../../WebSocketDaemon';
import { ContinuumContext, continuumContextFactory } from '../../../../types/shared/core/ContinuumTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

describe('WebSocket Console Forwarding Integration Tests', () => {
  let webSocketDaemon: WebSocketDaemon;
  let mockContext: ContinuumContext;
  let testSessionId: string;
  let testLogDir: string;

  beforeEach(async () => {
    testSessionId = `test-console-${Date.now()}`;
    mockContext = continuumContextFactory.create({
      sessionId: testSessionId as any,
      environment: 'server'
    });

    testLogDir = path.join(
      process.cwd(),
      '.continuum',
      'sessions',
      'user',
      'shared',
      testSessionId,
      'logs'
    );

    // Create test directory
    await fs.mkdir(testLogDir, { recursive: true });

    webSocketDaemon = new WebSocketDaemon(mockContext);
  });

  afterEach(async () => {
    await webSocketDaemon.stop();
    
    // Clean up test directory
    try {
      await fs.rm(path.dirname(testLogDir), { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  });

  describe('Console Message Handling', () => {
    test('should handle browser console messages', async () => {
      let messageHandled = false;
      
      // Mock the handleBrowserConsoleMessage method
      const originalHandle = (webSocketDaemon as any).handleBrowserConsoleMessage;
      (webSocketDaemon as any).handleBrowserConsoleMessage = async (connectionId: string, message: any) => {
        messageHandled = true;
        assert.strictEqual(message.type, 'console_log');
        assert.strictEqual(message.data.level, 'info');
        assert.strictEqual(message.data.message, 'Test console message');
        return originalHandle.call(webSocketDaemon, connectionId, message);
      };

      await webSocketDaemon.start();

      // Simulate browser console message
      const mockMessage = {
        type: 'console_log',
        data: {
          level: 'info',
          message: 'Test console message',
          timestamp: Date.now()
        }
      };

      // Call handleWebSocketMessage directly
      (webSocketDaemon as any).handleWebSocketMessage('test-connection', JSON.stringify(mockMessage));

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 10));

      assert.ok(messageHandled, 'Console message should have been handled');
    });

    test('should handle different console levels', async () => {
      const handledMessages: any[] = [];
      
      // Mock the handleBrowserConsoleMessage method
      const originalHandle = (webSocketDaemon as any).handleBrowserConsoleMessage;
      (webSocketDaemon as any).handleBrowserConsoleMessage = async (connectionId: string, message: any) => {
        handledMessages.push(message);
        return originalHandle.call(webSocketDaemon, connectionId, message);
      };

      await webSocketDaemon.start();

      // Test different console levels
      const levels = ['log', 'info', 'warn', 'error'];
      
      for (const level of levels) {
        const mockMessage = {
          type: 'console_log',
          data: {
            level,
            message: `Test ${level} message`,
            timestamp: Date.now()
          }
        };

        (webSocketDaemon as any).handleWebSocketMessage('test-connection', JSON.stringify(mockMessage));
      }

      // Wait for async processing
      await new Promise(resolve => setTimeout(resolve, 50));

      assert.strictEqual(handledMessages.length, levels.length);
      
      for (let i = 0; i < levels.length; i++) {
        assert.strictEqual(handledMessages[i].data.level, levels[i]);
        assert.ok(handledMessages[i].data.message.includes(levels[i]));
      }
    });
  });

  describe('Session Context Management', () => {
    test('should map connections to sessions', async () => {
      await webSocketDaemon.start();

      // Set up connection mapping
      const connectionSessions = webSocketDaemon.getConnectionSessions();
      connectionSessions.set('test-connection', testSessionId);

      // Verify mapping
      assert.strictEqual(connectionSessions.get('test-connection'), testSessionId);
    });

    test('should handle connection closure cleanup', async () => {
      await webSocketDaemon.start();

      // Set up connection mapping
      const connectionSessions = webSocketDaemon.getConnectionSessions();
      connectionSessions.set('test-connection', testSessionId);

      // Simulate connection closure
      await (webSocketDaemon as any).handleConnectionClosed('test-connection');

      // Verify cleanup
      assert.strictEqual(connectionSessions.get('test-connection'), undefined);
    });
  });

  describe('Error Handling', () => {
    test('should handle malformed console messages gracefully', async () => {
      let errorHandled = false;
      
      // Mock error handling
      const originalLog = (webSocketDaemon as any).log;
      (webSocketDaemon as any).log = (message: string, level?: string) => {
        if (level === 'error' && message.includes('parse error')) {
          errorHandled = true;
        }
        return originalLog.call(webSocketDaemon, message, level);
      };

      await webSocketDaemon.start();

      // Send malformed message
      (webSocketDaemon as any).handleWebSocketMessage('test-connection', 'invalid json');

      // Wait for error handling
      await new Promise(resolve => setTimeout(resolve, 10));

      assert.ok(errorHandled, 'Error should have been handled gracefully');
    });

    test('should handle console messages without session mapping', async () => {
      let warningLogged = false;
      
      // Mock warning logging
      const originalLog = (webSocketDaemon as any).log;
      (webSocketDaemon as any).log = (message: string, level?: string) => {
        if (level === 'warn' && message.includes('No session found')) {
          warningLogged = true;
        }
        return originalLog.call(webSocketDaemon, message, level);
      };

      await webSocketDaemon.start();

      // Send console message without session mapping
      const mockMessage = {
        type: 'console_log',
        data: {
          level: 'info',
          message: 'Test without session',
          timestamp: Date.now()
        }
      };

      (webSocketDaemon as any).handleWebSocketMessage('unmapped-connection', JSON.stringify(mockMessage));

      // Wait for warning
      await new Promise(resolve => setTimeout(resolve, 10));

      assert.ok(warningLogged, 'Warning should have been logged for unmapped connection');
    });
  });

  describe('Performance', () => {
    test('should handle multiple console messages efficiently', async () => {
      const handledMessages: any[] = [];
      
      // Mock the handleBrowserConsoleMessage method
      const originalHandle = (webSocketDaemon as any).handleBrowserConsoleMessage;
      (webSocketDaemon as any).handleBrowserConsoleMessage = async (connectionId: string, message: any) => {
        handledMessages.push(message);
        return originalHandle.call(webSocketDaemon, connectionId, message);
      };

      await webSocketDaemon.start();

      const messageCount = 50;
      const startTime = Date.now();

      // Send multiple console messages
      for (let i = 0; i < messageCount; i++) {
        const mockMessage = {
          type: 'console_log',
          data: {
            level: 'info',
            message: `Performance test message ${i}`,
            timestamp: Date.now()
          }
        };

        (webSocketDaemon as any).handleWebSocketMessage('test-connection', JSON.stringify(mockMessage));
      }

      // Wait for all messages to be processed
      await new Promise(resolve => setTimeout(resolve, 100));

      const endTime = Date.now();
      const processingTime = endTime - startTime;

      // Verify all messages were handled
      assert.strictEqual(handledMessages.length, messageCount);
      
      // Performance check - should handle 50 messages quickly
      assert.ok(processingTime < 1000, `Processing took ${processingTime}ms, expected < 1000ms`);
    });
  });

  describe('Message Routing', () => {
    test('should route console messages correctly', async () => {
      let routedCorrectly = false;
      
      await webSocketDaemon.start();

      // Mock message routing
      const originalHandle = (webSocketDaemon as any).handleWebSocketMessage;
      (webSocketDaemon as any).handleWebSocketMessage = (connectionId: string, data: any) => {
        try {
          const message = JSON.parse(data);
          if (message.type === 'console_log' || message.type === 'browser_console') {
            routedCorrectly = true;
          }
        } catch (error) {
          // Ignore parse errors for this test
        }
        return originalHandle.call(webSocketDaemon, connectionId, data);
      };

      // Send console message
      const mockMessage = {
        type: 'console_log',
        data: {
          level: 'info',
          message: 'Routing test message',
          timestamp: Date.now()
        }
      };

      (webSocketDaemon as any).handleWebSocketMessage('test-connection', JSON.stringify(mockMessage));

      // Wait for routing
      await new Promise(resolve => setTimeout(resolve, 10));

      assert.ok(routedCorrectly, 'Console message should have been routed correctly');
    });
  });

  describe('Integration with Session Manager', () => {
    test('should interact with session manager for log paths', async () => {
      let sessionManagerCalled = false;
      
      // Mock session manager
      const mockSessionManager = {
        handleMessage: async (message: any) => {
          sessionManagerCalled = true;
          return {
            success: true,
            data: {
              artifacts: {
                logs: {
                  client: ['/test/path/browser.log']
                }
              }
            }
          };
        }
      };

      await webSocketDaemon.start();

      // Register mock session manager
      (webSocketDaemon as any).registeredDaemons.set('session-manager', mockSessionManager);

      // Set up connection mapping
      const connectionSessions = webSocketDaemon.getConnectionSessions();
      connectionSessions.set('test-connection', testSessionId);

      // Send console message
      const mockMessage = {
        type: 'console_log',
        data: {
          level: 'info',
          message: 'Session manager test',
          timestamp: Date.now()
        }
      };

      (webSocketDaemon as any).handleWebSocketMessage('test-connection', JSON.stringify(mockMessage));

      // Wait for session manager interaction
      await new Promise(resolve => setTimeout(resolve, 50));

      assert.ok(sessionManagerCalled, 'Session manager should have been called for log path');
    });
  });
});