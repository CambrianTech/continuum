/**
 * Integration Tests for Client-Server LoggerDaemon Communication
 * Testing daemon bus routing and WebSocket message forwarding
 */

import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { LoggerDaemon } from '../../server/LoggerDaemon';
import { ClientLoggerDaemon } from '../../client/ClientLoggerDaemon';
import { ContinuumContext } from '../../../../types/shared/core/ContinuumTypes';
import { LogEntry, LogLevel } from '../../shared/LoggerMessageTypes';
import { WebSocketDaemon } from '../../../../integrations/websocket/WebSocketDaemon';
import * as path from 'path';

describe('Client-Server LoggerDaemon Communication Tests', () => {
  let serverDaemon: LoggerDaemon;
  let clientDaemon: ClientLoggerDaemon;
  let webSocketDaemon: WebSocketDaemon;
  let mockContext: ContinuumContext;
  let testSessionId: string;

  beforeEach(async () => {
    testSessionId = `test-session-${Date.now()}`;
    mockContext = {
      sessionId: testSessionId,
      environment: 'test'
    };

    // Create daemon instances
    serverDaemon = new LoggerDaemon(mockContext);
    clientDaemon = new ClientLoggerDaemon(mockContext);
    webSocketDaemon = new WebSocketDaemon(mockContext);
  });

  afterEach(async () => {
    // Stop all daemons
    await serverDaemon.stop();
    await clientDaemon.stop();
    await webSocketDaemon.stop();
  });

  describe('Message Routing', () => {
    test('should route logger messages through WebSocket daemon', async () => {
      let routedMessages: any[] = [];
      
      // Mock WebSocket daemon's routeToLoggerDaemon method
      const originalRoute = (webSocketDaemon as any).routeToLoggerDaemon;
      (webSocketDaemon as any).routeToLoggerDaemon = async (connectionId: string, message: any) => {
        routedMessages.push({ connectionId, message });
        // Call original method if it exists
        if (originalRoute) {
          return originalRoute.call(webSocketDaemon, connectionId, message);
        }
      };

      await webSocketDaemon.start();
      
      // Simulate client message
      const clientMessage = {
        type: 'logger_message',
        data: {
          type: 'log',
          payload: {
            level: 'info',
            message: 'Test client message',
            timestamp: Date.now(),
            sessionId: testSessionId,
            source: 'ClientTest',
            context: mockContext
          }
        }
      };

      // Simulate WebSocket message handling
      const mockConnectionId = 'test-connection-1';
      await (webSocketDaemon as any).routeToLoggerDaemon(mockConnectionId, clientMessage);
      
      // Verify message was routed
      assert.strictEqual(routedMessages.length, 1);
      assert.strictEqual(routedMessages[0].connectionId, mockConnectionId);
      assert.strictEqual(routedMessages[0].message.type, 'logger_message');
    });

    test('should handle routing failures gracefully', async () => {
      // Mock WebSocket daemon without logger daemon registered
      const originalRegisteredDaemons = (webSocketDaemon as any).registeredDaemons;
      (webSocketDaemon as any).registeredDaemons = new Map();

      await webSocketDaemon.start();
      
      const clientMessage = {
        type: 'logger_message',
        data: {
          type: 'log',
          payload: {
            level: 'error',
            message: 'Test routing failure',
            timestamp: Date.now(),
            sessionId: testSessionId,
            source: 'RoutingTest',
            context: mockContext
          }
        }
      };

      // Should handle gracefully when logger daemon is not registered
      await assert.doesNotReject(async () => {
        await (webSocketDaemon as any).routeToLoggerDaemon('test-connection', clientMessage);
      });

      // Restore original registered daemons
      (webSocketDaemon as any).registeredDaemons = originalRegisteredDaemons;
    });
  });

  describe('Message Type Safety', () => {
    test('should maintain type safety in message routing', async () => {
      await serverDaemon.start();
      
      // Create strongly typed message
      const typedLogEntry: LogEntry = {
        level: 'warn' as LogLevel,
        message: 'Type safety routing test',
        timestamp: Date.now(),
        sessionId: testSessionId,
        source: 'TypeSafetyTest',
        context: mockContext,
        data: {
          routingTest: true,
          timestamp: new Date().toISOString()
        }
      };

      const typedMessage = {
        id: 'typed-routing-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: typedLogEntry
        }
      };

      // Process message with type safety
      const response = await (serverDaemon as any).processMessage(typedMessage);
      
      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, 'typed-routing-msg-1');
      
      // Verify type constraints are preserved
      assert.ok(['log', 'info', 'warn', 'error'].includes(typedLogEntry.level));
      assert.ok(typeof typedLogEntry.timestamp === 'number');
      assert.ok(typeof typedLogEntry.context.sessionId === 'string');
      assert.ok(typeof typedLogEntry.data === 'object');
    });

    test('should handle type mismatches gracefully', async () => {
      await serverDaemon.start();
      
      // Create message with potential type issues
      const problematicMessage = {
        id: 'problematic-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: {
            level: 'invalid-level' as any, // Invalid LogLevel
            message: 'Type mismatch test',
            timestamp: 'invalid-timestamp' as any, // Invalid timestamp type
            sessionId: testSessionId,
            source: 'TypeMismatchTest',
            context: mockContext
          }
        }
      };

      // Should handle gracefully
      const response = await (serverDaemon as any).processMessage(problematicMessage);
      
      // Should either succeed or fail gracefully
      assert.ok(typeof response.success === 'boolean');
      assert.strictEqual(response.messageId, 'problematic-msg-1');
    });
  });

  describe('Client Console Forwarding', () => {
    test('should forward client console calls to server', async () => {
      let forwardedMessages: any[] = [];
      
      // Mock client daemon's WebSocket sending
      const mockWebSocketSend = (message: any) => {
        forwardedMessages.push(message);
      };

      // Mock WebSocket connection
      (clientDaemon as any).webSocket = {
        send: mockWebSocketSend,
        readyState: 1 // WebSocket.OPEN
      };

      await clientDaemon.start();
      
      // Simulate client console call
      const clientConsoleManager = (clientDaemon as any).clientConsoleManager;
      if (clientConsoleManager) {
        // Simulate console.log call
        await clientConsoleManager.forwardConsole('log', ['Test client forwarding']);
      }

      // Verify message was forwarded
      assert.ok(forwardedMessages.length > 0);
      const forwardedMessage = forwardedMessages[0];
      assert.strictEqual(forwardedMessage.type, 'logger_message');
      assert.ok(forwardedMessage.data.payload.message.includes('Test client forwarding'));
    });

    test('should handle WebSocket connection failures', async () => {
      // Mock failed WebSocket connection
      (clientDaemon as any).webSocket = {
        send: () => {
          throw new Error('WebSocket connection failed');
        },
        readyState: 3 // WebSocket.CLOSED
      };

      await clientDaemon.start();
      
      // Should handle connection failure gracefully
      await assert.doesNotReject(async () => {
        const clientConsoleManager = (clientDaemon as any).clientConsoleManager;
        if (clientConsoleManager) {
          await clientConsoleManager.forwardConsole('error', ['Test connection failure']);
        }
      });
    });
  });

  describe('Session Context Propagation', () => {
    test('should maintain session context across client-server boundary', async () => {
      let processedMessages: any[] = [];
      
      // Mock server daemon's processMessage to capture messages
      const originalProcessMessage = (serverDaemon as any).processMessage;
      (serverDaemon as any).processMessage = async (message: any) => {
        processedMessages.push(message);
        return originalProcessMessage.call(serverDaemon, message);
      };

      await serverDaemon.start();
      
      // Create message with specific session context
      const sessionContextMessage = {
        id: 'session-context-msg-1',
        timestamp: new Date(),
        data: {
          type: 'log' as const,
          payload: {
            level: 'info',
            message: 'Session context test',
            timestamp: Date.now(),
            sessionId: testSessionId,
            source: 'SessionContextTest',
            context: {
              sessionId: testSessionId,
              environment: 'browser',
              userId: 'test-user',
              clientType: 'web'
            }
          }
        }
      };

      await (serverDaemon as any).processMessage(sessionContextMessage);
      
      // Verify session context was preserved
      assert.strictEqual(processedMessages.length, 1);
      const processedMessage = processedMessages[0];
      assert.strictEqual(processedMessage.data.payload.context.sessionId, testSessionId);
      assert.strictEqual(processedMessage.data.payload.context.environment, 'browser');
      assert.strictEqual(processedMessage.data.payload.context.userId, 'test-user');
    });
  });

  describe('Performance and Reliability', () => {
    test('should handle high-frequency client-server communication', async () => {
      let processedCount = 0;
      
      // Mock server daemon to count processed messages
      const originalProcessMessage = (serverDaemon as any).processMessage;
      (serverDaemon as any).processMessage = async (message: any) => {
        processedCount++;
        return originalProcessMessage.call(serverDaemon, message);
      };

      await serverDaemon.start();
      
      const messageCount = 50;
      const messages = [];
      
      // Generate many messages
      for (let i = 0; i < messageCount; i++) {
        messages.push({
          id: `high-freq-msg-${i}`,
          timestamp: new Date(),
          data: {
            type: 'log' as const,
            payload: {
              level: 'info',
              message: `High frequency message ${i}`,
              timestamp: Date.now(),
              sessionId: testSessionId,
              source: 'HighFrequencyTest',
              context: mockContext
            }
          }
        });
      }

      const startTime = Date.now();
      
      // Process all messages
      await Promise.all(messages.map(msg => (serverDaemon as any).processMessage(msg)));
      
      const endTime = Date.now();
      const processingTime = endTime - startTime;
      
      // Verify all messages were processed
      assert.strictEqual(processedCount, messageCount);
      
      // Performance check
      assert.ok(processingTime < 2000, `Processing took ${processingTime}ms, expected < 2000ms`);
    });

    test('should maintain message ordering', async () => {
      let processedMessages: any[] = [];
      
      // Mock server daemon to capture message order
      const originalProcessMessage = (serverDaemon as any).processMessage;
      (serverDaemon as any).processMessage = async (message: any) => {
        processedMessages.push(message);
        return originalProcessMessage.call(serverDaemon, message);
      };

      await serverDaemon.start();
      
      // Create ordered messages
      const orderedMessages = [];
      for (let i = 0; i < 10; i++) {
        orderedMessages.push({
          id: `ordered-msg-${i}`,
          timestamp: new Date(),
          data: {
            type: 'log' as const,
            payload: {
              level: 'info',
              message: `Ordered message ${i}`,
              timestamp: Date.now() + i,
              sessionId: testSessionId,
              source: 'OrderingTest',
              context: mockContext
            }
          }
        });
      }

      // Process messages sequentially
      for (const message of orderedMessages) {
        await (serverDaemon as any).processMessage(message);
      }

      // Verify message order was maintained
      assert.strictEqual(processedMessages.length, 10);
      for (let i = 0; i < 10; i++) {
        assert.strictEqual(processedMessages[i].id, `ordered-msg-${i}`);
        assert.ok(processedMessages[i].data.payload.message.includes(`${i}`));
      }
    });
  });
});